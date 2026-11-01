//! Auberean's realm: the player's Asheron's Call 2 install, the Auberean and retail clients
//! started into a world, and Athenaeum's worlds. Yalain's worlds on this computer are the
//! launcher's hosted worlds ([`crate::hosts`], [`crate::host_yalain`]).
//!
//! The three programs are components ([`crate::components`]): downloaded, updated and removed as
//! Empyrean and Lyceum are, or this checkout's builds in a development build. What this module adds
//! is what runs them, over what [`dereth_launch::auberean`] knows:
//!
//! - **The install.** The player names their own install's folder; the launcher checks its data
//!   files ([`dereth_launch::auberean::check_install`]) and keeps the folder in its state. Nothing
//!   of the game is downloaded or copied: the client and Yalain read the folder named.
//! - **Playing.** Auberean, or on Windows the retail client in the install, is started into a
//!   world of Asheron's Call 2's by its address (a listed world, or one of the player's own), as
//!   the player chose on its page or on Play, with the account and its secret: the password typed,
//!   else the one the launcher keeps for that world and account, else a new one it makes and keeps
//!   ([`dereth_launch::auberean::secret_for`]), so that every launch passes the secret the account
//!   was made with. A secret is never shown. The launch is kept as any launch is: the account for
//!   the world, the combination among the recent ones, the running game Discord shows.
//! - **Friends.** Before each launch the world is asked whether it serves its friends service
//!   ([`dereth_launch::auberean::probe_friends`]); when it does, the client is given the friends
//!   options and its ticket, and the retail client the friends library the launcher carries. The
//!   page says what the last probe of each world found.
//! - **Athenaeum's worlds.** New World makes one with `athenaeum new` in the player's Documents and
//!   opens the editor on it; Open World opens one by its folder. The editor finds Yalain and the
//!   client beside itself.

use std::collections::{HashMap, HashSet};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use dereth_launch::auberean::{self as ac2, Client, InstallCheck, RetailClient};
use dereth_launch::components::Product;
use dereth_launch::install::ClientKind;
use serde::Serialize;
use tauri::State;

use crate::backend;
use crate::components;

/// The folder in the person's Documents that Athenaeum's new worlds go in.
pub const WORLDS_FOLDER: &str = "Athenaeum";

/// The launcher's own list of the Athenaeum worlds it made or opened, newest first.
const OPENED_FILE: &str = "athenaeum-worlds.json";

/// How many worlds that list keeps.
const OPENED_KEPT: usize = 20;

pub struct Store {
    /// The launcher's data folder.
    data: PathBuf,
    /// What the last action said, and whether it went wrong.
    message: Option<String>,
    error: bool,
    /// The editors the launcher started.
    editors: Vec<Child>,
    /// The last check of the install, by the folder it checked.
    check: Option<InstallCheck>,
    /// The retail client in that install, found with the check.
    retail: Option<RetailClient>,
    /// Where Athenaeum's new worlds go.
    home: Option<PathBuf>,
    /// The retail client's friends library this launcher carries, when it carries one.
    dll: Option<PathBuf>,
    /// Whether each world's friends service answered the last probe, by the world's address as
    /// the page names it.
    friends: HashMap<String, bool>,
    /// The worlds being probed now, by the same address.
    probing: HashSet<String>,
}

pub type Shared = Arc<Mutex<Store>>;

/// The store, over the launcher's data folder `data`, with the friends library `dll` the launcher
/// carries ([`friends_dll`]).
pub fn new(data: PathBuf, dll: Option<PathBuf>) -> Shared {
    Arc::new(Mutex::new(Store {
        data,
        message: None,
        error: false,
        editors: Vec::new(),
        check: None,
        retail: None,
        home: None,
        dll,
        friends: HashMap::new(),
        probing: HashSet::new(),
    }))
}

/// The retail client's friends library this launcher carries: beside it (or in its package's
/// resources), else in a development build that uses its checkout's builds
/// ([`components::uses_checkout`]) the checkout's 32-bit Windows build of it.
pub fn friends_dll(here: &Path, checkout: bool) -> Option<PathBuf> {
    ac2::friends_dll_beside(here).or_else(|| {
        let folders = if checkout {
            components::build_folders(here)
        } else {
            Vec::new()
        };
        folders.into_iter().find_map(|d| {
            let profile = d.file_name()?;
            let p = d
                .parent()?
                .join("i686-pc-windows-msvc")
                .join(profile)
                .join(ac2::FRIENDS_DLL);
            p.is_file().then_some(p)
        })
    })
}

fn lock(shared: &Shared) -> std::sync::MutexGuard<'_, Store> {
    shared
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Athenaeum's folder for new worlds.
pub fn set_worlds_home(shared: &Shared, home: Option<PathBuf>) {
    lock(shared).home = home;
}

/// What the page shows of Auberean's realm.
#[derive(Clone, Serialize)]
pub struct View {
    /// The check of the player's install, once they named one.
    pub check: Option<InstallCheck>,
    /// Whether an editor the launcher started is open.
    pub editing: bool,
    /// Athenaeum's worlds the launcher made or opened, newest first.
    pub worlds: Vec<PathBuf>,
    pub home: Option<PathBuf>,
    pub message: Option<String>,
    pub error: bool,
    /// The retail client in the install, when this system can start it.
    pub retail: Option<RetailClient>,
    /// The client the player plays with.
    pub client: Client,
    /// Whether each world's friends service answered the last probe, by the world's address
    /// (`host:port`).
    pub friends: HashMap<String, bool>,
    /// Whether the launcher carries the retail client's friends library.
    pub friends_dll: bool,
}

/// The page's view, with the install the backend keeps checked again when it changed.
pub fn view(shared: &Shared, b: &backend::Shared) -> View {
    let settings = backend::lock(b).auberean_settings();
    let mut store = lock(shared);
    store
        .editors
        .retain_mut(|c| matches!(c.try_wait(), Ok(None)));
    if let Some(install) = &settings.install {
        if store.check.as_ref().is_none_or(|c| &c.dir != install) {
            store.check = Some(ac2::check_install(install));
            store.retail = ac2::find_retail(install);
        }
    }
    View {
        check: settings.install.as_ref().and(store.check.clone()),
        editing: !store.editors.is_empty(),
        worlds: opened_worlds(&store.data),
        home: store.home.clone(),
        message: store.message.clone(),
        error: store.error,
        retail: settings
            .install
            .as_ref()
            .and(store.retail.clone())
            .filter(|_| cfg!(windows)),
        client: settings.client,
        friends: store.friends.clone(),
        friends_dll: store.dll.is_some(),
    }
}

/// Ask the world at `addr` (`address` on the page) whether it serves its friends service, on a
/// thread of its own, and keep what it says. A world on this computer (`local`) is given the
/// moment it may need after its log says it is ready ([`probe`]).
fn probe_in_background(shared: &Shared, address: String, addr: SocketAddr, local: bool) {
    if !lock(shared).probing.insert(address.clone()) {
        return;
    }
    let shared = shared.clone();
    std::thread::spawn(move || {
        let on = probe(addr, local);
        let mut store = lock(&shared);
        store.probing.remove(&address);
        store.friends.insert(address, on);
    });
}

/// Whether the world at `addr` serves its friends service: it greets the probe within
/// [`ac2::PROBE_WAIT`]. The world on this computer (`local`), just started, may open its friends
/// service a moment after its log says it is ready, so a refused connection is tried again within
/// the same wait.
fn probe(addr: SocketAddr, local: bool) -> bool {
    let began = Instant::now();
    loop {
        let left = ac2::PROBE_WAIT.saturating_sub(began.elapsed());
        if probe_friends(addr, left) {
            return true;
        }
        if !local || began.elapsed() + Duration::from_millis(200) >= ac2::PROBE_WAIT {
            return false;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// Whether the world at `addr` serves the friends stream on its login port: it greets the stream's
/// own greeting ([`ac2::FriendsProbe`]), over a connection of this host's, within `wait`
/// ([`ac2::PROBE_WAIT`] before a launch).
fn probe_friends(addr: SocketAddr, wait: Duration) -> bool {
    use std::io::{Read, Write};
    let began = Instant::now();
    let Ok(mut stream) = TcpStream::connect_timeout(&addr, wait) else {
        return false;
    };
    let mut probe = ac2::FriendsProbe::new();
    if stream.write_all(&probe.opening()).is_err() {
        return false;
    }
    let mut buf = [0u8; 4096];
    loop {
        let left = wait.saturating_sub(began.elapsed());
        if left.is_zero() || stream.set_read_timeout(Some(left)).is_err() {
            return false;
        }
        match stream.read(&mut buf) {
            Ok(0) | Err(_) => return false,
            Ok(n) => match probe.read(&buf[..n]) {
                ac2::Heard::Nothing => {}
                ac2::Heard::Welcome(_) => {
                    let _ = stream.write_all(&probe.goodbye());
                    return true;
                }
                ac2::Heard::Refused => return false,
            },
        }
    }
}

/// A new secret for an account, from the system's random source ([`ac2::secret_from`]).
pub fn new_secret() -> Result<String, String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|e| format!("No secret could be made: {e}"))?;
    Ok(ac2::secret_from(&bytes))
}

/// Ask the listed world `slug` whether it serves its friends service, for its page.
#[tauri::command]
pub fn probe_auberean_friends(a: State<'_, Shared>, b: State<'_, backend::Shared>, slug: String) {
    let Some((host, port)) = backend::lock(&b).world_address(&slug) else {
        return;
    };
    let shared = a.inner().clone();
    std::thread::spawn(move || {
        if let Ok(addr) = resolve(&host, port) {
            probe_in_background(
                &shared,
                format!("{host}:{port}"),
                addr,
                addr.ip().is_loopback(),
            );
        }
    });
}

/// Keep `client` as the client the player plays with.
#[tauri::command]
pub fn set_auberean_client(b: State<'_, backend::Shared>, client: Client) {
    backend::lock(&b).set_auberean_client(client);
}

/// The player's install, checked: an error in words for a person when there is none or it is not
/// ready.
fn install(shared: &Shared, b: &backend::Shared) -> Result<PathBuf, String> {
    let install = backend::lock(b)
        .auberean_settings()
        .install
        .ok_or("Choose your Asheron's Call 2 folder first.")?;
    let check = ac2::check_install(&install);
    let ok = check.ok;
    let summary = check.summary.clone();
    let mut store = lock(shared);
    store.check = Some(check);
    store.retail = ac2::find_retail(&install);
    drop(store);
    if ok {
        Ok(install)
    } else {
        Err(summary)
    }
}

/// Name `dir` as the player's Asheron's Call 2 install, and check it. It is kept whatever the
/// check says, so the page can say what is wrong with it.
#[tauri::command]
pub fn set_auberean_install(
    a: State<'_, Shared>,
    b: State<'_, backend::Shared>,
    dir: PathBuf,
) -> Result<InstallCheck, String> {
    let check = ac2::check_install(&dir);
    let retail = ac2::find_retail(&dir);
    backend::lock(&b).set_auberean_install(dir);
    let mut store = lock(&a);
    store.check = Some(check.clone());
    store.retail = retail;
    Ok(check)
}

/// Ask for the player's Asheron's Call 2 folder, and name it as their install.
#[tauri::command]
pub async fn choose_auberean_install(
    app: tauri::AppHandle,
    a: State<'_, Shared>,
    b: State<'_, backend::Shared>,
) -> Result<Option<InstallCheck>, String> {
    use tauri_plugin_dialog::DialogExt;
    let Some(dir) = app
        .dialog()
        .file()
        .set_title("Your Asheron's Call 2 folder (the one with portal.dat in it)")
        .blocking_pick_folder()
        .and_then(|p| p.into_path().ok())
    else {
        return Ok(None);
    };
    set_auberean_install(a, b, dir).map(Some)
}

/// The command that runs `program`, with what the launcher's own run asks of it: a settings
/// folder of its own under `DERETH_SETTINGS_DIR` when that is set, so a trial run leaves the
/// player's settings alone.
fn command(program: &Path) -> std::process::Command {
    let mut cmd = components::command(program);
    if std::env::var_os("AUBEREAN_SETTINGS_DIR").is_none() {
        if let Some(dir) = std::env::var_os("DERETH_SETTINGS_DIR").filter(|d| !d.is_empty()) {
            cmd.env("AUBEREAN_SETTINGS_DIR", PathBuf::from(dir).join("auberean"));
        }
    }
    if let Some(dir) = program.parent() {
        cmd.current_dir(dir);
    }
    cmd
}

/// The Yalain program to run: the Yalain download, else the one beside Athenaeum's editor.
fn yalain_program(c: &components::Shared) -> Result<PathBuf, String> {
    if let Some(p) = components::program_of(c, Product::Yalain) {
        return Ok(p);
    }
    components::program_of(c, Product::Athenaeum)
        .map(|editor| editor.with_file_name(format!("yalain{}", std::env::consts::EXE_SUFFIX)))
        .filter(|p| p.is_file())
        .ok_or_else(|| "Download Yalain first.".to_owned())
}

/// A launch: the world to start a client into, where it is and what it is called, and the client,
/// the account and the secret the player typed.
struct Launch<'a> {
    /// The name its accounts' secrets are kept under: the world's slug, or the world on this
    /// computer's.
    slug: &'a str,
    /// Its address as the page names it.
    address: String,
    server: SocketAddr,
    /// The world on this computer.
    local: bool,
    client: Client,
    account: &'a str,
    /// The secret the player typed, if any.
    typed: Option<&'a str>,
}

/// The retail client in `install` the launcher starts: the September 2013 client, on Windows.
fn retail_client(install: &Path) -> Result<RetailClient, String> {
    if !cfg!(windows) {
        return Err("The retail client needs Windows. Play with Auberean here.".into());
    }
    let retail = ac2::find_retail(install)
        .ok_or("Your Asheron's Call 2 folder has no AC2client.exe. Play with Auberean.")?;
    if !retail.september_2013 {
        return Err("This AC2client.exe is not the September 2013 client, the one these files are for. Play with Auberean.".into());
    }
    Ok(retail)
}

/// Start the launch's client into its world as its account, with the secret the player typed or
/// the one kept for that world and account, and with the world's friends service when it answers:
/// the client's process.
fn start_client(
    shared: &Shared,
    c: &components::Shared,
    b: &backend::Shared,
    install: &Path,
    to: &Launch<'_>,
) -> Result<Child, String> {
    let (account, client) = (to.account, to.client);
    let program = match client {
        Client::Auberean => {
            components::program_of(c, Product::Auberean).ok_or("Download Auberean first.")?
        }
        Client::Retail => retail_client(install)?.exe,
    };
    let secret = backend::lock(b).auberean_secret(to.slug, account, to.typed)?;
    if client == Client::Retail && !ac2::retail_takes(&secret) {
        return Err("The retail client can't take a password with a double quote in it.".into());
    }
    let answered = probe(to.server, to.local);
    let data = {
        let mut store = lock(shared);
        store.friends.insert(to.address.clone(), answered);
        store.data.clone()
    };
    let friends =
        (answered && ac2::friends_ticket(account, &secret).is_some()).then(|| ac2::Friends {
            host: to.server.ip().to_string(),
            port: to.server.port(),
            dll: None,
        });
    let (plan, mut cmd) = match client {
        Client::Auberean => {
            let plan = ac2::auberean_plan(&program, install, to.server, account, friends.as_ref());
            let cmd = command(&plan.exe);
            (plan, cmd)
        }
        Client::Retail => {
            // The library goes with the friends options; without it the client plays without.
            let dll = lock(shared).dll.clone();
            let friends = friends.zip(dll).map(|(f, dll)| ac2::Friends {
                dll: Some(ac2::dll_for_retail(&dll, &data, &short_path)),
                ..f
            });
            let plan = ac2::retail_plan(&program, to.server, account, friends.as_ref());
            let mut cmd = std::process::Command::new(&plan.exe);
            cmd.current_dir(&plan.working_dir);
            if friends.is_some() {
                // The library writes its log where it is told, in a folder that must be there.
                let logs = data.join("auberean");
                let _ = std::fs::create_dir_all(&logs);
                cmd.env("AUBEREAN_FRIENDS_LOG", logs.join("friends-library.log"));
            }
            (plan, cmd)
        }
    };
    eprintln!("launcher: {}", plan.redacted());
    let name = match client {
        Client::Auberean => "Auberean",
        Client::Retail => "the retail client",
    };
    cmd.args(plan.argv(&secret))
        .stdin(Stdio::null())
        .spawn()
        .map_err(|e| format!("Could not start {name}: {e}"))
}

/// The short (8.3) form of `path`, where the file system keeps one.
#[cfg(windows)]
fn short_path(path: &Path) -> Option<PathBuf> {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use windows_sys::Win32::Storage::FileSystem::GetShortPathNameW;
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    let mut out = vec![0u16; 1024];
    let room = u32::try_from(out.len()).ok()?;
    // SAFETY: `wide` is a NUL-terminated wide string and `out` a buffer of `room` wide characters,
    // both alive for the call, which writes at most `room` characters into `out`.
    let n = unsafe { GetShortPathNameW(wide.as_ptr(), out.as_mut_ptr(), room) };
    let n = usize::try_from(n).ok()?;
    (n > 0 && n < out.len()).then(|| PathBuf::from(std::ffi::OsString::from_wide(&out[..n])))
}

/// No other system keeps short forms.
#[cfg(not(windows))]
fn short_path(_: &Path) -> Option<PathBuf> {
    None
}

/// The address of `host` at `port` the client connects to: the host as it is when it is an
/// address, else what the system's name service answers, a version 4 address first.
fn resolve(host: &str, port: u16) -> Result<SocketAddr, String> {
    let found: Vec<SocketAddr> = (host, port)
        .to_socket_addrs()
        .map_err(|_| format!("{host} could not be found. Check the address."))?
        .collect();
    found
        .iter()
        .find(|a| a.is_ipv4())
        .or(found.first())
        .copied()
        .ok_or_else(|| format!("{host} could not be found. Check the address."))
}

/// Play a world of Asheron's Call 2's as the player chose on its page or on Play: its client
/// started into it as the account, with the secret typed or kept, and kept as any launch is
/// ([`backend::Backend::launched`]): the account for the world, the combination among the recent
/// ones, the running game.
pub fn launch(
    shared: &Shared,
    c: &components::Shared,
    b: &backend::Shared,
    choice: backend::Choice,
) -> backend::LaunchOutcome {
    let err = |message: String| backend::LaunchOutcome::Error { message };
    let account = match ac2::account_name(&choice.account) {
        Ok(a) => a.to_owned(),
        Err(e) => return err(e.to_string()),
    };
    let client = match choice.client {
        ClientKind::Auberean => Client::Auberean,
        ClientKind::Retail => Client::Retail,
        ClientKind::Dereth => return err("The Dereth client plays Asheron's Call's worlds.".into()),
    };
    let started = (|| {
        let install = install(shared, b)?;
        let world = backend::lock(b)
            .world(&choice.world_slug)
            .ok_or("That world is no longer listed.")?;
        let endpoint = world
            .endpoint
            .clone()
            .ok_or("This world names no address to connect to.")?;
        let server = resolve(&endpoint.address, endpoint.port)?;
        let to = Launch {
            slug: &world.slug,
            address: format!("{}:{}", endpoint.address, endpoint.port),
            server,
            local: server.ip().is_loopback(),
            client,
            account: &account,
            typed: choice.password.as_deref(),
        };
        let child = start_client(shared, c, b, &install, &to)?;
        Ok::<_, String>((world, child))
    })();
    match started {
        Ok((world, child)) => {
            let choice = backend::Choice {
                account,
                // The secret is kept whatever was asked: the world admits no other.
                remember: true,
                password: None,
                dat_set_id: None,
                classic_set_id: None,
                ..choice
            };
            let mut backend = backend::lock(b);
            backend.set_auberean_client(client);
            backend.launched(&world, &choice, child, true)
        }
        Err(e) => err(e),
    }
}

// ----- Athenaeum ----------------------------------------------------------------------------------

/// The launcher's own list of the Athenaeum worlds it made or opened, newest first, those no longer
/// there left out.
fn opened_worlds(data: &Path) -> Vec<PathBuf> {
    let listed: Vec<PathBuf> = std::fs::read(data.join(OPENED_FILE))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    listed.into_iter().filter(|w| w.is_dir()).collect()
}

/// Keep `world` first on the launcher's list of Athenaeum's worlds, once.
fn remember(data: &Path, world: &Path) {
    let mut worlds = opened_worlds(data);
    worlds.retain(|w| w != world);
    worlds.insert(0, world.to_path_buf());
    worlds.truncate(OPENED_KEPT);
    if let Ok(bytes) = serde_json::to_vec_pretty(&worlds) {
        let _ = std::fs::create_dir_all(data);
        let _ = std::fs::write(data.join(OPENED_FILE), bytes);
    }
}

/// The `athenaeum` command beside the editor `editor`, as a program and the arguments that come
/// before the command's own: the program beside it, or in an AppImage (the editor's file is the
/// AppImage, with no command beside it) the AppImage given the command's name first.
fn athenaeum_command(editor: &Path) -> (PathBuf, Vec<&'static str>) {
    let beside = editor.with_file_name(format!("athenaeum{}", std::env::consts::EXE_SUFFIX));
    if beside.is_file() {
        (beside, Vec::new())
    } else {
        (editor.to_path_buf(), vec!["athenaeum"])
    }
}

/// Open Athenaeum's editor on `world`, drawn from the player's install.
fn open_editor(
    shared: &Shared,
    c: &components::Shared,
    install: &Path,
    world: &Path,
) -> Result<(), String> {
    let editor =
        components::program_of(c, Product::Athenaeum).ok_or("Download Athenaeum first.")?;
    let child = command(&editor)
        .arg(world)
        .arg("--dats")
        .arg(install)
        .stdin(Stdio::null())
        .spawn()
        .map_err(|e| format!("Could not open Athenaeum: {e}"))?;
    let mut store = lock(shared);
    remember(&store.data, world);
    store.editors.push(child);
    Ok(())
}

/// Open Athenaeum on the world in the folder `world`.
#[tauri::command]
pub fn open_athenaeum(
    a: State<'_, Shared>,
    c: State<'_, components::Shared>,
    b: State<'_, backend::Shared>,
    world: PathBuf,
) -> Result<(), String> {
    let install = install(&a, &b)?;
    if !world.join("world.toml").is_file() {
        return Err("That folder holds no Athenaeum world (no world.toml).".into());
    }
    open_editor(&a, &c, &install, &world)
}

/// Ask for a world's folder and open Athenaeum on it.
#[tauri::command]
pub async fn choose_athenaeum_world(
    app: tauri::AppHandle,
    a: State<'_, Shared>,
    c: State<'_, components::Shared>,
    b: State<'_, backend::Shared>,
) -> Result<(), String> {
    use tauri_plugin_dialog::DialogExt;
    let mut dialog = app.dialog().file().set_title("An Athenaeum world's folder");
    if let Some(home) = lock(&a).home.clone().filter(|h| h.is_dir()) {
        dialog = dialog.set_directory(home);
    }
    let Some(world) = dialog
        .blocking_pick_folder()
        .and_then(|p| p.into_path().ok())
    else {
        return Ok(());
    };
    open_athenaeum(a, c, b, world)
}

/// Make a new world named `name` in Athenaeum's folder for worlds, then open the editor on it.
#[tauri::command]
pub async fn new_athenaeum_world(
    a: State<'_, Shared>,
    c: State<'_, components::Shared>,
    b: State<'_, backend::Shared>,
    name: String,
) -> Result<(), String> {
    let (a, c, b) = (a.inner().clone(), c.inner().clone(), b.inner().clone());
    tauri::async_runtime::spawn_blocking(move || {
        let install = install(&a, &b)?;
        let folder = components::world_folder_name(&name)
            .ok_or("Give the world a name with a letter or a number in it.")?;
        let home = lock(&a)
            .home
            .clone()
            .ok_or("There is no Documents folder to put the world in.")?;
        let world = home.join(folder);
        if world.exists() {
            return Err(format!("{} is already there.", world.display()));
        }
        std::fs::create_dir_all(&home).map_err(|e| e.to_string())?;
        let editor =
            components::program_of(&c, Product::Athenaeum).ok_or("Download Athenaeum first.")?;
        // A new world lies over the world database Yalain pins, which must be in the cache.
        let yalain = editor.with_file_name(format!("yalain{}", std::env::consts::EXE_SUFFIX));
        let yalain = if yalain.is_file() {
            yalain
        } else {
            yalain_program(&c)?
        };
        let fetched = command(&yalain)
            .args(ac2::WORLD_FETCH_ARGS)
            .stdin(Stdio::null())
            .output()
            .map_err(|e| format!("Could not run Yalain: {e}"))?;
        if !fetched.status.success() {
            return Err("Could not get the world database a new world lies over.".into());
        }
        let (program, before) = athenaeum_command(&editor);
        let made = command(&program)
            .args(before)
            .arg("new")
            .arg(&world)
            .stdin(Stdio::null())
            .output()
            .map_err(|e| format!("Could not run Athenaeum: {e}"))?;
        if !made.status.success() {
            let said = String::from_utf8_lossy(&made.stderr);
            return Err(format!(
                "Athenaeum could not make the world. {}",
                said.lines().last().unwrap_or("").trim()
            ));
        }
        open_editor(&a, &c, &install, &world)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Show Athenaeum's folder for worlds.
#[tauri::command]
pub fn open_athenaeum_folder(a: State<'_, Shared>) -> Result<(), String> {
    let home = lock(&a)
        .home
        .clone()
        .ok_or("There is no folder for worlds.")?;
    std::fs::create_dir_all(&home).map_err(|e| e.to_string())?;
    components::open_folder(&home)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the launcher's Auberean realm: its lists and the commands it runs).
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("launcher-ac2-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn athenaeum_s_worlds_are_listed_newest_first_once_and_only_while_there() {
        let d = scratch("worlds");
        let (one, two) = (d.join("one"), d.join("two"));
        std::fs::create_dir_all(&one).unwrap();
        std::fs::create_dir_all(&two).unwrap();
        remember(&d, &one);
        remember(&d, &two);
        remember(&d, &one);
        assert_eq!(opened_worlds(&d), [one.clone(), two.clone()]);
        std::fs::remove_dir_all(&two).unwrap();
        assert_eq!(opened_worlds(&d), [one]);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn the_athenaeum_command_is_beside_the_editor_or_the_appimage_given_its_name() {
        let d = scratch("command");
        let exe = std::env::consts::EXE_SUFFIX;
        let editor = d.join(format!("athenaeum-editor{exe}"));
        assert_eq!(
            athenaeum_command(&editor),
            (editor.clone(), vec!["athenaeum"])
        );
        let beside = d.join(format!("athenaeum{exe}"));
        std::fs::write(&beside, b"").unwrap();
        assert_eq!(athenaeum_command(&editor), (beside, Vec::new()));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn the_friends_library_is_beside_the_launcher_or_in_its_checkouts_32_bit_build() {
        let d = scratch("dll");
        // A checkout: the workspace's manifests, and the launcher built into its release folder.
        std::fs::write(d.join("Cargo.toml"), b"").unwrap();
        std::fs::create_dir_all(d.join("dereth/client")).unwrap();
        std::fs::write(d.join("dereth/client/Cargo.toml"), b"").unwrap();
        let here = d.join("target").join("release");
        std::fs::create_dir_all(&here).unwrap();
        assert_eq!(friends_dll(&here, true), None);
        let built = d
            .join("target")
            .join("i686-pc-windows-msvc")
            .join("release");
        std::fs::create_dir_all(&built).unwrap();
        std::fs::write(built.join(ac2::FRIENDS_DLL), b"dll").unwrap();
        assert_eq!(friends_dll(&here, true), Some(built.join(ac2::FRIENDS_DLL)));
        // A release looks only beside itself.
        assert_eq!(friends_dll(&here, false), None);
        std::fs::write(here.join(ac2::FRIENDS_DLL), b"dll").unwrap();
        assert_eq!(friends_dll(&here, false), Some(here.join(ac2::FRIENDS_DLL)));
        assert_eq!(friends_dll(&here, true), Some(here.join(ac2::FRIENDS_DLL)));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A world's half of the friends stream, as the probe meets it.
    fn world() -> auberean_friends::ServerStream {
        auberean_friends::ServerStream::new(
            Duration::ZERO,
            auberean_friends::ServerConfig::new("dawnsong", "Dawnsong"),
        )
    }

    fn written(out: Vec<auberean_friends::ServerOut>) -> Vec<u8> {
        out.into_iter()
            .filter_map(|o| match o {
                auberean_friends::ServerOut::Write(b) => Some(b),
                _ => None,
            })
            .flatten()
            .collect()
    }

    /// A world on this computer that serves the friends stream on one connection, on its own
    /// thread.
    fn serving_world() -> (SocketAddr, std::thread::JoinHandle<bool>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let world = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut w = world();
            let mut buf = [0u8; 4096];
            while let Ok(n) = s.read(&mut buf) {
                if n == 0 {
                    break;
                }
                w.on_bytes(Duration::ZERO, &buf[..n]);
                let out = written(w.poll(Duration::ZERO));
                if !out.is_empty() && s.write_all(&out).is_err() {
                    break;
                }
                if w.is_ended() {
                    break;
                }
            }
            w.is_ended()
        });
        (addr, world)
    }

    #[test]
    fn a_world_that_serves_the_stream_answers_the_probe_and_one_that_does_not_never_does() {
        // The wait only stops a hang: a welcome ends it.
        let (addr, world) = serving_world();
        assert!(probe_friends(addr, Duration::from_secs(60)));
        assert!(world.join().unwrap(), "the probe said goodbye");

        // A port no one listens on.
        let closed = {
            let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap()
        };
        assert!(!probe_friends(closed, Duration::from_secs(60)));

        // One that takes the connection and says nothing, until the wait is over.
        let silent = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        assert!(!probe_friends(
            silent.local_addr().unwrap(),
            Duration::from_millis(50)
        ));
    }

    #[test]
    fn a_new_secret_is_random() {
        let (a, b) = (new_secret().unwrap(), new_secret().unwrap());
        assert_ne!(a, b);
        assert_eq!(a.len(), 22);
    }

    #[test]
    fn a_world_s_address_is_resolved_and_one_that_is_an_address_kept() {
        assert_eq!(
            resolve("127.0.0.1", 9200).unwrap(),
            "127.0.0.1:9200".parse::<SocketAddr>().unwrap()
        );
        assert!(resolve("not a host name", 9200).is_err());
    }
}
