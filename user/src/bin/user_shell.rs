#![no_std]
#![no_main]
#![allow(clippy::println_empty_string)]

extern crate alloc;

#[macro_use]
extern crate user_lib;

mod shell;

const LF: u8 = 0x0au8;
const CR: u8 = 0x0du8;
const DL: u8 = 0x7fu8;
const BS: u8 = 0x08u8;

const THEME_COLOR: &str = "\u{1B}[38;5;14m";
const RESET_COLOR: &str = "\u{1B}[0m";

use alloc::string::String;
use alloc::vec::Vec;
use shell::command::Command;
use user_lib::console::getchar;
use user_lib::{execve, fork, waitpid};

fn print_prompt() {
    print!("{}RROS>> {}", THEME_COLOR, RESET_COLOR);
}

#[no_mangle]
pub fn main() -> i32 {
    let mut line: String = String::new();
    let mut history: Vec<String> = Vec::new(); // 存储历史命令
    let mut history_index: usize = 0; // 当前显示的历史命令索引
    print_prompt();

    loop {
        let c = getchar();
        match c {
            LF | CR => {
                println!("");
                if !line.is_empty() {
                    // 存入历史
                    history.push(line.clone());
                    history_index = history.len(); // 重置索引到最新

                    // 执行命令
                    let cmd = Command::from(line.as_str());
                    let pid = fork();
                    if pid == 0 {
                        cmd.exec();
                    } else {
                        let mut exit_code: i32 = 0;
                        let exit_pid = waitpid(pid as usize, &mut exit_code);
                        println!("pid: {}, exit_pid: {}", pid, exit_pid);
                        assert_eq!(pid, exit_pid);
                        println!("Shell: Process {} exited with code {}", pid, exit_code);
                    }
                    line.clear();
                }
                print_prompt();
            }
            BS | DL => {
                if !line.is_empty() {
                    print!("{}", BS as char);
                    print!(" ");
                    print!("{}", BS as char);
                    line.pop();
                }
            }
            // 处理方向键（上键 `ESC [ A`，下键 `ESC [ B`）
            0x1B => {
                // 检查是否是方向键（`ESC [ A` 或 `ESC [ B`）
                let next_c = getchar();
                if next_c == 0x5B {
                    // '['
                    match getchar() {
                        0x41 => {
                            // 上键 'A'
                            if history_index > 0 {
                                history_index -= 1;
                                // 清除当前行并替换为历史命令
                                print!("\x1B[2K\r"); // ANSI 清行
                                print_prompt();
                                line = history[history_index].clone();
                                print!("{}", line);
                            }
                        }
                        0x42 => {
                            // 下键 'B'
                            if history_index < history.len() {
                                history_index += 1;
                                print!("\x1B[2K\r"); // ANSI 清行
                                print_prompt();
                                if history_index < history.len() {
                                    line = history[history_index].clone();
                                } else {
                                    line.clear();
                                }
                                print!("{}", line);
                            }
                        }
                        _ => {} // 其他键忽略
                    }
                }
            }
            _ => {
                print!("{}", c as char);
                line.push(c as char);
            }
        }
    }
}
