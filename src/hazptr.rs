use std::{
    ptr,
    sync::atomic::{
        AtomicBool,
        AtomicPtr,
        Ordering,
    },
};

pub struct HazPtr {
    ptr: AtomicPtr<u8>,
    active: AtomicBool,
}

impl HazPtr {
    #[inline]
    pub const fn new(active: bool) -> Self {
        Self {
            ptr: AtomicPtr::new(ptr::null_mut()),
            active: AtomicBool::new(active),
        }
    }

    #[inline]
    pub fn ptr(&self) -> *mut u8 {
        self.ptr.load(Ordering::Acquire)
    }

    #[inline]
    pub fn reset(&self) {
        self.ptr.store(std::ptr::null_mut(), Ordering::Release);
    }

    #[inline]
    pub fn protect(&self, ptr: *mut u8) {
        self.ptr.store(ptr, Ordering::Release);
    }

    #[inline]
    pub fn release(&self) {
        self.active.store(false, Ordering::Release);
    }

    #[inline]
    pub fn try_acquire(&self) -> bool {
        let active = self.active.load(Ordering::Acquire);
        !active
            && self
                .active
                .compare_exchange(active, true, Ordering::Release, Ordering::Relaxed)
                .is_ok()
    }
}
