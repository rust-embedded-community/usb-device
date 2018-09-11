use core::sync::atomic::{AtomicUsize, Ordering};
use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};

const FREE: usize = 0;
const BORROW_MUT: usize = 1;
const FROZEN: usize = 2;

pub struct FreezableRefCell<T> {
    state: AtomicUsize,
    value: UnsafeCell<T>,
}

unsafe impl<T: Send> Send for FreezableRefCell<T> { }
unsafe impl<T: Send> Sync for FreezableRefCell<T> { }

impl<T> FreezableRefCell<T> {
    pub fn new(value: T) -> FreezableRefCell<T> {
        FreezableRefCell {
            state: AtomicUsize::new(FREE),
            value: UnsafeCell::new(value),
        }
    }

    pub fn default() -> FreezableRefCell<T> where T: Default {
        FreezableRefCell::new(Default::default())
    }

    // Disable inlining to work around LLVM bug
    #[inline(never)]
    pub fn borrow_mut(&self) -> RefMut<T> {
        if self.state.compare_and_swap(
            FREE,
            BORROW_MUT,
            Ordering::SeqCst) != FREE
        {
            panic!("cell not mutably borrowable");
        }

        RefMut {
            state: &self.state,
            value: unsafe { &mut *self.value.get() },
        }
    }

    // Disable inlining to work around LLVM bug
    #[inline(never)]
    pub fn freeze(&self) {
        if self.state.compare_and_swap(
            FREE,
            FROZEN,
            Ordering::SeqCst) != FREE
        {
            panic!("cell not freezable");
        }
    }

    // Disable inlining to work around LLVM bug
    #[inline(never)]
    pub fn borrow(&self) -> &T {
        if self.state.load(Ordering::SeqCst) != FROZEN {
            panic!("cell not frozen")
        }

        unsafe { &*self.value.get() }
    }
}

pub struct RefMut<'a, T: 'a> {
    state: &'a AtomicUsize,
    value: &'a mut T,
}

impl<'a, T: 'a> Deref for RefMut<'a, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.value
    }
}

impl<'a, T: 'a> DerefMut for RefMut<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.value
    }
}

impl<'a, T: 'a> Drop for RefMut<'a, T> {
    fn drop(&mut self) {
        self.state.store(FREE, Ordering::SeqCst);
    }
}
