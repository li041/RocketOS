use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicI32, Ordering};
use spin::Mutex;

use crate::syscall::errno::{Errno, SyscallRet};
use crate::task::{current_task, wait, wakeup, Tid};
use crate::fs::file::{FileOp, OpenFlags};
use crate::fs::pipe::{PipeRingBuffer, RingBufferStatus};

/// 全双工 SocketPairBuffer 使用两个 PipeRingBuffer
pub struct SocketPairBuffer {
    /// A -> B 方向缓冲区
    pub a_to_b: Arc<Mutex<PipeRingBuffer>>,
    /// B -> A 方向缓冲区
    pub b_to_a: Arc<Mutex<PipeRingBuffer>>,
}

impl SocketPairBuffer {
    pub fn new() -> Self {
        let buf = Self {
            a_to_b: Arc::new(Mutex::new(PipeRingBuffer::new())),
            b_to_a: Arc::new(Mutex::new(PipeRingBuffer::new())),
        };
        buf
    }
}

/// BufferEnd 代表 SocketPair 的一端
pub struct BufferEnd {
    /// 用于读操作的缓冲区
    read_buf: Arc<Mutex<PipeRingBuffer>>,
    /// 用于写操作的缓冲区
    write_buf: Arc<Mutex<PipeRingBuffer>>,
    flags: AtomicI32,
    // 标记该端是读还是写
    readable: bool,
}

impl BufferEnd {
    pub fn new(
        read_buf: Arc<Mutex<PipeRingBuffer>>,
        write_buf: Arc<Mutex<PipeRingBuffer>>,
        flags: OpenFlags,
    ) -> Self {
        Self {
            read_buf,
            write_buf,
            flags: AtomicI32::new(flags.bits()),
            readable: true,
        }
    }

    pub fn from_pair(
        buf: &SocketPairBuffer,
        endpoint: usize,
        flags: OpenFlags,
    ) -> Self {
        if endpoint == 0 {
            Self::new(buf.b_to_a.clone(), buf.a_to_b.clone(), flags)
        } else {
            let mut end = Self::new(buf.a_to_b.clone(), buf.b_to_a.clone(), flags);
            end.readable = true;
            end
        }
    }
}

impl FileOp for BufferEnd {
    fn as_any(&self) -> &dyn core::any::Any {
        self
    }

    fn read<'a>(&'a self, buf: &'a mut [u8]) -> SyscallRet {
        let nonblock =
            self.flags.load(Ordering::Relaxed) & OpenFlags::O_NONBLOCK.bits() != 0;
        loop {
            let mut ring = self.read_buf.lock();
            if ring.status == RingBufferStatus::EMPTY {
                if nonblock {
                    return Err(Errno::EAGAIN);
                }
                ring.add_waiter(current_task().tid());
                drop(ring);
                if wait() == -1 {
                    return Err(Errno::ERESTARTSYS);
                }
                continue;
            }
            let n = ring.buffer_read(buf);
            let waiter = ring.get_one_waiter();
            drop(ring);
            if waiter != 0 {
                wakeup(waiter);
            }
            return Ok(n);
        }
    }

    fn write<'a>(&'a self, buf: &'a [u8]) -> SyscallRet {
        let nonblock =
            self.flags.load(Ordering::Relaxed) & OpenFlags::O_NONBLOCK.bits() != 0;
        loop {
            let mut ring = self.write_buf.lock();
            if ring.status == RingBufferStatus::FULL {
                if nonblock {
                    return Err(Errno::EAGAIN);
                }
                ring.add_waiter(current_task().tid());
                drop(ring);
                if wait() == -1 {
                    return Err(Errno::ERESTARTSYS);
                }
                continue;
            }
            let n = ring.buffer_write(buf);
            let waiter = ring.get_one_waiter();
            drop(ring);
            if waiter != 0 {
                wakeup(waiter);
            }
            return Ok(n);
        }
    }

    fn ioctl(&self, _: usize, _: usize) -> SyscallRet {
        Err(Errno::ENOTTY)
    }
    fn fsync(&self) -> SyscallRet {
        Err(Errno::EINVAL)
    }
    fn readable(&self) -> bool {
        let ring = self.read_buf.lock();
        ring.status != RingBufferStatus::EMPTY
    }
    fn writable(&self) -> bool {
        let ring = self.write_buf.lock();
        ring.status != RingBufferStatus::FULL
    }
    fn seek(&self, _: isize, _: crate::fs::uapi::Whence) -> SyscallRet {
        Err(Errno::ESPIPE)
    }
    fn r_ready(&self) -> bool {
        let ring = self.read_buf.lock();
        ring.status != RingBufferStatus::EMPTY
    }
    fn w_ready(&self) -> bool {
        let ring = self.write_buf.lock();
        ring.status != RingBufferStatus::FULL
    }
    fn hang_up(&self) -> bool {
        if self.readable {
            let ring = self.read_buf.lock();
            ring.all_write_ends_closed()
        } else {
            let ring = self.write_buf.lock();
            ring.all_read_ends_closed()
        }
    }
    fn get_flags(&self) -> OpenFlags {
        OpenFlags::from_bits(self.flags.load(Ordering::Relaxed)).unwrap()
    }
    fn set_flags(&self, f: OpenFlags) {
        self.flags.store(f.bits(), Ordering::Relaxed);
    }
}

/// 创建 SocketPair 的两个 BufferEnd
pub fn create_buffer_ends(flags: OpenFlags) -> (BufferEnd, BufferEnd) {
    let buf = SocketPairBuffer::new();
    (
        BufferEnd::new(buf.b_to_a.clone(), buf.a_to_b.clone(), flags),
        BufferEnd::new(buf.a_to_b.clone(), buf.b_to_a.clone(), flags),
    )
}
