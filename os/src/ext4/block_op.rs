//! 用于处理EXT4文件系统的块操作, 如读取目录项, 操作位图等
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use universal_hash::typenum::bit;

use crate::drivers::block;
use crate::drivers::block::block_cache::get_block_cache;
use crate::drivers::block::block_dev::BlockDevice;
use crate::ext4::dentry::Ext4DirEntry;
use crate::fs::dentry::LinuxDirent64;
use crate::syscall::errno::Errno;

use super::extent_tree::{Ext4Extent, Ext4ExtentHeader, Ext4ExtentIdx};
use super::{dentry::EXT4_DT_DIR, fs::EXT4_BLOCK_SIZE};

use crate::arch::config::PAGE_SIZE;
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
    // 注意: 目录项可能被截断
    // buf在内核空间
    // 返回: (file_offset, buf_offset)文件偏移的增加量和读取的字节数
    pub fn getdents(&self, buf: &mut [u8]) -> Result<(usize, usize), Errno> {
        const NAME_OFFSET: usize = 19;
        let mut buf_offset = 0;
        let mut file_offset = 0;
        let buf_len = buf.len();
        let content_len = self.content.len();
        while file_offset + 5 < content_len {
            // rec_len是u16, 2字节
            let rec_len =
                u16::from_le_bytes([self.content[file_offset + 4], self.content[file_offset + 5]]);
            if rec_len == 0 || file_offset + rec_len as usize > content_len {
                break;
            }
            let dentry =
                Ext4DirEntry::try_from(&self.content[file_offset..file_offset + rec_len as usize])
                    .expect("DirEntry::try_from failed");
            file_offset += rec_len as usize;
            // 将Ext4DirEntry转换为LinuxDirent64
            if dentry.inode_num == 0 {
                continue;
            }
            let null_term_name_len = dentry.name.len() + 1;
            // LinuxDirent64的reclen需要对齐到8字节
            let d_reclen = (NAME_OFFSET + null_term_name_len + 7) & !0x7;
            let dirent = LinuxDirent64 {
                d_ino: dentry.inode_num as u64,
                d_off: file_offset as u64,
                d_reclen: d_reclen as u16,
                d_type: dentry.file_type,
                d_name: dentry.name.clone(),
            };
            if buf_offset + d_reclen as usize > buf_len {
                break;
            }
            dirent.write_to_mem(&mut buf[buf_offset..buf_offset + d_reclen]);
            buf_offset += d_reclen as usize;
        }
        Ok((file_offset, buf_offset))
    }
    pub fn find(&self, name: &str) -> Option<Ext4DirEntry> {
        let mut rec_len_total = 0;
        let content_len = self.content.len();
        while rec_len_total < content_len {
            let rec_len = u16::from_le_bytes([
                self.content[rec_len_total + 4],
                self.content[rec_len_total + 5],
            ]);
            // 需要判断下rec_len是否超出范围
            if rec_len_total + rec_len as usize > content_len {
                break;
            }
            let dentry = Ext4DirEntry::try_from(
                &self.content[rec_len_total..rec_len_total + rec_len as usize],
            )
            .unwrap();
            let dentry_name = String::from_utf8_lossy(&dentry.name[..dentry.name_len as usize]);
            if dentry_name == name {
                return Some(dentry);
            }
            rec_len_total += rec_len as usize;
        }
        None
    }
}

// ToOptimize: 目前这个函数进行了很多不必要的拷贝, 需要优化
impl<'a> Ext4DirContentWE<'a> {
    pub fn new(data: &'a mut [u8]) -> Self {
        Self { content: data }
    }
    /// 注意目录的size应该是对齐到块大小的, 一个数据块中目录项是根据rec_len找到下一个的, 并且该数据块中目录项的结束是到数据块的末尾(就可能导致最后一个rec_len很大)
    /// 由上层调用者保证name在目录中不存在
    /// Todo: 当块内空间不足时, 需要分配新的块, 并将新的目录项写入新的块(需要extents_tree管理)
    /// ToOptimize: 目前这个函数进行了很多不必要的拷贝, 需要优化
    /// 注意目录项的rec_len要保证对齐到4字节
    ///
    pub fn add_entry(
        &mut self,
        name: &str,
        inode_num: u32,
        file_type: u8,
    ) -> Result<(), &'static str> {
        let name_len = name.len();
        // if name_len > 255 {
        //     return Err("Name too long");
        // }
        // 计算新目录项所需空间（4字节对齐）
        let needed_len = ((name_len + 8 + 3) & !3) as u16;
        let mut offset = 0;
        let content_len = self.content.len();

        // 遍历目录块
        while offset < content_len {
            // 检查 rec_len 是否有效
            if offset + 8 > content_len {
                return Err("Invalid directory entry");
            }
            let rec_len = u16::from_le_bytes([self.content[offset + 4], self.content[offset + 5]]);
            if rec_len < 8 || offset + rec_len as usize > content_len {
                return Err("Invalid rec_len");
            }

            let dentry =
                match Ext4DirEntry::try_from(&self.content[offset..offset + rec_len as usize]) {
                    Ok(d) => d,
                    Err(_) => return Err("Corrupted directory entry"),
                };

            // 情况1: 空闲目录项（inode_num == 0）
            if dentry.inode_num == 0 {
                if rec_len >= needed_len {
                    // 直接复用空闲项
                    let new_dentry = Ext4DirEntry {
                        inode_num,
                        rec_len,
                        name_len: name_len as u8,
                        file_type,
                        name: name.as_bytes().to_vec(),
                    };
                    new_dentry.write_to_mem(&mut self.content[offset..]);
                    return Ok(());
                }
            }
            // 情况2: 拆分目录项
            else {
                let current_len = ((dentry.name_len as usize + 8 + 3) & !3) as u16;
                if rec_len >= current_len + needed_len {
                    // 更新当前目录项的 rec_len
                    let mut updated_dentry = dentry;
                    updated_dentry.rec_len = current_len;
                    updated_dentry.write_to_mem(&mut self.content[offset..]);

                    // 写入新目录项
                    let new_dentry = Ext4DirEntry {
                        inode_num,
                        rec_len: rec_len - current_len,
                        name_len: name_len as u8,
                        file_type,
                        name: name.as_bytes().to_vec(),
                    };
                    new_dentry.write_to_mem(&mut self.content[offset + current_len as usize..]);
                    return Ok(());
                }
            }

            offset += rec_len as usize;
        }

        // 情况3: 没有足够空间
        Err("No space left in directory block")
    }
    // pub fn add_entry(
    //     &mut self,
    //     name: &str,
    //     inode_num: u32,
    //     file_type: u8,
    // ) -> Result<(), &'static str> {
    //     // 新的目录项长度为name长度加上8字节
    //     let new_entry_name_len = name.len() as u16;
    //     // Ext4Dirent rec_len对齐到4字节
    //     let needed_len = (new_entry_name_len + 8 + 3) & !(3 as u16);
    //     let mut rec_len_total = 0;

    //     let content_len = self.content.len();
    //     assert!(content_len > 0 && content_len % EXT4_BLOCK_SIZE == 0);
    //     log::info!(
    //         "[Ext4DirContentWE::add_entry] content_len: {}, needed_len: {}",
    //         content_len,
    //         needed_len
    //     );

    //     let mut dentry: Ext4DirEntry = Ext4DirEntry::default();
    //     let mut rec_len = 0;
    //     while rec_len_total < content_len {
    //         rec_len = u16::from_le_bytes([
    //             self.content[rec_len_total + 4],
    //             self.content[rec_len_total + 5],
    //         ]);
    //         // if rec_len + rec_len_total as u16 > content_len as u16 {
    //         //     log::warn!(
    //         //         "[Ext4DirContentWE::add_entry] rec_len_total: {}, rec_len: {}, content_len: {}",
    //         //         rec_len_total,
    //         //         rec_len,
    //         //         content_len
    //         //     );
    //         //     return Err("Invalid rec_len");
    //         // }
    //         dentry = Ext4DirEntry::try_from(
    //             &self.content[rec_len_total..rec_len_total + rec_len as usize],
    //         )
    //         .expect("DirEntry::try_from failed");

    //         // 情况1: 找到空闲位置, 已删除的目录项且有足够空间(inode_num为0, 表示已被删除)
    //         if dentry.inode_num == 0 && rec_len > needed_len {
    //             log::info!("Using empty dir entry at offset {}", rec_len_total);
    //             // 更新name_len, inode_num, file_type
    //             let mut new_dentry = dentry;
    //             new_dentry.name_len = new_entry_name_len as u8;
    //             new_dentry.inode_num = inode_num;
    //             new_dentry.file_type = file_type;
    //             // 写回
    //             new_dentry.write_to_mem(
    //                 &mut self.content[rec_len_total..rec_len_total + needed_len as usize],
    //             );
    //             return Ok(());
    //         }
    //         // 情况2: 目录项仍在使用, 但rec_len够大
    //         // 检查当前记录是否有足够的空间容纳新的目录项
    //         let current_dentry_len = ((dentry.name_len as usize + 8 + 3) & !3) as u16;
    //         let surplus_len = rec_len - current_dentry_len;
    //         if surplus_len > needed_len {
    //             // 有足够的空间容纳新的目录项
    //             log::info!(
    //                 "Splitting dir entry at offset {}, rec_len: {}, current_dentry_len: {}, surplus_len: {}, needed_len: {}",
    //                 rec_len_total,
    //                 rec_len,
    //                 current_dentry_len,
    //                 surplus_len,
    //                 needed_len,
    //             );
    //             // 修改原有目录项的rec_len
    //             let mut updated_dentry = dentry;
    //             updated_dentry.rec_len = current_dentry_len;
    //             updated_dentry.write_to_mem(
    //                 &mut self.content[rec_len_total..rec_len_total + current_dentry_len as usize],
    //             );
    //             // 在后续空间写入新的目录项
    //             let new_dentry = Ext4DirEntry {
    //                 inode_num,
    //                 rec_len: surplus_len,
    //                 name_len: new_entry_name_len as u8,
    //                 file_type,
    //                 name: name.as_bytes().to_vec(),
    //             };
    //             new_dentry
    //                 .write_to_mem(&mut self.content[rec_len_total + current_dentry_len as usize..]);
    //             log::info!(
    //                 "[Ext4DirContentWE::add_entry] new_dentry: offset: {}, rec_len: {}",
    //                 rec_len_total + current_dentry_len as usize,
    //                 rec_len
    //             );
    //             return Ok(());
    //         }
    //         rec_len_total += rec_len as usize;
    //     }
    //     // 没有找到unused的目录项, 则看是否最后一个目录项的rec_len可以容纳新的目录项
    //     // 此时rec_len是最后一个目录项的rec_len, dentry是最后一个目录项
    //     let dentry_len = dentry.name_len as u16 + 8;
    //     if rec_len < dentry_len + needed_len {
    //         log::warn!("No enough space for new entry",);
    //         return Err("No enough space for new entry");
    //     }
    //     dentry.rec_len = dentry_len;
    //     let surplus_len = rec_len - dentry.rec_len;
    //     dentry.write_to_mem(
    //         &mut self.content[content_len - rec_len as usize
    //             ..content_len - rec_len as usize + dentry.rec_len as usize],
    //     );
    //     let new_dentry = Ext4DirEntry {
    //         inode_num,
    //         rec_len: surplus_len,
    //         name_len: new_entry_name_len as u8,
    //         file_type,
    //         name: name.as_bytes().to_vec(),
    //     };
    //     new_dentry.write_to_mem(&mut self.content[content_len - surplus_len as usize..content_len]);
    //     Ok(())
    // }
    /// 基于合并相邻目录项的方式
    ///     1. 如果删除的dentry前面有目录项, 则将`rec_len`合并到前一个目录项
    ///     2. 如果删除的dentry是块中的第一个, 则仅见`inode`设为0
    pub fn delete_entry(&mut self, name: &str, inode_num: u32) -> Result<(), Errno> {
        let mut rec_len_total = 0;
        let mut prev_len_total = 0;
        let content_len = self.content.len();
        while rec_len_total < content_len {
            let rec_len = u16::from_le_bytes([
                self.content[rec_len_total + 4],
                self.content[rec_len_total + 5],
            ]);
            let mut dentry = Ext4DirEntry::try_from(
                &self.content[rec_len_total..rec_len_total + rec_len as usize],
            )
            .expect("DirEntry::try_from failed");
            // log::error!(
            //     "[Ext4DirContentWE::delete_entry] check dentry at offset {}: {:?}",
            //     rec_len_total,
            //     dentry
            // );
            let dentry_name = String::from_utf8_lossy(&dentry.name[..dentry.name_len as usize]);
            if dentry_name == name {
                debug_assert!(
                    dentry.inode_num == inode_num,
                    "[Ext4DirContentWE::delete_entry] name match, but inode_num mismatch: expected {}, found {}",
                    inode_num,
                    dentry.inode_num
                );
                // 删除目录项
                if rec_len_total == 0 {
                    // 删除的是块中的第一个目录项
                    dentry.inode_num = 0;
                    dentry.write_to_mem(
                        &mut self.content[rec_len_total..rec_len_total + rec_len as usize],
                    );
                    return Ok(());
                } else {
                    // 合并到前一个目录项的rec_len
                    let mut prev_dentry = Ext4DirEntry::try_from(
                        &self.content[prev_len_total..rec_len_total as usize],
                    )
                    .expect("[Ext4DirContentWE::delete_entry] merge into previous dentry failed");
                    prev_dentry.rec_len += rec_len;
                    prev_dentry
                        .write_to_mem(&mut self.content[prev_len_total..rec_len_total as usize]);
                }

                return Ok(());
            }
            prev_len_total = rec_len_total;
            rec_len_total += rec_len as usize;
        }
        Err(Errno::ENOENT)
    }
    // 在rename的时候如果new_dentry存在, 调用这个函数修改inode_num和file_type
    pub fn set_entry(
        &mut self,
        old_name: &str,
        new_inode_num: u32,
        new_file_type: u8,
    ) -> Result<(), &'static str> {
        let mut rec_len_total = 0;
        let content_len = self.content.len();

        while rec_len_total < content_len {
            let rec_len = u16::from_le_bytes([
                self.content[rec_len_total + 4],
                self.content[rec_len_total + 5],
            ]);
            let mut dentry = Ext4DirEntry::try_from(
                &self.content[rec_len_total..rec_len_total + rec_len as usize],
            )
            .map_err(|_| "DirEntry::try_from failed")?;

            let dentry_name = String::from_utf8_lossy(&dentry.name[..dentry.name_len as usize]);
            if dentry_name == old_name {
                dentry.inode_num = new_inode_num;
                dentry.file_type = new_file_type;
                dentry.write_to_mem(
                    &mut self.content[rec_len_total..rec_len_total + rec_len as usize],
                );
                return Ok(());
            }

            rec_len_total += rec_len as usize;
        }

        Err("Entry not found")
    }

    pub fn init_dot_dotdot(
        &mut self,
        parent_inode_num: u32,
        self_inode_num: u32,
        ext4_block_size: usize,
    ) {
        let mut dentry = Ext4DirEntry::default();
        // 初始化`.`目录项
        dentry.inode_num = self_inode_num;
        dentry.rec_len = 12;
        dentry.name_len = 1;
        dentry.file_type = EXT4_DT_DIR;
        dentry.name = vec![b'.'];
        dentry.write_to_mem(&mut self.content[0..9]);

        // 初始化`..`目录项
        dentry.inode_num = parent_inode_num;
        dentry.rec_len = ext4_block_size as u16 - 12;
        dentry.name_len = 2;
        dentry.name = vec![b'.', b'.'];
        dentry.write_to_mem(&mut self.content[12..22]);
    }
}

// 注意: ext4的bitmap一般会有多块, inode_bitmap_size = inodes_per_group / 8 (byte), block_bitmap_size = blocks_per_group / 8 (byte)
pub struct Ext4Bitmap<'a> {
    bitmap: &'a mut [u8; EXT4_BLOCK_SIZE],
}

impl<'a> Ext4Bitmap<'a> {
    pub fn new(bitmap: &'a mut [u8; EXT4_BLOCK_SIZE]) -> Self {
        Self { bitmap }
    }
    // 分配一个位
    // 返回分配的位的编号(是一个块内的偏移, 需要转换为inode_bitmap中的编号), 由上层调用者负责转换
    /// 注意: inode_num从1开始, 而bitmap的索引从0开始, bit_index = inode_num - 1
    /// 注意: inode_bitmap_size的单位是byte
    pub fn alloc(&mut self, inode_bitmap_size: usize) -> Option<usize> {
        // 逐字节处理, 加速alloc过程
        for (i, byte) in self.bitmap.iter_mut().enumerate() {
            if *byte != 0xff {
                for j in 0..8 {
                    if (*byte & (1 << j)) == 0 {
                        *byte |= 1 << j;
                        if i <= inode_bitmap_size {
                            // 这里加1是因为inode_num从1开始
                            return Some(i * 8 + j + 1);
                        } else {
                            // 找到第一个未使用的位时, 已经超出了inode_bitmap的大小, 说明inode_bitmap不够用
                            log::error!("i byte, j bit: {}, {}", i, j);
                            return None;
                        }
                    }
                }
            }
        }
        None
    }
    //尝试一次性分配 block_count 个连续块; 如果不能, 就返回够返回尽可能多的连续块
    pub fn alloc_contiguous(
        &mut self,
        bitmap_size: usize,
        max_count: usize,
    ) -> Option<(usize, u32)> {
        let total_bits = bitmap_size * 8;
        let mut current_run = 0;
        let mut start_bit = 0;
        let mut longest_run = 0;
        let mut longest_start = 0;

        for bit in 0..total_bits {
            let byte_index = bit / 8;
            let bit_index = bit % 8;

            if byte_index >= self.bitmap.len() {
                break;
            }

            if self.bitmap[byte_index] & (1 << bit_index) == 0 {
                if current_run == 0 {
                    start_bit = bit;
                }
                current_run += 1;
                if current_run > longest_run {
                    longest_run = current_run;
                    longest_start = start_bit;
                }
                if current_run == max_count {
                    // 找到了 max_count 个连续空闲位，立即返回
                    for b in start_bit..(start_bit + max_count) {
                        let bi = b / 8;
                        let bj = b % 8;
                        self.bitmap[bi] |= 1 << bj;
                    }
                    // 如果分配的是 inode bitmap，要从 1 开始编号
                    return Some((start_bit + 1, max_count as u32));
                }
            } else {
                current_run = 0;
            }
        }

        if longest_run > 0 {
            // 标记 longest_run 这段为已分配
            for b in longest_start..(longest_start + longest_run) {
                let bi = b / 8;
                let bj = b % 8;
                self.bitmap[bi] |= 1 << bj;
            }
            return Some((longest_start + 1, longest_run as u32));
        }

        None
    }

    // 注意block_offset只是inode_num % (block_size * 8), 需要上层调用者负责转换
    pub fn dealloc(&mut self, block_offset: usize, bitmap_size: usize) {
        // 逐字节处理, 加速dealloc过程
        let byte_index = block_offset / 8;
        let bit_index = block_offset % 8;
        if byte_index < self.bitmap.len() {
            // 检查是否在bitmap范围内
            if byte_index < bitmap_size {
                self.bitmap[byte_index] &= !(1 << bit_index);
            } else {
                log::error!(
                    "Dealloc block offset out of range: {}, bitmap size: {}",
                    block_offset,
                    bitmap_size
                );
            }
        } else {
            log::error!(
                "Dealloc block offset out of range: {}, bitmap length: {}",
                block_offset,
                self.bitmap.len()
            );
        }
    }
    /// 释放连续的块
    pub fn dealloc_contiguous(
        &mut self,
        start_block: usize,
        mut block_count: usize,
        bitmap_size: usize,
    ) {
        let mut byte_index = start_block / 8;
        let mut bit_index = start_block % 8;
        if byte_index < self.bitmap.len() {
            // 检查是否在bitmap范围内
            if byte_index + block_count < bitmap_size {
                for _ in 0..block_count {
                    self.bitmap[byte_index] &= !(1 << bit_index);
                    if bit_index == 7 {
                        // 移动到下一个字节
                        byte_index += 1;
                        bit_index = 0;
                    } else {
                        bit_index += 1;
                    }
                }
            } else {
                log::error!(
                "Dealloc block out of range, start_block: {}, block_count: {}, bitmap length: {}",
                start_block,
                block_count,
                self.bitmap.len()
            );
            }
        }
    }
}

// 硬编码, 对于ext4块大小为4096的情况
pub const EXTENT_BLOCK_MAX_ENTRIES: usize = 340; // (ext4_block_size - 12(extent_header)) / 12(ext4_extent_idx)
pub struct Ext4ExtentBlock<'a> {
    block: &'a mut [u8; EXT4_BLOCK_SIZE],
}

impl<'a> Ext4ExtentBlock<'a> {
    pub fn new(block: &'a mut [u8; EXT4_BLOCK_SIZE]) -> Self {
        Self { block }
    }
    fn extent_header(&self) -> &mut Ext4ExtentHeader {
        unsafe { &mut *(self.block.as_ptr() as *mut Ext4ExtentHeader) }
    }
}

impl<'a> Ext4ExtentBlock<'a> {
    // 递归查找
    pub fn lookup_extent(
        &self,
        logical_block: u32,
        block_device: Arc<dyn BlockDevice>,
        ext4_block_size: usize,
    ) -> Option<Ext4Extent> {
        let header = self.extent_header();
        if header.depth == 0 {
            // 叶子节点
            let extents = unsafe {
                core::slice::from_raw_parts(
                    self.block.as_ptr().add(12) as *const Ext4Extent,
                    header.entries as usize,
                )
            };
            for extent in extents {
                if logical_block >= extent.logical_block
                    && logical_block < extent.logical_block + extent.len as u32
                {
                    return Some(*extent);
                }
            }
            return None;
        } else {
            // 索引节点
            let idxs = unsafe {
                core::slice::from_raw_parts(
                    self.block.as_ptr().add(12) as *const Ext4ExtentIdx,
                    header.entries as usize,
                )
            };
            if let Some(idx) = idxs.iter().find(|idx| logical_block >= idx.block) {
                let block_num = idx.physical_leaf_block();
                return Ext4ExtentBlock::new(
                    get_block_cache(block_num, block_device.clone(), ext4_block_size)
                        .lock()
                        .get_mut(0),
                )
                .lookup_extent(logical_block, block_device, ext4_block_size);
            } else {
                return None;
            }
        }
    }
    /// 递归遍历整个 extent B+ 树，收集所有叶子节点的 Ext4Extent
    pub fn iter_all_extents(
        &mut self,
        block_device: Arc<dyn BlockDevice>,
        block_size: usize,
        result: &mut Vec<Ext4Extent>,
    ) {
        let header = self.extent_header();

        if header.depth > 0 {
            // 当前是索引节点
            unimplemented!("[Ext4ExtentBlock::iter_all_extents]Iterating over index nodes is not implemented yet");
            // for idx in self.extent_idxs(&header) {
            //     let child_block = idx.physical_leaf_block();

            //     let mut child_block_ref = Ext4ExtentBlock::new(
            //         get_block_cache(child_block, block_device.clone(), block_size)
            //             .lock()
            //             .get_mut(0),
            //     );
            //     child_block_ref.iter_all_extents(block_device.clone(), block_size, result);
            // }
        } else {
            // 当前是叶子节点，收集 extent 列表
            let header = self.extent_header();
            let extents = unsafe {
                core::slice::from_raw_parts(
                    self.block.as_ptr().add(12) as *const Ext4Extent,
                    header.entries as usize,
                )
            };
            result.extend(extents);
        }
    }
    // 递归插入
    pub fn insert_extent(
        &mut self,
        logical_block_num: u32,
        physical_block_num: u64,
        blocks_count: u32,
    ) -> Result<(), &'static str> {
        let header = self.extent_header();
        if header.depth == 0 {
            // 叶子节点
            let extents = unsafe {
                core::slice::from_raw_parts_mut(
                    self.block.as_ptr().add(12) as *mut Ext4Extent,
                    header.entries as usize,
                )
            };
            // 遍历, 查找合适的extent合并
            for (i, extent) in extents.iter().enumerate() {
                let lend_block = extent.logical_block + extent.len as u32;
                let pend_block = extent.physical_start_block() as u32 + extent.len as u32;

                // 情况 0: 直接合并, 物理块号连续, 且逻辑块号连续
                if logical_block_num == lend_block
                    && physical_block_num as u32 == pend_block
                    && extent.len < 32768
                {
                    unsafe {
                        let extent_ptr = self.block.as_ptr().add(12 + i * 12) as *mut Ext4Extent;
                        (*extent_ptr).len += blocks_count as u16;
                        // log::info!("[update_extent] Extend existing extent");
                        return Ok(());
                    }
                }
            }
            // 情况 1: extent entries 超出最大数量, 需要创建索引节点
            if header.entries as usize >= EXTENT_BLOCK_MAX_ENTRIES {
                panic!("Extent block is full, Uimplement split_extent_block");
            }
            // 情况 2: 插入新的 extent, 并按logical_block排序, 更新header.entries
            // 情况1已经保证了有位置可插入
            let new_extent = Ext4Extent::new(
                logical_block_num,
                blocks_count as u16,
                physical_block_num as usize,
            );
            let insert_pos = extents
                .iter()
                .position(|extent| extent.logical_block > logical_block_num)
                .unwrap_or(extents.len());
            unsafe {
                let extents_ptr = self.block.as_ptr().add(12) as *mut Ext4Extent;
                core::ptr::copy(
                    extents_ptr.add(insert_pos),
                    extents_ptr.add(insert_pos + 1),
                    (header.entries as usize) - insert_pos,
                );
                // 写入新的 extent
                core::ptr::write(extents_ptr.add(insert_pos), new_extent);
            }
            header.entries += 1;
            Ok(())
        } else {
            // 索引节点
            unimplemented!()
        }
    }
    /// 初始化当前 block 成为一个叶子节点
    /// right_extents: 要拷贝到这个叶子节点的新 extent 列表
    pub fn init_as_leaf(&mut self, extents: &[Ext4Extent]) {
        // 清零整个 block（防止脏数据）
        self.block.fill(0);

        // 初始化 extent_header
        let header = unsafe { &mut *(self.block.as_mut_ptr() as *mut Ext4ExtentHeader) };
        header.magic = 0xf30a; // EXT4_EXT_MAGIC
        header.entries = extents.len() as u16;
        header.max = EXTENT_BLOCK_MAX_ENTRIES as u16;
        header.depth = 0; // 叶子节点

        // 拷贝 right_extents 到 block中
        for (i, extent) in extents.iter().enumerate() {
            unsafe {
                let dst_ptr = self
                    .block
                    .as_mut_ptr()
                    .add(12 + i * core::mem::size_of::<Ext4Extent>())
                    as *mut Ext4Extent;
                dst_ptr.write(*extent);
            }
        }
    }
}
