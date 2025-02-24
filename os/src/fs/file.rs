use alloc::{sync::Arc, vec::Vec};
use log::info;
use virtio_drivers::PAGE_SIZE;

use crate::mutex::SpinNoIrqLock;

use super::inode::InodeOp;

pub struct File {
    inner: SpinNoIrqLock<FileInner>,
}

pub struct FileInner {
    // pub inode: ,
    /// 单位是字节
    offset: usize,
    pub inode: Arc<dyn InodeOp>,
}

/// File trait
pub trait FileOp: Send + Sync {
    /// Read file to `UserBuffer`
    fn read<'a>(&'a self, buf: &'a mut [u8]) -> usize;
    /// Write `UserBuffer` to file
    fn write<'a>(&'a self, buf: &'a [u8]) -> usize;
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
    pub fn new(inode: Arc<dyn InodeOp>) -> Self {
        Self {
            inner: SpinNoIrqLock::new(FileInner { offset: 0, inode }),
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
}
