use super::{
    dentry::{insert_dentry, lookup_dcache_with_absolute_path, Dentry},
    inode::InodeOp,
    os_inode::OSInode,
};
use crate::{
    ext4::{dentry, inode},
    fs::{dentry::lookup_dcache, get_root_inode},
    mutex::SpinNoIrqLock,
    task::current_task,
};
use alloc::{sync::Arc, vec::Vec};
use bitflags::parser;

pub struct Nameidata<'a> {
    path_segments: Vec<&'a str>,
    // 以下字段在路径解析过程中需要更新
    // 注意Dentry和InodeOp的锁粒度都在他们自己的结构体内部
    dentry: Arc<Dentry>,
    inode: Arc<dyn InodeOp>,
    // 当前处理到的路径
    depth: usize,
}

impl<'a> Nameidata<'a> {
    fn new(path: &'a str) -> Self {
        let path_segments: Vec<&'a str> = path.split('/').filter(|s| !s.is_empty()).collect();
        let mut inode: Arc<dyn InodeOp>;
        let mut dentry: Arc<Dentry>;
        if path.starts_with("/") {
            // 绝对路径
            inode = get_root_inode();
            dentry = lookup_dcache_with_absolute_path("").unwrap();
        } else {
            // 相对路径
            let current_task = current_task();
            let cwd = current_task.inner.lock().cwd.clone();
            dentry = lookup_dcache_with_absolute_path(&cwd).unwrap();
            if let Some(cwd_inode) = dentry.inner.lock().inode.as_ref() {
                inode = cwd_inode.clone();
            } else {
                panic!("No inode found for cwd");
            }
        }
        Nameidata {
            path_segments,
            dentry,
            inode,
            depth: 0,
        }
    }
}

// Todo:
// pub fn user_path_create(dfd: i32, path: &str) {}

// Todo: 增加权限检查
// pub fn path_lookupat(path: &str, flags: i32) -> Result<Arc<dyn InodeOp>, isize> {
// let mut nd = Nameidata::new(path);
// link_path_walk(&mut nd)
//     .map(|inode| {
//         if flags & 0x100 == 0 {
//             // O_CREAT flag not set
//             return inode;
//         }
//         // O_CREAT flag set
//         let mut inner = nd.dentry.inner.lock();
//         if inner.inode.is_none() {
//             let parent_inode = inner.parent.as_ref().unwrap().upgrade().unwrap();
//             let new_inode = parent_inode.create(&nd.path_segments[nd.depth], 0o755);
//             let new_dentry = insert_dentry(&nd.dentry, &nd.path_segments[nd.depth], new_inode);
//             nd.dentry = new_dentry;
//             nd.inode = new_inode;
//         }
//         nd.inode.clone()
//     })
//     .ok_or(-1)
// }

// 注意: name可能为"."或"..", 在DentryCache中绝对路径不包括这两个特殊目录
pub fn link_path_walk(nd: &mut Nameidata) -> Option<Arc<dyn InodeOp>> {
    let mut current_dir = nd.dentry.absolute_path.clone();
    while nd.depth < nd.path_segments.len() {
        if nd.path_segments[nd.depth] == "." {
            continue;
        } else if nd.path_segments[nd.depth] == ".." {
            // 所有的dentry都有parent(根目录的parent是自己), 直接unwarp
            let parent_dentry = nd
                .dentry
                .inner
                .lock()
                .parent
                .clone()
                .unwrap()
                .upgrade()
                .unwrap();
            let parent_inode = parent_dentry.inner.lock().inode.clone().unwrap();
            nd.depth += 1;
            nd.dentry = parent_dentry;
            nd.inode = parent_inode;
        } else {
            // name是String
            current_dir = current_dir + "/" + nd.path_segments[nd.depth];
            // 查找DentryCache, 如果没有则从目录中查找
            let dentry = lookup_dcache_with_absolute_path(&current_dir)
                .or_else(|| {
                    let current_dentry = nd.dentry.clone();
                    let dentry = Some(nd.inode.lookup(&current_dir, current_dentry));
                    return dentry;
                })
                .unwrap();

            let inner = dentry.inner.lock();
            if let Some(inode) = inner.inode.as_ref() {
                nd.depth += 1;
                nd.dentry = dentry.clone();
                nd.inode = inode.clone();
            } else {
                return None;
            }
        }
    }
    Some(nd.inode.clone())
}
