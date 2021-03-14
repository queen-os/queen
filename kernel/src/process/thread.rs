use super::{Process, PID_INIT};
use crate::{
    memory::MemorySet,
    sync::spin::{MutexNoIrq, RwLock},
};
use aarch64::trap::UserContext;
use alloc::{boxed::Box, collections::BTreeMap, sync::Arc};

pub type Tid = usize;

pub static THREADS: RwLock<BTreeMap<Tid, Arc<Thread>>> = RwLock::new(BTreeMap::new());

/// Mutable part of a thread struct
#[derive(Default)]
struct ThreadInner {
    context: Option<Box<UserContext>>,
}

pub struct Thread {
    inner: MutexNoIrq<ThreadInner>,
    process: Arc<MutexNoIrq<Process>>,
    vm: Arc<MutexNoIrq<MemorySet>>,
    pub tid: Tid,
}

impl Thread {
    /// Assign a tid and put itself to global thread table.
    pub fn add_to_table(mut self) -> Arc<Self> {
        let mut thread_table = THREADS.write();

        // assign tid, do not start from 0
        let tid = (PID_INIT..)
            .find(|i| thread_table.get(i).is_none())
            .unwrap();
        self.tid = tid;

        // put to thread table
        let self_ref = Arc::new(self);
        thread_table.insert(tid, self_ref.clone());

        self_ref
    }
}
