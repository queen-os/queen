//! Reference to Linux `kernel/sched/features.h`.
use bitflags::bitflags;

bitflags! {
    #[allow(non_snake_case)]
    pub struct SchedFeatures: u32 {
        /// Only give sleepers 50% of their service deficit. This allows
        /// them to run sooner, but does not allow tons of sleepers to
        /// rip the spread apart.
        const GENTLE_FAIR_SLEEPERS = 0b0000_0001;
        /// Place new tasks ahead so that they do not starve already running tasks.
        const START_DEBIT          = 0b0000_0010;
        /// Prefer to schedule the task we woke last (assuming it failed
        /// wakeup-preemption), since its likely going to consume data we
        /// touched, increases cache locality.
        const NEXT_BUDDY           = 0b0000_0100;
        /// Prefer to schedule the task that ran last (when we did
        /// wake-preempt) as that likely will touch the same data, increases
        /// cache locality.
        const LAST_BUDDY           = 0b0000_1000;
        /// Consider buddies to be cache hot, decreases the likelyness of a
        /// cache buddy being migrated away, increases cache locality.
        const CACHE_HOT_BUDDY      = 0b0001_0000;
        /// Allow wakeup-time preemption of the current task:
        const WAKEUP_PREEMPTION    = 0b0010_0000;
    }
}

impl Default for SchedFeatures {
    fn default() -> Self {
        Self::GENTLE_FAIR_SLEEPERS
            | Self::START_DEBIT
            | Self::LAST_BUDDY
            | Self::CACHE_HOT_BUDDY
            | Self::WAKEUP_PREEMPTION
    }
}

impl SchedFeatures {
    #[inline]
    pub const fn new() -> Self {
        Self::GENTLE_FAIR_SLEEPERS
            | Self::START_DEBIT
            | Self::LAST_BUDDY
            | Self::CACHE_HOT_BUDDY
            | Self::WAKEUP_PREEMPTION
    }
}