use alloc::sync::Arc;

use crate::mutex::SpinNoIrqLock;

use super::inode::InodeOp;

pub struct File {
    inner: SpinNoIrqLock<FileInner>,
}

pub struct FileInner {
    // pub inode: ,
    /// 单位是字节
    pub offset: usize,
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
