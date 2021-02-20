use core::{
    fmt,
    ops::{Deref, DerefMut},
};
use spin::MutexGuard;

pub type Mutex<T> = spin::Mutex<T>;

pub struct MutexNoIrq<T>(Mutex<T>);

unsafe impl<T: Send> Sync for MutexNoIrq<T> {}
unsafe impl<T: Send> Send for MutexNoIrq<T> {}

impl<T> MutexNoIrq<T> {
    #[inline]
    pub const fn new(value: T) -> Self {
        Self(Mutex::new(value))
    }

    #[inline]
    pub fn into_inner(self) -> T {
        self.0.into_inner()
    }
}

impl<T> MutexNoIrq<T> {
    /// Returns `true` if the lock is currently held.
    ///
    /// # Safety
    ///
    /// This function provides no synchronization guarantees and so its result should be considered 'out of date'
    /// the instant it is called. Do not use it for synchronization purposes. However, it may be useful as a heuristic.
    #[inline]
    pub fn is_locked(&self) -> bool {
        self.0.is_locked()
    }

    /// Locks the [`Mutex`] and returns a guard that permits access to the inner data.
    ///
    /// The returned value may be dereferenced for data access
    /// and the lock will be dropped when the guard falls out of scope.
    #[inline]
    pub fn lock(&self) -> MutexGuardNoIrq<T> {
        MutexGuardNoIrq::new(self.0.lock())
    }

    /// Force unlock this [`Mutex`].
    ///
    /// # Safety
    ///
    /// This is *extremely* unsafe if the lock is not held by the current
    /// thread. However, this can be useful in some instances for exposing the
    /// lock to FFI that doesn't know how to deal with RAII.
    #[inline]
    pub unsafe fn force_unlock(&self) {
        self.0.force_unlock()
    }

    /// Try to lock this [`Mutex`], returning a lock guard if successful.
    #[inline]
    pub fn try_lock(&self) -> Option<MutexGuardNoIrq<T>> {
        self.0.try_lock().map(|guard| MutexGuardNoIrq::new(guard))
    }

    /// Returns a mutable reference to the underlying data.
    ///
    /// Since this call borrows the [`Mutex`] mutably, and a mutable reference is guaranteed to be exclusive in Rust,
    /// no actual locking needs to take place -- the mutable borrow statically guarantees no locks exist. As such,
    /// this is a 'zero-cost' operation.
    #[inline]
    pub fn get_mut(&mut self) -> &mut T {
        self.0.get_mut()
    }
}

impl<T: fmt::Debug> fmt::Debug for MutexNoIrq<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

impl<T: Default> Default for MutexNoIrq<T> {
    fn default() -> MutexNoIrq<T> {
        Self::new(Default::default())
    }
}

impl<T> From<T> for MutexNoIrq<T> {
    fn from(data: T) -> Self {
        Self::new(data)
    }
}

pub struct MutexGuardNoIrq<'a, T: 'a> {
    inner: MutexGuard<'a, T>,
    flags: usize,
}

impl<'a, T: 'a> MutexGuardNoIrq<'a, T> {
    fn new(inner: MutexGuard<'a, T>) -> Self {
        let flags = unsafe { crate::arch::interrupt::disable_and_store() };
        MutexGuardNoIrq { inner, flags }
    }
}

impl<'a, T: 'a> Drop for MutexGuardNoIrq<'a, T> {
    fn drop(&mut self) {
        unsafe { crate::arch::interrupt::restore(self.flags) }
    }
}

impl<'a, T: fmt::Debug> fmt::Debug for MutexGuardNoIrq<'a, T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}

impl<'a, T: fmt::Display> fmt::Display for MutexGuardNoIrq<'a, T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&**self, f)
    }
}

impl<'a, T: 'a> Deref for MutexGuardNoIrq<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.inner.deref()
    }
}

impl<'a, T: 'a> DerefMut for MutexGuardNoIrq<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.inner.deref_mut()
    }
}
