//! The unit tests' allocator: the system's, counting the allocations each thread makes, so a
//! test can say how many a piece of work made ([`allocations`]). Each thread counts its own, so
//! tests running side by side do not see each other's.

#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

thread_local! {
    static MADE: Cell<u64> = const { Cell::new(0) };
}

fn count() {
    // A thread being torn down has no counter left; its allocations go uncounted.
    let _ = MADE.try_with(|m| m.set(m.get() + 1));
}

struct Counting;

// SAFETY: every call is passed to the system allocator unchanged, with the caller's own layout
// and pointer; counting touches only a thread-local cell, which allocates nothing.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count();
        // SAFETY: the caller's layout, as `GlobalAlloc::alloc` requires of it.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count();
        // SAFETY: as `alloc`.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        count();
        // SAFETY: `ptr` came from this allocator, which is the system's, with `layout`.
        unsafe { System.realloc(ptr, layout, new_size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` came from this allocator, which is the system's, with `layout`.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static COUNTING: Counting = Counting;

/// What `work` returns, and how many allocations (and reallocations) this thread made doing it.
pub(crate) fn allocations<T>(work: impl FnOnce() -> T) -> (T, u64) {
    let before = MADE.with(Cell::get);
    let out = work();
    (out, MADE.with(Cell::get) - before)
}
