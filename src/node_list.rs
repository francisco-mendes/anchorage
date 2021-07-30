use std::{
    alloc::{
        Allocator,
        Global,
    },
    iter,
    ptr,
    sync::atomic::{
        AtomicIsize,
        AtomicPtr,
        Ordering,
    },
};

#[derive(Debug)]
pub struct Node<T> {
    pub next: AtomicPtr<Node<T>>,
    pub value: T,
}

impl<T> Node<T> {
    pub fn iter(&self) -> impl Iterator<Item = &Node<T>> {
        // Safety: node atomic pointers are either null or point to a valid Node which is never
        // deallocated while we can still access the list.

        iter::successors(Some(self), |&ptr| unsafe {
            ptr.next.load(Ordering::Relaxed).as_ref()
        })
    }
}

pub struct List<T> {
    pub head: AtomicPtr<Node<T>>,
    pub count: AtomicIsize,
}

impl<T> List<T> {
    #[inline]
    pub const fn new() -> Self {
        Self {
            head: AtomicPtr::new(ptr::null_mut()),
            count: AtomicIsize::new(0),
        }
    }

    #[inline]
    pub fn push_front(&self, value: T) -> &T {
        // Need to allocate a new node
        let node = Box::into_raw(Box::new_in(
            Node {
                next: AtomicPtr::new(ptr::null_mut()),
                value,
            },
            Global,
        ));

        self.push_list_front(node, node, 1)
    }

    #[inline]
    pub(crate) fn push_list_front(
        &self,
        new_head: *mut Node<T>,
        new_tail: *mut Node<T>,
        count: isize,
    ) -> &T {
        crate::asymmetric_fence::light();

        let mut head = self.head.load(Ordering::Acquire);

        let ret = loop {
            // Safety: hazptr was never shared, so &mut is ok.
            *unsafe { &mut *new_tail }.next.get_mut() = head;

            // Note: Folly uses Release, but needs to be both for the load on success.
            match self.head.compare_exchange_weak(
                head,
                new_head,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                // Safety: hazptr is never null and this domain lasts for the whole program.
                Ok(_) => break unsafe { new_head.as_ref().map(|n| &n.value).unwrap_unchecked() },
                // Head has changed, try again with that as our next ptr.
                Err(head_now) => head = head_now,
            }
        };

        // Note: Folly uses SeqCst because it's the default, not clear if necessary.
        self.count.fetch_add(count, Ordering::SeqCst);
        ret
    }

    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        let node = unsafe { self.head.load(Ordering::Acquire).as_ref() };
        node.into_iter().flat_map(|n| n.iter().map(|n| &n.value))
    }
}
