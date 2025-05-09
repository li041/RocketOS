pub mod aux;
mod context;
mod id;
mod kstack;
mod processor;
mod scheduler;
mod task;
mod manager;
mod signal;

use crate::{
    arch::trap::TrapContext,
    drivers::BLOCK_DEVICE,
    fs::{
        fdtable::FdTable, file::FileOp, mount::do_ext4_mount, namei::path_openat, path::Path,
        AT_FDCWD,
    },
    loader::get_app_data_by_name,
    mutex::SpinNoIrqLock,
    utils::{c_str_to_string, extract_cstrings},
};
use alloc::sync::Arc;
use alloc::{string::String, vec};

use core::arch::asm;
use lazy_static::lazy_static;

pub use task::{Task,INIT_PROC_PID};
pub use context::TaskContext;
pub use processor::{current_task, run_tasks};
pub use scheduler::{add_task, remove_task, switch_to_next_task, yield_current_task, WaitOption};
pub use task::kernel_exit;
pub use kstack::get_stack_top_by_sp;
pub use manager::TASK_MANAGER;
pub use task::CloneFlags;

pub type Tid = usize;

lazy_static! {
    /// 初始进程
    pub static ref INITPROC: Arc<Task> = Task::initproc(get_app_data_by_name("initproc").unwrap(), do_ext4_mount(BLOCK_DEVICE.clone()));
}

#[cfg(target_arch = "riscv64")]
#[cfg(target_arch = "riscv64")]
pub fn add_initproc() {
    // 设置tp寄存器指向INITPROC
    let initproc_tp = Arc::as_ptr(&INITPROC) as usize;
    unsafe {
        asm!("mv tp, {}", in(reg) initproc_tp);
    }
}

#[cfg(target_arch = "loongarch64")]
// 设置tp寄存器指向INITPROC
pub fn add_initproc() {
    log::error!("add_initproc");
    let initproc_tp = Arc::as_ptr(&INITPROC) as usize;
    log::error!("initproc_tp: {:#x}", initproc_tp);
    unsafe {
        asm!("addi.d $r2, {}, 0", in(reg) initproc_tp);
    }
}
