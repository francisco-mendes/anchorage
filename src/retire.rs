use std::{
    marker::PhantomData,
    mem::needs_drop,
    ops::Deref,
    ptr::NonNull,
};

use crate::{
    domain::Domain,
    Hazard,
};

pub struct Retire<'dom, T, D>
where
    D: Domain<'dom>,
    T: Hazard<'dom>,
{
    ptr: NonNull<T>,
    domain: D,
    __mk: PhantomData<&'dom D>,
}

impl<'dom, T, D> Retire<'dom, T, D>
where
    D: Domain<'dom>,
    T: Hazard<'dom>,
{
    #[inline]
    pub(crate) fn new_in(obj: *mut T, domain: D) -> Self {
        // Safety: old was kept by this HazBox, so it is both non null and a valid reference to T.
        Self {
            ptr: unsafe { NonNull::new_unchecked(obj) },
            domain,
            __mk: PhantomData,
        }
    }
}

impl<'dom, T, D> Deref for Retire<'dom, T, D>
where
    D: Domain<'dom>,
    T: Hazard<'dom>,
{
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        // Safety:
        // This pointer is well aligned, points to a valid T, no exclusive
        // reference exists to it and its pointee will outlive it because of the lifetime inferred
        // from this function's signature.
        unsafe { self.ptr.as_ref() }
    }
}

impl<'dom, T, D> Drop for Retire<'dom, T, D>
where
    D: Domain<'dom>,
    T: Hazard<'dom>,
{
    fn drop(&mut self) {
        if needs_drop::<T>() {
            // Safety: T is a Hazard, thus nothing in it can dangle from its destructor,
            // for the lifetime 'dom.
            unsafe { self.domain.retire(self.ptr) }
        }
    }
}
