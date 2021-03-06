use core::{
    hint::spin_loop,
    sync::atomic::{AtomicBool, Ordering},
};

use alloc::vec::Vec;

use crate::drivers;

mod boot;
#[cfg_attr(feature = "bsp_virt", path = "bsp/virt/mod.rs")]
pub mod bsp;
pub mod consts;
pub mod cpu;
pub mod interrupt;
pub mod memory;
pub mod paging;
pub mod syscall;
pub mod timer;

static AP_CAN_INIT: AtomicBool = AtomicBool::new(false);

#[no_mangle]
unsafe extern "C" fn main_start() -> ! {
    let device_tree_addr: usize;
    asm!("mov {}, x23", out(reg) device_tree_addr, options(pure, nomem, nostack));

    crate::logging::init();

    let device_tree = drivers::DeviceTree::from_raw(device_tree_addr).unwrap();
    memory::init(memory::MemInitOpts::new(
        device_tree.probe_memory().unwrap(),
    ));

    let device_tree =
        drivers::DeviceTree::from_raw(crate::memory::phys_to_virt(device_tree_addr)).unwrap();
    let mut buf = Vec::<u8>::with_capacity(device_tree.totalsize());
    buf.extend_from_slice(device_tree.device_tree().buf());
    let device_tree = drivers::DeviceTree::new(buf.as_slice()).unwrap();

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
    interrupt::init_other();
    crate::kmain();
}
