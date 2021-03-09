use spin::RwLock;

#[derive(Debug, Clone, Copy)]
pub struct MiniUART {
    addr: usize,
}

impl MiniUART {
    pub const fn new(addr: usize) -> Self {
        Self { addr }
    }
}

impl Default for MiniUART {
    fn default() -> Self {
        MiniUART::new(0x0900_0000)
    }
}

impl core::fmt::Write for MiniUART {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for c in s.chars() {
            unsafe {
                core::ptr::write_volatile(self.addr as *mut u8, c as u8);
            }
        }
        Ok(())
    }
}

pub static UART: RwLock<MiniUART> = RwLock::new(MiniUART::new(0x0900_0000));

#[inline]
pub fn uart() -> MiniUART {
    *UART.read()
}

#[inline]
pub fn set_new_uart(addr: usize) {
    *UART.write() = MiniUART::new(addr);
}
