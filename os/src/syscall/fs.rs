use crate::{
    ext4::{self, dentry, inode::S_IFDIR},
    fs::{
        namei::{filename_create, path_openat, Nameidata},
        pipe::Pipe,
    },
    mm::copy_to_user,
    task::current_task,
    utils::c_str_to_string,
};

pub fn sys_read(fd: usize, buf: *mut u8, len: usize) -> isize {
    let task = current_task();
    /* cannot use `inner` as MutexGuard will cross `await` that way */
    let fd_table_len = task.inner_handler(|inner| inner.fd_table.max_fd());
    if fd > fd_table_len {
        return -1;
    }
    // let file = task.inner_handler(|inner| inner.fd_table[fd].clone());
    let file = task.inner_handler(|inner| inner.fd_table.get_file(fd));
    if let Some(file) = file {
        let file = file.clone();
        if !file.readable() {
            return -1;
        }
        let ret = file.read(unsafe { core::slice::from_raw_parts_mut(buf, len) });
        log::info!("sys_read: fd: {}, len: {}, ret: {}", fd, len, ret);
        ret as isize
    } else {
        -1
    }
}

pub fn sys_write(fd: usize, buf: *const u8, len: usize) -> isize {
    let task = current_task();
    let fd_table_len = task.inner_handler(|inner| inner.fd_table.max_fd());
    if fd >= fd_table_len {
        return -1;
    }
    let file = task.inner_handler(|inner| inner.fd_table.get_file(fd));
    if let Some(file) = file {
        if !file.writable() {
            return -1;
        }
        let file = file.clone();
        let ret = file.write(unsafe { core::slice::from_raw_parts(buf as *const u8, len) });
        ret as isize
    } else {
        -1
    }
}

/// mode是直接传递给ext4_create, 由其处理(仅当O_CREAT设置时有效, 指定inode的权限)
/// flags影响文件的打开, 在flags中指定O_CREAT, 则创建文件
pub fn sys_openat(dirfd: i32, pathname: *const u8, flags: usize, mode: usize) -> isize {
    log::info!(
        "[sys_openat] dirfd: {}, pathname: {:?}, flags: {}, mode: {}",
        dirfd,
        pathname,
        flags,
        mode
    );
    let task = current_task();
    let path = c_str_to_string(pathname);
    if let Ok(file) = path_openat(&path, flags, dirfd, mode) {
        let fd = task.inner_handler(|inner| inner.alloc_fd(file));
        log::info!("[sys_openat] success to open file: {}, fd: {}", path, fd);
        return fd as isize;
    } else {
        log::info!("[sys_openat] fail to open file: {}", path);
        -1
    }
}

pub fn sys_mkdirat(dirfd: isize, pathname: *const u8, mode: usize) -> isize {
    log::info!(
        "[sys_mkdirat] dirfd: {}, pathname: {:?}, mode: {}",
        dirfd,
        pathname,
        mode
    );
    let path = c_str_to_string(pathname);
    let mut nd = Nameidata::new(&path, dirfd as i32);
    let fake_lookup_flags = 0;
    match filename_create(&mut nd, fake_lookup_flags) {
        Ok(dentry) => {
            let parent_inode = nd.dentry.get_inode();
            let parent_ino = nd.dentry.inode_num as u32;
            parent_inode.mkdir(parent_ino, dentry, mode as u16 | S_IFDIR);
            // Debug Ok
            // ext4_list_apps();
            return 0;
        }
        Err(e) => {
            log::info!("[sys_mkdirat] fail to create dir: {}, {}", path, e);
            -1
        }
    }
}

/// 由copy_to_user保证用户指针的合法性
pub fn sys_getcwd(buf: *mut u8, buf_size: usize) -> isize {
    // glibc getcwd(3) says that if buf is NULL, it will allocate a buffer
    // let cwd = current_task().inner.lock().cwd.clone();
    let cwd = current_task().inner_handler(|inner| inner.pwd.dentry.absolute_path.clone());
    let copy_len = cwd.len() + 1;
    if copy_len > buf_size {
        log::error!("getcwd: buffer is too small");
        // buf太小返回NULL
        return 0;
    }
    let from: *const u8 = cwd.as_bytes().as_ptr();
    if let Err(err) = copy_to_user(buf, from, copy_len) {
        log::error!("getcwd: copy_to_user failed: {}", err);
        return 0;
    }
    // 成功返回buf指针
    buf as isize
}

pub fn sys_pipe2(fdset: *const u8) -> isize {
    let task = current_task();
    let pipe_pair = Pipe::new_pair();
    let fdret = task.inner_handler(|inner| {
        // let fd1 = inner.alloc_fd();
        // inner.fd_table[fd1] = Some(pipe_pair.0.clone());
        // let fd2 = inner.alloc_fd();
        // inner.fd_table[fd2] = Some(pipe_pair.1.clone());
        let fd1 = inner.alloc_fd(pipe_pair.0.clone());
        let fd2 = inner.alloc_fd(pipe_pair.1.clone());
        (fd1, fd2)
    });
    /* the FUCKING user fd is `i32` type! */
    let fdret: [i32; 2] = [fdret.0 as i32, fdret.1 as i32];
    let fdset_ptr = fdset as *mut [i32; 2];
    unsafe {
        core::ptr::write(fdset_ptr, fdret);
    }
    0
}

pub fn sys_close(fd: usize) -> isize {
    log::info!("[sys_close] fd: {}", fd);
    let task = current_task();
    let fd_table_len = task.inner_handler(|inner| inner.fd_table.max_fd());
    if fd > fd_table_len {
        return -1;
    }
    return task.inner_handler(|inner| {
        if inner.fd_table.close(fd) {
            0
        } else {
            // fd not found
            -1
        }
    });
}
