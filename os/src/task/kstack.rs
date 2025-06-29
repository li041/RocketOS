//! 用户的内核栈分配
use crate::{
    arch::config::PAGE_SIZE,
    fs::kstat,
    mm::{MapPermission, VPNRange, VirtAddr, KERNEL_SPACE},
};

use super::{
    id::{kid_alloc, kid_dealloc},
    wait,
};

pub const KSTACK_TOP: usize = 0xffff_ffff_ffff_f000;
pub const KSTACK_SIZE: usize = (PAGE_SIZE << 4) - PAGE_SIZE;
/// 内核栈的底部, 注意每个内核栈下面还有一个页用于保护

/// 使用struct KernelStack包装, 实现Drop特性, 实际上就是内核栈的sp
/// 注意在`__switch`中, 会通过tp寄存器的值修改栈顶指针
pub struct KernelStack(pub usize);

// Todo: 实现懒分配
// 当使用kstack_alloc时, 会给对应页加入映射, 返回栈顶指针
pub fn kstack_alloc() -> usize {
    let kstack_id = kid_alloc();
    let kstack_top = KSTACK_TOP - kstack_id * (KSTACK_SIZE + PAGE_SIZE);
    let kstack_bottom = kstack_top - KSTACK_SIZE;
    // log::trace!(
    //     "[kstack_alloc] kstack:\t[{:#x},{:#x})",
    //     kstack_top,
    //     kstack_bottom
    // );
    let vpn_range = VPNRange::new(
        VirtAddr::from(kstack_bottom).floor(),
        VirtAddr::from(kstack_top).ceil(),
    );
    KERNEL_SPACE
        .lock()
        .insert_framed_area(vpn_range, MapPermission::R | MapPermission::W, false);
    kstack_top
}

// drop KernelStack时, 取消相应内核栈的映射
impl Drop for KernelStack {
    fn drop(&mut self) {
        // let kstack_top = KSTACK_TOP - self.0 * (KSTACK_SIZE + PAGE_SIZE);
        let kstack_id = get_kstack_id(self.0);
        // kid_dealloc(kstack_id);
        let kstack_top = KSTACK_TOP - kstack_id * (KSTACK_SIZE + PAGE_SIZE);
        let kstack_bottom = kstack_top - KSTACK_SIZE;
        KERNEL_SPACE
            .lock()
            .remove_area_with_start_vpn(VirtAddr::from(kstack_bottom).floor());
        kid_dealloc(kstack_id);
    }
}

/// 计算内核栈编号
/// 参数 kstack_top: 当前sp寄存器的值（当前栈指针位置）
pub fn get_kstack_id(kstack_top: usize) -> usize {
    (KSTACK_TOP - kstack_top) / (KSTACK_SIZE + PAGE_SIZE)
}

/// 通过sp寄存器的值得到内核栈顶（高地址）
pub fn get_stack_top_by_sp(sp: usize) -> usize {
    let kstack_id = get_kstack_id(sp);
    let kstack_top = KSTACK_TOP - kstack_id * (KSTACK_SIZE + PAGE_SIZE);
    kstack_top
}

// pub fn check_init_task_kstack_overflow(stval: usize) {
//     if stval == 0 {
//         return;
//     }
//     let kstack_top = KSTACK_TOP;
//     let kstack_bottom = kstack_top - KSTACK_SIZE;
//     if stval < kstack_bottom {
//         panic!("Kernel stack overflow!");
//     }
// }
