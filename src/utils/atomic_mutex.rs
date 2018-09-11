use core::sync::atomic::{AtomicBool, Ordering};
use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};

pub struct AtomicMutex<T> {
    lock: AtomicBool,
    value: UnsafeCell<T>,
}

unsafe impl<T: Send> Send for AtomicMutex<T> { }
unsafe impl<T: Send> Sync for AtomicMutex<T> { }

impl<T> AtomicMutex<T> {
    pub fn new(value: T) -> AtomicMutex<T> {
        AtomicMutex {
            lock: AtomicBool::new(false),
            value: UnsafeCell::new(value),
        }
    }

    pub fn try_lock(&self) -> Option<AtomicMutexGuard<T>> {
        if self.lock.compare_and_swap(false, true, Ordering::SeqCst) == false {
            Some(AtomicMutexGuard {
                lock: &self.lock,
                value: unsafe { &mut *self.value.get() },
            })
        } else {
            None
        }
    }
}

pub struct AtomicMutexGuard<'a, T: 'a> {
    lock: &'a AtomicBool,
    value: &'a mut T,
}

impl<'a, T: 'a> Deref for AtomicMutexGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.value
    }
}

impl<'a, T: 'a> DerefMut for AtomicMutexGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.value
    }
}

impl<'a, T: 'a> Drop for AtomicMutexGuard<'a, T> {
    fn drop(&mut self) {
        self.lock.store(false, Ordering::SeqCst);
    }
}
