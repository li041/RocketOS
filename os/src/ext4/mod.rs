use alloc::{
    string::{String, ToString},
    sync::Arc,
};
use dentry::EXT4_DT_REG;
use inode::{load_inode, Ext4Inode, EXT4_EXTENTS_FL};

use crate::fs::{
    dentry::{lookup_dcache, Dentry},
    inode::InodeOp,
};

use alloc::vec::Vec;

mod block_group;
pub mod block_op;
pub mod dentry;
pub mod extent_tree;
pub mod fs;
pub mod inode;
pub mod super_block;

impl InodeOp for Ext4Inode {
    fn read<'a>(&'a self, offset: usize, buf: &'a mut [u8]) -> usize {
        self.read(offset, buf).expect("Ext4Inode::read failed")
    }

    fn write<'a>(&'a self, page_offset: usize, buf: &'a [u8]) -> usize {
        self.write(page_offset, buf)
    }
    // 上层调用者应先查找DentryCache, 如果没有才调用该函数
    // 先查找parent_entry的child(child是惰性加载的), 如果还没有则从目录中查找
    // name在parent_entry下的命名空间下, 不是绝对路径, 例如`/a/b/c`中的`c`, parent_entry是`/a/b`
    // 对于之前未加载的inode: 1. 加载inode 2. 关联到Dentry 3. 建立dentry的父子关系
    fn lookup<'a>(&'a self, name: &str, parent_entry: Arc<Dentry>) -> Arc<Dentry> {
        log::info!("lookup: {}", name);
        let mut dentry = Dentry::negative(name.to_string(), Some(parent_entry.clone()));
        if let Some(child) = parent_entry.inner.lock().children.get(name) {
            // 先查找parent_entry的child
            return child.clone();
        } else {
            // 从目录中查找
            if let Some(ext4_dentry) = self.lookup(name) {
                log::info!("lookup: ext4_dentry: {:?}", ext4_dentry);
                let inode_num = ext4_dentry.inode_num as usize;
                // 1.从磁盘加载inode
                let inode = load_inode(
                    inode_num,
                    self.block_device.clone(),
                    self.ext4_fs.upgrade().unwrap().clone(),
                );
                // 2. 关联到Dentry
                dentry = Dentry::new(
                    name.to_string(),
                    inode_num,
                    Some(parent_entry.clone()),
                    inode,
                );
            }
            // } else {
            // 不存在, 返回负目录项
            // }
        }
        // 注意: 这里建立父子关系的dentry可能是负目录项
        // 3. 建立dentry的父子关系
        parent_entry
            .inner
            .lock()
            .children
            .insert(name.to_string(), dentry.clone());
        dentry
    }
    // Todo: 增加日志
    // 1. 创建新的inode, 关联到dentry
    // 2. 更新父目录的数据块
    // 上层调用者保证: dentry是负目录项, 且父子关系已经建立
    fn create<'a>(&'a self, dentry: Arc<Dentry>, mode: u16) {
        // dentry应该是负目录项
        assert!(dentry.is_negative());
        // 分配inode_num
        let new_inode_num = self
            .ext4_fs
            .upgrade()
            .unwrap()
            .alloc_inode(self.block_device.clone(), false);
        // 初始化新的inode结构
        let new_inode = Ext4Inode::new(
            mode | 0o777,
            EXT4_EXTENTS_FL,
            self.ext4_fs.clone(),
            self.block_device.clone(),
        );
        // 在父目录中添加对应项
        self.add_entry(dentry.clone(), new_inode_num as u32, EXT4_DT_REG);
        // 关联到dentry
        dentry.inner.lock().inode = Some(new_inode);
    }
    fn getdents(&self) -> Vec<String> {
        self.getdents().iter().map(|s| s.get_name()).collect()
    }
}
