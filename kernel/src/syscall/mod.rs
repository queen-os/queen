use crate::{
    arch::{cpu, syscall::*},
    memory::{MemorySet, VmError},
    process::{Process, Thread},
    sync::MutexGuardNoIrq,
};
use aarch64::trap::UserContext;
use alloc::sync::Arc;
use core::ptr::NonNull;

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
        let cid = cpu::id();
        let pid = self.process().pid.clone();
        let tid = self.thread.tid;

        let ret = match id {
            // file
            SYS_READ => self.sys_read(args[0], args[1], args[2]).await,
            SYS_WRITE => self.sys_write(args[0], args[1] as _, args[2]),
            SYS_OPENAT => self.sys_open_at(args[0], args[1] as _, args[2], args[3]),
            SYS_CLOSE => self.sys_close(args[0]),
            SYS_LSEEK => self.sys_lseek(args[0], args[1] as i64, args[2] as u8),
            SYS_PREAD64 => self.sys_pread(args[0], args[1], args[2], args[3]).await,
            SYS_PWRITE64 => self.sys_pwrite(args[0], args[1] as _, args[2], args[3]),
            SYS_FSYNC => self.sys_fsync(args[0]),
            SYS_FDATASYNC => self.sys_fdata_sync(args[0]),
            SYS_TRUNCATE => self.sys_truncate(args[0] as _, args[1]),
            SYS_FTRUNCATE => self.sys_ftruncate(args[0], args[1]),
            SYS_GETCWD => self.sys_get_cwd(args[0] as _, args[1]),
            SYS_CHDIR => self.sys_chdir(args[0] as _),
            SYS_RENAMEAT => self.sys_rename_at(args[0], args[1] as _, args[2], args[3] as _),
            SYS_MKDIRAT => self.sys_mkdir_at(args[0], args[1] as _, args[2]),
            SYS_LINKAT => self.sys_link_at(args[0], args[1] as _, args[2], args[3] as _, args[4]),
            SYS_UNLINKAT => self.sys_unlink_at(args[0], args[1] as _, args[2]),
            SYS_SYMLINKAT => self.sys_symlink_at(args[0] as _, args[1] as usize, args[2] as _),
            SYS_FACCESSAT => self.sys_faccess_at(args[0], args[1] as _, args[2], args[3]),
            SYS_DUP3 => self.sys_dup3(args[0], args[1], args[2]),

            // schedule
            SYS_SCHED_YIELD => self.sys_yield().await,

            // process
            SYS_CLONE => self.sys_clone(args[0], args[1], args[2] as _, args[3] as _, args[4]),
            SYS_EXIT => self.sys_exit(args[0]),
            SYS_EXIT_GROUP => self.sys_exit_group(args[0]),
            SYS_WAIT4 => self.sys_wait4(args[0] as _, args[1]).await, // TODO: wait4
            SYS_SET_TID_ADDRESS => self.sys_set_tid_address(args[0] as _),
            SYS_NANOSLEEP => self.sys_nanosleep(args[0]).await,

            // time
            SYS_CLOCK_GETTIME => {
                self.sys_clock_get_time(args[0], NonNull::new(args[1] as _).unwrap())
            }

            _ => {
                error!("unknown syscall id: {}, args: {:x?}", id, args);
                todo!()
            }
        };

        match ret {
            Ok(code) => code as isize,
            Err(err) => -(err as isize),
        }
    }
}

impl From<VmError> for SysError {
    fn from(_: VmError) -> Self {
        SysError::EFAULT
    }
}
