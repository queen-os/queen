use core::{
    hint::spin_loop,
    sync::atomic::{AtomicBool, Ordering},
};

mod boot;
#[cfg_attr(feature = "bsp_virt", path = "bsp/virt/mod.rs")]
pub mod bsp;
pub mod consts;
pub mod cpu;
pub mod memory;
pub mod paging;

static AP_CAN_INIT: AtomicBool = AtomicBool::new(false);

#[no_mangle]
unsafe extern "C" fn main_start() -> ! {
    cpu::start_others();
    memory::init();
    crate::logging::init();
    println!("Hello {}! from CPU {}", bsp::BOARD_NAME, cpu::id());
    AP_CAN_INIT.store(true, Ordering::Release);
    crate::kmain();
}

#[no_mangle]
extern "C" fn others_start() -> ! {
    println!("Hello {}! from CPU {}", bsp::BOARD_NAME, cpu::id());
    while !AP_CAN_INIT.load(Ordering::Acquire) {
        spin_loop()
    }
    crate::kmain();
}
