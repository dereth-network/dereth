//! The world's listing on a Dereth Network hub, when the operator turns it on (`--hub`).
//!
//! The hub lists every world registered with it, by game; this world is an Asheron's Call 2 world
//! (`auberean`), so the launcher's Auberean realm lists it and Dereth's does not. Its operator
//! registers it with the hub once (the hub's command line, or the hub's site), which gives a key;
//! from then on the server posts a heartbeat to the hub over HTTPS with that key, saying it is up,
//! how many characters are in the world, its version and its game:
//!
//! - once at start, then once a minute, with the latest count;
//! - at once when its state changes (`maintenance` while it counts down to a shutdown);
//! - `offline` as it stops.
//!
//! The heartbeat is the network's own ([`dereth_network_protocol::heartbeat`]), naming this
//! server as its adapter (`yalain`).
//!
//! The posts go from a thread of their own, so the world's loop never waits on the hub. A post the
//! hub refuses, or that does not arrive, is logged and tried again at the next minute. The world
//! holds no link to the hub: tells and friends between worlds are not carried here.
//!
//! Nothing here logs the key.

use std::path::Path;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use dereth_network_protocol::heartbeat::{self, Adapter, Heartbeat};
use dereth_network_protocol::Game;

/// How often the world reports while nothing changes. The hub counts a world's status for three
/// minutes after its last heartbeat.
pub const EVERY: Duration = Duration::from_secs(60);

/// The game this world runs.
pub const GAME: Game = Game::Auberean;

/// The kind of software the world's heartbeats say sent them.
pub const ADAPTER: &str = "yalain";

/// How long one post may take before it is given up.
pub const POST_TIMEOUT: Duration = Duration::from_secs(20);

/// Where and as what the world reports: the hub's address, the world's slug there, and its key.
#[derive(Clone, PartialEq, Eq)]
pub struct HubSettings {
    /// `https://host[:port][/path]`, or `http://` to this machine; no trailing slash.
    pub url: String,
    pub slug: String,
    pub key: String,
}

impl std::fmt::Debug for HubSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HubSettings")
            .field("url", &self.url)
            .field("slug", &self.slug)
            .finish_non_exhaustive()
    }
}

impl HubSettings {
    /// The settings `--hub`, `--hub-slug` and `--hub-key-file` give, checked: the key is read
    /// from its file.
    ///
    /// # Errors
    /// What is wrong, for the operator: an address that is neither `https://` nor `http://` to
    /// this machine, a slug the hub would not have given, or a file that holds no key.
    pub fn new(url: &str, slug: &str, key_file: &Path) -> Result<Self, String> {
        let key = std::fs::read_to_string(key_file)
            .map_err(|e| format!("--hub-key-file {}: {e}", key_file.display()))?;
        let key = key.trim();
        if !is_key(key) {
            return Err(format!(
                "--hub-key-file {}: no hub key in it (drth_ and 43 more characters)",
                key_file.display()
            ));
        }
        if !is_slug(slug) {
            return Err(format!(
                "--hub-slug {slug}: 2-40 small letters, digits and single inner hyphens"
            ));
        }
        Ok(Self {
            url: hub_url(url)?,
            slug: slug.to_owned(),
            key: key.to_owned(),
        })
    }

    /// The address the world's heartbeats are posted to.
    #[must_use]
    pub fn heartbeat_url(&self) -> String {
        format!("{}{}", self.url, heartbeat::path(&self.slug))
    }
}

/// The hub's address, read: `https://` anywhere, `http://` only to this machine, since the key goes
/// with every post. `wss://` and `ws://`, the forms a world's link to the hub takes, name the same
/// hub.
fn hub_url(url: &str) -> Result<String, String> {
    let url = url.trim().trim_end_matches('/');
    let (tls, rest) = if let Some(r) = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("wss://"))
    {
        (true, r)
    } else if let Some(r) = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("ws://"))
    {
        (false, r)
    } else {
        return Err(format!("--hub {url}: the hub's address is https://host"));
    };
    let authority = rest.split('/').next().unwrap_or("");
    let host = match authority.strip_prefix('[') {
        Some(v6) => v6.split(']').next().unwrap_or(""),
        None => authority.rsplit_once(':').map_or(authority, |(h, _)| h),
    };
    if host.is_empty() || authority.contains('@') {
        return Err(format!("--hub {url}: no host"));
    }
    let loopback = host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback());
    if !tls && !loopback {
        return Err(format!(
            "--hub {url}: plain http:// is for a hub on this machine only; use https://"
        ));
    }
    let scheme = if tls { "https" } else { "http" };
    Ok(format!("{scheme}://{rest}"))
}

/// A hub key: `drth_` and 43 characters of base64url.
#[must_use]
pub fn is_key(key: &str) -> bool {
    key.len() == 48
        && key.starts_with("drth_")
        && key[5..]
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// A slug the hub registers: 2-40 of `a-z`, `0-9` and single inner hyphens.
#[must_use]
pub fn is_slug(slug: &str) -> bool {
    (2..=40).contains(&slug.len())
        && slug
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !slug.starts_with('-')
        && !slug.ends_with('-')
        && !slug.contains("--")
}

/// How the world is, as it reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// Running: players may come in.
    Online,
    /// Counting down to a shutdown.
    Maintenance,
}

/// The heartbeat the world sends: its state, the characters in the world (counted as characters
/// connected), its version, the server that sent it and its game. An `offline` one counts no one.
#[must_use]
pub fn heartbeat_of(state: Option<State>, players: u32) -> Heartbeat {
    Heartbeat {
        state: match state {
            Some(State::Online) => "online",
            Some(State::Maintenance) => "maintenance",
            None => "offline",
        }
        .to_owned(),
        players_online: state.map(|_| players),
        players_kind: Some(heartbeat::CONNECTED_CHARACTERS.to_owned()),
        emulator_version: Some(env!("CARGO_PKG_VERSION").to_owned()),
        adapter: Some(Adapter {
            kind: ADAPTER.to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
        }),
        game: Some(GAME),
    }
}

/// What the world tells the reporting thread.
enum Report {
    /// The world now: its state and how many characters are in it.
    Now(State, u32),
    /// The world is stopping.
    Stop,
}

/// The thread that posts the world's heartbeats.
#[derive(Debug)]
pub struct Reporter {
    tx: Sender<Report>,
    thread: Option<JoinHandle<()>>,
    last: (State, u32),
}

impl Reporter {
    /// Starts reporting as `settings` say: the first heartbeat goes at once.
    #[must_use]
    pub fn start(settings: HubSettings) -> Self {
        let (tx, rx) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("hub".into())
            .spawn(move || report(&settings, &rx))
            .ok();
        if thread.is_none() {
            tracing::warn!("the hub: could not start its thread; the world is not reported");
        }
        Self {
            tx,
            thread,
            last: (State::Online, 0),
        }
    }

    /// The world as it is now. Cheap: the thread posts a change of state at once, and a change of
    /// count with the next minute's heartbeat.
    pub fn now(&mut self, state: State, players: u32) {
        if (state, players) != self.last {
            self.last = (state, players);
            let _ = self.tx.send(Report::Now(state, players));
        }
    }

    /// Reports the world `offline` and stops the thread, once that heartbeat is posted or given
    /// up.
    pub fn stop(mut self) {
        let _ = self.tx.send(Report::Stop);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// The thread: a heartbeat at start, each minute, at each change of state, and `offline` at the
/// end.
fn report(settings: &HubSettings, rx: &Receiver<Report>) {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .user_agent(concat!("yalain/", env!("CARGO_PKG_VERSION")))
        .timeout_global(Some(POST_TIMEOUT))
        .http_status_as_error(false)
        .build()
        .into();
    let url = settings.heartbeat_url();
    tracing::info!("the hub: reporting this world as {}", settings.slug);
    let (mut state, mut players) = (State::Online, 0);
    let mut failing = false;
    let mut post = |h: &Heartbeat| {
        let body = serde_json::to_string(h).unwrap_or_default();
        let answer = agent
            .post(&url)
            .header("Authorization", format!("Bearer {}", settings.key))
            .header("Content-Type", "application/json")
            .send(body.as_bytes());
        match answer {
            Ok(r) if r.status().is_success() => {
                if std::mem::take(&mut failing) {
                    tracing::info!("the hub takes this world's heartbeats again");
                }
            }
            Ok(mut r) => {
                let status = r.status().as_u16();
                let why = r
                    .body_mut()
                    .with_config()
                    .limit(4096)
                    .read_to_string()
                    .unwrap_or_default();
                if !std::mem::replace(&mut failing, true) {
                    tracing::warn!(
                        "the hub refused this world's heartbeat (HTTP {status}{}): {why}",
                        if status == 401 {
                            "; is the key the one the hub gave?"
                        } else {
                            ""
                        }
                    );
                }
            }
            Err(e) => {
                if !std::mem::replace(&mut failing, true) {
                    tracing::warn!("the hub could not be reached: {e}");
                }
            }
        }
    };
    post(&heartbeat_of(Some(state), players));
    let mut due = Instant::now() + EVERY;
    loop {
        match rx.recv_timeout(due.saturating_duration_since(Instant::now())) {
            Ok(Report::Now(s, n)) => {
                let changed = s != state;
                (state, players) = (s, n);
                if changed {
                    post(&heartbeat_of(Some(state), players));
                    due = Instant::now() + EVERY;
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                post(&heartbeat_of(Some(state), players));
                due = Instant::now() + EVERY;
            }
            Ok(Report::Stop) | Err(RecvTimeoutError::Disconnected) => {
                post(&heartbeat_of(None, 0));
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the hub's settings, and the heartbeats' order and shape at a stand-in hub)
    //!
    //! The executable's report itself is a live station.
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::{TcpListener, TcpStream};

    const KEY: &str = "drth_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

    fn key_file(dir: &Path, text: &str) -> std::path::PathBuf {
        let path = dir.join("hub.key");
        std::fs::write(&path, text).expect("the key file");
        path
    }

    fn temp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("yalain-hub-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a folder");
        dir
    }

    /// One request a stand-in hub took: its request line, its headers and its body.
    struct Taken {
        line: String,
        headers: Vec<(String, String)>,
        body: serde_json::Value,
    }

    /// Takes the next request on `listener` and answers it `200`.
    fn take_now(listener: &TcpListener) -> Taken {
        let (stream, _) = listener.accept().expect("the world posts");
        let mut reader = BufReader::new(stream.try_clone().expect("the stream"));
        let mut line = String::new();
        reader.read_line(&mut line).expect("the request line");
        let mut headers = Vec::new();
        loop {
            let mut h = String::new();
            reader.read_line(&mut h).expect("a header");
            let h = h.trim_end();
            if h.is_empty() {
                break;
            }
            let (k, v) = h.split_once(':').expect("a header line");
            headers.push((k.trim().to_ascii_lowercase(), v.trim().to_owned()));
        }
        let length: usize = headers
            .iter()
            .find(|(k, _)| k == "content-length")
            .map_or(0, |(_, v)| v.parse().expect("a length"));
        let mut body = vec![0; length];
        reader.read_exact(&mut body).expect("the body");
        let mut out: TcpStream = stream;
        out.write_all(
            b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
        )
        .expect("the answer");
        Taken {
            line: line.trim_end().to_owned(),
            headers,
            body: serde_json::from_slice(&body).expect("JSON"),
        }
    }

    /// [`take_now`] on a thread of its own, waited for: the wait is a guard against a world that
    /// never posts, not the time a post is expected to take.
    fn take(listener: &TcpListener) -> Taken {
        let listener = listener.try_clone().expect("the listener");
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(take_now(&listener));
        });
        rx.recv_timeout(Duration::from_secs(120))
            .expect("the world posts its heartbeat")
    }

    #[test]
    fn the_world_reports_itself_to_the_hub_as_an_auberean_world_at_start_on_a_change_and_at_stop() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a stand-in hub");
        let port = listener.local_addr().expect("its port").port();
        let dir = temp("report");
        let settings = HubSettings::new(
            &format!("http://127.0.0.1:{port}/"),
            "shardfall",
            &key_file(&dir, &format!("{KEY}\n")),
        )
        .expect("settings");
        let mut reporter = Reporter::start(settings);

        let first = take(&listener);
        assert_eq!(first.line, "POST /v1/servers/shardfall/heartbeat HTTP/1.1");
        assert!(first
            .headers
            .contains(&("authorization".into(), format!("Bearer {KEY}"))));
        assert_eq!(first.body["state"], "online");
        assert_eq!(first.body["playersOnline"], 0);
        assert_eq!(first.body["playersKind"], "connected-characters");
        assert_eq!(first.body["game"], "auberean");
        assert_eq!(first.body["emulatorVersion"], env!("CARGO_PKG_VERSION"));
        assert_eq!(first.body["adapter"]["kind"], "yalain");
        assert_eq!(first.body["adapter"]["version"], env!("CARGO_PKG_VERSION"));

        // A count alone waits for the minute; a change of state goes at once, with the count.
        reporter.now(State::Online, 3);
        reporter.now(State::Maintenance, 2);
        let counting = take(&listener);
        assert_eq!(counting.body["state"], "maintenance");
        assert_eq!(counting.body["playersOnline"], 2);
        assert_eq!(counting.body["game"], "auberean");

        let stopping = std::thread::spawn(move || reporter.stop());
        let last = take(&listener);
        stopping.join().expect("stopped");
        assert_eq!(last.body["state"], "offline");
        assert!(last.body.get("playersOnline").is_none(), "{}", last.body);
        assert_eq!(last.body["game"], "auberean");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_hub_settings_take_a_key_file_a_slug_and_an_https_or_local_address() {
        let dir = temp("settings");
        let good = key_file(&dir, &format!("  {KEY}\r\n"));
        let s = HubSettings::new("https://hub.example.org/", "shardfall", &good).expect("read");
        assert_eq!(s.key, KEY);
        assert_eq!(
            s.heartbeat_url(),
            "https://hub.example.org/v1/servers/shardfall/heartbeat"
        );
        assert!(!format!("{s:?}").contains(KEY), "the key is never shown");
        assert_eq!(
            HubSettings::new("wss://hub.example.org", "shardfall", &good)
                .expect("read")
                .url,
            "https://hub.example.org"
        );
        assert_eq!(
            HubSettings::new("ws://127.0.0.1:8650", "shardfall", &good)
                .expect("read")
                .url,
            "http://127.0.0.1:8650"
        );
        assert!(HubSettings::new("http://hub.example.org", "shardfall", &good).is_err());
        assert!(HubSettings::new("hub.example.org", "shardfall", &good).is_err());
        assert!(HubSettings::new("https://user@hub.example.org", "shardfall", &good).is_err());
        for slug in [
            "s",
            "Shardfall",
            "-shard",
            "shard-",
            "sh--ard",
            "shard fall",
        ] {
            assert!(
                HubSettings::new("https://hub.example.org", slug, &good).is_err(),
                "{slug}"
            );
        }
        let bad = key_file(&dir, "drth_short");
        assert!(HubSettings::new("https://hub.example.org", "shardfall", &bad).is_err());
        assert!(HubSettings::new(
            "https://hub.example.org",
            "shardfall",
            &dir.join("missing.key")
        )
        .is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}
