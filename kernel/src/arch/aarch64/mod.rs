use core::{
    hint::spin_loop,
    sync::atomic::{AtomicBool, Ordering},
};

use crate::drivers;

mod boot;
#[cfg_attr(feature = "bsp_virt", path = "bsp/virt/mod.rs")]
pub mod bsp;
pub mod consts;
pub mod cpu;
pub mod interrupt;
pub mod memory;
pub mod paging;

static AP_CAN_INIT: AtomicBool = AtomicBool::new(false);

#[no_mangle]
unsafe extern "C" fn main_start() -> ! {
    crate::logging::init();
    let device_tree = drivers::device_tree::DeviceTree::new(bsp::DEVICE_TREE_ADDR).unwrap();
    cpu::start_others();
    memory::init(memory::MemInitOpts::new(
        device_tree.probe_memory().unwrap(),
    ));
    interrupt::init(device_tree);
    println!("Hello {}! from CPU {}", bsp::BOARD_NAME, cpu::id());
    AP_CAN_INIT.store(true, Ordering::Release);
    crate::kmain();
}

#[no_mangle]
unsafe extern "C" fn others_start() -> ! {
    println!("Hello {}! from CPU {}", bsp::BOARD_NAME, cpu::id());
    while !AP_CAN_INIT.load(Ordering::Acquire) {
        spin_loop()
    }
    memory::init_other();
    // interrupt::init_other();
    crate::kmain();
}
