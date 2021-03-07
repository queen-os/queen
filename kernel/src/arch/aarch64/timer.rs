use crate::drivers::{self, Driver};
use aarch64::registers::*;
use core::time::Duration;

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
        CNTP_TVAL_EL0.set(count as u64);
    }
}

impl Driver for GenericTimer {
    fn compatible(&self) -> &'static str {
        "arm,armv8-timer"
    }

    fn init(&self) -> drivers::Result<()> {
        CNTP_CTL_EL0.write(CNTP_CTL_EL0::ENABLE::SET);
        Ok(())
    }

    fn handle_interrupt(&self) {
        crate::task::timer::TIMER.lock().expire(self.read());
        self.tick_in(10 * 1000);
    }

    fn device_type(&self) -> drivers::DeviceType {
        drivers::DeviceType::Timer
    }
}