//! Vectors: local synthetic world snapshots and HTTP request/response cases in this module
//! Status snapshot of a fresh/opened world, exact escaped JSON, GET answers, health follows the
//! world thread, other requests, address from CLI/config, source URL reported, the WebSocket URL
//! reported and readable from an admitted page origin only, a shut-down endpoint frees its
//! address.
//! Fixture: isolated configuration paths and synthetic server state.

use empyrean_common::clock::{ClockSnapshot, VirtualClock};
use empyrean_common::era::EraExt as _;
use empyrean_dat::FakeDats;
use empyrean_server::status_endpoint::{
    cors_header, respond, status_address, DatIterations, StatusSnapshot,
};
use empyrean_world::managers::world_manager;
use empyrean_world::World;

fn world() -> World {
    let clock = VirtualClock::default();
    World::new(
        ClockSnapshot::take(&clock, 0.0),
        FakeDats::new().build().expect("empty fake dats"),
    )
}

fn sample() -> StatusSnapshot {
    StatusSnapshot {
        world_name: "Test \"Shard\"".to_owned(),
        version: "0.0.0".to_owned(),
        source_url: "https://example.org/src".to_owned(),
        uptime_seconds: 42,
        world_open: true,
        shutting_down: false,
        connections: 3,
        authenticated_connections: 2,
        players_online: 1,
        landblocks_loaded: 5,
        content_hash: Some("ab12".to_owned()),
        corrections_digest: "v1:0123456789abcdef".to_owned(),
        era: "infiltration".to_owned(),
        features: empyrean_common::era::EraFeatures::INFILTRATION,
        dats: DatIterations {
            portal: Some(2072),
            cell: Some(982),
            local: Some(994),
            highres: None,
        },
        dat_patching: false,
        client_versions: vec!["1802".to_owned()],
        websocket_url: Some("wss://play.example.org/ws".to_owned()),
        not_ported: vec![("ACE: A.B".to_owned(), 2), ("ACE: C.D".to_owned(), 1)],
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).expect("utf-8 response")
}

#[test]
fn snapshot_of_a_fresh_world_then_opened() {
    let mut w = world();
    let s = StatusSnapshot::take(&w, "Dereth", 7);
    assert_eq!(s.world_name, "Dereth");
    assert_eq!(s.uptime_seconds, 7);
    assert_eq!(
        s.source_url, "https://github.com/dereth-network/dereth",
        "the build's repository while server.source_url is empty"
    );
    assert!(!s.world_open, "WorldManager.WorldStatus starts Closed");
    assert!(!s.shutting_down);
    assert_eq!(
        (
            s.connections,
            s.authenticated_connections,
            s.players_online,
            s.landblocks_loaded
        ),
        (0, 0, 0, 0)
    );
    assert_eq!(
        s.corrections_digest,
        empyrean_content::corrections::digest()
    );
    assert_eq!(
        s.content_hash.as_ref().map(String::len),
        Some(64),
        "the test world's pack hash"
    );

    empyrean_common::not_ported::hit("ACE: StatusEndpointTest.Probe");
    let probed = StatusSnapshot::take(&w, "Dereth", 7).not_ported;
    assert!(
        probed
            .iter()
            .any(|(name, hits)| name == "ACE: StatusEndpointTest.Probe" && *hits >= 1),
        "{probed:?}"
    );

    world_manager::open(&mut w, None);
    assert!(StatusSnapshot::take(&w, "Dereth", 8).world_open);

    w.server_manager.shutdown_initiated = true;
    assert!(StatusSnapshot::take(&w, "Dereth", 9).shutting_down);
}

#[test]
fn status_json_is_exact_and_escaped() {
    assert_eq!(
        sample().to_json(),
        "{\"world_name\":\"Test \\\"Shard\\\"\",\"version\":\"0.0.0\",\"source_url\":\"https://example.org/src\",\"uptime_seconds\":42,\"world_open\":true,\"shutting_down\":false,\"connections\":3,\"authenticated_connections\":2,\"players_online\":1,\"landblocks_loaded\":5,\"content_hash\":\"ab12\",\"corrections_digest\":\"v1:0123456789abcdef\",\"era\":\"infiltration\",\"features\":{\"ratings\":false,\"consolidated_weapon_skills\":false,\"item_spell_auras\":false,\"assessed_armor_and_ratings\":false,\"swear_to_lower_level\":false,\"pre_order_items_and_rares\":false,\"dual_wield\":false,\"weapon_masteries\":false,\"innate_augmentations\":false,\"aetheria\":false,\"luminance\":false,\"contracts\":false,\"titles\":false,\"cloaks\":false,\"trinkets\":false,\"journal\":false,\"trade\":true,\"housing\":true,\"apartments\":true,\"tinkering\":true,\"cantrips\":true,\"spell_research\":false,\"chess\":true},\"dats\":{\"portal\":2072,\"cell\":982,\"local\":994,\"highres\":null,\"patching\":false},\"client_versions\":[\"1802\"],\"websocket_url\":\"wss://play.example.org/ws\",\"not_ported\":{\"ACE: A.B\":2,\"ACE: C.D\":1}}\n"
    );
}

#[test]
fn get_status_answers_the_snapshot() {
    let body = sample().to_json();
    let r = text(&respond("GET /status HTTP/1.1", || Some(sample())));
    assert!(r.starts_with("HTTP/1.0 200 OK\r\n"), "{r}");
    assert!(
        r.contains("Content-Type: application/json; charset=utf-8\r\n"),
        "{r}"
    );
    assert!(
        r.contains(&format!("Content-Length: {}\r\n", body.len())),
        "{r}"
    );
    assert!(r.ends_with(&format!("\r\n\r\n{body}")), "{r}");
    // A query string is ignored.
    assert!(
        text(&respond("GET /status?x=1 HTTP/1.0", || Some(sample())))
            .starts_with("HTTP/1.0 200 OK")
    );
}

/// The era, the dats and the client versions: what a launcher reads to choose a client and a dat
/// set, at `/v1/world` as at `/status`.
/// Divergence: V418
#[test]
fn the_status_names_the_era_its_systems_the_dats_and_the_client_versions() {
    let mut w = world();
    let s = StatusSnapshot::take(&w, "Dereth", 1);
    assert_eq!(s.era, "eor");
    assert_eq!(s.client_versions, ["1802"]);
    assert_eq!(s.dats, DatIterations::of(&w.dats));
    w.era = empyrean_common::era::EraId::Infiltration.rules();
    let s = StatusSnapshot::take(&w, "Dereth", 1);
    assert_eq!(s.era, "infiltration");
    let json = s.to_json();
    assert!(json.contains(",\"era\":\"infiltration\","), "{json}");
    assert!(json.contains(",\"client_versions\":[\"1802\"],"), "{json}");
    assert_eq!(s.features, empyrean_common::era::EraFeatures::INFILTRATION);
    assert!(
        json.contains(",\"features\":{\"ratings\":false,") && json.contains(",\"chess\":true},"),
        "{json}"
    );
    // A system the world's configuration turns on is announced as on.
    let features = empyrean_common::era::EraFeatures {
        aetheria: true,
        ..empyrean_common::era::EraFeatures::INFILTRATION
    };
    w.era = empyrean_common::era::with_features(w.era, features);
    let s = StatusSnapshot::take(&w, "Dereth", 1);
    assert_eq!((s.era.as_str(), s.features), ("infiltration", features));
    assert!(s.to_json().contains(",\"aetheria\":true,"));

    let v1 = text(&respond("GET /v1/world HTTP/1.1", || Some(sample())));
    let status = text(&respond("GET /status HTTP/1.1", || Some(sample())));
    assert!(v1.starts_with("HTTP/1.0 200 OK\r\n"), "{v1}");
    assert_eq!(v1, status);
}

#[test]
fn health_follows_the_world_thread() {
    assert!(text(&respond("GET /health HTTP/1.1", || Some(sample()))).ends_with("\r\n\r\nok\n"));
    let stuck = text(&respond("GET /health HTTP/1.1", || None));
    assert!(
        stuck.starts_with("HTTP/1.0 503 Service Unavailable\r\n"),
        "{stuck}"
    );
    assert!(text(&respond("GET /status HTTP/1.1", || None)).starts_with("HTTP/1.0 503 "));
}

#[test]
fn other_requests() {
    let mut asked = false;
    let r = text(&respond("GET / HTTP/1.1", || {
        asked = true;
        None
    }));
    assert!(r.starts_with("HTTP/1.0 404 Not Found\r\n"), "{r}");
    assert!(!asked, "an unknown path does not bother the world thread");
    assert!(text(&respond("POST /status HTTP/1.1", || Some(sample()))).starts_with("HTTP/1.0 405 "));
    assert!(text(&respond("", || Some(sample()))).starts_with("HTTP/1.0 405 "));

    let head = text(&respond("HEAD /health HTTP/1.1", || Some(sample())));
    assert!(
        head.starts_with("HTTP/1.0 200 OK\r\n") && head.contains("Content-Length: 3\r\n"),
        "{head}"
    );
    assert!(head.ends_with("\r\n\r\n"), "HEAD has no body: {head}");
}

#[test]
fn address_from_the_command_line_or_the_configuration() {
    let addr = |s: &str| Some(s.parse().expect("socket address"));
    assert_eq!(status_address(None, ""), Ok(None), "empty: off");
    assert_eq!(status_address(None, "  "), Ok(None));
    assert_eq!(
        status_address(None, "127.0.0.1:9100"),
        Ok(addr("127.0.0.1:9100"))
    );
    assert_eq!(
        status_address(Some("127.0.0.1:9200"), "127.0.0.1:9100"),
        Ok(addr("127.0.0.1:9200")),
        "--status wins"
    );
    assert_eq!(
        status_address(Some("[::1]:9100"), ""),
        Ok(addr("[::1]:9100"))
    );
    assert!(status_address(Some("localhost:9100"), "").is_err());
    assert!(status_address(Some("9100"), "").is_err());
    assert!(status_address(None, "nine").is_err());
    // The key is server.status_address.
    let config = empyrean_common::toml_config::from_toml_str(
        "[server]\nstatus_address = \"127.0.0.1:9100\"\n",
    )
    .unwrap()
    .config;
    assert_eq!(
        status_address(None, &config.server.status_address),
        Ok(addr("127.0.0.1:9100"))
    );
    assert_eq!(
        empyrean_common::master_configuration::MasterConfiguration::default()
            .server
            .status_address,
        ""
    );
}

/// `server.source_url` replaces the build's repository in the snapshot (an operator running
/// modified code points players at that code).
#[test]
fn the_configured_source_url_is_reported() {
    let w = world();
    let mut config = empyrean_common::master_configuration::MasterConfiguration::default();
    config.server.source_url = "https://example.org/my-fork".to_owned();
    let _fork = empyrean_common::config_manager::ConfigManager::override_for_thread(config);
    let s = StatusSnapshot::take(&w, "Dereth", 1);
    assert_eq!(s.source_url, "https://example.org/my-fork");
    assert!(
        s.to_json()
            .contains(",\"source_url\":\"https://example.org/my-fork\","),
        "{}",
        s.to_json()
    );
}

/// Divergence: the WebSocket endpoint is Empyrean's own. A fresh world reports no WebSocket URL
/// (the server sets it when the endpoint is on), and only a page origin the endpoint admits may
/// read the status from a browser.
#[test]
fn only_an_admitted_page_origin_may_read_the_status_and_its_websocket_url() {
    assert_eq!(
        StatusSnapshot::take(&world(), "Dereth", 1).websocket_url,
        None
    );
    let admitted = vec!["https://play.example.org".to_owned()];
    assert_eq!(
        cors_header(Some("https://play.example.org"), &admitted).as_deref(),
        Some("Access-Control-Allow-Origin: https://play.example.org\r\nVary: Origin\r\n")
    );
    assert_eq!(
        cors_header(Some("https://elsewhere.example.org"), &admitted),
        None
    );
    assert_eq!(cors_header(None, &admitted), None);
    assert_eq!(cors_header(Some("https://play.example.org"), &[]), None);
}

/// A server that hands over to another (after an update) shuts its endpoint down, and the address
/// is free for the next one; the endpoint answers until then.
#[test]
fn a_shut_down_endpoint_frees_its_address() {
    use std::io::{Read, Write};
    let endpoint =
        empyrean_server::status_endpoint::start("127.0.0.1:0".parse().unwrap(), Vec::new(), || {
            Some(sample())
        })
        .expect("started");
    let addr = endpoint.local_addr();
    let mut s = std::net::TcpStream::connect(addr).expect("it listens");
    s.write_all(b"GET /health HTTP/1.0\r\n\r\n").unwrap();
    let mut answer = String::new();
    let _ = s.read_to_string(&mut answer);
    assert!(answer.starts_with("HTTP/1.0 200"), "{answer}");

    endpoint.shutdown();
    let end = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let rebound = loop {
        match std::net::TcpListener::bind(addr) {
            Ok(l) => break Some(l),
            Err(_) if std::time::Instant::now() < end => {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(_) => break None,
        }
    };
    assert!(rebound.is_some(), "{addr} is free again");
}
