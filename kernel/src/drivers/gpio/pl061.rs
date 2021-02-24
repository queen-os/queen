use register::{mmio::*, register_bitfields, register_structs};

use crate::drivers::{self, common::MMIODerefWrapper, Driver};

register_bitfields! {
    u32,


}

register_structs! {
    #[allow(non_snake_case)]
    pub RegisterBlock {
        (0x000 => DATA: ReadWrite<u8>),
        (0x400 => DIR: ReadWrite<u8>),
        (0x404 => IS: ReadWrite<u8>),
        (0x408 => IBE: ReadWrite<u8>),
        (0x40c => IEV: ReadWrite<u8>),
        (0x410 => IE: ReadWrite<u8>),
        (0x414 => RIS: ReadOnly<u8>),
        (0x418 => MIS: ReadOnly<u8>),
        (0x41c => IC: WriteOnly<u8>),
        (0x420 => AFSEL: ReadWrite<u8>),
        (0x424 => @END),
    }
}

/// Abstraction for the associated MMIO registers.
type Registers = MMIODerefWrapper<RegisterBlock>;

pub struct Pl061Gpio {
    registers: Registers,
}

impl Pl061Gpio {
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

    pub fn init(&self) {
        self.registers.IC.set(1);
        // high level
        self.registers.IEV.set(0b1111_1111);
        // unmask
        self.registers.IE.set(0b1111_1111);
        // high level
        self.registers.IS.set(0b1111_1111);
        // enable hardware control
        self.registers.AFSEL.set(0b1111_1111);
    }

    pub fn get_raw_status(&self) -> u8 {
        self.registers.RIS.get()
    }
}
