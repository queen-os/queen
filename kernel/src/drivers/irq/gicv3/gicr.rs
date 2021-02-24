use crate::{drivers::common::MMIODerefWrapper, sync::spin::MutexNoIrq};
use register::{mmio::*, register_bitfields, register_structs};

register_bitfields! {
    u32,
    /// Controls the operation of a Redistributor, and enables the signaling of LPIs by the Redistributor to
    /// the connected PE.
    CTLR [
        EnableLPIs OFFSET(0) NUMBITS(1) [],
        /// Register Write Pending. This bit indicates whether a register write for the current Security state is
        /// in progress or not.
        RWP OFFSET(3) NUMBITS(1) []
    ]
}

register_structs! {
    #[allow(non_snake_case)]
    pub RegisterBlock {
        (0x0000 => CTLR: ReadWrite<u32, CTLR::Register>),
        (0x0008 => TYPER: ReadOnly<u32>),
        (0x0100  => @END),
    }
}

register_structs! {
    #[allow(non_snake_case)]
    pub SgiBasedRegisterBlock {
        (0x0080 => IGROUPR0: ReadWrite<u32>),
        /// Enables forwarding of the corresponding SGI or PPI to the CPU interfaces.
        (0x0100 => ISENABLER0: ReadWrite<u32>),
        (0x0180 => ICENABLER0: ReadWrite<u32>),
        (0x0280 => ICPENDR0: ReadWrite<u32>),
        (0x0E00 => @END),
    }
}

/// Abstraction for the associated MMIO registers.
type Registers = MMIODerefWrapper<RegisterBlock>;
type SgiBasedRegisters = MMIODerefWrapper<SgiBasedRegisterBlock>;

pub struct GicR {
    registers: Registers,
    sgi_based_registers: SgiBasedRegisters,
}

impl GicR {
    /// Create an instance.
    ///
    /// # Safety
    ///
    /// - The user must ensure to provide a correct MMIO start address.
    pub const unsafe fn new(mmio_start_addr: usize) -> Self {
        Self {
            registers:Registers::new(mmio_start_addr),
            sgi_based_registers: SgiBasedRegisters::new(mmio_start_addr + 0x10000),
        }
    }

    pub fn enable_ppi(&self, i: usize) {
        assert!(i <= 32);
        self.sgi_based_registers.ISENABLER0.set(1 << i);
    }

    pub fn init(&self) {
        let regs = &self.sgi_based_registers;
        // configure sgi/ppi as non-secure group 1.
        regs.IGROUPR0.set(!0);
        self.wait_for_rwp();
        // clear and mask sgi/ppi.
        regs.ICENABLER0.set(0xffff_ffff);
        regs.ICPENDR0.set(!0);
        self.wait_for_rwp();
    }

    fn wait_for_rwp(&self) {
        let mut count = 100_0000i32;
        while self.registers.CTLR.read(CTLR::RWP) != 0 {
            count -= 1;
            if count.is_negative() {
                panic!("arm_gicv3: rwp timeout");
            }
        }
    }
}
