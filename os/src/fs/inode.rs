//! new
use crate::ext4::inode::Ext4Inode;

use super::dentry::Dentry;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

/// 由页缓存直接和block device交互
/// inode查extent_tree, 返回页号
/// page_offset是页偏移, page_offset * PAGE_SIZE是字节偏移
pub trait InodeOp: Send + Sync {
    // 用于文件读写
    fn read<'a>(&'a self, offset: usize, buf: &'a mut [u8]) -> usize;
    fn write<'a>(&'a self, page_offset: usize, buf: &'a [u8]) -> usize;
    // 返回目录项
    // 先查找Denrty的children, 如果没有再查找目录
    // 注意这里的返回值不是`Option<..>`, 对于没有查找的情况, 返回负目录项`dentry.inode = NULL`
    // lookup需要加载Inode进入内存, 关联到Dentry(除非是负目录项), 建立dentry的父子关系
    // 注意: 这里可能返回负目录项
    // 上层调用者保证:
    //      1. 先查找dentry cache, 如果没有再查找目录
    //      3. 上层调用者保证将dentry放进dentry cache中
    fn lookup<'a>(&'a self, name: &str, parent_dentry: Arc<Dentry>) -> Arc<Dentry>;
    // self是目录inode, name是新建文件的名字, mode是新建文件的类型
    // fn mknod<'a>(&'a self, name: &str, mode: u16) -> Arc<Dentry>;
    // self是目录, Dentry是上层根据文件名新建的负目录项(已经建立了父子关系)
    // 上层调用者保证:
    //      1. 创建的文件名在目录中不存在
    //      2. Dentry的inode字段为None(负目录项)
    fn create<'a>(&'a self, negative_dentry: Arc<Dentry>, mode: u16);
    // 创建临时文件, 用于临时文件系统, inode没有对应的路径, 不会分配目录项
    // 临时文件没有对应的目录项, 只能通过fd进行访问
    // 与create的唯一区别是: 1. 没有对应的目录项
    fn tmpfile<'a>(&'a self, mode: u16) -> Arc<Ext4Inode>;
    fn mkdir<'a>(&'a self, parent_inode_num: u32, dentry: Arc<Dentry>, mode: u16);
    // 上层readdir调用
    fn getdents(&self) -> Vec<String>;
    // 检查是否是目录, 且有子目录项可以用于lookup
    fn can_lookup(&self) -> bool;
}
