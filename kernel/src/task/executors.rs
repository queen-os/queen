use crate::{arch, sync::spin::MutexNoIrq};
use alloc::{collections::BTreeMap, sync::Arc};
use async_task::{Runnable, Task};
use core::{
    cmp,
    future::Future,
    ops::{self, Not},
    sync::atomic::{AtomicUsize, Ordering},
};
use priority_queue::PriorityQueue;

/// Targeted preemption latency for CPU-bound tasks:
///
/// NOTE: this latency value is not the same as the concept of
/// 'timeslice length' - timeslices in CFS are of variable length
/// and have no persistent notion like in traditional, time-slice
/// based scheduling concepts.
///
/// (to see the precise effective timeslice length of your workload,
///  run vmstat and monitor the context-switches (cs) field)
///
/// (default: 6ms  units: nanoseconds)
const SCHED_LATENCY: usize = 6_000_000;

/// SCHED_OTHER wake-up granularity.
///
/// This option delays the preemption effects of decoupled workload
/// and reduces their over-scheduling. Synchronous workloads will s
/// have immediate wakeup/sleep latencies.
/// (default: 1 msec, units: nanoseconds)
const SCHED_WAKEUP_GRANULARITY: usize = 1_000_000;

/// Minimal preemption granularity for CPU-bound tasks, units: nanoseconds.
pub const SCHED_MIN_GRANULARITY: usize = 750_000;

/// This value is kept at sysctl_sched_latency/sysctl_sched_min_granularity.
const SCHED_NR_LATENCY: usize = 8;

/// After fork, child runs first. If set to false (default) then parent will (try to) run first.
const SCHED_CHILD_RUNS_FIRST: bool = false;

/// Default task weight.
const NICE_0_WEIGHT: usize = priority_to_weight(0);

/// SchedEntity ID
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
        #[allow(clippy::while_let_loop)]
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
            tasks.current.take();
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

impl Default for Executor {
    fn default() -> Self {
        Self::new()
    }
}

pub struct State {
    tasks: MutexNoIrq<RunQueue>,
    next_tid: AtomicUsize,
}

impl State {
    pub fn new() -> Self {
        Self {
            tasks: MutexNoIrq::new(RunQueue::new()),
            next_tid: AtomicUsize::new(0),
        }
    }
}

impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}

struct RunQueue {
    run_queue: PriorityQueue<Tid, ReadyTask, hashbrown::hash_map::DefaultHashBuilder>,
    tasks: BTreeMap<Tid, SchedTask>,
    current: Option<Tid>,
    load: LoadWeight,
}

impl RunQueue {
    fn new() -> Self {
        Self {
            run_queue: PriorityQueue::with_capacity_and_default_hasher(1024),
            tasks: BTreeMap::new(),
            current: None,
            load: LoadWeight::new(0), // TODO: initial weight
        }
    }

    fn min_vruntime(&self) -> usize {
        let current_vruntime = self
            .current
            .and_then(|tid| Some(self.tasks.get(&tid)?.vruntime))
            .unwrap_or(usize::MAX);

        let next_vruntime = self
            .run_queue
            .peek()
            .map(|(_, task)| task.vruntime)
            .unwrap_or(0);

        current_vruntime.min(next_vruntime)
    }

    #[inline]
    fn nr_running(&self) -> usize {
        self.run_queue.len()
    }

    /// Pop a runnable task to run.
    fn pop_task_to_run(&mut self) -> Option<(SchedTask, Runnable)> {
        {
            let (
                next_tid,
                ReadyTask {
                    runnable: _,
                    vruntime: next_vruntime,
                },
            ) = self.run_queue.peek()?;
            let next_task = self.tasks.get(&next_tid)?;

            if let Some(current_vruntime) = self
                .current
                .and_then(|tid| Some(self.tasks.get(&tid)?.vruntime))
            {
                if current_vruntime.saturating_sub(*next_vruntime)
                    <= next_task.delta_fair(SCHED_WAKEUP_GRANULARITY)
                {
                    return None;
                }
            }
        }

        let (tid, ReadyTask { runnable, .. }) = self.run_queue.pop()?;
        self.current.replace(tid);
        self.tasks.get(&tid).map(|&task| (task, runnable))
    }

    /// Insert or update a task to ready.
    fn push_task_to_ready(&mut self, tid: usize, default_priority: isize, runnable: Runnable) {
        // TODO:
        let min_vruntime = self.min_vruntime();
        let task = self
            .tasks
            .entry(tid)
            .or_insert_with(|| SchedTask::new(tid, default_priority, min_vruntime));
        task.vruntime = task.vruntime.max(min_vruntime);
        self.run_queue
            .push(tid, ReadyTask::new(task.vruntime, runnable));
    }

    /// Increase a task's `vruntime` by `delta_exec`(microseconds).
    fn increase_vruntime(&mut self, tid: usize, delta_exec: usize) {
        let se = self.tasks.get_mut(&tid).unwrap();
        se.vruntime += se.delta_fair(delta_exec);
        self.run_queue
            .change_priority_by(&tid, |task_prio| task_prio.vruntime = se.vruntime);
    }

    /// We calculate the wall-time slice from the period by taking a part
    /// proportional to the weight.
    ///
    /// `s = p * P[w/rw]`
    fn sched_slice(&self, se: &SchedTask) -> usize {
        let period = sched_period(self.nr_running() + se.on_rq.not() as usize);
        let load = if !se.on_rq {
            self.load + se.load
        } else {
            self.load
        };
        calc_delta(period, se.load.weight, load)
    }

    /// We calculate the vruntime slice of a to-be-inserted task.
    ///
    /// `vs = s/w`
    #[inline]
    fn sched_vslice(&self, se: &SchedTask) -> usize {
        se.delta_fair(self.sched_slice(se))
    }
}

#[derive(Debug, Clone, Copy)]
struct SchedTask {
    tid: Tid,
    priority: isize,
    vruntime: usize,
    load: LoadWeight,
    on_rq: bool,
}

impl SchedTask {
    fn new(tid: Tid, priority: isize, vruntime: usize) -> Self {
        Self {
            tid,
            priority,
            vruntime,
            load: LoadWeight::new(priority_to_weight(priority)),
            on_rq: false,
        }
    }

    /// `delta /= w`
    #[inline]
    fn delta_fair(&self, delta_exec: usize) -> usize {
        if self.load.weight == NICE_0_WEIGHT {
            delta_exec
        } else {
            calc_delta(delta_exec, NICE_0_WEIGHT, self.load)
        }
    }
}

/// Reverse ordering by `vruntime`.
struct ReadyTask {
    vruntime: usize,
    runnable: Runnable,
}

impl ReadyTask {
    #[inline]
    fn new(vruntime: usize, runnable: Runnable) -> Self {
        Self { vruntime, runnable }
    }
}

impl Ord for ReadyTask {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        self.vruntime.cmp(&other.vruntime).reverse()
    }
}

impl PartialOrd for ReadyTask {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        self.vruntime
            .partial_cmp(&other.vruntime)
            .map(cmp::Ordering::reverse)
    }
}

impl PartialEq for ReadyTask {
    fn eq(&self, other: &Self) -> bool {
        self.vruntime.eq(&other.vruntime)
    }
}

impl Eq for ReadyTask {}

#[inline]
const fn priority_to_weight(priority: isize) -> usize {
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
    const PRIORITY_TO_WEIGHT: [usize; 40] = [
       /* -20 */     88761,     71755,     56483,     46273,     36291,
       /* -15 */     29154,     23254,     18705,     14949,     11916,
       /* -10 */      9548,      7620,      6100,      4904,      3906,
       /*  -5 */      3121,      2501,      1991,      1586,      1277,
       /*   0 */      1024,       820,       655,       526,       423,
       /*   5 */       335,       272,       215,       172,       137,
       /*  10 */       110,        87,        70,        56,        45,
       /*  15 */        36,        29,        23,        18,        15,
    ];

    PRIORITY_TO_WEIGHT[(priority + 20) as usize]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LoadWeight {
    weight: usize,
    __inv_weight: u32,
}

impl LoadWeight {
    const WMULT_CONST: u32 = !0;
    const WMULT_SHIFT: usize = 32;

    #[inline]
    const fn new(weight: usize) -> Self {
        let inv_weight = if weight == 0 {
            Self::WMULT_CONST
        } else if weight <= Self::WMULT_CONST as usize {
            Self::WMULT_CONST / weight as u32
        } else {
            1
        };

        LoadWeight {
            weight,
            __inv_weight: inv_weight,
        }
    }

    const fn inv_weight(self) -> u32 {
        let Self {
            weight,
            __inv_weight: inv_weight,
        } = self;
        if inv_weight != 0 {
            inv_weight
        } else if weight == 0 {
            Self::WMULT_CONST
        } else if weight <= Self::WMULT_CONST as usize {
            Self::WMULT_CONST / weight as u32
        } else {
            1
        }
    }
}

impl ops::Add<usize> for LoadWeight {
    type Output = Self;

    fn add(self, rhs: usize) -> Self::Output {
        Self::new(self.weight + rhs)
    }
}

impl ops::Add for LoadWeight {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self::new(self.weight + rhs.weight)
    }
}

/// `delta_exec * weight / lw.weight`
///   OR
/// `(delta_exec * (weight * lw->inv_weight)) >> WMULT_SHIFT`
///
/// Either `weight := NICE_0_LOAD` and lw \e sched_prio_to_wmult[], in which case
/// we're guaranteed shift stays positive because inv_weight is guaranteed to
/// fit 32 bits, and `NICE_0_LOAD` gives another 10 bits; therefore shift >= 22.
///
/// Or, `weight <= lw.weight` (because `lw.weight` is the runqueue weight), thus
/// `weight/lw.weight <= 1`, and therefore our shift will also be positive.
#[cold]
const fn calc_delta(delta_exec: usize, weight: usize, lw: LoadWeight) -> usize {
    let mut fact = weight;
    let mut shift = LoadWeight::WMULT_SHIFT;

    while (fact >> 32) != 0 {
        fact >>= 1;
        shift -= 1;
    }

    fact *= lw.inv_weight() as usize;

    while (fact >> 32) != 0 {
        fact >>= 1;
        shift -= 1;
    }

    ((delta_exec as u128 * fact as u128) >> shift) as usize
}

/// The idea is to set a period in which each task runs once.
///
/// When there are too many tasks (sched_nr_latency) we have to stretch
/// this period because otherwise the slices get too small.
///
/// `p = (nr <= nl) ? l : l*nr/nl`
#[inline]
const fn sched_period(nr_running: usize) -> usize {
    if nr_running > SCHED_NR_LATENCY {
        nr_running * SCHED_MIN_GRANULARITY
    } else {
        SCHED_LATENCY
    }
}
