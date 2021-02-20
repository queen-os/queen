use super::Driver;

pub mod pl011_uart;

pub trait SerialDriver: Driver {
    /// Write a single character.
    fn write_char(&self, c: char);

    /// Block until the last buffered character has been physically put on the TX wire.
    fn flush(&self);

    /// Read a single character.
    fn read_char(&self) -> char {
        ' '
    }

    /// Clear RX buffers, if any.
    fn clear_rx(&self);
}
