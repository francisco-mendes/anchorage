use std::{
    alloc::Allocator,
    ptr::NonNull,
};

use crate::{
    hazptr::HazPtr,
    Hazard,
};

pub mod global;
pub mod scoped;

/// Owns a set of [`HazPtrs`][HazPtr] to prevent [`Hazards`][Hazard] from being dropped, and retires
/// said [`Hazards`][Hazard] when they are no longer protected by any [`HazPtr`] from this domain.
///
/// # Safety
///
/// * Implementations of this trait must ensure that any two [equal] objects implementing this
/// trait must be completely equivalent, i.e. where one is used so can the other, without
/// resulting in any memory unsafety or undefined behaviour.
///
/// * [`Domain::allocator`] must always return a reference to the same allocator.
/// This ensures that any storage held by a [`HazBox`] using this domain is [*currently allocated*]
/// by it and can thus be safely [retired] and deallocated by this domain later on.
///
/// * [`Domain::acquire`] must [acquire] and return a pointer to an unused [`HazPtr`] owned by
/// this domain or return [None].
/// This ensures that [`Anchors`][Anchor] can assume that the [`HazPtr`] can safely be cast to
/// a shared reference and is not being used concurrently by another [`Anchor`].
///
/// * [`Hazards`][Hazard] that are validly [retired] to this domain must not be [dropped]
/// if any active [`HazPtr`] owned by this domain is protecting it.
/// This is a base requirement for HazBox Pointers, which ensures the memory safety of
/// the algorithm.
///
/// # Notes
///
/// To ensure wait freedom, implementations of [`Domain`] must ensure that the implementations of
/// [`PartialEq`] for this are also wait free since [`Anchors`][Anchor] have to check equality when
/// [protecting] [`HazBoxes`][HazBox].
///
/// Implementations of [`PartialEq`] and [`Clone`] for [`Domains`][Domain] should ensure that clones
/// and **copies** are [equal] to the original. Otherwise [`Anchors`][Anchor] created using a clone
/// of the domain of a [`HazBox`] will end up panicking when [protecting] said [`HazBox`].
///
/// [*currently allocated*]: Allocator#currently-allocated-memory
/// [acquire]: HazPtr::try_acquire
/// [acquired]: HazPtr::try_acquire
/// [dropped]: Drop::drop
/// [equal]: PartialEq::eq
/// [protecting]: Anchor::moor
/// [retired]: Domain::retire
///
pub unsafe trait Domain<'dom>: Copy + Eq + 'dom {
    ///
    /// The allocator used to allocate and deallocate storage for protected [`Hazards`][Hazard].
    ///
    /// * Used by [`HazBox`] to allocate storage.
    /// * Used to deallocate storage for [retired] [`Hazards`][Hazard].
    /// * May be used to allocate storage for the [`HazPtrs`][HazPtr] owned by this domain.
    ///
    /// [*currently allocated*]: Allocator#currently-allocated-memory
    /// [acquire]: HazPtr::try_acquire
    /// [acquired]: HazPtr::try_acquire
    /// [dropped]: Drop::drop
    /// [equal]: PartialEq::eq
    /// [protecting]: Anchor::moor
    /// [retired]: Domain::retire
    ///
    type Alloc: Allocator;

    /// Returns a reference to the underlying allocator.
    ///
    /// # Implementation Safety
    ///
    /// * Any storage allocated by this allocator must also be able to be deallocated by it, i.e.
    /// it must be safe to call [`Allocator::deallocate`] on storage allocated by a [`HazBox`] that
    /// uses this domain.
    ///
    fn allocator(self) -> &'dom Self::Alloc;

    /// Acquires a [`HazPtr`] and returns it.
    /// Returns [None] if no free [`HazPtr`] was found and none could have been created.
    ///
    /// # Implementation Safety
    ///
    /// * The returned [`HazPtr`] must not already be in use, i.e. it must be
    /// [acquired] and [`HazPtr::try_acquire`] must return true in the implementation.
    ///
    /// # Notes
    ///
    /// * May or may not create a new [`HazPtr`], depending on the implementation.
    ///
    /// [*currently allocated*]: Allocator#currently-allocated-memory
    /// [acquire]: HazPtr::try_acquire
    /// [acquired]: HazPtr::try_acquire
    /// [dropped]: Drop::drop
    /// [equal]: PartialEq::eq
    /// [protecting]: Anchor::moor
    /// [retired]: Domain::retire
    ///
    fn acquire(self) -> Option<&'dom HazPtr>;

    ///
    /// Sets the [`Hazards`][Hazard] pointed by `retired` to be [dropped] some time after no more
    /// [`HazPtrs`][HazPtr] owned by this domain are protecting it.
    ///
    /// # Safety
    ///
    /// * The storage for `retired` must be [*currently allocated*] by the same allocator returned
    /// by [allocator].
    /// This ensures that [`Allocator::deallocate`] can be called safely.
    ///
    /// # Implementation Safety
    ///
    /// * Must not drop `retired` until no [`HazPtr`] owned by this domain is protecting it.
    ///
    /// [*currently allocated*]: Allocator#currently-allocated-memory
    /// [retired]: Domain::retire
    ///
    unsafe fn retire(self, retired: NonNull<dyn Hazard<'dom>>);
}
