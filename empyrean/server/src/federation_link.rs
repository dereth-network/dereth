//! Not ACE: the Dereth Network's link: one WebSocket from this world to the hub, and the file the
//! network's records are kept in.
//!
//! The world half of the network (`empyrean_world::federation`) runs on the world thread and never
//! touches a socket. This module is its host:
//!
//! - **The link** is the network's own ([`dereth_network_link::LinkClient`]): a thread that
//!   connects to `<hub>/v1/servers/<slug>/link` with the world's key, carries frames both ways,
//!   keeps the link alive and reconnects with growing waits (1, 2, 5, 10, then every 30 seconds; a
//!   minute after the hub refuses the key or the world). `wss://` is served through rustls with
//!   the web's root certificates; plain `ws://` is accepted for a loopback hub only. A hub that
//!   sends faster than the world takes its frames loses the link, as one that falls silent does.
//! - **The pump** ([`FederationLink::pump`], run on the world thread between loop iterations)
//!   hands the world what the link received, at most [`FRAMES_PER_PUMP`] at a time, lets the
//!   world's clock run, and takes what it has to send, for the connection the world last heard
//!   come up. It logs what happened to the link.
//! - **`/dn verify`** goes to the hub's verification route over HTTPS (HTTP for a loopback hub),
//!   with the world's key, on a thread of its own per request; its answer comes back to the world
//!   on a later pump, so nothing waits on the hub. While it waits, the hub challenges the world's
//!   game port: each nonce the world takes from a challenge goes back to the hub's answer route
//!   the same way, from whatever address the world's calls leave by.
//! - **The store** is written by a thread of its own, the newest snapshot only, through a
//!   temporary file and a rename, at most once every [`SAVE_EVERY`]: the world thread hands over a
//!   copy, which shares the stand-in table and every character's record with the world's own (so
//!   making it copies none of them), and never serialises it or waits on the disk. Its next
//!   stand-in id is written [`STAND_IN_MARGIN`] ahead, and a copy goes at once whenever half that
//!   many ids have been given since the last, so a crash between giving an id and saving never
//!   gives that id again.
//!
//! Nothing here logs a key or a message body.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use dereth_network_link::{is_key, Event, HubUrl, LinkClient};
use dereth_network_protocol::verification;
use empyrean_common::federation_configuration::FederationConfiguration;
use empyrean_world::federation::{self, FederationSettings, NetworkStore, RoomSlot};
use empyrean_world::World;

pub use dereth_network_link::verify::VERIFY_TIMEOUT;

/// What a refusal of the world's key is answered with, and where the key is set.
const KEY_UNKNOWN: &str = "The Dereth Network did not verify this world: the hub does not know this world's key. Check federation.key_file against the key the site gave you.";

/// What the world's calls to the hub's verification routes name themselves.
const USER_AGENT: &str = concat!("empyrean-server/", env!("CARGO_PKG_VERSION"));

/// The least time between two saves of the store: a burst of changes is one save.
pub const SAVE_EVERY: Duration = Duration::from_secs(2);

/// The most of what the link received that one pump hands the world; the rest waits for the next,
/// so a hub that sends without pause holds no iteration of the world's loop for long.
pub const FRAMES_PER_PUMP: usize = 256;

/// The network's settings and records, ready for the world.
pub struct Prepared {
    pub settings: FederationSettings,
    pub store: NetworkStore,
    pub url: HubUrl,
    pub key: String,
    pub store_path: PathBuf,
}

impl std::fmt::Debug for Prepared {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Prepared")
            .field("settings", &self.settings)
            .field("url", &self.url)
            .field("store_path", &self.store_path)
            .finish_non_exhaustive()
    }
}

/// Reads `[federation]`, the key file and the store. `resolve` turns a configured path into one
/// to open.
///
/// # Errors
/// A setting that cannot be served, a key file that cannot be read or holds no key, or a store
/// that exists and cannot be read (rather than start over and give an id twice). The server does
/// not start on any of these.
pub fn prepare(
    config: &FederationConfiguration,
    resolve: impl Fn(&str) -> PathBuf,
) -> Result<Option<Prepared>, String> {
    if !config.enabled {
        return Ok(None);
    }
    if !dereth_network_protocol::address::is_network_slug(&config.slug) {
        return Err(format!(
            "federation.slug {:?} is not a network slug (2-12 lowercase letters, digits and hyphens, not reserved)",
            config.slug
        ));
    }
    let url = HubUrl::parse(&config.hub_url)?;
    let global_room = global_room(config.global_room.as_deref())?;
    let key_path = resolve(&config.key_file);
    let key = std::fs::read_to_string(&key_path)
        .map_err(|e| format!("federation.key_file {}: {e}", key_path.display()))?
        .trim()
        .to_owned();
    if !is_key(&key) {
        return Err(format!(
            "federation.key_file {} holds no registry key (drth_...)",
            key_path.display()
        ));
    }
    let store_path = resolve(&config.store_path);
    let store = load_store(&store_path)?;
    Ok(Some(Prepared {
        settings: FederationSettings {
            enabled: true,
            slug: config.slug.clone(),
            global_room,
            global_room_named: config.global_room.is_some(),
            min_character_age_secs: config.min_character_age_seconds,
            websocket_url: None,
            status_url: None,
        },
        store,
        url,
        key,
        store_path,
    }))
}

/// Where a browser reaches this world, for its `hello`: the WebSocket endpoint's URL while
/// `[server.websocket]` serves one (the URL the status endpoint reports), and
/// `server.status_public_url`. Each only within the network's rule (`wss://` or `https://`, or a
/// plain one on this machine): one outside it is left out, and the log says why.
#[must_use]
pub fn web_addresses(
    server: &empyrean_common::game_configuration::GameConfiguration,
) -> (Option<String>, Option<String>) {
    use dereth_network_protocol::web::{is_status_url, is_websocket_url};
    let ws = &server.web_socket;
    let websocket = ws
        .enabled
        .then(|| ws.validate().ok().map(|(listen, tls)| ws.url(listen, tls)))
        .flatten()
        .filter(|url| {
            let ok = is_websocket_url(url);
            if !ok {
                log::warn!(
                    "Dereth Network: the hub is not told the WebSocket endpoint {url:?}: it must be \
                     wss://, or ws:// on this machine (server.websocket.public_url names it)"
                );
            }
            ok
        });
    let status = Some(server.status_public_url.trim())
        .filter(|url| !url.is_empty())
        .filter(|url| {
            let ok = is_status_url(url);
            if !ok {
                log::warn!(
                    "Dereth Network: the hub is not told server.status_public_url {url:?}: it \
                     must be https://, or http:// on this machine"
                );
            }
            ok
        })
        .map(str::to_owned);
    (websocket, status)
}

/// `federation.global_room`: the room the world carries the Global room in until its hub keeps
/// one, and tells a hub that keeps none; Roleplay when not set, and none when empty.
///
/// # Errors
/// A name that is not one of the four rooms.
fn global_room(named: Option<&str>) -> Result<Option<RoomSlot>, String> {
    let word = dereth_network_protocol::rooms::read_setting(named, "roleplay")?;
    Ok(word.and_then(RoomSlot::parse))
}

/// The saved records; none yet is an empty store.
///
/// # Errors
/// A file that exists and does not read as the store.
pub fn load_store(path: &Path) -> Result<NetworkStore, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text)
            .map_err(|e| format!("the network store {}: {e}", path.display())),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(NetworkStore::default()),
        Err(e) => Err(format!("the network store {}: {e}", path.display())),
    }
}

/// Writes `bytes` to `path` through a temporary file and a rename.
///
/// # Errors
/// When either step fails.
pub fn write_atomically(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        if !dir.as_os_str().is_empty() {
            std::fs::create_dir_all(dir)?;
        }
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

/// How far past the next stand-in id a saved store starts (see `StandIns::reserved`).
pub const STAND_IN_MARGIN: u32 = 1024;

/// The store as it is written: the stand-in table moved on by [`STAND_IN_MARGIN`].
fn saved(store: &NetworkStore) -> NetworkStore {
    let mut s = store.clone();
    s.stand_ins = Arc::new(s.stand_ins.reserved(STAND_IN_MARGIN));
    s
}

/// The line a `/dn verify` answers with, from the hub's answer (`status` and its JSON body).
#[must_use]
pub fn verify_line(status: u16, body: &str) -> String {
    verification::answer_line(status, body, KEY_UNKNOWN)
}

/// Posts the nonce of the hub's challenge (32 hex digits) to the hub's answer route with the
/// world's key: the hub's status, or why it could not be reached.
///
/// # Errors
/// The hub could not be reached.
pub fn send_answer(url: &str, key: &str, nonce: &str) -> Result<u16, String> {
    dereth_network_link::verify::send_answer(url, key, nonce, USER_AGENT)
}

/// Sends one `/dn verify` to the hub and says how it went.
fn send_verification(url: &str, key: &str, r: &federation::VerifyRequest) -> String {
    dereth_network_link::verify::send_verification(&dereth_network_link::verify::Verification {
        url,
        key,
        token: &r.token,
        by: &r.by,
        user_agent: USER_AGENT,
        key_unknown: KEY_UNKNOWN,
    })
}

/// The running link.
pub struct FederationLink {
    link: LinkClient,
    saves: Saves,
    saver: Option<JoinHandle<()>>,
    /// The hub's verification route, the route for the answer to its challenge, and the key both
    /// are called with.
    verify_url: String,
    answer_url: String,
    key: String,
    /// Answers to `/dn verify`, from their threads.
    verified: (Sender<VerifyAnswer>, Receiver<VerifyAnswer>),
}

/// The answer to one `/dn verify`: whom to show it to (the console when none), and the line.
type VerifyAnswer = (Option<empyrean_entity::ObjectGuid>, String);

/// The store's saves, as the world thread makes them: a copy of the store handed to the writer
/// thread, which turns it into JSON and writes it, at most once every [`SAVE_EVERY`] however
/// often it changes, unless half of [`STAND_IN_MARGIN`] new stand-in ids have been given since the
/// last copy: then at once, so the ids given since the last save never outrun the margin the
/// saved table starts past. The world thread never serialises the store or waits on the disk.
#[derive(Debug)]
struct Saves {
    tx: Sender<NetworkStore>,
    next: Instant,
    /// How many stand-in ids the store had given at the last copy.
    stand_ins: usize,
}

impl Saves {
    /// Saves of a store that has given `stand_ins` ids.
    fn new(tx: Sender<NetworkStore>, stand_ins: usize) -> Self {
        Self {
            tx,
            next: Instant::now(),
            stand_ins,
        }
    }

    /// Hands a copy of the store to the writer when it changed and the last save is old enough,
    /// or when it has given half the margin in new ids since; otherwise it stays marked changed
    /// for a later call.
    fn offer(&mut self, now: Instant, w: &mut World) {
        let given = w.federation.store.stand_ins.len();
        let urgent = given.saturating_sub(self.stand_ins) >= (STAND_IN_MARGIN / 2) as usize;
        if !w.federation.dirty || (now < self.next && !urgent) {
            return;
        }
        w.federation.dirty = false;
        self.next = now + SAVE_EVERY;
        self.stand_ins = given;
        let _ = self.tx.send(w.federation.store.clone());
    }
}

/// The bytes the writer puts on disk for a store.
///
/// # Errors
/// When the store does not serialise.
fn store_bytes(store: &NetworkStore) -> serde_json::Result<Vec<u8>> {
    serde_json::to_vec(&saved(store))
}

impl std::fmt::Debug for FederationLink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FederationLink")
            .field("link", &self.link)
            .finish_non_exhaustive()
    }
}

impl FederationLink {
    /// Starts the link thread and the store's writer.
    ///
    /// # Errors
    /// When a thread cannot be started.
    pub fn start(p: &Prepared) -> io::Result<Self> {
        let link = LinkClient::start(&p.url, &p.settings.slug, &p.key)?;
        let (saves, save_rx) = mpsc::channel::<NetworkStore>();
        let path = p.store_path.clone();
        let saver = std::thread::Builder::new()
            .name("federation-store".into())
            .spawn(move || {
                while let Ok(mut store) = save_rx.recv() {
                    while let Ok(newer) = save_rx.try_recv() {
                        store = newer;
                    }
                    match store_bytes(&store) {
                        Ok(bytes) => {
                            if let Err(e) = write_atomically(&path, &bytes) {
                                log::error!("Dereth Network: saving {}: {e}", path.display());
                            }
                        }
                        Err(e) => log::error!("Dereth Network: the store does not serialise: {e}"),
                    }
                }
            })?;
        Ok(Self {
            link,
            saves: Saves::new(saves, p.store.stand_ins.len()),
            saver: Some(saver),
            verify_url: p.url.verify(&p.settings.slug),
            answer_url: p.url.answer(&p.settings.slug),
            key: p.key.clone(),
            verified: mpsc::channel(),
        })
    }

    /// On the world thread: what the link received goes to the world, the world's clock runs,
    /// and what it has to send goes to the link; a changed store is saved.
    pub fn pump(&mut self, w: &mut World) {
        for _ in 0..FRAMES_PER_PUMP {
            let Some(e) = self.link.try_event() else {
                break;
            };
            match e {
                Event::Up => federation::on_link_up(w),
                Event::Frame(text) => federation::on_link_frame(w, &text),
                Event::Down(why) => {
                    log::info!("Dereth Network: link down ({why})");
                    federation::on_link_down(w);
                }
                Event::Refused { status, .. } => log::error!(
                    "Dereth Network: the hub refused this world ({status}: {}); trying again in a minute",
                    if status == 401 { "the key" } else { "its federation flag is not on" }
                ),
                Event::Unreachable { why, retry_in } => log::warn!(
                    "Dereth Network: cannot reach the hub ({why}); trying again in {} s",
                    retry_in.as_secs()
                ),
            }
        }
        federation::tick(w);
        for frame in federation::take_frames(w) {
            self.link.send(frame);
        }
        for r in federation::take_verify_requests(w) {
            let (url, key, tx) = (
                self.verify_url.clone(),
                self.key.clone(),
                self.verified.0.clone(),
            );
            let spawned = std::thread::Builder::new()
                .name("federation-verify".into())
                .spawn(move || {
                    let line = send_verification(&url, &key, &r);
                    let _ = tx.send((r.who, line));
                });
            if spawned.is_err() {
                federation::on_verify_answer(w, None, verification::NOT_SENT);
            }
        }
        for a in federation::take_challenge_answers(w) {
            let (url, key) = (self.answer_url.clone(), self.key.clone());
            let spawned = std::thread::Builder::new()
                .name("federation-answer".into())
                .spawn(move || match send_answer(&url, &key, &a.nonce) {
                    Ok(200) => log::info!("Dereth Network: answered the hub's challenge"),
                    Ok(status) => log::info!(
                        "Dereth Network: the hub did not take the answer to its challenge ({status})"
                    ),
                    Err(e) => log::warn!(
                        "Dereth Network: the answer to the hub's challenge could not be sent: {e}"
                    ),
                });
            if spawned.is_err() {
                log::warn!("Dereth Network: the answer to the hub's challenge could not be sent");
            }
        }
        while let Ok((who, line)) = self.verified.1.try_recv() {
            federation::on_verify_answer(w, who, &line);
        }
        self.saves.offer(Instant::now(), w);
    }

    /// Saves what is left and stops both threads.
    pub fn shutdown(mut self, w: &mut World) {
        let _ = self.saves.tx.send(w.federation.store.clone());
        w.federation.dirty = false;
        self.link.stop();
        drop(std::mem::replace(&mut self.saves.tx, mpsc::channel().0));
        if let Some(t) = self.saver.take() {
            let _ = t.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Divergence: V451
    /// A world tells the hub its WebSocket endpoint while it serves one, as the status endpoint
    /// reports it, and its status document's public address, each only as `wss://` or
    /// `https://`, or a plain one on this machine.
    #[test]
    fn the_hub_is_told_the_websocket_endpoint_and_the_public_status_address() {
        use empyrean_common::game_configuration::GameConfiguration;
        let mut server = GameConfiguration::default();
        assert_eq!(web_addresses(&server), (None, None), "neither is on");
        server.web_socket.enabled = true;
        server.web_socket.listen = "127.0.0.1:9180".into();
        server.status_public_url = "http://127.0.0.1:9100/status".into();
        assert_eq!(
            web_addresses(&server),
            (
                Some("ws://127.0.0.1:9180/".into()),
                Some("http://127.0.0.1:9100/status".into())
            )
        );
        server.web_socket.listen = "10.0.0.5:9180".into();
        server.web_socket.behind_tls_proxy = true;
        server.status_public_url = "http://play.example.org/status".into();
        assert_eq!(
            web_addresses(&server),
            (None, None),
            "plain addresses to another machine are not told"
        );
        server.web_socket.public_url = "wss://play.example.org/ws".into();
        server.status_public_url = " https://play.example.org/status ".into();
        assert_eq!(
            web_addresses(&server),
            (
                Some("wss://play.example.org/ws".into()),
                Some("https://play.example.org/status".into())
            )
        );
        server.web_socket.enabled = false;
        assert_eq!(web_addresses(&server).0, None, "the endpoint is off");
    }

    /// A hub on loopback that challenges quickly, and a world `slug` an operator registered on it
    /// at `port` of 127.0.0.1: the hub, the world's key and its token.
    fn hub_with_world(
        slug: &str,
        port: u16,
    ) -> (dereth_network_hub::server::Running, String, String) {
        use dereth_network_hub::registry::{Registration, Registry};
        use dereth_network_hub::server::{start_with, HubConfig};
        let registry = Registry::open(
            ":memory:",
            "dereth.network",
            Box::new(std::time::SystemTime::now),
        )
        .expect("a registry");
        let config = HubConfig {
            listen: "127.0.0.1:0".parse().expect("an address"),
            db: ":memory:".into(),
            loopback_allowed: true,
            challenge_every: Duration::from_millis(100),
            challenge_wait: Duration::from_secs(5),
            ..HubConfig::default()
        };
        let running = start_with(config, registry).expect("the hub starts");
        let op = running
            .registry()
            .signed_in("700", "Operator")
            .expect("an operator");
        let r = Registration::from_json(&serde_json::json!({
            "slug": slug, "name": format!("World {slug}"), "ruleset": "PvE",
            "endpoint": { "mode": "manual", "address": "127.0.0.1", "port": port },
            "emulator": { "family": "Empyrean" },
        }))
        .expect("a registration");
        let (_, key, issued) = running
            .registry()
            .register_for(&op, &r)
            .expect("registered");
        (running, key, issued.token)
    }

    /// A socket on loopback standing in for the world's game port, polled without blocking.
    fn game_port() -> (std::net::UdpSocket, u16) {
        let s = std::net::UdpSocket::bind("127.0.0.1:0").expect("a game port");
        s.set_nonblocking(true).expect("nonblocking");
        let port = s.local_addr().expect("an address").port();
        (s, port)
    }

    /// Divergence: V451
    #[test]
    fn dn_verify_goes_to_the_hubs_verification_route_with_the_worlds_key() {
        let (socket, port) = game_port();
        let (running, key, token) = hub_with_world("aa", port);
        let hub = HubUrl::parse(&format!("ws://{}", running.local_addr())).expect("a hub");
        let (url, answer_url) = (hub.verify("aa"), hub.answer("aa"));
        assert_eq!(
            url,
            format!("http://{}/v1/servers/aa/verify", running.local_addr())
        );
        let request = federation::VerifyRequest {
            token: token.clone(),
            by: "Asheron".into(),
            who: None,
        };
        let wrong = send_verification(&url, &format!("drth_{}", "x".repeat(43)), &request);
        assert!(wrong.contains("does not know this world's key"), "{wrong}");
        let not_it = federation::VerifyRequest {
            token: format!("dnv_{}", "q".repeat(32)),
            ..request.clone()
        };
        let refused = send_verification(&url, &key, &not_it);
        assert!(
            refused.starts_with("The Dereth Network did not verify this world: That is not"),
            "{refused}"
        );
        // The right token: the hub challenges the game port, and the nonce goes back to it.
        let line = std::thread::scope(|s| {
            let call = s.spawn(|| send_verification(&url, &key, &request));
            let until = Instant::now() + Duration::from_secs(5);
            let mut buf = [0u8; 2048];
            let nonce = loop {
                assert!(Instant::now() < until, "no challenge came");
                if let Ok((n, _)) = socket.recv_from(&mut buf) {
                    if let Some(c) = dereth_network_protocol::challenge::Challenge::parse(&buf[..n])
                    {
                        break c.nonce;
                    }
                }
                std::thread::sleep(Duration::from_millis(5));
            };
            let hex = dereth_network_protocol::challenge::nonce_hex(&nonce);
            assert_eq!(send_answer(&answer_url, &key, &hex), Ok(200));
            call.join().expect("the call")
        });
        assert_eq!(
            line,
            "The Dereth Network verified this world. It is on the network now; the link comes up within a minute."
        );
        assert_eq!(
            running
                .registry()
                .federation("aa")
                .expect("read")
                .map(|x| x.1),
            Some(dereth_network_protocol::Federation::On)
        );
        running.shutdown();
        assert!(send_verification(&url, &key, &request)
            .starts_with("The Dereth Network's hub could not be reached"));
    }

    /// Divergence: V451
    #[test]
    fn while_dn_verify_waits_the_pump_posts_back_the_challenge_its_game_port_heard() {
        let (socket, port) = game_port();
        let (running, key, token) = hub_with_world("bb", port);
        let hub = HubUrl::parse(&format!("ws://{}", running.local_addr())).expect("a hub");
        let mut w = world();
        federation::configure(
            &mut w,
            FederationSettings {
                enabled: true,
                slug: "bb".into(),
                global_room: None,
                global_room_named: false,
                min_character_age_secs: 0,
                websocket_url: None,
                status_url: None,
            },
            NetworkStore::default(),
        );
        let (mut link, _events) = bare_link();
        link.verify_url = hub.verify("bb");
        link.answer_url = hub.answer("bb");
        link.key = key;
        assert_eq!(
            federation::commands::verify(&mut w, None, "Asheron", std::slice::from_ref(&token)),
            "Asking the Dereth Network's hub to verify this world..."
        );
        let on = || {
            running
                .registry()
                .federation("bb")
                .expect("read")
                .map(|x| x.1)
                == Some(dereth_network_protocol::Federation::On)
        };
        let until = Instant::now() + Duration::from_secs(10);
        let mut buf = [0u8; 2048];
        while !on() && Instant::now() < until {
            link.pump(&mut w);
            while let Ok((n, from)) = socket.recv_from(&mut buf) {
                let now = w.now;
                w.net
                    .on_datagram(empyrean_net::PortKind::C2S, from, &buf[..n], now);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(on(), "verified, and on the network");
        let audit = running.registry().audit("bb").expect("the log");
        let verified = audit
            .as_array()
            .expect("events")
            .iter()
            .find(|e| e["kind"] == "verified")
            .cloned()
            .expect("verified");
        assert_eq!(verified["details"]["method"], "game");
        assert_eq!(verified["details"]["address"], "127.0.0.1");
        running.shutdown();
    }

    /// Divergence: V451
    #[test]
    fn a_verification_answer_is_a_line_for_whoever_typed_it() {
        assert!(verify_line(200, r#"{"approval":"rate_limited"}"#).contains("allowance"));
        assert!(verify_line(200, r#"{"approval":"not_network_slug"}"#).contains("slug"));
        assert!(verify_line(429, "{}").contains("too many attempts"));
        assert_eq!(
            verify_line(
                422,
                r#"{"error":{"message":"That is not this world's verification token\nForged"}}"#
            ),
            "The Dereth Network did not verify this world: That is not this world's verification tokenForged."
        );
        assert_eq!(
            verify_line(
                422,
                r#"{"error":{"message":"The hub's challenge to 192.0.2.1:9000 got no answer: is the game port reachable from the internet?"}}"#
            ),
            "The Dereth Network did not verify this world: The hub's challenge to 192.0.2.1:9000 got no answer: is the game port reachable from the internet?",
            "a question is not ended again"
        );
        assert_eq!(
            verify_line(503, "not json"),
            "The Dereth Network did not verify this world: the hub answered 503."
        );
        let p = HubUrl::parse("wss://example.org:9443/net").expect("a prefix");
        assert_eq!(
            p.verify("aa"),
            "https://example.org:9443/net/v1/servers/aa/verify"
        );
        assert_eq!(
            p.answer("aa"),
            "https://example.org:9443/net/v1/servers/aa/verify/answer"
        );
    }

    /// Divergence: V451
    #[test]
    fn the_store_is_saved_with_its_next_stand_in_id_moved_on() {
        let mut store = NetworkStore::default();
        let bob = dereth_network_protocol::Address::parse("Bob@bb").expect("an address");
        let eve = dereth_network_protocol::Address::parse("Eve@bb").expect("an address");
        let first = Arc::make_mut(&mut store.stand_ins)
            .id_for(&bob)
            .expect("an id");
        let mut on_disk = (*saved(&store).stand_ins).clone();
        let mut live = (*store.stand_ins).clone();
        assert_eq!(
            on_disk.get(&bob),
            Some(first),
            "saved ids keep their identities"
        );
        let given = live.id_for(&eve).expect("an id");
        let after_restart = on_disk.id_for(&eve).expect("an id");
        assert!(
            after_restart >= given + STAND_IN_MARGIN,
            "{after_restart:#x} after {given:#x}"
        );
    }

    /// Divergence: V451
    #[test]
    fn ids_given_faster_than_the_save_pace_are_never_given_again_after_a_crash() {
        let clock = empyrean_common::clock::VirtualClock::default();
        let mut w = World::new(
            empyrean_common::clock::ClockSnapshot::take(&clock, 0.0),
            empyrean_dat::FakeDats::new()
                .build()
                .expect("empty fake dats"),
        );
        let (tx, rx) = mpsc::channel();
        let mut saves = Saves::new(tx, 0);
        let start = Instant::now();
        let mut on_disk: Option<NetworkStore> = None;
        let mut given = 0u32;
        // Six remote worlds and a few friend adds, 200 new ids at a time, all within one save
        // interval.
        for burst in 0..20u32 {
            for i in 0..200u32 {
                let n = burst * 200 + i;
                let name: String = [1, 26, 676]
                    .iter()
                    .map(|d| char::from(b'a' + u8::try_from(n / d % 26).expect("a letter")))
                    .collect();
                let who = dereth_network_protocol::Address::parse(&format!("Mint{name}@ww"))
                    .expect("an address");
                given = given.max(
                    Arc::make_mut(&mut w.federation.store.stand_ins)
                        .id_for(&who)
                        .expect("an id"),
                );
            }
            w.federation.dirty = true;
            saves.offer(start + Duration::from_millis(u64::from(burst)), &mut w);
            while let Ok(s) = rx.try_recv() {
                on_disk = Some(saved(&s));
            }
            // A crash now: the world starts again from what was last written.
            let mut restarted =
                (*on_disk.clone().expect("a save was handed over").stand_ins).clone();
            let next = restarted
                .id_for(&dereth_network_protocol::Address::parse("New@zz").expect("an address"))
                .expect("an id");
            assert!(
                next > given,
                "after burst {burst}: {next:#x} would be given again (given up to {given:#x})"
            );
        }
    }

    fn world() -> World {
        let clock = empyrean_common::clock::VirtualClock::default();
        World::new(
            empyrean_common::clock::ClockSnapshot::take(&clock, 0.0),
            empyrean_dat::FakeDats::new()
                .build()
                .expect("empty fake dats"),
        )
    }

    /// Divergence: V451
    #[test]
    fn a_copy_for_the_writer_shares_the_table_and_every_record_and_keeps_what_it_was_handed() {
        let mut w = world();
        let bob = dereth_network_protocol::Address::parse("Bob@bb").expect("an address");
        w.federation
            .store
            .characters
            .insert(1, Arc::new(federation::CharacterRecord::default()));
        Arc::make_mut(&mut w.federation.store.stand_ins)
            .id_for(&bob)
            .expect("an id");
        let (tx, rx) = mpsc::channel();
        let mut saves = Saves::new(tx, 0);
        w.federation.dirty = true;
        saves.offer(Instant::now(), &mut w);
        let copy = rx.try_recv().expect("a copy is handed over");
        assert!(
            Arc::ptr_eq(&copy.stand_ins, &w.federation.store.stand_ins),
            "the table is shared, not copied"
        );
        assert!(
            Arc::ptr_eq(&copy.characters[&1], &w.federation.store.characters[&1]),
            "and every record"
        );
        // The world changes a record while the writer holds the copy: the copy is unchanged.
        Arc::make_mut(w.federation.store.characters.get_mut(&1).expect("a record"))
            .friends
            .insert(bob.key(), bob.clone());
        assert!(copy.characters[&1].friends.is_empty());
        assert_eq!(w.federation.store.characters[&1].friends.len(), 1);
    }

    /// A link with no thread: the test hands it events itself.
    fn bare_link() -> (FederationLink, dereth_network_link::Detached) {
        let (link, far) = LinkClient::detached();
        let (saves, _) = mpsc::channel();
        (
            FederationLink {
                link,
                saves: Saves::new(saves, 0),
                saver: None,
                verify_url: String::new(),
                answer_url: String::new(),
                key: String::new(),
                verified: mpsc::channel(),
            },
            far,
        )
    }

    /// Divergence: V451
    #[test]
    fn one_pump_hands_the_world_at_most_its_budget_of_frames() {
        let mut w = world();
        let (mut link, mut far) = bare_link();
        far.hand(Event::Up);
        for _ in 0..1_000 {
            far.hand(Event::Frame(r#"{"type":"a.later.type"}"#.into()));
        }
        link.pump(&mut w);
        assert_eq!(
            far.waiting(),
            1_000 - (FRAMES_PER_PUMP - 1),
            "what was taken no longer counts as waiting"
        );
        let mut left = 0;
        while link.link.try_event().is_some() {
            left += 1;
        }
        assert_eq!(left, 1_001 - FRAMES_PER_PUMP);
    }

    /// Divergence: V451
    #[test]
    fn the_world_links_to_its_hub_through_the_networks_link_and_is_welcomed() {
        use dereth_network_hub::registry::{Registration, Registry};
        use dereth_network_hub::server::{start_with, HubConfig};
        let registry = Registry::open(
            ":memory:",
            "dereth.network",
            Box::new(std::time::SystemTime::now),
        )
        .expect("a registry");
        let r = Registration::from_json(&serde_json::json!({
            "slug": "cc", "name": "World cc", "ruleset": "PvE",
            "endpoint": { "mode": "manual", "address": "192.0.2.1", "port": 9000 },
            "emulator": { "family": "Empyrean" },
        }))
        .expect("a registration");
        let (_, key) = registry.register(&r).expect("registered");
        registry
            .set_federation("cc", dereth_network_protocol::Federation::On)
            .expect("on");
        let running = start_with(
            HubConfig {
                listen: "127.0.0.1:0".parse().expect("an address"),
                db: ":memory:".into(),
                ..HubConfig::default()
            },
            registry,
        )
        .expect("the hub starts");
        let dir =
            std::env::temp_dir().join(format!("empyrean-federation-link-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a temp dir");
        let settings = FederationSettings {
            enabled: true,
            slug: "cc".into(),
            global_room: None,
            global_room_named: false,
            min_character_age_secs: 0,
            websocket_url: None,
            status_url: None,
        };
        let prepared = Prepared {
            settings: settings.clone(),
            store: NetworkStore::default(),
            url: HubUrl::parse(&format!("ws://{}", running.local_addr())).expect("a hub"),
            key,
            store_path: dir.join("network.json"),
        };
        let mut w = world();
        federation::configure(&mut w, settings, NetworkStore::default());
        let mut link = FederationLink::start(&prepared).expect("the link starts");
        // A guard against a link that never comes up, not the time it is expected to take.
        let until = Instant::now() + Duration::from_secs(60);
        while w.federation.link() != federation::LinkState::Up && Instant::now() < until {
            link.pump(&mut w);
            std::thread::sleep(Duration::from_millis(5));
        }
        let up = w.federation.link() == federation::LinkState::Up;
        let linked = running.linked();
        link.shutdown(&mut w);
        running.shutdown();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(up, "welcomed by the hub");
        assert!(linked.contains("cc"), "{linked:?}");
    }

    /// Divergence: V451
    #[test]
    fn a_store_it_cannot_read_is_refused_rather_than_started_over() {
        let dir = std::env::temp_dir().join(format!("empyrean-federation-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a temp dir");
        let key = dir.join("network.key");
        std::fs::write(&key, format!("drth_{}\n", "A".repeat(43))).expect("a key");
        let config = FederationConfiguration {
            enabled: true,
            slug: "aa".into(),
            global_room: Some("trade".into()),
            ..FederationConfiguration::default()
        };
        let resolve = |p: &str| dir.join(p);
        let ok = prepare(&config, resolve).expect("prepared").expect("on");
        assert_eq!(ok.settings.global_room, Some(RoomSlot::Trade));
        assert!(ok.settings.global_room_named);
        assert_eq!(
            ok.store,
            NetworkStore::default(),
            "no store yet is an empty one"
        );
        std::fs::write(dir.join("network.json"), "{ not json").expect("a broken store");
        assert!(prepare(&config, resolve).is_err());
        write_atomically(&dir.join("network.json"), b"{}").expect("written");
        assert!(prepare(&config, resolve).expect("read").is_some());
        let off = FederationConfiguration::default();
        assert!(prepare(&off, resolve).expect("off").is_none());
        std::fs::write(&key, "not a key").expect("a bad key");
        assert!(prepare(&config, resolve).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Divergence: V449
    #[test]
    fn the_settings_global_room_is_roleplay_unless_named_and_none_when_empty() {
        assert_eq!(global_room(None), Ok(Some(RoomSlot::Roleplay)));
        assert_eq!(global_room(Some(" General ")), Ok(Some(RoomSlot::General)));
        assert_eq!(global_room(Some("")), Ok(None));
        assert!(global_room(Some("global")).is_err());
    }
}
