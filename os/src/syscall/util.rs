use core::{cell::UnsafeCell, time};

use spin::Mutex;

use super::errno::SyscallRet;
use crate::{
    arch::{
        mm::{copy_from_user, copy_to_user},
        sbi::shutdown,
    },
    fs::{
        file::OpenFlags,
        namei::path_openat,
        uapi::{RLimit, Resource},
    },
    syscall::errno::Errno,
    task::{
        add_real_timer, current_task, get_task, remove_timer, rusage::RUsage, update_real_timer,
        ITIMER_PROF, ITIMER_REAL, ITIMER_VIRTUAL,
    },
    time::{config::ClockIdFlags, do_adjtimex, KernelTimex, LAST_TIMEX},
    timer::{ITimerVal, TimeSpec, TimeVal},
};
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Utsname {
    /// 系统名称
    pub sysname: [u8; 65],
    /// 网络上主机名称 from etc/hostname
    pub nodename: [u8; 65],
    /// 发行编号
    pub release: [u8; 65],
    /// 版本
    pub version: [u8; 65],
    /// 域名
    pub machine: [u8; 65],
    ///domainname
    pub domainname: [u8; 65],
}

impl Default for Utsname {
    fn default() -> Self {
        Self {
            sysname: Self::from_str("RocketOS"),
            nodename: Self::from_str("LAPTOP"),
            release: Self::from_str("5.15.146.1-standard"),
            version: Self::from_str("#1 SMP Thu Jan"),
            machine: Self::from_str("RISC-V SiFive Freedom U740 SoC"),
            domainname: Self::from_str("SHY"),
        }
    }
}

impl Utsname {
    fn from_str(info: &str) -> [u8; 65] {
        let mut data: [u8; 65] = [0; 65];
        data[..info.len()].copy_from_slice(info.as_bytes());
        data
    }
    pub fn set_nodename(&mut self, nodename: &[u8]) {
        let len = core::cmp::min(nodename.len(), 64);
        self.nodename = [0u8; 65];
        self.nodename[..len].copy_from_slice(&nodename[..len]);
    }
    pub fn set_domainname(&mut self, domainname: &[u8]) {
        let len = core::cmp::min(domainname.len(), 64);
        self.domainname = [0u8; 65];
        self.domainname[..len].copy_from_slice(&domainname[..len]);
    }
}

/// sys_times, 单位都是us
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Tms {
    /// CPU time spent executing instructions of the calling process
    pub utime: usize,
    /// inside the kernel
    pub stime: usize,
    /// the sum of the utime for all waited-for teminated children
    pub cutime: usize,
    /// the sum of stime for all waited-for teminated children
    pub cstime: usize,
}

impl Default for Tms {
    fn default() -> Self {
        Self {
            utime: 1,
            stime: 1,
            cutime: 1,
            cstime: 1,
        }
    }
}

/// fake uname  
///
/// Todo?:
pub fn sys_uname(uts: usize) -> SyscallRet {
    log::info!("[sys_uname] uts: {:#x}", uts);
    let uts = uts as *mut Utsname;
    //Todo!: check validarity
    let mut utsname = Utsname::default();
    //todo:还差其他的
    let hostnamefile = path_openat("/etc/hostname", OpenFlags::O_CLOEXEC, -100, 0)?;
    let nodename = hostnamefile.read_all();
    log::error!("[sys_uname] nodename is {:?}", nodename);
    if nodename.len() > 0 {
        utsname.set_nodename(nodename.as_slice());
    }
    let domainnamefile = path_openat("/etc/domainname", OpenFlags::O_CLOEXEC, -100, 0)?;
    let domainname = domainnamefile.read_all();
    log::error!("[sys_uname] domainname is {:?}", domainname);
    if domainname.len() > 0 {
        utsname.set_domainname(domainname.as_slice());
    }
    copy_to_user(uts, &utsname as *const Utsname, 1).unwrap();
    Ok(0)
}

/// fake sys_times
/// Todo?:
#[allow(unused)]
pub fn sys_times(buf: usize) -> SyscallRet {
    let buf = buf as *mut Tms;
    let tms = Tms::default();
    // unsafe {
    //     core::ptr::write(buf, tms);
    // }
    copy_to_user(buf, &tms as *const Tms, 1).unwrap();
    Ok(0)
}

#[derive(Debug, Clone, Copy, Default)]
pub enum SyslogAction {
    CLOSE = 0,
    OPEN = 1,
    READ = 2,
    READ_ALL = 3,
    READ_CLEAR = 4,
    CLEAR = 5,
    CONSOLE_OFF = 6,
    CONSOLE_ON = 7,
    CONSOLE_LEVEL = 8,
    SIZE_UNREAD = 9,
    SIZE_BUFFER = 10,
    #[default]
    ILLEAGAL,
}

impl TryFrom<usize> for SyslogAction {
    type Error = ();

    fn try_from(value: usize) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(SyslogAction::CLOSE),
            1 => Ok(SyslogAction::OPEN),
            2 => Ok(SyslogAction::READ),
            3 => Ok(SyslogAction::READ_ALL),
            4 => Ok(SyslogAction::READ_CLEAR),
            5 => Ok(SyslogAction::CLEAR),
            6 => Ok(SyslogAction::CONSOLE_OFF),
            7 => Ok(SyslogAction::CONSOLE_ON),
            8 => Ok(SyslogAction::CONSOLE_LEVEL),
            9 => Ok(SyslogAction::SIZE_UNREAD),
            10 => Ok(SyslogAction::SIZE_BUFFER),
            _ => Err(()),
        }
    }
}

pub fn sys_syslog(log_type: usize, buf: *mut u8, len: usize) -> SyscallRet {
    const LOG_BUF_LEN: usize = 4096;
    const LOG: &str = "<5>[    0.000000] Linux version 5.10.102.1-microsoft-standard-WSL2 (rtrt@TEAM-NPUCORE) (gcc (Ubuntu 9.4.0-1ubuntu1~20.04) 9.4.0, GNU ld (GNU Binutils for Ubuntu) 2.34) #1 SMP Thu Mar 10 13:31:47 CST 2022";
    // let token = current_user_token();
    let log_type = SyslogAction::try_from(log_type).unwrap();
    let log = LOG.as_bytes();
    let len = LOG.len().min(len as usize);
    match log_type {
        SyslogAction::CLOSE | SyslogAction::OPEN => Ok(0),
        SyslogAction::READ => copy_to_user(buf, log.as_ptr(), len),
        SyslogAction::READ_ALL => copy_to_user(buf, log.as_ptr(), len),
        SyslogAction::READ_CLEAR => todo!(),
        SyslogAction::CLEAR => todo!(),
        SyslogAction::CONSOLE_OFF => todo!(),
        SyslogAction::CONSOLE_ON => todo!(),
        SyslogAction::CONSOLE_LEVEL => todo!(),
        SyslogAction::SIZE_UNREAD => todo!(),
        SyslogAction::SIZE_BUFFER => Ok(LOG_BUF_LEN),
        SyslogAction::ILLEAGAL => return Err(Errno::EINVAL),
    }
}

// Todo: 检查当前进程是否有权限修改其他进程的rlimit, 检查是否有权限修改硬限制
pub fn sys_prlimit64(
    pid: usize,
    resource: i32,
    new_limit: *const RLimit,
    old_limit: *mut RLimit,
) -> SyscallRet {
    // 根据tid获取操作的进程
    let task = if pid == 0 {
        current_task()
    } else {
        get_task(pid).expect("[sys_prlimit64]: invalid pid")
    };
    let resource = Resource::try_from(resource).unwrap();
    log::error!(
        "resource: {:?}, new_limit: {:#x}, old_limit: {:#x}",
        resource,
        new_limit as usize,
        old_limit as usize
    );
    // 如果old_limit不为NULL, 则将当前的rlimit写入old_limit
    if !old_limit.is_null() {
        let old_rlimit = task
            .get_rlimit(resource)
            .expect("[sys_prlimit64] get rlimit failed");
        // 这里需要copy_to_user
        copy_to_user(old_limit, &old_rlimit as *const RLimit, 1).unwrap();
    }
    // 如果new_limit不为NULL, 则将new_limit写入当前的rlimit
    if !new_limit.is_null() {
        let mut limit_buf = RLimit::default();
        copy_from_user(new_limit, &mut limit_buf as *mut RLimit, 1)?;
        return task.set_rlimit(resource, &limit_buf);
    }
    Ok(0)
}

// clockid
pub const SUPPORT_CLOCK: usize = 2;
/// 一个可设置的系统级实时时钟，用于测量真实（即墙上时钟）时间
pub const CLOCK_REALTIME: usize = 0;
/// 一个不可设置的系统级时钟，代表自某个未指定的过去时间点以来的单调时间
pub const CLOCK_MONOTONIC: usize = 1;
/// 用于测量调用进程消耗的CPU时间
pub const CLOCK_PROCESS_CPUTIME_ID: usize = 2;
/// 用于测量调用线程消耗的CPU时间
pub const CLOCK_THREAD_CPUTIME_ID: usize = 3;
/// 一个不可设置的系统级时钟，代表自某个未指定的过去时间点以来的单调时间
pub const CLOCK_MONOTONIC_RAW: usize = 4;
/// 一个不可设置的系统级实时时钟，用于测量真实（即墙上时钟）时间
pub const CLOCK_REALTIME_COARSE: usize = 5;
pub fn sys_clock_gettime(clock_id: usize, timespec: *mut TimeSpec) -> SyscallRet {
    //如果tp是NULL, 函数不会存储时间值, 但仍然会执行其他检查（如 `clockid` 是否有效）。
    if timespec.is_null() {
        return Ok(0);
    }
    // log::error!(
    //     "[sys_clock_gettime] clock_id is {:?},timespec {:?}",
    //     clock_id,
    //     timespec
    // );
    match clock_id {
        CLOCK_REALTIME | CLOCK_REALTIME_COARSE => {
            let time = TimeSpec::new_wall_time();
            // log::info!("[sys_clock_gettime] CLOCK_REALTIME: {:?}", time);
            copy_to_user(timespec, &time as *const TimeSpec, 1)?;
        }
        CLOCK_MONOTONIC | CLOCK_MONOTONIC_RAW => {
            let time = TimeSpec::new_machine_time();
            // log::info!("[sys_clock_gettime] CLOCK_MONOTONIC: {:?}", time);
            copy_to_user(timespec, &time as *const TimeSpec, 1)?;
        }
        CLOCK_PROCESS_CPUTIME_ID => {
            // let time = TimeSpec::new_process_time();
            let task = current_task();
            let (utime, stime) = task.process_us_time();
            let time = TimeSpec::from(utime + stime);
            // log::info!("[sys_clock_gettime] CLOCK_PROCESS_CPUTIME_ID: {:?}", time);
            copy_to_user(timespec, &time as *const TimeSpec, 1)?;
        }
        _ => {
            log::error!("[sys_clock_gettime] Unsupported clock_id: {}", clock_id);
            return Err(Errno::EINVAL);
        }
    }
    Ok(0)
}
pub fn sys_clock_settime(clock_id: usize, timespec: *const TimeSpec) -> SyscallRet {
    if timespec.is_null() {
        return Ok(0);
    }
    log::error!("[sys_clock_settime] clock_id is {:?}", clock_id);
    let mut time = TimeSpec::default();
    copy_from_user(timespec, &mut time as *mut TimeSpec, 1)?;
    match clock_id {
        CLOCK_REALTIME | CLOCK_REALTIME_COARSE => {
            return Ok(0);
        }
        CLOCK_MONOTONIC | CLOCK_MONOTONIC_RAW => {
            return Ok(0);
        }
        CLOCK_PROCESS_CPUTIME_ID => {
            return Ok(0);
        }
        _ => {
            // panic!("[sys_clock_gettime] invalid clock_id: {}", clock_id);
            return Err(Errno::EINVAL);
        }
    }
    Ok(0)
}

pub fn sys_setitimer(
    which: i32,
    value_ptr: *const ITimerVal,
    ovalue_ptr: *mut ITimerVal,
) -> SyscallRet {
    if which > 2 {
        return Err(Errno::EINVAL);
    }
    let mut new = ITimerVal::default();
    copy_from_user(value_ptr, &mut new as *mut ITimerVal, 1)?;
    if !new.is_valid() {
        return Err(Errno::EINVAL);
    }
    log::info!(
        "[sys_setitimer] which: {}, it_value: {:?}, it_interval: {:?}",
        which,
        new.it_value,
        new.it_interval
    );
    match which {
        ITIMER_REAL => {
            let task = current_task();
            // 启用定时器
            let (should_update, old) = task.op_itimerval_mut(|itimerval| {
                let real_itimeval = &mut itimerval[which as usize];
                let should_update =
                    !real_itimeval.it_value.is_zero() || !real_itimeval.it_interval.is_zero();
                // 计算旧的定时器值(it_value)剩余时间
                let current_time = TimeVal::new_wall_time();
                let old_it_value = if real_itimeval.it_value < current_time {
                    TimeVal { sec: 0, usec: 0 }
                } else {
                    real_itimeval.it_value - current_time
                };
                let old = ITimerVal {
                    it_value: old_it_value,
                    it_interval: real_itimeval.it_interval,
                };
                log::warn!("old_value: {:?}", old);
                // 设定新的定时器值
                real_itimeval.it_value = current_time + new.it_value;
                real_itimeval.it_interval = new.it_interval;
                (should_update, old)
            });
            // 将旧的定时器值写入ovalue_ptr
            if !ovalue_ptr.is_null() {
                copy_to_user(ovalue_ptr, &old as *const ITimerVal, 1)
                    .expect("[sys_setitimer] copy_to_user failed");
            }
            // 禁用定时器
            if new.it_value.is_zero() {
                log::info!("[sys_setitimer] disable timer");
                remove_timer(task.tid(), ITIMER_REAL);
                return Ok(0);
            }
            // 设置或更新已有定时器
            if should_update {
                log::warn!("[sys_setitimer] update timer");
                update_real_timer(task.tid(), new.it_value.into());
            } else {
                add_real_timer(task.tid(), new.it_value.into());
            }
            return Ok(0);
        }
        ITIMER_VIRTUAL => {
            unimplemented!();
        }
        ITIMER_PROF => {
            unimplemented!();
        }
        _ => {
            // 已进行参数检查, 不会进入这里
            panic!("[sys_setitimer] invalid which: {}", which);
        }
    };
}
/// 调用进程的资源使用情况。
pub const RUSAGE_SELF: i32 = 0;
/// 已终止并被等待的所有子进程的资源使用情况
pub const RUSAGE_CHILDREN: i32 = -1;
/// 调用线程的资源使用情况（需要 Linux 2.6.26 以上版本，并定义了 `_GNU_SOURCE` 宏）
pub const RUSAGE_THREAD: i32 = 1;
pub fn sys_getrusage(who: i32, rusage: *mut RUsage) -> SyscallRet {
    if rusage.is_null() {
        return Err(Errno::EINVAL);
    }
    let task = current_task();
    let mut usage = RUsage::default();
    match who {
        RUSAGE_SELF => {
            let (utime, stime) = task.process_us_time();
            usage.utime = utime;
            usage.stime = stime;
        }
        RUSAGE_CHILDREN => {
            unimplemented!();
        }
        RUSAGE_THREAD => {
            let (utime, stime) = task.time_stat().thread_us_time();
            usage.utime = utime;
            usage.stime = stime;
        }
        _ => {
            return Err(Errno::EINVAL);
        }
    }
    copy_to_user(rusage, &usage as *const RUsage, 1).expect("[sys_getrusage] copy_to_user failed");
    Ok(0)
}

/*
   函数 clock_getres() 用于查找指定时钟 clockid 的分辨率（精度）
   如果 res 非空，则将其存储在 res 指向的 timespec 结构体中。
   如果 clock_settime() 的参数 tp 指向的时间值不是 res 的倍数，则将其截断为 res 的倍数。（Todo)
*/
pub fn sys_clock_getres(_clockid: usize, res: usize) -> SyscallRet {
    if res == 0 {
        return Ok(0);
    }
    log::info!("[sys_clock_getres] res set 1 nanos");
    copy_to_user(res as *mut TimeSpec, &TimeSpec::from_nanos(1), 1)?;
    Ok(0)
}

/// 调用进程的系统时间调整
/// 根据传入的kernelTimex调整时间并返回最新的内核结构体到指针
pub fn sys_adjtimex(user_timex: *mut KernelTimex) -> SyscallRet {
    log::error!("[sys_adjtimex] user_timex: {:#x}", user_timex as usize);
    if user_timex.is_null() {
        return Err(Errno::EFAULT);
    }
    if user_timex as usize == 0xffffffffffffffff {
        return Err(Errno::EFAULT);
    }
    let task = current_task();
    log::error!("[sys_adjtimex] task uid: {:?}", task.euid());
    let mut kernel_timex = KernelTimex::default();
    copy_from_user(
        user_timex as *const u8,
        &mut kernel_timex as *mut KernelTimex as *mut u8,
        size_of::<KernelTimex>(),
    )?;
    log::error!("[sys_adjtimex] kernel_timex: {:?}", kernel_timex);
    log::error!(
        "[sys_adjtimex] kernel_timex modes: {:?}",
        kernel_timex.modes
    );
    if kernel_timex.modes == 0x8000 {
        return Err(Errno::EINVAL);
    }
    if kernel_timex.modes == 0 {
        //只读
        unsafe {
            log::error!("[sys_adjtimex] last_timex is {:?}", LAST_TIMEX);
        }
        unsafe {
            copy_to_user(user_timex, &LAST_TIMEX as *const KernelTimex, 1)?;
        }
        return Ok(0);
    }
    //非只读模式下必须root权限
    if kernel_timex.modes != 0 && task.euid() != 0 {
        return Err(Errno::EPERM);
    }
    //保存非0设置的kernel_timex并在只读中返回回去
    let status = do_adjtimex(&mut kernel_timex)?;
    kernel_timex.tick = 10000;
    //写回到last_timex
    unsafe { LAST_TIMEX.clone_from(&kernel_timex) };
    unsafe {
        log::error!("[sys_adjtimex] last_timex is {:?}", LAST_TIMEX);
    }
    let out_from = &kernel_timex as *const KernelTimex;
    copy_to_user(user_timex, out_from, 1)?;
    Ok(status as usize)
}
//较adjtimex,可以选择调整哪个时钟
pub fn sys_clock_adjtime(clock_id: i32, user_timex: *mut KernelTimex) -> SyscallRet {
    log::error!(
        "[sys_clock_adjtime] clock_id: {}, user_timex: {:#x}",
        clock_id,
        user_timex as usize
    );
    if user_timex.is_null() {
        return Err(Errno::EINVAL);
    }
    let task = current_task();
    log::error!(
        "[sys_clock_adjtime] euid {:?},egid {:?}",
        task.euid(),
        task.egid()
    );
    let clock_type = ClockIdFlags::from_clockid(clock_id)?;
    if clock_type.contains(ClockIdFlags::REALTIME) {
        // 调整实时时钟
        return sys_adjtimex(user_timex);
    } else if clock_type.contains(ClockIdFlags::MONOTONIC) {
        // 调整单调时钟
        unimplemented!()
    } else {
        unimplemented!()
    }
}

pub fn sys_shutdown() -> SyscallRet {
    shutdown(false);
}
