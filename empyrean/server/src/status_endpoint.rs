//! Not ACE: a small HTTP status endpoint for monitoring.
//!
//! ACE has no status endpoint: its operators read the console or run `serverstatus` in game. A
//! service manager or load balancer needs something it can poll, so `empyrean-server` can answer two
//! plain HTTP/1.0 requests on an address of its own, **off unless asked for** (`--status <addr>` or
//! `server.status_address` in `empyrean.toml`):
//!
//! - `GET /health`: `200 ok` when the world thread answered within [`WORLD_REPLY_TIMEOUT`],
//!   otherwise `503 unavailable` (the world loop is stuck, or it has stopped).
//! - `GET /status`: the same check, with a JSON body ([`StatusSnapshot::to_json`]): the world name,
//!   version, where the server's source is, uptime, whether the world is open or shutting down, connections, players online and
//!   loaded landblocks (the counts `serverstatus` reports), the world pack's content hash and the
//!   corrections digest (which together name the world data served), the era the world plays
//!   (`[era] profile`) and the systems it has (`features`: every system by name, the era's table
//!   with the world's `[era]` settings over it), the iterations of the dats it compares a client's against and whether it
//!   patches them, the client versions it admits, the WebSocket endpoint's URL (`null` when it is
//!   off), and the unported ACE members this process has reached so far with their hit counts
//!   (`empyrean_common::not_ported::global_snapshot`).
//! - `GET /v1/world`: the same document, at the address a launcher's world registry names.
//!
//! The snapshot is taken on the world thread (a command through the world queue), so the
//! endpoint never reads the world from another thread. Anything else is `404`, or `405` for a method
//! other than `GET`/`HEAD`. One request per connection; the listener is meant for loopback or a
//! private network, and there is no authentication.
//!
//! A web page may read `/status` from a page origin the WebSocket endpoint admits
//! (`server.websocket.allowed_origins`): the answer then carries `Access-Control-Allow-Origin` for
//! that origin, so the page can find the WebSocket URL. No other origin is named.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use empyrean_world::managers::world_manager::WorldStatusState;
use empyrean_world::managers::{landblock_manager, player_manager};
use empyrean_world::World;

/// How long a request waits for the world thread's snapshot before answering 503.
pub const WORLD_REPLY_TIMEOUT: Duration = Duration::from_secs(2);

/// The iteration of each dat the server opened (`None`: not opened).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DatIterations {
    pub portal: Option<i32>,
    pub cell: Option<i32>,
    pub local: Option<i32>,
    pub highres: Option<i32>,
}

impl DatIterations {
    /// The iterations of `dats`, as their headers give them.
    #[must_use]
    pub fn of(dats: &empyrean_dat::DatManager) -> Self {
        Self {
            portal: Some(dats.portal_dat().iteration()),
            cell: Some(dats.cell_dat().iteration()),
            local: Some(dats.language_dat().iteration()),
            highres: dats.high_res_dat().map(|d| d.iteration()),
        }
    }
}

/// What `GET /status` reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusSnapshot {
    /// `Server.WorldName`.
    pub world_name: String,
    /// The server's package version.
    pub version: String,
    /// Where the server's source is (`empyrean_common::brand::source_url`: `server.source_url`, else the
    /// build's repository).
    pub source_url: String,
    /// Seconds since the process started serving.
    pub uptime_seconds: u64,
    /// `WorldManager.WorldStatus == Open`.
    pub world_open: bool,
    /// `ServerManager.ShutdownInitiated`.
    pub shutting_down: bool,
    /// `NetworkManager.GetSessionCount()`.
    pub connections: usize,
    /// `NetworkManager.GetAuthenticatedSessionCount()`.
    pub authenticated_connections: usize,
    /// `PlayerManager.GetOnlineCount()`.
    pub players_online: i32,
    /// `LandblockManager.GetLoadedLandblocks().Count`.
    pub landblocks_loaded: usize,
    /// The world pack's content hash (`None` without a pack).
    pub content_hash: Option<String>,
    /// This build's corrections digest (`empyrean_content::corrections::digest`).
    pub corrections_digest: String,
    /// The era the world plays (`[era] profile`), by its configuration name.
    pub era: String,
    /// The systems the world has: the era's table with the world's `[era]` settings over it.
    pub features: empyrean_common::era::EraFeatures,
    /// The iterations of the server's dats, which it compares a client's against.
    pub dats: DatIterations,
    /// Whether the server patches a client's dats from its own (`ddd.enable_dat_patching`).
    pub dat_patching: bool,
    /// The client (logon) versions the server admits.
    pub client_versions: Vec<String>,
    /// The WebSocket endpoint's URL (`server.websocket`), `None` when it is off.
    pub websocket_url: Option<String>,
    /// Each `not_ported!` site reached in this process, with its hits, by name.
    pub not_ported: Vec<(String, u64)>,
}

impl StatusSnapshot {
    /// Reads the world (on the world thread). `uptime_seconds` comes from the caller's clock.
    #[must_use]
    pub fn take(w: &World, world_name: &str, uptime_seconds: u64) -> Self {
        Self {
            world_name: world_name.to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            source_url: empyrean_common::brand::source_url(),
            uptime_seconds,
            world_open: w.world_manager.world_status == WorldStatusState::Open,
            shutting_down: w.server_manager.shutdown_initiated,
            connections: w.net.get_session_count(),
            authenticated_connections: w.net.get_authenticated_session_count(),
            players_online: player_manager::get_online_count(w),
            landblocks_loaded: landblock_manager::get_loaded_landblocks(w).len(),
            content_hash: w.content.content_hash(),
            corrections_digest: empyrean_content::corrections::digest().to_owned(),
            era: w.era.id.name().to_owned(),
            features: w.era.features,
            dats: DatIterations::of(&w.dats),
            dat_patching: empyrean_common::config_manager::ConfigManager::try_config()
                .is_some_and(|c| c.ddd.enable_dat_patching),
            client_versions: vec![
                empyrean_net::handlers::authentication_handler::CLIENT_VERSION.to_owned(),
            ],
            websocket_url: None,
            not_ported: empyrean_common::not_ported::global_snapshot()
                .into_iter()
                .map(|(k, v)| (k.to_owned(), v))
                .collect(),
        }
    }

    /// One JSON object, keys in declaration order, ending in a newline.
    #[must_use]
    pub fn to_json(&self) -> String {
        let not_ported: Vec<String> = self
            .not_ported
            .iter()
            .map(|(name, hits)| format!("{}:{hits}", json_string(name)))
            .collect();
        let iteration = |i: Option<i32>| i.map_or_else(|| "null".to_owned(), |i| i.to_string());
        let dats = format!(
            "{{\"portal\":{},\"cell\":{},\"local\":{},\"highres\":{},\"patching\":{}}}",
            iteration(self.dats.portal),
            iteration(self.dats.cell),
            iteration(self.dats.local),
            iteration(self.dats.highres),
            self.dat_patching
        );
        let features: Vec<String> = self
            .features
            .iter()
            .map(|(name, on)| format!("{}:{on}", json_string(name)))
            .collect();
        let client_versions: Vec<String> = self
            .client_versions
            .iter()
            .map(|v| json_string(v))
            .collect();
        format!(
            "{{\"world_name\":{},\"version\":{},\"source_url\":{},\"uptime_seconds\":{},\"world_open\":{},\"shutting_down\":{},\"connections\":{},\"authenticated_connections\":{},\"players_online\":{},\"landblocks_loaded\":{},\"content_hash\":{},\"corrections_digest\":{},\"era\":{},\"features\":{{{}}},\"dats\":{dats},\"client_versions\":[{}],\"websocket_url\":{},\"not_ported\":{{{}}}}}\n",
            json_string(&self.world_name),
            json_string(&self.version),
            json_string(&self.source_url),
            self.uptime_seconds,
            self.world_open,
            self.shutting_down,
            self.connections,
            self.authenticated_connections,
            self.players_online,
            self.landblocks_loaded,
            self.content_hash.as_deref().map_or_else(|| "null".to_owned(), json_string),
            json_string(&self.corrections_digest),
            json_string(&self.era),
            features.join(","),
            client_versions.join(","),
            self.websocket_url.as_deref().map_or_else(|| "null".to_owned(), json_string),
            not_ported.join(","),
        )
    }
}

/// A JSON string literal.
fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if u32::from(c) < 0x20 => out.push_str(&format!("\\u{:04x}", u32::from(c))),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The endpoint's address: `--status` (`cli`), else `server.status_address` (`config`); `None` when
/// neither is set or both are blank.
///
/// # Errors
/// A value that is not `host:port` with an IP host.
pub fn status_address(cli: Option<&str>, config: &str) -> Result<Option<SocketAddr>, String> {
    let Some(value) = cli
        .or(Some(config))
        .map(str::trim)
        .filter(|v| !v.is_empty())
    else {
        return Ok(None);
    };
    value
        .parse()
        .map(Some)
        .map_err(|_| format!("status address is not <ip>:<port>: {value}"))
}

/// The response to one request, given its request line and a way to get the world's snapshot
/// (`None` when the world did not answer in time).
#[must_use]
pub fn respond(request_line: &str, snapshot: impl FnOnce() -> Option<StatusSnapshot>) -> Vec<u8> {
    let mut parts = request_line.split_whitespace();
    let (method, target) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    let path = target.split('?').next().unwrap_or("");
    let head_only = method == "HEAD";
    if method != "GET" && !head_only {
        return response(405, "text/plain", "method not allowed\n", head_only);
    }
    match path {
        "/health" => match snapshot() {
            Some(_) => response(200, "text/plain", "ok\n", head_only),
            None => response(503, "text/plain", "unavailable\n", head_only),
        },
        "/status" | "/v1/world" => match snapshot() {
            Some(s) => response(200, "application/json", &s.to_json(), head_only),
            None => response(
                503,
                "application/json",
                "{\"error\":\"the world thread did not answer\"}\n",
                head_only,
            ),
        },
        _ => response(
            404,
            "text/plain",
            "not found; try /status or /health\n",
            head_only,
        ),
    }
}

fn response(code: u16, content_type: &str, body: &str, head_only: bool) -> Vec<u8> {
    let reason = match code {
        200 => "OK",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Service Unavailable",
    };
    let mut out = format!(
        "HTTP/1.0 {code} {reason}\r\nContent-Type: {content_type}; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    if !head_only {
        out.extend_from_slice(body.as_bytes());
    }
    out
}

/// The `Access-Control-Allow-Origin` line for a request from `origin`, when it is one of
/// `allowed`; `None` otherwise.
#[must_use]
pub fn cors_header(origin: Option<&str>, allowed: &[String]) -> Option<String> {
    let origin = origin?.trim();
    allowed
        .iter()
        .any(|a| {
            a.trim_end_matches('/')
                .eq_ignore_ascii_case(origin.trim_end_matches('/'))
        })
        .then(|| format!("Access-Control-Allow-Origin: {origin}\r\nVary: Origin\r\n"))
}

/// `response` with `header` (one or more `\r\n`-ended lines) added after its status line.
fn with_header(mut response: Vec<u8>, header: &str) -> Vec<u8> {
    if let Some(at) = response.windows(2).position(|w| w == b"\r\n") {
        response.splice(at + 2..at + 2, header.bytes());
    }
    response
}

/// A running status endpoint.
#[derive(Debug)]
pub struct StatusEndpoint {
    local: SocketAddr,
    stopped: Arc<AtomicBool>,
}

impl StatusEndpoint {
    /// The address it listens on.
    #[must_use]
    pub fn local_addr(&self) -> SocketAddr {
        self.local
    }

    /// Stops listening and frees the address (for a server that starts another in its place).
    pub fn shutdown(&self) {
        if !self.stopped.swap(true, Ordering::SeqCst) {
            // Wakes the accept, which then sees the flag and drops the listener.
            let mut wake = self.local;
            if wake.ip().is_unspecified() {
                wake.set_ip(match wake {
                    SocketAddr::V4(_) => std::net::Ipv4Addr::LOCALHOST.into(),
                    SocketAddr::V6(_) => std::net::Ipv6Addr::LOCALHOST.into(),
                });
            }
            let _ = TcpStream::connect_timeout(&wake, Duration::from_secs(1));
        }
    }
}

/// Binds the endpoint and serves it on a thread of its own until the process ends or the
/// endpoint is [shut down](StatusEndpoint::shutdown). `snapshot` is called once per `/status` or
/// `/health` request and must answer within its own timeout. A request from a page origin in
/// `allowed_origins` is answered with that origin allowed.
///
/// # Errors
/// The address cannot be bound, or the thread cannot start.
pub fn start(
    addr: SocketAddr,
    allowed_origins: Vec<String>,
    snapshot: impl Fn() -> Option<StatusSnapshot> + Send + 'static,
) -> Result<StatusEndpoint, String> {
    let listener = TcpListener::bind(addr).map_err(|e| format!("{addr}: {e}"))?;
    let local = listener.local_addr().map_err(|e| format!("{addr}: {e}"))?;
    let stopped = Arc::new(AtomicBool::new(false));
    let stop = Arc::clone(&stopped);
    std::thread::Builder::new()
        .name("Status Endpoint".to_owned())
        .spawn(move || {
            for stream in listener.incoming() {
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                match stream {
                    Ok(stream) => serve_one(stream, &allowed_origins, &snapshot),
                    Err(e) => log::warn!("Status endpoint: accept failed: {e}"),
                }
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(StatusEndpoint { local, stopped })
}

fn serve_one(
    mut stream: TcpStream,
    allowed_origins: &[String],
    snapshot: &dyn Fn() -> Option<StatusSnapshot>,
) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
    // The request line and the Origin header are all that matter; read until the headers end (or
    // 2 KB, or the timeout).
    let mut buf = Vec::with_capacity(512);
    let mut chunk = [0u8; 512];
    while !buf.windows(4).any(|w| w == b"\r\n\r\n") && buf.len() < 2048 {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
        }
    }
    let text = String::from_utf8_lossy(&buf);
    let request_line = text.lines().next().unwrap_or("");
    let origin = text.lines().skip(1).find_map(|l| {
        let (name, value) = l.split_once(':')?;
        name.trim()
            .eq_ignore_ascii_case("origin")
            .then(|| value.trim())
    });
    let mut answer = respond(request_line, snapshot);
    if let Some(header) = cors_header(origin, allowed_origins) {
        answer = with_header(answer, &header);
    }
    let _ = stream.write_all(&answer);
    let _ = stream.flush();
}
