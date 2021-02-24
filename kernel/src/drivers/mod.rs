pub mod common;
pub mod device_tree;
pub mod irq;
pub mod rtc;
pub mod serial;
pub mod gpio;

#[derive(Debug)]
pub struct DriverError {}

pub type Result<T> = core::result::Result<T, DriverError>;

#[derive(Debug, Eq, PartialEq)]
pub enum DeviceType {
    Net,
    Gpu,
    Input,
    Block,
    Rtc,
    Serial,
    /// Interrupt controller
    Intc,
    Timer,
}

pub trait Driver: Send + Sync {
    /// Return a compatibility string for identifying the driver.
    fn compatible(&self) -> &'static str;

    /// Called by the kernel to bring up the device.
    fn init(&self) -> Result<()> {
        Ok(())
    }

    fn handle_interrupt(&self) {}

    /// return the correspondent device type
    fn device_type(&self) -> DeviceType;
}
