pub const BOARD_NAME: &'static str = "QEMU Virt";
pub const PERIPHERALS_START: usize = 0x0800_0000;
pub const PERIPHERALS_END: usize = 0x1000_0000;
pub const CPU_NUM: usize = 4;

pub mod uart;