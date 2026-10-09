//! The passes run in one fixed order.

use dereth_render_hifi::graph::{Graph, Slot};

/// Behaviour: hifi.graph.the-passes-run-in-one-fixed-order
#[test]
fn every_pass_has_one_slot_and_the_slots_run_in_the_fixed_order() {
    let order = Graph::standard().order();
    let names: Vec<&str> = order.iter().map(|(_, n)| *n).collect();
    let mut unique = names.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(
        unique.len(),
        names.len(),
        "a pass is listed twice: {names:?}"
    );
    let rank = |s: Slot| {
        Slot::ORDER
            .iter()
            .position(|o| *o == s)
            .expect("every slot is ranked")
    };
    assert!(
        order.windows(2).all(|w| rank(w[0].0) <= rank(w[1].0)),
        "passes out of slot order: {order:?}"
    );
    // Each option has its pass.
    for name in ["gtao", "atmosphere", "lighting", "gi"] {
        assert!(names.contains(&name), "no {name} pass");
    }
}
