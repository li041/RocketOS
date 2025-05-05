/*
 * @Author: Peter/peterluck2021@163.com
 * @Date: 2025-04-16 21:36:51
 * @LastEditors: Peter/peterluck2021@163.com
 * @LastEditTime: 2025-05-24 16:54:12
 * @FilePath: /RocketOS_netperfright/os/src/arch/la64/register/mod.rs
 * @Description: 
 * 
 * Copyright (c) 2025 by peterluck2021@163.com, All Rights Reserved. 
 */
#[macro_use]
mod macros;
mod base;
mod mmu;
mod ras;
mod timer;

pub use base::{
    badi::*, badv::*, cpuid::*, crmd::*, ecfg::*, eentry::*, era::*, estat::*, euen::*, llbctl::*,
    misc::*, prcfg::*, prmd::*, rvacfg::*,
};
pub use mmu::{
    asid::*, dmw::*, pgd::*, pwch::*, pwcl::*, stlbps::*, tlbehi::*, tlbelo, tlbidx::*,
    tlbrbadv::*, tlbrehi::*, tlbrelo, tlbrentry::*, tlbrera::*, tlbrprmd::*, tlbrsave::*,
    MemoryAccessType,
};
pub use ras::{merrctl::*, merrentry::*, merrera::*, merrinfo::*, merrsave::*};
pub use timer::{cntc::*, tcfg::*, ticlr::*, tid::*, tval::*};
