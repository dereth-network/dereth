//! The shared resources' accounting: a budget, and release after a quiet spell.

use dereth_render_hifi::resources::{Ledger, ResourceName, IDLE_FRAMES};
use dereth_render_hifi::HifiError;

/// Behaviour: hifi.resources.the-shared-resources-keep-to-their-budget
#[test]
fn the_shared_resources_keep_to_their_budget_and_go_when_unused() {
    let mut ledger = Ledger::new(1000);
    ledger.charge(ResourceName::WorldColour, 600).expect("fits");
    assert_eq!(ledger.used(), 600);
    assert_eq!(ledger.left(), 400);
    // Past the budget: refused, and nothing changes.
    assert_eq!(
        ledger.charge(ResourceName::WorldDepth, 500),
        Err(HifiError::Budget {
            wanted: 500,
            left: 400
        })
    );
    assert_eq!(ledger.used(), 600);
    assert!(!ledger.holds(ResourceName::WorldDepth));
    // A new charge under a name replaces the old one rather than adding to it.
    ledger
        .charge(ResourceName::WorldColour, 900)
        .expect("replaces");
    assert_eq!(ledger.used(), 900);
    ledger
        .charge(ResourceName::WorldColour, 300)
        .expect("shrinks");
    ledger
        .charge(ResourceName::WorldDepth, 500)
        .expect("fits now");
    assert_eq!(ledger.used(), 800);

    // Asked for every frame, a resource stays; left alone for the idle spell, it goes.
    for _ in 0..IDLE_FRAMES * 2 {
        assert!(ledger.begin_frame().is_empty());
        assert!(ledger.touch(ResourceName::WorldColour));
        assert!(ledger.touch(ResourceName::WorldDepth));
    }
    for _ in 0..IDLE_FRAMES {
        assert!(ledger.begin_frame().is_empty());
        ledger.touch(ResourceName::WorldColour);
    }
    assert_eq!(ledger.begin_frame(), vec![ResourceName::WorldDepth]);
    assert!(ledger.holds(ResourceName::WorldColour));
    assert!(!ledger.holds(ResourceName::WorldDepth));
    assert_eq!(ledger.used(), 300);
    ledger.release_all();
    assert_eq!(ledger.used(), 0);
}
