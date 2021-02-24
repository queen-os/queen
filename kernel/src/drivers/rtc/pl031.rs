use register::{mmio::*, register_bitfields, register_structs};

use crate::drivers::{self, common::MMIODerefWrapper, Driver};

use super::RtcDriver;

register_bitfields! {
    u32,

    /// Control Register.
    CR [
        /// RTC start
        /// If set to 1, the RTC is enabled. After the RTC is enabled, do not write to this bit otherwise the current
        /// RTC value is reset to zero.
        /// A read returns the status of the RTC.
        RTCEN OFFSET(0) NUMBITS(1) []
    ],
    /// Interrupt Mask Set/Clear Register.
    IMSC [
        /// RTCIMSC is a 1-bit read/write register, and controls the masking of the interrupt that the RTC
        /// generates. Writing to bit[0] sets or clears the mask. Reading this register returns the current
        /// value of the mask on the RTCINTR interrupt.
        RTCIMSC OFFSET(0) NUMBITS(1) []
    ],
    /// Raw Interrupt Status.
    RIS [
        /// Gives the raw interrupt state (before masking) of the RTCINTR interrupt.
        RTCRIS OFFSET(0) NUMBITS(1) []
    ],
    /// Masked Interrupt Status
    MIS [
        /// Gives the masked interrupt status (after masking) of the RTCINTR interrupt.
        RTCMIS OFFSET(0) NUMBITS(1) []
    ],
    /// Interrupt Clear Register
    ICR [
        /// Clears the RTCINTR interrupt.
        /// Writing 1 clears the interrupt. Writing 0 has no effect
        RTCICR OFFSET(0) NUMBITS(1) []
    ]

}

register_structs! {
    #[allow(non_snake_case)]
    pub RegisterBlock {
        (0x00 => DR: ReadOnly<u32>),
        (0x04 => MR: ReadWrite<u32>),
        (0x08 => LR: ReadWrite<u32>),
        (0x0c => CR: ReadWrite<u32, CR::Register>),
        (0x10 => IMSC: ReadWrite<u32, IMSC::Register>),
        (0x14 => RIS: ReadOnly<u32, RIS::Register>),
        (0x18 => MIS: ReadOnly<u32, MIS::Register>),
        (0x1c => ICR: WriteOnly<u32, ICR::Register>),
        (0x20 => @END),
    }
}

/// Abstraction for the associated MMIO registers.
type Registers = MMIODerefWrapper<RegisterBlock>;

pub struct Pl031Rtc {
    registers: Registers,
}

impl Pl031Rtc {
    /// Create an instance.
    ///
    /// # Safety
    ///
    /// - The user must ensure to provide a correct MMIO start address.
    pub const unsafe fn new(mmio_start_addr: usize) -> Self {
        Self {
            registers: Registers::new(mmio_start_addr),
        }
    }

    pub fn set_next(&self) {
        let x = self.read_epoch() as u32;
        self.registers.MR.set(x + 2);
    }
}

impl Driver for Pl031Rtc {
    fn compatible(&self) -> &'static str {
        "BCM PL031 RTC"
    }

    fn init(&self) -> drivers::Result<()> {
        // Turn the RTC off temporarily.
        self.registers.CR.set(0);
        // Clear any pending alarm interrupts.
        self.registers.ICR.write(ICR::RTCICR::SET);
        // Enable IRQ
        self.registers.IMSC.write(IMSC::RTCIMSC::CLEAR);
        // Turn the RTC on
        self.registers.CR.write(CR::RTCEN::SET);

        Ok(())
    }

    fn device_type(&self) -> drivers::DeviceType {
        drivers::DeviceType::Rtc
    }

    fn handle_interrupt(&self) {
        todo!()
    }
}

impl RtcDriver for Pl031Rtc {
    fn read_epoch(&self) -> u64 {
        self.registers.DR.get() as u64
    }
}
