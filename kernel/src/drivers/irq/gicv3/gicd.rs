use crate::{drivers::common::MMIODerefWrapper, sync::spin::MutexNoIrq};
use register::{mmio::*, register_bitfields, register_structs};

register_bitfields! {
    u32,

    /// Distributor Control Register
    CTLR [
        /// Enable Group 0 interrupts.
        EnableGrp0 OFFSET(0) NUMBITS(1) [],
        EnableGrp1 OFFSET(1) NUMBITS(1) [],
        /// Affinity Routing Enable
        ARE OFFSET(4) NUMBITS(1) [],
        /// Register Write Pending. Read only. Indicates whether a register write is in progress or not.
        RWP OFFSET(31) NUMBITS(1) []
    ],

    /// Clear Non-secure SPI Pending Register
    CLRSPI_NSR [
        INTID OFFSET(0) NUMBITS(12) []
    ],

    /// Interrupt Controller Type Register
    TYPER [
        /// For the INTID range 32 to 1019, indicates the maximum SPI supported.
        ITLinesNumber OFFSET(0)  NUMBITS(5) [],
        /// Reports the number of PEs that can be used when affinity routing is not enabled, minus 1.
        CPUNumber OFFSET(5) NUMBITS(3) [],
        ESPI_range OFFSET(27) NUMBITS(5) []
    ],

    /// Interrupt Processor Targets Registers
    ITARGETSR [
        Offset3 OFFSET(24) NUMBITS(8) [],
        Offset2 OFFSET(16) NUMBITS(8) [],
        Offset1 OFFSET(8)  NUMBITS(8) [],
        Offset0 OFFSET(0)  NUMBITS(8) []
    ]
}

register_structs! {
    #[allow(non_snake_case)]
    RegisterBlock {
        (0x0000 => CTLR: ReadWrite<u32, CTLR::Register>),
        (0x0004 => TYPER: ReadOnly<u32, TYPER::Register>),
        (0x0008 => IIDR: ReadOnly<u32>),
        (0x0040 => SETSPI_NSR: WriteOnly<u32>),
        (0x0048 => CLRSPI_NSR: WriteOnly<u32, CLRSPI_NSR::Register>),
        (0x0080 => IGROUPR: [ReadWrite<u32>; 32]),
        (0x0100 => ISENABLER: [ReadWrite<u32>; 32]),
        (0x0180 => ICENABLER: [ReadWrite<u32>; 32]),
        (0x0200 => ISPENDR: [ReadWrite<u32> ;32]),
        (0x0280 => ICPENDR: [ReadWrite<u32> ;32]),
        (0x0400 => IPRIRITYR: [ReadWrite<u32>; 255]),
        (0x0820 => ITARGETSR: [ReadWrite<u32, ITARGETSR::Register>; 248]),
        (0x0c00 => ICFGR: [ReadWrite<u32>; 64]),
        (0x0d00 => IGRPMODR: [WriteOnly<u32>; 32]),
        (0x6000 => IROUTER: [ReadWrite<u64>; 1020]),
        (0xfffc => @END),
    }
}

/// Abstraction for the non-banked parts of the associated MMIO registers.
type Registers = MMIODerefWrapper<RegisterBlock>;

/// Representation of the GIC Distributor.
pub struct GicD {
    /// Access to shared registers is guarded with a lock.
    registers: MutexNoIrq<Registers>,
}

impl Registers {
    /// Return the number of IRQs that this HW implements.
    #[inline]
    fn num_irqs(&self) -> usize {
        // Query number of implemented IRQs.
        ((self.TYPER.read(TYPER::ITLinesNumber) as usize) + 1) * 32
    }

    fn wait_for_rwp(&self) {
        let mut count = 100_0000i32;
        while self.CTLR.read(CTLR::RWP) != 0 {
            count -= 1;
            if count.is_negative() {
                panic!("arm_gicv3: rwp timeout");
            }
        }
    }
}

impl GicD {
    /// Create an instance.
    ///
    /// # Safety
    ///
    /// - The user must ensure to provide a correct MMIO start address.
    pub const unsafe fn new(mmio_start_addr: usize) -> Self {
        Self {
            registers: MutexNoIrq::new(Registers::new(mmio_start_addr)),
        }
    }

    /// Route all SPIs to the boot core and enable the distributor.
    pub fn boot_core_init(&self) {
        let regs = self.registers.lock();

        // disable the distributor
        regs.CTLR.set(0);
        regs.wait_for_rwp();

        let gic_max_int = regs.num_irqs();

        // distributor config: mask and clear all spis, set group 1.
        for i in 1..gic_max_int / 32 {
            regs.ICENABLER[i].set(!0);
            regs.ICPENDR[i].set(!0);
            regs.IGROUPR[i].set(!0);
            regs.IGRPMODR[i].set(!0);
        }
        regs.wait_for_rwp();

        // enable distributor with ARE, group 1 enable
        regs.CTLR
            .write(CTLR::EnableGrp0::SET + CTLR::EnableGrp1::SET + CTLR::ARE::SET);
        unsafe { crate::cpu::isb() }

        // set spi to target cpu 0 (affinity 0.0.0.0). must do this after ARE enable
        for i in 32..gic_max_int {
            regs.IROUTER[i].set(0);
        }

        regs.wait_for_rwp();
        unsafe { crate::cpu::isb() }
    }

    /// Enable an interrupt.
    pub fn enable(&self, irq_num: usize) {
        // Each bit in the u32 enable register corresponds to one IRQ number. Shift right by 5
        // (division by 32) and arrive at the index for the respective ISENABLER[i].
        let enable_reg_index = irq_num >> 5;
        let enable_bit = 1u32 << (irq_num % 32);
        let enable_reg = &self.registers.lock().ISENABLER[enable_reg_index];
        enable_reg.set(enable_reg.get() | enable_bit);
    }
}
