use std::{
    alloc::Global,
    collections::HashSet,
    convert::TryFrom,
    iter,
    ptr,
    ptr::NonNull,
    sync::atomic::{
        AtomicU64,
        AtomicUsize,
        Ordering,
    },
};

use crate::{
    domain::Domain,
    hazptr::HazPtr,
    node_list::{
        List,
        Node,
    },
    Hazard,
};

const SYNC_TIME_PERIOD: u64 = std::time::Duration::from_nanos(2_000_000_000).as_nanos() as u64;
const RETIRED_COUNT_THRESHOLD: isize = 1000;
const HP_COUNT_MULTIPLIER: isize = 2;

static GLOBAL: GlobalDomainStatic = GlobalDomainStatic::new();

const fn reached_threshold(retired_num: isize, hazptr_num: isize) -> bool {
    retired_num >= RETIRED_COUNT_THRESHOLD && retired_num >= HP_COUNT_MULTIPLIER * hazptr_num
}

struct GlobalDomainStatic {
    hazptrs: List<HazPtr>,
    retired: List<NonNull<dyn Hazard<'static>>>,
    sync_time: AtomicU64,
    nbulk_reclaims: AtomicUsize,
}

impl GlobalDomainStatic {
    pub const fn new() -> Self {
        Self {
            hazptrs: List::new(),
            retired: List::new(),
            sync_time: AtomicU64::new(0),
            nbulk_reclaims: AtomicUsize::new(0),
        }
    }

    fn try_acquire_existing(&self) -> Option<&HazPtr> {
        self.hazptrs.iter().find(|hp| hp.try_acquire())
    }

    fn acquire_new(&self) -> &HazPtr {
        self.hazptrs.push_front(HazPtr::new(true))
    }

    fn retire(&self, retired: NonNull<dyn Hazard<'static>>) {
        self.retired.push_front(retired);

        // Folly has if check here, but only for recursion from bulk_lookup_and_reclaim,
        // which we don't do, so check isn't necessary.
        self.check_cleanup_and_reclaim();
    }

    fn check_cleanup_and_reclaim(&self) {
        if self.try_timed_cleanup() {
            return;
        }

        let retired_num = self.retired.count.load(Ordering::Acquire);
        let hazptr_num = self.hazptrs.count.load(Ordering::Acquire);
        if reached_threshold(retired_num, hazptr_num) {
            self.try_bulk_reclaim();
        }
    }

    fn try_timed_cleanup(&self) -> bool {
        if !self.check_sync_time() {
            return false;
        }
        self.relaxed_cleanup();
        true
    }

    fn check_sync_time(&self) -> bool {
        let time = u64::try_from(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time is set to before the epoch")
                .as_nanos(),
        )
        .expect("system time is too far into the future");

        let sync_time = self.sync_time.load(Ordering::Relaxed);

        // If it's not time to clean yet, or someone else just started cleaning, don't clean.
        time > sync_time
            && self
                .sync_time
                .compare_exchange(
                    sync_time,
                    time + SYNC_TIME_PERIOD,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                )
                .is_ok()
    }

    fn relaxed_cleanup(&self) {
        self.retired.count.store(0, Ordering::Release);
        self.bulk_reclaim(true);
    }

    fn try_bulk_reclaim(&self) {
        let retired_num = self.retired.count.load(Ordering::Acquire);
        let hazptr_num = self.hazptrs.count.load(Ordering::Acquire);

        if !reached_threshold(retired_num, hazptr_num) {
            return;
        }

        let retired_num = self.retired.count.swap(0, Ordering::Release);

        // No need to add retired_num back to self.retired.count.
        // At least one concurrent try_bulk_reclaim will proceed to bulk_reclaim.
        if !reached_threshold(retired_num, hazptr_num) {
            return;
        }

        self.bulk_reclaim(false);
    }

    fn bulk_reclaim(&self, transitive: bool) -> usize {
        self.nbulk_reclaims.fetch_add(1, Ordering::Acquire);

        let mut reclaimed = 0;
        loop {
            let steal = self.retired.head.swap(ptr::null_mut(), Ordering::Acquire);

            crate::asymmetric_fence::heavy();

            if steal.is_null() {
                return reclaimed;
            }

            // Find all guarded addresses.
            let guarded_ptrs = self
                .hazptrs
                .iter()
                .map(|hp| hp.ptr() as *const _)
                .collect::<HashSet<_>>();

            let (reclaimed_now, done) = self.bulk_lookup_and_reclaim(steal, guarded_ptrs);
            reclaimed += reclaimed_now;

            if done || !transitive {
                break;
            }
        }
        self.nbulk_reclaims.fetch_sub(1, Ordering::Release);
        reclaimed
    }

    fn bulk_lookup_and_reclaim(
        &self,
        stolen_hazard_head: *mut Node<NonNull<dyn Hazard<'static>>>,
        guarded_ptrs: HashSet<*const u8>,
    ) -> (usize, bool) {
        struct LiveList {
            head: *mut Node<NonNull<dyn Hazard<'static>>>,
            tail: Option<NonNull<Node<NonNull<dyn Hazard<'static>>>>>,
        }

        // Reclaim any retired objects that aren't guarded
        let mut live_list = LiveList {
            head: ptr::null_mut(),
            tail: None,
        };

        let mut reclaimed: usize = 0;
        let mut still_retired: isize = 0;

        // Safety: All accessors only access the head, and the head is no longer pointing here.
        // We own the only pointers to these nodes, and they are all valid or null
        let nodes = iter::successors(
            NonNull::new(stolen_hazard_head),
            // Same here
            |node| unsafe {
                let next = node.as_ref().next.load(Ordering::Relaxed);
                debug_assert_ne!(node.as_ptr(), next);
                NonNull::new(next)
            },
        );

        for node in nodes {
            let node_ref = unsafe { node.as_ref() };
            if !guarded_ptrs.contains(&(node_ref.value.as_ptr() as *const u8)) {
                // Safety: The hazard is not being protected, thus we can drop it,
                // as well as the node pointer. Both were allocated using Global.
                unsafe {
                    let drop_node = Box::from_raw_in(node.as_ptr(), Global);
                    drop(Box::from_raw_in(drop_node.value.as_ptr(), Global));
                    drop(drop_node);
                }
                reclaimed += 1;
            } else {
                node_ref.next.store(live_list.head, Ordering::Relaxed);
                if live_list.tail.is_none() {
                    live_list = LiveList {
                        head: node.as_ptr(),
                        tail: Some(node),
                    };
                } else {
                    live_list.head = node.as_ptr();
                }
                still_retired += 1;
            }
        }

        let done = self.retired.head.load(Ordering::Acquire).is_null();

        match live_list {
            LiveList {
                head,
                tail: Some(tail),
            } => {
                assert!(!head.is_null());
                assert_ne!(still_retired, 0);
                self.retired
                    .push_list_front(head, tail.as_ptr(), still_retired);
            }
            LiveList {
                head,
                tail: Option::None,
            } => {
                assert!(head.is_null());
                assert_eq!(still_retired, 0);
            }
        };
        (reclaimed, done)
    }
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub struct GlobalDomain;

impl GlobalDomain {
    pub fn eager_reclaim(&self) -> usize {
        GLOBAL.bulk_reclaim(true)
    }
}

unsafe impl Domain<'static> for GlobalDomain {
    type Alloc = Global;

    #[inline]
    fn allocator(self) -> &'static Self::Alloc {
        &Global
    }

    fn acquire(self) -> Option<&'static HazPtr> {
        let ptr = match GLOBAL.try_acquire_existing() {
            Some(hazptr) => hazptr,
            None => GLOBAL.acquire_new(),
        };
        Some(ptr)
    }

    unsafe fn retire(self, retired: NonNull<dyn Hazard<'static>>) {
        GLOBAL.retire(retired)
    }
}
