pub const MEMORY_OFFSET: usize = 0x4000_0000;
pub const KERNEL_OFFSET: usize = 0xffff_0000_0000_0000;
pub const PHYSICAL_MEMORY_OFFSET: usize = 0xffff_8000_0000_0000;
pub const KERNEL_HEAP_SIZE: usize = 64 * 1024 * 1024;

pub const ARCH: &str = "aarch64";
