//! Interrupt and exception for aarch64.

pub use self::handler::*;

use crate::drivers::{self, irq::GicV2, rtc::Pl031Rtc, DeviceTree, Driver};
use aarch64::registers::*;
use alloc::sync::Arc;
use spin::Once;

pub mod consts;
pub mod handler;
mod syndrome;

/// Enable the interrupt (only IRQ).
/// # Safety
#[inline]
pub unsafe fn enable() {
    asm!("msr DAIFClr, #2");
}

/// Disable the interrupt (only IRQ).
/// # Safety
#[inline]
pub unsafe fn disable() {
    asm!("msr DAIFSet, #2");
}

/// Disable the interrupt and store the status.
///
/// return: status(usize)
/// # Safety
#[inline]
pub unsafe fn disable_and_store() -> usize {
    let daif = DAIF.get() as usize;
    disable();
    daif
}

/// Use the original status to restore the process
///
/// Arguments:
/// * flags:  original status(usize)
/// # Safety
#[inline]
pub unsafe fn restore(flags: usize) {
    DAIF.set(flags as u64);
}

pub fn wait_for_interrupt() {
    let daif = DAIF.get();
    unsafe {
        asm!("msr daifclr, #2");
    }
    aarch64::asm::wfe();
    DAIF.set(daif);
}

pub static IRQ_MANAGER: Once<GicV2> = Once::new();

pub fn init(device_tree: DeviceTree) {
    unsafe {
        aarch64::trap::init();
    }

    let irq_manager = drivers::irq::gicv2::driver_init(device_tree).unwrap();
    irq_manager.init().unwrap();
    IRQ_MANAGER.call_once(|| irq_manager);

    crate::arch::timer::driver_init(device_tree, &irq_manager);

    drivers::serial::pl011_uart::driver_init(device_tree, &irq_manager);

    drivers::rtc::pl031::driver_init(device_tree, &irq_manager).unwrap();

    unsafe {
        enable();
    }
}

pub fn init_other() {
    unsafe {
        aarch64::trap::init();
        IRQ_MANAGER.wait().init().unwrap();
        enable();
    }
}
