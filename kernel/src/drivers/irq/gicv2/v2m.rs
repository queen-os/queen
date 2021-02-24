use crate::{drivers::common::MMIODerefWrapper, sync::spin::MutexNoIrq};
use register::{mmio::*, register_bitfields, register_structs};

register_bitfields! {
    u32,

}


register_structs! {
    #[allow(non_snake_case)]
    pub RegisterBlock {
        (0x000 => CTLR: ReadWrite<u32>),
        (0x008 => TYPER: ReadOnly<u32>),
        (0x040 => SETSPI_NS: ReadWrite<u32>),
        (0x050  => @END),
    }
}

/// Abstraction for the associated MMIO registers.
type Registers = MMIODerefWrapper<RegisterBlock>;