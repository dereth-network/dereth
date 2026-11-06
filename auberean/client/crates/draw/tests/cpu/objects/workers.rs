//! Work shared among the threads there are.
//!
//! Behaviour: none (how a frame's work is shared out, which no behavior of the game names)

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use auberean_draw::workers::share;

/// Every item is worked once before a share comes back, however many there are.
#[test]
fn every_item_is_worked_once_before_a_share_comes_back() {
    for count in [0, 1, 2, 3, 7, 1000] {
        let items: Vec<Arc<AtomicU32>> = (0..count).map(|_| Arc::default()).collect();
        share(items.clone(), |item: &Arc<AtomicU32>| {
            item.fetch_add(1, Ordering::Relaxed);
        });
        assert!(
            items.iter().all(|i| i.load(Ordering::Relaxed) == 1),
            "{count} items"
        );
    }
}

/// Work that panics on any thread does not leave the share waiting, and shares go on after it.
#[test]
fn a_panic_in_the_work_does_not_leave_a_share_waiting() {
    let items: Vec<u32> = (0..64).collect();
    let _ = std::panic::catch_unwind(|| {
        share(items, |item: &u32| assert_ne!(*item % 8, 0, "as asked"));
    });
    let after: Vec<Arc<AtomicU32>> = (0..64).map(|_| Arc::default()).collect();
    share(after.clone(), |item: &Arc<AtomicU32>| {
        item.fetch_add(1, Ordering::Relaxed);
    });
    assert!(after.iter().all(|i| i.load(Ordering::Relaxed) == 1));
}
