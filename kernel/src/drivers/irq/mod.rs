use alloc::sync::Arc;

use super::Driver;

pub mod gicv2;

/// IRQ management functions.
///
/// The `BSP` is supposed to supply one global instance. Typically implemented by the
/// platform's interrupt controller.
pub trait IRQManager {
    /// Register interrupt controller local irq
    fn register_local_irq(&self, irq_num: usize, driver: Arc<dyn Driver>) -> Result<(), ()>;

    /// Enable an interrupt in the controller.
    fn enable(&self, irq_num: usize);

    fn handle_pending_irqs(&self);
}
