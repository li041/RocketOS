OUTPUT_ARCH(riscv)
ENTRY(_start)

BASE_ADDRESS = 0x10000;

SECTIONS
{
    . = BASE_ADDRESS;

    /* 代码和只读数据 */
    .text : {
        *(.text.entry)
        *(.text .text.*)
    }
    . = ALIGN(4K);
    .rodata : {
        *(.rodata .rodata.*)
        *(.srodata .srodata.*)
    }
    . = ALIGN(4K);

    /* 可读写数据 */
    sdata = .;
    .data : {
        *(.data .data.*)
        *(.sdata .sdata.*)
    }
    . = ALIGN(4K);
    
    /* 普通 BSS */
    .bss : {
        *(.bss .bss.*)
        *(.sbss .sbss.*)
    }

    /* 弃用调试和异常表 */
    /DISCARD/ : {
        *(.eh_frame)
        *(.debug*)
    }
}
