#![no_std]
#![no_main]

extern crate user_lib;

use user_lib::{execve, fork, wait, yield_};

#[no_mangle]
fn main() -> i32 {
    if fork() == 0 {
        // execve("user_shell\0", &["user_shell\0"], &["\0"]);
        execve("submit_script\0", &["submit_script\0"], &["\0"]);
    } else {
        loop {
            let mut exit_code: i32 = 0;
            let pid = wait(&mut exit_code);
            if pid == -1 {
                yield_();
                continue;
            }
            // println!(
            //     "[initproc] Released a zombie process, pid={}, exit_code={}",
            //     pid, exit_code,
            // );
        }
    }
    0
}
