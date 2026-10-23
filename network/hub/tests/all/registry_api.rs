//! The registry's HTTP API on a running hub: resolution, heartbeats with and without the key, the
//! network list and the compatibility feeds, and a world tried from outside for its host.
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
