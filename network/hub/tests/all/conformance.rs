//! The conformance suite's hub checks, run against this hub over real WebSockets and its HTTP
//! routes on loopback.
//! Fixture: a hub on an ephemeral loopback port over an in-memory registry with three federated
//! worlds and one whose federation flag is off, its clock the time now plus what the suite lets
//! pass.

use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use dereth_network_conformance::hub::{run, HubClock, HubTarget};
use dereth_network_conformance::{failures, vectors};
use dereth_network_hub::registry::{Registration, Registry};
use dereth_network_hub::server::{start_with, HubConfig, Running};
use dereth_network_protocol::Federation;

fn registration(slug: &str) -> Registration {
    Registration::from_json(&serde_json::json!({
        "slug": slug, "name": slug.to_uppercase(), "description": "A conformance world.",
        "ruleset": "PvE",
        "endpoint": { "mode": "manual", "address": "192.0.2.1", "port": 9000 },
        "emulator": { "family": "Test" },
    }))
    .expect("a registration")
}

/// A hub with worlds `aa`, `bb`, `cc` (on) and `dd` (off): the hub and the keys.
pub(crate) fn hub() -> (Running, Vec<(String, String)>, (String, String)) {
    hub_with(HubConfig::default())
}

/// [`hub`] with `config`'s settings (its address and database are the fixture's).
pub(crate) fn hub_with(config: HubConfig) -> (Running, Vec<(String, String)>, (String, String)) {
    let (running, worlds, off, _) = hub_with_clock(config);
    (running, worlds, off)
}

/// A world's slug and its key.
type World = (String, String);

/// [`hub_with`], and the hub's clock for the suite to let time pass on.
fn hub_with_clock(config: HubConfig) -> (Running, Vec<World>, World, HubClock) {
    let passed = Arc::new(Mutex::new(Duration::ZERO));
    let read = Arc::clone(&passed);
    let registry = Registry::open(
        ":memory:",
        "dereth.network",
        Box::new(move || SystemTime::now() + *read.lock().expect("the clock")),
    )
    .expect("a registry");
    let mut keys = Vec::new();
    for slug in ["aa", "bb", "cc", "dd"] {
        let (_, key) = registry.register(&registration(slug)).expect("registered");
        let flag = if slug == "dd" {
            Federation::Off
        } else {
            Federation::On
        };
        registry.set_federation(slug, flag).expect("flag");
        keys.push((slug.to_owned(), key));
    }
    let config = HubConfig {
        listen: "127.0.0.1:0".parse().expect("an address"),
        db: ":memory:".into(),
        ..config
    };
    let running = start_with(config, registry).expect("the hub starts");
    let off = keys.pop().expect("dd");
    let clock = HubClock::new(move |by| *passed.lock().expect("the clock") += by);
    (running, keys, off, clock)
}

/// Spec: conformance: every hub check passes against this hub.
#[test]
fn the_hub_passes_every_hub_check() {
    let (running, worlds, off, clock) = hub_with_clock(HubConfig::default());
    let target = HubTarget {
        base: format!("ws://{}", running.local_addr()),
        worlds,
        off: Some(off),
        slow: false,
        clock: Some(clock),
    };
    let checks = run(&target);
    let failed = failures(&checks);
    running.shutdown();
    assert!(checks.len() >= 31, "{} checks ran", checks.len());
    assert_eq!(
        checks
            .iter()
            .filter(|c| c.name.starts_with("person."))
            .count(),
        3,
        "the person layer's checks ran"
    );
    assert_eq!(
        checks
            .iter()
            .filter(|c| c.name.starts_with("listing."))
            .count(),
        2,
        "the directory's link checks ran"
    );
    assert_eq!(
        checks
            .iter()
            .filter(|c| c.name.starts_with("directory."))
            .count(),
        9,
        "the directory's checks over the public routes ran"
    );
    assert!(
        failed.is_empty(),
        "{}",
        failed
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// Spec: conformance: the hub's own protocol crate agrees with every vector.
#[test]
fn the_protocol_crate_agrees_with_every_vector() {
    let checks = vectors::all();
    for suite in [
        "addresses",
        "text rules",
        "link messages",
        "challenge",
        "directory",
        "reach",
        "say",
        "rooms",
        "web addresses",
    ] {
        assert!(
            checks.iter().any(|c| c.suite == suite),
            "no {suite} vectors ran"
        );
    }
    let failed = failures(&checks);
    assert!(
        failed.is_empty(),
        "{}",
        failed
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// Spec: conformance: a tell nobody answers is answered for, by the hub, within its timer.
#[test]
#[ignore = "waits out the hub's 30-second answer timer and its minute's hold; run with --ignored"]
fn the_hub_answers_an_unanswered_tell_itself() {
    let (running, worlds, off, clock) = hub_with_clock(HubConfig::default());
    let target = HubTarget {
        base: format!("ws://{}", running.local_addr()),
        worlds,
        off: Some(off),
        slow: true,
        clock: Some(clock),
    };
    let checks = run(&target);
    running.shutdown();
    for name in [
        "tell.an_unanswered_tell_is_answered_unavailable",
        "tell.a_held_tell_lapses_unavailable",
    ] {
        let slow = checks
            .iter()
            .find(|c| c.name == name)
            .expect("the slow check ran");
        assert_eq!(slow.result, Ok(()), "{name}");
    }
    let failed = failures(&checks);
    assert!(
        failed.is_empty(),
        "{}",
        failed
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    );
}
