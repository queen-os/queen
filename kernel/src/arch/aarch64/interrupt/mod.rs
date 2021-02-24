//! Interrupt and exception for aarch64.

pub use self::handler::*;
use crate::drivers::{
    device_tree::DeviceTree,
    gpio::Pl061Gpio,
    irq::{gicv2::GicV2, gicv3::GicV3, IrqManager},
    rtc::Pl031Rtc,
    serial::{pl011_uart::Pl011Uart, SerialDriver},
    Driver,
};
use aarch64::registers::{self, *};
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

#[inline]
pub fn enable_irq(irq_num: usize) {
    IRQ_MANAGER.enable(irq_num);
}

pub fn wait_for_interrupt() {
    let daif = DAIF.get();
    unsafe {
        asm!("msr daifclr, #2");
    }
    aarch64::asm::wfe();
    DAIF.set(daif);
}

pub static IRQ_MANAGER: Lazy<GicV3> =
    Lazy::new(|| unsafe { GicV3::new(0x08000000, 0x08010000, 0x80a0000) });

pub fn init(_device_tree: DeviceTree) {
    unsafe {
        aarch64::trap::init();
        IRQ_MANAGER.init().unwrap();
        let timer = Arc::new(GenericTimer::new());
        IRQ_MANAGER.register_local_irq(27, timer.clone()).unwrap();
        IRQ_MANAGER.enable(27);
        timer.init().unwrap();

        // let gpio = Pl061Gpio::new(0x09030000);
        // gpio.init();

        // let uart = Arc::new(Pl011Uart::new(0x9000000));
        // IRQ_MANAGER.register_local_irq(33, uart.clone()).unwrap();
        // IRQ_MANAGER.enable(33);
        // for i in 0..256 {
        //     IRQ_MANAGER.enable(i);
        // }
        // uart.init().unwrap();

        // let rtc = Arc::new(Pl031Rtc::new(0x09010000));
        // IRQ_MANAGER.register_local_irq(34, rtc.clone()).unwrap();
        // IRQ_MANAGER.enable(34);
        // rtc.init().unwrap();
        // rtc.set_next();

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
