//! Community imports run beside a serving hub: the import and the hub's own writes share the
//! registry file without either failing, and an applied import shows in the public lists within
//! the snapshot's second.
//! Fixture: a hub on loopback over a registry file in a temporary folder, a second connection
//! writing heartbeats as the hub does, and the import on a third.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use dereth_network_hub::registry::{
    CommunityEntry, Heartbeat, ImportOptions, Registration, Registry,
};
use dereth_network_hub::server::{start_with, HubConfig};
use serde_json::json;

fn get(addr: std::net::SocketAddr, path: &str) -> String {
    let mut s = TcpStream::connect(addr).expect("connect");
    write!(s, "GET {path} HTTP/1.1\r\nHost: localhost\r\n\r\n").expect("write");
    let mut out = String::new();
    s.read_to_string(&mut out).expect("read");
    out
}

fn open(db: &str) -> Registry {
    Registry::open(db, "dereth.network", Box::new(std::time::SystemTime::now)).expect("a registry")
}

fn entry(n: usize, hash: &str) -> CommunityEntry {
    let slug = format!("listed-{n}");
    CommunityEntry {
        external_id: format!("00000000-0000-4000-8000-{n:012}"),
        registration: Registration::from_json(&json!({
            "slug": slug, "name": slug, "description": "A listing.", "ruleset": "PvE",
            "endpoint": { "mode": "manual", "address": "192.0.2.10", "port": 9000 },
            "emulator": { "family": "ACE" },
        }))
        .expect("a registration"),
        source_hash: hash.into(),
        legacy_type: "PvE".into(),
    }
}

/// Spec: hub: an applied community import run while the hub serves waits for the hub's writes
/// rather than failing, the hub's writes wait for it, and what it imports is in the public lists
/// within a few seconds.
#[test]
fn an_import_beside_a_serving_hub_succeeds_and_shows_promptly() {
    let dir = std::env::temp_dir().join(format!(
        "dereth-hub-community-serving-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("a folder");
    let db = dir.join("hub.sqlite");
    for f in ["hub.sqlite", "hub.sqlite-wal", "hub.sqlite-shm"] {
        let _ = std::fs::remove_file(dir.join(f));
    }
    let db = db.to_str().expect("a path").to_owned();
    let hub_registry = open(&db);
    hub_registry
        .register(
            &Registration::from_json(&json!({
                "slug": "coldeve", "name": "Coldeve", "description": "A world.", "ruleset": "PvE",
                "endpoint": { "mode": "manual", "address": "192.0.2.1", "port": 9000 },
                "emulator": { "family": "ACE" },
            }))
            .expect("a registration"),
        )
        .expect("registered");
    let running = start_with(
        HubConfig {
            listen: "127.0.0.1:0".parse().expect("an address"),
            db: db.clone(),
            ..HubConfig::default()
        },
        hub_registry,
    )
    .expect("the hub starts");
    let addr = running.local_addr();

    // The hub's own writes, far more often than worlds send them.
    let stop = Arc::new(AtomicBool::new(false));
    let writer = {
        let (stop, db) = (Arc::clone(&stop), db.clone());
        std::thread::spawn(move || {
            let r = open(&db);
            let beat: Heartbeat =
                serde_json::from_value(json!({ "state": "online", "playersOnline": 1 }))
                    .expect("a heartbeat");
            let (mut ok, mut failed) = (0usize, Vec::new());
            while !stop.load(Ordering::SeqCst) {
                match r.heartbeat("coldeve", &beat, None) {
                    Ok(_) => ok += 1,
                    Err(e) => failed.push(e.to_string()),
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            (ok, failed)
        })
    };

    let mut importer = open(&db);
    let options = ImportOptions {
        apply: true,
        skip_conflicts: true,
        retire_after: Some(3),
        also_on_list: Vec::new(),
    };
    for run in 0..40 {
        let list: Vec<CommunityEntry> = (0..200).map(|n| entry(n, &run.to_string())).collect();
        importer
            .import_community_with(&list, &options)
            .unwrap_or_else(|e| panic!("run {run}: {e}"));
    }
    stop.store(true, Ordering::SeqCst);
    let (beats, failed) = writer.join().expect("the writer");
    assert!(failed.is_empty(), "the hub's writes failed: {failed:?}");
    assert!(beats > 0);

    let fresh: Vec<CommunityEntry> = (0..201).map(|n| entry(n, "40")).collect();
    importer
        .import_community_with(&fresh, &options)
        .expect("one more");
    let started = Instant::now();
    loop {
        let page = get(addr, "/v1/servers?q=listed-200");
        if page.contains("\"slug\":\"listed-200\"") {
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the import is not in the public lists: {page}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    running.shutdown();
    drop(importer);
    let _ = std::fs::remove_dir_all(&dir);
}
