use alloc::sync::Arc;

use super::{dentry::Dentry, mount::VfsMount};

pub struct Path {
    pub mnt: Arc<VfsMount>,
    pub dentry: Arc<Dentry>,
}

impl Path {
    pub fn zero_init() -> Arc<Self> {
        Arc::new(Path {
            mnt: Arc::new(VfsMount::zero_init()),
            dentry: Arc::new(Dentry::zero_init()),
        })
    }
    pub fn new(mnt: Arc<VfsMount>, dentry: Arc<Dentry>) -> Arc<Self> {
        Arc::new(Path { mnt, dentry })
    }
}
