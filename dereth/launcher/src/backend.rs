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
use dereth_launch::datset::{scan_dir, DatOrigin, DatSet};
use dereth_launch::folders::Folders;
use dereth_launch::install::{ClientKind, Installation};
use dereth_launch::launch::{plan, LaunchRequest};
use dereth_launch::library::{self, FolderFind};
use dereth_launch::state::{Account, Favourite, LauncherState, Recent, WorldPrefs};
use dereth_launch::status::LiveStatus;
use dereth_launch::vault::{target, Vault};
use dereth_launch::world::{AccountModel, StatusMethod, World, WorldState};
use serde::{Deserialize, Serialize};

/// Where the world list comes from unless `DERETH_SERVERS_API` says otherwise.
pub const DEFAULT_SERVERS_API: &str = "https://api.dereth.network/v1/servers";

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

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// One HTTPS GET, bounded. Plain HTTP is refused: the URLs come from configuration and the
/// directory, and quietly downgrading either to plaintext is not a favour.
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

/// Open an `http`/`https` URL in the browser. Operator- and directory-supplied links are not
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
}

/// A world page's options.
#[derive(Debug, Clone, Serialize)]
pub struct WorldView {
    pub world: World,
    pub clients: Vec<ClientView>,
    pub default_client: Option<ClientKind>,
    /// The dat sets the Dereth client may be given here.
    pub dat_sets: Vec<DatSet>,
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
    servers_api: String,
    state: LauncherState,
    dereth: Option<Installation>,
    worlds: Vec<World>,
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
    /// Load the state from the settings folder, read the Dereth client at `dereth_exe`, and start.
    ///
    /// A first run whose Dereth client has a full end-of-retail set beside it (a developer's
    /// checkout, say) takes that set as the shared one, so there is something to play with at once.
    pub fn new(
        folders: Folders,
        servers_api: String,
        dereth_exe: Option<PathBuf>,
        vault: Box<dyn Vault + Send>,
        vault_name: &'static str,
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
        Self {
            folders,
            servers_api,
            state,
            dereth,
            worlds: Vec::new(),
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

    fn with_live(&self, mut w: World) -> World {
        // A world with no status document is up or down by whether it answered the probe.
        if let Some(up) = self.probed.get(&w.slug) {
            w.state = if *up {
                WorldState::Online
            } else {
                WorldState::Offline
            };
        }
        if let Some(l) = self.live.get(&w.slug) {
            w.state = l.state;
            if l.players.is_some() {
                w.players = l.players;
            }
            if l.patching.is_some() {
                w.dats.patches_over_wire = l.patching;
            }
            if w.account_model == AccountModel::Unknown && l.auto_create_accounts == Some(true) {
                w.account_model = AccountModel::AutoCreateOnFirstLogin;
            }
        }
        w
    }

    /// Every world: the directory's, then the servers the player added.
    fn all_worlds(&self) -> impl Iterator<Item = World> + '_ {
        self.worlds
            .iter()
            .cloned()
            .chain(self.state.custom_worlds.iter().map(|c| c.to_world()))
            .map(|w| self.with_live(w))
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            worlds: self.all_worlds().collect(),
            list_state: self.list_state.clone(),
            list_error: self.list_error.clone(),
            state: self.state.clone(),
            dereth: self.dereth.clone(),
            running: self.running_clients(),
            job: self.job.clone(),
            message: self.message.clone(),
            vault_name: self.vault_name,
            platform: std::env::consts::OS,
            library_dir: self.library_dir().display().to_string(),
        }
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

    /// Fetch the directory on a thread, following its pages.
    pub fn refresh(shared: &Shared) {
        let url = {
            let mut b = lock(shared);
            b.list_state = ListState::Loading;
            b.servers_api.clone()
        };
        let shared = shared.clone();
        std::thread::spawn(move || {
            let result = fetch_all(&url);
            let mut b = lock(&shared);
            match result {
                Ok(worlds) => {
                    b.worlds = worlds;
                    b.list_state = ListState::Loaded;
                    b.list_error = None;
                }
                Err(e) => {
                    b.list_state = ListState::Unavailable;
                    b.list_error = Some(e);
                }
            }
            drop(b);
            Self::probe_all(&shared);
        });
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

    /// Ask the named worlds whether they are up, a few at a time, on threads of their own.
    pub fn probe(shared: &Shared, slugs: Vec<String>) {
        let queue: Vec<(String, String, u16)> = {
            let mut b = lock(shared);
            let targets: Vec<(String, String, u16)> = slugs
                .iter()
                .filter(|s| !b.probing.contains(*s))
                .filter_map(|s| b.world(s))
                .filter_map(|w| w.endpoint.map(|e| (w.slug, e.address, e.port)))
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
                let Some((slug, host, port)) = next else {
                    break;
                };
                let up = probe_host(&host, port);
                let mut b = lock(&shared);
                b.probing.remove(&slug);
                b.probed.insert(slug, up);
            });
        }
    }

    /// Once a second: ask for each Empyrean world's status when it is due, and watch the clients.
    pub fn tick(shared: &Shared) {
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
        let dat_set_id = (choice.client == ClientKind::Dereth)
            .then(|| choice.dat_set_id.clone())
            .flatten();
        let dat_dir = dat_set_id
            .as_deref()
            .and_then(|id| self.state.dat_set(id))
            .map(|s| s.path.clone());
        let req = LaunchRequest {
            world: &world,
            install: &install,
            account: &choice.account,
            dat_dir,
            extra_args: &[],
        };
        let plan = match plan(&req) {
            Ok(p) => p,
            Err(e) => return err(e.to_string()),
        };
        if !plan.exe.exists() {
            return err(format!("Client not found: {}", plan.exe.display()));
        }
        // The one form of the command line that may be logged; the page names only the world and\r
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
                account: Some(choice.account.clone()),
                last_played: Some(now()),
            },
        );
        self.state.record_recent(Recent {
            world_slug: choice.world_slug.clone(),
            account: choice.account.clone(),
            client: choice.client,
            dat_set_id: dat_set_id.clone(),
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

    /// Make a set the default the Dereth client is offered first.
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
            origin: DatOrigin::Custom {
                sha256: custom.sha256.clone().unwrap_or_default(),
            },
            files: scan_dir(path),
            last_patched_by_server: None,
            created_by_launcher: false,
        };
        let c = set
            .iterations()
            .compare(&custom.iterations, &dereth_launch::DatRole::ALL);
        if !c.newer.is_empty() || !c.older.is_empty() || !c.missing.is_empty() {
            return Err(format!(
                "These are not {}'s data files: they read {}, the world publishes {}.",
                w.name,
                set.iterations().label(),
                custom.iterations.label()
            ));
        }
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

    /// Add a server by hand.
    pub fn add_custom_world(
        &mut self,
        name: &str,
        host: &str,
        port: &str,
    ) -> Result<String, String> {
        let slug = self
            .state
            .add_custom_world(name, host, port)
            .map_err(|e| e.to_string())?;
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

/// Follow the directory's `nextOffset` until it is exhausted, up to a bound.
fn fetch_all(url: &str) -> Result<Vec<World>, String> {
    let mut out = Vec::new();
    let mut next: Option<u64> = None;
    for _ in 0..20 {
        let page_url = match next {
            None => url.to_owned(),
            Some(off) => format!(
                "{url}{}offset={off}",
                if url.contains('?') { '&' } else { '?' }
            ),
        };
        let body = http_get(&page_url)?;
        let (mut rows, more) =
            dereth_launch::world::parse_page(&body).map_err(|e| e.to_string())?;
        out.append(&mut rows);
        match more {
            Some(off) => next = Some(off),
            None => break,
        }
    }
    Ok(out)
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
            String::new(),
            Some(exe),
            Box::new(MemoryVault::default()),
            "memory",
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
        let slug = b.add_custom_world("Home", "127.0.0.1", "9001").unwrap();
        assert!(b
            .snapshot()
            .worlds
            .iter()
            .any(|w| w.slug == slug && w.name == "Home"));
        assert!(b.world_view(&slug).is_ok());
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
