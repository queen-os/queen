use core::ptr::NonNull;
use cstr_core::{c_char, CStr};
use crate::{
    arch::{cpu, syscall::*},
    memory::{MemorySet, VmError},
    process::{Process, Thread},
    sync::MutexGuardNoIrq,
};
use aarch64::trap::UserContext;
use alloc::sync::Arc;

pub use queen_syscall::Error as SysError;
pub type SysResult = queen_syscall::Result<usize>;

pub use self::{fs::*, process::*, time::*};

mod fs;
mod process;
mod time;

/// System call dispatcher
pub async fn handle_syscall(thread: &Arc<Thread>, context: &mut UserContext) -> bool {
    let regs = &context.general;
    let num = context.get_syscall_num();
    let args = context.get_syscall_args();

    let mut syscall = Syscall {
        thread,
        context,
        exit: false,
    };
    let ret = syscall.syscall(num, args).await;
    let exit = syscall.exit;
    context.set_syscall_ret(ret as usize);

    exit
}

/// All context needed for syscall
struct Syscall<'a> {
    pub thread: &'a Arc<Thread>,
    pub context: &'a mut UserContext,
    /// Set `true` to exit current task.
    pub exit: bool,
}

impl Syscall<'_> {
    /// Get current process
    #[inline]
    pub fn process(&self) -> MutexGuardNoIrq<Process> {
        self.thread.process.lock()
    }

    /// Get current virtual memory
    #[inline]
    pub fn vm(&self) -> MutexGuardNoIrq<MemorySet> {
        self.thread.vm.lock()
    }

    async fn syscall(&mut self, id: usize, args: [usize; 6]) -> isize {
        todo!()
    }
}

impl From<VmError> for SysError {
    fn from(_: VmError) -> Self {
        SysError::EFAULT
    }
}

#[inline]
pub fn parse_cstr<'a>(ptr: NonNull<u8>) -> Result<&'a str, SysError> {
    unsafe {
        CStr::from_ptr(ptr.cast().as_ptr()).to_str().map_err(|_| SysError::EFAULT)
    }
}