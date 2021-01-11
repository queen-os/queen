mod boot;
#[cfg_attr(feature = "bsp_virt", path = "bsp/virt/mod.rs")]
pub mod bsp;
pub mod cpu;
pub mod memory;

#[no_mangle]
pub fn main_start() -> ! {
    memory::early_init();
    crate::kmain();
}

#[no_mangle]
pub fn others_start() -> ! {
    crate::kmain();
}
