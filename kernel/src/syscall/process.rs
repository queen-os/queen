use super::*;
use crate::{
    arch::timer,
    process::{Pgid, Thread, PROCESSES},
    sync::{wait_for_event, Event, EventBus, MutexNoIrq},
    task::timer::TIMER,
    TimeSpec,
};
use alloc::{boxed::Box, vec::Vec};
use core::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};
use queen_syscall::flags::CloneFlags;

impl Syscall<'_> {
    /// Fork the current process. Return the child's PID.
    pub fn sys_fork(&mut self) -> SysResult {
        let new_thread = self.thread.fork(self.context);
        new_thread.spawn();
        let pid = new_thread.process.lock().pid;

        Ok(pid)
    }

    /// Create a new thread in the current process.
    /// The new thread's stack pointer will be set to `newsp`,
    /// and thread pointer will be set to `newtls`.
    /// The child tid will be stored at both `parent_tid` and `child_tid`.
    /// This is partially implemented for musl only.
    pub fn sys_clone(
        &mut self,
        flags: usize,
        new_sp: usize,
        parent_tid: *mut u32,
        child_tid: *mut u32,
        new_tls: usize,
    ) -> SysResult {
        let clone_flags = CloneFlags::from_bits_truncate(flags);
        if flags == 0x4111 || flags == 0x11 {
            warn!("sys_clone is calling sys_fork instead, ignoring other args");
            return self.sys_fork();
        }
        if (flags != 0x7d0f00) && (flags != 0x5d0f00) {
            // 0x5d0f00 is the args from gcc of alpine linux
            warn!(
                "sys_clone only support sys_fork or musl pthread_create without flags{:x}",
                flags
            );
            return Err(SysError::ENOSYS);
        }
        let parent_tid_ref = unsafe { self.vm().check_write_ptr(parent_tid)? };
        // child_tid buffer should not be set because CLONE_CHILD_SETTID flag is not specified in the current implementation
        let child_tid_ref = unsafe { self.vm().check_write_ptr(child_tid)? };
        let new_thread =
            self.thread
                .new_clone(self.context, new_sp, new_tls, child_tid as usize);
        if clone_flags.contains(CloneFlags::CHILD_CLEARTID) {
            new_thread.inner.lock().clear_child_tid = child_tid as usize;
        }
        let tid: usize = new_thread.tid;
        *parent_tid_ref = tid as u32;
        *child_tid_ref = tid as u32;

        new_thread.spawn();

        Ok(tid)
    }

    /// Wait for the process exit.
    /// Return the PID. Store exit code to `wstatus` if it's not null.
    pub async fn sys_wait4(&mut self, pid: isize, wstatus: usize) -> SysResult {
        #[derive(Debug)]
        enum WaitFor {
            AnyChild,
            AnyChildInGroup,
            Pid(usize),
        }
        let target = match pid {
            -1 => WaitFor::AnyChild,
            0 => WaitFor::AnyChildInGroup,
            p if p > 0 => WaitFor::Pid(p as usize),
            _ => unimplemented!(),
        };

        loop {
            let mut process = self.process();

            // check child state
            let find = match target {
                WaitFor::AnyChild | WaitFor::AnyChildInGroup => {
                    let mut res = None;
                    for (pid, child) in &process.children {
                        if let Some(c) = child.upgrade() {
                            let p = c.lock();
                            if p.exited() {
                                res = Some((p.pid, p.exit_code));
                                break;
                            }
                        }
                    }
                    res
                }
                WaitFor::Pid(pid) => {
                    let mut res = None;
                    if let Some(c) = crate::process::process(pid) {
                        let p = c.lock();
                        if p.exited() {
                            res = Some((p.pid, p.exit_code));
                        }
                    }
                    res
                }
            };
            // if found, return
            if let Some((pid, exit_code)) = find {
                // write before removing to handle EFAULT
                let wstatus = wstatus as *mut i32;
                if !wstatus.is_null() {
                    unsafe { *wstatus = exit_code as i32 }
                }

                // remove from process table
                PROCESSES.write().remove(&pid);

                // remove from children
                process.children.retain(|(p, _)| *p != pid);

                return Ok(pid);
            }
            // if not, check pid
            let invalid = {
                let children = process
                    .children
                    .iter()
                    .filter_map(|(pid, weak)| {
                        if weak.upgrade().is_none() {
                            None
                        } else {
                            Some(*pid)
                        }
                    })
                    .collect::<Vec<_>>();
                match target {
                    WaitFor::AnyChild | WaitFor::AnyChildInGroup => children.len() == 0,
                    WaitFor::Pid(pid) => children.into_iter().find(|&p| p == pid).is_none(),
                }
            };
            if invalid {
                return Err(SysError::ECHILD);
            }

            let event_bus = process.event_bus.clone();
            drop(process);

            wait_for_event(event_bus.clone(), Event::CHILD_PROCESS_QUIT).await;
            event_bus.lock().clear(Event::CHILD_PROCESS_QUIT);
        }
    }

    pub async fn sys_yield(&mut self) -> SysResult {
        crate::task::yield_now().await;

        Ok(0)
    }

    /// Get the current process id
    pub fn sys_get_pid(&mut self) -> SysResult {
        Ok(self.process().pid)
    }

    pub fn sys_get_pgid(&self, mut pid: usize) -> SysResult {
        if pid == 0 {
            pid = self.process().pid;
        }

        let process_table = PROCESSES.read();
        let process = process_table.get(&pid);
        if let Some(process) = process {
            Ok(process.lock().pgid as usize)
        } else {
            Err(SysError::ESRCH)
        }
    }

    pub fn sys_set_pgid(&self, mut pid: usize, pgid: usize) -> SysResult {
        if pid == 0 {
            pid = self.process().pid;
        }

        let process_table = PROCESSES.read();
        let process = process_table.get(&pid);
        if let Some(process) = process {
            process.lock().pgid = pgid as Pgid;
            Ok(0)
        } else {
            Err(SysError::ESRCH)
        }
    }

    /// Get the current thread id
    pub fn sys_get_tid(&mut self) -> SysResult {
        Ok(self.thread.tid)
    }

    /// Get the parent process id
    pub fn sys_get_ppid(&mut self) -> SysResult {
        let (pid, parent) = self.process().parent.clone();
        if parent.upgrade().is_some() {
            Ok(pid)
        } else {
            Ok(0)
        }
    }

    /// Exit the current thread
    pub fn sys_exit(&mut self, exit_code: usize) -> SysResult {
        let tid = self.thread.tid;

        let mut process = self.process();
        process.threads.retain(|&id| id != tid);

        // for last thread, exit the process
        if process.threads.len() == 0 {
            process.exit(exit_code);
        }

        drop(process);
        self.exit = true;
        Ok(0)
    }

    /// Exit the current thread group (i.e. process)
    pub fn sys_exit_group(&mut self, exit_code: usize) -> SysResult {
        self.process().exit(exit_code);
        // TODO: quit other threads
        self.exit = true;
        Ok(0)
    }

    pub async fn sys_nanosleep(&mut self, req: usize) -> SysResult {
        let time = unsafe { *(req as *const TimeSpec) };
        if !time.is_zero() {
            self.sleep_for(time.into()).await?;
            if self.thread.has_signal_to_handle() {
                return Err(SysError::EINTR);
            }
        }
        Ok(0)
    }

    pub fn sys_set_priority(&mut self, priority: usize) -> SysResult {
        // TODO
        Ok(0)
    }

    pub fn sys_set_tid_address(&mut self, tidptr: *mut u32) -> SysResult {
        self.thread.inner.lock().clear_child_tid = tidptr as usize;

        Ok(self.thread.tid)
    }

    // sleeping
    pub fn sleep_for(&mut self, duration: Duration) -> impl Future<Output = SysResult> {
        SleepFuture {
            deadline: timer::read() + duration,
            duration,
            thread: self.thread.clone(),
            event_bus: self.thread.process.lock().event_bus.clone(),
        }
    }
}

#[must_use = "future does nothing unless polled/`await`-ed"]
pub struct SleepFuture {
    deadline: Duration,
    duration: Duration,
    thread: Arc<Thread>,
    event_bus: Arc<MutexNoIrq<EventBus>>,
}

impl Future for SleepFuture {
    type Output = SysResult;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        // check
        if timer::read() >= self.deadline {
            return Poll::Ready(Ok(0));
        } else if self.thread.has_signal_to_handle() {
            return Poll::Ready(Err(SysError::EINTR));
        }

        // handle infinity
        if self.duration.as_nanos() < i64::max_value() as u128 {
            TIMER.lock().add(self.deadline, cx.waker().clone());
        }

        let waker = cx.waker().clone();
        self.event_bus.lock().subscribe(Box::new({
            move |_| {
                waker.wake_by_ref();
                true
            }
        }));

        Poll::Pending
    }
}
