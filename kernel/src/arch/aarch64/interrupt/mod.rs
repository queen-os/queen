//! Interrupt and exception for aarch64.

pub use self::handler::*;
use crate::drivers::{
    device_tree::DeviceTree,
    irq::{gicv2::GicV2, IrqManager},
    rtc::Pl031Rtc,
    serial::pl011_uart::Pl011Uart,
    Driver,
};
use aarch64::registers::*;
use alloc::sync::Arc;
use spin::Lazy;

use super::bsp::timer::GenericTimer;

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

pub static IRQ_MANAGER: Lazy<GicV2> = Lazy::new(|| unsafe { GicV2::new(0x08000000, 0x8010000) });

pub fn init(_device_tree: DeviceTree) {
    unsafe {
        aarch64::trap::init();
        IRQ_MANAGER.init().unwrap();

        let timer = Arc::new(GenericTimer::new());
        timer.init().unwrap();
        IRQ_MANAGER
            .register_and_enable_local_irq(30, timer)
            .unwrap();

        let uart = Arc::new(Pl011Uart::new(0x9000000));
        uart.init().unwrap();
        IRQ_MANAGER.register_and_enable_local_irq(33, uart).unwrap();

        let rtc = Arc::new(Pl031Rtc::new(0x09010000));
        rtc.init().unwrap();
        IRQ_MANAGER.register_and_enable_local_irq(34, rtc).unwrap();

        enable();
    }
}

pub fn init_other() {
    unsafe {
        aarch64::trap::init();
        IRQ_MANAGER.init().unwrap();
        enable();
    }
}
