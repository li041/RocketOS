use super::{
    dentry::{insert_dentry, lookup_dcache_with_absolute_path, Dentry},
    inode::InodeOp,
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
    // 通过dentry可以找到inode
    // 注意Dentry和InodeOp的锁粒度都在他们自己的结构体内部
    dentry: Arc<Dentry>,
    // 当前处理到的路径
    depth: usize,
}

impl<'a> Nameidata<'a> {
    // 绝对路径dentry初始化为root, 相对路径则是cwd 
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
            assert!(!dentry.is_negative());
        }
        Nameidata {
            path_segments,
            dentry,
            depth: 0,
        }
    }
}

pub const O_CREAT: i32 = 0x100;

// Todo: 增加权限检查
/// 根据路径查找inode, 如果不存在, 则根据flags创建
pub fn path_lookup(path: &str, flags: i32) -> Result<Arc<dyn InodeOp>, isize> {
    let mut nd = Nameidata::new(path);
    let dentry = link_path_walk(&mut nd);
    // Todo: 根据flags设置mode
    let fake_mode = 0o777;

    if dentry.is_negative() {
        if flags & O_CREAT != 0 {
            // 创建文件
            let dir_inode = nd.dentry.get_inode();
            dir_inode.create(dentry.clone(), fake_mode);
            assert!(dentry.inner.lock().inode.is_some());
            return Ok(dentry.get_inode());
        } else {
            return Err(-1);
        }
    }
    // 找到了
    return Ok(dentry.get_inode());
}

// 注意: name可能为"."或"..", 在DentryCache中绝对路径不包括这两个特殊目录
/// 若找不到, 则返回负目录项, nd中的dentry和inode为父目录的
pub fn link_path_walk(nd: &mut Nameidata) -> Arc<Dentry> {
    log::info!("[link_path_walk] path: {:?}", nd.path_segments);
    let mut absolute_current_dir = nd.dentry.absolute_path.clone();
    while nd.depth < nd.path_segments.len() {
        if nd.path_segments[nd.depth] == "." {
            continue;
        } else if nd.path_segments[nd.depth] == ".." {
            let parent_dentry = nd.dentry.get_parent();
            nd.depth += 1;
            nd.dentry = parent_dentry;
        } else {
            // name是String
            absolute_current_dir = absolute_current_dir + "/" + nd.path_segments[nd.depth];

            log::info!("[link_path_walk] current_dir: {:?}", absolute_current_dir);
            // 查找DentryCache, 如果没有则从目录中查找
            // let dentry = lookup_dcache_with_absolute_path(&current_dir)
            //     .or_else(|| {
            //         Some(
            //             nd.dentry
            //                 .get_inode()
            //                 .lookup(&current_dir, nd.dentry.clone()),
            //         )
            //     })
            //     .unwrap();
            // Debug begin
            let mut dentry = lookup_dcache_with_absolute_path(&absolute_current_dir);
            if dentry.is_none() {
                // dentry = Some(
                //     nd.dentry
                //         .get_inode()
                //         .lookup(&current_dir, nd.dentry.clone()),
                // );
                // log::info!("current_dir: {:?}", nd.dentry.absolute_path);
                let current_dir_inode = nd.dentry.get_inode();
                // log::error!("get inode");
                // log::info!("{:?}", current_dir_inode.getdents());
                // current dir是ok的
                dentry =
                    Some(current_dir_inode.lookup(&nd.path_segments[nd.depth], nd.dentry.clone()));
            }
            let dentry = dentry.unwrap();
            // Debug end
            log::info!(
                "[link_path_walk] dentry: {:?}, is_negative: {}",
                dentry.absolute_path,
                dentry.is_negative()
            );
            if dentry.is_negative() {
                return dentry;
            }
            nd.depth += 1;
            nd.dentry = dentry;
        }
    }
    nd.dentry.clone()
}
