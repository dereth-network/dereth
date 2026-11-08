//! Tells the Dereth Network's hub how many players a world said it has.
//!
//! A world tells each client that logs in how many players are on (`Login_WorldInfo`, `0xF7E1`).
//! This client passes that count on, for the launcher's list of worlds: the world's address and
//! port and the count, and nothing else: no account, no character, no id of this client. There is
//! no switch for it; the project's README says so. The hub keeps the latest count and its time.
//!
//! It runs on a thread of its own, and a report that fails is forgotten, so it can neither delay
//! nor break a login. A world on this computer or a private network is nobody else's to count, so
//! it is never reported.

use std::net::IpAddr;
use std::time::Duration;

/// The hub reported to unless [`HUB_ENV`] names another.
pub const HUB: &str = "https://hub.dereth.network";

/// Names another hub to report to: a launcher reading another hub's list sets it.
pub const HUB_ENV: &str = "DERETH_PLAYER_COUNT_HUB";

/// How long a report may take.
const TIMEOUT: Duration = Duration::from_secs(5);

/// The world to report `host` and `port` (the address connected to) as, unless that is on this
/// computer or a private network.
fn target(host: &str, port: u32) -> Option<(String, u16)> {
    // The first of a list of hosts, as `host` or `host:port`.
    let first = host.split(',').next()?.trim();
    let (host, port) = match first.rsplit_once(':') {
        Some((h, p)) if !h.contains(':') => (h, p.parse().ok()?),
        _ => (first, u16::try_from(port).ok()?),
    };
    let private = match host.trim_matches(['[', ']']).parse::<IpAddr>() {
        Ok(IpAddr::V4(ip)) => {
            ip.is_loopback() || ip.is_private() || ip.is_link_local() || ip.is_unspecified()
        }
        Ok(IpAddr::V6(ip)) => {
            let first = ip.segments()[0];
            ip.is_loopback()
                || ip.is_unspecified()
                || first & 0xfe00 == 0xfc00
                || first & 0xffc0 == 0xfe80
        }
        Err(_) => host.is_empty() || host.eq_ignore_ascii_case("localhost"),
    };
    (!private).then(|| (host.to_owned(), port))
}

/// The address a report goes to, and all that it says.
fn request(hub: &str, host: &str, port: u16, players: u32) -> (String, String) {
    let url = format!("{}/v1/players", hub.trim_end_matches('/'));
    let body = serde_json::json!({ "host": host, "port": port, "players": players });
    (url, body.to_string())
}

/// Makes the report. What became of it is nobody's concern.
fn post(url: &str, body: &str) {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .user_agent(concat!("dereth/", env!("CARGO_PKG_VERSION")))
        .timeout_global(Some(TIMEOUT))
        .http_status_as_error(false)
        .max_redirects(0)
        .build()
        .into();
    let _ = agent
        .post(url)
        .header("Content-Type", "application/json")
        .send(body);
}

/// What to do with the count a world says at login, for a client connecting to `host` and `port`:
/// report it, off this thread. `None` for a world that is not reported.
#[must_use]
pub fn reporter(host: &str, port: u32) -> Option<Box<dyn FnMut(u32)>> {
    let (host, port) = target(host, port)?;
    let hub = std::env::var(HUB_ENV)
        .ok()
        .filter(|h| !h.trim().is_empty())
        .unwrap_or_else(|| HUB.to_owned());
    Some(Box::new(move |players| {
        let (url, body) = request(&hub, &host, port, players);
        // A thread that cannot start is one report the fewer.
        let _ = std::thread::Builder::new()
            .name("player-count".into())
            .spawn(move || post(&url, &body));
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Behaviour: net.player-count.a-login-tells-the-hub-the-worlds-count-and-nothing-else
    #[test]
    fn a_report_says_the_worlds_address_and_the_count_and_nothing_else() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let hub = format!("http://{}/", listener.local_addr().expect("an address"));
        let (url, body) = request(&hub, "coldeve.example.org", 9000, 12);
        assert!(
            url.ends_with("/v1/players") && !url.contains("//v1"),
            "{url}"
        );
        let read: serde_json::Value = serde_json::from_str(&body).expect("JSON");
        assert_eq!(
            read,
            serde_json::json!({"host": "coldeve.example.org", "port": 9000, "players": 12})
        );

        let sent = std::thread::spawn(move || post(&url, &body));
        let (mut stream, _) = listener.accept().expect("the report arrives");
        let mut request = Vec::new();
        let mut buffer = [0u8; 1024];
        while !request.ends_with(b"\"port\":9000}") {
            let n = stream.read(&mut buffer).expect("read");
            assert!(n > 0, "{}", String::from_utf8_lossy(&request));
            request.extend_from_slice(&buffer[..n]);
        }
        let request = String::from_utf8(request).expect("text");
        assert!(
            request.starts_with("POST /v1/players HTTP/1.1\r\n"),
            "{request}"
        );
        write!(stream, "HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n").expect("answer");
        sent.join().expect("the report ends");
    }

    /// Behaviour: net.player-count.a-report-that-fails-is-forgotten
    #[test]
    fn a_hub_that_is_gone_or_refuses_is_nobodys_concern() {
        use std::io::{Read, Write};
        // Nothing listens: the port of a listener just closed.
        let gone = std::net::TcpListener::bind("127.0.0.1:0")
            .and_then(|l| l.local_addr())
            .expect("an address");
        let (url, body) = request(&format!("http://{gone}"), "a.example.org", 9000, 1);
        post(&url, &body);
        post("not a url", &body);

        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let hub = format!("http://{}", listener.local_addr().expect("an address"));
        let (url, body) = request(&hub, "a.example.org", 9000, 1);
        let sent = std::thread::spawn(move || post(&url, &body));
        let (mut stream, _) = listener.accept().expect("the report arrives");
        let mut buffer = [0u8; 1024];
        let _ = stream.read(&mut buffer).expect("read");
        write!(
            stream,
            "HTTP/1.1 429 Too Many Requests\r\nContent-Length: 0\r\n\r\n"
        )
        .expect("answer");
        sent.join().expect("a refusal is no panic");
    }

    /// Behaviour: net.player-count.a-world-on-a-private-network-is-never-reported
    #[test]
    fn only_a_world_on_the_internet_is_reported_under_its_own_address() {
        for host in [
            "127.0.0.1",
            "localhost",
            "LOCALHOST",
            "10.1.2.3",
            "192.168.0.9",
            "172.16.0.1",
            "169.254.0.1",
            "0.0.0.0",
            "[::1]",
            "::1",
            "fd00::1",
            "fe80::1",
            "",
        ] {
            assert_eq!(target(host, 9000), None, "{host}");
        }
        for (host, wanted) in [
            ("coldeve.example.org", ("coldeve.example.org", 9000)),
            ("203.0.113.9", ("203.0.113.9", 9000)),
            ("coldeve.example.org:9100", ("coldeve.example.org", 9100)),
            (
                "a.example.org:9100,b.example.org:9200",
                ("a.example.org", 9100),
            ),
        ] {
            assert_eq!(
                target(host, 9000),
                Some((wanted.0.to_owned(), wanted.1)),
                "{host}"
            );
        }
        assert_eq!(target("a.example.org", 70_000), None, "no such port");
    }
}
