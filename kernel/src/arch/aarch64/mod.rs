use alloc::vec::Vec;
use core::{
    hint::spin_loop,
    sync::atomic::{AtomicBool, Ordering},
};

use crate::{drivers, memory::phys_to_virt};

mod boot;
#[cfg_attr(feature = "bsp_virt", path = "bsp/virt/mod.rs")]
pub mod bsp;
pub mod consts;
pub mod cpu;
pub mod interrupt;
pub mod memory;
pub mod paging;
pub mod signal;
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

    let device_tree = drivers::DeviceTree::from_raw(phys_to_virt(device_tree_addr)).unwrap();
    let mut buf = Vec::<u8>::with_capacity(device_tree.totalsize());
    buf.extend_from_slice(device_tree.device_tree().buf());
    let device_tree = drivers::DeviceTree::new(buf.as_slice()).unwrap();

    crate::task::init(bsp::CPU_NUM);
    interrupt::init(device_tree);

    println!("Hello {}! from CPU {}", bsp::BOARD_NAME, cpu::id());
    async_test();
    AP_CAN_INIT.store(true, Ordering::Release);
    crate::kmain();
}

fn async_test() {
    use crate::task::*;
    use core::time::Duration;

    let task = spawn(async {
        loop {
            println!("Hello from kernel task[A]!");
            delay_for(Duration::from_secs(1)).await;
        }
    });
    task.detach();

    let task = spawn(async {
        delay_for(Duration::from_millis(500)).await;
        loop {
            println!("Hello from kernel task[B]!");
            delay_for(Duration::from_secs(2)).await;
        }
    });
    task.detach();

    local_executor().run();
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
