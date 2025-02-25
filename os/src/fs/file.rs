use core::any::Any;

use alloc::{sync::Arc, vec::Vec};
use log::info;
use virtio_drivers::PAGE_SIZE;

use crate::mutex::SpinNoIrqLock;

use super::{dentry::Dentry, inode::InodeOp, path::Path};

// 普通文件
pub struct File {
    inner: SpinNoIrqLock<FileInner>,
}

pub struct FileInner {
    // pub inode: ,
    /// 单位是字节
    offset: usize,
    // pub dentry: Arc<Dentry>,
    pub path: Arc<Path>,
    pub inode: Arc<dyn InodeOp>,
    pub flags: usize,
}

/// File trait
pub trait FileOp: Any + Send + Sync {
    fn as_any(&self) -> &dyn Any;
    // 从文件中读取数据到buf中, 返回读取的字节数, 同时更新文件偏移量
    fn read<'a>(&'a self, buf: &'a mut [u8]) -> usize;
    /// Write `UserBuffer` to file
    fn write<'a>(&'a self, buf: &'a [u8]) -> usize;
    // move the file offset
    fn seek(&self, offset: usize);
    // Get the file offset
    fn get_offset(&self) -> usize;
    // readable
    fn readable(&self) -> bool;
    // writable
    fn writable(&self) -> bool;
}

impl File {
    pub fn inner_handler<T>(&self, f: impl FnOnce(&mut FileInner) -> T) -> T {
        f(&mut self.inner.lock())
    }
    pub fn add_offset(&self, offset: usize) {
        self.inner_handler(|inner| inner.offset += offset);
    }
    pub fn get_offset(&self) -> usize {
        self.inner_handler(|inner| inner.offset)
    }
}

impl File {
    pub fn new(path: Arc<Path>, inode: Arc<dyn InodeOp>, flags: usize) -> Self {
        Self {
            inner: SpinNoIrqLock::new(FileInner {
                offset: 0,
                path,
                inode,
                flags,
            }),
        }
    }
    /// Read all data inside a inode into vector
    pub fn read_all(&self) -> Vec<u8> {
        info!("[File::read_all]");
        let inode = self.inner_handler(|inner| inner.inode.clone());
        let mut buffer = [0u8; PAGE_SIZE];
        let mut v: Vec<u8> = Vec::new();
        // Debug
        let mut totol_read = 0;
        loop {
            let offset = self.get_offset();
            let len = inode.read(offset, &mut buffer);
            totol_read += len;
            if len == 0 {
                break;
            }
            self.add_offset(len);
            v.extend_from_slice(&buffer[..len]);
        }
        log::info!("read_all: totol_read: {}", totol_read);
        v
    }
}

impl FileOp for File {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn read<'a>(&'a self, buf: &'a mut [u8]) -> usize {
        let read_size = self.inner_handler(|inner| inner.inode.read(inner.offset, buf));
        self.add_offset(read_size);
        read_size
    }

    fn write<'a>(&'a self, buf: &'a [u8]) -> usize {
        let write_size = self.inner_handler(|inner| inner.inode.write(inner.offset, buf));
        self.add_offset(write_size);
        write_size
    }
    fn seek(&self, offset: usize) {
        self.inner_handler(|inner| inner.offset = offset);
    }
    fn get_offset(&self) -> usize {
        self.inner_handler(|inner| inner.offset)
    }
    // O_RDONLY = 0, 以只读方式打开文件, 具体的权限检查由VFS层完成
    // Todo:
    fn readable(&self) -> bool {
        // self.inner_handler(|inner| inner.flags & O_RDONLY != 0)
        true
    }
    // Todo:
    fn writable(&self) -> bool {
        true
    }
}

pub const O_RDONLY: usize = 0;
pub const O_WRONLY: usize = 1;
pub const O_RDWR: usize = 2;
pub const O_CREAT: usize = 0x40;
pub const O_DIRECTORY: usize = 0x10000;
// bitflags! {
//     ///Open file flags
//     pub struct OpenFlags: u32 {
//         const APPEND = 1 << 10;
//         const ASYNC = 1 << 13;
//         const DIRECT = 1 << 14;
//         const DSYNC = 1 << 12;
//         const EXCL = 1 << 7;
//         const NOATIME = 1 << 18;
//         const NOCTTY = 1 << 8;
//         const NOFOLLOW = 1 << 17;
//         const PATH = 1 << 21;
//         /// TODO: need to find 1 << 15
//         const TEMP = 1 << 15;
//         /// Read only
//         const RDONLY = 0;
//         /// Write only
//         const WRONLY = 1 << 0;
//         /// Read & Write
//         const RDWR = 1 << 1;
//         /// Allow create
//         const CREATE = 1 << 6;
//         /// Clear file and return an empty one
//         const TRUNC = 1 << 9;
//         /// Directory
//         const DIRECTORY = 1 << 16;
//         /// Enable the close-on-exec flag for the new file descriptor
//         const CLOEXEC = 1 << 19;
//         /// When possible, the file is opened in nonblocking mode
//         const NONBLOCK = 1 << 11;
//     }
// }

// impl OpenFlags {
//     /// Do not check validity for simplicity
//     /// Return (readable, writable)
//     pub fn read_write(&self) -> (bool, bool) {
//         if self.is_empty() {
//             (true, false)
//         } else if self.contains(Self::WRONLY) {
//             (false, true)
//         } else {
//             (true, true)
//         }
//     }
// }
