use super::*;
use core::ptr::NonNull;
use queen_fs::TimeSpec;

impl Syscall<'_> {
    pub fn sys_clock_get_time(&mut self, clock: usize, mut ts: NonNull<TimeSpec>) -> SysResult {
        unsafe {
            *ts.as_mut() = crate::drivers::read_epoch();
        }

        Ok(0)
    }
}
