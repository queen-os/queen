use crate::{arch, sync::spin::MutexNoIrq};
use alloc::collections::BTreeMap;
use core::{
    future::Future,
    pin::Pin,
    task::{Context, Poll, Waker},
    time::Duration,
};

pub static TIMER: MutexNoIrq<Timer> = MutexNoIrq::new(Timer::new());

/// A naive timer.
#[derive(Default)]
pub struct Timer {
    events: BTreeMap<Duration, Waker>,
}

impl Timer {
    pub const fn new() -> Self {
        Timer {
            events: BTreeMap::new(),
        }
    }

    /// Add a timer.
    pub fn add(&mut self, mut deadline: Duration, waker: Waker) {
        while self.events.contains_key(&deadline) {
            deadline += Duration::from_nanos(1);
        }
        self.events.insert(deadline, waker);
    }

    /// Expire timers.
    ///
    /// Given the current time `now`, trigger and remove all expired timers.
    pub fn expire(&mut self, now: Duration) {
        while let Some(entry) = self.events.first_entry() {
            if *entry.key() > now {
                return;
            }
            let (_, waker) = entry.remove_entry();
            waker.wake();
        }
    }
}

/// Creates a timer that expires after the given duration of time.
pub async fn delay_for(duration: Duration) {
    DelayFuture {
        deadline: arch::timer::read() + duration,
    }
    .await;
}

pub struct DelayFuture {
    deadline: Duration,
}

impl Future for DelayFuture {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let deadline = self.deadline;
        // fast path
        if arch::timer::read() >= deadline {
            return Poll::Ready(());
        }
        let waker = cx.waker().clone();
        TIMER.lock().add(deadline, waker);
        Poll::Pending
    }
}
