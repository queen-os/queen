use super::Driver;

pub mod pl011_uart;

pub use pl011_uart::{PanicUart, Pl011Uart};

pub trait SerialDriver: Driver {
    /// Write a single character.
    fn write_char(&self, c: char);

    /// Write a string.
    fn write_str(&self, str: &str) {
        for c in str.chars() {
            self.write_char(c);
        }
    }

    /// Block until the last buffered character has been physically put on the TX wire.
    fn flush(&self);

    /// Read a single character.
    fn read_char(&self) -> char {
        ' '
    }

    /// Clear RX buffers, if any.
    fn clear_rx(&self);
}
