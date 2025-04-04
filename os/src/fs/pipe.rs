use alloc::vec;
use alloc::{sync::Arc, vec::Vec};
use core::cmp;

use crate::{mutex::SpinNoIrqLock, task::yield_current_task};

use super::{file::FileOp, FileMeta, FileOld};

const PIPE_BUFFER_SIZE: usize = 4096;

pub struct Pipe {
    buffer: Arc<SpinNoIrqLock<PipeRingBuffer>>,
    readable: bool,
    writeable: bool,
}

impl Pipe {
    /// return (pipe_read, pipe_write)
    pub fn new_pair() -> (Arc<Self>, Arc<Self>) {
        let buffer = Arc::new(SpinNoIrqLock::new(PipeRingBuffer::new(PIPE_BUFFER_SIZE)));
        (
            Arc::new(Self {
                buffer: buffer.clone(),
                readable: true,
                writeable: false,
            }),
            Arc::new(Self {
                buffer: buffer.clone(),
                readable: false,
                writeable: true,
            }),
        )
    }

    pub fn read_inner(&self, buf: &mut [u8]) -> usize {
        let mut buffer = self.buffer.lock();
        buffer.read(buf)
    }

    pub fn write_inner(&self, buf: &[u8]) -> usize {
        let mut buffer = self.buffer.lock();
        buffer.write(buf)
    }
}

impl FileOp for Pipe {
    fn as_any(&self) -> &dyn core::any::Any {
        self
    }
    fn read<'a>(&'a self, buf: &'a mut [u8]) -> usize {
        loop {
            let ret = self.read_inner(buf);
            if ret != 0 {
                return ret;
            } else if self.buffer.lock().is_empty() {
                return 0;
            } else {
                yield_current_task();
            }
        }
    }
    fn write<'a>(&'a self, buf: &'a [u8]) -> usize {
        self.write_inner(buf)
    }
    fn readable(&self) -> bool {
        self.readable
    }
    fn writable(&self) -> bool {
        self.writeable
    }
    fn get_offset(&self) -> usize {
        panic!("Pipe does not support get_offset")
    }
    fn seek(&self, _offset: usize) {
        panic!("Pipe does not support seek")
    }
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
enum RingBufferState {
    #[default]
    Empty,
    Full,
    Normal,
}

pub struct PipeRingBuffer {
    arr: Vec<u8>,
    // NOTE: When and only when `head` equals `tail`, `state` can only be `Full` or `Empty`.
    head: usize,
    tail: usize,
    state: RingBufferState,
}

impl PipeRingBuffer {
    pub fn new(len: usize) -> Self {
        Self {
            arr: vec![0; len],
            head: 0,
            tail: 0,
            state: RingBufferState::Empty,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.state == RingBufferState::Empty
    }

    pub fn is_full(&self) -> bool {
        self.state == RingBufferState::Full
    }

    /// Read as much as possible to fill `buf`.
    pub fn read(&mut self, buf: &mut [u8]) -> usize {
        if self.state == RingBufferState::Empty || buf.is_empty() {
            assert!(self.head == self.tail || buf.is_empty());
            return 0;
        }

        let ret_len;
        let n = self.arr.len();
        if self.head < self.tail {
            ret_len = cmp::min(self.tail - self.head, buf.len());
            buf[..ret_len].copy_from_slice(&self.arr[self.head..self.head + ret_len]);
        } else {
            // also handles full
            ret_len = cmp::min(n - self.head + self.tail, buf.len());
            if ret_len <= (n - self.head) {
                buf[..ret_len].copy_from_slice(&self.arr[self.head..self.head + ret_len]);
            } else {
                let right_len = n - self.head;
                buf[..right_len].copy_from_slice(&self.arr[self.head..]);
                buf[right_len..ret_len].copy_from_slice(&self.arr[..(ret_len - right_len)]);
            }
        }
        self.head = (self.head + ret_len) % n;

        if self.head == self.tail {
            self.state = RingBufferState::Empty;
        } else {
            self.state = RingBufferState::Normal;
        }

        ret_len
    }

    /// Write as much as possible to fill the ring buffer.
    pub fn write(&mut self, buf: &[u8]) -> usize {
        if self.state == RingBufferState::Full || buf.is_empty() {
            return 0;
        }

        let ret_len;
        let n = self.arr.len();
        if self.head <= self.tail {
            // also handles empty
            ret_len = cmp::min(n - (self.tail - self.head), buf.len());
            if ret_len <= (n - self.tail) {
                self.arr[self.tail..self.tail + ret_len].copy_from_slice(&buf[..ret_len]);
            } else {
                self.arr[self.tail..].copy_from_slice(&buf[..n - self.tail]);
                self.arr[..(ret_len - (n - self.tail))]
                    .copy_from_slice(&buf[n - self.tail..ret_len]);
            }
        } else {
            ret_len = cmp::min(self.head - self.tail, buf.len());
            self.arr[self.tail..self.tail + ret_len].copy_from_slice(&buf[..ret_len]);
        }
        self.tail = (self.tail + ret_len) % n;

        if self.head == self.tail {
            self.state = RingBufferState::Full;
        } else {
            self.state = RingBufferState::Normal;
        }

        ret_len
    }
}
