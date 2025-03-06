pub mod aux;
mod context;
mod id;
mod kstack;
mod processor;
mod scheduler;
mod switch;
mod task;

use crate::{
    drivers::BLOCK_DEVICE,
    fs::{
        fdtable::FdTable, file::FileOp, mount::do_ext4_mount, namei::path_openat, path::Path,
        AT_FDCWD,
    },
    loader::get_app_data_by_name,
    mutex::SpinNoIrqLock,
    sbi::shutdown,
    timer::get_time_ms,
    trap::TrapContext,
    utils::{c_str_to_string, extract_cstrings},
};
use alloc::{string::String, vec};
use alloc::{
    sync::Arc,
};

use core::arch::asm;
use lazy_static::lazy_static;
use task::{TaskStatus,Task};

pub use context::TaskContext;
pub use processor::{current_task, run_tasks};
pub use scheduler::{add_task, yield_current_task, WaitOption};

pub type Tid = usize;

lazy_static! {
    /// 初始进程
    pub static ref INITPROC: Arc<Task> = Task::initproc(get_app_data_by_name("initproc").unwrap());
}



