use alloc::{collections::BTreeMap, sync::Arc, vec::Vec};

use crate::{drivers::Driver, sync::spin::MutexNoIrq};

use super::IRQManager;

mod gicc;
mod gicd;

/// Representation of the GIC.
pub struct GICv2 {
    /// The Distributor.
    gicd: gicd::GICD,

    /// The CPU Interface.
    gicc: gicc::GICC,

    irq_map: MutexNoIrq<BTreeMap<usize, Vec<Arc<dyn Driver>>>>,
}

impl GICv2 {
    /// Create an instance.
    ///
    /// # Safety
    ///
    /// - The user must ensure to provide a correct MMIO start address.
    pub unsafe fn new(gicd_mmio_start_addr: usize, gicc_mmio_start_addr: usize) -> Self {
        Self {
            gicd: gicd::GICD::new(gicd_mmio_start_addr),
            gicc: gicc::GICC::new(gicc_mmio_start_addr),
            irq_map: MutexNoIrq::new(BTreeMap::new()),
        }
    }
}

impl Driver for GICv2 {
    fn compatible(&self) -> &'static str {
        "GICv2 (ARM Generic Interrupt Controller v2)"
    }

    fn init(&self) -> Result<(), ()> {
        if crate::cpu::id() == crate::arch::bsp::BOOT_CORE_ID {
            self.gicd.boot_core_init();
        }

        self.gicc.priority_accept_all();
        self.gicc.enable();

        Ok(())
    }
}

impl IRQManager for GICv2 {
    fn register_local_irq(&self, irq_num: usize, driver: Arc<dyn Driver>) -> Result<(), ()> {
        let mut map = self.irq_map.lock();
        map.entry(irq_num).or_insert_with(Vec::new).push(driver);

        Ok(())
    }

    fn enable(&self, irq_num: usize) {
        self.gicd.enable(irq_num);
    }

    fn handle_pending_irqs(&self) {
        // Extract the highest priority pending IRQ number from the Interrupt Acknowledge Register
        // (IAR).
        let irq_number = self.gicc.pending_irq_number();

        if let Some(drivers) = self.irq_map.lock().get(&irq_number) {
            for driver in drivers {
                driver.handle_interrupt();
            }
        } else {
            panic!("No handler registered for IRQ {}", irq_number)
        }

        // Signal completion of handling.
        self.gicc.mark_completed(irq_number as u32);
    }
}
