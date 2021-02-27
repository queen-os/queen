use super::IrqManager;
use crate::{
    drivers::{self, Driver},
    sync::spin::MutexNoIrq,
};
use aarch64::registers::*;
use alloc::{collections::BTreeMap, sync::Arc, vec::Vec};

mod gicd;
mod gicr;

/// Representation of the GIC.
pub struct GicV3 {
    /// The Distributor.
    gicd: gicd::GicD,

    gicr: gicr::GicR,

    irq_map: MutexNoIrq<BTreeMap<usize, Vec<Arc<dyn Driver>>>>,
}

impl GicV3 {
    /// Create an instance.
    ///
    /// # Safety
    ///
    /// - The user must ensure to provide a correct MMIO start address.
    pub unsafe fn new(gicd_mmio_start_addr: usize, gicr_mmio_start_addr: usize) -> Self {
        Self {
            gicd: gicd::GicD::new(gicd_mmio_start_addr),
            gicr: gicr::GicR::new(gicr_mmio_start_addr),
            irq_map: MutexNoIrq::new(BTreeMap::new()),
        }
    }

    fn gicc_init(&self) {
        // enable system register interface
        let sre = ICC_SRE_EL1.get_sre();
        if !sre {
            ICC_SRE_EL1.set_sre(true);
        }

        // set priority threshold to max.
        ICC_PMR_EL1.set_priority(0xff);
        // ICC_CTLR_EL1.EOImode.
        ICC_CTLR_EL1.set_eoi_mode(true);
        // enable group 1 interrupts.
        ICC_IGRPEN1_EL1.set_enable(true);

        unsafe { crate::cpu::isb() }
    }
}

impl Driver for GicV3 {
    fn compatible(&self) -> &'static str {
        "GICv3 (ARM Generic Interrupt Controller v3)"
    }

    fn init(&self) -> drivers::Result<()> {
        if crate::cpu::id() == crate::arch::bsp::BOOT_CORE_ID {
            self.gicd.boot_core_init();
        }

        self.gicr.init();
        self.gicc_init();

        Ok(())
    }

    fn handle_interrupt(&self) {
        self.handle_pending_irqs();
    }

    fn device_type(&self) -> drivers::DeviceType {
        drivers::DeviceType::Intc
    }
}

impl IrqManager for GicV3 {
    fn register_and_enable_local_irq(
        &self,
        irq_num: usize,
        driver: Arc<dyn Driver>,
    ) -> drivers::Result<()> {
        let mut map = self.irq_map.lock();
        map.entry(irq_num).or_insert_with(Vec::new).push(driver);

        match irq_num {
            0..=31 => self.gicr.enable(irq_num),
            _ => self.gicd.enable(irq_num),
        }

        Ok(())
    }

    fn handle_pending_irqs(&self) {
        // Extract the highest priority pending IRQ number from the Interrupt Acknowledge Register
        // (IAR).
        let irq_num = ICC_IAR1_EL1.get_pending_interrupt() as usize;
        dbg!(irq_num);

        if irq_num == 1023 {
            return;
        }

        if let Some(drivers) = self.irq_map.lock().get(&irq_num) {
            for driver in drivers {
                driver.handle_interrupt();
            }
        } else {
            panic!("No handler registered for IRQ {}", irq_num);
        }

        // Signal completion of handling.
        ICC_EOIR1_EL1.mark_completed(irq_num as u32);
    }
}
