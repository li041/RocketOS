     .section .text.entry
     .globl _start
 _start:    
    # a0 = hart id 
    # pc = 0x8020_0000
    slli t0, a0, 16
    la sp, boot_stack_top
    sub sp, sp, t0

    # since the kernel base addr is 0xffff_ffc0_8020_0000
    # we need to activate pagetable here
    # satp: 8 << 60 | boot_pagetable
    la t0, boot_pagetable
    li t1, 8 << 60
    srli t0, t0, 12
    or t0, t0, t1
    csrw satp, t0
    sfence.vma

    la t0, fake_main      # 加载虚拟地址符号
    jr t0                 # 间接跳转，跳到 fake_main


    .section .bss.stack

    .globl boot_stack_lower_bound
boot_stack_lower_bound:

    .space 4096 * 16 * 4  # 4 CPUS at most

    .globl boot_stack_top
boot_stack_top:

    .section .data
    .align 12
boot_pagetable:
    # we need 2 pte here
    # 0x0000_0000_8000_0000 -> 0x0000_0000_8000_0000
    # 0xffff_fc00_8000_0000 -> 0x0000_0000_8000_0000
    .quad 0
    .quad 0
    .quad (0x80000 << 10) | 0xcf # VRWXAD
    .zero 8 * 255
    .quad (0x80000 << 10) | 0xcf # VRWXAD
    .zero 8 * 253