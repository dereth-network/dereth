//! The registry's HTTP API on a running hub: resolution, heartbeats with and without the key, the
//! network list and the compatibility feeds, each game's list of worlds, a world tried from
//! outside for its host, and (in `status_ping`) what the listed Empyrean worlds said when the hub
//! asked them.
//! Fixture: the conformance fixture's hub, spoken to over plain HTTP/1.1 on loopback.

use std::io::{Read, Write};
use std::net::TcpStream;

fn http(
    addr: std::net::SocketAddr,
    method: &str,
    path: &str,
    auth: Option<&str>,
    body: &str,
) -> (u16, String) {
    let mut s = TcpStream::connect(addr).expect("connect");
    let auth = auth
        .map(|k| format!("Authorization: Bearer {k}\r\n"))
        .unwrap_or_default();
    write!(
        s,
        "{method} {path} HTTP/1.1\r\nHost: localhost\r\n{auth}Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
    .expect("write");
    let mut out = String::new();
    s.read_to_string(&mut out).expect("read");
    let status = out
        .split_whitespace()
        .nth(1)
        .and_then(|c| c.parse().ok())
        .unwrap_or(0);
    let body = out
        .split_once("\r\n\r\n")
        .map(|(_, b)| b.to_owned())
        .unwrap_or_default();
    (status, body)
}

/// Spec: hub: the registry answers over HTTP, and a heartbeat needs the world's key.
#[test]
fn the_registry_answers_over_http_and_a_heartbeat_needs_the_key() {
    let (running, worlds, _) = super::conformance::hub();
    let addr = running.local_addr();
    assert_eq!(http(addr, "GET", "/healthz", None, "").0, 200);
    let (status, body) = http(addr, "GET", "/v1/resolve/aa.dereth.network", None, "");
    assert_eq!(status, 200, "{body}");
    assert!(
        body.contains("\"configuredAddress\":\"192.0.2.1\""),
        "{body}"
    );
    let beat = r#"{"state":"online","playersOnline":7,"playersKind":"connected-characters"}"#;
    assert_eq!(
        http(addr, "POST", "/v1/servers/aa/heartbeat", None, beat).0,
        401
    );
    assert_eq!(
        http(
            addr,
            "POST",
            "/v1/servers/aa/heartbeat",
            Some(&worlds[1].1),
            beat
        )
        .0,
        401,
        "another world's key"
    );
    let (status, body) = http(
        addr,
        "POST",
        "/v1/servers/aa/heartbeat",
        Some(&worlds[0].1),
        beat,
    );
    assert_eq!(status, 200, "{body}");
    let (_, one) = http(addr, "GET", "/v1/servers/aa", None, "");
    assert!(
        one.contains("\"playersOnline\":7") && one.contains("\"federation\":\"on\""),
        "{one}"
    );
    let (_, network) = http(addr, "GET", "/v1/network", None, "");
    assert!(
        network.contains("\"slug\":\"aa\"") && !network.contains("\"slug\":\"dd\""),
        "{network}"
    );
    let (status, xml) = http(addr, "GET", "/compat/thwarg/Servers.xml", None, "");
    assert_eq!(status, 200);
    assert!(
        xml.contains("<server_host>192.0.2.1</server_host>"),
        "{xml}"
    );
    let (_, counts) = http(
        addr,
        "GET",
        "/compat/treestats/player_counts-latest.json",
        None,
        "",
    );
    assert!(counts.contains("\"count\":7"), "{counts}");
    assert_eq!(http(addr, "GET", "/v1/nothing", None, "").0, 404);
    running.shutdown();
}

/// Spec: hub: the list of worlds names a world's web addresses while its link is up, as its
/// `hello` named them, and none for a world whose link is down.
#[test]
fn the_list_names_a_linked_worlds_web_addresses() {
    let (running, worlds, _) = super::conformance::hub();
    let addr = running.local_addr();
    let base = format!("ws://{addr}");
    let (aa, ka) = &worlds[0];
    let mut la = dereth_network_conformance::hub::Link::open(&base, aa, ka).expect("aa links");
    la.send(&dereth_network_protocol::Message::Hello {
        proto: dereth_network_protocol::wire::PROTOCOL,
        software: "test".into(),
        websocket_url: Some("wss://aa.example.org/ws".into()),
        status_url: Some("https://aa.example.org/status".into()),
    })
    .expect("hello");
    la.expect(std::time::Duration::from_secs(5), "welcome", |m| {
        matches!(m, dereth_network_protocol::Message::Welcome { .. }).then_some(())
    })
    .expect("welcomed");
    let (status, body) = http(addr, "GET", "/v1/servers?limit=100", None, "");
    assert_eq!(status, 200, "{body}");
    let list: serde_json::Value = serde_json::from_str(&body).expect("JSON");
    let world = |slug: &str| {
        list["data"]
            .as_array()
            .and_then(|a| a.iter().find(|w| w["slug"] == slug))
            .cloned()
            .unwrap_or_else(|| panic!("{slug} listed: {body}"))
    };
    assert_eq!(world("aa")["websocketUrl"], "wss://aa.example.org/ws");
    assert_eq!(world("aa")["statusUrl"], "https://aa.example.org/status");
    assert!(world("bb")["websocketUrl"].is_null() && world("bb")["statusUrl"].is_null());
    la.close();
    running.shutdown();
}

/// `{"websocketUrl": <url>}` as a JSON body.
fn announcement(url: &str) -> String {
    serde_json::json!({ "websocketUrl": url }).to_string()
}

/// One world as `GET /v1/servers/<slug>` shows it.
fn public_world(addr: std::net::SocketAddr, slug: &str) -> serde_json::Value {
    let (status, body) = http(addr, "GET", &format!("/v1/servers/{slug}"), None, "");
    assert_eq!(status, 200, "{body}");
    serde_json::from_str(&body).expect("JSON")
}

/// Spec: hub: a world announces its WebSocket endpoint over HTTP with its key, within the link's
/// rule for the address; the public answers list it as the world's `websocketUrl`, and `null`
/// withdraws it.
#[test]
fn an_announced_websocket_endpoint_needs_the_key_and_a_good_address() {
    let (running, worlds, _) = super::conformance::hub();
    let addr = running.local_addr();
    let (aa, ka) = &worlds[0];
    let path = format!("/v1/servers/{aa}/web");
    let good = announcement("wss://relay.example.org/");
    assert_eq!(http(addr, "POST", &path, None, &good).0, 401, "no key");
    assert_eq!(
        http(addr, "POST", &path, Some(&worlds[1].1), &good).0,
        401,
        "another world's key"
    );
    for refused in [
        "ws://192.0.2.5:9180/",
        "https://relay.example.org/",
        "wss://user@relay.example.org/",
        &format!("wss://relay.example.org/{}", "x".repeat(200)),
    ] {
        let (status, body) = http(addr, "POST", &path, Some(ka), &announcement(refused));
        assert_eq!(status, 400, "{refused}: {body}");
    }
    for malformed in ["", "{}", r#"{"websocketUrl": 7}"#, "[]"] {
        assert_eq!(
            http(addr, "POST", &path, Some(ka), malformed).0,
            400,
            "{malformed}"
        );
    }
    assert!(public_world(addr, aa)["websocketUrl"].is_null());

    let (status, body) = http(addr, "POST", &path, Some(ka), &good);
    assert_eq!(status, 200, "{body}");
    let answer: serde_json::Value = serde_json::from_str(&body).expect("JSON");
    assert_eq!(answer["websocketUrl"], "wss://relay.example.org/");
    assert!(answer["expiresAt"].is_string(), "{body}");
    let one = public_world(addr, aa);
    assert_eq!(one["websocketUrl"], "wss://relay.example.org/");
    assert!(
        one.get("announcedWebsocketUrl").is_none(),
        "the public answer names one address: {one}"
    );
    let (_, list) = http(addr, "GET", "/v1/servers?limit=100", None, "");
    let list: serde_json::Value = serde_json::from_str(&list).expect("JSON");
    let listed = list["data"]
        .as_array()
        .and_then(|a| a.iter().find(|w| w["slug"] == aa.as_str()))
        .expect("aa listed");
    assert_eq!(listed["websocketUrl"], "wss://relay.example.org/");
    let (status, resolved) = http(addr, "GET", &format!("/v1/resolve/{aa}"), None, "");
    assert_eq!(status, 200, "{resolved}");
    assert!(
        resolved.contains("\"websocketUrl\":\"wss://relay.example.org/\""),
        "{resolved}"
    );
    assert!(
        public_world(addr, &worlds[1].0)["websocketUrl"].is_null(),
        "another world names none"
    );

    let (status, body) = http(addr, "POST", &path, Some(ka), r#"{"websocketUrl":null}"#);
    assert_eq!(status, 200, "{body}");
    assert!(
        public_world(addr, aa)["websocketUrl"].is_null(),
        "withdrawn"
    );
    running.shutdown();
}

/// Spec: hub: an announced WebSocket endpoint is listed for as long as a heartbeat keeps a world's
/// status fresh, and again once it is announced again.
#[test]
fn an_announced_websocket_endpoint_expires_with_a_heartbeats_lifetime() {
    let (running, worlds, _, clock) =
        super::conformance::hub_with_clock(dereth_network_hub::server::HubConfig::default());
    let addr = running.local_addr();
    let (aa, ka) = &worlds[0];
    let path = format!("/v1/servers/{aa}/web");
    let good = announcement("wss://relay.example.org/");
    assert_eq!(http(addr, "POST", &path, Some(ka), &good).0, 200);
    let ttl = std::time::Duration::from_secs(dereth_network_hub::registry::HEARTBEAT_TTL_SECS);
    clock.advance(ttl - std::time::Duration::from_secs(1));
    assert_eq!(
        public_world(addr, aa)["websocketUrl"],
        "wss://relay.example.org/",
        "still fresh"
    );
    clock.advance(std::time::Duration::from_secs(2));
    assert!(
        public_world(addr, aa)["websocketUrl"].is_null(),
        "expired with the heartbeat's lifetime"
    );
    assert_eq!(http(addr, "POST", &path, Some(ka), &good).0, 200);
    assert_eq!(
        public_world(addr, aa)["websocketUrl"],
        "wss://relay.example.org/",
        "announced again"
    );
    running.shutdown();
}

/// Spec: hub: while a world's link is up, the WebSocket endpoint its `hello` named is listed in
/// place of one it announced; a `hello` that names none leaves the announced one listed, and the
/// announced one is listed again once the link is down.
#[test]
fn a_linked_worlds_own_websocket_endpoint_wins_over_an_announced_one() {
    use dereth_network_protocol::Message;
    // The wait below asks the hub again until the link is down: an allowance it cannot spend.
    let (running, worlds, _) =
        super::conformance::hub_with(dereth_network_hub::server::HubConfig {
            http_per_minute: 1_000_000,
            ..dereth_network_hub::server::HubConfig::default()
        });
    let addr = running.local_addr();
    let base = format!("ws://{addr}");
    let link = |slug: &str, key: &str, websocket_url: Option<&str>| {
        let mut l = dereth_network_conformance::hub::Link::open(&base, slug, key).expect("links");
        l.send(&Message::Hello {
            proto: dereth_network_protocol::wire::PROTOCOL,
            software: "test".into(),
            websocket_url: websocket_url.map(str::to_owned),
            status_url: None,
        })
        .expect("hello");
        l.expect(std::time::Duration::from_secs(5), "welcome", |m| {
            matches!(m, Message::Welcome { .. }).then_some(())
        })
        .expect("welcomed");
        l
    };
    for (slug, key) in &worlds[..2] {
        let (status, body) = http(
            addr,
            "POST",
            &format!("/v1/servers/{slug}/web"),
            Some(key),
            &announcement(&format!("wss://relay-{slug}.example.org/")),
        );
        assert_eq!(status, 200, "{body}");
    }
    let (aa, ka) = &worlds[0];
    let (bb, kb) = &worlds[1];
    let la = link(aa, ka, Some("wss://aa.example.org/ws"));
    let lb = link(bb, kb, None);
    assert_eq!(
        public_world(addr, aa)["websocketUrl"],
        "wss://aa.example.org/ws"
    );
    assert_eq!(
        public_world(addr, bb)["websocketUrl"],
        "wss://relay-bb.example.org/",
        "a hello that names none"
    );
    la.close();
    lb.close();
    // The hub notices the closed link on its own thread: wait for the event, not for a while.
    let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while public_world(addr, aa)["websocketUrl"] != "wss://relay-aa.example.org/" {
        assert!(
            std::time::Instant::now() < until,
            "the announced address once the link is down"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    running.shutdown();
}

/// `POST /v1/probe` on a running hub: a world's host asks the hub to try the world from outside.
/// Fixture: the operators' fixture hub (loopback allowed, its registry on a clock the test moves),
/// and stand-in worlds on loopback that answer the status ping, the server-tracker login, or
/// nothing.
mod probe {
    use std::net::UdpSocket;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::thread::JoinHandle;
    use std::time::Duration;

    use dereth_network_hub::probe::Timing;
    use dereth_network_protocol::limits::Rate;
    use dereth_network_protocol::probe::{self, Status};

    use super::super::operators::{call, hub_with, Answer, Fixture};

    /// What a stand-in world answers.
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Speaks {
        /// The status ping only, as an Empyrean whose tracker answer is lost.
        StatusPing,
        /// The server-tracker login only, as ACE and GDLE.
        Tracker,
        /// Nothing at all.
        Nothing,
    }

    /// A world on loopback that answers as told until dropped.
    struct World {
        port: u16,
        stop: Arc<AtomicBool>,
        thread: Option<JoinHandle<()>>,
    }

    const TOKEN: [u8; probe::TOKEN_LEN] = [3; probe::TOKEN_LEN];

    impl World {
        fn new(speaks: Speaks) -> Self {
            let socket = UdpSocket::bind("127.0.0.1:0").expect("a game port");
            socket
                .set_read_timeout(Some(Duration::from_millis(50)))
                .expect("a timeout");
            let port = socket.local_addr().expect("its address").port();
            let stop = Arc::new(AtomicBool::new(false));
            let stopping = Arc::clone(&stop);
            let thread = std::thread::spawn(move || {
                let mut buf = [0u8; 1500];
                while !stopping.load(Ordering::SeqCst) {
                    let Ok((n, from)) = socket.recv_from(&mut buf) else {
                        continue;
                    };
                    let d = &buf[..n];
                    let reply = match (speaks, probe::status_request(d)) {
                        (Speaks::StatusPing, Some(None)) => probe::status_token_reply(&TOKEN),
                        (Speaks::StatusPing, Some(Some(t))) if t == TOKEN => Status {
                            state: 1,
                            players: 3,
                            software: "Empyrean".into(),
                            version: "0.3.3".into(),
                            name: "Stand-in".into(),
                            ..Status::default()
                        }
                        .encode(),
                        (Speaks::Tracker, None) if d == probe::TRACKER_LOGIN => {
                            // A packet with no body: what a server's first handshake reply starts as.
                            vec![0u8; probe::HEADER_LEN]
                        }
                        _ => continue,
                    };
                    let _ = socket.send_to(&reply, from);
                }
            });
            Self {
                port,
                stop,
                thread: Some(thread),
            }
        }
    }

    impl Drop for World {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::SeqCst);
            if let Some(t) = self.thread.take() {
                let _ = t.join();
            }
        }
    }

    /// A hub whose probes wait `wait` at most, a copy every tenth of a second.
    fn hub(
        wait: Duration,
        tweak: impl FnOnce(&mut dereth_network_hub::server::HubConfig),
    ) -> Fixture {
        hub_with(|c| {
            c.probe_timing = Timing {
                every: Duration::from_millis(100),
                wait,
            };
            tweak(c);
        })
    }

    fn probe_port(f: &Fixture, body: &str) -> Answer {
        call(
            f.addr(),
            "POST",
            "/v1/probe",
            &[("Content-Type", "application/json")],
            body,
        )
    }

    fn probe_at(f: &Fixture, port: u16) -> Answer {
        probe_port(f, &format!(r#"{{"port":{port}}}"#))
    }

    /// Long enough that a world on loopback always answers first, however loaded the machine; the
    /// probe ends as soon as it does.
    const PATIENT: Duration = Duration::from_secs(60);

    /// Spec: hub: a probe tries the caller's own address at the port it names, and an Empyrean
    /// world's answer says its name, players, software, version and state.
    #[test]
    fn a_probe_tries_the_callers_own_address_and_reads_what_the_world_says() {
        let world = World::new(Speaks::StatusPing);
        let f = hub(PATIENT, |_| {});
        let a = probe_at(&f, world.port);
        assert_eq!(a.status, 200, "{}", a.body);
        let v = a.json();
        assert_eq!(v["address"], "127.0.0.1", "the caller's own address");
        assert_eq!(v["port"], world.port);
        assert_eq!(v["reached"], true);
        assert_eq!(v["world"]["name"], "Stand-in");
        assert_eq!(v["world"]["players"], 3);
        assert_eq!(v["world"]["software"], "Empyrean");
        assert_eq!(v["world"]["version"], "0.3.3");
        assert_eq!(v["world"]["state"], "open");
        assert_eq!(a.header("access-control-allow-origin"), Some("*"));
    }

    /// Spec: hub: a world that answers only the server-tracker login, as ACE and GDLE do, is reached,
    /// with nothing said of it.
    #[test]
    fn a_world_that_answers_only_the_tracker_login_is_reached() {
        let world = World::new(Speaks::Tracker);
        let f = hub(PATIENT, |_| {});
        let a = probe_at(&f, world.port);
        assert_eq!(a.status, 200, "{}", a.body);
        let v = a.json();
        assert_eq!(v["reached"], true);
        assert!(v["world"].is_null(), "{v}");
    }

    /// Spec: hub: a world that answers nothing is not reached.
    #[test]
    fn a_world_that_answers_nothing_is_not_reached() {
        let world = World::new(Speaks::Nothing);
        let f = hub(Duration::from_millis(300), |_| {});
        let a = probe_at(&f, world.port);
        assert_eq!(a.status, 200, "{}", a.body);
        let v = a.json();
        assert_eq!(v["reached"], false);
        assert!(v["world"].is_null());
    }

    /// Spec: hub: a probe names a port from 1024 to 65535 with POST; anything else is refused before
    /// anything is sent.
    #[test]
    fn a_probe_needs_post_and_a_port_from_1024() {
        let f = hub(PATIENT, |_| {});
        for body in [
            r#"{"port":80}"#,
            r#"{"port":70000}"#,
            r#"{"host":"192.0.2.1"}"#,
            "nonsense",
        ] {
            let a = probe_port(&f, body);
            assert_eq!(a.status, 400, "{body}: {}", a.body);
            assert_eq!(a.json()["error"]["code"], "INVALID_REQUEST");
        }
        let a = call(f.addr(), "GET", "/v1/probe", &[], "");
        assert_eq!(a.status, 405, "{}", a.body);
    }

    /// Spec: hub: a hub that does not take loopback for the internet never tries a caller on
    /// loopback, or on any address it would not reach on the internet.
    #[test]
    fn a_caller_not_on_the_internet_is_refused() {
        let world = World::new(Speaks::StatusPing);
        let f = hub(PATIENT, |c| c.loopback_allowed = false);
        let a = probe_at(&f, world.port);
        assert_eq!(a.status, 403, "{}", a.body);
    }

    /// Spec: hub: a caller has five probes, then one every twelve minutes on the hub's clock; the
    /// sixth is refused `429` with when to try again.
    #[test]
    fn a_callers_probes_are_limited_on_the_hubs_clock() {
        let world = World::new(Speaks::Tracker);
        let f = hub(PATIENT, |_| {});
        for i in 0..5 {
            let a = probe_at(&f, world.port);
            assert_eq!(a.status, 200, "probe {i}: {}", a.body);
        }
        let a = probe_at(&f, world.port);
        assert_eq!(a.status, 429, "{}", a.body);
        assert_eq!(a.header("retry-after"), Some("720"));
        assert_eq!(a.json()["error"]["retryAfterSeconds"], 720);
        f.advance(Duration::from_secs(720));
        let a = probe_at(&f, world.port);
        assert_eq!(a.status, 200, "{}", a.body);
    }

    /// Spec: hub: the hub's own allowance, shared by every caller, refuses a probe once spent.
    #[test]
    fn the_hubs_own_allowance_is_shared_by_every_caller() {
        let world = World::new(Speaks::Tracker);
        let f = hub(PATIENT, |c| {
            c.probe_per_caller = Rate {
                burst: 100.0,
                per_sec: 0.0,
            };
            c.probe_overall = Rate {
                burst: 2.0,
                per_sec: 0.1,
            };
        });
        assert_eq!(probe_at(&f, world.port).status, 200);
        assert_eq!(probe_at(&f, world.port).status, 200);
        let a = probe_at(&f, world.port);
        assert_eq!(a.status, 429, "{}", a.body);
        assert_eq!(a.header("retry-after"), Some("10"));
        f.advance(Duration::from_secs(10));
        assert_eq!(probe_at(&f, world.port).status, 200);
    }
}

fn game_registration(slug: &str, emulator: &str) -> dereth_network_hub::registry::Registration {
    dereth_network_hub::registry::Registration::from_json(&serde_json::json!({
        "slug": slug, "name": slug.to_uppercase(), "ruleset": "PvE",
        "endpoint": { "mode": "manual", "address": "192.0.2.7", "port": 9000 },
        "emulator": { "family": emulator },
    }))
    .expect("a registration")
}

/// A world's `population` once it is `wanted`, read from the list, whose snapshot may be a moment
/// behind. The wait only guards against a hang.
fn population_until(
    addr: std::net::SocketAddr,
    slug: &str,
    wanted: impl Fn(&serde_json::Value) -> bool,
) -> serde_json::Value {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        let (status, body) = http(addr, "GET", "/v1/servers", None, "");
        assert_eq!(status, 200, "{body}");
        let page: serde_json::Value = serde_json::from_str(&body).expect("a page");
        let row = page["data"]
            .as_array()
            .expect("rows")
            .iter()
            .find(|w| w["slug"] == slug)
            .expect("listed");
        if wanted(&row["population"]) {
            return row["population"].clone();
        }
        assert!(std::time::Instant::now() < deadline, "{row}");
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// Spec: hub: a client's report for a listed world's address and port is listed with its source and
/// age and left out of the network's totals, and a world's own heartbeat goes before it; a report
/// that names no listed world is answered alike and leaves nothing; a report past the limits or out
/// of range is refused.
#[test]
fn a_clients_report_is_listed_until_the_worlds_own_word_takes_over() {
    let (running, worlds, _) = super::conformance::hub();
    let addr = running.local_addr();
    let port = public_world(addr, "aa")["endpoint"]["port"]
        .as_u64()
        .expect("a port");
    let report = |host: &str, players: u32| {
        format!(r#"{{"host":"{host}","port":{port},"players":{players}}}"#)
    };
    let post = |body: &str| http(addr, "POST", "/v1/players", None, body);

    let nowhere = post(&report("nowhere.example.org", 9));
    assert_eq!(nowhere.0, 200, "{nowhere:?}");
    assert_eq!(nowhere.1, "{}");
    assert_eq!(
        public_world(addr, "aa")["population"],
        serde_json::Value::Null
    );

    assert_eq!(post(&report("192.0.2.1", 5001)).0, 400, "past the most");
    assert_eq!(http(addr, "GET", "/v1/players", None, "").0, 405);

    let said = post(&report("192.0.2.1", 9));
    assert_eq!(said, nowhere, "answered alike");
    let shown = population_until(addr, "aa", |p| p["source"] == "client");
    assert_eq!(shown["players"], 9);
    assert_eq!(
        public_world(addr, "aa")["population"]["players"],
        9,
        "one world too"
    );
    assert!(shown["ageSeconds"].is_u64(), "{shown}");

    let (_, totals) = http(addr, "GET", "/v1/status", None, "");
    let totals: serde_json::Value = serde_json::from_str(&totals).expect("totals");
    assert_eq!(totals["playersOnline"], 0, "not in the totals");
    let (_, counts) = http(
        addr,
        "GET",
        "/compat/treestats/player_counts-latest.json",
        None,
        "",
    );
    assert!(!counts.contains("\"count\":9"), "not in the feed: {counts}");

    let beat = r#"{"state":"online","playersOnline":7,"playersKind":"connected-characters"}"#;
    let (status, body) = http(
        addr,
        "POST",
        "/v1/servers/aa/heartbeat",
        Some(&worlds[0].1),
        beat,
    );
    assert_eq!(status, 200, "{body}");
    let shown = population_until(addr, "aa", |p| p["source"] == "heartbeat");
    assert_eq!(shown["players"], 7);

    for _ in 0..8 {
        if post(&report("192.0.2.1", 1)).0 == 429 {
            return;
        }
    }
    panic!("one address has only a few reports");
}

/// Spec: hub: a hub told where TreeStats' counts are reads them by itself and lists a count for a
/// world nothing better speaks for, under the world's name.
#[test]
fn a_world_with_nothing_better_has_treestats_count() {
    use std::io::{Read as _, Write as _};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let url = format!(
        "http://{}/counts.json",
        listener.local_addr().expect("addr")
    );
    let (running, _, _) = super::conformance::hub();
    let name = public_world(running.local_addr(), "aa")["name"]
        .as_str()
        .expect("a name")
        .to_owned();
    drop(running);
    let now = dereth_network_hub::registry::iso(std::time::SystemTime::now());
    let date = format!("{} {} UTC", &now[..10], &now[11..19]);
    let body = format!(r#"[{{"server":"{name}","count":630,"date":"{date}"}}]"#);
    std::thread::spawn(move || {
        while let Ok((mut stream, _)) = listener.accept() {
            let mut head = [0u8; 2048];
            let _ = stream.read(&mut head);
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    let (running, _, _) = super::conformance::hub_with(dereth_network_hub::server::HubConfig {
        treestats_url: Some(url),
        loopback_allowed: true,
        ..Default::default()
    });
    let shown = population_until(running.local_addr(), "aa", |p| p["source"] == "treestats");
    assert_eq!(shown["players"], 630);
}

/// The slugs and games of a page of `/v1/servers`, in its order.
fn listed_games(addr: std::net::SocketAddr, query: &str) -> Vec<(String, String)> {
    let (status, body) = http(addr, "GET", &format!("/v1/servers{query}"), None, "");
    assert_eq!(status, 200, "{query}: {body}");
    let page: serde_json::Value = serde_json::from_str(&body).expect("a page");
    page["data"]
        .as_array()
        .expect("rows")
        .iter()
        .map(|w| {
            (
                w["slug"].as_str().unwrap_or("").to_owned(),
                w["game"].as_str().unwrap_or("").to_owned(),
            )
        })
        .collect()
}

/// Spec: hub: a world that names no game is listed as `dereth`'s, one whose heartbeat names
/// `auberean` as `auberean`'s; a list that names no game, and the community list and population
/// feeds, show `dereth`'s worlds alone, a game's list that game's, and `all` both.
#[test]
fn each_games_list_shows_its_own_worlds_and_a_list_naming_none_shows_dereths() {
    let registry = dereth_network_hub::registry::Registry::open(
        ":memory:",
        "dereth.network",
        Box::new(std::time::SystemTime::now),
    )
    .expect("a registry");
    let (_, linked_key) = registry
        .register(&game_registration("coldeve", "Empyrean"))
        .expect("registered");
    registry
        .set_federation("coldeve", dereth_network_protocol::Federation::On)
        .expect("on");
    let (_, reporting_key) = registry
        .register(&game_registration("shardfall", "Example"))
        .expect("registered");
    let running = dereth_network_hub::server::start_with(
        dereth_network_hub::server::HubConfig {
            listen: "127.0.0.1:0".parse().expect("an address"),
            db: ":memory:".into(),
            ..dereth_network_hub::server::HubConfig::default()
        },
        registry,
    )
    .expect("the hub starts");
    let addr = running.local_addr();

    // The first world holds a link, says hello naming no game, and reports its status there.
    let mut link = dereth_network_conformance::hub::Link::open(
        &format!("ws://{addr}"),
        "coldeve",
        &linked_key,
    )
    .expect("coldeve links");
    link.greet().expect("welcomed");
    link.send(&dereth_network_protocol::Message::Status {
        state: "online".into(),
        players_online: Some(5),
    })
    .expect("status");

    // The second reports over HTTP, naming its game.
    let beat = dereth_network_protocol::heartbeat::Heartbeat {
        state: "online".into(),
        players_online: Some(2),
        players_kind: Some(dereth_network_protocol::heartbeat::CONNECTED_CHARACTERS.into()),
        emulator_version: Some("0.1.0".into()),
        adapter: Some(dereth_network_protocol::heartbeat::Adapter {
            kind: "yalain".into(),
            version: "0.1.0".into(),
        }),
        game: Some(dereth_network_protocol::Game::Auberean),
    };
    let body = serde_json::to_string(&beat).expect("written");
    let read: dereth_network_hub::registry::Heartbeat =
        serde_json::from_str(&body).expect("the hub reads what a world writes");
    assert_eq!(read.game, Some(dereth_network_protocol::Game::Auberean));
    let (status, answer) = http(
        addr,
        "POST",
        &dereth_network_protocol::heartbeat::path("shardfall"),
        Some(&reporting_key),
        &body,
    );
    assert_eq!(status, 200, "{answer}");

    let (_, one) = http(addr, "GET", "/v1/servers/shardfall", None, "");
    let one: serde_json::Value = serde_json::from_str(&one).expect("a world");
    assert_eq!(one["game"], "auberean");
    assert_eq!(one["status"]["playersOnline"], 2);
    assert_eq!(one["status"]["reported"]["adapter"]["kind"], "yalain");

    let dereth = vec![("coldeve".to_owned(), "dereth".to_owned())];
    let auberean = vec![("shardfall".to_owned(), "auberean".to_owned())];
    assert_eq!(listed_games(addr, ""), dereth, "a list naming no game");
    assert_eq!(listed_games(addr, "?game=dereth"), dereth);
    assert_eq!(listed_games(addr, "?game=auberean"), auberean);
    let mut every = listed_games(addr, "?game=all&limit=100");
    every.sort();
    assert_eq!(every, [dereth.clone(), auberean].concat());
    assert!(listed_games(addr, "?game=ac2").is_empty());

    // The totals count both games' worlds, and each game's apart; they come from the same
    // snapshot as the lists above, so whichever of coldeve's reports it holds, the games add up.
    let (_, status) = http(addr, "GET", "/v1/status", None, "");
    let status: serde_json::Value = serde_json::from_str(&status).expect("the totals");
    assert_eq!(status["total"], 2, "{status}");
    let (d, a) = (&status["games"]["dereth"], &status["games"]["auberean"]);
    assert_eq!(
        (d["total"].as_u64(), a["total"].as_u64()),
        (Some(1), Some(1)),
        "{status}"
    );
    assert_eq!(a["playersOnline"], 2, "{status}");
    assert_eq!(
        (&d["federatedServers"], &a["federatedServers"]),
        (&serde_json::json!(1), &serde_json::json!(0))
    );
    assert_eq!(
        status["playersOnline"].as_u64(),
        Some(d["playersOnline"].as_u64().expect("dereth's") + 2)
    );

    let (_, xml) = http(addr, "GET", "/compat/thwarg/Servers.xml", None, "");
    assert!(
        xml.contains("COLDEVE") && !xml.contains("SHARDFALL"),
        "{xml}"
    );
    let (_, counts) = http(
        addr,
        "GET",
        "/compat/treestats/player_counts-latest.json",
        None,
        "",
    );
    assert!(
        !counts.contains("SHARDFALL") && !counts.contains("shardfall"),
        "{counts}"
    );

    link.close();
    running.shutdown();
}

mod status_ping {
    //! The hub asks the Empyrean worlds it lists how they are, and lists what each said.
    //! Fixture: a hub on loopback over an in-memory registry with two worlds on loopback, an
    //! Empyrean stand-in that answers the status ping and an ACE stand-in that counts what it is
    //! sent.

    use std::io::{Read, Write};
    use std::net::{SocketAddr, TcpStream, UdpSocket};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::thread::JoinHandle;
    use std::time::{Duration, Instant, SystemTime};

    use dereth_network_hub::probe::Timing;
    use dereth_network_hub::registry::{Registration, Registry};
    use dereth_network_hub::server::{start_with, HubConfig};
    use dereth_network_protocol::probe::{self, Status};
    use serde_json::Value;

    /// A world on loopback that answers the status ping as an Empyrean when it is one, and counts
    /// every datagram it is sent, until dropped.
    struct StandIn {
        port: u16,
        heard: Arc<AtomicUsize>,
        stop: Arc<AtomicBool>,
        thread: Option<JoinHandle<()>>,
    }

    const TOKEN: [u8; probe::TOKEN_LEN] = [5; probe::TOKEN_LEN];

    impl StandIn {
        fn new(answers: Option<Status>) -> Self {
            let socket = UdpSocket::bind("127.0.0.1:0").expect("a game port");
            socket
                .set_read_timeout(Some(Duration::from_millis(50)))
                .expect("a timeout");
            let port = socket.local_addr().expect("its address").port();
            let heard = Arc::new(AtomicUsize::new(0));
            let stop = Arc::new(AtomicBool::new(false));
            let (counting, stopping) = (Arc::clone(&heard), Arc::clone(&stop));
            let thread = std::thread::spawn(move || {
                let mut buf = [0u8; 1500];
                while !stopping.load(Ordering::SeqCst) {
                    let Ok((n, from)) = socket.recv_from(&mut buf) else {
                        continue;
                    };
                    counting.fetch_add(1, Ordering::SeqCst);
                    let Some(status) = &answers else {
                        continue;
                    };
                    let reply = match probe::status_request(&buf[..n]) {
                        Some(None) => probe::status_token_reply(&TOKEN),
                        Some(Some(t)) if t == TOKEN => status.encode(),
                        _ => continue,
                    };
                    let _ = socket.send_to(&reply, from);
                }
            });
            Self {
                port,
                heard,
                stop,
                thread: Some(thread),
            }
        }
    }

    impl Drop for StandIn {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::SeqCst);
            if let Some(t) = self.thread.take() {
                let _ = t.join();
            }
        }
    }

    fn register(registry: &Registry, slug: &str, family: &str, port: u16) {
        let r = Registration::from_json(&serde_json::json!({
            "slug": slug, "name": slug.to_uppercase(), "ruleset": "PvE",
            "endpoint": { "mode": "manual", "address": "127.0.0.1", "port": port },
            "emulator": { "family": family },
        }))
        .expect("a registration");
        registry.register(&r).expect("registered");
    }

    fn get(addr: SocketAddr, path: &str) -> Value {
        let mut s = TcpStream::connect(addr).expect("connect");
        write!(
            s,
            "GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
        )
        .expect("write");
        let mut out = String::new();
        s.read_to_string(&mut out).expect("read");
        let body = out.split_once("\r\n\r\n").map_or("", |(_, b)| b);
        serde_json::from_str(body).unwrap_or(Value::Null)
    }

    fn listed<'a>(list: &'a Value, slug: &str) -> &'a Value {
        list["data"]
            .as_array()
            .and_then(|a| a.iter().find(|w| w["slug"] == slug))
            .expect("listed")
    }

    /// Spec: hub: the hub asks a listed Empyrean world with the status ping and lists its state,
    /// players, era, systems, software, version and name; a world whose emulator is not Empyrean is
    /// never sent it.
    #[test]
    fn the_hub_lists_what_an_empyrean_world_says_and_never_asks_another() {
        let empyrean = StandIn::new(Some(Status {
            state: 1,
            players: 4,
            era: "infiltration".into(),
            table: 1,
            systems: vec![0x00, 0x00, 0xff],
            software: "Empyrean".into(),
            version: "0.3.7".into(),
            name: "Stand-in".into(),
        }));
        let ace = StandIn::new(None);
        let registry = Registry::open(":memory:", "dereth.network", Box::new(SystemTime::now))
            .expect("a registry");
        register(&registry, "inf", "Empyrean", empyrean.port);
        register(&registry, "ace", "ACE", ace.port);
        let config = HubConfig {
            listen: "127.0.0.1:0".parse().expect("an address"),
            db: ":memory:".into(),
            loopback_allowed: true,
            // Patient: a world on loopback answers first however loaded the machine; the ask ends
            // as soon as it does.
            probe_timing: Timing {
                every: Duration::from_millis(100),
                wait: Duration::from_secs(60),
            },
            ..HubConfig::default()
        };
        let running = start_with(config, registry).expect("the hub starts");
        let addr = running.local_addr();

        // Waits for the answer, never for a time; the guard only stops a hang.
        let guard = Instant::now() + Duration::from_secs(120);
        let list = loop {
            let list = get(addr, "/v1/servers");
            if !listed(&list, "inf")["statusPing"].is_null() {
                break list;
            }
            assert!(Instant::now() < guard, "the hub never listed the answer");
            std::thread::sleep(Duration::from_millis(50));
        };
        let said = &listed(&list, "inf")["statusPing"];
        assert_eq!(said["state"], "open");
        assert_eq!(said["playersOnline"], 4);
        assert_eq!(said["era"], "infiltration");
        assert_eq!(said["eraFeatures"], "1:0000ff");
        assert_eq!(said["software"], "Empyrean");
        assert_eq!(said["version"], "0.3.7");
        assert_eq!(said["name"], "Stand-in");
        assert!(said["expiresAt"].is_string() && said["askedAt"].is_string());
        assert_eq!(
            get(addr, "/v1/servers/inf")["statusPing"]["era"],
            "infiltration",
            "the world's own answer carries it too"
        );

        assert!(listed(&list, "ace")["statusPing"].is_null());
        assert_eq!(ace.heard.load(Ordering::SeqCst), 0, "ACE is never asked");
        running.shutdown();
    }
}
