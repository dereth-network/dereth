//! The status endpoint (`--status <ip>:<port>`), as Empyrean's: plain HTTP on its own TCP port,
//! answering `GET /health` with `ok` and `GET /status` (also `GET /v1/world`) with the world's
//! state as JSON: its name, the server's version and game, how long it has run, whether it is
//! open or shutting down, the players in the world, the logins, the connections and the WebSocket
//! endpoint's address (`websocket_url`, null with the endpoint off). It has no authentication: it is
//! meant for loopback or a private network.
//!
//! A web page may read it from a page origin the WebSocket endpoint admits
//! (`server.websocket.allowed_origins`), so that it can find the endpoint: the answer then carries
//! `Access-Control-Allow-Origin` for that origin. No other origin is named.
//!
//! The loop puts the latest [`Snapshot`] in with [`Endpoint::set`]; a thread of its own answers
//! the requests from it, one at a time.

use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex, PoisonError};
use std::time::Duration;

use dereth_server_websocket::settings::cors_header;

/// What `/status` says.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub world_name: String,
    /// Whole seconds since the server started.
    pub uptime_seconds: u64,
    /// The world is counting down to a shutdown.
    pub shutting_down: bool,
    /// Characters in the world.
    pub players: usize,
    /// Logins held, and the most the world holds.
    pub logins: usize,
    pub max_logins: u32,
    /// Connections open, handshakes included.
    pub connections: usize,
    /// The URL the WebSocket endpoint is reached at, while it serves one.
    pub websocket_url: Option<String>,
    /// The world's saves are failing.
    pub saves_failing: bool,
}

impl Snapshot {
    /// `/status`'s body.
    #[must_use]
    pub fn json(&self) -> String {
        serde_json::json!({
            "world_name": self.world_name,
            "saves_failing": self.saves_failing,
            "server": "yalain",
            "version": env!("CARGO_PKG_VERSION"),
            "game": "auberean",
            "uptime_seconds": self.uptime_seconds,
            "state": if self.shutting_down { "maintenance" } else { "online" },
            "players_online": self.players,
            "logins": self.logins,
            "max_logins": self.max_logins,
            "connections": self.connections,
            "websocket_url": self.websocket_url,
        })
        .to_string()
    }
}

/// The endpoint: its listener's thread, and the snapshot it answers from.
#[derive(Debug, Clone)]
pub struct Endpoint {
    latest: Arc<Mutex<Snapshot>>,
    addr: SocketAddr,
    stopping: Arc<AtomicBool>,
    stopped: Arc<Mutex<mpsc::Receiver<()>>>,
}

impl Endpoint {
    /// Listen at `at` and answer from a thread of its own, from `first` until the loop gives
    /// another; a page from one of `allowed_origins` may read the answers.
    ///
    /// # Errors
    /// The address cannot be bound.
    pub fn start(
        at: SocketAddr,
        first: Snapshot,
        allowed_origins: Vec<String>,
    ) -> Result<Self, String> {
        let listener = TcpListener::bind(at).map_err(|e| format!("--status {at}: {e}"))?;
        let addr = listener
            .local_addr()
            .map_err(|e| format!("--status {at}: {e}"))?;
        let latest = Arc::new(Mutex::new(first));
        let shared = Arc::clone(&latest);
        let stopping = Arc::new(AtomicBool::new(false));
        let stop = Arc::clone(&stopping);
        let (closed, stopped) = mpsc::channel();
        std::thread::Builder::new()
            .name("status".into())
            .spawn(move || {
                // Dropped in reverse order: the listener closes, and the address is free, before
                // `stop` hears of it.
                let _closed = closed;
                let listener = listener;
                for stream in listener.incoming().flatten() {
                    if stop.load(Ordering::SeqCst) {
                        return;
                    }
                    let snapshot = shared
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .clone();
                    if let Err(e) = answer(stream, &snapshot, &allowed_origins) {
                        tracing::debug!("status endpoint: {e}");
                    }
                }
            })
            .map_err(|e| format!("--status: {e}"))?;
        Ok(Self {
            latest,
            addr,
            stopping,
            stopped: Arc::new(Mutex::new(stopped)),
        })
    }

    /// Stop listening, so the next server can have the address. A request being answered is given a
    /// moment, and a guard against a hang.
    pub fn stop(&self) {
        self.stopping.store(true, Ordering::SeqCst);
        let mut at = self.addr;
        if at.ip().is_unspecified() {
            at.set_ip(if at.is_ipv4() {
                Ipv4Addr::LOCALHOST.into()
            } else {
                Ipv6Addr::LOCALHOST.into()
            });
        }
        let _ = TcpStream::connect_timeout(&at, Duration::from_secs(1));
        let stopped = self.stopped.lock().unwrap_or_else(PoisonError::into_inner);
        let _ = stopped.recv_timeout(Duration::from_secs(5));
    }

    /// Where it listens.
    #[must_use]
    pub const fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Answer from this snapshot from now on.
    pub fn set(&self, s: Snapshot) {
        *self.latest.lock().unwrap_or_else(PoisonError::into_inner) = s;
    }
}

/// Answer one request.
fn answer(stream: TcpStream, s: &Snapshot, allowed_origins: &[String]) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request = String::new();
    reader.read_line(&mut request)?;
    // The headers, up to the blank line; only the page's origin is read.
    let mut line = String::new();
    let mut origin = None;
    while reader.read_line(&mut line)? > 2 {
        if let Some((name, value)) = line.split_once(':') {
            if name.trim().eq_ignore_ascii_case("origin") {
                origin = Some(value.trim().to_owned());
            }
        }
        line.clear();
    }
    let page = cors_header(origin.as_deref(), allowed_origins).unwrap_or_default();
    let mut words = request.split_whitespace();
    let (method, path) = (words.next().unwrap_or(""), words.next().unwrap_or(""));
    let path = path.split('?').next().unwrap_or("");
    let (status, kind, body) = match (method, path) {
        ("GET", "/health") => ("200 OK", "text/plain", "ok".to_owned()),
        ("GET", "/status" | "/v1/world") => ("200 OK", "application/json", s.json()),
        ("GET", _) => ("404 Not Found", "text/plain", "not found".to_owned()),
        _ => (
            "405 Method Not Allowed",
            "text/plain",
            "GET only".to_owned(),
        ),
    };
    let mut out = stream;
    write!(
        out,
        "HTTP/1.1 {status}\r\n{page}Content-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )?;
    out.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn get(at: SocketAddr, path: &str) -> String {
        let mut s = TcpStream::connect(at).expect("connects");
        write!(s, "GET {path} HTTP/1.1\r\nHost: x\r\n\r\n").expect("sent");
        let mut text = String::new();
        s.read_to_string(&mut text).expect("an answer");
        text
    }

    /// The status endpoint answers `/health` with ok, `/status` and `/v1/world` with the world's
    /// name, the server, its version and game, its uptime and state, the players in the world, the
    /// logins and connections as the loop last gave them, and anything else with not found.
    /// Behaviour: server.operator.status-endpoint
    #[test]
    fn the_status_endpoint_answers_health_and_the_worlds_state() {
        let e = Endpoint::start(
            SocketAddr::from(([127, 0, 0, 1], 0)),
            Snapshot::default(),
            Vec::new(),
        )
        .expect("listens");
        e.set(Snapshot {
            websocket_url: None,
            saves_failing: true,
            world_name: "Dawnsong".into(),
            uptime_seconds: 61,
            shutting_down: true,
            players: 3,
            logins: 4,
            max_logins: 128,
            connections: 7,
        });
        let health = get(e.addr(), "/health");
        assert!(health.starts_with("HTTP/1.1 200 OK\r\n"), "{health}");
        assert!(health.ends_with("\r\n\r\nok"), "{health}");
        for path in ["/status", "/v1/world"] {
            let text = get(e.addr(), path);
            assert!(text.starts_with("HTTP/1.1 200 OK\r\n"), "{text}");
            let body = text.split("\r\n\r\n").nth(1).expect("a body");
            let v: serde_json::Value = serde_json::from_str(body).expect("JSON");
            assert_eq!(v["world_name"], "Dawnsong");
            assert_eq!(v["server"], "yalain");
            assert_eq!(v["version"], env!("CARGO_PKG_VERSION"));
            assert_eq!(v["game"], "auberean");
            assert_eq!(v["uptime_seconds"], 61);
            assert_eq!(v["state"], "maintenance");
            assert_eq!(v["players_online"], 3);
            assert_eq!(v["logins"], 4);
            assert_eq!(v["max_logins"], 128);
            assert_eq!(v["connections"], 7);
            assert_eq!(v["saves_failing"], true);
            assert_eq!(v["websocket_url"], serde_json::Value::Null);
        }
        assert!(get(e.addr(), "/nothing").starts_with("HTTP/1.1 404"));
    }

    /// `/status` reports the WebSocket endpoint's address, and a page from an origin the endpoint
    /// admits may read it, as on Empyrean's; no other page is named in the answer.
    /// Behaviour: server.operator.websocket-url-on-the-status-endpoint
    #[test]
    fn the_status_endpoint_says_where_the_websocket_is_and_only_an_admitted_page_may_read_it() {
        let e = Endpoint::start(
            SocketAddr::from(([127, 0, 0, 1], 0)),
            Snapshot {
                websocket_url: Some("wss://play.example.org/ws".into()),
                ..Snapshot::default()
            },
            vec!["https://play.example.org".into()],
        )
        .expect("listens");
        let asked = |origin: &str| {
            let mut s = TcpStream::connect(e.addr()).expect("connects");
            write!(
                s,
                "GET /status HTTP/1.1\r\nHost: x\r\nOrigin: {origin}\r\n\r\n"
            )
            .expect("sent");
            let mut text = String::new();
            s.read_to_string(&mut text).expect("an answer");
            text
        };
        let text = asked("https://play.example.org");
        assert!(
            text.contains("\r\nAccess-Control-Allow-Origin: https://play.example.org\r\n"),
            "{text}"
        );
        let body = text.split("\r\n\r\n").nth(1).expect("a body");
        let v: serde_json::Value = serde_json::from_str(body).expect("JSON");
        assert_eq!(v["websocket_url"], "wss://play.example.org/ws");
        let text = asked("https://elsewhere.example.org");
        assert!(!text.contains("Access-Control-Allow-Origin"), "{text}");
        assert!(!get(e.addr(), "/status").contains("Access-Control-Allow-Origin"));
    }
}
