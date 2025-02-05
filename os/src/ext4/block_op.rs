//! 用于处理EXT4文件系统的块操作, 如读取目录项, 操作位图等
use core::ptr;

use alloc::{string::String, vec::Vec};

use crate::{
    drivers::BLOCK_DEVICE,
    ext4::dentry::{self, Ext4DirEntry},
    fs::inode_trait,
};

use super::fs::EXT4_BLOCK_SIZE;

/// EXT4中文件名最大长度
pub const EXT4_NAME_LEN: usize = 255;

/*
 * 默认情况下，每个目录都以“几乎是线性”数组列出条目。我写“几乎”，因为它不是内存意义上的线性阵列，因为目录条目是跨文件系统块分开。
 * 因此，说目录是一系列数据块，并且每个块包含目录条目的线性阵列。每个块阵列的末端通过到达块的末端来表示；该块中的最后一个条目具有记录长度，将其一直延伸到块的末端。
 * 当然，整个目录的末尾可以通过到达文件的末尾来表示。未使用的目录条目由Inode = 0。
 */
#[repr(C)]
pub struct Ext4DirContentRO<'a> {
    content: &'a [u8],
}

#[repr(C)]
pub struct Ext4DirContentWE<'a> {
    content: &'a mut [u8],
}

// 用于解析目录项
impl<'a> Ext4DirContentRO<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { content: data }
    }
    // 遍历目录项
    // 在文件系统的一个块中, 目录项是连续存储的, 每个目录项的长度不一定相同, 根据目录项的rec_len字段来判断, 到达ext4块尾部即为结束
    // 这个由调用者保证, 传入的buf是目录所有内容
    pub fn list(&self) -> Vec<Ext4DirEntry> {
        let mut entries = Vec::new();
        let mut rec_len_total = 0;
        let content_len = self.content.len();
        log::info!("[Ext4DirContent::list] content_len: {}", content_len);
        while rec_len_total < content_len {
            // rec_len是u16, 2字节
            let rec_len = u16::from_le_bytes([
                self.content[rec_len_total + 4],
                self.content[rec_len_total + 5],
            ]);
            let dentry = Ext4DirEntry::try_from(
                &self.content[rec_len_total..rec_len_total + rec_len as usize],
            )
            .expect("DirEntry::try_from failed");
            log::info!("dentry: {:?}", dentry);
            entries.push(dentry);
            rec_len_total += rec_len as usize;
        }
        entries
    }
    pub fn find(&self, name: &str) -> Option<Ext4DirEntry> {
        let mut rec_len_total = 0;
        let content_len = self.content.len();
        while rec_len_total < content_len {
            let rec_len = u16::from_le_bytes([
                self.content[rec_len_total + 4],
                self.content[rec_len_total + 5],
            ]);
            let dentry = Ext4DirEntry::try_from(
                &self.content[rec_len_total..rec_len_total + rec_len as usize],
            )
            .expect("DirEntry::try_from failed");
            let dentry_name = String::from_utf8(dentry.name[..].to_vec()).unwrap();
            if dentry_name == name {
                return Some(dentry);
            }
            rec_len_total += rec_len as usize;
        }
        None
    }
}

impl<'a> Ext4DirContentWE<'a> {
    pub fn new(data: &'a mut [u8]) -> Self {
        Self { content: data }
    }
    /// 注意目录的size应该是对齐到块大小的, 一个数据块中目录项是根据rec_len找到下一个的, 并且该数据块中目录项的结束是到数据块的末尾(就可能导致最后一个rec_len很大)
    /// 由上层调用者保证name在目录中不存在
    /// Todo: 当块内空间不足时, 需要分配新的块, 并将新的目录项写入新的块(需要extents_tree管理)
    /// ToOptimize: 目前这个函数进行了很多不必要的拷贝, 需要优化
    pub fn add_entry(
        &mut self,
        name: &str,
        inode_num: u32,
        file_type: u8,
    ) -> Result<(), &'static str> {
        // 新的目录项长度为name长度加上8字节
        let new_entry_name_len = name.len() as u16;
        let needed_len = new_entry_name_len + 8;
        let mut rec_len_total = 0;

        let content_len = self.content.len();
        assert!(content_len > 0 && content_len % EXT4_BLOCK_SIZE == 0);

        let mut dentry: Ext4DirEntry = Ext4DirEntry::default();
        let mut rec_len = 0;
        while rec_len_total < content_len {
            rec_len = u16::from_le_bytes([
                self.content[rec_len_total + 4],
                self.content[rec_len_total + 5],
            ]);
            dentry = Ext4DirEntry::try_from(
                &self.content[rec_len_total..rec_len_total + rec_len as usize],
            )
            .expect("DirEntry::try_from failed");
            // 找到空闲位置(inode_num为0, 表示已被删除)
            if dentry.inode_num == 0 && rec_len > needed_len {
                // 更新name_len, inode_num, file_type
                let mut new_dentry = dentry;
                new_dentry.name_len = new_entry_name_len as u8;
                new_dentry.inode_num = inode_num;
                new_dentry.file_type = file_type;
                // 写回
                new_dentry.write_to_mem(
                    &mut self.content[rec_len_total..rec_len_total + needed_len as usize],
                );
                return Ok(());
            }
            rec_len_total += rec_len as usize;
        }
        // 没有找到unused的目录项, 则看是否最后一个目录项的rec_len可以容纳新的目录项
        // 此时rec_len是最后一个目录项的rec_len, dentry是最后一个目录项
        dentry.rec_len = dentry.name_len as u16 + 8;
        let surplus_len = rec_len - dentry.rec_len;
        assert!(surplus_len >= needed_len);
        dentry.write_to_mem(
            &mut self.content[content_len - rec_len as usize
                ..content_len - rec_len as usize + dentry.rec_len as usize],
        );
        let new_dentry = Ext4DirEntry {
            inode_num,
            rec_len: surplus_len,
            name_len: new_entry_name_len as u8,
            file_type,
            name: name.as_bytes().to_vec(),
        };
        new_dentry.write_to_mem(&mut self.content[content_len - surplus_len as usize..content_len]);
        Ok(())
    }
}

pub struct Ext4Bitmap<'a> {
    bitmap: &'a mut [u8; EXT4_BLOCK_SIZE],
}

impl<'a> Ext4Bitmap<'a> {
    pub fn new(bitmap: &'a mut [u8; EXT4_BLOCK_SIZE]) -> Self {
        Self { bitmap }
    }
    // 分配一个位
    pub fn alloc(&mut self) -> Option<usize> {
        // 逐字节处理, 加速alloc过程
        for (i, byte) in self.bitmap.iter_mut().enumerate() {
            if *byte != 0xff {
                for j in 0..8 {
                    if (*byte & (1 << j)) == 0 {
                        *byte |= 1 << j;
                        return Some(i * 8 + j);
                    }
                }
            }
        }
        None
    }
}
