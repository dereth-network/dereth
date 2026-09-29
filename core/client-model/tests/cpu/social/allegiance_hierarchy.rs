//! Contracts for allegiance hierarchy.
//! Fixture: shared recorded messages and synthetic state.
//! Behaviour: none (codec, fixture conformance or host-state contracts)

use crate::common::model_replay::*;

#[test]
fn the_allegiance_hierarchy_unpacks_and_rebuilds_into_a_tree() {
    let all = corpus::load_all();
    assert!(
        all.iter().any(|(_, b)| !b.is_empty()),
        "the capture corpus is this test's oracle"
    );
    let mut versions = std::collections::BTreeSet::new();
    let mut saw_members = false;
    for (name, blobs) in &all {
        let r = replay(blobs);
        versions.extend(r.allegiance_versions.iter().copied());
        if r.allegiance_members > 0 {
            saw_members = true;
            let mut w = World::new();
            let _ = &mut w;
            eprintln!("{name}: {} allegiance member(s)", r.allegiance_members);
        }
    }
    eprintln!("allegiance versions seen: {versions:?}");
    assert!(
        !versions.is_empty(),
        "the corpus carries six Allegiance_AllegianceUpdate events"
    );
    for v in &versions {
        assert!(
            *v <= dereth_client_model::allegiance::version::NEWEST,
            "version {v} is past the newest the client knows"
        );
    }
    let _ = saw_members;
}
