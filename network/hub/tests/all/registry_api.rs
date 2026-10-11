//! The registry's HTTP API on a running hub: resolution, heartbeats with and without the key, the
//! network list and the compatibility feeds.
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
