//! The hub's I/O: one TCP listener for the registry's HTTP API and the worlds' links.
//!
//! - **The listener** binds loopback unless told otherwise. TLS is a reverse proxy's job in front
//!   of it; a forwarded client address is read only from the proxies the configuration names
//!   ([`crate::forwarded`]).
//! - **A link** is `GET /v1/servers/<slug>/link` upgraded to a WebSocket, with the world's key as
//!   `Authorization: Bearer drth_...`. A wrong key is refused 401 and a world whose federation
//!   flag is not on 403, before the upgrade. Each link has a thread that carries its frames both
//!   ways; every frame goes to the one hub thread, which owns the routing ([`crate::core::Hub`])
//!   and hands each link what it is to send. A world that stops taking its frames is dropped once
//!   a write to it has waited [`LINK_WRITE_TIMEOUT`], or at once when the frames waiting for it
//!   pass [`LINK_QUEUE_BYTES`]; the other worlds are told it is offline, as for any lost link.
//! - **The HTTP API** is the registry's: worlds, status, resolution, heartbeats and the two
//!   compatibility feeds; and the operator API ([`crate::api`]). Each request gets its own
//!   short-lived thread.
//! - **Limits on the listener.** Every request and every link attempt takes from its client
//!   address's allowance ([`HubConfig::http_per_minute`]); at most [`MAX_CONNECTIONS`] connections
//!   are open at once, and at most [`MAX_CONNECTIONS_PER_ADDRESS`] from one address that is not a
//!   trusted proxy (an IPv6 client counted by its /64); a request's head is given less time while
//!   half the connections are still sending theirs. A link stays up only while the key it was
//!   opened with is the world's key.
//! - **Channels** are saved one row each, the changed ones together in one transaction, at most
//!   once a second, after they change.
//!
//! Nothing here writes a message body or a key to the log.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::{self, Read, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use dereth_network_protocol::limits::{Bucket, Limits, Rate};
use dereth_network_protocol::wire::{WireError, MAX_MESSAGE_BYTES};
use dereth_network_protocol::{Federation, Message};
use serde_json::{json, Value};
use tungstenite::handshake::server::{Request, Response};

use crate::core::{Channels, Hub, Out};
use crate::forwarded::{ClientHeader, ProxyRange};
use crate::registry::{Heartbeat, Registry, RegistryError};

/// The hub's own name and version, as `welcome` reports it.
pub const SOFTWARE: &str = concat!("dereth-network-hub ", env!("CARGO_PKG_VERSION"));

/// How often a link is pinged, and how long one may stay silent before it is closed.
pub const PING_EVERY: Duration = Duration::from_secs(20);
/// See [`PING_EVERY`].
pub const IDLE_AFTER: Duration = Duration::from_secs(70);

/// How long a request's head may take to arrive.
const HEAD_TIMEOUT: Duration = Duration::from_secs(10);
/// How long it may take while many connections are still sending theirs ([`head_timeout`]).
const HEAD_TIMEOUT_BUSY: Duration = Duration::from_secs(2);
/// How long a request's whole body may take to arrive, however it trickles in.
pub const BODY_DEADLINE: Duration = Duration::from_secs(10);

/// The most connections open at once, and from one address that is not a trusted proxy.
pub const MAX_CONNECTIONS: usize = 2_048;
/// See [`MAX_CONNECTIONS`].
pub const MAX_CONNECTIONS_PER_ADDRESS: usize = 32;

/// The least time between two saves of the changed channels.
const SAVE_CHANNELS_EVERY: Duration = Duration::from_secs(1);

/// How long one write to a link may wait before the link is dropped: a world that stops taking
/// its frames is gone, as one that stops sending is.
pub const LINK_WRITE_TIMEOUT: Duration = Duration::from_secs(10);

/// How long a link's thread rests when it finds nothing to read or write.
const LINK_IDLE_WAIT: Duration = Duration::from_millis(10);

/// The most a link's frames may wait to be written, in bytes of JSON. A world that takes its
/// frames more slowly than they come is dropped once this many wait, so it never holds the hub's
/// memory.
pub const LINK_QUEUE_BYTES: usize = 4 * 1024 * 1024;

/// The hub's settings.
#[derive(Debug, Clone)]
pub struct HubConfig {
    pub listen: SocketAddr,
    pub db: String,
    pub domain: String,
    /// The proxies whose forwarded client address is believed: addresses or CIDR ranges.
    pub trusted_proxies: Vec<ProxyRange>,
    /// The header a trusted proxy names the client in.
    pub client_header: ClientHeader,
    /// How many proxies a request crosses before the hub ([`crate::forwarded::client_address`]).
    pub proxy_hops: usize,
    /// The hub's own address as browsers and worlds reach it (`https://hub.dereth.network`): the
    /// operator API's sign-in comes back to it, and a request from it is trusted.
    pub public_url: Option<String>,
    /// The site's origins (`https://dereth.network`), which may call the operator API with the
    /// session; the first is where sign-in returns.
    pub site_origins: Vec<String>,
    /// The Discord application operators sign in with. Without it, and a public URL and a site
    /// origin, the operator API answers 503.
    pub discord: Option<crate::discord::Discord>,
    /// The Discord ids of the operators who may use the admin routes.
    pub admins: Vec<String>,
    /// Worlds approved onto the network a day, for every operator together.
    pub approvals_per_day: usize,
    /// How old a Discord account must be, in days, to register or claim a world.
    pub min_account_age_days: u64,
    /// How long a registered world may stay `pending`, in days, before it is removed.
    pub pending_days: u64,
    /// Whether loopback addresses count as public (for endpoints and verification pages): for
    /// tests and a hub developed on one machine, never a public hub.
    pub loopback_allowed: bool,
    /// How long reading a verification page may take, redirects included.
    pub fetch_timeout: Duration,
    /// Ranges a verification page is never read from, whatever else allows them: the hub's own
    /// public address, its prefix and its local network.
    pub deny_fetch: Vec<ProxyRange>,
    /// How the hub looks host names up ([`crate::resolver`]): the system's resolver unless a test
    /// stands in for it.
    pub lookup: crate::resolver::Lookup,
    /// How long a caller waits for a lookup ([`crate::resolver::DEADLINE`]).
    pub resolve_deadline: Duration,
    /// How long a verification in game waits for its endpoint's lookup
    /// ([`crate::challenge::LOOKUP_DEADLINE`]).
    pub challenge_lookup_deadline: Duration,
    /// The time between two copies of a challenge ([`crate::challenge::EVERY`]).
    pub challenge_every: Duration,
    /// How long a verification in game waits for the world's answer
    /// ([`crate::challenge::WAIT`]).
    pub challenge_wait: Duration,
    pub limits: Limits,
    /// HTTP requests a minute from one address.
    pub http_per_minute: u32,
    /// A file to write one line per link frame to: the time, the direction, the world and the
    /// message's type. Never a body. `None` writes nothing.
    pub wire_log: Option<std::path::PathBuf>,
    /// How long one write to a link may wait ([`LINK_WRITE_TIMEOUT`]).
    pub link_write_timeout: Duration,
    /// The most bytes a link's frames may wait to be written ([`LINK_QUEUE_BYTES`]).
    pub link_queue_bytes: usize,
    /// How long a request's whole body may take ([`BODY_DEADLINE`]).
    pub body_deadline: Duration,
}

impl Default for HubConfig {
    fn default() -> Self {
        Self {
            listen: SocketAddr::from(([127, 0, 0, 1], 8650)),
            db: "hub.sqlite".into(),
            domain: "dereth.network".into(),
            trusted_proxies: Vec::new(),
            client_header: ClientHeader::default(),
            proxy_hops: 1,
            public_url: None,
            site_origins: Vec::new(),
            discord: None,
            admins: Vec::new(),
            approvals_per_day: 500,
            min_account_age_days: 30,
            pending_days: 14,
            loopback_allowed: false,
            fetch_timeout: Duration::from_secs(10),
            deny_fetch: Vec::new(),
            lookup: crate::resolver::Lookup::default(),
            resolve_deadline: crate::resolver::DEADLINE,
            challenge_lookup_deadline: crate::challenge::LOOKUP_DEADLINE,
            challenge_every: crate::challenge::EVERY,
            challenge_wait: crate::challenge::WAIT,
            limits: Limits::DEFAULT,
            http_per_minute: 180,
            wire_log: None,
            link_write_timeout: LINK_WRITE_TIMEOUT,
            link_queue_bytes: LINK_QUEUE_BYTES,
            body_deadline: BODY_DEADLINE,
        }
    }
}

/// How old the public lists' snapshot may be before it is built again.
const PUBLIC_EVERY: Duration = Duration::from_secs(1);

/// How long a cache in front of the hub may keep a public list.
const PUBLIC_CACHE: &str = "public, max-age=10";

/// The public lists, built from the registry at one moment.
struct Public {
    built: Instant,
    /// Every world the public answers show, rendered.
    worlds: Vec<Value>,
    /// The network's totals.
    totals: Value,
}

/// One line on stderr. Never a message body, never a key.
pub fn log(level: &str, msg: &str) {
    let now = crate::registry::iso(std::time::SystemTime::now());
    eprintln!("{now} {level:<5} {msg}");
}

enum Event {
    Linked {
        slug: String,
        name: String,
        /// The digest of the key the link was opened with.
        key: String,
        conn: u64,
        tx: Sender<String>,
        queue: Arc<LinkQueue>,
    },
    Frame {
        slug: String,
        conn: u64,
        text: String,
    },
    Closed {
        slug: String,
        conn: u64,
    },
    /// A person changed on the site: the watched views of these characters, all of the person's,
    /// and those naming one of them, are looked at again.
    PersonsChanged(Vec<dereth_network_protocol::Address>),
}

/// What a link's thread and the routing share besides the frames themselves: how many bytes of
/// frames wait for the thread, whether the routing has asked it to close the link, and whether the
/// routing has dropped the link outright.
struct LinkQueue {
    bytes: AtomicUsize,
    closing: Mutex<Option<&'static str>>,
    cut: AtomicBool,
    socket: TcpStream,
}

impl LinkQueue {
    fn new(socket: TcpStream) -> Self {
        Self {
            bytes: AtomicUsize::new(0),
            closing: Mutex::new(None),
            cut: AtomicBool::new(false),
            socket,
        }
    }

    /// The routing asks the thread to close the link, for the reason given.
    fn close(&self, why: &'static str) {
        if let Ok(mut c) = self.closing.lock() {
            c.get_or_insert(why);
        }
    }

    fn closing(&self) -> Option<&'static str> {
        self.closing.lock().ok().and_then(|c| *c)
    }

    /// Counts `len` more bytes waiting; false, and nothing counted, past `limit`.
    fn offer(&self, len: usize, limit: usize) -> bool {
        let mut now = self.bytes.load(Ordering::SeqCst);
        loop {
            let Some(next) = now.checked_add(len).filter(|n| *n <= limit) else {
                return false;
            };
            match self
                .bytes
                .compare_exchange(now, next, Ordering::SeqCst, Ordering::SeqCst)
            {
                Ok(_) => return true,
                Err(seen) => now = seen,
            }
        }
    }

    /// `len` bytes were taken off to be written.
    fn taken(&self, len: usize) {
        let _ = self
            .bytes
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| {
                Some(n.saturating_sub(len))
            });
    }

    /// The routing dropped the link: the thread ends it without a word.
    fn cut(&self) {
        self.cut.store(true, Ordering::SeqCst);
        let _ = self.socket.shutdown(std::net::Shutdown::Both);
    }

    fn is_cut(&self) -> bool {
        self.cut.load(Ordering::SeqCst)
    }
}

/// A link as the routing holds it.
struct Routed {
    conn: u64,
    tx: Sender<String>,
    queue: Arc<LinkQueue>,
}

pub(crate) struct Shared {
    registry: Mutex<Registry>,
    linked: Mutex<BTreeSet<String>>,
    events: Mutex<Sender<Event>>,
    pub(crate) config: HubConfig,
    /// The key the sign-in cookies are signed with, made at start: a sign-in in progress is its
    /// cookie's, and the hub keeps nothing of it ([`crate::api`]).
    pub(crate) sign_in_key: [u8; 32],
    /// Until when Discord asked the hub to ask it nothing (its `429`), on the registry's clock: no
    /// sign-in calls it before then.
    discord_wait: Mutex<Option<std::time::SystemTime>>,
    /// The states of finished sign-ins, oldest first, so none is used twice.
    spent_states: Mutex<(
        std::collections::VecDeque<String>,
        std::collections::HashSet<String>,
    )>,
    http_buckets: Mutex<HashMap<IpAddr, Bucket>>,
    /// Open connections by the address they are counted under ([`counted_as`]).
    open: Mutex<HashMap<IpAddr, usize>>,
    /// Connections still sending their request's head.
    heads: AtomicUsize,
    /// When a refusal was last logged, by what was refused.
    refusals: Mutex<HashMap<String, Instant>>,
    /// The public lists' snapshot, built at most once a second, never waiting on the registry.
    public: Mutex<Option<Arc<Public>>>,
    /// The hub's name lookups.
    pub(crate) resolver: crate::resolver::Resolver,
    /// The challenges of verifications in game waiting for the world's answer.
    pub(crate) challenges: crate::challenge::Waiting,
    started: Instant,
    next_conn: AtomicU64,
    stop: AtomicBool,
}

impl Shared {
    pub(crate) fn registry(&self) -> MutexGuard<'_, Registry> {
        self.registry
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
    fn send(&self, e: Event) {
        if let Ok(tx) = self.events.lock() {
            let _ = tx.send(e);
        }
    }

    /// A person changed on the site (a character linked or unlinked, the setting, a person friend),
    /// and `characters` are all of its characters, before and after: the routing looks again at
    /// the worlds' views of them, and those naming one of them.
    pub(crate) fn persons_changed(&self, characters: Vec<dereth_network_protocol::Address>) {
        if !characters.is_empty() {
            self.send(Event::PersonsChanged(characters));
        }
    }

    /// Whether Discord's last `429` still asks the hub to wait.
    pub(crate) fn discord_waiting(&self) -> bool {
        let until = *self
            .discord_wait
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        until.is_some_and(|t| self.registry().now() < t)
    }

    /// Discord answered `429`, asking for nothing for `secs` seconds: until then, sign-ins call
    /// it for nothing.
    pub(crate) fn discord_wait(&self, secs: u64) {
        let until = self.registry().now() + Duration::from_secs(secs);
        *self
            .discord_wait
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(until);
        log(
            "info",
            &format!("Discord asked the hub to wait {secs} seconds; sign-ins wait with it"),
        );
    }

    /// Logs a refusal at most once a minute for each `what`, so refusals cost a sender more than
    /// they cost the log.
    fn log_refusal(&self, what: &str, msg: &str) {
        let mut r = self
            .refusals
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if r.len() > 10_000 {
            r.clear();
        }
        let now = Instant::now();
        let due = r
            .get(what)
            .is_none_or(|t| now.duration_since(*t) >= Duration::from_secs(60));
        if due {
            r.insert(what.to_owned(), now);
            drop(r);
            log("info", msg);
        }
    }

    /// Marks a sign-in's state used: false when it already was. At most the last
    /// [`crate::api::SPENT_STATES`] are remembered.
    pub(crate) fn spend_state(&self, state: &str) -> bool {
        let mut s = self
            .spent_states
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let (order, set) = &mut *s;
        if !set.insert(state.to_owned()) {
            return false;
        }
        order.push_back(state.to_owned());
        while order.len() > crate::api::SPENT_STATES {
            if let Some(old) = order.pop_front() {
                set.remove(&old);
            }
        }
        true
    }

    /// The public lists as they were at most a second ago. A request never waits on the registry
    /// for them: while another thread holds it, the last snapshot is served, however old.
    fn public(&self) -> Result<Arc<Public>, RegistryError> {
        let last = self
            .public
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        if let Some(p) = last.as_ref().filter(|p| p.built.elapsed() < PUBLIC_EVERY) {
            return Ok(Arc::clone(p));
        }
        let registry = match self.registry.try_lock() {
            Ok(r) => r,
            Err(std::sync::TryLockError::Poisoned(e)) => e.into_inner(),
            Err(std::sync::TryLockError::WouldBlock) => match last {
                Some(p) => return Ok(p),
                None => self.registry(),
            },
        };
        let worlds = registry.all()?;
        drop(registry);
        let now = std::time::SystemTime::now();
        let fresh = Arc::new(Public {
            built: Instant::now(),
            totals: crate::registry::totals_of(&worlds, now),
            worlds,
        });
        *self
            .public
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(Arc::clone(&fresh));
        Ok(fresh)
    }

    /// Takes one request from `ip`'s allowance (an IPv6 client's by its /64, as connections are
    /// counted); when it is spent, how many seconds until it is not.
    fn allowed(&self, ip: IpAddr) -> Result<(), u64> {
        let per_min = f64::from(self.config.http_per_minute);
        let now = self.started.elapsed().as_secs_f64();
        let mut b = self
            .http_buckets
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if b.len() > 10_000 {
            b.retain(|_, x| !x.full_at(now));
        }
        let bucket = b.entry(counted_as(ip)).or_insert_with(|| {
            Bucket::new(
                Rate {
                    burst: per_min,
                    per_sec: per_min / 60.0,
                },
                now,
            )
        });
        if bucket.take(now) {
            Ok(())
        } else {
            Err(whole_seconds(bucket.wait(now)))
        }
    }
}

/// `secs` rounded up to whole seconds, at least one and at most a day.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn whole_seconds(secs: f64) -> u64 {
    if secs.is_finite() {
        secs.ceil().clamp(1.0, 86_400.0) as u64
    } else {
        86_400
    }
}

/// The address a connection is counted under: an IPv4 address as it is, an IPv6 address by its
/// /64 (a single host is commonly given a whole /64), and an IPv4-mapped one as the IPv4 address.
fn counted_as(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V4(_) => ip,
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => IpAddr::V4(v4),
            None => {
                let s = v6.segments();
                IpAddr::V6(std::net::Ipv6Addr::new(s[0], s[1], s[2], s[3], 0, 0, 0, 0))
            }
        },
    }
}

/// How long a request's head may take while `heads` connections are still sending theirs: the
/// full [`HEAD_TIMEOUT`], or [`HEAD_TIMEOUT_BUSY`] once half the connections are heads, so that
/// heads that never finish cannot hold every slot for long.
fn head_timeout(heads: usize) -> Duration {
    if heads >= MAX_CONNECTIONS / 2 {
        HEAD_TIMEOUT_BUSY
    } else {
        HEAD_TIMEOUT
    }
}

/// One open connection, counted while it lives.
struct Opened {
    shared: Arc<Shared>,
    /// The address it is counted under ([`counted_as`]).
    key: IpAddr,
}

impl Opened {
    /// Counts a connection from `ip`, or refuses it over the limits.
    fn new(shared: &Arc<Shared>, ip: IpAddr) -> Option<Self> {
        let mut open = shared
            .open
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let total: usize = open.values().sum();
        let proxy = crate::forwarded::is_trusted(&shared.config.trusted_proxies, ip);
        let key = if proxy { ip } else { counted_as(ip) };
        let from = open.get(&key).copied().unwrap_or(0);
        if total >= MAX_CONNECTIONS || (!proxy && from >= MAX_CONNECTIONS_PER_ADDRESS) {
            return None;
        }
        *open.entry(key).or_default() += 1;
        Some(Self {
            shared: Arc::clone(shared),
            key,
        })
    }
}

/// A connection still sending its request's head, counted while it does.
struct Heading<'a>(&'a AtomicUsize);

impl<'a> Heading<'a> {
    fn new(heads: &'a AtomicUsize) -> Self {
        heads.fetch_add(1, Ordering::SeqCst);
        Self(heads)
    }
}

impl Drop for Heading<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

impl Drop for Opened {
    fn drop(&mut self) {
        let mut open = self
            .shared
            .open
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(n) = open.get_mut(&self.key) {
            *n = n.saturating_sub(1);
            if *n == 0 {
                open.remove(&self.key);
            }
        }
    }
}

/// A running hub.
pub struct Running {
    addr: SocketAddr,
    shared: Arc<Shared>,
    threads: Vec<JoinHandle<()>>,
}

impl std::fmt::Debug for Running {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Running")
            .field("addr", &self.addr)
            .finish_non_exhaustive()
    }
}

impl Running {
    /// The address it listens on.
    #[must_use]
    pub fn local_addr(&self) -> SocketAddr {
        self.addr
    }

    /// The registry, for an operator's command or a test.
    pub fn registry(&self) -> MutexGuard<'_, Registry> {
        self.shared.registry()
    }

    /// How many connections are open now.
    #[must_use]
    pub fn open_connections(&self) -> usize {
        self.shared
            .open
            .lock()
            .map(|o| o.values().sum())
            .unwrap_or_default()
    }

    /// The worlds with a link up.
    #[must_use]
    pub fn linked(&self) -> BTreeSet<String> {
        self.shared
            .linked
            .lock()
            .map(|l| l.clone())
            .unwrap_or_default()
    }

    /// Stops listening, closes every link and waits for the threads.
    pub fn shutdown(mut self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }
}

/// Opens the registry at `config.db` and starts the hub.
///
/// # Errors
/// When the registry cannot be opened or the address cannot be bound.
pub fn start(config: HubConfig) -> io::Result<Running> {
    let registry = Registry::open(
        &config.db,
        &config.domain,
        Box::new(std::time::SystemTime::now),
    )
    .map_err(|e| io::Error::other(e.to_string()))?;
    start_with(config, registry)
}

/// Starts the hub over a registry already open.
///
/// # Errors
/// When the address cannot be bound.
pub fn start_with(config: HubConfig, mut registry: Registry) -> io::Result<Running> {
    let channels = load_channels(&registry)?;
    registry.policy = crate::operators::Policy {
        approvals_per_day: config.approvals_per_day,
        loopback_allowed: config.loopback_allowed,
        min_account_age_days: config.min_account_age_days,
        pending_days: config.pending_days,
    };
    registry.admins.clone_from(&config.admins);
    registry.set_web(config.public_url.as_deref(), &config.site_origins);
    let listener = TcpListener::bind(config.listen)?;
    listener.set_nonblocking(true)?;
    let addr = listener.local_addr()?;
    let (tx, rx) = mpsc::channel();
    let limits = config.limits;
    let resolver = crate::resolver::Resolver::new(
        config.lookup.clone(),
        config.resolve_deadline,
        crate::resolver::WORKERS,
        crate::resolver::REMEMBER,
    );
    let shared = Arc::new(Shared {
        registry: Mutex::new(registry),
        linked: Mutex::new(BTreeSet::new()),
        events: Mutex::new(tx),
        config,
        sign_in_key: crate::registry::random_bytes::<32>(),
        discord_wait: Mutex::new(None),
        spent_states: Mutex::new(Default::default()),
        http_buckets: Mutex::new(HashMap::new()),
        open: Mutex::new(HashMap::new()),
        heads: AtomicUsize::new(0),
        refusals: Mutex::new(HashMap::new()),
        public: Mutex::new(None),
        resolver,
        challenges: crate::challenge::Waiting::default(),
        started: Instant::now(),
        next_conn: AtomicU64::new(1),
        stop: AtomicBool::new(false),
    });
    let mut hub = Hub::new(SOFTWARE, channels, limits);
    hub.set_link_url(site_page(&shared.config, "characters"));
    hub.set_person_url(site_page(&shared.config, "characters"));
    let s1 = Arc::clone(&shared);
    let router = std::thread::Builder::new()
        .name("hub-router".into())
        .spawn(move || route(&s1, hub, &rx))?;
    let s2 = Arc::clone(&shared);
    let accept = std::thread::Builder::new()
        .name("hub-accept".into())
        .spawn(move || accept_loop(&s2, &listener))?;
    log(
        "info",
        &format!("listening on http://{addr} (links at ws://{addr}/v1/servers/<slug>/link)"),
    );
    Ok(Running {
        addr,
        shared,
        threads: vec![accept, router],
    })
}

/// A page of the site for persons, on a hub whose sign-in is configured (none otherwise): the first
/// site origin's `/characters/`, where a link code is entered and a person shows or hides its
/// characters.
fn site_page(config: &HubConfig, page: &str) -> Option<String> {
    let site = config.site_origins.first()?;
    (config.discord.is_some() && config.public_url.is_some())
        .then(|| format!("{}/{page}/", site.trim_end_matches('/')))
}

/// The channels as saved, one row each.
fn load_channels(registry: &Registry) -> io::Result<Channels> {
    let mut channels = Channels::default();
    let rows = registry
        .channels()
        .map_err(|e| io::Error::other(e.to_string()))?;
    for (name, value) in rows {
        match serde_json::from_str(&value) {
            Ok(c) => {
                channels.channels.insert(name, c);
            }
            Err(e) => log("error", &format!("the saved channel {name:?}: {e}")),
        }
    }
    Ok(channels)
}

/// Writes the channels changed since the last save.
fn save_channels(shared: &Shared, hub: &mut Hub) {
    let changed = hub.take_changed_channels();
    if changed.is_empty() {
        return;
    }
    let rows: Vec<(String, Option<String>)> = changed
        .into_iter()
        .map(|name| {
            let json = hub
                .channels
                .channels
                .get(&name)
                .and_then(|c| serde_json::to_string(c).ok());
            (name, json)
        })
        .collect();
    let n = rows.len();
    if let Err(e) = shared.registry().set_channels(&rows) {
        log("error", &format!("saving {n} changed channel(s): {e}"));
    }
}

fn accept_loop(shared: &Arc<Shared>, listener: &TcpListener) {
    while !shared.stop.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, peer)) => {
                let Some(opened) = Opened::new(shared, peer.ip()) else {
                    shared.log_refusal(
                        &format!("connections from {}", peer.ip()),
                        &format!("refused a connection from {}: too many open", peer.ip()),
                    );
                    drop(stream);
                    continue;
                };
                let s = Arc::clone(shared);
                let _ = std::thread::Builder::new()
                    .name("hub-conn".into())
                    .spawn(move || {
                        let _opened = opened;
                        if let Err(e) = connection(&s, stream, peer) {
                            if e.kind() != io::ErrorKind::UnexpectedEof {
                                log("debug", &format!("connection from {peer}: {e}"));
                            }
                        }
                    });
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(e) => {
                log("warn", &format!("accept: {e}"));
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }
}

/// The routing thread: every link's frames in, every link's frames out, and the clock.
fn route(shared: &Arc<Shared>, mut hub: Hub, rx: &Receiver<Event>) {
    let mut links: BTreeMap<String, Routed> = BTreeMap::new();
    let mut wire = shared.config.wire_log.as_ref().and_then(|p| {
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(p)
            .map_err(|e| log("error", &format!("the wire log {}: {e}", p.display())))
            .ok()
    });
    let mut next_tick = Instant::now();
    let mut save_at: Option<Instant> = None;
    loop {
        if shared.stop.load(Ordering::SeqCst) {
            for l in links.into_values() {
                l.queue.close("hub stopping");
            }
            save_channels(shared, &mut hub);
            return;
        }
        let now = shared.started.elapsed().as_secs_f64();
        let outs = match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(Event::Linked {
                slug,
                name,
                key,
                conn,
                tx,
                queue,
            }) => {
                log("info", &format!("link up: {slug}"));
                let out = hub.connect_keyed(&slug, &name, &key);
                // A replaced link is told to close by the outs; the new one takes its place after.
                let mut out = out;
                apply(shared, &mut hub, &mut links, &mut out);
                links.insert(slug.clone(), Routed { conn, tx, queue });
                set_linked(shared, &links);
                Vec::new()
            }
            Ok(Event::Frame { slug, conn, text }) => {
                if links.get(&slug).is_some_and(|l| l.conn == conn) {
                    match Message::from_json(&text) {
                        Ok(m) => {
                            if let Some(f) = wire.as_mut() {
                                wire_line(f, &format!("{slug} -> hub"), m.kind());
                            }
                            let reg = shared.registry();
                            hub.receive(&*reg, &slug, m, now)
                        }
                        Err(WireError::Unknown(t)) => {
                            log(
                                "debug",
                                &format!("{slug}: skipped a message of unknown type {t:?}"),
                            );
                            Vec::new()
                        }
                        Err(WireError::Malformed(_)) => vec![Out::Send(
                            slug.clone(),
                            Message::Error {
                                code: "malformed".into(),
                                message: "a frame that is not a message".into(),
                            },
                        )],
                    }
                } else {
                    Vec::new()
                }
            }
            Ok(Event::PersonsChanged(characters)) => {
                hub.persons_changed(&characters);
                Vec::new()
            }
            Ok(Event::Closed { slug, conn }) => {
                if links.get(&slug).is_some_and(|l| l.conn == conn) {
                    links.remove(&slug);
                    set_linked(shared, &links);
                    log("info", &format!("link down: {slug}"));
                    hub.disconnect(&slug)
                } else {
                    Vec::new()
                }
            }
            Err(RecvTimeoutError::Timeout) => Vec::new(),
            Err(RecvTimeoutError::Disconnected) => return,
        };
        let mut outs = outs;
        if Instant::now() >= next_tick {
            next_tick = Instant::now() + Duration::from_millis(500);
            let reg = shared.registry();
            outs.extend(hub.tick(&*reg, now));
        }
        if let Some(f) = wire.as_mut() {
            for o in &outs {
                if let Out::Send(slug, m) = o {
                    if links.contains_key(slug) {
                        wire_line(f, &format!("hub -> {slug}"), m.kind());
                    }
                }
            }
        }
        // The channels that changed are saved a moment later, together.
        if outs.iter().any(|o| matches!(o, Out::ChannelsChanged)) && save_at.is_none() {
            save_at = Some(Instant::now() + SAVE_CHANNELS_EVERY);
        }
        apply(shared, &mut hub, &mut links, &mut outs);
        if save_at.is_some_and(|t| Instant::now() >= t) {
            save_at = None;
            save_channels(shared, &mut hub);
        }
    }
}

fn wire_line(f: &mut std::fs::File, way: &str, kind: &str) {
    let now = crate::registry::iso(std::time::SystemTime::now());
    let _ = writeln!(f, "{now} {way}: {kind}");
}

fn set_linked(shared: &Shared, links: &BTreeMap<String, Routed>) {
    if let Ok(mut l) = shared.linked.lock() {
        *l = links.keys().cloned().collect();
    }
}

/// Does what the routing returned. A link whose waiting frames would pass
/// [`HubConfig::link_queue_bytes`] is dropped there and then: its socket is shut, and the hub
/// treats it as a link gone down (the other worlds are told it is offline).
fn apply(
    shared: &Shared,
    hub: &mut Hub,
    links: &mut BTreeMap<String, Routed>,
    outs: &mut Vec<Out>,
) {
    let mut work: std::collections::VecDeque<Out> = outs.drain(..).collect();
    while let Some(o) = work.pop_front() {
        match o {
            Out::Send(slug, m) => {
                let Some(l) = links.get(&slug) else {
                    continue;
                };
                let frame = m.to_json();
                if l.queue.offer(frame.len(), shared.config.link_queue_bytes) {
                    let _ = l.tx.send(frame);
                } else if let Some(l) = links.remove(&slug) {
                    log(
                        "info",
                        &format!("dropping {slug}'s link: it is not taking its frames"),
                    );
                    l.queue.cut();
                    set_linked(shared, links);
                    work.extend(hub.disconnect(&slug));
                }
            }
            Out::Close(slug, why) => {
                if let Some(l) = links.remove(&slug) {
                    log("info", &format!("closing {slug}'s link: {why}"));
                    l.queue.close(why);
                }
                set_linked(shared, links);
            }
            Out::Heartbeat {
                slug,
                state,
                players_online,
            } => {
                let h = Heartbeat {
                    state,
                    players_online,
                    message: String::new(),
                    players_kind: "connected-characters".into(),
                    emulator_version: None,
                    uptime_seconds: None,
                    adapter: None,
                };
                if let Err(e) = shared.registry().heartbeat(&slug, &h, None) {
                    log(
                        "debug",
                        &format!("{slug}: link heartbeat refused: {}", e.message),
                    );
                }
            }
            Out::ChannelsChanged => {}
        }
    }
}

/// A request's head, peeked (not consumed).
struct Head {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    len: usize,
}

impl Head {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

fn peek_head(shared: &Shared, stream: &TcpStream) -> io::Result<Head> {
    let _heading = Heading::new(&shared.heads);
    let started = Instant::now();
    let mut buf = vec![0u8; 16 * 1024];
    stream.set_read_timeout(Some(Duration::from_millis(200)))?;
    loop {
        let n = match stream.peek(&mut buf) {
            Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
            Ok(n) => n,
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                0
            }
            Err(e) => return Err(e),
        };
        if n > 0 {
            let mut headers = [httparse::EMPTY_HEADER; 64];
            let mut req = httparse::Request::new(&mut headers);
            match req.parse(&buf[..n]) {
                Ok(httparse::Status::Complete(len)) => {
                    return Ok(Head {
                        method: req.method.unwrap_or("").to_owned(),
                        path: req.path.unwrap_or("").to_owned(),
                        headers: req
                            .headers
                            .iter()
                            .map(|h| {
                                (
                                    h.name.to_owned(),
                                    String::from_utf8_lossy(h.value).into_owned(),
                                )
                            })
                            .collect(),
                        len,
                    })
                }
                Ok(httparse::Status::Partial) if n == buf.len() => {
                    return Err(io::Error::new(io::ErrorKind::InvalidData, "head too long"))
                }
                Ok(httparse::Status::Partial) => {}
                Err(e) => return Err(io::Error::new(io::ErrorKind::InvalidData, e.to_string())),
            }
        }
        if started.elapsed() > head_timeout(shared.heads.load(Ordering::SeqCst)) {
            return Err(io::ErrorKind::TimedOut.into());
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// An error answer to a public route's request, as [`respond`] writes one.
fn plain_error(status: u16, message: &str) -> crate::api::Reply {
    crate::api::Reply {
        status,
        body: error_json(status, message),
        kind: "application/json; charset=utf-8",
        headers: Vec::new(),
        cors: crate::api::Cors::Any,
    }
}

/// Answers a request refused before its body was read, then takes in what the client is still
/// sending (a bounded amount, briefly) before closing: closing on unread data resets the
/// connection, and a reset can lose the answer on its way.
fn refuse(stream: &mut TcpStream, reply: &crate::api::Reply) -> io::Result<()> {
    write_reply(stream, reply)?;
    drop(stream.shutdown(std::net::Shutdown::Write));
    let started = Instant::now();
    let mut left: usize = 64 * 1024;
    let mut buf = [0u8; 8 * 1024];
    while left > 0 && started.elapsed() < Duration::from_secs(1) {
        if stream
            .set_read_timeout(Some(Duration::from_millis(250)))
            .is_err()
        {
            break;
        }
        match stream.read(&mut buf) {
            Ok(n) if n > 0 => left = left.saturating_sub(n),
            _ => break,
        }
    }
    Ok(())
}

/// A request's body of `len` bytes, read within `deadline` in all; none when it does not arrive
/// in time, however it trickles.
fn read_body(
    stream: &mut TcpStream,
    len: usize,
    deadline: Duration,
) -> io::Result<Option<Vec<u8>>> {
    let started = Instant::now();
    let mut body = vec![0u8; len];
    let mut got = 0;
    while got < len {
        let Some(left) = deadline
            .checked_sub(started.elapsed())
            .filter(|d| !d.is_zero())
        else {
            return Ok(None);
        };
        stream.set_read_timeout(Some(left))?;
        match stream.read(&mut body[got..]) {
            Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
            Ok(n) => got += n,
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                return Ok(None)
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(Some(body))
}

fn link_slug(path: &str) -> Option<&str> {
    path.split('?')
        .next()?
        .strip_prefix("/v1/servers/")?
        .strip_suffix("/link")
        .filter(|s| !s.contains('/'))
}

fn bearer(head: &Head) -> Option<String> {
    let v = head.header("authorization")?;
    let (scheme, key) = v.split_once(' ')?;
    (scheme.eq_ignore_ascii_case("bearer") && crate::registry::is_key_shape(key.trim()))
        .then(|| key.trim().to_owned())
}

/// A request's client address; none for a forwarded header from a trusted proxy that does not
/// read, which is refused.
fn client_ip(shared: &Shared, peer: SocketAddr, head: &Head) -> Option<IpAddr> {
    let name = match shared.config.client_header {
        ClientHeader::XForwardedFor => "x-forwarded-for",
        ClientHeader::CfConnectingIp => "cf-connecting-ip",
    };
    // Every line of the header, joined as one list.
    let values: Vec<&str> = head
        .headers
        .iter()
        .filter(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
        .collect();
    let joined = (!values.is_empty()).then(|| values.join(","));
    crate::forwarded::client_address(
        peer.ip(),
        &shared.config.trusted_proxies,
        shared.config.client_header,
        shared.config.proxy_hops,
        joined.as_deref(),
    )
    .ok()
}

fn connection(shared: &Arc<Shared>, mut stream: TcpStream, peer: SocketAddr) -> io::Result<()> {
    stream.set_nonblocking(false)?;
    let head = peek_head(shared, &stream)?;
    if let Some(slug) = link_slug(&head.path).map(str::to_ascii_lowercase) {
        if head
            .header("upgrade")
            .is_some_and(|u| u.eq_ignore_ascii_case("websocket"))
        {
            // A link attempt takes from the same allowance as a request, before its key is read.
            let Some(ip) = client_ip(shared, peer, &head) else {
                consume(&mut stream, head.len)?;
                return respond(
                    &mut stream,
                    400,
                    &error_json(400, "The forwarded client address does not read"),
                );
            };
            if let Err(wait) = shared.allowed(ip) {
                consume(&mut stream, head.len)?;
                return write_reply(&mut stream, &too_many(wait, crate::api::Cors::Nothing));
            }
            return link(shared, stream, &head, &slug, peer);
        }
    }
    // Plain HTTP: consume the head we peeked; the allowance is taken before the body is read.
    let mut discard = vec![0u8; head.len];
    stream.set_read_timeout(Some(HEAD_TIMEOUT))?;
    stream.read_exact(&mut discard)?;
    let Some(ip) = client_ip(shared, peer, &head) else {
        return refuse(
            &mut stream,
            &plain_error(400, "The forwarded client address does not read"),
        );
    };
    if head.path != "/healthz" {
        if let Err(wait) = shared.allowed(ip) {
            // An operator route's answer is readable by the site that asked, as any of its others.
            let cors = if crate::api::is_api(&head.path) {
                crate::api::cors_for(shared, head.header("origin"))
            } else {
                crate::api::Cors::Any
            };
            return refuse(&mut stream, &too_many(wait, cors));
        }
    }
    // A body comes with its length: a chunked one is refused, and the connection closed with it.
    if head.header("transfer-encoding").is_some() {
        return refuse(
            &mut stream,
            &plain_error(411, "Send the body with a Content-Length, not chunked"),
        );
    }
    let body_len: usize = head
        .header("content-length")
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(0);
    if body_len > 512 * 1024 {
        return refuse(&mut stream, &plain_error(413, "Request body too large"));
    }
    let Some(body) = read_body(&mut stream, body_len, shared.config.body_deadline)? else {
        return refuse(
            &mut stream,
            &plain_error(408, "The request's body took too long"),
        );
    };
    if crate::api::is_api(&head.path) {
        let req = crate::api::Req {
            method: &head.method,
            path: &head.path,
            headers: &head.headers,
            body: &body,
            ip,
        };
        let reply = crate::api::handle(shared, &req);
        return write_reply(&mut stream, &reply);
    }
    let (status, body, kind) = http(shared, &head, &body, ip);
    // The public lists may be kept a moment by a cache in front of the hub.
    let path = head.path.split('?').next().unwrap_or("");
    let listed = status == 200
        && matches!(
            path,
            "/v1/servers"
                | "/v1/status"
                | "/v1/network"
                | "/compat/thwarg/Servers.xml"
                | "/compat/treestats/player_counts-latest.json"
        );
    write_reply(
        &mut stream,
        &crate::api::Reply {
            status,
            body,
            kind,
            headers: if listed {
                vec![("Cache-Control".into(), PUBLIC_CACHE.into())]
            } else {
                Vec::new()
            },
            cors: crate::api::Cors::Any,
        },
    )
}

/// A request over the client's allowance, and when to try again.
fn too_many(wait: u64, cors: crate::api::Cors) -> crate::api::Reply {
    crate::api::Reply {
        status: 429,
        body: json!({ "error": { "code": crate::api::code(429), "message": "Too many requests",
                                 "retryAfterSeconds": wait } })
        .to_string(),
        kind: "application/json; charset=utf-8",
        headers: vec![("Retry-After".into(), wait.to_string())],
        cors,
    }
}

fn error_json(status: u16, message: &str) -> String {
    json!({ "error": { "code": crate::api::code(status), "message": message } }).to_string()
}

fn respond(stream: &mut TcpStream, status: u16, body: &str) -> io::Result<()> {
    respond_as(stream, status, body, "application/json; charset=utf-8")
}

/// A public route's answer: anyone's page may read it.
fn respond_as(
    stream: &mut TcpStream,
    status: u16,
    body: &str,
    kind: &'static str,
) -> io::Result<()> {
    write_reply(
        stream,
        &crate::api::Reply {
            status,
            body: body.to_owned(),
            kind,
            headers: Vec::new(),
            cors: crate::api::Cors::Any,
        },
    )
}

fn write_reply(stream: &mut TcpStream, r: &crate::api::Reply) -> io::Result<()> {
    let reason = match r.status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        302 => "Found",
        303 => "See Other",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        411 => "Length Required",
        409 => "Conflict",
        413 => "Payload Too Large",
        422 => "Unprocessable Content",
        429 => "Too Many Requests",
        503 => "Service Unavailable",
        _ => "Internal Server Error",
    };
    let mut head = format!(
        "HTTP/1.1 {} {reason}\r\nContent-Type: {}\r\nContent-Length: {}\r\n\
         X-Content-Type-Options: nosniff\r\nX-Frame-Options: DENY\r\n\
         Referrer-Policy: strict-origin-when-cross-origin\r\n",
        r.status,
        r.kind,
        r.body.len()
    );
    // Nothing is kept by a cache unless the answer says it may be.
    if !r
        .headers
        .iter()
        .any(|(k, _)| k.eq_ignore_ascii_case("cache-control"))
    {
        head.push_str("Cache-Control: no-store\r\n");
    }
    match &r.cors {
        crate::api::Cors::Any => head.push_str(
            "Access-Control-Allow-Origin: *\r\nAccess-Control-Expose-Headers: Retry-After\r\n",
        ),
        crate::api::Cors::Credentials(origin) => head.push_str(&format!(
            "Access-Control-Allow-Origin: {origin}\r\nAccess-Control-Allow-Credentials: true\r\n\
             Access-Control-Expose-Headers: Retry-After\r\nVary: Origin\r\n"
        )),
        crate::api::Cors::Nothing => head.push_str("Vary: Origin\r\n"),
    }
    for (k, v) in &r.headers {
        // A header value never carries a line break (each is the hub's own).
        if !v.contains(['\r', '\n']) {
            head.push_str(&format!("{k}: {v}\r\n"));
        }
    }
    head.push_str("Connection: close\r\n\r\n");
    stream.write_all(head.as_bytes())?;
    stream.write_all(r.body.as_bytes())?;
    stream.flush()
}

pub(crate) fn query(path: &str) -> HashMap<String, String> {
    path.split_once('?')
        .map(|(_, q)| {
            q.split('&')
                .filter_map(|kv| kv.split_once('=').or(Some((kv, ""))))
                .map(|(k, v)| (k.to_owned(), percent_decode(v)))
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    let hex = |j: usize| b.get(j).and_then(|&c| char::from(c).to_digit(16));
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' => match (hex(i + 1), hex(i + 2)) {
                (Some(h), Some(l)) => {
                    out.push(u8::try_from(h * 16 + l).unwrap_or(b'?'));
                    i += 2;
                }
                _ => out.push(b'%'),
            },
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn reg_result(r: Result<Value, RegistryError>) -> (u16, String) {
    match r {
        Ok(v) => (200, v.to_string()),
        Err(e) => {
            let msg = if e.status >= 500 {
                "An internal error occurred"
            } else {
                e.message.as_str()
            };
            if e.status >= 500 {
                log("error", &e.message);
            }
            (e.status, error_json(e.status, msg))
        }
    }
}

fn http(shared: &Shared, head: &Head, body: &[u8], ip: IpAddr) -> (u16, String, &'static str) {
    const JSON: &str = "application/json; charset=utf-8";
    let path = head.path.split('?').next().unwrap_or("");
    let segs: Vec<&str> = path.trim_matches('/').split('/').collect();
    let get = head.method == "GET" || head.method == "HEAD";
    let (status, body) = match (head.method.as_str(), segs.as_slice()) {
        (_, ["healthz"]) if get => (200, json!({ "status": "ok", "hub": SOFTWARE }).to_string()),
        (_, ["v1", "config"]) if get => (
            200,
            json!({
                "baseDomain": shared.config.domain,
                "authEnabled": shared.config.discord.is_some()
                    && shared.config.public_url.is_some()
                    && !shared.config.site_origins.is_empty(),
                "signIn": "/v1/auth/discord",
                "heartbeatTtlSeconds": crate::registry::HEARTBEAT_TTL_SECS,
            })
            .to_string(),
        ),
        (_, ["v1", "servers"]) if get => {
            let q = query(&head.path);
            let limit = q
                .get("limit")
                .and_then(|v| v.parse().ok())
                .unwrap_or(50usize)
                .clamp(1, 100);
            let offset = q
                .get("offset")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0usize)
                .min(1_000_000);
            reg_result(shared.public().map(|p| {
                crate::registry::page_of(
                    &p.worlds,
                    q.get("q").map_or("", String::as_str),
                    q.get("ruleset").map_or("", String::as_str),
                    q.get("federation").map_or("", String::as_str),
                    limit,
                    offset,
                )
            }))
        }
        (_, ["v1", "status"]) if get => reg_result(shared.public().map(|p| p.totals.clone())),
        (_, ["v1", "network"]) if get => {
            let linked = shared.linked.lock().map(|l| l.clone()).unwrap_or_default();
            let all = shared.public().map(|p| p.worlds.clone());
            reg_result(all.map(|servers| {
                let worlds: Vec<Value> = servers
                    .iter()
                    .filter(|s| s["federation"] == Federation::On.word())
                    .map(|s| {
                        let slug = s["slug"].as_str().unwrap_or("");
                        json!({ "slug": slug, "name": s["name"], "online": linked.contains(slug) })
                    })
                    .collect();
                json!({ "hub": SOFTWARE, "worlds": worlds })
            }))
        }
        (_, ["v1", "resolve", identity]) if get => {
            let identity = identity.to_ascii_lowercase();
            let suffix = format!(".{}", shared.config.domain);
            let slug = identity.trim_end_matches('.');
            let slug = slug.strip_suffix(&suffix).unwrap_or(slug).to_owned();
            let server = shared.registry().public_server(&slug);
            match server {
                Ok(Some(s)) => resolve(s, &shared.resolver, shared.config.loopback_allowed),
                Ok(None) => (404, error_json(404, "Server not found")),
                Err(e) => reg_result(Err(e)),
            }
        }
        (_, ["v1", "servers", slug]) if get => match shared.registry().public_server(slug) {
            Ok(Some(s)) => (200, s.to_string()),
            Ok(None) => (404, error_json(404, "Server not found")),
            Err(e) => reg_result(Err(e)),
        },
        (_, ["v1", "servers", slug, "status"]) if get => {
            match shared.registry().public_server(slug) {
                Ok(Some(s)) => (
                    200,
                    json!({ "id": s["id"], "slug": s["slug"], "status": s["status"] }).to_string(),
                ),
                Ok(None) => (404, error_json(404, "Server not found")),
                Err(e) => reg_result(Err(e)),
            }
        }
        ("POST", ["v1", "servers", slug, "heartbeat"]) => {
            let reg = shared.registry();
            match bearer(head) {
                Some(key) if reg.authenticate(slug, &key) => {
                    match serde_json::from_slice::<Heartbeat>(body) {
                        Ok(h) => reg_result(
                            reg.heartbeat(slug, &h, Some(&ip.to_string()))
                                .map(|status| json!({ "status": status })),
                        ),
                        Err(e) => (400, error_json(400, &e.to_string())),
                    }
                }
                _ => (
                    401,
                    error_json(401, "A valid key for this server is required"),
                ),
            }
        }
        (_, ["compat", "thwarg", "Servers.xml"]) if get => {
            return match shared.public() {
                Ok(p) => (
                    200,
                    crate::community::thwarg_xml(&p.worlds),
                    "application/xml; charset=utf-8",
                ),
                Err(e) => {
                    let (s, b) = reg_result(Err(e));
                    (s, b, JSON)
                }
            };
        }
        (_, ["compat", "treestats", "player_counts-latest.json"]) if get => {
            reg_result(shared.public().map(|p| {
                crate::community::treestats_counts(&p.worlds, std::time::SystemTime::now())
            }))
        }
        _ => (404, error_json(404, "Route not found")),
    };
    (status, body, JSON)
}

/// A world with its endpoint's addresses: a host name's as the hub already knows them, never
/// waiting on a lookup (one is started for the next caller when it does not), and only those on
/// the public internet.
fn resolve(
    mut s: Value,
    resolver: &crate::resolver::Resolver,
    loopback_allowed: bool,
) -> (u16, String) {
    let Some(configured) = s["endpoint"]["address"].as_str().map(str::to_owned) else {
        return (
            503,
            json!({ "error": { "code": "ENDPOINT_UNAVAILABLE", "message": "No game endpoint has been reported yet" } })
                .to_string(),
        );
    };
    let port = s["endpoint"]["port"].as_u64().unwrap_or(0);
    let (addresses, source): (Vec<Value>, &str) = if let Ok(ip) = configured.parse::<IpAddr>() {
        (
            vec![json!({ "address": ip.to_string(), "family": if ip.is_ipv4() { 4 } else { 6 } })],
            "literal",
        )
    } else {
        match resolver.known(&configured) {
            crate::resolver::Resolved::Found(found) => (
                found
                    .iter()
                    .filter(|a| {
                        crate::operators::Policy {
                            loopback_allowed,
                            ..crate::operators::Policy::default()
                        }
                        .allows(crate::forwarded::canonical(**a))
                    })
                    .map(|a| json!({ "address": a.to_string(), "family": if a.is_ipv4() { 4 } else { 6 } }))
                    .collect(),
                "dns",
            ),
            crate::resolver::Resolved::Unknown => {
                s["resolution"] = json!({
                    "configuredAddress": configured, "addresses": [], "source": "unknown",
                    "port": port,
                    "resolvedAt": Value::Null,
                    "refreshAfterSeconds": 2, "staleIfErrorSeconds": 0,
                });
                return (200, s.to_string());
            }
        }
    };
    let Some(first) = addresses
        .first()
        .and_then(|a| a["address"].as_str())
        .map(str::to_owned)
    else {
        return (
            503,
            json!({ "error": { "code": "DNS_RESOLUTION_FAILED", "message": "The configured game hostname could not be resolved" } })
                .to_string(),
        );
    };
    s["ip"] = json!(first);
    s["endpoint"]["address"] = json!(first);
    s["resolution"] = json!({
        "configuredAddress": configured, "addresses": addresses, "source": source,
        "port": port,
        "resolvedAt": crate::registry::iso(std::time::SystemTime::now()),
        "refreshAfterSeconds": 60, "staleIfErrorSeconds": 300,
    });
    (200, s.to_string())
}

/// A world's link: checked before the upgrade, then carried both ways until either side closes
/// it or it falls silent.
// The handshake's callback answers with tungstenite's own response type, which is large.
#[allow(clippy::result_large_err)]
fn link(
    shared: &Arc<Shared>,
    mut stream: TcpStream,
    head: &Head,
    slug: &str,
    peer: SocketAddr,
) -> io::Result<()> {
    let (ok, name, key) = {
        let reg = shared.registry();
        let key = bearer(head).filter(|k| reg.authenticate(slug, k));
        let Some(key) = key else {
            drop(reg);
            shared.log_refusal(
                &format!("link {slug}"),
                &format!("link refused for {slug} from {peer}: bad key (logged once a minute)"),
            );
            consume(&mut stream, head.len)?;
            return respond(
                &mut stream,
                401,
                &error_json(401, "A valid key for this server is required"),
            );
        };
        let key = crate::registry::key_digest(&key);
        match reg.federation(slug) {
            Ok(Some((name, f))) if f.links() => (true, name, key),
            _ => (false, String::new(), key),
        }
    };
    if !ok {
        shared.log_refusal(
            &format!("link {slug}"),
            &format!("link refused for {slug}: federation is not on (logged once a minute)"),
        );
        consume(&mut stream, head.len)?;
        return respond(
            &mut stream,
            403,
            &error_json(403, "This world's federation flag is not on"),
        );
    }
    stream.set_read_timeout(Some(HEAD_TIMEOUT))?;
    let config = tungstenite::protocol::WebSocketConfig::default()
        .max_message_size(Some(MAX_MESSAGE_BYTES))
        .max_frame_size(Some(MAX_MESSAGE_BYTES));
    let mut ws =
        tungstenite::accept_hdr_with_config(stream, |_: &Request, r: Response| Ok(r), Some(config))
            .map_err(|e| io::Error::other(e.to_string()))?;
    // The thread never waits on the socket: a world that stops taking its frames holds no write,
    // and is gone once one has waited the write timeout.
    ws.get_mut().set_nonblocking(true)?;
    let queue = Arc::new(LinkQueue::new(ws.get_ref().try_clone()?));
    let conn = shared.next_conn.fetch_add(1, Ordering::SeqCst);
    let (tx, rx) = mpsc::channel();
    shared.send(Event::Linked {
        slug: slug.to_owned(),
        name,
        key,
        conn,
        tx,
        queue: Arc::clone(&queue),
    });
    let mut last_heard = Instant::now();
    let mut next_ping = Instant::now() + PING_EVERY;
    // Since when the world has not taken what was written to it.
    let mut stalled: Option<Instant> = None;
    let blocked = |e: &tungstenite::Error| matches!(e, tungstenite::Error::Io(e) if e.kind() == io::ErrorKind::WouldBlock);
    let why = loop {
        if queue.is_cut() {
            break "dropped: it was not taking its frames";
        }
        let closing = if shared.stop.load(Ordering::SeqCst) {
            Some("hub stopping")
        } else {
            queue.closing()
        };
        if let Some(why) = closing {
            // What the routing sent before it asked for the close goes first (an error saying
            // why, say), as far as the world takes it.
            while let Ok(t) = rx.try_recv() {
                queue.taken(t.len());
                let _ = ws.write(tungstenite::Message::Text(t.into()));
            }
            let _ = ws.close(None);
            let _ = ws.flush();
            break why;
        }
        let mut busy = false;
        match ws.read() {
            Ok(tungstenite::Message::Text(t)) => {
                busy = true;
                last_heard = Instant::now();
                shared.send(Event::Frame {
                    slug: slug.to_owned(),
                    conn,
                    text: t.to_string(),
                });
            }
            Ok(tungstenite::Message::Close(_)) => break "closed by the world",
            Ok(tungstenite::Message::Binary(_)) => break "a binary frame",
            Ok(_) => {
                busy = true;
                last_heard = Instant::now();
            }
            Err(e) if blocked(&e) => {}
            Err(_) => break "the connection failed",
        }
        // Frames are taken off the queue only while the world takes them; while it does not, they
        // wait there, where the routing counts them against the bound.
        let mut failed = false;
        while stalled.is_none() {
            let Ok(t) = rx.try_recv() else {
                break;
            };
            busy = true;
            queue.taken(t.len());
            match ws.write(tungstenite::Message::Text(t.into())) {
                Ok(()) => {}
                Err(e) if blocked(&e) => stalled = Some(Instant::now()),
                Err(_) => {
                    failed = true;
                    break;
                }
            }
        }
        if failed {
            break "write failed";
        }
        if Instant::now() >= next_ping {
            next_ping = Instant::now() + PING_EVERY;
            let _ = ws.write(tungstenite::Message::Ping(Vec::new().into()));
        }
        match ws.flush() {
            Ok(()) => stalled = None,
            Err(e) if blocked(&e) => {
                let since = *stalled.get_or_insert_with(Instant::now);
                if since.elapsed() > shared.config.link_write_timeout {
                    break "it stopped taking its frames";
                }
            }
            Err(_) => break "flush failed",
        }
        if last_heard.elapsed() > IDLE_AFTER {
            let _ = ws.close(None);
            let _ = ws.flush();
            break "silent too long";
        }
        if !busy {
            std::thread::sleep(LINK_IDLE_WAIT);
        }
    };
    let _ = queue.socket.shutdown(std::net::Shutdown::Both);
    log("debug", &format!("{slug}'s link ended: {why}"));
    shared.send(Event::Closed {
        slug: slug.to_owned(),
        conn,
    });
    Ok(())
}

fn consume(stream: &mut TcpStream, len: usize) -> io::Result<()> {
    let mut b = vec![0u8; len];
    stream.set_read_timeout(Some(HEAD_TIMEOUT))?;
    stream.read_exact(&mut b)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Spec: hub: connections are counted by address, an IPv6 client by its /64, so one host
    /// cannot take every slot by spreading its connections over the addresses it is given.
    #[test]
    fn an_ipv6_client_is_counted_by_its_64() {
        let ip = |s: &str| s.parse::<IpAddr>().expect("an address");
        assert_eq!(
            counted_as(ip("2001:db8:1:2:aaaa::1")),
            counted_as(ip("2001:db8:1:2:ffff:1:2:3"))
        );
        assert_ne!(
            counted_as(ip("2001:db8:1:2::1")),
            counted_as(ip("2001:db8:1:3::1"))
        );
        assert_eq!(counted_as(ip("::ffff:192.0.2.7")), ip("192.0.2.7"));
        assert_eq!(counted_as(ip("192.0.2.7")), ip("192.0.2.7"));
        assert_ne!(counted_as(ip("192.0.2.7")), counted_as(ip("192.0.2.8")));
    }

    /// Spec: hub: the link's keep-alive is well inside the idle timeout of a proxy in front of the
    /// hub (Cloudflare closes a connection idle for 100 seconds).
    #[test]
    fn the_links_keep_alive_is_well_inside_a_proxys_idle_timeout() {
        let proxy_idle = Duration::from_secs(100);
        assert!(
            PING_EVERY * 2 <= proxy_idle,
            "a ping at least twice in each idle window"
        );
        assert!(IDLE_AFTER < proxy_idle);
    }

    /// Spec: hub: a head that never finishes is given less time once half the connections are
    /// heads.
    #[test]
    fn heads_get_less_time_while_many_are_unfinished() {
        assert_eq!(head_timeout(0), HEAD_TIMEOUT);
        assert_eq!(head_timeout(MAX_CONNECTIONS / 2 - 1), HEAD_TIMEOUT);
        assert_eq!(head_timeout(MAX_CONNECTIONS / 2), HEAD_TIMEOUT_BUSY);
        assert!(HEAD_TIMEOUT_BUSY < HEAD_TIMEOUT);
    }
}
