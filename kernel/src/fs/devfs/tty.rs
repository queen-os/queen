use crate::{
    process::{process_group, Pgid},
    signal::{send_signal, Siginfo, Signal, SI_KERNEL},
    sync::{Event, EventBus},
};
use alloc::{boxed::Box, collections::VecDeque, sync::Arc};
use core::{
    any::Any,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use queen_fs::vfs::*;
use spin::{Lazy, Mutex, RwLock};

/// console tty
// Ref: [https://linux.die.net/man/4/tty]
#[derive(Default)]
pub struct TtyINode {
    /// foreground process group
    foreground_pgid: RwLock<Pgid>,
    buf: Mutex<VecDeque<u8>>,
    event_bus: Mutex<EventBus>,
}

pub static TTY: Lazy<Arc<TtyINode>> = Lazy::new(|| Arc::new(TtyINode::default()));

pub fn foreground_pgid() -> Pgid {
    *TTY.foreground_pgid.read()
}

impl TtyINode {
    pub fn push(&self, c: u8) {
        if [0o3, 0o34, 0o32, 0o31].contains(&(c as i32)) {
            let foreground_processes = process_group(foreground_pgid());
            match c as i32 {
                // INTR
                0o3 => {
                    for proc in foreground_processes {
                        send_signal(
                            proc,
                            -1,
                            Siginfo {
                                signo: Signal::SIGINT as i32,
                                errno: 0,
                                code: SI_KERNEL,
                                field: Default::default(),
                            },
                        );
                    }
                }
                _ => warn!("special char {} is unimplented", c),
            }
        } else {
            self.buf.lock().push_back(c);
            self.event_bus.lock().set(Event::READABLE);
        }
    }

    pub fn pop(&self) -> u8 {
        let mut buf_lock = self.buf.lock();
        let c = buf_lock.pop_front().unwrap();
        if buf_lock.len() == 0 {
            self.event_bus.lock().clear(Event::READABLE);
        }
        return c;
    }

    pub fn can_read(&self) -> bool {
        return self.buf.lock().len() > 0;
    }
}

impl INode for TtyINode {
    /// Read bytes at `offset` into `buf`, return the number of bytes read.
    fn read_at(&self, offset: usize, buf: &mut [u8]) -> Result<usize> {
        if self.can_read() {
            buf[0] = self.pop() as u8;
            Ok(1)
        } else {
            Err(FsError::Again)
        }
    }

    /// Write bytes at `offset` from `buf`, return the number of bytes written.
    fn write_at(&self, offset: usize, buf: &[u8]) -> Result<usize> {
        use core::str;
        // we do not care the utf-8 things, we just want to print it!
        let s = unsafe { str::from_utf8_unchecked(buf) };
        print!("{}", s);
        Ok(buf.len())
    }

    /// Poll the events, return a bitmap of events.
    fn poll(&self) -> Result<PollStatus> {
        Ok(PollStatus {
            read: self.can_read(),
            write: true,
            error: false,
        })
    }

    fn async_poll<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = Result<PollStatus>> + Send + Sync + 'a>> {
        #[must_use = "future does nothing unless polled/`await`-ed"]
        struct SerialFuture<'a> {
            tty: &'a TtyINode,
        }

        impl<'a> Future for SerialFuture<'a> {
            type Output = Result<PollStatus>;

            fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
                if self.tty.can_read() {
                    return Poll::Ready(self.tty.poll());
                }
                let waker = cx.waker().clone();
                self.tty.event_bus.lock().subscribe(Box::new({
                    move |_| {
                        waker.wake_by_ref();
                        true
                    }
                }));
                Poll::Pending
            }
        }

        Box::pin(SerialFuture { tty: self })
    }

    /// Get metadata of the INode
    fn metadata(&self) -> Result<Metadata> {
        Ok(Metadata {
            dev: 1,
            inode: 13,
            size: 0,
            blk_size: 0,
            blocks: 0,
            atime: TimeSpec::zero(),
            mtime: TimeSpec::zero(),
            ctime: TimeSpec::zero(),
            r#type: FileType::CharDevice,
            mode: 0o666,
            nlinks: 1,
            uid: 0,
            gid: 0,
            rdev: make_rdev(5, 0),
        })
    }

    fn as_any_ref(&self) -> &dyn Any {
        self
    }
}
