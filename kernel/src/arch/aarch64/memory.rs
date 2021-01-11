use core::{cell::UnsafeCell, ops::RangeInclusive};

// Symbols from the linker script.
extern "Rust" {
    static __bss_start: UnsafeCell<u64>;
    static __bss_end_inclusive: UnsafeCell<u64>;
}

/// Return the inclusive range spanning the .bss section.
///
/// # Safety
///
/// - Values are provided by the linker script and must be trusted as-is.
/// - The linker-provided addresses must be u64 aligned.
#[inline]
pub fn bss_range_inclusive() -> RangeInclusive<*mut u64> {
    unsafe {
        RangeInclusive::new(__bss_start.get(), __bss_end_inclusive.get())
    }
}

pub fn early_init() {
    unsafe {crate::memory::zero_volatile(bss_range_inclusive());}
}