use core::future::Future;

pub mod executor;
mod future;
pub mod timer;

pub use executor::{Executor, local_executor, Task, SchedTaskRef};
pub use future::*;
pub use timer::delay_for;

#[inline]
pub fn init(cpu_count: usize) {
    executor::init(cpu_count);
    info!("Initialized completely fair task scheduler.");
}

#[inline]
pub fn spawn(future: impl Future<Output = ()> + Send) -> Task {
    executor::local_executor()
        .spawn(future, 0, Default::default())
        .0
}
