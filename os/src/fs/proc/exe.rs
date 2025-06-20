use core::{default, str};

use lazy_static::lazy_static;
use spin::{lazy, mutex, Once, RwLock};

use crate::{
    ext4::inode::Ext4InodeDisk,
    fs::{
        file::{FileOp, OpenFlags},
        inode::InodeOp,
        kstat::Kstat,
        path::Path,
        uapi::Whence,
        FileOld,
    },
    syscall::errno::SyscallRet,
    task::current_task,
    timer::TimeSpec,
};

use alloc::{
    string::{String, ToString},
    sync::Arc,
};

pub static EXE: Once<Arc<dyn FileOp>> = Once::new();

pub struct ExeInode {
    pub inner: RwLock<ExeInodeInner>,
}
pub struct ExeInodeInner {
    pub inode_on_disk: Ext4InodeDisk,
}

impl ExeInode {
    pub fn new(inode_on_disk: Ext4InodeDisk) -> Arc<Self> {
        Arc::new(ExeInode {
            inner: RwLock::new(ExeInodeInner { inode_on_disk }),
        })
    }
}

impl InodeOp for ExeInode {
    fn get_link(&self) -> String {
        let mut exe_path = current_task().exe_path();
        assert!(
            exe_path.starts_with('/'),
            "ExeInode::get_link: exe_path is not absolute: {}",
            exe_path
        );
        exe_path
        // log::warn!(
        //     "ExeInode::get_link: exe_path is {}, current task is {}",
        //     exe_path,
        //     current_task().tid()
        // );
        // if !exe_path.starts_with('/') {
        //     // 如果是相对路径, 则转换为绝对路径
        //     let cwd = current_task().pwd().dentry.absolute_path.clone();
        //     // 将"."替换为cwd
        //     if exe_path.starts_with(".") {
        //         exe_path.replace_range(0..1, &cwd.to_string());
        //     } else {
        //         exe_path = cwd.to_string() + "/" + &exe_path;
        //     }
        //     log::warn!(
        //         "ExeInode::get_link: exe_path is relative, replaced with cwd, return {}",
        //         exe_path
        //     );
        //     return exe_path;
        // } else {
        //     // 否则直接返回
        //     log::warn!(
        //         "ExeInode::get_link: exe_path is absolute, return {}",
        //         exe_path
        //     );
        //     return exe_path;
        // }
    }
    fn getattr(&self) -> Kstat {
        let mut kstat = Kstat::new();
        let inner_guard = self.inner.read();
        let inode_on_disk = &inner_guard.inode_on_disk;

        kstat.mode = inode_on_disk.get_mode();
        kstat.uid = inode_on_disk.get_uid() as u32;
        kstat.gid = inode_on_disk.get_gid() as u32;
        kstat.nlink = inode_on_disk.get_nlinks() as u32;
        kstat.size = inode_on_disk.get_size();

        // Todo: 目前没有更新时间戳
        kstat.atime = inode_on_disk.get_atime();
        kstat.mtime = inode_on_disk.get_mtime();
        kstat.ctime = inode_on_disk.get_ctime();
        // Todo: 创建时间
        // kstat.btime = TimeSpec {
        //     sec: inode_on_disk.create_time as usize,
        //     nsec: (inode_on_disk.create_time_extra >> 2) as usize,
        // };
        // Todo: Direct I/O 对齐参数
        // inode版本号
        kstat.change_cookie = inode_on_disk.generation as u64;

        kstat
    }
    fn get_resident_page_count(&self) -> usize {
        0
    }

    /* get/set属性方法 */
    // Todo
    fn get_mode(&self) -> u16 {
        self.inner.read().inode_on_disk.get_mode()
    }
    /* 时间戳 */
    fn get_atime(&self) -> TimeSpec {
        self.inner.read().inode_on_disk.get_atime()
    }
    fn set_atime(&self, atime: TimeSpec) {
        self.inner.write().inode_on_disk.set_atime(atime);
    }
    fn get_mtime(&self) -> TimeSpec {
        self.inner.read().inode_on_disk.get_mtime()
    }
    fn set_mtime(&self, mtime: TimeSpec) {
        self.inner.write().inode_on_disk.set_mtime(mtime);
    }
    fn get_ctime(&self) -> TimeSpec {
        self.inner.read().inode_on_disk.get_ctime()
    }
    fn set_ctime(&self, ctime: TimeSpec) {
        self.inner.write().inode_on_disk.set_ctime(ctime);
    }
    fn set_mode(&self, mode: u16) {
        self.inner.write().inode_on_disk.set_mode(mode);
    }
}

pub struct ExeFile {
    pub path: Arc<Path>,
    pub inode: Arc<dyn InodeOp>,
    pub flags: OpenFlags,
}

impl ExeFile {
    pub fn new(path: Arc<Path>, inode: Arc<dyn InodeOp>, flags: OpenFlags) -> Arc<Self> {
        Arc::new(ExeFile { path, inode, flags })
    }
}

impl FileOp for ExeFile {
    fn as_any(&self) -> &dyn core::any::Any {
        self
    }
    fn get_inode(&self) -> Arc<dyn InodeOp> {
        self.inode.clone()
    }
    fn get_flags(&self) -> OpenFlags {
        self.flags
    }
}
