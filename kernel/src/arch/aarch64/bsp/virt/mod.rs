pub const BOARD_NAME: &str = "QEMU Virt";
pub const PERIPHERALS_START: usize = 0x0800_0000;
pub const PERIPHERALS_END: usize = 0x1000_0000;
pub const MEMORY_START: usize = 0x4000_0000;
pub const MEMORY_END: usize = 0x8000_0000;
pub const CPU_NUM: usize = 4;
pub const BOOT_CORE_ID: usize = 0;
pub const DEVICE_TREE_ADDR: usize = 0x4000_0000;

pub mod timer;
pub mod uart;
