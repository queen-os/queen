pub mod common;
pub mod irq;
pub mod rtc;
pub mod serial;

pub trait Driver: Send + Sync {
    /// Return a compatibility string for identifying the driver.
    fn compatible(&self) -> &'static str;

    /// Called by the kernel to bring up the device.
    fn init(&self) -> Result<(), ()> {
        Ok(())
    }

    fn handle_interrupt(&self) {}
}
