//! File system in os
use core::cell::OnceCell;

use alloc::{string::ToString, sync::Arc};
use dentry::{insert_dentry, Dentry, DENTRY_CACHE};
use inode::InodeOp;
use inode_trait::InodeTrait;
// use alloc::sync::Arc;
pub use os_inode_old::{create_dir, list_apps, open_file, open_inode, OpenFlags};
pub use stdio::{Stdin, Stdout};

use crate::{
    drivers::BLOCK_DEVICE,
    ext4::{fs::Ext4FileSystem, inode::Ext4Inode},
    mutex::SpinNoIrqLock,
};
use lazy_static::lazy_static;

pub mod dentry;
pub mod file;
pub mod inode;
pub mod inode_trait;
pub mod namei;
mod os_inode;
mod os_inode_old;
pub mod page_cache;
pub mod path_old;
pub mod pipe;
mod stdio;
pub mod super_block;

// 文件系统的锁先使用SpinNoIrqLock, Todo: 改成RwLock
pub type FSMutex<T> = SpinNoIrqLock<T>;
// Todo: 这里动态初始化一个FS_block_size
lazy_static! {
    pub static ref FS_BLOCK_SIZE: usize = 4096;
}
#[allow(unused)]
use crate::drivers::block::VIRTIO_BLOCK_SIZE;

pub struct FileMeta {
    pub inode: Option<Arc<dyn InodeTrait>>,
    pub offset: usize,
}

impl FileMeta {
    pub fn new(inode: Option<Arc<dyn InodeTrait>>, offset: usize) -> Self {
        Self { inode, offset }
    }
}

/// File trait
pub trait FileOld: Send + Sync {
    /// If readable
    fn readable(&self) -> bool;
    /// If writable
    fn writable(&self) -> bool;
    /// Read file to `UserBuffer`
    fn read<'a>(&'a self, buf: &'a mut [u8]) -> usize;
    /// Write `UserBuffer` to file
    fn write<'a>(&'a self, buf: &'a [u8]) -> usize;
    fn get_meta(&self) -> FileMeta;
    fn seek(&self, offset: usize);
}

// 指示在当前工作目录下打开文件
pub const AT_FDCWD: isize = -100;
pub const AT_REMOVEDIR: u32 = 0x200;

lazy_static! {
    // 这里不需要加锁, 锁的粒度在Ext4FileSystem内部
    pub static ref EXT4FS: Arc<Ext4FileSystem> = Ext4FileSystem::open(BLOCK_DEVICE.clone());
}

lazy_static! {
    // root_inode是一个全局变量, 用于表示根目录
    // 需要加锁, 因为在多线程环境下可能会有多个线程同时访问
    pub static ref ROOT_INODE: Arc<Ext4Inode> = {
        let root_inode = Arc::new(Ext4Inode::new_root(
            BLOCK_DEVICE.clone(),
            EXT4FS.clone(),
            &EXT4FS.block_groups[0].clone(),
        ));
        // 更新DentryCache
        let root_dentry = Dentry::new("".to_string(), 2, None, root_inode.clone());
        root_dentry.inner.lock().parent = Some(Arc::downgrade(&root_dentry));
        insert_dentry(root_dentry);
        root_inode
    };
}

pub fn get_root_inode() -> Arc<dyn InodeOp> {
    ROOT_INODE.clone()
}
