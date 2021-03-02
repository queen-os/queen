use crate::sync::spin::{MutexNoIrq, RwLock};
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
    tid: Tid,
}

