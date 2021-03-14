use crate::{arch, sync::spin::MutexNoIrq};
use alloc::{collections::BTreeMap, sync::Arc};
use async_task::{Runnable, Task};
use core::{
    future::Future,
    sync::atomic::{AtomicUsize, Ordering},
};
use priority_queue::PriorityQueue;

/// SCHED_OTHER wake-up granularity.
/// This option delays the preemption effects of decoupled workload
/// and reduces their over-scheduling. Synchronous workloads will s
/// have immediate wakeup/sleep latencies.
/// (default: 1 msec, units: microseconds)
const SCHED_WAKEUP_GRANULARITY: usize = 1000;
/// Minimal preemption granularity for CPU-bound tasks, units: microseconds.
pub const SCHED_MIN_GRANULARITY: usize = 750;
/// Default task weight.
const NICE_0_WEIGHT: usize = 1024;

type Tid = usize;

/// An async executor using [CFS](https://en.wikipedia.org/wiki/Completely_Fair_Scheduler)-like scheduling algorithm.
pub struct Executor {
    state: Arc<State>,
}

impl Executor {
    pub fn new() -> Self {
        Self {
            state: Arc::new(State::new()),
        }
    }

    pub fn spawn<T: Send>(
        &self,
        future: impl Future<Output = T> + Send,
        priority: isize,
    ) -> Task<T> {
        let tid = self.state.next_tid.fetch_add(1, Ordering::SeqCst);
        let (runnable, task) =
            unsafe { async_task::spawn_unchecked(future, self.schedule(tid, priority)) };
        runnable.schedule();
        task
    }

    pub fn run(&self) {
        loop {
            self.run_ready_tasks();
            arch::interrupt::wait_for_interrupt();
        }
    }

    fn run_ready_tasks(&self) {
        loop {
            let (task, runnable) = match self.state.tasks.lock().pop_task_to_run() {
                Some(x) => x,
                None => break,
            };
            debug!("Task[{}] running", task.tid);

            let start = arch::timer::read();
            runnable.run();
            let end = arch::timer::read();
            let delta_exec = (end - start).as_micros() as usize;

            debug!("Task[{}] end running after {} us", task.tid, delta_exec);

            let mut tasks = self.state.tasks.lock();
            tasks.current_task.take();
            tasks.increase_vruntime(task.tid, delta_exec);
        }
    }

    #[inline]
    fn schedule(&self, tid: usize, priority: isize) -> impl Fn(Runnable) + Send + Sync + 'static {
        let state = self.state.clone();

        move |runnable| {
            debug!("Task[{}] ready", tid);
            state
                .tasks
                .lock()
                .push_task_to_ready(tid, priority, runnable);
        }
    }
}

pub struct State {
    tasks: MutexNoIrq<TaskQueue>,
    next_tid: AtomicUsize,
}

impl State {
    pub fn new() -> Self {
        Self {
            tasks: MutexNoIrq::new(TaskQueue::new()),
            next_tid: AtomicUsize::new(0),
        }
    }
}

struct TaskQueue {
    ready_tasks: PriorityQueue<Tid, TaskPriority, hashbrown::hash_map::DefaultHashBuilder>,
    tasks: BTreeMap<Tid, TaskStats>,
    current_task: Option<Tid>,
}

impl TaskQueue {
    fn new() -> Self {
        Self {
            ready_tasks: PriorityQueue::with_capacity_and_default_hasher(1024),
            tasks: BTreeMap::new(),
            current_task: None,
        }
    }

    fn min_vruntime(&self) -> usize {
        let current_vruntime = self
            .current_task
            .and_then(|tid| Some(self.tasks.get(&tid)?.vruntime))
            .unwrap_or(usize::MAX);

        let next_vruntime = self
            .ready_tasks
            .peek()
            .map(|(_, task)| task.vruntime)
            .unwrap_or(0);

        current_vruntime.min(next_vruntime)
    }

    /// Pop a runnable task to run.
    fn pop_task_to_run(&mut self) -> Option<(TaskStats, Runnable)> {
        {
            let (
                next_tid,
                TaskPriority {
                    runnable: next_runnable,
                    vruntime: next_vruntime,
                },
            ) = self.ready_tasks.peek()?;
            let next_task = self.tasks.get(&next_tid)?;

            if let Some(current_vruntime) = self
                .current_task
                .and_then(|tid| Some(self.tasks.get(&tid)?.vruntime))
            {
                if current_vruntime.saturating_sub(*next_vruntime)
                    <= calc_delta_fair(SCHED_WAKEUP_GRANULARITY, next_task)
                {
                    return None;
                }
            }
        }

        let (tid, TaskPriority { runnable, vruntime }) = self.ready_tasks.pop()?;
        self.current_task.replace(tid);
        self.tasks.get(&tid).map(|&task| (task, runnable))
    }

    /// Insert or update a task to ready.
    fn push_task_to_ready(&mut self, tid: usize, default_priority: isize, runnable: Runnable) {
        // TODO: 
        let min_vruntime = self.min_vruntime();
        let task = self
            .tasks
            .entry(tid)
            .or_insert_with(|| TaskStats::new(tid, default_priority, min_vruntime));
        task.vruntime = task.vruntime.max(min_vruntime);
        self.ready_tasks
            .push(tid, TaskPriority::new(task.vruntime, runnable));
    }

    /// Increase a task's `vruntime` by `delta_exec`(microseconds).
    fn increase_vruntime(&mut self, tid: usize, delta_exec: usize) {
        let task = self.tasks.get_mut(&tid).unwrap();
        task.vruntime += calc_delta_fair(delta_exec, task);
        self.ready_tasks
            .change_priority_by(&tid, |task_prio| task_prio.vruntime = task.vruntime);
    }
}

#[derive(Debug, Clone, Copy)]
struct TaskStats {
    tid: Tid,
    priority: isize,
    vruntime: usize,
}

impl TaskStats {
    fn new(tid: Tid, priority: isize, vruntime: usize) -> Self {
        Self {
            tid,
            priority,
            vruntime,
        }
    }
}

/// Reverse ordering by `vruntime`.
struct TaskPriority {
    vruntime: usize, // TODO: use `vruntime - min_vruntime` instead, in case of overflow.
    runnable: Runnable,
}

impl TaskPriority {
    fn new(vruntime: usize, runnable: Runnable) -> Self {
        Self { vruntime, runnable }
    }
}

impl Ord for TaskPriority {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.vruntime.cmp(&other.vruntime).reverse()
    }
}

impl PartialOrd for TaskPriority {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        self.vruntime
            .partial_cmp(&other.vruntime)
            .map(core::cmp::Ordering::reverse)
    }
}

impl PartialEq for TaskPriority {
    fn eq(&self, other: &Self) -> bool {
        self.vruntime.eq(&other.vruntime)
    }
}

impl Eq for TaskPriority {}

#[inline]
fn priority_to_weight(priority: isize) -> usize {
    /**
    Nice levels are multiplicative, with a gentle 10% change for every
    nice level changed. I.e. when a CPU-bound task goes from nice 0 to
    nice 1, it will get ~10% less CPU time than another CPU-bound task
    that remained on nice 0.

    The "10% effect" is relative and cumulative: from _any_ nice level,
    if you go up 1 level, it's -10% CPU usage, if you go down 1 level
    it's +10% CPU usage. (to achieve that we use a multiplier of 1.25.
    If a task goes up by ~10% and another task goes down by ~10% then
    the relative distance between them is ~25%.)
    */
    #[rustfmt::skip]
    static PRIORITY_TO_WEIGHT: [usize; 40] = [
       /* -20 */     88761,     71755,     56483,     46273,     36291,
       /* -15 */     29154,     23254,     18705,     14949,     11916,
       /* -10 */      9548,      7620,      6100,      4904,      3906,
       /*  -5 */      3121,      2501,      1991,      1586,      1277,
       /*   0 */      1024,       820,       655,       526,       423,
       /*   5 */       335,       272,       215,       172,       137,
       /*  10 */       110,        87,        70,        56,        45,
       /*  15 */        36,        29,        23,        18,        15,
    ];

    assert!(-20 <= priority && priority <= 20);
    PRIORITY_TO_WEIGHT[(priority + 20) as usize]
}

#[inline]
fn calc_delta_fair(delta_exec: usize, task: &TaskStats) -> usize {
    delta_exec * NICE_0_WEIGHT / priority_to_weight(task.priority)
}
