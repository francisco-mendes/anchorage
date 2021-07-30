use std::{
    alloc::Allocator,
    mem,
    ptr,
    ptr::NonNull,
    sync::atomic::Ordering,
};

use crate::{
    domain::Domain,
    hazptr::HazPtr,
    node_list::List,
    Hazard,
};

pub struct ScopedDomain<'dom, A>
where
    A: Allocator,
{
    hazptrs: List<HazPtr>,
    retired: List<NonNull<dyn Hazard<'dom>>>,
    allocator: A,
}

impl<'dom, A> ScopedDomain<'dom, A>
where
    A: Allocator,
{
    fn try_acquire_existing(&self) -> Option<&HazPtr> {
        self.hazptrs.iter().find(|hp| hp.try_acquire())
    }

    fn acquire_new(&self) -> &HazPtr {
        self.hazptrs.push_front(HazPtr::new(true))
    }

    fn retire(&self, retired: NonNull<dyn Hazard<'dom>>) {
        self.retired.push_front(retired);
    }
}

impl<'dom, A> Drop for ScopedDomain<'dom, A>
where
    A: Allocator,
{
    fn drop(&mut self) {
        let mut node_ptr = *self.retired.head.get_mut();
        while !node_ptr.is_null() {
            // Safety: The hazard and node were allocated using self.allocator by a Box.
            unsafe {
                let mut node = Box::from_raw_in(node_ptr, &self.allocator);
                let _ = Box::from_raw_in(node.value.as_ptr(), &self.allocator);
                node_ptr = *node.next.get_mut();
            }
        }
        let mut node_ptr = *self.hazptrs.head.get_mut();
        while !node_ptr.is_null() {
            // Safety: The node with the hazptr was allocated using self.allocator by a Box.
            unsafe {
                node_ptr = *Box::from_raw_in(node_ptr, &self.allocator).next.get_mut();
            }
        }
    }
}

pub struct ScopedDomainRef<'dom, A>(&'dom ScopedDomain<'dom, A>)
where
    A: Allocator;

impl<'dom, A> Eq for ScopedDomainRef<'dom, A> where A: Allocator {}

impl<'dom, A> Copy for ScopedDomainRef<'dom, A> where A: Allocator {}

impl<'dom, A> PartialEq for ScopedDomainRef<'dom, A>
where
    A: Allocator,
{
    fn eq(&self, other: &Self) -> bool {
        ptr::eq(self.0, other.0)
    }
}

impl<'dom, A> Clone for ScopedDomainRef<'dom, A>
where
    A: Allocator,
{
    fn clone(&self) -> Self {
        *self
    }
}

unsafe impl<'dom, A> Domain<'dom> for ScopedDomainRef<'dom, A>
where
    A: Allocator,
{
    type Alloc = A;

    #[inline]
    fn allocator(self) -> &'dom Self::Alloc {
        &self.0.allocator
    }

    fn acquire(self) -> Option<&'dom HazPtr> {
        self.0
            .try_acquire_existing()
            .or_else(|| Some(self.0.acquire_new()))
    }

    unsafe fn retire(self, retired: NonNull<dyn Hazard<'dom>>) {
        self.0.retire(retired)
    }
}
