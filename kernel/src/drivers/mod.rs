use spin::Once;
use alloc::sync::Arc;

pub mod block;
pub mod bus;
mod common;
pub mod device_tree;
pub mod gpio;
pub mod irq;
pub mod rtc;
pub mod serial;

use core::fmt::Display;

pub use device_tree::DeviceTree;
pub use irq::IrqManager;
pub use rtc::RtcDriver;
pub use serial::SerialDriver;


pub type Result<T> = core::result::Result<T, DriverError>;

pub static RTC_DRIVER: Once<Arc<dyn RtcDriver>> = Once::new();

#[inline]
pub fn read_epoch() -> crate::TimeSpec {
    RTC_DRIVER.get().map(|rtc| rtc.read_epoch()).unwrap_or(crate::TimeSpec::zero())
}

#[derive(Debug)]
pub struct DriverError {}

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

impl DeviceType {
    #[inline]
    fn description(&self) -> &'static str {
        match self {
            DeviceType::Net => "Net",
            DeviceType::Gpu => "GPU",
            DeviceType::Input => "Input",
            DeviceType::Block => "Block",
            DeviceType::Rtc => "RTC",
            DeviceType::Serial => "Serial",
            DeviceType::Intc => "Interrupt Controller",
            DeviceType::Timer => "Timer",
        }
    }
}

impl Display for DeviceType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.description())
    }
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
