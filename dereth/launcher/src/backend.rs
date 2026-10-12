//! Everything the web front end asks for, done here, with `dereth_launch` doing the thinking.
//!
//! The page holds only what is on screen. What the launcher knows (the Dereth client beside it, the
//! retail client and the dat sets, the accounts, the world list, the clients it started) lives in
//! one [`Backend`] behind a mutex, and the page reads it whole with [`Backend::snapshot`] and
//! changes it through one call per action. Slow work (the world list, a world's status, copying a
//! dat set) runs on its own thread and lands back in the same state, where the page's next snapshot
//! finds it.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use dereth_launch::check::RunningClient;
use dereth_launch::choices::{self, ClientOption};
use dereth_launch::datset::{scan_dir, DatOrigin, DatSet, SetKind};
use dereth_launch::eras::{self, EraInfo, FeatureInfo};
use dereth_launch::folders::Folders;
use dereth_launch::install::{ClientKind, Installation};
use dereth_launch::interface::{self, Offer};
use dereth_launch::launch::{plan, LaunchRequest};
use dereth_launch::library::{self, FolderFind};
use dereth_launch::serverlist::{self, ListCache, ListSource};
use dereth_launch::state::{Account, Favourite, LauncherState, Recent, WorldPrefs};
use dereth_launch::status::LiveStatus;
use dereth_launch::vault::{target, Vault};
#[cfg(test)]
use dereth_launch::world::WorldState;
use dereth_launch::world::{AccountModel, Emulator, StatusMethod, Told, World};
use dereth_launch::worlds;
use serde::{Deserialize, Serialize};

const STATUS_INTERVAL: Duration = Duration::from_secs(60);

/// How many servers are asked whether they are up at once.
const PROBES_AT_ONCE: usize = 8;

/// Ask a server whether it is up with the server-tracker login ([`dereth_launch::probe`]): three
/// tries, 700 ms apiece. Any well-formed packet back from its address is a yes.
fn probe_host(host: &str, port: u16) -> bool {
    use std::net::{ToSocketAddrs, UdpSocket};
    let Some(addr) = (host, port)
        .to_socket_addrs()
        .ok()
        .and_then(|mut a| a.find(std::net::SocketAddr::is_ipv4))
    else {
        return false;
    };
    let Ok(sock) = UdpSocket::bind(("0.0.0.0", 0)) else {
        return false;
    };
    let wait = Duration::from_millis(700);
    let _ = sock.set_read_timeout(Some(wait));
    let request = dereth_launch::probe::request();
    let mut buf = [0u8; 1500];
    for _ in 0..3 {
        if sock.send_to(&request, addr).is_err() {
            return false;
        }
        let until = Instant::now() + wait;
        while Instant::now() < until {
            match sock.recv_from(&mut buf) {
                Ok((n, from))
                    if from.ip() == addr.ip() && dereth_launch::probe::is_reply(&buf[..n]) =>
                {
                    return true
                }
                Ok(_) => {}
                // A timeout, or the system reporting that nothing listens there.
                Err(_) => break,
            }
        }
    }
    false
}

/// Ask a world's live status with the status ping ([`dereth_launch::probe::status_hello`]): the
/// hello, then the ask with the token it brings back. Two tries for each step, 600 ms apiece.
/// `None` when the server does not answer it, which a server other than Empyrean never does.
fn ping_status(host: &str, port: u16) -> Option<LiveStatus> {
    use std::net::{ToSocketAddrs, UdpSocket};
    let addr = (host, port)
        .to_socket_addrs()
        .ok()?
        .find(std::net::SocketAddr::is_ipv4)?;
    let sock = UdpSocket::bind(("0.0.0.0", 0)).ok()?;
    let wait = Duration::from_millis(600);
    sock.set_read_timeout(Some(wait)).ok()?;
    // Sends `request` and waits for a datagram from the server that `wanted` takes.
    let exchange = |request: &[u8], wanted: &dyn Fn(&[u8]) -> bool| {
        let mut buf = [0u8; 1500];
        for _ in 0..2 {
            sock.send_to(request, addr).ok()?;
            let until = Instant::now() + wait;
            while Instant::now() < until {
                match sock.recv_from(&mut buf) {
                    Ok((n, from)) if from == addr && wanted(&buf[..n]) => {
                        return Some(buf[..n].to_vec());
                    }
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
        }
        None
    };
    let token = exchange(&dereth_launch::probe::status_hello(), &|d| {
        dereth_launch::probe::status_ask(d).is_some()
    })?;
    let ask = dereth_launch::probe::status_ask(&token)?;
    let reply = exchange(&ask, &|d| dereth_launch::probe::status_reply(d).is_some())?;
    dereth_launch::probe::status_reply(&reply)
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// One HTTPS GET, bounded. Plain HTTP is refused: the URLs come from configuration and the
/// world list, and quietly downgrading either to plaintext is not a favour.
pub fn http_get(url: &str) -> Result<Vec<u8>, String> {
    if !url.to_ascii_lowercase().starts_with("https://") {
        return Err(format!("{url}: only https is accepted"));
    }
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(8)))
        .build()
        .into();
    let mut resp = agent.get(url).call().map_err(|e| format!("{url}: {e}"))?;
    resp.body_mut()
        .with_config()
        .limit(1 << 20)
        .read_to_vec()
        .map_err(|e| format!("{url}: {e}"))
}

/// Open an `http`/`https` URL in the browser. Operator- and list-supplied links are not
/// trusted to be anything else.
pub fn open_url(url: &str) -> Result<(), String> {
    let lower = url.to_ascii_lowercase();
    if !(lower.starts_with("https://") || lower.starts_with("http://")) {
        return Err(format!("not opening {url}: only web links are opened"));
    }
    #[cfg(windows)]
    let mut cmd = {
        let mut c = Command::new("rundll32");
        c.arg("url.dll,FileProtocolHandler");
        c
    };
    #[cfg(target_os = "macos")]
    let mut cmd = Command::new("open");
    #[cfg(not(any(windows, target_os = "macos")))]
    let mut cmd = Command::new("xdg-open");
    cmd.arg(url).spawn().map(|_| ()).map_err(|e| e.to_string())
}

/// A client the launcher started.
#[derive(Debug)]
struct Running {
    child: Child,
    world_slug: String,
    account: String,
    kind: ClientKind,
    multi_instance: bool,
    dat_set_id: Option<String>,
}

/// A long job, as the page shows it.
#[derive(Debug, Clone, Serialize)]
pub struct JobView {
    pub what: String,
    pub done: u64,
    pub total: u64,
}

/// A line for the status bar.
#[derive(Debug, Clone, Serialize)]
pub struct Message {
    pub text: String,
    pub error: bool,
}

/// Where the world list stands.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ListState {
    Loading,
    Loaded,
    Unavailable,
}

/// Everything the page draws from.
#[derive(Debug, Clone, Serialize)]
pub struct Snapshot {
    pub worlds: Vec<World>,
    pub list_state: ListState,
    pub list_error: Option<String>,
    /// When the world list was fetched, in seconds since the epoch; `None` before it ever was.
    pub list_fetched_at: Option<u64>,
    /// The eras a world can play, for the era drop-downs.
    pub eras: Vec<EraInfo>,
    /// Every system an era has or lacks, for the check boxes.
    pub features: Vec<FeatureInfo>,
    pub state: LauncherState,
    /// The Dereth client the launcher found beside itself (or, in a build from the repository, the
    /// one that build made).
    pub dereth: Option<Installation>,
    pub running: Vec<RunningClient>,
    pub job: Option<JobView>,
    pub message: Option<Message>,
    pub vault_name: &'static str,
    pub platform: &'static str,
    pub library_dir: String,
    /// The interface the Dereth client starts in. `None` when the client keeps no preferences
    /// here (the system names no home), when there is nothing to ask.
    pub interface: Option<InterfaceView>,
}

/// The interface the Dereth client starts in, and the question the launcher asks for it.
#[derive(Debug, Clone, Serialize)]
pub struct InterfaceView {
    /// The interface the client's preferences name; `None` while they name none, and the page
    /// asks.
    pub chosen: Option<&'static str>,
    /// The interfaces offered, in order: the first is chosen when the question opens.
    pub offers: Vec<Offer>,
    /// The client's preferences file, where the answer is written.
    pub file: String,
}

/// A world page's options.
#[derive(Debug, Clone, Serialize)]
pub struct WorldView {
    pub world: World,
    pub clients: Vec<ClientView>,
    pub default_client: Option<ClientKind>,
    /// The Modern sets the Dereth client may be given here.
    pub dat_sets: Vec<DatSet>,
    /// The Classic sets the Dereth client may be given here.
    pub classic_sets: Vec<DatSet>,
    /// Which kind of set the world is drawn from. A Modern set is always needed besides; a
    /// Classic one is optional when this is Modern.
    pub requires: SetKind,
    pub offers_private_copy: bool,
    pub accounts: Vec<Account>,
    pub prefs: WorldPrefs,
    pub live: Option<LiveStatus>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClientView {
    #[serde(flatten)]
    pub option: ClientOption,
    /// Whether this machine can start it at all. A retail client needs Windows.
    pub runnable: bool,
}

/// What the player chose on a world page, or on Home.
#[derive(Debug, Clone, Deserialize)]
pub struct Choice {
    pub world_slug: String,
    pub account: String,
    /// `None` means "take it from the vault".
    pub password: Option<String>,
    #[serde(default)]
    pub remember: bool,
    pub client: ClientKind,
    /// The Dereth client's data files. A retail client plays with the ones beside it.
    pub dat_set_id: Option<String>,
    /// The Dereth client's Classic set, if one is chosen.
    #[serde(default)]
    pub classic_set_id: Option<String>,
}

/// The Add server form, as the player filled it in.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct NewServer {
    #[serde(default)]
    pub name: String,
    pub host: String,
    pub port: String,
    /// `PvE` or `PvP`; `None` when not said.
    #[serde(default)]
    pub ruleset: Option<String>,
    /// Unknown when not said.
    #[serde(default)]
    pub emulator: Emulator,
    /// An era's name; `None` when not said.
    #[serde(default)]
    pub era: Option<String>,
}

/// What pressing PLAY came to.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LaunchOutcome {
    /// The client was started for this world and account; the page says so in those words.
    Launched {
        world: String,
        account: String,
    },
    NeedPassword {
        message: String,
    },
    Error {
        message: String,
    },
}

fn can_run(kind: ClientKind) -> bool {
    kind == ClientKind::Dereth || RETAIL_HERE
}

/// Whether this system can run the retail client at all. It is a Windows program; elsewhere the
/// launcher offers the Dereth client alone and never takes a folder's `acclient.exe`.
const RETAIL_HERE: bool = cfg!(windows);

/// The launcher's state and everything it is doing.
pub struct Backend {
    folders: Folders,
    /// Where the world list is read from, in the order the sources are tried.
    list_sources: Vec<ListSource>,
    state: LauncherState,
    dereth: Option<Installation>,
    worlds: Vec<World>,
    list_state: ListState,
    list_error: Option<String>,
    /// The day's copy of the list, when there is one.
    list_cache: Option<ListCache>,
    live: HashMap<String, LiveStatus>,
    polled: HashMap<String, Instant>,
    /// Whether each server answered the server-tracker login, for worlds with no status document.
    probed: HashMap<String, bool>,
    probing: HashSet<String>,
    running: Vec<Running>,
    job: Option<JobView>,
    message: Option<Message>,
    vault: Box<dyn Vault + Send>,
    vault_name: &'static str,
    /// The preferences file the Dereth client reads when the launcher starts it, which names the
    /// interface it starts in ([`interface::preferences_file`]).
    client_preferences: Option<PathBuf>,
}

impl std::fmt::Debug for Backend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Backend")
            .field("folders", &self.folders)
            .field("worlds", &self.worlds.len())
            .finish()
    }
}

pub type Shared = Arc<Mutex<Backend>>;

pub fn lock(b: &Shared) -> MutexGuard<'_, Backend> {
    // A panic on another thread must not take the launcher down with it.
    b.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl Backend {
    /// Load the state from the settings folder and the last copy of the world list from the data
    /// folder, read the Dereth client at `dereth_exe`, and start. `list_sources` are where the list is
    /// read from, in the order they are tried ([`serverlist::sources`]), and
    /// `client_preferences` the preferences file the Dereth client reads.
    ///
    /// A first run whose Dereth client has a full end-of-retail set beside it (a developer's
    /// checkout, say) takes that set as the shared one, so there is something to play with at once.
    pub fn new(
        folders: Folders,
        list_sources: Vec<ListSource>,
        dereth_exe: Option<PathBuf>,
        vault: Box<dyn Vault + Send>,
        vault_name: &'static str,
        client_preferences: Option<PathBuf>,
    ) -> Self {
        let (mut state, _) = LauncherState::load(&folders.config);
        // Files change outside the launcher (a patch, a copy, a deleted folder); each set's four
        // headers are re-read once at start, which costs a few small reads.
        for set in &mut state.dat_sets {
            set.refresh(now());
        }
        let dereth = dereth_exe.as_deref().and_then(library::dereth_client);
        if state.dat_sets.is_empty() {
            if let Some(d) = &dereth {
                let find = library::read_folder(&d.path, &state);
                if find
                    .dats
                    .as_ref()
                    .is_some_and(|s| s.iterations().is_end_of_retail())
                {
                    library::add_find(
                        &mut state,
                        FolderFind {
                            retail: None,
                            ..find
                        },
                    );
                    let _ = state.save(&folders.config);
                }
            }
        }
        let list_cache = ListCache::load(&folders.data);
        let worlds = list_cache
            .as_ref()
            .and_then(|c| c.worlds().ok())
            .unwrap_or_default();
        Self {
            folders,
            list_sources,
            state,
            dereth,
            worlds,
            list_state: ListState::Loading,
            list_error: None,
            list_cache,
            live: HashMap::new(),
            polled: HashMap::new(),
            probed: HashMap::new(),
            probing: HashSet::new(),
            running: Vec::new(),
            job: None,
            message: None,
            vault,
            vault_name,
            client_preferences,
        }
    }

    fn save(&mut self) {
        if let Err(e) = self.state.save(&self.folders.config) {
            self.say(format!("Could not save the launcher's state: {e}"), true);
        }
    }

    fn say(&mut self, text: impl Into<String>, error: bool) {
        self.message = Some(Message {
            text: text.into(),
            error,
        });
    }

    /// Show an error in the notice, as any other part of the app can.
    pub fn notify_error(&mut self, text: impl Into<String>) {
        self.say(text, true);
    }

    pub fn dismiss_message(&mut self) {
        self.message = None;
    }

    fn library_dir(&self) -> PathBuf {
        self.state.library_dir(&self.folders.data)
    }

    fn with_live(&self, w: World) -> World {
        let probed = self.probed.get(&w.slug).copied();
        let live = self.live.get(&w.slug);
        worlds::shown(w, probed, live, &self.state)
    }

    /// Forget Play Again's entries for worlds the list read last no longer names. Only after a list
    /// was read whole: a failed fetch, or one that came back empty, forgets nothing.
    fn forget_unlisted(&mut self) {
        if self.worlds.is_empty() {
            return;
        }
        let listed: HashSet<&str> = self.worlds.iter().map(|w| w.slug.as_str()).collect();
        if self.state.forget_unlisted(&listed) {
            self.save();
        }
    }

    /// Every world: the list's, then the servers the player added.
    fn all_worlds(&self) -> impl Iterator<Item = World> + '_ {
        worlds::all(&self.worlds, &self.state).map(|w| self.with_live(w))
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            worlds: self.all_worlds().collect(),
            list_state: self.list_state.clone(),
            list_error: self.list_error.clone(),
            list_fetched_at: self.list_cache.as_ref().map(|c| c.fetched_at),
            eras: eras::eras(),
            features: eras::features(),
            state: self.state.clone(),
            dereth: self.dereth.clone(),
            running: self.running_clients(),
            job: self.job.clone(),
            message: self.message.clone(),
            vault_name: self.vault_name,
            platform: std::env::consts::OS,
            library_dir: self.library_dir().display().to_string(),
            interface: self.interface_view(),
        }
    }

    // ----- the interface -------------------------------------------------------------------------

    /// The interface the Dereth client starts in, read from its preferences each time, so a choice
    /// made in the client's own options shows here too.
    fn interface_view(&self) -> Option<InterfaceView> {
        let file = self.client_preferences.as_deref()?;
        Some(InterfaceView {
            chosen: interface::saved(file).map(interface::Interface::label),
            offers: interface::offers(),
            file: file.display().to_string(),
        })
    }

    /// Start the Dereth client in the interface named `name` from now on: the answer to the
    /// first-start question, or a change from the Library. Refused while a Dereth client the
    /// launcher started is running, because it saves its own interface when it closes.
    pub fn set_interface(&mut self, name: &str) -> Result<(), String> {
        let choice = interface::by_name(name)
            .ok_or_else(|| format!("{name} is not an interface the Dereth client has."))?;
        let file = self
            .client_preferences
            .clone()
            .ok_or("The Dereth client keeps no settings on this computer.")?;
        if self.running.iter().any(|r| r.kind == ClientKind::Dereth) {
            return Err(
                "Close the Dereth client first: it saves its own interface when it closes.".into(),
            );
        }
        interface::save(&file, choice)
            .map_err(|e| format!("Could not save the interface to {}: {e}", file.display()))
    }

    fn world(&self, slug: &str) -> Option<World> {
        self.all_worlds().find(|w| w.slug == slug)
    }

    fn running_clients(&self) -> Vec<RunningClient> {
        self.running
            .iter()
            .map(|r| RunningClient {
                world_slug: r.world_slug.clone(),
                account: r.account.clone(),
                pid: r.child.id(),
                kind: r.kind,
                multi_instance: r.multi_instance,
            })
            .collect()
    }

    // ----- the world list ------------------------------------------------------------------------

    /// The world list at start: the day's copy when it is less than a day old, else fetched again.
    pub fn load_list(shared: &Shared) {
        let fresh = {
            let mut b = lock(shared);
            let fresh = b.list_cache.as_ref().is_some_and(|c| c.is_fresh(now()));
            if fresh {
                b.list_state = ListState::Loaded;
            }
            fresh
        };
        if fresh {
            lock(shared).forget_unlisted();
            Self::probe_all(shared);
        } else {
            Self::refresh(shared);
        }
    }

    /// Fetch the list again on a thread, whatever the copy's age: the refresh button. Its sources
    /// are tried in order until one answers with a list; when none does, the last copy's worlds
    /// stay on screen.
    pub fn refresh(shared: &Shared) {
        Self::refresh_with(shared, http_get);
    }

    /// [`Self::refresh`], fetching each address with `get`. Answers the thread that fetches.
    fn refresh_with<G>(shared: &Shared, get: G) -> std::thread::JoinHandle<()>
    where
        G: FnMut(&str) -> Result<Vec<u8>, String> + Send + 'static,
    {
        let sources = {
            let mut b = lock(shared);
            b.list_state = ListState::Loading;
            b.list_sources.clone()
        };
        let shared = shared.clone();
        std::thread::spawn(move || {
            let result = serverlist::fetch_first(&sources, now(), get);
            let mut b = lock(&shared);
            match result {
                Ok((cache, worlds)) => {
                    if let Err(e) = cache.save(&b.folders.data) {
                        eprintln!("dereth-launcher: the world list's copy was not saved: {e}");
                    }
                    b.worlds = worlds;
                    b.list_cache = Some(cache);
                    b.list_state = ListState::Loaded;
                    b.list_error = None;
                    b.forget_unlisted();
                }
                Err(e) => {
                    b.list_state = ListState::Unavailable;
                    b.list_error = Some(e);
                }
            }
            drop(b);
            Self::probe_all(&shared);
        })
    }

    /// Ask every world without a status document (the player's own servers included) whether it
    /// is up.
    pub fn probe_all(shared: &Shared) {
        let slugs: Vec<String> = {
            let b = lock(shared);
            b.all_worlds()
                .filter(|w| w.status_method != StatusMethod::EmpyreanHttp)
                .map(|w| w.slug)
                .collect()
        };
        Self::probe(shared, slugs);
    }

    /// Ask the named worlds whether they are up, a few at a time, on threads of their own. A world
    /// with no status document that may be Empyrean (it says so, or nobody has said what it runs)
    /// is asked with the status ping first, which tells its state, players, era, systems, software
    /// and name; one that does not answer it, and every other, with the server-tracker login.
    pub fn probe(shared: &Shared, slugs: Vec<String>) {
        let queue: Vec<(String, String, u16, bool)> = {
            let mut b = lock(shared);
            let targets: Vec<(String, String, u16, bool)> = slugs
                .iter()
                .filter(|s| !b.probing.contains(*s))
                .filter_map(|s| b.world(s))
                .filter_map(|w| {
                    let ping = w.status_method != StatusMethod::EmpyreanHttp
                        && matches!(w.emulator, Emulator::Empyrean | Emulator::Unknown);
                    w.endpoint.map(|e| (w.slug, e.address, e.port, ping))
                })
                .collect();
            for (slug, ..) in &targets {
                b.probing.insert(slug.clone());
            }
            targets
        };
        let queue = Arc::new(Mutex::new(queue));
        for _ in 0..PROBES_AT_ONCE {
            let (queue, shared) = (queue.clone(), shared.clone());
            std::thread::spawn(move || loop {
                let next = queue
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .pop();
                let Some((slug, host, port, ping)) = next else {
                    break;
                };
                let live = ping.then(|| ping_status(&host, port)).flatten();
                let up = live.is_some() || probe_host(&host, port);
                let mut b = lock(&shared);
                b.probing.remove(&slug);
                b.probed.insert(slug.clone(), up);
                if ping {
                    b.polled.insert(slug.clone(), Instant::now());
                    // A world that stopped answering the ping is up or down by the login alone.
                    match live {
                        Some(live) => b.live.insert(slug, live),
                        None => b.live.remove(&slug),
                    };
                }
            });
        }
    }

    /// Once a second: ask for each Empyrean world's status when it is due (its status document,
    /// or the status ping for a world that answered it), and watch the clients.
    pub fn tick(shared: &Shared) {
        let pings: Vec<String> = {
            let b = lock(shared);
            b.all_worlds()
                .filter(|w| w.status_method != StatusMethod::EmpyreanHttp)
                .filter(|w| b.live.contains_key(&w.slug) && !b.probing.contains(&w.slug))
                .filter(|w| {
                    b.polled
                        .get(&w.slug)
                        .is_none_or(|t| t.elapsed() >= STATUS_INTERVAL)
                })
                .map(|w| w.slug)
                .collect()
        };
        if !pings.is_empty() {
            Self::probe(shared, pings);
        }
        let due: Vec<(String, String)> = {
            let b = lock(shared);
            b.worlds
                .iter()
                .filter(|w| w.status_method == StatusMethod::EmpyreanHttp)
                .filter_map(|w| w.status_url.clone().map(|u| (w.slug.clone(), u)))
                .filter(|(slug, _)| {
                    b.polled
                        .get(slug)
                        .is_none_or(|t| t.elapsed() >= STATUS_INTERVAL)
                })
                .collect()
        };
        for (slug, url) in due {
            lock(shared).polled.insert(slug.clone(), Instant::now());
            let shared = shared.clone();
            std::thread::spawn(move || {
                if let Some(live) = http_get(&url)
                    .ok()
                    .and_then(|b| dereth_launch::status::parse_world_document(&b))
                {
                    lock(&shared).live.insert(slug, live);
                }
            });
        }
        lock(shared).poll_running();
    }

    fn poll_running(&mut self) {
        let mut dirty = false;
        let mut i = 0;
        while i < self.running.len() {
            if matches!(self.running[i].child.try_wait(), Ok(Some(_)) | Err(_)) {
                let r = self.running.remove(i);
                // A patching world may have written to the set during the session.
                if let Some(set) = r
                    .dat_set_id
                    .as_deref()
                    .and_then(|id| self.state.dat_set_mut(id))
                {
                    set.refresh(now());
                    dirty = true;
                }
            } else {
                i += 1;
            }
        }
        if dirty {
            self.save();
        }
    }

    // ----- a world's page ------------------------------------------------------------------------

    pub fn world_view(&self, slug: &str) -> Result<WorldView, String> {
        let world = self.world(slug).ok_or("That world is no longer listed.")?;
        let prefs = self
            .state
            .world_prefs
            .get(slug)
            .cloned()
            .unwrap_or_default();
        let mut options = choices::clients_for(&self.state, &world, self.dereth.as_ref());
        options.retain(|o| can_run(o.kind));
        let default_client = choices::default_client(&options, &world, prefs.client);
        Ok(WorldView {
            clients: options
                .into_iter()
                .map(|option| ClientView {
                    runnable: can_run(option.kind),
                    option,
                })
                .collect(),
            default_client,
            dat_sets: choices::dat_sets_for(&self.state, &world),
            classic_sets: choices::classic_sets_for(&self.state),
            requires: choices::required_set_for(&world),
            offers_private_copy: choices::offers_private_copy(&self.state, &world),
            accounts: self.state.accounts_for(slug).cloned().collect(),
            prefs,
            live: self.live.get(slug).cloned(),
            world,
        })
    }

    /// The client a choice names.
    fn client_for(&self, kind: ClientKind) -> Result<Installation, String> {
        match kind {
            ClientKind::Dereth => self
                .dereth
                .clone()
                .ok_or_else(|| "The Dereth client was not found beside the launcher.".to_owned()),
            ClientKind::Retail => self.state.retail.clone().ok_or_else(|| {
                "There is no retail client yet: choose its folder in the Library.".to_owned()
            }),
        }
    }

    pub fn has_password(&self, slug: &str, username: &str) -> bool {
        self.vault
            .get(&target(slug, username))
            .ok()
            .flatten()
            .is_some()
    }

    // ----- PLAY ----------------------------------------------------------------------------------

    /// Start the client on a choice. Nothing is checked against the world first: what a world
    /// accepts is not known well enough yet to stop anyone.
    pub fn launch(&mut self, choice: Choice) -> LaunchOutcome {
        let err = |m: String| LaunchOutcome::Error { message: m };
        let Some(world) = self.world(&choice.world_slug) else {
            return err("That world is no longer listed.".into());
        };
        let install = match self.client_for(choice.client) {
            Ok(i) => i,
            Err(e) => return err(e),
        };
        if !can_run(install.kind) {
            return err(
                "The retail client needs Windows. Here, only the Dereth client can be started."
                    .into(),
            );
        }
        let key = target(&world.slug, &choice.account);
        let password = match choice.password.clone().filter(|p| !p.is_empty()) {
            Some(p) => p,
            None => match self.vault.get(&key).ok().flatten() {
                Some(p) => p,
                None => {
                    return LaunchOutcome::NeedPassword {
                        message: format!("Enter the password for {}.", choice.account),
                    };
                }
            },
        };
        let dereth = choice.client == ClientKind::Dereth;
        let dat_set_id = dereth.then(|| choice.dat_set_id.clone()).flatten();
        let classic_set_id = dereth.then(|| choice.classic_set_id.clone()).flatten();
        let path_of = |id: Option<&str>, kind: SetKind| {
            id.and_then(|id| self.state.dat_set(id))
                .filter(|s| s.kind == kind)
                .map(|s| s.path.clone())
        };
        // A world that ships its own files is never started on another world's: the chosen set
        // must report what the world's files report.
        if let Some(set) = dat_set_id.as_deref().and_then(|id| self.state.dat_set(id)) {
            if let Err(why) = choices::set_matches(&world, set) {
                return err(why);
            }
        }
        let (dat_dir, classic_dat_dir) = if dereth {
            let modern = path_of(dat_set_id.as_deref(), SetKind::Modern);
            let classic = path_of(classic_set_id.as_deref(), SetKind::Classic);
            match choices::dat_dirs_for(&world, modern.as_deref(), classic.as_deref()) {
                Ok(d) => (Some(d.dat_dir), d.classic_dat_dir),
                Err(e) => return err(e.to_string()),
            }
        } else {
            (None, None)
        };
        let req = LaunchRequest {
            world: &world,
            install: &install,
            account: &choice.account,
            dat_dir,
            classic_dat_dir,
            extra_args: &[],
        };
        let plan = match plan(&req) {
            Ok(p) => p,
            Err(e) => return err(e.to_string()),
        };
        if !plan.exe.exists() {
            return err(format!("Client not found: {}", plan.exe.display()));
        }
        // The one form of the command line that may be logged; the page names only the world and
        // account.
        let shown = plan.redacted();
        eprintln!("dereth-launcher: starting {shown}");

        let child = match Command::new(&plan.exe)
            .current_dir(&plan.working_dir)
            .args(plan.argv(&password))
            .spawn()
        {
            Ok(c) => c,
            Err(e) => return err(format!("Could not start: {e}")),
        };

        // Remember what worked. The password goes in the vault only when asked to.
        if choice.remember {
            if let Err(e) = self.vault.set(&key, &choice.account, &password) {
                self.say(e.to_string(), true);
            }
        } else {
            let _ = self.vault.delete(&key);
        }
        let existing = self
            .state
            .account(&choice.world_slug, &choice.account)
            .cloned();
        let auto_create = world.account_model == AccountModel::AutoCreateOnFirstLogin;
        self.state.upsert_account(Account {
            world_slug: choice.world_slug.clone(),
            username: choice.account.clone(),
            label: existing.as_ref().and_then(|a| a.label.clone()),
            remember: choice.remember,
            created_by_launcher: existing.map_or(auto_create, |a| a.created_by_launcher),
        });
        self.state.remember_launch(
            &choice.world_slug,
            WorldPrefs {
                client: Some(choice.client),
                dat_set_id: dat_set_id.clone(),
                classic_set_id: classic_set_id.clone(),
                account: Some(choice.account.clone()),
                last_played: Some(now()),
            },
        );
        self.state.record_recent(Recent {
            world_slug: choice.world_slug.clone(),
            account: choice.account.clone(),
            client: choice.client,
            dat_set_id: dat_set_id.clone(),
            classic_set_id,
            last_played: now(),
        });
        self.save();
        self.running.push(Running {
            child,
            world_slug: world.slug.clone(),
            account: choice.account.clone(),
            kind: install.kind,
            multi_instance: install.multi_instance,
            dat_set_id,
        });
        LaunchOutcome::Launched {
            world: world.name.clone(),
            account: choice.account.clone(),
        }
    }

    // ----- the library ---------------------------------------------------------------------------

    pub fn read_folder(&self, path: &Path) -> FolderFind {
        let find = library::read_folder(path, &self.state);
        if RETAIL_HERE {
            find
        } else {
            FolderFind {
                retail: None,
                ..find
            }
        }
    }

    pub fn add_folder(&mut self, find: FolderFind) {
        let find = if RETAIL_HERE {
            find
        } else {
            FolderFind {
                retail: None,
                ..find
            }
        };
        library::add_find(&mut self.state, find);
        self.save();
    }

    /// Choose the era of a world that does not say its own. `None`: not chosen.
    pub fn set_world_era(&mut self, slug: &str, era: Option<&str>) -> Result<(), String> {
        let w = self.world(slug).ok_or("That world is no longer listed.")?;
        if w.era_source == Some(Told::World) {
            return Err(format!("{} says its own era.", w.name));
        }
        self.state.set_world_era(slug, era);
        self.save();
        Ok(())
    }

    /// Turn one of a world's systems on or off, for a world that does not say its own. The choice
    /// is made over the era the world plays, whoever named it.
    pub fn set_world_feature(&mut self, slug: &str, name: &str, on: bool) -> Result<(), String> {
        let w = self.world(slug).ok_or("That world is no longer listed.")?;
        if w.features_source == Some(Told::World) {
            return Err(format!("{} says which systems it has.", w.name));
        }
        if w.era_source == Some(Told::World)
            && self
                .state
                .world_eras
                .get(slug)
                .and_then(|c| c.era.as_deref())
                != w.era.as_deref()
        {
            self.state.set_world_era(slug, w.era.as_deref());
        }
        if !self.state.set_world_feature(slug, name, on) {
            return Err(format!("{name} is not a system the client knows."));
        }
        self.save();
        Ok(())
    }

    /// Make a set the default of its kind, the one the Dereth client is offered first.
    pub fn set_default_set(&mut self, id: &str) -> Result<(), String> {
        if !self.state.set_default_set(id) {
            return Err("Only a folder you added can be the default; a world's own copy stays with its world.".into());
        }
        self.save();
        Ok(())
    }

    /// A downloaded custom set, checked against what its world published before it is kept.
    pub fn add_custom_dats(&mut self, slug: &str, path: &Path) -> Result<(), String> {
        let w = self.world(slug).ok_or("That world is no longer listed.")?;
        let custom = w
            .dats
            .custom
            .clone()
            .ok_or("This world does not ship its own data files.")?;
        let set = DatSet {
            id: self.state.new_id("d"),
            path: path.to_path_buf(),
            kind: SetKind::Modern,
            origin: DatOrigin::Custom {
                sha256: custom.sha256.clone().unwrap_or_default(),
            },
            files: scan_dir(path),
            last_patched_by_server: None,
            created_by_launcher: false,
        };
        // The files the world's server compares, as the world's own files report them.
        choices::set_matches(&w, &set)?;
        self.state.dat_sets.push(set);
        self.save();
        Ok(())
    }

    pub fn forget_retail(&mut self) {
        self.state.forget_retail();
        self.save();
    }

    /// Forget a set; delete its files too when the launcher made them, and only inside the library.
    pub fn delete_set(&mut self, id: &str) -> Result<(), String> {
        let set = self.state.dat_set(id).cloned().ok_or("No such set.")?;
        if set.created_by_launcher {
            let library = self.library_dir();
            if set.path.starts_with(&library) && set.path != library {
                std::fs::remove_dir_all(&set.path)
                    .map_err(|e| format!("Could not delete {}: {e}", set.path.display()))?;
            }
        }
        self.state.dat_sets.retain(|d| d.id != id);
        self.save();
        Ok(())
    }

    fn start_job(
        shared: &Shared,
        what: String,
        work: impl FnOnce(&dyn Fn(u64, u64)) -> Result<(), String> + Send + 'static,
    ) {
        {
            let mut b = lock(shared);
            if b.job.is_some() {
                b.say("Another copy is still running.", true);
                return;
            }
            b.job = Some(JobView {
                what: what.clone(),
                done: 0,
                total: 0,
            });
        }
        let shared = shared.clone();
        std::thread::spawn(move || {
            let progress_to = shared.clone();
            let report = move |done, total| {
                if let Some(j) = lock(&progress_to).job.as_mut() {
                    j.done = done;
                    j.total = total;
                }
            };
            let result = work(&report);
            let mut b = lock(&shared);
            b.job = None;
            if let Err(e) = result {
                b.say(e, true);
            }
        });
    }

    /// Copy the shared set into a set of the world's own, in the background.
    pub fn create_private_copy(shared: &Shared, slug: &str) {
        let (from, to, id, name) = {
            let mut b = lock(shared);
            let Some(set) = b.state.shared_set().cloned() else {
                b.say(
                    "There is no shared set to copy yet. Add a folder of data files first.",
                    true,
                );
                return;
            };
            let id = b.state.new_id("d");
            let to = b.library_dir().join("datsets").join(&id);
            let name = b.world(slug).map_or_else(|| slug.to_owned(), |w| w.name);
            (set.path, to, id, name)
        };
        let slug = slug.to_owned();
        let done_to = shared.clone();
        Self::start_job(
            shared,
            format!("Copying data files for {name}"),
            move |report| {
                dereth_launch::copy::copy_dat_set(&from, &to, &mut |d, t| report(d, t))
                    .map_err(|e| format!("The copy failed: {e}"))?;
                let mut b = lock(&done_to);
                b.state.dat_sets.push(DatSet {
                    id,
                    files: scan_dir(&to),
                    path: to,
                    kind: SetKind::Modern,
                    origin: DatOrigin::World { slug },
                    last_patched_by_server: None,
                    created_by_launcher: true,
                });
                b.save();
                b.say(format!("{name} has its own data files now."), false);
                Ok(())
            },
        );
    }

    pub fn reset_set(shared: &Shared, id: &str) {
        let (from, set) = {
            let b = lock(shared);
            match (b.state.shared_set().cloned(), b.state.dat_set(id).cloned()) {
                (Some(s), Some(set)) if set.created_by_launcher => (s.path, set),
                _ => return,
            }
        };
        let done_to = shared.clone();
        Self::start_job(shared, "Resetting data files".into(), move |report| {
            dereth_launch::copy::copy_dat_set(&from, &set.path, &mut |d, t| report(d, t))
                .map_err(|e| format!("The reset failed: {e}"))?;
            let mut b = lock(&done_to);
            if let Some(s) = b.state.dat_set_mut(&set.id) {
                s.refresh(now());
                s.last_patched_by_server = None;
            }
            b.save();
            Ok(())
        });
    }

    /// Check the retail client's whole folder against its build's manifest, in the background.
    pub fn verify_retail(shared: &Shared) {
        let Some(inst) = lock(shared).state.retail.clone() else {
            return;
        };
        let done_to = shared.clone();
        Self::start_job(
            shared,
            format!("Checking {}", inst.display_name()),
            move |_| {
                let mut inst = inst;
                dereth_launch::install::verify(&mut inst, now());
                let mut b = lock(&done_to);
                let summary = inst.manifest_result.as_ref().map(|r| r.summary());
                if b.state.retail.as_ref().is_some_and(|r| r.path == inst.path) {
                    b.state.retail = Some(inst);
                }
                b.save();
                if let Some(s) = summary {
                    b.say(s, false);
                }
                Ok(())
            },
        );
    }

    // ----- favourites, accounts ------------------------------------------------------------------

    pub fn save_favourite(&mut self, mut fav: Favourite) {
        if fav.id.is_empty() {
            fav.id = self.state.new_id("f");
        }
        self.state.favourites.retain(|f| f.id != fav.id);
        self.state.favourites.push(fav);
        self.save();
    }

    pub fn remove_favourite(&mut self, id: &str) {
        self.state.favourites.retain(|f| f.id != id);
        self.save();
    }

    /// Add a server by hand, with the rules, the emulator and the era the player says it has.
    pub fn add_custom_world(&mut self, s: &NewServer) -> Result<String, String> {
        let slug = self
            .state
            .add_custom_world(&s.name, &s.host, &s.port, s.ruleset.as_deref(), s.emulator)
            .map_err(|e| e.to_string())?;
        self.state.set_world_era(&slug, s.era.as_deref());
        self.save();
        Ok(slug)
    }

    /// Remove a server the player added, and every password kept for it.
    pub fn remove_custom_world(&mut self, slug: &str) {
        let targets: Vec<String> = self
            .state
            .accounts_for(slug)
            .map(|a| target(slug, &a.username))
            .collect();
        for t in targets {
            let _ = self.vault.delete(&t);
        }
        self.state.remove_custom_world(slug);
        self.save();
    }

    /// Forget every account, and every password kept for one.
    pub fn forget_all_accounts(&mut self) {
        let all: Vec<(String, String)> = self
            .state
            .accounts
            .iter()
            .map(|a| (a.world_slug.clone(), a.username.clone()))
            .collect();
        for (slug, username) in all {
            let _ = self.vault.delete(&target(&slug, &username));
            self.state.forget_account(&slug, &username);
        }
        self.save();
    }

    pub fn forget_account(&mut self, slug: &str, username: &str) {
        let _ = self.vault.delete(&target(slug, username));
        self.state.forget_account(slug, username);
        self.save();
    }

    pub fn set_remember(&mut self, slug: &str, username: &str, remember: bool) {
        if !remember {
            let _ = self.vault.delete(&target(slug, username));
        }
        if let Some(a) = self
            .state
            .accounts
            .iter_mut()
            .find(|a| a.world_slug == slug && a.username.eq_ignore_ascii_case(username))
        {
            a.remember = remember;
        }
        self.save();
    }

    /// Every password this launcher stored. The secret store cannot be listed by prefix, so it goes
    /// account by account, which is every entry the launcher can have written.
    pub fn forget_all_passwords(&mut self) {
        let targets: Vec<String> = self
            .state
            .accounts
            .iter()
            .map(|a| target(&a.world_slug, &a.username))
            .collect();
        for t in targets {
            let _ = self.vault.delete(&t);
        }
        for a in &mut self.state.accounts {
            a.remember = false;
        }
        self.save();
        self.say("Every remembered password is forgotten.", false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_launch::testing::write_dat_set;
    use dereth_launch::vault::MemoryVault;
    use dereth_launch::world::{Emulator, Endpoint};
    use dereth_launch::Iterations;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("dereth-tauri-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A state folder, and a "Dereth client" with an end-of-retail set beside it. On a Unix system
    /// the client is a script that records how it was started.
    fn backend(name: &str) -> (Backend, PathBuf) {
        let dir = tmp(name);
        let client = dir.join("client");
        write_dat_set(&client, Iterations::END_OF_RETAIL);
        let exe = client.join("dereth-client");
        std::fs::write(
            &exe,
            "#!/bin/sh\necho \"$@\" > \"$(dirname \"$0\")/args.txt\"\n",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let mut b = Backend::new(
            Folders::single(dir.join("state")),
            Vec::new(),
            Some(exe),
            Box::new(MemoryVault::default()),
            "memory",
            Some(dir.join("settings").join("UserPreferences.ini")),
        );
        let mut w = World::new("eulmore", "Eulmore");
        w.emulator = Emulator::Empyrean;
        w.state = WorldState::Online;
        w.endpoint = Some(Endpoint {
            address: "127.0.0.1".into(),
            port: 9000,
            transport: None,
        });
        w.dats.expected = Some(Iterations::END_OF_RETAIL);
        b.worlds = vec![w];
        (b, dir)
    }

    fn choice(password: Option<&str>) -> Choice {
        Choice {
            world_slug: "eulmore".into(),
            account: "player".into(),
            password: password.map(Into::into),
            remember: true,
            client: ClientKind::Dereth,
            dat_set_id: Some("eor".into()),
            classic_set_id: None,
        }
    }

    #[test]
    fn the_dereth_client_and_the_set_beside_it_are_found_without_first_run() {
        let (b, dir) = backend("found");
        let s = b.snapshot();
        assert_eq!(
            s.dereth.as_ref().map(|d| d.exe.as_str()),
            Some("dereth-client")
        );
        assert!(s.state.shared_set().is_some());
        assert!(s.state.retail.is_none());
        let v = b.world_view("eulmore").unwrap();
        assert_eq!(v.default_client, Some(ClientKind::Dereth));
        assert_eq!(
            v.clients.len(),
            1,
            "no retail client until the player adds one"
        );
        assert_eq!(v.dat_sets.len(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[cfg(unix)]
    #[test]
    fn play_passes_the_password_with_v_and_remembers_it_only_in_the_vault() {
        let (mut b, dir) = backend("play");
        let out = b.launch(choice(Some("hunter2")));
        assert!(
            matches!(out, LaunchOutcome::Launched { ref world, ref account }
                if world == "Eulmore" && account == "player"),
            "{out:?}"
        );
        let mut child = b.running.remove(0).child;
        child.wait().unwrap();
        let args = std::fs::read_to_string(dir.join("client/args.txt")).unwrap();
        assert!(
            args.contains("-v hunter2") && args.contains("--dat-dir"),
            "{args}"
        );
        assert!(b.has_password("eulmore", "player"));
        let saved =
            std::fs::read_to_string(dir.join("state").join(dereth_launch::state::STATE_FILE))
                .unwrap();
        assert!(!saved.contains("hunter2"));

        // Next time, from Home, the vault supplies it.
        let out = b.launch(choice(None));
        assert!(matches!(out, LaunchOutcome::Launched { .. }), "{out:?}");
        assert_eq!(
            b.state.recent.len(),
            1,
            "the same combination is listed once"
        );
        b.running.remove(0).child.wait().unwrap();
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn no_password_anywhere_asks_for_one() {
        let (mut b, dir) = backend("nopw");
        assert!(matches!(
            b.launch(choice(None)),
            LaunchOutcome::NeedPassword { .. }
        ));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_retail_folder_offers_the_retail_client_and_leaves_the_default_set_alone() {
        let (mut b, dir) = backend("retail");
        let game = dir.join("game");
        write_dat_set(&game, Iterations::END_OF_RETAIL);
        std::fs::write(game.join("acclient.exe"), b"MZ").unwrap();
        let find = b.read_folder(&game);
        b.add_folder(find);
        assert_eq!(
            b.state.shared_set().unwrap().path,
            dir.join("client"),
            "the first set stays the default"
        );
        if RETAIL_HERE {
            assert_eq!(b.world_view("eulmore").unwrap().clients.len(), 2);
        } else {
            // Off Windows the folder's acclient.exe is not taken, and only Dereth is offered.
            assert!(b.state.retail.is_none());
            assert_eq!(b.world_view("eulmore").unwrap().clients.len(), 1);
            let out = b.launch(Choice {
                client: ClientKind::Retail,
                ..choice(Some("pw"))
            });
            assert!(matches!(out, LaunchOutcome::Error { .. }), "{out:?}");
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_server_added_by_hand_is_a_world_and_goes_with_its_accounts() {
        let (mut b, dir) = backend("custom");
        let slug = b
            .add_custom_world(&NewServer {
                name: "Home".into(),
                host: "127.0.0.1".into(),
                port: "9001".into(),
                ruleset: Some("PvP".into()),
                emulator: Emulator::ClassicAce,
                era: Some("infiltration".into()),
            })
            .unwrap();
        assert!(b
            .snapshot()
            .worlds
            .iter()
            .any(|w| w.slug == slug && w.name == "Home"));
        let v = b.world_view(&slug).unwrap();
        assert_eq!(v.world.ruleset.as_deref(), Some("PvP"));
        assert_eq!(v.world.emulator, Emulator::ClassicAce);
        // The form as the page sends it.
        let form: NewServer = serde_json::from_str(
            r#"{"name":"","host":"h","port":"9000","ruleset":null,"emulator":"classic_ace","era":null}"#,
        )
        .unwrap();
        assert_eq!(form.emulator, Emulator::ClassicAce);
        assert_eq!(v.world.era.as_deref(), Some("infiltration"));
        assert_eq!(v.world.era_source, Some(Told::Player));
        assert_eq!(
            v.requires,
            SetKind::Classic,
            "an era before Throne of Destiny"
        );
        b.state.upsert_account(Account {
            world_slug: slug.clone(),
            username: "player".into(),
            label: None,
            remember: true,
            created_by_launcher: false,
        });
        b.remove_custom_world(&slug);
        assert!(b.world_view(&slug).is_err());
        assert!(b.state.accounts.is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_server_that_answers_the_tracker_login_is_online_and_one_that_does_not_is_offline() {
        use std::net::UdpSocket;
        // A stand-in server on loopback that answers any packet with its own request.
        let server = UdpSocket::bind("127.0.0.1:0").unwrap();
        let port = server.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let mut buf = [0u8; 1500];
            if let Ok((n, from)) = server.recv_from(&mut buf) {
                assert!(dereth_launch::probe::is_reply(&buf[..n]));
                let _ = server.send_to(&dereth_launch::probe::request(), from);
            }
        });
        assert!(probe_host("127.0.0.1", port));
        let silent = UdpSocket::bind("127.0.0.1:0").unwrap();
        assert!(!probe_host(
            "127.0.0.1",
            silent.local_addr().unwrap().port()
        ));

        let (mut b, dir) = backend("probe");
        b.probed.insert("eulmore".into(), false);
        assert_eq!(b.world("eulmore").unwrap().state, WorldState::Offline);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A stand-in for Empyrean's side of the status ping: a token for the hello, the status for
    /// the ask that carries it.
    #[test]
    fn a_server_that_answers_the_status_ping_tells_its_state_players_era_software_and_name() {
        use dereth_launch::probe::status_ping as sp;
        use std::net::UdpSocket;
        let server = UdpSocket::bind("127.0.0.1:0").unwrap();
        let port = server.local_addr().unwrap().port();
        let token = sp::Token {
            window: 1,
            mac: [5; 16],
        };
        std::thread::spawn(move || {
            let mut buf = [0u8; 1500];
            for _ in 0..2 {
                let Ok((n, from)) = server.recv_from(&mut buf) else {
                    return;
                };
                let reply = match sp::parse_request(&buf[..n]) {
                    Some(sp::Request::Hello) => sp::token_reply(token),
                    Some(sp::Request::Ask(t)) if t == token => sp::StatusReply {
                        format_version: 1,
                        state: sp::WorldState::Open,
                        players: 4,
                        era: "infiltration".into(),
                        era_table_version: 1,
                        era_features: vec![0, 0, 0xdf],
                        software: "Empyrean".into(),
                        software_version: "0.2.0".into(),
                        world_name: "Loopback".into(),
                    }
                    .encode(),
                    _ => return,
                };
                let _ = server.send_to(&reply, from);
            }
        });
        let live = ping_status("127.0.0.1", port).expect("answered");
        assert_eq!(live.state, WorldState::Online);
        assert_eq!(live.players, Some(4));
        assert_eq!(live.era.as_deref(), Some("infiltration"));

        // A server that does not answer it is asked no further.
        let silent = UdpSocket::bind("127.0.0.1:0").unwrap();
        assert_eq!(
            ping_status("127.0.0.1", silent.local_addr().unwrap().port()),
            None
        );

        // A server the player added without a name is called what it says it is called.
        let (mut b, dir) = backend("ping");
        let slug = b
            .add_custom_world(&NewServer {
                host: "127.0.0.1".into(),
                port: port.to_string(),
                ..NewServer::default()
            })
            .unwrap();
        b.live.insert(slug.clone(), live);
        let w = b.world(&slug).unwrap();
        assert_eq!(w.name, "Loopback");
        assert_eq!(w.emulator, Emulator::Empyrean);
        assert_eq!(w.emulator_version.as_deref(), Some("0.2.0"));
        assert_eq!(w.players, Some(4));
        assert_eq!(w.era_source, Some(Told::World));
        assert_eq!(w.features_source, Some(Told::World));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_players_era_fills_in_for_a_world_that_does_not_say_and_never_over_one_that_does() {
        let (mut b, dir) = backend("eras");
        let mut w = World::new("leafcull", "Leafcull");
        w.endpoint = b.worlds[0].endpoint.clone();
        b.worlds.push(w);

        b.set_world_era("leafcull", Some("infiltration")).unwrap();
        b.set_world_feature("leafcull", "aetheria", true).unwrap();
        let w = b.world("leafcull").unwrap();
        assert_eq!(w.era.as_deref(), Some("infiltration"));
        assert_eq!(w.era_source, Some(Told::Player));
        assert_eq!(w.era_features.as_deref(), Some("aetheria=true"));
        assert_eq!(w.features_source, Some(Told::Player));
        assert_eq!(b.world_view("leafcull").unwrap().requires, SetKind::Classic);
        assert!(b.set_world_feature("leafcull", "nothing", true).is_err());

        // Eulmore's status says its era and systems: they stand, and the player cannot change them.
        b.live.insert(
            "eulmore".into(),
            dereth_launch::status::parse_world_document(
                br#"{"world_open":true,"version":"0.1.2","era":"eor","features":{"trade":true}}"#,
            )
            .unwrap(),
        );
        let w = b.world("eulmore").unwrap();
        assert_eq!(
            (w.era.as_deref(), w.era_source),
            (Some("eor"), Some(Told::World))
        );
        assert_eq!(w.emulator_version.as_deref(), Some("0.1.2"));
        assert!(b.set_world_era("eulmore", Some("infiltration")).is_err());
        assert!(b.set_world_feature("eulmore", "trade", false).is_err());

        // A world that says its era but not its systems: the player's systems are over its era.
        b.live.get_mut("eulmore").unwrap().era_features = None;
        b.set_world_feature("eulmore", "trade", false).unwrap();
        assert_eq!(b.state.world_eras["eulmore"].era.as_deref(), Some("eor"));
        let w = b.world("eulmore").unwrap();
        assert_eq!(
            (w.era_features.as_deref(), w.features_source),
            (Some("trade=false"), Some(Told::Player))
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_classic_era_world_is_not_started_without_classic_data_files() {
        let (mut b, dir) = backend("classic-needed");
        b.worlds[0].era = Some("infiltration".into());
        let out = b.launch(choice(Some("pw")));
        assert!(
            matches!(&out, LaunchOutcome::Error { message } if message.contains("Classic")),
            "{out:?}"
        );
        // A Classic set's id is not taken as the Modern one.
        let classic = dir.join("feb2005");
        std::fs::create_dir_all(&classic).unwrap();
        std::fs::write(classic.join("portal.dat"), b"p").unwrap();
        std::fs::write(classic.join("cell.dat"), b"c").unwrap();
        let find = b.read_folder(&classic);
        let id = find.classic.as_ref().unwrap().id.clone();
        b.add_folder(find);
        b.worlds[0].era = None;
        let out = b.launch(Choice {
            dat_set_id: Some(id),
            ..choice(Some("pw"))
        });
        assert!(
            matches!(&out, LaunchOutcome::Error { message } if message.contains("Modern")),
            "{out:?}"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[cfg(unix)]
    #[test]
    fn the_dereth_client_is_given_the_classic_set_and_the_chosen_era() {
        let (mut b, dir) = backend("classic-play");
        let classic = dir.join("feb2005");
        std::fs::create_dir_all(&classic).unwrap();
        std::fs::write(classic.join("portal.dat"), b"p").unwrap();
        std::fs::write(classic.join("cell.dat"), b"c").unwrap();
        let find = b.read_folder(&classic);
        let id = find.classic.as_ref().unwrap().id.clone();
        b.add_folder(find);
        b.set_world_era("eulmore", Some("infiltration")).unwrap();
        b.set_world_feature("eulmore", "aetheria", true).unwrap();
        let out = b.launch(Choice {
            classic_set_id: Some(id.clone()),
            ..choice(Some("pw"))
        });
        assert!(matches!(out, LaunchOutcome::Launched { .. }), "{out:?}");
        b.running.remove(0).child.wait().unwrap();
        let args = std::fs::read_to_string(dir.join("client/args.txt")).unwrap();
        assert!(
            args.contains(&format!("--classic-dat-dir {}", classic.display()))
                && args.contains("--era infiltration --era-features aetheria=true"),
            "{args}"
        );
        assert_eq!(
            b.state.recent[0].classic_set_id.as_deref(),
            Some(id.as_str())
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_last_copy_of_the_list_is_on_screen_at_start() {
        let dir = tmp("list-copy");
        let folders = Folders::single(dir.join("state"));
        ListCache {
            fetched_at: now(),
            list: "<ArrayOfServerItem><ServerItem><name>Leafcull</name><server_host>l.example</server_host>\
                   <server_port>9000</server_port><type>PvE</type></ServerItem></ArrayOfServerItem>"
                .into(),
            hub: Vec::new(),
        }
        .save(&folders.data)
        .unwrap();
        let b = Backend::new(
            folders,
            Vec::new(),
            None,
            Box::new(MemoryVault::default()),
            "memory",
            None,
        );
        let s = b.snapshot();
        assert_eq!(s.worlds.len(), 1);
        assert_eq!(s.worlds[0].slug, "leafcull");
        assert!(s.list_fetched_at.is_some());
        assert!(
            b.list_cache.as_ref().unwrap().is_fresh(now()),
            "fresh: no fetch at start"
        );
        assert_eq!(s.eras.len(), 2);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The list the launcher fetches by default comes from the hub, or from the community's own
    /// list when the hub does not answer with its list, and becomes the day's copy; when neither
    /// answers with a list that shows a world, the last copy's worlds stay on screen and the list is
    /// unavailable. A world keeps its key, and what is remembered under it, whichever list it came
    /// from.
    #[test]
    fn the_community_list_stands_in_for_the_hub_and_neither_answering_keeps_the_last_copy() {
        use serverlist::{COMMUNITY_LIST_URL, HUB_LIST_URL};
        let list_of = |names: &[&str]| {
            let items: String = names
                .iter()
                .map(|n| format!("<ServerItem><name>{n}</name></ServerItem>"))
                .collect();
            format!("<ArrayOfServerItem>{items}</ArrayOfServerItem>")
        };
        // The hub lists the world under a slug of its own; its address is this machine's.
        let hub_page = serde_json::json!({"data": [{"slug": "leafcull", "name": "Leafcull Isle",
            "isDemo": false, "emulator": {"family": "ACE"},
            "endpoint": {"mode": "manual", "address": "127.0.0.1", "port": 9}}],
            "nextOffset": null})
        .to_string();
        let dir = tmp("list-sources");
        let folders = Folders::single(dir.join("state"));
        ListCache {
            fetched_at: 1,
            list: list_of(&["Leafcull Isle"]),
            hub: Vec::new(),
        }
        .save(&folders.data)
        .unwrap();
        let shared: Shared = Arc::new(Mutex::new(Backend::new(
            folders.clone(),
            serverlist::sources(None),
            None,
            Box::new(MemoryVault::default()),
            "memory",
            None,
        )));
        lock(&shared).state.record_recent(Recent {
            world_slug: "leafcull-isle".into(),
            account: "player".into(),
            client: ClientKind::Dereth,
            dat_set_id: None,
            classic_set_id: None,
            last_played: 1,
        });
        let slugs = || -> Vec<String> {
            lock(&shared)
                .snapshot()
                .worlds
                .into_iter()
                .map(|w| w.slug)
                .collect()
        };
        let remembered = || -> Vec<String> {
            let b = lock(&shared);
            b.state
                .recent
                .iter()
                .map(|r| r.world_slug.clone())
                .collect()
        };
        assert_eq!(
            slugs(),
            ["leafcull-isle"],
            "the last copy, before any fetch"
        );

        let fetch = |hub: Result<String, &'static str>, community: Result<String, &'static str>| {
            let asked = Arc::new(Mutex::new(Vec::new()));
            let ask = asked.clone();
            Backend::refresh_with(&shared, move |url| {
                ask.lock().unwrap().push(url.to_owned());
                let answer = if url.starts_with(HUB_LIST_URL) {
                    hub.clone()
                } else {
                    community.clone()
                };
                answer
                    .map(String::into_bytes)
                    .map_err(|e| format!("{url}: {e}"))
            })
            .join()
            .unwrap();
            let asked = asked.lock().unwrap().clone();
            asked
        };

        let asked = fetch(Ok(hub_page.clone()), Ok(list_of(&["Eulmore"])));
        assert_eq!(asked.len(), 1, "{asked:?}: the hub answered");
        assert!(asked[0].starts_with(HUB_LIST_URL), "{asked:?}");
        assert_eq!(
            slugs(),
            ["leafcull-isle"],
            "kept under its name, not the hub's slug"
        );
        assert_eq!(
            remembered(),
            ["leafcull-isle"],
            "and what was remembered with it"
        );
        assert_eq!(lock(&shared).list_state, ListState::Loaded);
        let copy = ListCache::load(&folders.data).unwrap();
        assert_eq!(
            copy.hub,
            std::slice::from_ref(&hub_page),
            "the hub's list is the day's copy"
        );

        let started = now();
        let asked = fetch(
            Err("http status: 502"),
            Ok(list_of(&["Leafcull Isle", "Eulmore"])),
        );
        assert_eq!(asked.len(), 2, "{asked:?}");
        assert!(asked[0].starts_with(HUB_LIST_URL), "{asked:?}");
        assert_eq!(asked[1], COMMUNITY_LIST_URL);
        assert_eq!(slugs(), ["leafcull-isle", "eulmore"]);
        assert_eq!(remembered(), ["leafcull-isle"]);
        assert_eq!(lock(&shared).list_state, ListState::Loaded);
        assert_eq!(lock(&shared).list_error, None);
        let copy = ListCache::load(&folders.data).unwrap();
        assert_eq!(
            copy.list,
            list_of(&["Leafcull Isle", "Eulmore"]),
            "the community's list is the day's copy"
        );
        assert!(copy.hub.is_empty() && copy.fetched_at >= started);

        fetch(Err("connection refused"), Err("connection refused"));
        assert_eq!(
            slugs(),
            ["leafcull-isle", "eulmore"],
            "the last copy's worlds stay"
        );
        assert_eq!(lock(&shared).list_state, ListState::Unavailable);
        let error = lock(&shared).list_error.clone().unwrap();
        assert!(
            error.contains(HUB_LIST_URL) && error.contains(COMMUNITY_LIST_URL),
            "{error}"
        );
        assert_eq!(
            ListCache::load(&folders.data).as_ref(),
            Some(&copy),
            "the copy is kept as it was"
        );

        // Lists that show no world are no answer either: nothing empty takes the place of the
        // worlds on screen or of the day's copy.
        let empty_hub = serde_json::json!({"data": [], "total": 0, "nextOffset": null}).to_string();
        fetch(Ok(empty_hub), Ok(list_of(&[])));
        assert_eq!(slugs(), ["leafcull-isle", "eulmore"]);
        assert_eq!(remembered(), ["leafcull-isle"]);
        assert_eq!(lock(&shared).list_state, ListState::Unavailable);
        assert_eq!(
            ListCache::load(&folders.data),
            Some(copy),
            "an empty list is never the day's copy"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn play_again_forgets_a_world_the_list_no_longer_names_but_not_on_an_empty_list() {
        let (mut b, dir) = backend("unlisted");
        for world in ["eulmore", "gone"] {
            b.state.record_recent(Recent {
                world_slug: world.into(),
                account: "player".into(),
                client: ClientKind::Dereth,
                dat_set_id: None,
                classic_set_id: None,
                last_played: 1,
            });
        }
        let all = b.worlds.clone();
        b.worlds.clear();
        b.forget_unlisted();
        assert_eq!(b.state.recent.len(), 2, "an empty list forgets nothing");
        b.worlds = all;
        b.forget_unlisted();
        assert_eq!(
            b.state
                .recent
                .iter()
                .map(|r| r.world_slug.as_str())
                .collect::<Vec<_>>(),
            ["eulmore"]
        );
        let (saved, _) = LauncherState::load(&dir.join("state"));
        assert_eq!(saved.recent.len(), 1, "and the history on disk with it");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The first start asks which interface the Dereth client plays in, until the answer is in the
    /// client's preferences; the launcher started again does not ask, and a file that already names
    /// one (the client's own choice) is never asked about.
    #[test]
    fn the_launcher_asks_for_the_interface_until_one_is_saved_in_the_clients_preferences() {
        let (mut b, dir) = backend("interface");
        let file = dir.join("settings").join("UserPreferences.ini");
        let view = b
            .snapshot()
            .interface
            .expect("the client keeps preferences");
        assert_eq!(view.chosen, None, "nothing saved: the page asks");
        assert_eq!(
            view.offers.iter().map(|o| o.name).collect::<Vec<_>>(),
            ["Horizon", "Modern", "Classic"]
        );
        assert_eq!(view.file, file.display().to_string());
        let json = serde_json::to_string(&b.snapshot()).unwrap();
        assert!(json.contains("\"interface\":{\"chosen\":null"), "{json}");

        assert!(b.set_interface("Retail").is_err());
        assert!(!file.exists(), "a refused answer writes nothing");
        b.set_interface("classic").unwrap();
        assert_eq!(b.snapshot().interface.unwrap().chosen, Some("Classic"));
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "[UI]\r\nInterface=Classic\r\n",
            "in the client's file, as the client writes it"
        );

        let again = Backend::new(
            Folders::single(dir.join("state")),
            Vec::new(),
            None,
            Box::new(MemoryVault::default()),
            "memory",
            Some(file.clone()),
        );
        assert_eq!(again.snapshot().interface.unwrap().chosen, Some("Classic"));

        // The client's own choice, saved in its options, shows the same way.
        std::fs::write(
            &file,
            "[Display]\r\nFullScreen=True\r\n[UI]\r\nInterface=Modern\r\n",
        )
        .unwrap();
        assert_eq!(b.snapshot().interface.unwrap().chosen, Some("Modern"));

        // With no preferences at all, nothing is asked.
        let none = Backend::new(
            Folders::single(dir.join("state")),
            Vec::new(),
            None,
            Box::new(MemoryVault::default()),
            "memory",
            None,
        );
        assert!(none.snapshot().interface.is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn snapshots_serialise_for_the_page() {
        let (b, dir) = backend("json");
        let json = serde_json::to_string(&b.snapshot()).unwrap();
        assert!(json.contains("\"slug\":\"eulmore\""));
        assert!(json.contains("\"list_state\":\"loading\""));
        let view = serde_json::to_string(&b.world_view("eulmore").unwrap()).unwrap();
        assert!(
            view.contains("\"kind\":\"dereth\"") && view.contains("\"default_client\":\"dereth\""),
            "{view}"
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}
