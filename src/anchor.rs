use std::sync::atomic::Ordering;

use crate::{
    domain::{
        global::GlobalDomain,
        Domain,
    },
    hazbox::HazBox,
    hazptr::HazPtr,
    Hazard,
};

pub struct Anchor<'dom, D>
where
    D: Domain<'dom>,
{
    ptr: &'dom HazPtr,
    domain: D,
}

impl Anchor<'static, GlobalDomain> {
    #[inline]
    pub fn new() -> Self {
        // Safety: The global domain implementation is guaranteed to always return a HazPtr.
        Self {
            ptr: unsafe { GlobalDomain.acquire().unwrap_unchecked() },
            domain: GlobalDomain,
        }
    }
}

impl Default for Anchor<'static, GlobalDomain> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<'dom, D> Anchor<'dom, D>
where
    D: Domain<'dom>,
{
    #[inline]
    pub fn try_new_in(domain: D) -> Option<Self> {
        Some(Self {
            ptr: domain.acquire()?,
            domain,
        })
    }

    #[inline]
    pub fn new_in(domain: D) -> Self {
        Self::try_new_in(domain).expect("Unable to acquire a HazBox Pointer")
    }

    #[inline]
    pub fn domain(&self) -> D {
        self.domain
    }

    pub fn moor<'r, T>(&'r mut self, src: &'r HazBox<'dom, T, D>) -> &'r T
    where
        T: Hazard<'dom>,
    {
        assert!(self.domain == src.domain);

        let mut ptr = src.ptr.load(Ordering::Relaxed);
        let mut this = self;

        loop {
            match this.try_moor(src, ptr) {
                Ok(res) => return res,
                Err((next_this, next_ptr)) => {
                    this = next_this;
                    ptr = next_ptr
                }
            }
        }
    }

    pub fn try_moor<'r, T>(
        &'r mut self,
        src: &'r HazBox<'dom, T, D>,
        expected: *mut T,
    ) -> Result<&'r T, (&'r mut Self, *mut T)>
    where
        T: Hazard<'dom>,
    {
        assert!(self.domain == src.domain);

        self.ptr.protect(expected.cast());

        crate::asymmetric_fence::light();

        let actual = src.ptr.load(Ordering::Acquire);

        if expected == actual {
            // Safety:
            //  1. Target of actual will not be deallocated for the returned lifetime since
            //     our hazptr is active and pointing at it.
            //  2. Pointer address is a valid reference and not null since it was created from a HazBox.
            Ok(unsafe { &*actual })
        } else {
            self.reset();
            Err((self, actual))
        }
    }

    pub fn reset(&self) {
        self.ptr.reset();
    }
}

impl<'dom, D> Drop for Anchor<'dom, D>
where
    D: Domain<'dom>,
{
    fn drop(&mut self) {
        self.reset();
        self.ptr.release();
    }
}
