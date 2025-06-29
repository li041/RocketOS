pub const CLOCK_FREQ: usize = 12500000;
pub const MEMORY_END: usize = 0x10000_0000;

pub const MMIO: &[(usize, usize)] = &[
    (0x0010_0000, 0x00_2000), // VIRT_TEST/RTC  in virt machine
    (0x1000_2000, 0x00_1000), // Virtio Block in virt machine
    (0x1010_0000, 0x00_0024), // Goldfish RTC
];
