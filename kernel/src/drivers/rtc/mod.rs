use crate::drivers::Driver;

pub struct RTC {}

impl Driver for RTC {
    fn compatible(&self) -> &'static str {
        "RTC"
    }

    fn handle_interrupt(&self) {
        println!("rtc irq");
    }
}
