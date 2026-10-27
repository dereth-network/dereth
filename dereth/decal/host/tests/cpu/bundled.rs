//! Behaviour: none (a host inside another program: the plugins that come with Dereth, run in it)
//!
//! The plugins that come with Dereth (`dereth/plugins`) load in this host, run through a recorded
//! session without a fault, and show their windows as Decal views; follow, which needs the bus to
//! the player's other clients, is refused when the host has none.

use crate::abi::{settings, Replay};

/// The plugins that come with Dereth.
fn bundled() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins")
}

#[test]
fn the_plugins_that_come_with_dereth_run_through_a_recorded_session_without_a_fault() {
    let mut r = Replay::new(&settings(&[bundled()]), "long-solo-play");
    r.frame();
    r.login();
    r.advance(60.0);
    let report = r.dll.report();
    let plugins = report.get("plugins").as_list().unwrap_or_default();
    for id in [
        "network.dereth.trackers",
        "network.dereth.navigator",
        "network.dereth.tank",
        "network.dereth.looter",
        "network.dereth.buffbot",
        "network.dereth.xpmeter",
    ] {
        let p = plugins
            .iter()
            .find(|p| p.get("id").as_str() == Some(id))
            .unwrap_or_else(|| panic!("{id} is loaded: {report:?}"));
        assert_eq!(p.get("status").as_str(), Some("running"), "{id}: {p:?}");
        let console = p.get("console").as_list().unwrap_or_default();
        assert!(
            !console.iter().any(|l| l
                .as_str()
                .is_some_and(|l| l.to_lowercase().contains("error"))),
            "{id}: {console:?}"
        );
        assert!(
            r.surface
                .views
                .keys()
                .any(|k| k.starts_with(&format!("{id}/"))),
            "{id} has a view: {:?}",
            r.surface.views.keys().collect::<Vec<_>>()
        );
    }
    assert!(
        r.surface
            .printed
            .iter()
            .any(|l| l.contains("network.dereth.follow was not loaded")),
        "{:?}",
        r.surface.printed
    );
}
