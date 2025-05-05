/*
 * @Author: Peter/peterluck2021@163.com
 * @Date: 2025-04-16 21:36:51
 * @LastEditors: Peter/peterluck2021@163.com
 * @LastEditTime: 2025-04-16 21:40:09
 * @FilePath: /RocketOS /os/src/arch/mod.rs
 * @Description: 
 * 
 * Copyright (c) 2025 by peterluck2021@163.com, All Rights Reserved. 
 */
#[cfg(target_arch = "loongarch64")]
mod la64;
#[cfg(target_arch = "riscv64")]
mod riscv64;

#[cfg(target_arch = "loongarch64")]
pub use la64::*;

#[cfg(target_arch = "riscv64")]
pub use riscv64::*;
