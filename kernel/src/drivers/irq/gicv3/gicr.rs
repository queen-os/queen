use crate::drivers::common::MMIODerefWrapper;
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
    ],

    WAKER [
        ProcessorSleep OFFSET(1) NUMBITS(1) [],
        ChildrenAsleep OFFSET(2) NUMBITS(1) []
    ]
}

register_structs! {
    #[allow(non_snake_case)]
    pub RdBasedRegisterBlock {
        (0x0000 => CTLR: ReadWrite<u32, CTLR::Register>),
        (0x0008 => TYPER: ReadOnly<u32>),
        (0x0014 => WAKER: ReadWrite<u32, WAKER::Register>),
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
        (0x0400 => IPRIORITYR: [ReadWrite<u32>; 8]),
        (0x0c04 => ICFGR1: ReadWrite<u32>),
        (0x0E00 => @END),
    }
}

/// Abstraction for the associated MMIO registers.
type RdBasedRegisters = MMIODerefWrapper<RdBasedRegisterBlock>;
type SgiBasedRegisters = MMIODerefWrapper<SgiBasedRegisterBlock>;

pub struct GicR {
    rd_based_registers: RdBasedRegisters,
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
            rd_based_registers: RdBasedRegisters::new(mmio_start_addr),
            sgi_based_registers: SgiBasedRegisters::new(mmio_start_addr + 0x10000),
        }
    }

    #[inline]
    pub fn enable(&self, irq_num: usize) {
        self.sgi_based_registers.ISENABLER0.set(1 << irq_num);
        self.wait_for_rwp();
    }

    fn wakeup(&self) {
        self.rd_based_registers
            .WAKER
            .write(WAKER::ProcessorSleep::CLEAR);
        while self.rd_based_registers.WAKER.read(WAKER::ChildrenAsleep) != 0 {}
    }

    pub fn init(&self) {
        self.wakeup();

        let regs = &self.sgi_based_registers;

        // set the priority on PPI and SGI
        let pr = (0x90 << 24) | (0x90 << 16) | (0x90 << 8) | 0x90;
        for i in 0..4 {
            regs.IPRIORITYR[i].set(pr);
        }
        let pr = (0xa0 << 24) | (0xa0 << 16) | (0xa0 << 8) | 0xa0;
        for i in 4..8 {
            regs.IPRIORITYR[i].set(pr);
        }

        // disable all PPI and enable all SGI.
        regs.ICENABLER0.set(0xffff_0000);
        regs.ISENABLER0.set(0x0000_ffff);

        // configure sgi/ppi as non-secure group 1.
        regs.IGROUPR0.set(0xffff_ffff);

        self.wait_for_rwp();
        unsafe { crate::cpu::isb() }
    }

    fn wait_for_rwp(&self) {
        let mut count = 100_0000i32;
        while self.rd_based_registers.CTLR.read(CTLR::RWP) != 0 {
            count -= 1;
            if count.is_negative() {
                panic!("arm_gicv3: rwp timeout");
            }
        }
    }
}
