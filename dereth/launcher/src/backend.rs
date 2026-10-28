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
use dereth_launch::discord::Activity;
use dereth_launch::eras::{self, EraInfo, FeatureInfo};
use dereth_launch::folders::Folders;
use dereth_launch::install::{ClientKind, Installation};
use dereth_launch::interface::{self, Offer};
use dereth_launch::launch::{plan, LaunchRequest};
use dereth_launch::library::{self, FolderFind};
use dereth_launch::listing::WorldList;
use dereth_launch::plugins::{
    keep_tool, AutoUpdate, PluginError, PluginsView, Refresh, Shelf, TOOL_FILE,
};
use dereth_launch::serverlist::{ListCache, ListSource};
use dereth_launch::state::{Account, Favourite, LauncherState, Realm, Recent, WorldPrefs};
use dereth_launch::status::LiveStatus;
use dereth_launch::vault::{target, Vault};
#[cfg(test)]
use dereth_launch::world::WorldState;
use dereth_launch::world::{AccountModel, Emulator, StatusMethod, World};
use serde::{Deserialize, Serialize};

const STATUS_INTERVAL: Duration = Duration::from_secs(60);

/// How many servers are asked whether they are up at once.
const PROBES_AT_ONCE: usize = 8;

/// Ask a server whether it is up with the server-tracker login ([`dereth_launch::probe`]): three
/// tries, 700 ms apiece. Any well-formed packet back from its address is a yes.
pub(crate) fn probe_host(host: &str, port: u16) -> bool {
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
pub(crate) fn ping_status(host: &str, port: u16) -> Option<LiveStatus> {
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
    http_get_bounded(url, 1 << 20, Duration::from_secs(8))
}

/// One HTTPS GET of at most `limit` bytes within `timeout`.
fn http_get_bounded(url: &str, limit: u64, timeout: Duration) -> Result<Vec<u8>, String> {
    if !url.to_ascii_lowercase().starts_with("https://") {
        return Err(format!("{url}: only https is accepted"));
    }
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .build()
        .into();
    let mut resp = agent.get(url).call().map_err(|e| format!("{url}: {e}"))?;
    resp.body_mut()
        .with_config()
        .limit(limit)
        .read_to_vec()
        .map_err(|e| format!("{url}: {e}"))
}

/// How the plugins' downloads are made: one HTTPS GET of at most `limit` bytes, the bytes or why
/// not in words for the page. The launcher's is [`fetch_plugin_file`]; a test's serves its own.
pub type Fetch<'a> = dyn Fn(&str, usize) -> Result<Vec<u8>, String> + Sync + 'a;

/// One HTTPS GET for the plugins: a source's list or a plugin's zip, at most `limit` bytes. Plain
/// HTTP is refused.
pub fn fetch_plugin_file(url: &str, limit: usize) -> Result<Vec<u8>, String> {
    use std::io::Read as _;
    if !url.to_ascii_lowercase().starts_with("https://") {
        return Err(format!("{url} is not an https:// address"));
    }
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .https_only(true)
        .timeout_connect(Some(Duration::from_secs(20)))
        .timeout_global(Some(Duration::from_secs(300)))
        .build()
        .into();
    let mut resp = agent
        .get(url)
        .header("User-Agent", "Dereth-launcher")
        .call()
        .map_err(|e| match e {
            ureq::Error::StatusCode(404) => format!("{url} was not found (404)"),
            ureq::Error::StatusCode(code) => format!("{url} answered with an error ({code})"),
            ureq::Error::Io(_)
            | ureq::Error::Timeout(_)
            | ureq::Error::HostNotFound
            | ureq::Error::ConnectionFailed
            | ureq::Error::Tls(_) => {
                format!("{url} could not be reached; check your internet connection")
            }
            other => format!("{url}: {other}"),
        })?;
    let mut bytes = Vec::new();
    resp.body_mut()
        .with_config()
        .limit(limit as u64 + 1)
        .reader()
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| format!("{url}: the download stopped part way"))?;
    if bytes.len() > limit {
        return Err(format!("{url} is larger than its source says"));
    }
    Ok(bytes)
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
    /// What Discord shows while it runs: its world, client and era, from when it started.
    activity: Activity,
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
    /// The Plugins page: the sources, what they list and what is installed for the Dereth client.
    pub plugins: PluginsView,
}

/// A plugin the Plugins page made: the repository's folder, the plugin's own folder in it, and
/// what was made, in the words the author's tool says it in.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MadePlugin {
    pub folder: PathBuf,
    pub plugin: String,
    pub said: String,
}

/// What the template is filled in from for a plugin the Plugins page makes: its name, the GitHub
/// repository when one is given, and this launcher's Dereth release, as the author's tool of the
/// same release fills it from its own.
fn new_plugin<'a>(name: &'a str, github: Option<&'a str>) -> dereth_plugin_template::Options<'a> {
    dereth_plugin_template::Options {
        id: None,
        name: Some(name),
        github: github.map(str::trim).filter(|g| !g.is_empty()),
        dereth: env!("CARGO_PKG_VERSION"),
    }
}

/// Make a plugin called `name` in the folder `dir`, as the author's tool's `dereth-plugin new`
/// does, for this launcher's Dereth release, with links and workflows for the GitHub repository
/// `github` (`<username>/<repo>`) when there is one: a new plugin repository in a missing or empty
/// folder, or the plugin's own folder added to a folder that is a plugin repository already.
///
/// # Errors
/// Why not, in words for the page.
pub fn create_plugin(dir: &Path, name: &str, github: Option<&str>) -> Result<MadePlugin, String> {
    let made =
        dereth_plugin_template::new(dir, &new_plugin(name, github)).map_err(new_plugin_error)?;
    Ok(MadePlugin {
        folder: dir.to_owned(),
        said: made.said(dir),
        plugin: made.plugin,
    })
}

/// Whether [`create_plugin`] can make a plugin of `name` and `github`, before the player is asked
/// for a folder.
///
/// # Errors
/// Why not, in words for the page.
pub fn check_new_plugin(name: &str, github: Option<&str>) -> Result<(), String> {
    dereth_plugin_template::values(Path::new(""), &new_plugin(name, github))
        .map(drop)
        .map_err(new_plugin_error)
}

/// Why a plugin could not be made, in the page's words.
fn new_plugin_error(e: dereth_plugin_template::Error) -> String {
    use dereth_plugin_template::Error;
    match e {
        Error::Name(n) if n.is_empty() => "Give the plugin a name.".to_owned(),
        Error::Name(_) => "A plugin's name can't have quotes or backslashes.".to_owned(),
        Error::NoLetter(_) => "Start the name with a letter.".to_owned(),
        Error::Github(_) | Error::NotAnId { .. } | Error::Reserved(_) => {
            "Give the GitHub repository as username/repo.".to_owned()
        }
        Error::NotEmpty(_) => "Choose an empty folder or a plugin repository.".to_owned(),
        Error::InAPlugin(_) => "That is a plugin's own folder. Choose its repository.".to_owned(),
        Error::FolderTaken(dir) => format!(
            "The repository has a {} folder already. Give the plugin another name.",
            dir.file_name().unwrap_or_default().to_string_lossy()
        ),
        Error::IdTaken { id, .. } => format!("A plugin in the repository is {id} already."),
        Error::NoId(path) => format!("{} does not say its plugin's id.", path.display()),
        Error::Io(path, e) => format!("Could not write {}: {e}", path.display()),
    }
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
    state: LauncherState,
    dereth: Option<Installation>,
    /// The world list, read from its sources in order, its day's copy, and the hub's word on its
    /// worlds, read again about once a minute.
    list: WorldList,
    list_state: ListState,
    list_error: Option<String>,
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
    plugins: Shelf,
    /// The plugin author's tool this launcher ships with, which it keeps a copy of in the folders
    /// Make Your Own wrote in; `None` when there is none to copy.
    tool: Option<PathBuf>,
}

impl std::fmt::Debug for Backend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Backend")
            .field("folders", &self.folders)
            .field("worlds", &self.list.worlds().len())
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
        let list = WorldList::new(list_sources, ListCache::load(&folders.data));
        let plugins = Shelf::open(folders.plugins());
        Self {
            plugins,
            folders,
            state,
            dereth,
            list,
            list_state: ListState::Loading,
            list_error: None,
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
            tool: None,
        }
    }

    /// The plugin author's tool the launcher ships with, at `tool`, which it keeps a copy of in
    /// the folders Make Your Own writes in.
    #[must_use]
    pub fn with_tool(mut self, tool: Option<PathBuf>) -> Self {
        self.tool = tool;
        self
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

    /// Forget Play Again's entries for worlds the list read last no longer names. Only after a list
    /// was read whole: a failed fetch, or one that came back empty, forgets nothing.
    fn forget_unlisted(&mut self) {
        if self.list.worlds().is_empty() {
            return;
        }
        let listed: HashSet<&str> = self.list.worlds().iter().map(|w| w.slug.as_str()).collect();
        if self.state.forget_unlisted(&listed) {
            self.save();
        }
    }

    /// Every world: the list's, then the servers the player added, each with what it said of
    /// itself, else the hub's word, else whether it answered the server-tracker login.
    fn all_worlds(&self) -> impl Iterator<Item = World> + '_ {
        self.list.shown(&self.state, now(), |slug| {
            (self.live.get(slug), self.probed.get(slug).copied())
        })
    }

    /// The slugs of the worlds the Dereth Network's hub lists, as the list in use has them; none
    /// while the list is another's.
    pub fn hub_slugs(&self) -> HashSet<String> {
        self.list
            .worlds()
            .iter()
            .filter_map(|w| w.hub_slug.clone())
            .collect()
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            worlds: self.all_worlds().collect(),
            list_state: self.list_state.clone(),
            list_error: self.list_error.clone(),
            list_fetched_at: self.list.copy().map(|c| c.fetched_at),
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
            plugins: self.plugins.view(),
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

    // ----- Discord -------------------------------------------------------------------------------

    /// What Discord should show now ([`crate::discord`]): the last game started that is still
    /// running, or nothing when none is or the player turned the setting off.
    pub fn presence(&self) -> Option<Activity> {
        if !self.state.settings.discord_presence {
            return None;
        }
        self.running.last().map(|r| r.activity.clone())
    }

    /// "Show what I'm playing on Discord", on or off. Off clears what Discord shows within a
    /// second.
    pub fn set_discord_presence(&mut self, on: bool) {
        self.state.settings.discord_presence = on;
        self.save();
    }

    /// The game the page shows ([`Realm`]), kept so the launcher opens in it again.
    pub fn set_realm(&mut self, realm: Realm) {
        self.state.settings.realm = realm;
        self.save();
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
            let fresh = !b.list.wants_reading(now());
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
    /// stay on screen. The hub's list, fetched, also brings each world's status as the hub has it
    /// now.
    pub fn refresh(shared: &Shared) {
        Self::refresh_with(shared, http_get);
    }

    /// [`Self::refresh`], fetching each address with `get`. Answers the thread that fetches.
    fn refresh_with<G>(shared: &Shared, mut get: G) -> std::thread::JoinHandle<()>
    where
        G: FnMut(&str) -> Result<Vec<u8>, String> + Send + 'static,
    {
        let mut fetch = {
            let mut b = lock(shared);
            b.list_state = ListState::Loading;
            b.list.reading()
        };
        let shared = shared.clone();
        std::thread::spawn(move || {
            while let Some(url) = fetch.next_url().map(str::to_owned) {
                fetch.take(get(&url));
            }
            let mut guard = lock(&shared);
            let b = &mut *guard;
            match b.list.read(fetch, now()) {
                Ok(copy) => {
                    if let Err(e) = copy.save(&b.folders.data) {
                        eprintln!("dereth-launcher: the world list's copy was not saved: {e}");
                    }
                    b.list_state = ListState::Loaded;
                    b.list_error = None;
                    b.forget_unlisted();
                }
                Err(e) => {
                    b.list_state = ListState::Unavailable;
                    b.list_error = Some(e);
                }
            }
            drop(guard);
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

    /// Read the hub's list for each world's status when a read is due at `at` (seconds since the
    /// epoch; `HubWatch::start`): on a thread, each page fetched with `get`. Answers the thread,
    /// when a read started.
    fn read_hub_status_at<G>(
        shared: &Shared,
        at: u64,
        mut get: G,
    ) -> Option<std::thread::JoinHandle<()>>
    where
        G: FnMut(&str) -> Result<Vec<u8>, String> + Send + 'static,
    {
        let mut read = lock(shared).list.hub_mut().start(at)?;
        let shared = shared.clone();
        Some(std::thread::spawn(move || {
            while let Some(url) = read.next_url().map(str::to_owned) {
                read.take(get(&url));
            }
            lock(&shared).list.hub_mut().finish(read, at);
        }))
    }

    /// Once a second: read the hub's list for each world's status when it is due, ask for each
    /// Empyrean world's status when it is due (its status document, or the status ping for a world
    /// that answered it), and watch the clients.
    pub fn tick(shared: &Shared) {
        Self::read_hub_status_at(shared, now(), http_get);
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
            b.list
                .worlds()
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
        let hosted =
            match crate::host_publication::play_inputs(&self.folders.data, &choice.world_slug) {
                Ok(hosted) => hosted,
                Err(error) => return err(error),
            };
        if hosted.as_ref().is_some_and(|h| !h.args.is_empty()) && !dereth {
            return err("This published world needs the Dereth client.".into());
        }
        // A world on the Dereth client's overlay blocklist would be refused as it starts: it is
        // not started.
        let blocked = hosted
            .as_ref()
            .and_then(|h| h.world_key.as_deref())
            .zip(self.client_preferences.as_deref())
            .is_some_and(|(key, prefs)| dereth_launch::interface::refuses_world(prefs, key));
        if dereth && blocked {
            return err("This world is on your overlay blocklist, so it can't be played.".into());
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
        if hosted.as_ref().is_some_and(|h| {
            world
                .endpoint
                .as_ref()
                .is_none_or(|e| e.address != h.host || e.port != h.port)
        }) {
            return err(
                "This world's address changed. Press Play on its page in Host a world again."
                    .into(),
            );
        }
        let extra_args = hosted.as_ref().map_or_else(Vec::new, |h| h.args.clone());
        let (dat_dir, classic_dat_dir) = if let Some(hosted) = hosted.filter(|_| dereth) {
            let modern = path_of(dat_set_id.as_deref(), SetKind::Modern);
            let classic = path_of(classic_set_id.as_deref(), SetKind::Classic);
            match hosted.client_dats(modern.as_deref(), classic.as_deref()) {
                Ok(d) => (Some(d.dat_dir), d.classic_dat_dir),
                Err(e) => return err(e),
            }
        } else if dereth {
            let modern = path_of(dat_set_id.as_deref(), SetKind::Modern);
            let classic = path_of(classic_set_id.as_deref(), SetKind::Classic);
            match choices::dat_dirs_for(&world, modern.as_deref(), classic.as_deref()) {
                Ok(d) => (Some(d.dat_dir), d.classic_dat_dir),
                Err(e) => return err(e.to_string()),
            }
        } else {
            (None, None)
        };
        // A policy written in the world's entry goes to the client as a file; one at a URL is
        // passed as the URL.
        let plugin_policy_file = world
            .plugin_policy
            .as_deref()
            .filter(|_| world.plugin_policy_url().is_none())
            .and_then(|doc| {
                let f =
                    std::env::temp_dir().join(format!("dereth-plugin-policy-{}.toml", world.slug));
                std::fs::write(&f, doc).ok().map(|()| f)
            });
        let req = LaunchRequest {
            world: &world,
            install: &install,
            account: &choice.account,
            dat_dir,
            classic_dat_dir,
            extra_args: &extra_args,
            plugin_policy_file,
            plugins: self.offered_plugins(),
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
            // The launcher never learns which character is played, so the activity names none.
            activity: Activity::new(&world, install.kind, None, now()),
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

    /// Add a server by hand, with the rules and the emulator the player says it has. Its era is
    /// what it says when asked, else the end of retail.
    pub fn add_custom_world(&mut self, s: &NewServer) -> Result<String, String> {
        let slug = self
            .state
            .add_custom_world(&s.name, &s.host, &s.port, s.ruleset.as_deref(), s.emulator)
            .map_err(|e| e.to_string())?;
        self.save();
        Ok(slug)
    }

    pub fn open_host_world(&mut self, dir: &Path) -> Result<String, String> {
        let definition = crate::host_runtime::read_definition(dir)?;
        let settings = crate::host_config::Settings::from_config(
            &std::fs::read_to_string(dir.join("empyrean.toml")).map_err(|e| e.to_string())?,
        )?;
        let host = if settings.bind == "0.0.0.0" {
            "127.0.0.1".into()
        } else {
            settings.bind
        };
        let slug = self.add_custom_world(&NewServer {
            name: settings.name,
            host,
            port: settings.port.to_string(),
            ruleset: None,
            emulator: Emulator::Empyrean,
        })?;
        let find = self.read_folder(&settings.dats);
        self.add_folder(find);
        crate::host_publication::register_player(&self.folders.data, &definition.id, &slug)?;
        Ok(slug)
    }

    /// Remove a server the player added, and every password kept for it.
    pub fn remove_custom_world(&mut self, slug: &str) {
        if let Err(error) = crate::host_publication::forget_player(&self.folders.data, slug) {
            self.say(error, true);
            return;
        }
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

    // ----- plugins -------------------------------------------------------------------------------

    /// Refresh: read every source's list again, then apply the updates the setting lets apply on
    /// their own (a thread's work: it fetches). Answers what is new, a line each: the plugins a
    /// source lists now that it did not, the updates applied and waiting, and the sources that
    /// could not be read. Empty when nothing is.
    pub fn refresh_plugins(shared: &Shared, fetch: &Fetch<'_>) -> String {
        Self::walk_plugins(shared, fetch).said(&lock(shared).plugins)
    }

    /// Every source read, and the updates the setting allows applied: the walk, done.
    fn walk_plugins(shared: &Shared, fetch: &Fetch<'_>) -> Refresh {
        let mut walk = Refresh::new(&lock(shared).plugins);
        let mut fetched = None;
        loop {
            // The lock is let go while each download is made.
            let next = walk.next(&mut lock(shared).plugins, fetched.take(), now());
            let Some(download) = next else { break };
            fetched = Some(fetch(&download.url, download.limit));
        }
        walk
    }

    /// At start: [`Backend::refresh_plugins`], and what is new in the notice. A source that could
    /// not be read says so on its own row, not in the notice: started with no connection, the
    /// launcher would otherwise say so each time.
    pub fn refresh_plugins_at_start(shared: &Shared, fetch: &Fetch<'_>) {
        let said = Self::walk_plugins(shared, fetch).said_at_start(&lock(shared).plugins);
        if !said.is_empty() {
            lock(shared).say(format!("Plugins: {}", said.replace('\n', " ")), false);
        }
    }

    /// Add the source at `url`, which the page has the player confirm first, and read its list
    /// (a thread's work).
    ///
    /// # Errors
    /// The address was refused, in words for the page.
    pub fn add_plugin_source(
        shared: &Shared,
        fetch: &Fetch<'_>,
        url: &str,
    ) -> Result<String, String> {
        let url = lock(shared)
            .plugins
            .add_source(url)
            .map_err(|e| e.to_string())?;
        let fetched = fetch(&url, dereth_launch::plugins::MAX_LIST);
        let mut b = lock(shared);
        b.plugins
            .record_read(&url, fetched, now())
            .map_err(|e| e.to_string())?;
        let s = b.plugins.source(&url).ok_or("the source went away")?;
        Ok(match &s.error {
            Some(e) => format!("Added {url}, but its list could not be read: {e}"),
            None => match s.catalog.plugins.len() {
                1 => format!("Added {}: 1 plugin.", s.label()),
                n => format!("Added {}: {n} plugins.", s.label()),
            },
        })
    }

    /// Remove the source at `url`; what was installed from it stays, with no updates.
    ///
    /// # Errors
    /// Dereth's own source, or not a source.
    pub fn remove_plugin_source(&mut self, url: &str) -> Result<(), String> {
        self.plugins.remove_source(url).map_err(|e| e.to_string())
    }

    /// Install plugin `id` from `source` (a thread's work: it downloads the zip).
    ///
    /// # Errors
    /// The download, or a check the release failed, in words for the page.
    pub fn install_plugin(
        shared: &Shared,
        fetch: &Fetch<'_>,
        source: &str,
        id: &str,
    ) -> Result<String, String> {
        let plan = lock(shared)
            .plugins
            .plan_install(source, id)
            .map_err(|e| e.to_string())?;
        let zip = fetch(&plan.url, plan.limit)?;
        let done = lock(shared)
            .plugins
            .install(source, id, &zip)
            .map_err(|e| e.to_string())?;
        Ok(done.said())
    }

    /// Update plugin `id` from its own source (a thread's work: it downloads the zip).
    ///
    /// # Errors
    /// Nothing newer, the download, or a check the release failed.
    pub fn update_plugin(shared: &Shared, fetch: &Fetch<'_>, id: &str) -> Result<String, String> {
        let plan = lock(shared)
            .plugins
            .plan_update(id)
            .map_err(|e| e.to_string())?;
        let zip = fetch(&plan.url, plan.limit)?;
        let done = lock(shared)
            .plugins
            .update(id, &zip)
            .map_err(|e| e.to_string())?;
        Ok(done.said())
    }

    /// Remove `id`.
    ///
    /// # Errors
    /// What stood in the way, in words for the page.
    pub fn remove_plugin(&mut self, id: &str) -> Result<(), String> {
        self.plugins.remove(id).map_err(|e| e.to_string())
    }

    /// Which plugins update without the player: `none`, `all` or `first_party`.
    ///
    /// # Errors
    /// Not one of those.
    pub fn set_plugin_updates(&mut self, setting: &str) -> Result<(), String> {
        let setting = AutoUpdate::parse(setting)
            .ok_or_else(|| format!("{setting} is not an update setting"))?;
        self.plugins
            .set_auto_update(setting)
            .map_err(|e| e.to_string())
    }

    /// Add `folder`, one plugin's folder or a plugin repository (the folders directly in it hold
    /// plugins), to the folders on this computer whose plugins the client is offered as they are
    /// on disk. The page adds the folder the player chose at once.
    ///
    /// # Errors
    /// It is neither, or is on the list already, in words for the page.
    pub fn add_plugin_folder(&mut self, folder: &Path) -> Result<String, String> {
        let found = self.plugins.add_local(folder).map_err(|e| match e {
            PluginError::NoPluginIn(_) => {
                "That folder has no plugin.toml, and neither do the folders in it.".to_owned()
            }
            PluginError::FolderListed(_) => "That folder is on the list already.".to_owned(),
            e => e.to_string(),
        })?;
        Ok(match found {
            1 => format!("Added 1 plugin from {}.", folder.display()),
            n => format!("Added {n} plugins from {}.", folder.display()),
        })
    }

    /// Take `folder` off the folders on this computer. Nothing in it is deleted.
    ///
    /// # Errors
    /// It is not on the list.
    pub fn remove_plugin_folder(&mut self, folder: &Path) -> Result<(), String> {
        self.plugins.remove_local(folder).map_err(|e| e.to_string())
    }

    /// Make Your Own: [`create_plugin`], and the folder it wrote in put on the folders on this
    /// computer, so the new plugin is offered to the client at once, as one the launcher keeps
    /// the author's tool in: a copy of the tool it ships with goes at the folder's root, and stays
    /// that tool ([`Backend::keep_tools_current`]).
    ///
    /// # Errors
    /// As [`create_plugin`].
    pub fn make_plugin(
        &mut self,
        dir: &Path,
        name: &str,
        github: Option<&str>,
    ) -> Result<MadePlugin, String> {
        let made = create_plugin(dir, name, github)?;
        self.plugins.add_own(dir).map_err(|e| e.to_string())?;
        if let Some(bytes) = self.tool.as_deref().and_then(|t| std::fs::read(t).ok()) {
            let kept = keep_tool(dir, TOOL_FILE, &bytes).is_ok();
            self.plugins.tool_failed.retain(|f| f != dir);
            if !kept {
                self.plugins.tool_failed.push(dir.to_owned());
            }
        }
        Ok(made)
    }

    /// At start: the author's tool's copy in each folder Make Your Own wrote in made the tool this
    /// launcher ships with again, which after an update is the new release's (a thread's work: it
    /// reads and writes those folders). A copy that is that tool already is left alone, a folder
    /// that is gone is passed over, and one whose copy could not be written says so on its row.
    pub fn keep_tools_current(shared: &Shared) {
        let (tool, folders) = {
            let b = lock(shared);
            (b.tool.clone(), b.plugins.tool_folders.clone())
        };
        let Some(bytes) = tool.and_then(|t| std::fs::read(t).ok()) else {
            return;
        };
        // The lock is let go while the folders are written.
        let done: Vec<(PathBuf, bool)> = folders
            .into_iter()
            .filter(|f| f.is_dir())
            .map(|f| {
                let kept = keep_tool(&f, TOOL_FILE, &bytes).is_ok();
                (f, kept)
            })
            .collect();
        let mut b = lock(shared);
        for (folder, kept) in done {
            b.plugins.tool_failed.retain(|f| *f != folder);
            if !kept && b.plugins.tool_folders.contains(&folder) {
                b.plugins.tool_failed.push(folder);
            }
        }
    }

    /// What the Dereth client is offered when the launcher starts it: the plugins in the folders
    /// on this computer and the installed plugins.
    fn offered_plugins(&self) -> Vec<PathBuf> {
        self.plugins.offered_packages()
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
    //! Behaviour: none (the desktop launcher's backend: worlds, their status, the list, PLAY).
    use super::*;
    use dereth_launch::plugins as launcher_plugins;
    use dereth_launch::serverlist;
    use dereth_launch::testing::write_dat_set;
    use dereth_launch::vault::MemoryVault;
    use dereth_launch::world::{Emulator, Endpoint, Told};
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
        *b.list.worlds_mut() = vec![w];
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

    /// The plugins' downloads, served from memory: each address's bytes, and every address asked.
    struct Served {
        files: HashMap<String, Vec<u8>>,
        asked: Mutex<Vec<String>>,
    }

    impl Served {
        fn new(files: &[(&str, Vec<u8>)]) -> Self {
            Self {
                files: files
                    .iter()
                    .map(|(u, b)| ((*u).to_owned(), b.clone()))
                    .collect(),
                asked: Mutex::new(Vec::new()),
            }
        }

        fn fetch(&self, url: &str, limit: usize) -> Result<Vec<u8>, String> {
            self.asked.lock().unwrap().push(url.to_owned());
            let bytes = self
                .files
                .get(url)
                .cloned()
                .ok_or_else(|| format!("{url} was not found (404)"))?;
            if bytes.len() > limit {
                return Err(format!("{url} is larger than its source says"));
            }
            Ok(bytes)
        }
    }

    const ZIP_AT: &str = "https://plugins.dereth.network/";

    /// Plugins are on with no switch: a new launcher's page has its Plugins section, which shows
    /// what is installed, and the client is offered every plugin installed, with nothing more to
    /// turn on.
    #[test]
    fn the_plugins_section_is_there_and_the_client_is_offered_the_installed_plugins() {
        use dereth_launch::testing::{plugin_list, plugin_zip};
        let (mut b, dir) = backend("plugins-on");
        let id = "network.dereth.counter";
        let zip = plugin_zip(id, "1.0.0", "0.1", "");
        assert!(b.snapshot().plugins.installed.is_empty());
        assert!(b.offered_plugins().is_empty(), "nothing installed yet");
        b.plugins
            .record_read(launcher_plugins::FIRST_PARTY, Ok(plugin_list(&[&zip])), 1)
            .unwrap();
        b.plugins
            .install(launcher_plugins::FIRST_PARTY, id, &zip)
            .unwrap();

        let view = b.snapshot().plugins;
        assert_eq!(view.installed.len(), 1);
        assert_eq!(b.offered_plugins(), b.plugins.offered_packages());
        assert_eq!(b.offered_plugins().len(), 1);
        b.remove_plugin(id).expect("removed");
        assert!(b.offered_plugins().is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Every file under `dir`, by its `/`-separated path there, and its bytes.
    fn files_under(dir: &Path) -> std::collections::BTreeMap<String, Vec<u8>> {
        let mut out = std::collections::BTreeMap::new();
        let mut stack = vec![PathBuf::new()];
        while let Some(rel) = stack.pop() {
            for entry in std::fs::read_dir(dir.join(&rel)).unwrap() {
                let entry = entry.unwrap();
                let path = rel.join(entry.file_name());
                if entry.file_type().unwrap().is_dir() {
                    stack.push(path);
                } else {
                    let key = path
                        .components()
                        .map(|c| c.as_os_str().to_string_lossy().into_owned())
                        .collect::<Vec<_>>()
                        .join("/");
                    out.insert(key, std::fs::read(dir.join(&path)).unwrap());
                }
            }
        }
        out
    }

    /// The Plugins page's Make Your Own writes what `dereth-plugin new <folder> --name <name> --github
    /// <username>/<repo>` of the same Dereth release writes, byte for byte, and says what it says:
    /// the tool's release and this launcher's are one version, and the workflows install that
    /// release's tool. It writes a whole repository only into an empty folder.
    #[test]
    fn creating_a_plugin_writes_what_the_authors_tool_writes_and_only_into_an_empty_folder() {
        let (_b, dir) = backend("plugin-create");
        let version = env!("CARGO_PKG_VERSION");

        let made = dir.join("made");
        let said = format!(
            "Added xp-tracker/ to plugin repository at {}.",
            made.display()
        );
        assert_eq!(
            create_plugin(&made, "XP Tracker", Some(" Someone/xp-tracker ")),
            Ok(MadePlugin {
                folder: made.clone(),
                plugin: "xp-tracker".into(),
                said,
            })
        );
        // The author's tool carries Dereth's version, as the launcher does, and fills the
        // template with it; its `new` with these words is this.
        let tool = include_str!("../../../tools/plugin/Cargo.toml");
        assert!(
            tool.contains(&format!("\nversion = \"{version}\"\n")),
            "the tool is of this launcher's release"
        );
        let by_tool = dir.join("by-tool");
        dereth_plugin_template::new(
            &by_tool,
            &dereth_plugin_template::Options {
                id: None,
                name: Some("XP Tracker"),
                github: Some("Someone/xp-tracker"),
                dereth: version,
            },
        )
        .unwrap();
        let written = files_under(&made);
        assert_eq!(written, files_under(&by_tool));
        assert!(
            written.contains_key("xp-tracker/plugin.toml"),
            "{written:?}"
        );
        for workflow in ["check.yml", "release.yml"] {
            let text = String::from_utf8_lossy(&written[&format!(".github/workflows/{workflow}")])
                .into_owned();
            assert!(text.contains(&format!("DERETH: '{version}'")), "{text}");
        }
        let manifest = String::from_utf8_lossy(&written["xp-tracker/plugin.toml"]).into_owned();
        assert!(
            manifest.contains("id = \"io.github.someone.xp-tracker\""),
            "{manifest}"
        );

        // Without a repository it is the template's placeholders for one.
        let bare = dir.join("bare");
        create_plugin(&bare, "XP Tracker", Some("")).unwrap();
        let manifest = std::fs::read_to_string(bare.join("xp-tracker/plugin.toml")).unwrap();
        assert!(
            manifest.contains("id = \"example.xp-tracker\""),
            "{manifest}"
        );

        // A folder that holds anything is left as it is.
        let held = dir.join("held");
        std::fs::create_dir_all(&held).unwrap();
        std::fs::write(held.join("notes.txt"), "mine").unwrap();
        assert_eq!(
            create_plugin(&held, "XP Tracker", None),
            Err("Choose an empty folder or a plugin repository.".to_owned())
        );
        assert_eq!(files_under(&held).len(), 1);

        // The name and the repository are checked before the player is asked for a folder.
        assert!(check_new_plugin("XP Tracker", Some("Someone/xp")).is_ok());
        assert_eq!(
            check_new_plugin(" ", None),
            Err("Give the plugin a name.".to_owned())
        );
        assert_eq!(
            check_new_plugin("123", None),
            Err("Start the name with a letter.".to_owned())
        );
        assert_eq!(
            check_new_plugin("XP", Some("someone")),
            Err("Give the GitHub repository as username/repo.".to_owned())
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Make Your Own in a folder that is a plugin repository already adds only the new plugin's folder,
    /// as the author's tool's `new` does there, byte for byte, and says what it says. A folder
    /// name or an id the repository has is refused in the page's words, and nothing is written.
    #[test]
    fn building_into_a_plugin_repository_adds_only_the_plugins_folder_as_the_tool_does() {
        let (_b, dir) = backend("plugin-add");
        let version = env!("CARGO_PKG_VERSION");
        let options = |name| dereth_plugin_template::Options {
            id: None,
            name: Some(name),
            github: Some("Someone/plugins"),
            dereth: version,
        };
        let (here, by_tool) = (dir.join("here"), dir.join("by-tool"));
        create_plugin(&here, "XP Tracker", Some("Someone/plugins")).unwrap();
        dereth_plugin_template::new(&by_tool, &options("XP Tracker")).unwrap();
        let before = files_under(&here);

        let added = create_plugin(&here, "Loot Helper", Some("Someone/plugins")).unwrap();
        let tool = dereth_plugin_template::new(&by_tool, &options("Loot Helper")).unwrap();
        assert_eq!(files_under(&here), files_under(&by_tool));
        let mut new_files: Vec<String> = files_under(&here)
            .into_keys()
            .filter(|k| !before.contains_key(k))
            .collect();
        new_files.sort();
        assert_eq!(
            new_files,
            [
                "loot-helper/CHANGES.md",
                "loot-helper/main.lua",
                "loot-helper/plugin.toml"
            ]
        );
        assert!(files_under(&here)
            .iter()
            .filter(|(k, _)| before.contains_key(*k))
            .all(|(k, v)| &before[k] == v));
        assert_eq!(added.plugin, "loot-helper");
        assert_eq!(added.said, tool.said(&here));
        assert_eq!(
            added.said,
            format!(
                "Added loot-helper/ to plugin repository at {}.",
                here.display()
            )
        );

        let now = files_under(&here);
        assert_eq!(
            create_plugin(&here, "Loot Helper", None),
            Err(
                "The repository has a loot-helper folder already. Give the plugin another name."
                    .to_owned()
            )
        );
        std::fs::rename(here.join("loot-helper"), here.join("loot")).unwrap();
        assert_eq!(
            create_plugin(&here, "Loot Helper", Some("Someone/plugins")),
            Err("A plugin in the repository is io.github.someone.loot-helper already.".to_owned())
        );
        std::fs::rename(here.join("loot"), here.join("loot-helper")).unwrap();
        assert_eq!(
            create_plugin(&here.join("xp-tracker"), "Nested", None),
            Err("That is a plugin's own folder. Choose its repository.".to_owned())
        );
        assert_eq!(files_under(&here), now, "nothing is written when refused");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The Plugins page's flow over a fake network: Dereth's own source is read, a plugin is
    /// installed from it, another source is added and read at once, and a check for
    /// updates applies Dereth's own plugin's update on its own while one that asks for more waits.
    #[test]
    fn plugins_install_from_sources_and_update_as_the_setting_says() {
        use dereth_launch::testing::{plugin_list, plugin_zip};
        let (b, dir) = backend("plugins-sources");
        let shared: Shared = Arc::new(Mutex::new(b));
        let own = "network.dereth.counter";
        let theirs = "org.example.meter";
        let other = "https://example.github.io/plugins/plugins.json";
        let other_at = "https://example.github.io/plugins/";
        let own1 = plugin_zip(own, "1.0.0", "0.1", "");
        let theirs1 = plugin_zip(theirs, "1.0.0", "0.1", "");
        let served = Served::new(&[
            (launcher_plugins::FIRST_PARTY, plugin_list(&[&own1])),
            (&format!("{ZIP_AT}{own}.zip"), own1.clone()),
            (other, plugin_list(&[&theirs1])),
            (&format!("{other_at}{theirs}.zip"), theirs1.clone()),
        ]);
        let fetch = |u: &str, l: usize| served.fetch(u, l);

        // Read, and offered: nothing is installed until the player asks.
        assert_eq!(Backend::refresh_plugins(&shared, &fetch), "");
        let view = lock(&shared).snapshot().plugins;
        assert_eq!(view.available.len(), 1);
        assert!(view.installed.is_empty());
        let said = Backend::install_plugin(&shared, &fetch, launcher_plugins::FIRST_PARTY, own);
        assert_eq!(said.as_deref(), Ok("Installed Meter 1.0.0."));
        assert_eq!(lock(&shared).offered_plugins().len(), 1);

        // Another source: added, and read at once.
        let added = Backend::add_plugin_source(&shared, &fetch, other);
        assert_eq!(added.as_deref(), Ok("Added example.github.io: 1 plugin."));
        Backend::install_plugin(&shared, &fetch, other, theirs).expect("installed");

        // Both sources move on. Dereth's own updates on its own; the other's asks for a
        // permission, so it waits whatever the setting.
        lock(&shared).set_plugin_updates("all").expect("set");
        let own2 = plugin_zip(own, "1.1.0", "0.1", "");
        let theirs2 = plugin_zip(
            theirs,
            "1.1.0",
            "0.1",
            "[permissions]\n\"auto.loot\" = \"Loots.\"\n",
        );
        let served = Served::new(&[
            (launcher_plugins::FIRST_PARTY, plugin_list(&[&own2])),
            (&format!("{ZIP_AT}{own}.zip"), own2.clone()),
            (other, plugin_list(&[&theirs2])),
            (&format!("{other_at}{theirs}.zip"), theirs2.clone()),
        ]);
        let fetch = |u: &str, l: usize| served.fetch(u, l);
        let said = Backend::refresh_plugins(&shared, &fetch);
        assert_eq!(said, "Updated Meter to 1.1.0.\n1 plugin update is waiting.");
        let view = lock(&shared).snapshot().plugins;
        let waiting = view
            .installed
            .iter()
            .find(|p| p.id == theirs)
            .and_then(|p| p.update.clone())
            .expect("waiting");
        assert_eq!(waiting.new_permissions, ["auto.loot"]);
        assert!(
            !served
                .asked
                .lock()
                .unwrap()
                .iter()
                .any(|u| u.ends_with(&format!("{theirs}.zip"))),
            "a release that waits is not downloaded"
        );
        // The player applies it.
        let said = Backend::update_plugin(&shared, &fetch, theirs);
        assert_eq!(said.as_deref(), Ok("Updated Meter to 1.1.0."));
        // A plugin a source lists now that it did not is named as new; nothing new is nothing.
        let fresh = plugin_zip("org.example.fresh", "1.0.0", "0.1", "");
        let served = Served::new(&[
            (launcher_plugins::FIRST_PARTY, plugin_list(&[&own2])),
            (other, plugin_list(&[&theirs2, &fresh])),
        ]);
        let fetch = |u: &str, l: usize| served.fetch(u, l);
        let said = Backend::refresh_plugins(&shared, &fetch);
        assert_eq!(said, "New from example.github.io: Meter.");
        assert_eq!(Backend::refresh_plugins(&shared, &fetch), "");
        // A source that cannot be read says so, and its plugins stay.
        let gone = Served::new(&[]);
        let fetch = |u: &str, l: usize| gone.fetch(u, l);
        let said = Backend::refresh_plugins(&shared, &fetch);
        assert!(said.contains("example.github.io: https://example.github.io/plugins/plugins.json was not found (404)"), "{said}");
        assert_eq!(lock(&shared).offered_plugins().len(), 2);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// At start every source is read and what is new is said in the notice, whatever the update
    /// setting: with None, an update installs nothing and the notice says it waits.
    #[test]
    fn at_start_the_updates_that_wait_are_told_whatever_the_setting() {
        use dereth_launch::testing::{plugin_list, plugin_zip};
        let (b, dir) = backend("plugins-at-start");
        let shared: Shared = Arc::new(Mutex::new(b));
        let own = "network.dereth.counter";
        let v1 = plugin_zip(own, "1.0.0", "0.1", "");
        let v2 = plugin_zip(own, "1.1.0", "0.1", "");
        {
            let mut b = lock(&shared);
            b.plugins
                .record_read(launcher_plugins::FIRST_PARTY, Ok(plugin_list(&[&v1])), 1)
                .unwrap();
            b.plugins
                .install(launcher_plugins::FIRST_PARTY, own, &v1)
                .unwrap();
            b.set_plugin_updates("none").expect("set");
        }
        let served = Served::new(&[
            (launcher_plugins::FIRST_PARTY, plugin_list(&[&v2])),
            (&format!("{ZIP_AT}{own}.zip"), v2.clone()),
        ]);
        let fetch = |u: &str, l: usize| served.fetch(u, l);
        Backend::refresh_plugins_at_start(&shared, &fetch);
        let b = lock(&shared);
        assert_eq!(
            b.plugins.get(own).map(|p| p.release.version.as_str()),
            Some("1.0.0"),
            "nothing installs on its own"
        );
        assert_eq!(
            b.snapshot().message.map(|m| m.text).as_deref(),
            Some("Plugins: 1 plugin update is waiting.")
        );
        drop(b);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Started with no connection, the launcher says nothing of its sources in the notice: each
    /// source's row says it could not be read. Refresh still says so.
    #[test]
    fn at_start_a_source_that_cannot_be_read_says_so_on_its_row_and_not_in_the_notice() {
        let (b, dir) = backend("plugins-offline");
        let shared: Shared = Arc::new(Mutex::new(b));
        let offline = Served::new(&[]);
        let fetch = |u: &str, l: usize| offline.fetch(u, l);
        Backend::refresh_plugins_at_start(&shared, &fetch);
        let snap = lock(&shared).snapshot();
        assert!(snap.message.is_none(), "{:?}", snap.message.map(|m| m.text));
        assert!(snap.plugins.sources[0]
            .error
            .as_deref()
            .is_some_and(|e| e.contains("was not found")));
        let said = Backend::refresh_plugins(&shared, &fetch);
        assert!(said.starts_with("Dereth: "), "{said}");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A folder on this computer is added and taken off by the page; its plugins are offered to
    /// the client as their folders, before the installed plugins, and taking it off deletes
    /// nothing.
    #[test]
    fn a_folder_on_this_computer_is_offered_to_the_client_and_taken_off_deleting_nothing() {
        use dereth_launch::testing::{plugin_list, plugin_zip, write_plugin_folder};
        let (mut b, dir) = backend("plugins-local");
        let id = "network.dereth.counter";
        let zip = plugin_zip(id, "1.0.0", "0.1", "");
        b.plugins
            .record_read(launcher_plugins::FIRST_PARTY, Ok(plugin_list(&[&zip])), 1)
            .unwrap();
        b.plugins
            .install(launcher_plugins::FIRST_PARTY, id, &zip)
            .unwrap();
        let repo = dir.join("repo");
        write_plugin_folder(&repo.join("counter"), id, "Counter", "1.1.0");
        write_plugin_folder(&repo.join("hello"), "org.example.hello", "Hello", "0.1.0");
        assert_eq!(
            b.add_plugin_folder(&repo).as_deref(),
            Ok(format!("Added 2 plugins from {}.", repo.display()).as_str())
        );
        assert_eq!(
            b.offered_plugins(),
            [repo.join("counter"), repo.join("hello")],
            "the installed one of the same id is not offered"
        );
        let view = b.snapshot().plugins;
        assert_eq!(view.local.len(), 2);
        assert!(view.installed[0].local_in_its_place);
        let before = files_under(&repo);
        b.remove_plugin_folder(&repo).expect("taken off");
        assert_eq!(files_under(&repo), before);
        assert_eq!(b.offered_plugins(), b.plugins.offered_packages());
        assert_eq!(b.offered_plugins().len(), 1, "the installed one again");
        assert!(b.remove_plugin_folder(&repo).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Adding a folder is one step, with no question first, and says what it added in one short
    /// line: a plugin's own folder, or a repository whose folders directly in it hold plugins. A
    /// folder whose plugins are all further down, or one on the list already, is refused in one
    /// short line of the page's, and is not listed.
    #[test]
    fn a_plugin_folder_is_added_in_one_step_and_one_holding_plugins_only_further_down_is_refused() {
        use dereth_launch::testing::write_plugin_folder;
        let (mut b, dir) = backend("plugins-local-add");
        let single = dir.join("hello");
        write_plugin_folder(&single, "org.example.hello", "Hello", "0.1.0");
        assert_eq!(
            b.add_plugin_folder(&single),
            Ok(format!("Added 1 plugin from {}.", single.display()))
        );
        let code = dir.join("code");
        write_plugin_folder(
            &code.join("plugins").join("meter"),
            "org.example.meter",
            "Meter",
            "0.1.0",
        );
        assert_eq!(
            b.add_plugin_folder(&code),
            Err("That folder has no plugin.toml, and neither do the folders in it.".to_owned())
        );
        assert_eq!(
            b.add_plugin_folder(&single),
            Err("That folder is on the list already.".to_owned())
        );
        assert_eq!(b.plugins.local, std::slice::from_ref(&single));
        assert_eq!(
            b.add_plugin_folder(&code.join("plugins")),
            Ok(format!(
                "Added 1 plugin from {}.",
                code.join("plugins").display()
            ))
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Make Your Own puts the folder it wrote in on the folders on this computer, once, so the
    /// new plugin is offered to the client at once.
    #[test]
    fn make_your_own_puts_its_folder_on_the_list_so_the_new_plugin_is_offered() {
        let (mut b, dir) = backend("plugins-build-local");
        let repo = dir.join("repo");
        let made = b
            .make_plugin(&repo, "XP Tracker", Some("Someone/plugins"))
            .unwrap();
        assert_eq!(
            made.said,
            format!(
                "Added xp-tracker/ to plugin repository at {}.",
                repo.display()
            )
        );
        assert_eq!(b.plugins.local, std::slice::from_ref(&repo));
        assert_eq!(b.offered_plugins(), [repo.join("xp-tracker")]);
        b.make_plugin(&repo, "Loot Helper", Some("Someone/plugins"))
            .unwrap();
        assert_eq!(b.plugins.local, std::slice::from_ref(&repo), "listed once");
        assert_eq!(
            b.offered_plugins(),
            [repo.join("loot-helper"), repo.join("xp-tracker")]
        );
        let names: Vec<String> = b
            .snapshot()
            .plugins
            .local
            .into_iter()
            .map(|l| l.name)
            .collect();
        assert_eq!(names, ["Loot Helper", "XP Tracker"]);
        // A folder refused is not listed.
        let held = dir.join("held");
        std::fs::create_dir_all(&held).unwrap();
        std::fs::write(held.join("notes.txt"), "mine").unwrap();
        assert!(b.make_plugin(&held, "XP Tracker", None).is_err());
        assert_eq!(b.plugins.local, [repo]);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Make Your Own puts a copy of the author's tool the launcher ships with at the root of the
    /// folder it wrote in, kept out of git there, and keeps it that tool: at start, a copy that is
    /// not the tool (after an update, the older release's) is written again. A folder Add Plugin
    /// Folder added gets none. A folder taken off the list keeps its copy, which is no longer
    /// written; a folder that is gone is passed over, and one whose copy cannot be written says so
    /// on its row until it can.
    #[test]
    fn make_your_own_keeps_the_launchers_tool_in_its_folder_and_add_plugin_folder_does_not() {
        let (b, dir) = backend("plugins-tool");
        let tool = dir.join("release").join(TOOL_FILE);
        std::fs::create_dir_all(tool.parent().unwrap()).unwrap();
        std::fs::write(&tool, "the tool, 1.0").unwrap();
        let shared: Shared = Arc::new(Mutex::new(b.with_tool(Some(tool.clone()))));
        let copy_in = |folder: &Path| std::fs::read_to_string(folder.join(TOOL_FILE)).ok();
        let ignored = |folder: &Path| std::fs::read_to_string(folder.join(".gitignore")).unwrap();
        let template_ignore = dereth_plugin_template::FILES
            .iter()
            .find(|(p, _)| *p == ".gitignore")
            .unwrap()
            .1;

        // Made new: the copy at its root, which the template's .gitignore keeps out already.
        let repo = dir.join("repo");
        lock(&shared)
            .make_plugin(&repo, "XP Tracker", Some("Someone/plugins"))
            .unwrap();
        assert_eq!(copy_in(&repo).as_deref(), Some("the tool, 1.0"));
        assert_eq!(ignored(&repo), template_ignore, "its lines once");

        // An older repository, whose .gitignore lacks the lines: added with Add Plugin Folder, no
        // copy, then or at start; Make Your Own adding a plugin to it puts one, and the lines.
        let older = dir.join("older");
        dereth_plugin_template::new(
            &older,
            &dereth_plugin_template::Options {
                id: None,
                name: Some("Meter"),
                github: None,
                dereth: "0.1.0",
            },
        )
        .unwrap();
        std::fs::write(older.join(".gitignore"), "/dist/\n/site/\n").unwrap();
        lock(&shared).add_plugin_folder(&older).unwrap();
        assert_eq!(copy_in(&older), None);
        Backend::keep_tools_current(&shared);
        assert_eq!(copy_in(&older), None, "not a folder it keeps the tool in");
        lock(&shared)
            .make_plugin(&older, "Loot Helper", None)
            .unwrap();
        assert_eq!(copy_in(&older).as_deref(), Some("the tool, 1.0"));
        let older_ignore = "/dist/\n/site/\n/dereth-plugin\n/dereth-plugin.exe\n";
        assert_eq!(ignored(&older), older_ignore);

        // An update brings the next release's tool: at start each copy is that one, and the
        // .gitignore files are as they were.
        std::fs::write(&tool, "the tool, 1.1").unwrap();
        Backend::keep_tools_current(&shared);
        for folder in [&repo, &older] {
            assert_eq!(
                copy_in(folder).as_deref(),
                Some("the tool, 1.1"),
                "{folder:?}"
            );
        }
        assert_eq!(ignored(&older), older_ignore);
        assert_eq!(ignored(&repo), template_ignore);

        // Taken off the list, a folder keeps its copy as it is. A folder gone is passed over, and
        // one whose copy cannot be written says so on its row.
        lock(&shared).remove_plugin_folder(&older).unwrap();
        let gone = dir.join("gone");
        let blocked = dir.join("blocked");
        for folder in [&gone, &blocked] {
            lock(&shared).make_plugin(folder, "Radar", None).unwrap();
        }
        std::fs::remove_dir_all(&gone).unwrap();
        std::fs::remove_file(blocked.join(TOOL_FILE)).unwrap();
        std::fs::create_dir(blocked.join(TOOL_FILE)).unwrap();
        std::fs::write(&tool, "the tool, 1.2").unwrap();
        Backend::keep_tools_current(&shared);
        assert_eq!(copy_in(&older).as_deref(), Some("the tool, 1.1"));
        assert_eq!(copy_in(&repo).as_deref(), Some("the tool, 1.2"));
        assert!(!gone.exists(), "nothing made where a folder went");
        let problem = |folder: &Path| {
            lock(&shared)
                .snapshot()
                .plugins
                .local
                .into_iter()
                .filter(|r| r.listed == folder.display().to_string())
                .find_map(|r| r.tool_problem)
        };
        assert_eq!(
            problem(&blocked).as_deref(),
            Some("Could not update dereth-plugin in this folder.")
        );
        assert_eq!((problem(&repo), problem(&gone)), (None, None));
        std::fs::remove_dir(blocked.join(TOOL_FILE)).unwrap();
        Backend::keep_tools_current(&shared);
        assert_eq!(copy_in(&blocked).as_deref(), Some("the tool, 1.2"));
        assert_eq!(problem(&blocked), None);

        // With no tool to copy, a folder Make Your Own wrote in is still one it keeps the tool in:
        // the first start that has one puts the copy there.
        lock(&shared).tool = None;
        let later = dir.join("later");
        lock(&shared).make_plugin(&later, "Bank", None).unwrap();
        assert_eq!(copy_in(&later), None);
        lock(&shared).tool = Some(tool);
        Backend::keep_tools_current(&shared);
        assert_eq!(copy_in(&later).as_deref(), Some("the tool, 1.2"));
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
            r#"{"name":"","host":"h","port":"9000","ruleset":null,"emulator":"classic_ace"}"#,
        )
        .unwrap();
        assert_eq!(form.emulator, Emulator::ClassicAce);
        assert_eq!(
            (v.world.era.as_deref(), v.world.era_source),
            (None, None),
            "it says no era: the end of retail"
        );
        assert_eq!(v.requires, SetKind::Modern);
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

    /// A world's era and systems are its own word, and a world that says none plays the end of
    /// retail; the page is given nothing to choose them with.
    #[test]
    fn a_world_plays_the_era_it_says_and_any_other_the_end_of_retail() {
        let (mut b, dir) = backend("eras");
        let mut w = World::new("leafcull", "Leafcull");
        w.emulator = Emulator::Ace;
        w.endpoint = b.list.worlds()[0].endpoint.clone();
        b.list.worlds_mut().push(w);

        // Leafcull says nothing: the end of retail, from the later files.
        let w = b.world("leafcull").unwrap();
        assert_eq!((w.era.as_deref(), w.era_source), (None, None));
        assert_eq!(b.world_view("leafcull").unwrap().requires, SetKind::Modern);

        // Eulmore's status says its era and systems.
        b.live.insert(
            "eulmore".into(),
            dereth_launch::status::parse_world_document(
                br#"{"world_open":true,"version":"0.1.2","era":"infiltration","features":{"trade":false}}"#,
            )
            .unwrap(),
        );
        let w = b.world("eulmore").unwrap();
        assert_eq!(
            (w.era.as_deref(), w.era_source, w.features_source),
            (Some("infiltration"), Some(Told::World), Some(Told::World))
        );
        assert_eq!(w.emulator_version.as_deref(), Some("0.1.2"));
        assert_eq!(b.world_view("eulmore").unwrap().requires, SetKind::Classic);

        // Saying its era but not its systems, it has that era's whole table.
        b.live.get_mut("eulmore").unwrap().era_features = None;
        let w = b.world("eulmore").unwrap();
        assert_eq!((w.era_features.as_deref(), w.features_source), (None, None));
        let json = serde_json::to_string(&b.snapshot()).unwrap();
        assert!(!json.contains("world_eras"), "{json}");

        // A world the launcher's table knows plays the table's era, until it says its own; an era
        // it names alone has that era's whole table, not the systems the table gave another.
        let mut seedsow = World::new("seedsow", "Seedsow");
        seedsow.list_id = Some("394C58D0-885D-466B-B17F-D7E0B96FE3E2".into());
        dereth_launch::known::apply(&mut seedsow);
        seedsow.endpoint = b.list.worlds()[0].endpoint.clone();
        b.list.worlds_mut().push(seedsow);
        let w = b.world("seedsow").unwrap();
        assert_eq!(
            (w.era.as_deref(), w.era_source),
            (Some("infiltration"), Some(Told::World))
        );
        b.live.insert(
            "seedsow".into(),
            dereth_launch::status::parse_world_document(br#"{"world_open":true,"era":"eor"}"#)
                .unwrap(),
        );
        let w = b.world("seedsow").unwrap();
        assert_eq!(
            (w.era.as_deref(), w.era_features.as_deref()),
            (Some("eor"), None)
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_classic_era_world_is_not_started_without_classic_data_files() {
        let (mut b, dir) = backend("classic-needed");
        b.list.worlds_mut()[0].era = Some("infiltration".into());
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
        b.list.worlds_mut()[0].era = None;
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
    fn the_dereth_client_is_given_the_classic_set_and_the_era_the_world_says() {
        let (mut b, dir) = backend("classic-play");
        let classic = dir.join("feb2005");
        std::fs::create_dir_all(&classic).unwrap();
        std::fs::write(classic.join("portal.dat"), b"p").unwrap();
        std::fs::write(classic.join("cell.dat"), b"c").unwrap();
        let find = b.read_folder(&classic);
        let id = find.classic.as_ref().unwrap().id.clone();
        b.add_folder(find);
        b.live.insert(
            "eulmore".into(),
            LiveStatus {
                era: Some("infiltration".into()),
                era_features: Some("aetheria=true".into()),
                ..LiveStatus::default()
            },
        );
        let systems = dereth_launch::launch::describe(&b.world("eulmore").unwrap())
            .era_features
            .expect("the systems it says");
        let out = b.launch(Choice {
            classic_set_id: Some(id.clone()),
            ..choice(Some("pw"))
        });
        assert!(matches!(out, LaunchOutcome::Launched { .. }), "{out:?}");
        b.running.remove(0).child.wait().unwrap();
        let args = std::fs::read_to_string(dir.join("client/args.txt")).unwrap();
        assert!(
            args.contains(&format!("--classic-dat-dir {}", classic.display()))
                && args.contains(&format!("--era infiltration --era-features {systems}")),
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
            // The sources the launcher below reads: none.
            sources: Vec::new(),
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
        assert!(!b.list.wants_reading(now()), "fresh: no fetch at start");
        assert_eq!(s.eras.len(), 2);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A launcher started on another hub's list (`DERETH_HUB_LIST`) reads that hub's list at
    /// start, however fresh the copy of the network's, and the launcher started after it on the
    /// usual sources reads the network's again, however fresh the other hub's copy.
    #[test]
    fn a_launcher_on_another_hubs_list_reads_it_at_start_and_the_next_reads_the_networks_again() {
        const DEV: &str = "https://dev-hub.example.org/v1/servers";
        let page = |name: &str| {
            serde_json::json!({"data": [{"slug": serverlist::slug_for(name), "name": name,
                "isDemo": false, "emulator": {"family": "ACE"},
                "endpoint": {"mode": "manual", "address": "127.0.0.1", "port": 9}}],
                "nextOffset": null})
            .to_string()
        };
        let dir = tmp("list-other-hub");
        let folders = Folders::single(dir.join("state"));
        let usual = serverlist::sources(None);
        ListCache {
            fetched_at: now(),
            hub: vec![page("Leafcull")],
            sources: serverlist::source_urls(&usual),
            ..ListCache::default()
        }
        .save(&folders.data)
        .unwrap();
        let launcher = |sources: Vec<ListSource>| -> Shared {
            Arc::new(Mutex::new(Backend::new(
                folders.clone(),
                sources,
                None,
                Box::new(MemoryVault::default()),
                "memory",
                None,
            )))
        };
        let slugs = |shared: &Shared| -> Vec<String> {
            lock(shared)
                .snapshot()
                .worlds
                .into_iter()
                .map(|w| w.slug)
                .collect()
        };

        let on_dev = launcher(serverlist::sources_from(None, Some(DEV)));
        assert!(slugs(&on_dev).is_empty(), "the network's copy is not shown");
        assert!(
            lock(&on_dev).list.wants_reading(now()),
            "the dev hub's list is read at start"
        );
        let dev_page = page("Dev World");
        Backend::refresh_with(&on_dev, move |url| {
            assert!(url.starts_with(DEV), "{url}");
            Ok(dev_page.clone().into_bytes())
        })
        .join()
        .unwrap();
        assert_eq!(slugs(&on_dev), ["dev-world"]);

        let after = launcher(usual);
        assert!(slugs(&after).is_empty(), "the dev hub's copy is not shown");
        assert!(
            lock(&after).list.wants_reading(now()),
            "the network's list is read again"
        );
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
            sources: serverlist::source_urls(&serverlist::sources(None)),
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

    /// The window shows the same worlds as the web client's front page from the same answers: the
    /// hub's list, with how many are on as the hub has it, or the community's own when the hub does
    /// not answer.
    #[test]
    fn the_window_shows_the_worlds_every_launcher_shows_from_the_same_answers() {
        use dereth_launch::testing::world_list as fixture;
        let dir = tmp("same-worlds");
        let shared: Shared = Arc::new(Mutex::new(Backend::new(
            Folders::single(dir.join("state")),
            serverlist::sources(None),
            None,
            Box::new(MemoryVault::default()),
            "memory",
            None,
        )));
        let read = |hub_answers: bool| {
            Backend::refresh_with(&shared, fixture::fetch(hub_answers))
                .join()
                .unwrap();
            let b = lock(&shared);
            assert_eq!(b.list_state, ListState::Loaded);
            b.snapshot()
                .worlds
                .iter()
                .map(fixture::summary)
                .collect::<Vec<_>>()
        };
        assert_eq!(read(true), fixture::from_the_hub());
        assert_eq!(read(false), fixture::from_the_community());
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
        let all = b.list.worlds().to_vec();
        b.list.worlds_mut().clear();
        b.forget_unlisted();
        assert_eq!(b.state.recent.len(), 2, "an empty list forgets nothing");
        *b.list.worlds_mut() = all;
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

    /// A row of the hub's list: the world `name` on this machine's discard port, its status
    /// `state` (a heartbeat's, counting until `expires`, when `expires` is given).
    fn hub_row(
        name: &str,
        state: &str,
        players: Option<u32>,
        expires: Option<&str>,
    ) -> serde_json::Value {
        let status = match expires {
            Some(at) => serde_json::json!({"state": state, "playersOnline": players,
                "source": "server-heartbeat", "expiresAt": at,
                "playersKind": "connected-characters"}),
            None => serde_json::json!({"state": state, "playersOnline": null}),
        };
        serde_json::json!({"slug": serverlist::slug_for(name), "name": name, "isDemo": false,
            "emulator": {"family": "ACE"},
            "endpoint": {"mode": "manual", "address": "127.0.0.1", "port": 9},
            "status": status})
    }

    /// A page of the hub's list holding `rows`, the next starting at `next`.
    fn hub_page_of(rows: &[serde_json::Value], next: Option<u64>) -> String {
        serde_json::json!({"data": rows, "nextOffset": next, "total": rows.len()}).to_string()
    }

    /// The expiry of a heartbeat that counts for as long as any test runs, and of one long past.
    const LATER: &str = "2999-01-01T00:00:00.000Z";
    const LONG_AGO: &str = "2000-01-01T00:00:00.000Z";

    /// A launcher whose day's copy of the list is `copy`, read from the usual sources, or from the
    /// `Servers.xml` `named` alone.
    /// A launcher reading the usual sources, or the list `named` alone, with `copy` kept as the
    /// day's copy of what they gave.
    fn launcher_with(name: &str, copy: &ListCache, named: Option<&str>) -> (Shared, PathBuf) {
        let dir = tmp(name);
        let folders = Folders::single(dir.join("state"));
        let sources = serverlist::sources(named);
        ListCache {
            sources: serverlist::source_urls(&sources),
            ..copy.clone()
        }
        .save(&folders.data)
        .unwrap();
        let b = Backend::new(
            folders,
            sources,
            None,
            Box::new(MemoryVault::default()),
            "memory",
            None,
        );
        (Arc::new(Mutex::new(b)), dir)
    }

    /// The state and players a world is shown with.
    fn seen(shared: &Shared, slug: &str) -> (WorldState, Option<u32>) {
        let w = lock(shared).world(slug).unwrap();
        (w.state, w.players)
    }

    /// What was asked of a stand-in for the hub.
    type Asked = Arc<Mutex<Vec<String>>>;

    /// A stand-in for the hub that answers the addresses asked with `answers`, in order, and
    /// records them.
    fn stand_in(
        answers: Vec<Result<String, &'static str>>,
    ) -> (
        Asked,
        impl FnMut(&str) -> Result<Vec<u8>, String> + Send + 'static,
    ) {
        let asked: Asked = Arc::new(Mutex::new(Vec::new()));
        let ask = asked.clone();
        let get = move |url: &str| {
            let mut asked = ask.lock().unwrap();
            asked.push(url.to_owned());
            answers
                .get(asked.len() - 1)
                .cloned()
                .unwrap_or(Err("no answer"))
                .map(String::into_bytes)
                .map_err(|e| format!("{url}: {e}"))
        };
        (asked, get)
    }

    /// The day's copy holds the hub's list as it was: none of its status is shown. The hub's list,
    /// read again a page at a time about once a minute, gives each world's state and players, an
    /// ACE world's included; a read that fails changes nothing, and a heartbeat past its expiry
    /// leaves the world to its probe.
    #[test]
    fn the_hubs_list_read_once_a_minute_shows_an_ace_worlds_state_and_players() {
        use serverlist::HUB_LIST_URL;
        let copy = ListCache {
            fetched_at: now(),
            list: String::new(),
            hub: vec![hub_page_of(
                &[
                    hub_row("Coldeve", "online", Some(99), Some(LATER)),
                    hub_row("Frostfell", "online", Some(5), Some(LATER)),
                ],
                None,
            )],
            ..ListCache::default()
        };
        let (shared, dir) = launcher_with("hub-status", &copy, None);
        assert_eq!(
            seen(&shared, "coldeve"),
            (WorldState::Unknown, None),
            "nothing from the copy, however fresh"
        );
        // Neither answered the server-tracker login: down, until the hub says otherwise.
        lock(&shared).probed.insert("coldeve".into(), false);
        lock(&shared).probed.insert("frostfell".into(), false);
        assert_eq!(seen(&shared, "coldeve"), (WorldState::Offline, None));

        let t0 = now();
        let (asked, get) = stand_in(vec![
            Ok(hub_page_of(
                &[hub_row("Coldeve", "online", Some(12), Some(LATER))],
                Some(100),
            )),
            Ok(hub_page_of(
                &[hub_row("Frostfell", "maintenance", Some(0), Some(LATER))],
                None,
            )),
        ]);
        Backend::read_hub_status_at(&shared, t0, get)
            .expect("a read is due at once")
            .join()
            .unwrap();
        assert_eq!(
            *asked.lock().unwrap(),
            [
                format!("{HUB_LIST_URL}?limit=100&offset=0"),
                format!("{HUB_LIST_URL}?limit=100&offset=100")
            ],
            "one list, a page at a time"
        );
        assert_eq!(seen(&shared, "coldeve"), (WorldState::Online, Some(12)));
        assert_eq!(seen(&shared, "frostfell"), (WorldState::Starting, Some(0)));
        // The world's own answer (Empyrean's status ping) goes before the hub's word.
        lock(&shared).live.insert(
            "frostfell".into(),
            LiveStatus {
                state: WorldState::Online,
                players: Some(3),
                ..LiveStatus::default()
            },
        );
        assert_eq!(seen(&shared, "frostfell"), (WorldState::Online, Some(3)));
        lock(&shared).live.remove("frostfell");
        let json = serde_json::to_string(&lock(&shared).snapshot()).unwrap();
        assert!(json.contains("\"players\":12"), "{json}");

        let (asked, get) = stand_in(Vec::new());
        assert!(
            Backend::read_hub_status_at(&shared, t0 + 59, get).is_none(),
            "not within the minute"
        );
        assert!(asked.lock().unwrap().is_empty());

        let (asked, get) = stand_in(vec![Err("connection refused")]);
        Backend::read_hub_status_at(&shared, t0 + 60, get)
            .expect("due a minute later")
            .join()
            .unwrap();
        assert_eq!(asked.lock().unwrap().len(), 1, "the hub alone is asked");
        assert_eq!(
            seen(&shared, "coldeve"),
            (WorldState::Online, Some(12)),
            "a failed read changes nothing"
        );

        let (_, get) = stand_in(vec![Ok(hub_page_of(
            &[
                hub_row("Coldeve", "online", Some(12), Some(LONG_AGO)),
                hub_row("Frostfell", "unknown", None, None),
            ],
            None,
        ))]);
        lock(&shared).probed.insert("frostfell".into(), true);
        Backend::read_hub_status_at(&shared, t0 + 120, get)
            .unwrap()
            .join()
            .unwrap();
        assert_eq!(
            seen(&shared, "coldeve"),
            (WorldState::Offline, None),
            "an expired heartbeat leaves the world to its probe"
        );
        assert_eq!(seen(&shared, "frostfell"), (WorldState::Online, None));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The refresh button's list, when it is the hub's, brings each world's status at once. The
    /// community's list read in its place brings none, nor does a list the player names, and no
    /// status is read while either is shown.
    #[test]
    fn the_refresh_buttons_hub_list_brings_the_status_and_no_other_list_does() {
        let xml = "<ArrayOfServerItem><ServerItem><name>Coldeve</name>\
                   <server_host>127.0.0.1</server_host><server_port>9</server_port>\
                   </ServerItem></ArrayOfServerItem>";
        let community = ListCache {
            fetched_at: now(),
            list: xml.into(),
            ..ListCache::default()
        };
        let (shared, dir) = launcher_with("hub-refresh", &community, None);
        let (_, get) = stand_in(Vec::new());
        assert!(
            Backend::read_hub_status_at(&shared, now(), get).is_none(),
            "the community's list is on screen"
        );

        let hub = hub_page_of(&[hub_row("Coldeve", "online", Some(31), Some(LATER))], None);
        Backend::refresh_with(&shared, move |url| {
            assert!(url.starts_with(serverlist::HUB_LIST_URL), "{url}");
            Ok(hub.clone().into_bytes())
        })
        .join()
        .unwrap();
        assert_eq!(seen(&shared, "coldeve"), (WorldState::Online, Some(31)));
        let (_, get) = stand_in(Vec::new());
        assert!(
            Backend::read_hub_status_at(&shared, now() + 30, get).is_none(),
            "the list just fetched counts as a read"
        );

        Backend::refresh_with(&shared, move |url| {
            if url.starts_with(serverlist::HUB_LIST_URL) {
                Err("http status: 502".into())
            } else {
                Ok(xml.as_bytes().to_vec())
            }
        })
        .join()
        .unwrap();
        assert_eq!(
            seen(&shared, "coldeve").1,
            None,
            "the community's list says nothing of players"
        );
        let (_, get) = stand_in(Vec::new());
        assert!(Backend::read_hub_status_at(&shared, now() + 600, get).is_none());
        let _ = std::fs::remove_dir_all(dir);

        let hub_copy = ListCache {
            fetched_at: now(),
            hub: vec![hub_page_of(
                &[hub_row("Coldeve", "online", Some(31), Some(LATER))],
                None,
            )],
            ..ListCache::default()
        };
        let (shared, dir) = launcher_with(
            "hub-named",
            &hub_copy,
            Some("https://lists.example/Servers.xml"),
        );
        let (asked, get) = stand_in(Vec::new());
        assert!(Backend::read_hub_status_at(&shared, now(), get).is_none());
        assert!(
            asked.lock().unwrap().is_empty(),
            "a named list is read alone, and says nothing of status"
        );
        assert_eq!(seen(&shared, "coldeve"), (WorldState::Unknown, None));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A game the launcher "started" on `world`: this test program again, listing its tests,
    /// which ends at once.
    fn running_game(world: &World, kind: ClientKind) -> Running {
        let child = Command::new(std::env::current_exe().unwrap())
            .arg("--list")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        Running {
            child,
            world_slug: world.slug.clone(),
            account: "player".into(),
            kind,
            multi_instance: true,
            dat_set_id: None,
            activity: Activity::new(world, kind, None, 1_700_000_000),
        }
    }

    /// Discord is shown the last game started while it runs, and cleared once the games exit;
    /// with "Show what I'm playing on Discord" off (it is on to begin with, and kept with the
    /// launcher's state) Discord is not even looked for.
    #[test]
    fn discord_shows_the_running_game_until_it_exits_and_nothing_with_the_setting_off() {
        use crate::discord::tests::{endpoint, next, quiet, stand_in, Script, Seen};
        use crate::discord::Presence;
        let (mut b, dir) = backend("discord");
        assert!(b.state.settings.discord_presence, "on to begin with");
        assert_eq!(b.presence(), None, "no game, nothing shown");

        let eulmore = b.world("eulmore").unwrap();
        let mut leafcull = World::new("leafcull", "Leafcull");
        leafcull.era = Some("infiltration".into());
        b.running.push(running_game(&eulmore, ClientKind::Dereth));
        b.running.push(running_game(&leafcull, ClientKind::Retail));
        let shown = b.presence().expect("a game runs");
        assert_eq!(
            (shown.details.as_str(), shown.state.as_str()),
            ("Leafcull", "Retail client · Infiltration (February 2005)"),
            "the last game started"
        );

        let path = endpoint("backend");
        let rx = stand_in(&path, Script::Answer);
        let app = dereth_launch::discord::Application {
            id: "1".into(),
            large_image: None,
        };
        let mut discord = Presence::new(app, vec![path]);
        b.set_discord_presence(false);
        assert_eq!(b.presence(), None);
        discord.update(b.presence().as_ref(), Instant::now());
        assert!(quiet(&rx), "the setting off: nothing is said");
        let (saved, _) = LauncherState::load(&dir.join("state"));
        assert!(!saved.settings.discord_presence, "kept with the state");

        b.set_discord_presence(true);
        let t0 = Instant::now();
        discord.update(b.presence().as_ref(), t0);
        assert!(matches!(next(&rx), Seen::Frame(0, _)));
        match next(&rx) {
            Seen::Frame(1, v) => assert_eq!(v["args"]["activity"]["details"], "Leafcull"),
            other => panic!("{other:?}"),
        }

        let until = Instant::now() + Duration::from_secs(30);
        while !b.running.is_empty() && Instant::now() < until {
            b.poll_running();
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(b.running.is_empty(), "the games exit");
        assert_eq!(b.presence(), None);
        discord.update(b.presence().as_ref(), t0 + Duration::from_secs(1));
        match next(&rx) {
            Seen::Frame(1, v) => assert_eq!(v["args"].get("activity"), None, "cleared"),
            other => panic!("{other:?}"),
        }
        assert_eq!(next(&rx), Seen::Closed);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_realm_picked_is_kept_with_the_state_and_shown_to_the_page() {
        let (mut b, dir) = backend("realm");
        assert_eq!(
            b.state.settings.realm,
            Realm::Dereth,
            "Dereth to begin with"
        );
        b.set_realm(Realm::Auberean);
        let (saved, _) = LauncherState::load(&dir.join("state"));
        assert_eq!(saved.settings.realm, Realm::Auberean, "kept with the state");
        let json = serde_json::to_value(b.snapshot()).unwrap();
        assert_eq!(json["state"]["settings"]["realm"], "auberean");
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
    #[test]
    fn a_host_address_change_refuses_play_before_starting_a_client_on_the_old_address() {
        let (mut b, dir) = backend("host-address");
        let host = b.folders.data.join("hosts/host-test");
        std::fs::create_dir_all(&host).unwrap();
        let settings = crate::host_config::Settings {
            dats: dir.join("client"),
            port: 9001,
            ..Default::default()
        };
        std::fs::write(
            host.join("empyrean.toml"),
            settings.update_config("").unwrap(),
        )
        .unwrap();
        crate::host_runtime::write_json(
            &host.join("host.json"),
            &crate::host_runtime::Definition {
                id: "host-test".into(),
                version: "1.0.0".into(),
                program: host.join("server"),
                wrapper: host.join("wrapper"),
                checkout: false,
            },
        )
        .unwrap();
        let guard = std::fs::File::create(host.join("run.lock")).unwrap();
        guard.lock().unwrap();
        crate::host_runtime::write_json(
            &host.join("status.json"),
            &crate::host_runtime::Status {
                phase: "running".into(),
                ..Default::default()
            },
        )
        .unwrap();
        crate::host_publication::register_player(&b.folders.data, "host-test", "eulmore").unwrap();
        assert!(
            matches!(b.launch(choice(Some("test"))), LaunchOutcome::Error { message } if message.contains("address changed"))
        );
        assert!(b.running.is_empty());
    }

    /// A published Lyceum world on the Dereth client's overlay blocklist is not started: Play says
    /// so, before it asks for a password. Off the list, Play goes on.
    #[test]
    fn play_refuses_a_hosted_world_on_the_clients_overlay_blocklist() {
        use sha2::{Digest, Sha256};
        let (mut b, dir) = backend("host-blocked");
        let host = b.folders.data.join("hosts/host-test");
        let snapshot = host.join("published/snapshot-0");
        std::fs::create_dir_all(snapshot.join("overlay")).unwrap();
        let settings = crate::host_config::Settings {
            dats: dir.join("client"),
            port: 9000,
            ..Default::default()
        };
        let config = settings.update_config("").unwrap();
        let overlay = snapshot.join("overlay");
        std::fs::write(
            host.join("empyrean.toml"),
            format!(
                "{config}\n[dat_overlay]\npath = '{}'\n",
                overlay.to_string_lossy().replace('\\', "/")
            ),
        )
        .unwrap();
        let files = std::collections::BTreeMap::<String, String>::new();
        crate::host_runtime::write_json(
            &snapshot.join("publication.json"),
            &crate::host_publication::Publication {
                source: host.join("source.toml"),
                key: "blocked-world".into(),
                build: format!("{:x}", Sha256::digest(serde_json::to_vec(&files).unwrap())),
                files,
                ..crate::host_publication::Publication::default()
            },
        )
        .unwrap();
        crate::host_runtime::write_json(
            &host.join("host.json"),
            &crate::host_runtime::Definition {
                id: "host-test".into(),
                version: "1.0.0".into(),
                program: host.join("server"),
                wrapper: host.join("wrapper"),
                checkout: false,
            },
        )
        .unwrap();
        let guard = std::fs::File::create(host.join("run.lock")).unwrap();
        guard.lock().unwrap();
        crate::host_runtime::write_json(
            &host.join("status.json"),
            &crate::host_runtime::Status {
                phase: "running".into(),
                ..Default::default()
            },
        )
        .unwrap();
        crate::host_publication::register_player(&b.folders.data, "host-test", "eulmore").unwrap();
        std::fs::create_dir_all(dir.join("settings")).unwrap();
        std::fs::write(
            dir.join("settings").join("overlay-blocklist.txt"),
            "# refused\nblocked-world\n",
        )
        .unwrap();
        let out = b.launch(choice(None));
        assert!(
            matches!(&out, LaunchOutcome::Error { message } if message.contains("blocklist")),
            "{out:?}"
        );
        assert!(b.running.is_empty(), "no client started");
        // Off the list, Play goes on to the password.
        std::fs::write(
            dir.join("settings").join("overlay-blocklist.txt"),
            "another\n",
        )
        .unwrap();
        assert!(
            matches!(b.launch(choice(None)), LaunchOutcome::NeedPassword { .. }),
            "past the check"
        );
        drop(guard);
        let _ = std::fs::remove_dir_all(dir);
    }
}
