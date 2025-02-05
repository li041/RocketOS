pub trait SuperBlockOp: Send + Sync {
    // 返回VFS层的Inode
    fn alloc_inode(&self) -> usize;
}
