mod boot;
#[cfg_attr(feature = "bsp_virt", path = "bsp/virt/mod.rs")]
pub mod bsp;
pub mod cpu;
pub mod memory;

#[no_mangle]
unsafe extern "C" fn main_start() -> ! {
    memory::early_init();
    crate::logging::init();
    crate::kmain();
}


#[no_mangle]
extern "C" fn others_start() -> ! {
    crate::kmain();
}
