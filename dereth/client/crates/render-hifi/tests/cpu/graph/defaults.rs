//! With default settings the presentation would draw nothing differently.

use dereth_render_hifi::graph::Graph;
use dereth_render_hifi::{Caps, HifiFrame, HifiSettings};

/// Every capability on, so a pass cannot be unwanted for lack of one.
fn every_cap() -> Caps {
    Caps {
        compute: true,
        timestamp_query: true,
        float32_filterable: true,
        depth_clip_control: true,
        ray_query: true,
    }
}

/// Behaviour: hifi.defaults.with-default-settings-no-pass-runs
#[test]
fn with_default_settings_no_pass_is_wanted_and_nothing_is_effective() {
    let s = HifiSettings::default();
    assert!(!s.any_effective());
    let graph = Graph::standard();
    assert!(graph
        .wanted(&s, &HifiFrame::default(), &every_cap())
        .is_empty());
}
