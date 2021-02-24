use core::time::Duration;

use aarch64::registers::*;

use crate::drivers::{self, Driver, gpio::Pl061Gpio, serial::SerialDriver, serial::pl011_uart::Pl011Uart};
#[derive(Debug, Default)]
pub struct GenericTimer {}

impl GenericTimer {
    #[inline]
    pub fn freq() -> u64 {
        // 62500000 on qemu, 19200000 on real machine
        CNTFRQ_EL0.get() as u64
    }

    #[inline]
    pub const fn new() -> Self {
        GenericTimer {}
    }

    #[inline]
    pub fn stop(&self) {
        CNTP_CTL_EL0.write(CNTP_CTL_EL0::ENABLE::CLEAR);
    }

    #[inline]
    pub fn read(&self) -> Duration {
        Duration::from_micros((CNTPCT_EL0.get() * 1000000 / Self::freq()) as u64)
    }

    #[inline]
    pub fn tick_in(&self, us: usize) {
        let count = Self::freq() * (us as u64) / 1000000;
        // max `68719476` us (0xffff_ffff / 38400000 * 62500000).
        debug_assert!(count <= u32::max_value() as u64);
        CNTV_TVAL_EL0.set(count as u64);
    }
}

impl Driver for GenericTimer {
    fn compatible(&self) -> &'static str {
        "ARM Generic Timer"
    }

    fn init(&self) -> drivers::Result<()> {
        CNTV_CTL_EL0.write(CNTV_CTL_EL0::ENABLE::SET);
        Ok(())
    }

    fn handle_interrupt(&self) {
        // unsafe {
        //     let gpio = Pl061Gpio::new(0x09030000);
        //     dbg!(gpio.get_raw_status());
        //     let uart = Pl011Uart::new(0x9000000);
        //     dbg!(uart.get_status());
        // }
        self.tick_in(1000 * 1000);
    }

    fn device_type(&self) -> drivers::DeviceType {
        drivers::DeviceType::Timer
    }
}
