//! Interrupt and exception for aarch64.

pub use self::handler::*;
use aarch64::registers::*;

pub mod consts;
pub mod handler;
mod syndrome;

/// Enable the interrupt (only IRQ).
#[inline(always)]
pub unsafe fn enable() {
    asm!("msr daifclr, #2");
}

/// Disable the interrupt (only IRQ).
#[inline(always)]
pub unsafe fn disable() {
    asm!("msr daifset, #2");
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
