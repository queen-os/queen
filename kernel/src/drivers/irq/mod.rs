use alloc::sync::Arc;

use super::{Driver, Result};

pub mod gicv2;
// mod gicv3;

pub use gicv2::GicV2;

/// IRQ management functions.
///
/// The `BSP` is supposed to supply one global instance. Typically implemented by the
/// platform's interrupt controller.
pub trait IrqManager: Driver {
    /// Register and enable interrupt controller local irq
    fn register_and_enable_local_irq(&self, irq_num: usize, driver: Arc<dyn Driver>) -> Result<()>;

    fn handle_pending_irqs(&self);
}
