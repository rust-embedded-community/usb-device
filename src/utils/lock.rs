use core::sync::atomic::{AtomicBool, Ordering};

struct Lock(AtomicBool);

impl Lock {
    pub fn new() -> Lock {
        Lock(AtomicBool::new(false))
    }

    pub fn try_lock(&self) -> Option<LockGuard> {
        if self.0.compare_and_swap(false, true, Ordering::SeqCst) == false {
            Some(LockGuard(&self))
        } else {
            None
        }
    }
}

struct LockGuard<'a>(&'a AtomicBool);

impl Drop for LockGuard {

}