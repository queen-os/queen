use alloc::{collections::BTreeMap, sync::Arc, vec::Vec};

use crate::{
    drivers::{self, Driver},
    sync::spin::MutexNoIrq,
};

use super::IrqManager;

mod gicc;
mod gicd;
mod gicr;

/// Representation of the GIC.
pub struct GicV3 {
    /// The Distributor.
    pub gicd: gicd::GicD,

    /// The CPU Interface.
    gicc: gicc::GicC,

    gicr: gicr::GicR,

    irq_map: MutexNoIrq<BTreeMap<usize, Vec<Arc<dyn Driver>>>>,
}

impl GicV3 {
    /// Create an instance.
    ///
    /// # Safety
    ///
    /// - The user must ensure to provide a correct MMIO start address.
    pub unsafe fn new(gicd_mmio_start_addr: usize, gicc_mmio_start_addr: usize, gicr_mmio_start_addr:usize) -> Self {
        Self {
            gicd: gicd::GicD::new(gicd_mmio_start_addr),
            gicc: gicc::GicC::new(gicc_mmio_start_addr),
            gicr: gicr::GicR::new(gicr_mmio_start_addr),
            irq_map: MutexNoIrq::new(BTreeMap::new()),
        }
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

        // self.gicc.priority_accept_all();
        // self.gicc.enable();

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
    fn register_local_irq(&self, irq_num: usize, driver: Arc<dyn Driver>) -> drivers::Result<()> {
        let mut map = self.irq_map.lock();
        map.entry(irq_num).or_insert_with(Vec::new).push(driver);

        Ok(())
    }

    fn enable(&self, irq_num: usize) {
        self.gicd.enable(irq_num);
        self.gicr.enable_ppi(irq_num);
    }

    fn handle_pending_irqs(&self) {
        // Extract the highest priority pending IRQ number from the Interrupt Acknowledge Register
        // (IAR).
        let irq_number = self.gicc.pending_irq_number();
        dbg!(irq_number);
        // let p = self.gicd.read_pending();
        // println!("{:b} {:b}", p.0, p.1);
        if irq_number == 1023 {
            return;
        }

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