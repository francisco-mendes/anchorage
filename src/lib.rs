#![feature(
    allocator_api,
    arbitrary_self_types,
    const_fn,
    iter_map_while,
    maybe_uninit_extra,
    option_result_unwrap_unchecked,
    ptr_as_uninit
)]
// Lints
#![warn(
    future_incompatible,
    nonstandard_style,
    rust_2018_compatibility,
    rust_2018_idioms
)]
#![deny(unsafe_op_in_unsafe_fn)]

///
/// Marks a type as being able to be protected via Hazard Pointers.
///
/// [`Hazards`][Hazard] protected behind HazBox pointers must not be [dropped] while they are [protected].
/// This requires that the be [retired] to a domain, to be dropped some time in the future.
/// Since how long this takes is unknown, Hazards need to ensure that
/// their destructors do not access possibly dangling references.
///
/// This means that if one wants to protect hazards that have borrowed, non 'static data,
/// then the references of that data must outlive the domain. Thus borrowed data must be retired to
/// a temporary domain that is [dropped] before it and cannot be used with the [GlobalDomain].
///
/// [dropped]: Drop::drop
/// [protected]: Anchor::moor
/// [retired]: Domain::retire
///
pub trait Hazard<'dom>: Sync + Send + 'dom {}

impl<'dom, T> Hazard<'dom> for T where T: Sync + Send + 'dom {}

pub mod anchor;
pub mod domain;
pub mod hazbox;
pub mod hazptr;
pub mod node_list;

pub(crate) mod retire;

pub mod asymmetric_fence {
    use std::sync::atomic::{
        fence,
        Ordering,
    };

    #[inline(always)]
    pub fn light() {
        fence(Ordering::SeqCst);
    }

    #[inline(always)]
    pub fn heavy() {
        fence(Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use std::{
        alloc::Global,
        ptr::NonNull,
    };

    use crate::{
        domain::{
            global::GlobalDomain,
            Domain,
        },
        hazbox::HazBox,
        hazptr::HazPtr,
        Hazard,
    };

    #[test]
    pub fn test_uwu() {
        let s = vec![1usize, 2, 3];
        let s = vec![s];

        {
            let b: HazBox<'_, &[Vec<usize>], GlobalDomain>;
            let owo = s.as_slice();

            b = HazBox::<_, GlobalDomain>::new(owo);
        }
    }

    #[test]
    pub fn test_owo() {
        #[derive(Eq, PartialEq)]
        struct LocalDomain(usize);

        #[derive(Copy, Clone, Eq, PartialEq)]
        struct LocalDomainRef<'d>(&'d LocalDomain);

        unsafe impl<'d> Domain<'d> for LocalDomainRef<'d> {
            type Alloc = Global;

            fn allocator(self) -> &'d Self::Alloc {
                &Global
            }

            fn acquire(self) -> Option<&'d HazPtr> {
                todo!()
            }

            unsafe fn retire(self, _retired: NonNull<dyn Hazard<'d>>) {
                todo!()
            }
        }

        let s = vec![1usize, 2, 3];
        let s = vec![s];
        let d1 = LocalDomain(1);

        {
            let owo = s.as_slice();
            let b = HazBox::new_in(owo, LocalDomainRef(&d1));
        }

        {
            let d2 = LocalDomain(2);
            let owo = s.as_slice();
            let b = HazBox::new_in(owo, LocalDomainRef(&d2));
        }
        let b: HazBox<'_, &[Vec<usize, Global>], LocalDomainRef<'_>>;

        {
            let d3 = LocalDomain(3);
            let owo = s.as_slice();
            b = HazBox::new_in(owo, LocalDomainRef(&d3));
        }
    }
}
