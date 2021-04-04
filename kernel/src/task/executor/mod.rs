use crate::{
    arch,
    sync::spin::{Mutex, MutexGuard, MutexNoIrq, RwLock},
};
use ahash::RandomState;
use alloc::sync::Arc;
use async_task::Runnable;
use core::{
    cmp,
    future::Future,
    mem,
    num::NonZeroU32,
    ops::{self, Not},
};
use priority_queue::PriorityQueue;
use smallvec::SmallVec;
use spin::{Lazy, Once};
use vec_arena::Arena;

pub mod features;
pub use features::SchedFeatures;

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
const NICE_0_WEIGHT: usize = nice_to_weight(0);

static SCHED_FEAT: Lazy<SchedFeatures> = Lazy::new(SchedFeatures::new);

/// Higher nice value means lower priority.
pub const MAX_NICE: isize = 19;
pub const MIN_NICE: isize = -20;

/// SchedEntity ID
type Tid = usize;
type Task = async_task::Task<()>;
type ExecutorVec = SmallVec<[Executor; 16]>;
type RunQueueRef = Arc<MutexNoIrq<RunQueue>>;
pub type SchedTaskRef = Arc<Mutex<SchedTask>>;

static GLOBAL_STATE: GlobalState = GlobalState::new();

/// Must call this firstly.
pub fn init(cpu_count: usize) {
    GLOBAL_STATE
        .executors
        .call_once(|| (0..cpu_count).map(|_| Executor::new()).collect());
}

#[inline]
fn global_state() -> &'static GlobalState {
    &GLOBAL_STATE
}

/// Get the local executor for this CPU.
/// Must call after called `init(cpu_count)`.
#[inline]
pub fn local_executor() -> &'static Executor {
    global_state().executor(crate::cpu::id())
}

struct GlobalState {
    active_tasks: Lazy<RwLock<Arena<SchedTaskRef>>>,
    executors: Once<ExecutorVec>,
}

impl GlobalState {
    const fn new() -> Self {
        GlobalState {
            active_tasks: Lazy::new(Default::default),
            executors: Once::new(),
        }
    }

    #[inline]
    fn task(&self, tid: Tid) -> Option<SchedTaskRef> {
        self.active_tasks.read().get(tid).cloned()
    }

    #[inline]
    fn executor(&self, cpu_id: usize) -> &Executor {
        unsafe { &self.executors.get_unchecked()[cpu_id] }
    }

    #[inline]
    fn remove_task(&self, tid: usize) -> Option<SchedTaskRef> {
        let mut tasks = self.active_tasks.write();
        tasks.remove(tid)
    }

    #[inline]
    fn other_run_queues(&self, current_cpu_id: usize) -> SmallVec<[RunQueueRef; 16]> {
        let executors = unsafe { self.executors.get_unchecked() };
        let len = executors.len();
        (current_cpu_id + 1..len)
            .chain(0..current_cpu_id)
            .map(|i| executors[i].run_queue.clone())
            .collect()
    }
}

pub enum SpawnExtraOptions {
    None,
    Fork { parent_sched_task: SchedTaskRef },
}

impl SpawnExtraOptions {
    #[inline]
    pub fn none() -> Self {
        Self::None
    }

    #[inline]
    pub fn fork(parent_sched_task: SchedTaskRef) -> Self {
        Self::Fork { parent_sched_task }
    }
}

impl Default for SpawnExtraOptions {
    fn default() -> Self {
        Self::None
    }
}

pub struct Executor {
    run_queue: RunQueueRef,
}

impl Executor {
    #[inline]
    fn new() -> Self {
        let executor = Executor {
            run_queue: Arc::new(MutexNoIrq::new(RunQueue::new())),
        };

        let (idle_task, _) = executor.spawn(idle_task(), MAX_NICE, SpawnExtraOptions::none());
        idle_task.detach();

        executor
    }

    pub fn spawn(
        &self,
        future: impl Future<Output = ()> + Send,
        nice: isize,
        extra_options: SpawnExtraOptions,
    ) -> (Task, SchedTaskRef) {
        let mut active_tasks = global_state().active_tasks.write();
        let tid = active_tasks.next_vacant();
        let mut run_queue = self.run_queue.lock();

        let vruntime = match &extra_options {
            SpawnExtraOptions::None => run_queue.min_vruntime,
            SpawnExtraOptions::Fork { parent_sched_task } => parent_sched_task.lock().vruntime,
        };

        let mut sched_task = SchedTask::new(tid, nice, self.run_queue.clone(), vruntime);

        // The 'current' period is already promised to the current tasks,
        // however the extra weight of the new task will slow them down a
        // little, place the new task so that it fits in the slot that
        // stays open at the end.
        if SCHED_FEAT.contains(SchedFeatures::START_DEBIT) {
            sched_task.vruntime = sched_task
                .vruntime
                .max(run_queue.min_vruntime + run_queue.sched_vslice(&sched_task));
        }

        if SCHED_CHILD_RUNS_FIRST {
            if let SpawnExtraOptions::Fork { parent_sched_task } = extra_options {
                let mut parent_task = parent_sched_task.lock();
                if parent_task.vruntime < sched_task.vruntime {
                    mem::swap(&mut parent_task.vruntime, &mut sched_task.vruntime);
                    // TODO: is it possible that parent in rq already?
                }
            }
        }

        let vruntime = sched_task.vruntime;
        let load = sched_task.load;
        let sched_task = Arc::new(Mutex::new(sched_task));

        let future = {
            let sched_task = sched_task.clone();
            async move {
                let _guard = CallOnDrop(move || {
                    let mut sched_task = sched_task.lock();
                    sched_task.on_rq = false;
                    let run_queue = sched_task.run_queue.clone();
                    let mut run_queue = run_queue.lock();
                    run_queue.remove_task(sched_task);
                    global_state().remove_task(tid);
                });
                future.await
            }
        };
        let (runnable, task) = unsafe {
            async_task::spawn_unchecked(future, SchedTask::schedule_fn(sched_task.clone()))
        };

        active_tasks.insert(sched_task.clone());
        run_queue.insert_task(tid, ReadyTask::new(vruntime, runnable), load);
        sched_task.lock().on_rq = true;
        trace!("Task[{}] spawned", tid);

        (task, sched_task)
    }

    #[inline]
    pub fn run(&self) {
        loop {
            self.tick();
        }
    }

    fn tick(&self) {
        let run_queue = self.run_queue.clone();
        loop {
            let (tid, task, runnable) = run_queue.lock().pop_task_to_run();
            trace!("Task[{}] run", tid);
            let is_yielded = runnable.run();
            let mut run_queue = run_queue.lock();
            // if it not yielded then remove it.
            if !is_yielded {
                run_queue.remove_task(task.lock());
            }
            run_queue.task_tick(task.lock());
        }
    }
}

struct RunQueue {
    ready_tasks: PriorityQueue<Tid, ReadyTask, RandomState>,
    current_task: Option<(Tid, SchedTaskRef)>,
    load: LoadWeight,
    min_vruntime: VRuntime,
    nr_running: usize,
}

impl RunQueue {
    #[inline]
    fn new() -> Self {
        RunQueue {
            ready_tasks: PriorityQueue::with_default_hasher(),
            current_task: None,
            load: LoadWeight::new(0),
            min_vruntime: VRuntime(0),
            nr_running: 0,
        }
    }

    /// We calculate the wall-time slice from the period by taking a part
    /// proportional to the weight.
    ///
    /// `s = p * P[w/rw]`
    fn sched_slice(&self, task: &SchedTask) -> usize {
        let period = sched_period(self.nr_running + task.on_rq.not() as usize);
        let load = if !task.on_rq {
            self.load + task.load
        } else {
            self.load
        };
        calc_delta(period, task.load.weight, load)
    }

    /// We calculate the vruntime slice of a to-be-inserted task.
    ///
    /// `vs = s/w`
    #[inline]
    fn sched_vslice(&self, task: &SchedTask) -> usize {
        task.delta_fair(self.sched_slice(task))
    }

    fn insert_task(&mut self, tid: Tid, task: ReadyTask, load: LoadWeight) {
        if self
            .current_task
            .as_ref()
            .map(|(current_tid, _)| *current_tid != tid)
            .unwrap_or(true)
        {
            self.nr_running += 1;
            self.load += load;
        }
        self.ready_tasks.push(tid, task);

        trace!("Task[{}] inserted", tid);
    }

    #[inline]
    fn remove_task(&mut self, mut task: MutexGuard<SchedTask>) {
        task.on_rq = false;
        let contained = self.ready_tasks.remove(&task.tid).is_some();
        let contained = contained
            || self
                .current_task
                .as_ref()
                .map(|(tid, _)| *tid == task.tid)
                .unwrap_or(false);
        if contained {
            trace!("Task[{}] removed", task.tid);

            self.nr_running -= 1;
            self.load -= task.load;
            drop(task);
            self.update_min_vruntime();
        }
    }

    #[inline]
    fn peek(&self) -> (&Tid, &ReadyTask) {
        // idle task always ready.
        self.ready_tasks.peek().unwrap()
    }

    fn pop_task_to_run(&mut self) -> (Tid, SchedTaskRef, Runnable) {
        if self.nr_running < 2 {
            self.try_steal_tasks();
        }

        let next_tid = if self.nr_running < 2 {
            *self.peek().0
        } else if let Some((current_task_tid, current_task)) = self.current_task.clone() {
            let current_task = current_task.lock();
            let ideal_runtime = self.sched_slice(&current_task);
            let delta_exec = current_task.sum_exec_runtime - current_task.prev_sum_exec_runtime;
            let preempt_current = if !current_task.on_rq || delta_exec > ideal_runtime {
                true
                // TODO: clear buddies
            } else if delta_exec < SCHED_MIN_GRANULARITY {
                // Ensure that a task that missed wakeup preemption by a
                // narrow margin doesn't have to wait for a full slice.
                // This also mitigates buddy induced latencies under load.
                false
            } else {
                let (_, next_task) = self.peek();
                let delta = current_task.vruntime.delta(next_task.vruntime);

                delta > ideal_runtime as isize
            };
            if preempt_current {
                let current = self.ready_tasks.remove(&current_task_tid);
                let (&next_tid, _) = self.ready_tasks.peek().unwrap();
                if let Some(current) = current {
                    self.ready_tasks.push(current.0, current.1);
                }
                next_tid
            } else {
                current_task_tid
            }
        } else {
            *self.peek().0
        };

        let runnable = self.ready_tasks.remove(&next_tid).unwrap().1.runnable;
        let task = global_state().task(next_tid).unwrap();
        task.lock().exec_start = arch::timer::read_ns() as usize;

        (next_tid, task, runnable)
    }

    fn task_tick(&mut self, mut task: MutexGuard<SchedTask>) {
        task.tick();
        self.ready_tasks
            .change_priority_by(&task.tid, |t| t.vruntime = task.vruntime);
        drop(task);
        self.update_min_vruntime();
    }

    fn update_min_vruntime(&mut self) {
        let mut vruntime = self.min_vruntime;
        if let Some((_, current_task)) = self.current_task.clone() {
            let current_task = current_task.lock();
            if current_task.on_rq {
                vruntime = current_task.vruntime;
            }
        }
        if let Some((_, next)) = self.ready_tasks.peek() {
            if self.current_task.is_none() {
                vruntime = next.vruntime;
            } else {
                vruntime = vruntime.min(next.vruntime);
            }
        }
        // ensure we never gain time by being placed backwards.
        self.min_vruntime = self.min_vruntime.max(vruntime);
    }

    #[inline]
    fn is_current_task(&self, tid: Tid) -> bool {
        self.current_task
            .as_ref()
            .map(|(current_tid, _)| *current_tid == tid)
            .unwrap_or(false)
    }

    fn try_steal_tasks(&mut self) {
        let other_run_queues = global_state().other_run_queues(crate::cpu::id());
        let self_ref = global_state().executor(crate::cpu::id()).run_queue.clone();
        if let Some(mut rq) = other_run_queues
            .iter()
            .find_map(|rq| rq.try_lock().filter(|rq| rq.nr_running > 2))
        {
            let mut count = rq.ready_tasks.len() / 2;
            let mut task_to_push_back = None;
            while count != 0 {
                count -= 1;
                let (tid, ready_task) = rq.ready_tasks.pop().unwrap();
                if rq.is_current_task(tid) {
                    task_to_push_back = Some((tid, ready_task));
                    continue;
                }
                let task = global_state().task(tid).unwrap();
                if let Some(mut task) = task.try_lock() {
                    task.vruntime -= rq.min_vruntime;
                    task.vruntime += self.min_vruntime;
                    self.ready_tasks.push(tid, ready_task);
                    task.run_queue = self_ref.clone();
                };
            }
            if let Some((tid, task)) = task_to_push_back {
                rq.ready_tasks.push(tid, task);
            }
        };
    }
}

pub struct SchedTask {
    tid: Tid,
    load: LoadWeight,
    pub nice: isize,
    on_rq: bool,
    run_queue: RunQueueRef,

    exec_start: usize,
    sum_exec_runtime: usize,
    prev_sum_exec_runtime: usize,
    vruntime: VRuntime,
}

impl SchedTask {
    fn new(tid: Tid, nice: isize, run_queue: RunQueueRef, vruntime: VRuntime) -> Self {
        SchedTask {
            tid,
            load: LoadWeight::new(nice_to_weight(nice)),
            nice,
            on_rq: false,
            run_queue,
            exec_start: 0,
            sum_exec_runtime: 0,
            prev_sum_exec_runtime: 0,
            vruntime,
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

    fn schedule_fn(task: SchedTaskRef) -> impl Fn(Runnable) + Send + Sync + 'static {
        move |runnable: Runnable| {
            let mut task = task.lock();
            task.on_rq = true;

            let thresh = if SCHED_FEAT.contains(SchedFeatures::GENTLE_FAIR_SLEEPERS) {
                // Halve their sleep time's effect, to allow for a gentler effect of sleepers
                SCHED_LATENCY >> 1
            } else {
                SCHED_LATENCY
            };

            let run_queue = task.run_queue.clone();
            let mut run_queue = run_queue.lock();

            // sleeps up to a single latency don't count.
            let vruntime = run_queue.min_vruntime - thresh;
            // ensure we never gain time by being placed backwards.
            task.vruntime = task.vruntime.max(vruntime);

            let ready_task = ReadyTask::new(vruntime, runnable);
            run_queue.insert_task(task.tid, ready_task, task.load);
        }
    }

    fn tick(&mut self) {
        let now = arch::timer::read_ns() as usize;
        let delta_exec = now - self.exec_start;
        self.exec_start = now;
        self.sum_exec_runtime += delta_exec;
        self.vruntime += self.delta_fair(delta_exec);
    }
}

/// Reverse ordering by `vruntime`.
struct ReadyTask {
    vruntime: VRuntime,
    runnable: Runnable,
}

impl ReadyTask {
    #[inline]
    fn new(vruntime: VRuntime, runnable: Runnable) -> Self {
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

/// Virtual runtime dealing with overflow.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[repr(C)]
struct VRuntime(usize);

impl VRuntime {
    fn delta(self, other: Self) -> isize {
        self.0 as isize - other.0 as isize
    }
}

impl PartialOrd for VRuntime {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        (self.0 as isize - other.0 as isize).partial_cmp(&0)
    }
}

impl Ord for VRuntime {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        (self.0 as isize - other.0 as isize).cmp(&0)
    }
}

impl From<usize> for VRuntime {
    fn from(x: usize) -> Self {
        VRuntime(x)
    }
}

impl From<VRuntime> for usize {
    fn from(v: VRuntime) -> Self {
        v.0
    }
}

impl ops::Add<usize> for VRuntime {
    type Output = Self;

    fn add(self, rhs: usize) -> Self::Output {
        VRuntime(self.0 + rhs)
    }
}

impl ops::Add for VRuntime {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        VRuntime(self.0 + rhs.0)
    }
}

impl ops::AddAssign for VRuntime {
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
    }
}

impl ops::AddAssign<usize> for VRuntime {
    fn add_assign(&mut self, rhs: usize) {
        self.0 += rhs;
    }
}

impl ops::Sub<usize> for VRuntime {
    type Output = Self;

    fn sub(self, rhs: usize) -> Self::Output {
        VRuntime(self.0 - rhs)
    }
}

impl ops::Sub for VRuntime {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        VRuntime(self.0 - rhs.0)
    }
}

impl ops::SubAssign for VRuntime {
    fn sub_assign(&mut self, rhs: Self) {
        self.0 -= rhs.0;
    }
}

impl ops::SubAssign<usize> for VRuntime {
    fn sub_assign(&mut self, rhs: usize) {
        self.0 -= rhs;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LoadWeight {
    weight: usize,
    inv_weight: Option<NonZeroU32>,
}

impl LoadWeight {
    const WMULT_CONST: u32 = !0;
    const WMULT_SHIFT: usize = 32;

    #[inline]
    const fn new(weight: usize) -> Self {
        LoadWeight {
            weight,
            inv_weight: None,
        }
    }

    const fn inv_weight(self) -> u32 {
        let Self { weight, inv_weight } = self;

        if let Some(inv_weight) = inv_weight {
            inv_weight.get()
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

impl ops::AddAssign for LoadWeight {
    fn add_assign(&mut self, rhs: Self) {
        *self = Self::new(self.weight + rhs.weight);
    }
}

impl ops::Sub<usize> for LoadWeight {
    type Output = Self;

    fn sub(self, rhs: usize) -> Self::Output {
        Self::new(self.weight - rhs)
    }
}

impl ops::Sub for LoadWeight {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self::new(self.weight - rhs.weight)
    }
}

impl ops::SubAssign for LoadWeight {
    fn sub_assign(&mut self, rhs: Self) {
        *self = Self::new(self.weight - rhs.weight);
    }
}

#[inline]
const fn nice_to_weight(nice: isize) -> usize {
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
    const NICE_TO_WEIGHT: [usize; 40] = [
       /* -20 */     88761,     71755,     56483,     46273,     36291,
       /* -15 */     29154,     23254,     18705,     14949,     11916,
       /* -10 */      9548,      7620,      6100,      4904,      3906,
       /*  -5 */      3121,      2501,      1991,      1586,      1277,
       /*   0 */      1024,       820,       655,       526,       423,
       /*   5 */       335,       272,       215,       172,       137,
       /*  10 */       110,        87,        70,        56,        45,
       /*  15 */        36,        29,        23,        18,        15,
    ];

    NICE_TO_WEIGHT[(nice + 20) as usize]
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

#[inline]
async fn idle_task() {
    loop {
        super::yield_now().await;
        crate::cpu::halt();
    }
}

/// Runs a closure when dropped.
struct CallOnDrop<F: Fn()>(F);

impl<F: Fn()> Drop for CallOnDrop<F> {
    fn drop(&mut self) {
        (self.0)();
    }
}
