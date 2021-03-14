use async_task::Task;
use core::future::Future;

pub mod executor;
mod future;
pub mod timer;

pub use executor::Executor;
pub use future::*;
pub use timer::delay_for;

pub static GLOBAL_EXECUTOR: spin::Lazy<Executor> = spin::Lazy::new(Executor::new);

#[inline]
pub fn spawn<T: Send>(future: impl Future<Output = T> + Send) -> Task<T> {
    GLOBAL_EXECUTOR.spawn(future, 0)
}
