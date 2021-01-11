pub struct MiniUART {}

impl core::fmt::Write for MiniUART {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for c in s.chars() {
            unsafe {
                core::ptr::write_volatile(0x0900_0000 as *mut u8, c as u8);
            }
        }
        Ok(())
    }
}

pub fn uart() -> MiniUART {
    MiniUART {}
}
