//! Interrupt and exception for aarch64.

pub use self::handler::*;
use crate::drivers::{
    irq::{gicv2::GICv2, IRQManager},
    Driver,
};
use crate::drivers::{rtc::RTC, serial::pl011_uart::PL011Uart};
use aarch64::registers::{self, *};
use alloc::sync::Arc;
use spin::Lazy;

use super::bsp::timer::GenericTimer;

pub mod consts;
pub mod handler;
mod syndrome;

/// Enable the interrupt (only IRQ).
#[inline(always)]
pub unsafe fn enable() {
    asm!("msr DAIFClr, #2");
}

/// Disable the interrupt (only IRQ).
#[inline(always)]
pub unsafe fn disable() {
    asm!("msr DAIFSet, #2");
}

/// Disable the interrupt and store the status.
///
/// return: status(usize)
#[inline(always)]
pub unsafe fn disable_and_store() -> usize {
    let daif = DAIF.get() as usize;
    disable();
    daif
}

/// Use the original status to restore the process
///
/// Arguments:
/// * flags:  original status(usize)
#[inline(always)]
pub unsafe fn restore(flags: usize) {
    DAIF.set(flags as u64);
}

pub fn ack(_irq: usize) {
    // TODO
}

pub fn enable_irq(irq: usize) {
    // TODO
}

pub fn wait_for_interrupt() {
    let daif = DAIF.get();
    unsafe {
        asm!("msr daifclr, #2");
    }
    aarch64::asm::wfe();
    DAIF.set(daif);
}

pub static IRQ_MANAGER: Lazy<GICv2> = Lazy::new(|| unsafe { GICv2::new(0x08000000, 0x08010000) });

pub fn init() {
    unsafe {
        aarch64::trap::init();
        IRQ_MANAGER.init().unwrap();
        let timer = Arc::new(GenericTimer::new());
        IRQ_MANAGER.register_local_irq(27, timer.clone()).unwrap();
        IRQ_MANAGER.enable(27);
        timer.init();
        enable();
    }
}
