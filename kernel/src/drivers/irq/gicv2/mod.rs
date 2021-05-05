use alloc::{collections::BTreeMap, sync::Arc, vec::Vec};

use crate::{
    drivers::{self, Driver},
    sync::spin::MutexNoIrq,
};

use super::IrqManager;

mod gicc;
mod gicd;

/// Representation of the GIC.
pub struct GicV2 {
    /// The Distributor.
    pub gicd: gicd::GicD,

    /// The CPU Interface.
    gicc: gicc::GicC,

    irq_map: MutexNoIrq<BTreeMap<usize, Vec<Arc<dyn Driver>>>>,
}

impl GicV2 {
    pub const COMPATIBLE: &'static str = "arm,cortex-a15-gic";

    /// Create an instance.
    ///
    /// # Safety
    ///
    /// - The user must ensure to provide a correct MMIO start address.
    pub unsafe fn new(gicd_mmio_start_addr: usize, gicc_mmio_start_addr: usize) -> Self {
        Self {
            gicd: gicd::GicD::new(gicd_mmio_start_addr),
            gicc: gicc::GicC::new(gicc_mmio_start_addr),
            irq_map: MutexNoIrq::new(BTreeMap::new()),
        }
    }
}

impl Driver for GicV2 {
    fn compatible(&self) -> &'static str {
        Self::COMPATIBLE
    }

    fn init(&self) -> drivers::Result<()> {
        if crate::cpu::id() == crate::arch::bsp::BOOT_CORE_ID {
            self.gicd.boot_core_init();
        }

        self.gicc.priority_accept_all();
        self.gicc.enable();

        Ok(())
    }

    fn handle_interrupt(&self) {
        self.handle_pending_irqs();
    }

    fn device_type(&self) -> drivers::DeviceType {
        drivers::DeviceType::Intc
    }
}

impl IrqManager for GicV2 {
    fn register_and_enable_local_irq(
        &self,
        irq_num: usize,
        driver: Arc<dyn Driver>,
    ) -> drivers::Result<()> {
        info!("Enabled IRQ[{}] Device[{}; {}].", irq_num, driver.device_type(), driver.compatible());
        let mut map = self.irq_map.lock();
        map.entry(irq_num).or_insert_with(Vec::new).push(driver);
        self.gicd.enable(irq_num);
        Ok(())
    }

    fn handle_pending_irqs(&self) {
        // Extract the highest priority pending IRQ number from the Interrupt Acknowledge Register
        // (IAR).
        let irq_number = self.gicc.pending_irq_number();

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

pub fn driver_init(device_tree: drivers::DeviceTree) -> Option<GicV2> {
    use crate::memory::as_upper_range;
    use fdt_rs::prelude::PropReader;

    let gic_node = device_tree.find_node_with_prop(|prop| {
        Ok(prop.name()?.eq("compatible") && prop.str()?.eq(GicV2::COMPATIBLE))
    })?;
    let mut reg_range_iter = device_tree.node_reg_range_iter(&gic_node)?;

    let gicd_mmio_start_addr = as_upper_range(reg_range_iter.next()?.start);
    let gicc_mmio_start_addr = as_upper_range(reg_range_iter.next()?.start);

    let gic = unsafe { GicV2::new(gicd_mmio_start_addr, gicc_mmio_start_addr) };
    gic.init().unwrap();

    info!("Initialized GICv2 interrupt controller.");

    Some(gic)
}
