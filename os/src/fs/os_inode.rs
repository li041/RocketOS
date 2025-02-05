use alloc::{
    sync::{Arc, Weak},
    vec::Vec,
};
use virtio_drivers::device::socket::SocketError;

use crate::mutex::SpinNoIrqLock;

use super::{
    dentry::{self, lookup_dcache, Dentry},
    inode::InodeOp,
    path_old::PathOld,
};

pub struct OSInode {
    inode: Arc<SpinNoIrqLock<dyn InodeOp>>,
    dentry: Arc<Dentry>,
}
