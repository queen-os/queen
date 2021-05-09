use core::{slice, str};

/// Convert C string to Rust string.
#[inline]
pub unsafe fn from_cstr<'a>(s: *const u8) -> &'a str {
    let len = (0usize..).find(|&i| *s.add(i) == 0).unwrap();
    str::from_utf8_unchecked(slice::from_raw_parts(s, len))
}

/// Write a Rust string to C string.
#[inline]
pub unsafe fn write_cstr(ptr: *mut u8, s: &str) {
    ptr.copy_from(s.as_ptr(), s.len());
    ptr.add(s.len()).write(0);
}
