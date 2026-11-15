//! The status endpoint (`--status <ip>:<port>`), as Empyrean's: plain HTTP on its own TCP port,
//! answering `GET /health` with `ok` and `GET /status` (also `GET /v1/world`) with the world's
//! state as JSON: its name, the server's version and game, how long it has run, whether it is
//! open or shutting down, the players in the world, the objects the world holds
//! (`world_objects`), the logins, the connections and the WebSocket
//! endpoint's address (`websocket_url`, null with the endpoint off), and the world's step times
//! (`world_step`, in milliseconds: the step's budget; since the start, the steps and overruns, the
//! first step's work, which fills every camp, and the worst since; the creatures that acted at the
//! last step; and over the last minute's steps, the last, mean, 99th percentile and worst step's
//! work, and each stage's mean and worst); the world database it serves (`world_database`: its key
//! and era, the release it is, the release it lies over and the world whose overlay the install is
//! read through, each null when there is none); what clients asked for that the server doesn't
//! do yet (`not_handled`: the requests met, and each kind's count); the operator's game rates
//! that aren't 1 (`rates`, by name; empty when the game plays at its own); and whether it patches
//! retail clients' data files (`dats.patching`, as Empyrean's document says it), when a retail
//! player must play from a copy of the client's files kept for the world. It has no
//! authentication: it is meant for loopback or a private network.
//!
//! A web page may read it from a page origin the WebSocket endpoint admits
//! (`server.websocket.allowed_origins`), so that it can find the endpoint: the answer then carries
//! `Access-Control-Allow-Origin` for that origin. No other origin is named.
//!
//! The loop puts the latest [`Snapshot`] in with [`Endpoint::set`]; a thread of its own answers
//! the requests from it, one at a time.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read as _, Write};
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex, PoisonError};
use std::time::Duration;

use dereth_server_websocket::settings::cors_header;

/// What `/status` says.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Snapshot {
    pub world_name: String,
    /// Whole seconds since the server started.
    pub uptime_seconds: u64,
    /// The world is counting down to a shutdown.
    pub shutting_down: bool,
    /// Characters in the world.
    pub players: usize,
    /// The objects the world holds: creatures, items and everything else it has made.
    pub objects: usize,
    /// Logins held, and the most the world holds.
    pub logins: usize,
    pub max_logins: u32,
    /// Connections open, handshakes included.
    pub connections: usize,
    /// The URL the WebSocket endpoint is reached at, while it serves one.
    pub websocket_url: Option<String>,
    /// The world's saves are failing.
    pub saves_failing: bool,
    /// The world's step times.
    pub world_step: yalain_world::step_timer::Figures,
    /// The world database served.
    pub world: crate::world::Served,
    /// The requests met that the server doesn't handle, and each kind's count.
    pub not_handled: (u64, Arc<BTreeMap<String, u64>>),
    /// The operator's game rates that aren't 1, by name.
    pub rates: Vec<(&'static str, f64)>,
    /// The world patches retail clients' data files as they log in, so a retail player plays it
    /// from a copy of the client's files kept for it.
    pub patches_retail: bool,
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
            "world_objects": self.objects,
            "logins": self.logins,
            "max_logins": self.max_logins,
            "connections": self.connections,
            "websocket_url": self.websocket_url,
            "world_step": step_json(&self.world_step),
            "world_database": served_json(&self.world),
            "not_handled": {
                "requests": self.not_handled.0,
                "kinds": *self.not_handled.1,
            },
            "rates": self
                .rates
                .iter()
                .map(|(name, rate)| ((*name).to_owned(), serde_json::json!(rate)))
                .collect::<serde_json::Map<_, _>>(),
            "dats": { "patching": self.patches_retail },
        })
        .to_string()
    }
}

/// The world database served, as `/status` names it.
fn served_json(w: &crate::world::Served) -> serde_json::Value {
    serde_json::json!({
        "key": w.key,
        "era": w.era,
        "release": w.release,
        "base": w.base.as_ref().map(|b| serde_json::json!({ "key": b.key, "release": b.release })),
        "overlay": w.overlay,
    })
}

/// Microseconds as milliseconds.
#[allow(clippy::cast_precision_loss)]
fn ms(us: u64) -> f64 {
    us as f64 / 1000.0
}

/// The world's step times as `/status` gives them.
fn step_json(f: &yalain_world::step_timer::Figures) -> serde_json::Value {
    let stages: serde_json::Map<String, serde_json::Value> = f
        .stages
        .iter()
        .map(|(s, t)| {
            (
                s.name().to_owned(),
                serde_json::json!({ "mean_ms": ms(t.mean_us), "worst_ms": ms(t.worst_us) }),
            )
        })
        .collect();
    serde_json::json!({
        "budget_ms": ms(f.budget_us),
        "steps": f.steps,
        "overruns": f.overruns,
        "first_ms": ms(f.first_us),
        "worst_ms": ms(f.worst_ever_us),
        "creatures": f.creatures,
        "recent": {
            "steps": f.recent_steps,
            "overruns": f.recent_overruns,
            "last_ms": ms(f.last_us),
            "mean_ms": ms(f.mean_us),
            "p99_ms": ms(f.p99_us),
            "worst_ms": ms(f.worst_us),
            "stages": stages,
        },
    })
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

/// How long a request may take to arrive, and to be taken, on the endpoint's one thread.
const REQUEST_WAIT: Duration = Duration::from_secs(2);
/// The most of a request read: its line and its headers.
const MOST_REQUEST_BYTES: u64 = 2048;

/// Answer one request.
fn answer(stream: TcpStream, s: &Snapshot, allowed_origins: &[String]) -> std::io::Result<()> {
    stream.set_read_timeout(Some(REQUEST_WAIT))?;
    stream.set_write_timeout(Some(REQUEST_WAIT))?;
    let mut reader = BufReader::new(stream.try_clone()?.take(MOST_REQUEST_BYTES));
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
        s.set_read_timeout(Some(Duration::from_secs(60)))
            .expect("set");
        write!(s, "GET {path} HTTP/1.1\r\nHost: x\r\n\r\n").expect("sent");
        let mut text = String::new();
        s.read_to_string(&mut text).expect("an answer");
        text
    }

    /// A connection that says nothing, or never ends its line, holds the endpoint no longer than its
    /// wait, and keeps no more than a request's worth: the next request is answered.
    /// Behaviour: server.operator.status-endpoint
    #[test]
    fn a_silent_connection_or_an_endless_line_does_not_hold_up_the_next_request() {
        let e = Endpoint::start(
            "127.0.0.1:0".parse().expect("an address"),
            Snapshot::default(),
            Vec::new(),
        )
        .expect("starts");
        let silent = TcpStream::connect(e.addr()).expect("connects");
        let mut endless = TcpStream::connect(e.addr()).expect("connects");
        endless
            .set_write_timeout(Some(Duration::from_secs(60)))
            .expect("set");
        // Past what it keeps, the endpoint is done with it, and sending to it fails.
        let cut_off = (0..20_000).any(|_| endless.write_all(&[b'a'; 8192]).is_err());
        assert!(cut_off, "160 MB taken of one line");
        assert!(
            get(e.addr(), "/health").ends_with("ok"),
            "the next is answered"
        );
        drop(silent);
        e.stop();
    }

    /// The status endpoint answers `/health` with ok, `/status` and `/v1/world` with the world's
    /// name, the server, its version and game, its uptime and state, the players in the world, the
    /// objects it holds, the logins and connections as the loop last gave them, and anything else
    /// with not found.
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
            objects: 1234,
            logins: 4,
            max_logins: 128,
            connections: 7,
            world_step: yalain_world::step_timer::Figures::default(),
            ..Snapshot::default()
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
            assert_eq!(v["world_objects"], 1234);
            assert_eq!(v["logins"], 4);
            assert_eq!(v["max_logins"], 128);
            assert_eq!(v["connections"], 7);
            assert_eq!(v["saves_failing"], true);
            assert_eq!(v["websocket_url"], serde_json::Value::Null);
        }
        assert!(get(e.addr(), "/nothing").starts_with("HTTP/1.1 404"));
    }

    /// `/status` gives the world's step times as the step timer draws them, in milliseconds: the
    /// budget, the steps and overruns since the start, the first step and the worst since, the creatures that acted at
    /// the last step, and over the last minute's steps the last, mean, 99th percentile and worst
    /// and each stage's mean and worst.
    /// Behaviour: server.operator.world-step-times-on-the-status-endpoint
    #[test]
    fn the_status_endpoint_gives_the_worlds_step_times_by_stage() {
        use std::sync::{Arc, Mutex};
        use yalain_world::step_timer::{Stage, StepTimer};
        let at = Arc::new(Mutex::new(0.0_f64));
        let read = Arc::clone(&at);
        let mut timer = StepTimer::default();
        timer.set_clock(Box::new(move || {
            *read.lock().unwrap_or_else(PoisonError::into_inner)
        }));
        let steps = [(2.0, 0.0), (0.004, 0.001), (0.3, 0.0)];
        for (i, (creatures, network)) in steps.into_iter().enumerate() {
            timer.begin();
            *at.lock().unwrap_or_else(PoisonError::into_inner) += creatures;
            timer.lap(Stage::Creatures);
            *at.lock().unwrap_or_else(PoisonError::into_inner) += network;
            timer.lap(Stage::Network);
            timer.set_creatures(12 + i);
            timer.step_ends(f64::from(u8::try_from(i).expect("small")));
        }
        let e = Endpoint::start(
            SocketAddr::from(([127, 0, 0, 1], 0)),
            Snapshot {
                world_step: timer.figures().clone(),
                ..Snapshot::default()
            },
            Vec::new(),
        )
        .expect("listens");
        let text = get(e.addr(), "/status");
        let body = text.split("\r\n\r\n").nth(1).expect("a body");
        let v: serde_json::Value = serde_json::from_str(body).expect("JSON");
        let w = &v["world_step"];
        assert_eq!(w["budget_ms"], 250.0);
        assert_eq!((&w["steps"], &w["overruns"]), (&3.into(), &1.into()));
        assert_eq!(w["first_ms"], 2000.0);
        assert_eq!(w["worst_ms"], 300.0);
        assert_eq!(w["creatures"], 14);
        let r = &w["recent"];
        assert_eq!((&r["steps"], &r["overruns"]), (&2.into(), &1.into()));
        assert_eq!(r["last_ms"], 300.0);
        assert_eq!(r["mean_ms"], 152.5);
        assert_eq!(r["p99_ms"], 300.0);
        assert_eq!(r["worst_ms"], 300.0);
        assert_eq!(r["stages"]["creatures"]["mean_ms"], 152.0);
        assert_eq!(r["stages"]["creatures"]["worst_ms"], 300.0);
        assert_eq!(r["stages"]["network"]["mean_ms"], 0.5);
        assert_eq!(r["stages"]["network"]["worst_ms"], 1.0);
        let names: Vec<&String> = r["stages"].as_object().expect("by stage").keys().collect();
        assert_eq!(names.len(), Stage::ALL.len());
        e.stop();
    }

    /// `/status` names the operator's game rates that aren't 1, each by name, and none (an empty
    /// object) for a world that plays at the game's own.
    /// Behaviour: server.operator.game-rates-on-the-status-endpoint
    #[test]
    fn the_status_endpoint_names_the_game_rates_that_are_not_one() {
        let rates = |snapshot: Snapshot| {
            let e = Endpoint::start(SocketAddr::from(([127, 0, 0, 1], 0)), snapshot, Vec::new())
                .expect("listens");
            let text = get(e.addr(), "/status");
            e.stop();
            let body = text.split("\r\n\r\n").nth(1).expect("a body").to_owned();
            let v: serde_json::Value = serde_json::from_str(&body).expect("JSON");
            v["rates"].clone()
        };
        let set = yalain_world::rates::Rates {
            experience: 2.0,
            loot: 1.0,
            respawn: 0.5,
            movement: 3.0,
        };
        assert_eq!(
            rates(Snapshot {
                rates: set.changed(),
                ..Snapshot::default()
            }),
            serde_json::json!({ "experience": 2.0, "respawn": 0.5, "movement": 3.0 })
        );
        assert_eq!(rates(Snapshot::default()), serde_json::json!({}));
    }

    /// `/status` names the world database served (its key and era, its release, the release it
    /// lies over and the overlay's world, null where there is none) and what clients asked for that
    /// the server doesn't do yet, each kind with its count.
    /// Behaviour: server.operator.status-names-the-world-database
    #[test]
    fn the_status_endpoint_names_the_world_database_and_what_is_not_handled() {
        let mut not_handled = yalain_world::Unhandled::default();
        for what in [
            "script call 0x82120046",
            "script call 0x82120046",
            "a pet's name",
        ] {
            not_handled.note(what);
        }
        let e = Endpoint::start(
            SocketAddr::from(([127, 0, 0, 1], 0)),
            Snapshot {
                world: crate::world::Served {
                    key: "mine".into(),
                    era: "eor".into(),
                    release: None,
                    base: Some(crate::world::ServedBase {
                        key: "yalain-world".into(),
                        release: Some("v0.34.0".into()),
                    }),
                    overlay: Some("mine".into()),
                },
                not_handled: (
                    not_handled.requests(),
                    Arc::new(not_handled.counts().clone()),
                ),
                ..Snapshot::default()
            },
            Vec::new(),
        )
        .expect("listens");
        let text = get(e.addr(), "/status");
        let body = text.split("\r\n\r\n").nth(1).expect("a body");
        let v: serde_json::Value = serde_json::from_str(body).expect("JSON");
        let w = &v["world_database"];
        assert_eq!((&w["key"], &w["era"]), (&"mine".into(), &"eor".into()));
        assert_eq!(w["release"], serde_json::Value::Null);
        assert_eq!(w["base"]["key"], "yalain-world");
        assert_eq!(w["base"]["release"], "v0.34.0");
        assert_eq!(w["overlay"], "mine");
        let n = &v["not_handled"];
        assert_eq!(n["requests"], 3);
        assert_eq!(n["kinds"]["script call 0x82120046"], 2);
        assert_eq!(n["kinds"]["a pet's name"], 1);
        assert_eq!(n["kinds"].as_object().map(serde_json::Map::len), Some(2));
        e.stop();
        let quiet = Snapshot::default().json();
        let v: serde_json::Value = serde_json::from_str(&quiet).expect("JSON");
        assert_eq!(v["world_database"]["base"], serde_json::Value::Null);
        assert_eq!(v["not_handled"]["requests"], 0);
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
