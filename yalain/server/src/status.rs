//! The status endpoint (`--status <ip>:<port>`), as Empyrean's: plain HTTP on its own TCP port,
//! answering `GET /health` with `ok` and `GET /status` (also `GET /v1/world`) with the world's
//! state as JSON: its name, the server's version and game, how long it has run, whether it is
//! open or shutting down, the players in the world, the logins and the connections. It has no
//! authentication: it is meant for loopback or a private network.
//!
//! The loop puts the latest [`Snapshot`] in with [`Endpoint::set`]; a thread of its own answers
//! the requests from it, one at a time.

use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex, PoisonError};

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
}

impl Snapshot {
    /// `/status`'s body.
    #[must_use]
    pub fn json(&self) -> String {
        serde_json::json!({
            "world_name": self.world_name,
            "server": "yalain",
            "version": env!("CARGO_PKG_VERSION"),
            "game": "auberean",
            "uptime_seconds": self.uptime_seconds,
            "state": if self.shutting_down { "maintenance" } else { "online" },
            "players_online": self.players,
            "logins": self.logins,
            "max_logins": self.max_logins,
            "connections": self.connections,
        })
        .to_string()
    }
}

/// The endpoint: its listener's thread, and the snapshot it answers from.
#[derive(Debug, Clone)]
pub struct Endpoint {
    latest: Arc<Mutex<Snapshot>>,
    addr: SocketAddr,
}

impl Endpoint {
    /// Listen at `at` and answer from a thread of its own, from `first` until the loop gives
    /// another.
    ///
    /// # Errors
    /// The address cannot be bound.
    pub fn start(at: SocketAddr, first: Snapshot) -> Result<Self, String> {
        let listener = TcpListener::bind(at).map_err(|e| format!("--status {at}: {e}"))?;
        let addr = listener
            .local_addr()
            .map_err(|e| format!("--status {at}: {e}"))?;
        let latest = Arc::new(Mutex::new(first));
        let shared = Arc::clone(&latest);
        std::thread::Builder::new()
            .name("status".into())
            .spawn(move || {
                for stream in listener.incoming().flatten() {
                    let snapshot = shared
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .clone();
                    if let Err(e) = answer(stream, &snapshot) {
                        tracing::debug!("status endpoint: {e}");
                    }
                }
            })
            .map_err(|e| format!("--status: {e}"))?;
        Ok(Self { latest, addr })
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
fn answer(stream: TcpStream, s: &Snapshot) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request = String::new();
    reader.read_line(&mut request)?;
    // The headers, up to the blank line; none is read.
    let mut line = String::new();
    while reader.read_line(&mut line)? > 2 {
        line.clear();
    }
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
        "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
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
        let e = Endpoint::start(SocketAddr::from(([127, 0, 0, 1], 0)), Snapshot::default())
            .expect("listens");
        e.set(Snapshot {
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
        }
        assert!(get(e.addr(), "/nothing").starts_with("HTTP/1.1 404"));
    }
}
