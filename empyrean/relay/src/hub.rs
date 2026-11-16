//! Tells the Dereth Network's hub how many players the world said it has.
//!
//! The relay asks the world ([`crate::population`]) and passes each count on, for the launcher's
//! list of worlds: the world's own address and port, which the relay connects to, and the count,
//! and nothing else: no account, no character, no id of this relay or its player. It is the same
//! report a Dereth client makes of the count a world gives it at login, sent where the clients send
//! theirs. The hub keeps the latest count and its time.
//!
//! Each report goes on a thread of its own, and one that fails is forgotten, so it can neither
//! delay nor break a session. A world on this computer or a private network is nobody else's to
//! count, so it is never reported, whatever name it was given by.

use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

/// The hub reported to unless [`HUB_ENV`] names another.
pub const HUB: &str = "https://hub.dereth.network";

/// Names another hub to report to: a launcher reading another hub's list sets it, as it does for
/// the clients it starts.
pub const HUB_ENV: &str = "DERETH_PLAYER_COUNT_HUB";

/// How long a report may take.
const TIMEOUT: Duration = Duration::from_secs(5);

/// Whether `ip` is on this computer or a private network.
fn is_private(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            ip.is_loopback() || ip.is_private() || ip.is_link_local() || ip.is_unspecified()
        }
        IpAddr::V6(ip) => {
            let first = ip.segments()[0];
            ip.is_loopback()
                || ip.is_unspecified()
                || first & 0xfe00 == 0xfc00
                || first & 0xffc0 == 0xfe80
        }
    }
}

/// The world to report, as `upstream` (`host:port`, as the relay was told it) names it, unless
/// it, or `reached` (the address it stands for), is on this computer or a private network.
fn target(upstream: &str, reached: SocketAddr) -> Option<(String, u16)> {
    let (host, port) = upstream.trim().rsplit_once(':')?;
    let port: u16 = port.parse().ok().filter(|p| *p != 0)?;
    let host = host.trim();
    let private = match host.trim_matches(['[', ']']).parse::<IpAddr>() {
        Ok(ip) => is_private(ip),
        Err(_) => host.is_empty() || host.eq_ignore_ascii_case("localhost"),
    };
    (!private && !is_private(reached.ip())).then(|| (host.to_owned(), port))
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
        .user_agent(concat!("empyrean-relay/", env!("CARGO_PKG_VERSION")))
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

/// What to do with each count the world at `upstream` (reached at `reached`) gives: report it to
/// `hub`, off this thread. `None` for a world that is not reported.
fn reporter_to(hub: &str, upstream: &str, reached: SocketAddr) -> Option<Box<dyn FnMut(u32)>> {
    let (host, port) = target(upstream, reached)?;
    let hub = hub.to_owned();
    Some(Box::new(move |players| {
        let (url, body) = request(&hub, &host, port, players);
        // A thread that cannot start is one report the fewer.
        let _ = std::thread::Builder::new()
            .name("player-count".into())
            .spawn(move || post(&url, &body));
    }))
}

/// What to do with each count the world at `upstream` (`host:port`, reached at `reached`) gives:
/// report it to the hub [`HUB_ENV`] names, else [`HUB`], off this thread. `None` for a world that
/// is not reported.
#[must_use]
pub fn reporter(upstream: &str, reached: SocketAddr) -> Option<Box<dyn FnMut(u32)>> {
    let hub = std::env::var(HUB_ENV)
        .ok()
        .filter(|h| !h.trim().is_empty())
        .unwrap_or_else(|| HUB.to_owned());
    reporter_to(&hub, upstream, reached)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the relay's report of the world's count to the hub).

    use super::*;
    use std::io::{Read, Write};

    fn public() -> SocketAddr {
        "203.0.113.9:9110".parse().unwrap()
    }

    /// The request a fake hub on `listener` receives, whole, answered with `answer`.
    fn received(listener: &std::net::TcpListener, answer: &str) -> String {
        let (mut stream, _) = listener.accept().expect("the report arrives");
        let mut request = Vec::new();
        let mut buffer = [0u8; 1024];
        while !request.ends_with(b"}") {
            let n = stream.read(&mut buffer).expect("read");
            assert!(n > 0, "{}", String::from_utf8_lossy(&request));
            request.extend_from_slice(&buffer[..n]);
        }
        write!(stream, "HTTP/1.1 {answer}\r\nContent-Length: 0\r\n\r\n").expect("answer");
        String::from_utf8(request).expect("text")
    }

    #[test]
    fn a_count_is_reported_under_the_worlds_own_address_and_port_and_nothing_else() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let hub = format!("http://{}/", listener.local_addr().expect("an address"));
        let mut report =
            reporter_to(&hub, "ac1999.example.org:9110", public()).expect("a public world");
        report(12);
        let request = received(&listener, "200 OK");
        assert!(
            request.starts_with("POST /v1/players HTTP/1.1\r\n"),
            "{request}"
        );
        let body = request.split("\r\n\r\n").nth(1).expect("a body");
        let read: serde_json::Value = serde_json::from_str(body).expect("JSON");
        assert_eq!(
            read,
            serde_json::json!({"host": "ac1999.example.org", "port": 9110, "players": 12})
        );
    }

    #[test]
    fn a_hub_that_is_gone_or_refuses_is_nobodys_concern() {
        // Nothing listens: the port of a listener just closed.
        let gone = std::net::TcpListener::bind("127.0.0.1:0")
            .and_then(|l| l.local_addr())
            .expect("an address");
        let (url, body) = request(&format!("http://{gone}"), "a.example.org", 9110, 1);
        post(&url, &body);
        post("not a url", &body);

        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let hub = format!("http://{}", listener.local_addr().expect("an address"));
        let (url, body) = request(&hub, "a.example.org", 9110, 1);
        let sent = std::thread::spawn(move || post(&url, &body));
        received(&listener, "429 Too Many Requests");
        sent.join().expect("a refusal is no panic");
    }

    #[test]
    fn a_world_on_this_computer_or_a_private_network_is_never_reported() {
        for upstream in [
            "127.0.0.1:9110",
            "localhost:9110",
            "LOCALHOST:9110",
            "10.1.2.3:9110",
            "192.168.0.9:9110",
            "172.16.0.1:9110",
            "169.254.0.1:9110",
            "0.0.0.0:9110",
            "[::1]:9110",
            "[fd00::1]:9110",
            "[fe80::1]:9110",
            ":9110",
            "ac1999.example.org:0",
            "ac1999.example.org",
        ] {
            assert_eq!(target(upstream, public()), None, "{upstream}");
        }
        for reached in ["127.0.0.1:9110", "192.168.0.4:9110", "10.0.0.2:9110"] {
            assert_eq!(
                target("ac1999.example.org:9110", reached.parse().unwrap()),
                None,
                "a name for {reached}"
            );
        }
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let hub = format!("http://{}", listener.local_addr().expect("an address"));
        assert!(reporter_to(&hub, "127.0.0.1:9110", "127.0.0.1:9110".parse().unwrap()).is_none());

        for (upstream, wanted) in [
            ("ac1999.example.org:9110", ("ac1999.example.org", 9110)),
            (" 203.0.113.9:9110 ", ("203.0.113.9", 9110)),
        ] {
            assert_eq!(
                target(upstream, public()),
                Some((wanted.0.to_owned(), wanted.1)),
                "{upstream}"
            );
        }
    }
}
