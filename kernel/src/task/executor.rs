use alloc::sync::Arc;
use async_task::{Runnable, Task};
use core::future::Future;
use crossbeam_queue::ArrayQueue;

pub struct Executor {
    task_queue: Arc<ArrayQueue<Runnable>>,
}

impl Default for Executor {
    fn default() -> Self {
        Executor {
            task_queue: Arc::new(ArrayQueue::new(4096)),
        }
    }
}

impl Executor {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// Spawns a task onto the executor.
    pub fn spawn<T: Send>(&self, future: impl Future<Output = T> + Send) -> Task<T> {
        let (runnable, task) = unsafe { async_task::spawn_unchecked(future, self.schedule()) };
        runnable.schedule();
        task
    }

    /// Run all ready tasks then halt until other tasks ready.
    pub fn run(&self) -> ! {
        loop {
            self.run_ready_tasks();
            crate::arch::interrupt::wait_for_interrupt();
        }
    }

    fn run_ready_tasks(&self) {
        while let Some(runnable) = self.task_queue.pop() {
            runnable.run();
        }
    }

    #[inline]
    fn task_queue(&self) -> &Arc<ArrayQueue<Runnable>> {
        &self.task_queue
    }

    #[inline]
    fn schedule(&self) -> impl Fn(Runnable) + Send + Sync + 'static {
        let task_queue = self.task_queue().clone();
        move |runnable| {
            task_queue.push(runnable).unwrap();
        }
    }
}
