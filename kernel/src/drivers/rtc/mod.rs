use super::Driver;

pub mod pl031;

pub use pl031::Pl031Rtc;

pub trait RtcDriver: Driver {
    /// Read seconds since 1970-01-01
    fn read_epoch(&self) -> u64;
}
