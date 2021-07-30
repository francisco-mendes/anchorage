use std::{
    alloc::{
        handle_alloc_error,
        AllocError,
        Layout,
    },
    marker::PhantomData,
    mem::MaybeUninit,
    sync::atomic::{
        AtomicPtr,
        Ordering,
    },
};

use crate::{
    domain::{
        global::GlobalDomain,
        Domain,
    },
    retire::Retire,
    Hazard,
};

/// Owning atomic pointer type. Works as a mix between [`AtomicPtr<T>`] and [`Box<T>`].
///
/// [`HazBoxes`][HazBox] allocate and own the storage for a [`Hazards`][Hazard] that can be
/// [protected] by [`Anchors`][Anchor] to prevent them from being [dropped]
/// while the reference is active.
///
/// [`Hazards`][Hazard] can be replaced via [`HazBox::swap`] into a [`Retire`] that
/// holds the swapped [`Hazard`] until it is sent to the domain to be [retired].
///
/// [*currently allocated*]: Allocator#currently-allocated-memory
/// [equal]: PartialEq::eq
/// [protected]: Anchor::moor
/// [protecting]: Anchor::moor
/// [retired]: Domain::retire
///
pub struct HazBox<'dom, T, D>
where
    D: Domain<'dom>,
    T: Hazard<'dom>,
{
    pub(crate) ptr: AtomicPtr<T>,
    pub(crate) domain: D,
    __mk: PhantomData<&'dom D>,
}

impl<T> HazBox<'static, T, GlobalDomain>
where
    T: Hazard<'static>,
{
    #[inline]
    pub fn new(obj: T) -> Self {
        Self::new_in(obj, GlobalDomain)
    }
}

impl<'dom, T, D> HazBox<'dom, T, D>
where
    D: Domain<'dom>,
    T: Hazard<'dom>,
{
    pub fn try_new_in(obj: T, domain: D) -> Result<Self, AllocError> {
        let ptr = Box::try_new_in(obj, domain.allocator())?;

        Ok(Self {
            ptr: AtomicPtr::new(Box::into_raw(ptr)),
            domain,
            __mk: PhantomData,
        })
    }

    #[inline]
    pub fn new_in(obj: T, domain: D) -> Self {
        match Self::try_new_in(obj, domain) {
            Ok(haz) => haz,
            Err(_) => handle_alloc_error(Layout::new::<MaybeUninit<T>>()),
        }
    }

    #[inline]
    pub fn domain(&self) -> D {
        self.domain
    }

    #[inline]
    pub fn swap(&self, with: &mut T) -> Retire<'dom, T, D> {
        let old = self.ptr.swap(with as *mut T, Ordering::Relaxed);

        Retire::new_in(old, self.domain)
    }

    #[inline]
    pub fn set(&self, to: &mut T) {
        let _ = self.swap(to);
    }
}

impl<'dom, T, D> Drop for HazBox<'dom, T, D>
where
    D: Domain<'dom>,
    T: Hazard<'dom>,
{
    fn drop(&mut self) {
        // Safety: We own self.ptr and have exclusive access to it, thus no anchor can be protecting
        // it, thus we can just drop it here, without retiring to the domain.
        let _ = unsafe { Box::from_raw_in(self.ptr.get_mut(), self.domain.allocator()) };
    }
}
