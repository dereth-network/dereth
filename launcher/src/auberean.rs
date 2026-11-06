//! Auberean's realm: the player's Asheron's Call 2 install, the Auberean and retail clients
//! started into a world, and Athenaeum's worlds. Yalain's worlds on this computer are the
//! launcher's hosted worlds ([`crate::hosts`], [`crate::host_yalain`]).
//!
//! The three programs are components ([`crate::components`]): downloaded, updated and removed as
//! Empyrean and Lyceum are, or this checkout's builds in a development build. What this module adds
//! is what runs them, over what [`dereth_launch::auberean`] knows:
//!
//! - **The library.** The player adds their own installs' folders in the Library, as for
//!   Asheron's Call: the retail client, `AC2client.exe`, and the data sets
//!   ([`dereth_launch::auberean::read_folder`]), kept in the launcher's state. Nothing of the game
//!   is downloaded or copied: the client, Yalain and Athenaeum read the set they are given, the
//!   default one unless another is chosen, checked first
//!   ([`dereth_launch::auberean::check_install`]).
//! - **Playing.** Auberean, or on Windows the retail client in the install, is started into a
//!   world of Asheron's Call 2's by its address (a listed world, or one of the player's own), as
//!   the player chose on its page or on Play, with the account and its password, as Asheron's
//!   Call's: the one typed, else the one kept for that world and account, kept only when the
//!   player asks ([`backend::Backend::password_for`], [`backend::Backend::keep_password`]). The
//!   launch is kept as any launch is: the account for the world, the combination among the recent
//!   ones, the running game Discord shows.
//! - **Friends.** Before each launch the world is asked whether it serves its friends service
//!   ([`dereth_launch::auberean::probe_friends`]); when it does, the client is given the friends
//!   options and its ticket, and the retail client the friends library the launcher carries.
//! - **Athenaeum's worlds.** New World makes one with `athenaeum new` in the player's Documents and
//!   opens the editor on it; Open World opens one by its folder. The editor finds Yalain and the
//!   client beside itself. The page lists the worlds in that folder and those opened elsewhere, as
//!   Lyceum's, each with what it changes (`athenaeum facts`), and Delete takes one away while the
//!   editor is closed.

use std::collections::HashMap;
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

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
    /// The last check of the default data set, by the folder it checked.
    check: Option<InstallCheck>,
    /// The retail client the library names, by its link stamp, by the folder it was read in.
    retail: Option<RetailClient>,
    /// Where Athenaeum's new worlds go.
    home: Option<PathBuf>,
    /// The retail client's friends library this launcher carries, when it carries one.
    dll: Option<PathBuf>,
    /// A world is being deleted or exported.
    busy: bool,
    /// What `athenaeum facts` said of each world, by its folder, under the stamp it was asked at.
    facts: HashMap<PathBuf, (FactsStamp, Option<serde_json::Value>)>,
    /// Whether the worlds' facts are being asked now.
    facts_asking: bool,
}

/// When the `athenaeum` command was written, and the stamp of the world's file and packs.
type FactsStamp = (Option<SystemTime>, crate::lyceum_world::Stamp);

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
        busy: false,
        facts: HashMap::new(),
        facts_asking: false,
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

/// One of Athenaeum's worlds, as the page lists it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AthenaeumWorld {
    /// Its folder.
    pub path: PathBuf,
    /// It is in Athenaeum's folder for worlds, rather than opened from somewhere else.
    pub home: bool,
    /// When it was last saved, in seconds since 1970: when the newest of its world file and its
    /// packs' files last changed.
    pub saved: Option<u64>,
    /// What it changes, counted, as `athenaeum facts` says; none until it has said.
    pub facts: Option<serde_json::Value>,
}

/// What the page shows of Auberean's realm.
#[derive(Clone, Serialize)]
pub struct View {
    /// The check of the library's default data set, once there is one.
    pub check: Option<InstallCheck>,
    /// Whether an editor the launcher started is open.
    pub editing: bool,
    /// A world is being deleted or exported.
    pub busy: bool,
    /// Athenaeum's worlds: those in its folder for worlds, then the others the launcher made or
    /// opened, newest first, each once ([`list_worlds`]).
    pub worlds: Vec<AthenaeumWorld>,
    pub home: Option<PathBuf>,
    pub message: Option<String>,
    pub error: bool,
    /// The library's retail client, by its link stamp, when this system can start it.
    pub retail: Option<RetailClient>,
    /// The client the player plays with.
    pub client: Client,
}

/// The page's view, with the library's default set and retail client checked again when they
/// changed, and Athenaeum's worlds' facts asked again (on a thread of its own) when a world or the
/// `athenaeum` command changed since they were asked.
pub fn view(shared: &Shared, b: &backend::Shared, c: &components::Shared) -> View {
    let facts_command = components::program_of(c, Product::Athenaeum).map(|editor| {
        let (program, before) = athenaeum_command(&editor);
        let written = std::fs::metadata(&program).and_then(|m| m.modified()).ok();
        (program, before, written)
    });
    let (settings, dats) = {
        let b = backend::lock(b);
        let dats = b.auberean_dats(None).map(|s| s.path);
        (b.auberean_settings(), dats)
    };
    let retail_dir = settings.retail.as_ref().map(|r| r.path.clone());
    let mut store = lock(shared);
    store
        .editors
        .retain_mut(|c| matches!(c.try_wait(), Ok(None)));
    if let Some(dats) = &dats {
        if store.check.as_ref().is_none_or(|c| &c.dir != dats) {
            store.check = Some(ac2::check_install(dats));
        }
    }
    if let Some(dir) = &retail_dir {
        if store
            .retail
            .as_ref()
            .is_none_or(|r| r.exe.parent() != Some(dir.as_path()))
        {
            store.retail = ac2::find_retail(dir);
        }
    }
    let listed = list_worlds(store.home.as_deref(), &store.data);
    let written = facts_command.as_ref().map(|(_, _, w)| *w);
    let (worlds, ask) = with_facts(listed, &store.facts, written);
    if let (Some((program, before, _)), false, false) =
        (facts_command, ask.is_empty(), store.facts_asking)
    {
        store.facts_asking = true;
        let shared = shared.clone();
        std::thread::spawn(move || {
            for (world, stamp) in ask {
                let said = command(&program)
                    .args(&before)
                    .arg("facts")
                    .arg(&world)
                    .stdin(Stdio::null())
                    .output()
                    .ok()
                    .filter(|o| o.status.success())
                    .and_then(|o| components::facts_from(&String::from_utf8_lossy(&o.stdout)));
                lock(&shared).facts.insert(world, (stamp, said));
            }
            lock(&shared).facts_asking = false;
        });
    }
    View {
        check: dats.as_ref().and(store.check.clone()),
        editing: !store.editors.is_empty(),
        busy: store.busy,
        worlds,
        home: store.home.clone(),
        message: store.message.clone(),
        error: store.error,
        retail: retail_dir
            .as_ref()
            .and(store.retail.clone())
            .filter(|_| cfg!(windows)),
        client: settings.client,
    }
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

/// Keep `client` as the client the player plays with.
#[tauri::command]
pub fn set_auberean_client(b: State<'_, backend::Shared>, client: Client) {
    backend::lock(&b).set_auberean_client(client);
}

/// The data set `id` of the library (the default one when `None`), checked: its folder, or an
/// error in words for a person when there is none or it is not ready.
fn dats(shared: &Shared, b: &backend::Shared, id: Option<&str>) -> Result<PathBuf, String> {
    let dats = backend::lock(b)
        .auberean_dats(id)
        .ok_or("Add your Asheron's Call 2 game files in the Library first.")?
        .path;
    let check = ac2::check_install(&dats);
    let (ok, summary) = (check.ok, check.summary.clone());
    lock(shared).check = Some(check);
    if ok {
        Ok(dats)
    } else {
        Err(summary)
    }
}

/// Ask for an Asheron's Call 2 folder, and add what it holds to the library at once: its retail
/// client and its data set, the first set the default. The folder as read, so the page can say
/// what is wrong with one that holds no set.
#[tauri::command]
pub async fn choose_auberean_install(
    app: tauri::AppHandle,
    b: State<'_, backend::Shared>,
) -> Result<Option<dereth_launch::library::FolderFind>, String> {
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
    let mut b = backend::lock(&b);
    let find = b.read_folder(&dir, dereth_launch::world::Game::Auberean);
    b.add_folder(find.clone());
    Ok(Some(find))
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

/// A launch: the world to start a client into and where it is, and the client, the account and its
/// password.
struct Launch<'a> {
    server: SocketAddr,
    /// The world on this computer.
    local: bool,
    client: Client,
    account: &'a str,
    password: &'a str,
}

/// The library's retail client the launcher starts: the September 2013 client, on Windows, with
/// the data files beside it ready.
fn retail_client(b: &backend::Shared) -> Result<RetailClient, String> {
    if !cfg!(windows) {
        return Err("The retail client needs Windows. Play with Auberean here.".into());
    }
    let dir = backend::lock(b)
        .auberean_settings()
        .retail
        .map(|r| r.path)
        .ok_or("There is no retail client in the Library. Play with Auberean.")?;
    let retail = ac2::find_retail(&dir)
        .ok_or("The retail client's folder has no AC2client.exe now. Play with Auberean.")?;
    if !retail.september_2013 {
        return Err("This AC2client.exe is not the September 2013 client, the one these files are for. Play with Auberean.".into());
    }
    let check = ac2::check_install(&dir);
    if !check.ok {
        return Err(format!(
            "The retail client's own data files: {}",
            check.summary
        ));
    }
    Ok(retail)
}

/// Start the launch's client into its world as its account and password, with the world's friends
/// service when it answers: the client's process, and the file Auberean's own words go in
/// ([`client_log`]).
fn start_client(
    shared: &Shared,
    c: &components::Shared,
    b: &backend::Shared,
    dats: &Path,
    to: &Launch<'_>,
) -> Result<(Child, Option<PathBuf>), String> {
    let (account, client) = (to.account, to.client);
    let program = match client {
        Client::Auberean => {
            components::program_of(c, Product::Auberean).ok_or("Download Auberean first.")?
        }
        Client::Retail => retail_client(b)?.exe,
    };
    let secret = to.password;
    if client == Client::Retail && !ac2::retail_takes(secret) {
        return Err("The retail client can't take a password with a double quote in it.".into());
    }
    let answered = probe(to.server, to.local);
    let data = lock(shared).data.clone();
    let friends =
        (answered && ac2::friends_ticket(account, secret).is_some()).then(|| ac2::Friends {
            host: to.server.ip().to_string(),
            port: to.server.port(),
            dll: None,
        });
    let mut log = None;
    let (plan, mut cmd) = match client {
        Client::Auberean => {
            let plan = ac2::auberean_plan(&program, dats, to.server, account, friends.as_ref());
            let mut cmd = command(&plan.exe);
            // What it says goes in a file of the launcher's, so one that stops as it starts can
            // say why on the page.
            let file = client_log(&data, account);
            if let Some(out) = file.parent().and_then(|dir| {
                std::fs::create_dir_all(dir).ok()?;
                std::fs::File::create(&file).ok()
            }) {
                cmd.stderr(out);
                log = Some(file);
            }
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
            components::leave_the_mount(&mut cmd, &plan.exe);
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
    let child = cmd
        .args(plan.argv(secret))
        .stdin(Stdio::null())
        .spawn()
        .map_err(|e| format!("Could not start {name}: {e}"))?;
    Ok((child, log))
}

/// The file Auberean's own words go in when the launcher starts it as `account`, in the launcher's
/// data folder `data`: `auberean/auberean-<account>.log`, written again at each start.
fn client_log(data: &Path, account: &str) -> PathBuf {
    let name: String = account
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    data.join("auberean").join(format!("auberean-{name}.log"))
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
/// started into it as the account, with the password typed or kept (asked for when there is
/// neither), the password kept only when the player asked, and the launch kept as any launch is
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
    let Some(password) =
        backend::lock(b).password_for(&choice.world_slug, &account, choice.password.as_deref())
    else {
        return backend::need_password(&account);
    };
    // Auberean plays the set chosen, else the default; the retail client its own files.
    let set = (client == Client::Auberean)
        .then(|| backend::lock(b).auberean_dats(choice.dat_set_id.as_deref()))
        .flatten()
        .map(|s| s.id);
    let started = (|| {
        let dats = match client {
            Client::Auberean => dats(shared, b, set.as_deref())?,
            Client::Retail => PathBuf::new(),
        };
        let world = backend::lock(b)
            .world(&choice.world_slug)
            .ok_or("That world is no longer listed.")?;
        let endpoint = world
            .endpoint
            .clone()
            .ok_or("This world names no address to connect to.")?;
        let server = resolve(&endpoint.address, endpoint.port)?;
        let to = Launch {
            server,
            local: server.ip().is_loopback(),
            client,
            account: &account,
            password: &password,
        };
        let (child, log) = start_client(shared, c, b, &dats, &to)?;
        Ok::<_, String>((world, child, log))
    })();
    match started {
        Ok((world, child, log)) => {
            let choice = backend::Choice {
                account,
                password: None,
                dat_set_id: set,
                classic_set_id: None,
                ..choice
            };
            let mut backend = backend::lock(b);
            // Remember what worked. The password goes in the vault only when asked to.
            backend.keep_password(&world.slug, &choice.account, &password, choice.remember);
            backend.set_auberean_client(client);
            backend.launched(&world, &choice, child, true, log)
        }
        Err(e) => err(e),
    }
}

// ----- Athenaeum ----------------------------------------------------------------------------------

/// The launcher's own list of the Athenaeum worlds it made or opened, newest first, as it is kept.
fn opened_list(data: &Path) -> Vec<PathBuf> {
    std::fs::read(data.join(OPENED_FILE))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

/// The launcher's own list of the Athenaeum worlds it made or opened, newest first, those no longer
/// there left out.
fn opened_worlds(data: &Path) -> Vec<PathBuf> {
    opened_list(data)
        .into_iter()
        .filter(|w| w.is_dir())
        .collect()
}

/// The worlds the page lists, as Lyceum's ([`components::list_worlds`]): those in Athenaeum's
/// folder for worlds `home` (each a folder of it holding a `world.toml`), the ones opened lately
/// first and the rest by name, then the others the launcher made or opened, newest first; each
/// once, by its folder, with whether it is in `home` and the stamp of its world file and packs
/// (an Athenaeum world has a Lyceum world's shape).
fn list_worlds(
    home: Option<&Path>,
    data: &Path,
) -> Vec<(PathBuf, bool, crate::lyceum_world::Stamp)> {
    components::list_worlds(home, &opened_worlds(data))
        .into_iter()
        .filter_map(|(file, home)| {
            let stamp = crate::lyceum_world::stamp(&file);
            Some((file.parent()?.to_path_buf(), home, stamp))
        })
        .collect()
}

/// The `listed` worlds as the page shows them, with what `athenaeum facts` last `said` of each
/// under the stamp it has now; and those to ask about, never asked or changed since, with their
/// stamps. `written` is when the `athenaeum` command was written; with no command, nothing is
/// asked or shown.
fn with_facts(
    listed: Vec<(PathBuf, bool, crate::lyceum_world::Stamp)>,
    said: &HashMap<PathBuf, (FactsStamp, Option<serde_json::Value>)>,
    written: Option<Option<SystemTime>>,
) -> (Vec<AthenaeumWorld>, Vec<(PathBuf, FactsStamp)>) {
    let mut ask = Vec::new();
    let worlds = listed
        .into_iter()
        .map(|(path, home, stamp)| {
            let saved = crate::lyceum_world::saved(&stamp);
            let facts = written.and_then(|written| {
                let now = (written, stamp);
                match said.get(&path) {
                    Some((then, facts)) if *then == now => facts.clone(),
                    _ => {
                        ask.push((path.clone(), now));
                        None
                    }
                }
            });
            AthenaeumWorld {
                path,
                home,
                saved,
                facts,
            }
        })
        .collect();
    (worlds, ask)
}

/// What a delete says while an editor is open, or a program holds one of the world's files.
const STILL_OPEN: &str = "Cannot delete while Athenaeum is open";

/// Delete the Athenaeum world in the folder `world` and everything in it, while no editor the
/// launcher started is open. The page has the person confirm it in place first. The Yalain worlds
/// it was published to keep playing it: a publish is a copy in the hosted world's folder.
#[tauri::command]
pub async fn delete_athenaeum_world(a: State<'_, Shared>, world: PathBuf) -> Result<(), String> {
    let a = a.inner().clone();
    let (home, data) = {
        let mut store = lock(&a);
        store
            .editors
            .retain_mut(|c| matches!(c.try_wait(), Ok(None)));
        if store.busy || !store.editors.is_empty() {
            return Err(STILL_OPEN.into());
        }
        store.busy = true;
        store.error = false;
        store.message = Some("Deleting the world...".into());
        (store.home.clone(), store.data.clone())
    };
    let result =
        tauri::async_runtime::spawn_blocking(move || delete_world(&world, home.as_deref(), &data))
            .await
            .map_err(|e| e.to_string())
            .and_then(|r| r);
    let mut store = lock(&a);
    store.busy = false;
    store.message = None;
    result
}

/// Delete the world in the folder `world` ([`crate::lyceum_world::deletable`] says which folder,
/// and refuses any other, Athenaeum's folder for worlds `home` among them), then forget it in the
/// launcher's own list in its data folder `data` ([`components::delete_folder`]).
fn delete_world(world: &Path, home: Option<&Path>, data: &Path) -> Result<(), String> {
    let folder = crate::lyceum_world::deletable(world, home, "Athenaeum")?;
    // Which entries name it is read while it is there to compare.
    let mut opened = opened_list(data);
    let before = opened.len();
    opened.retain(|w| w.canonicalize().map_or(true, |w| w != folder));
    components::delete_folder(&folder, "Athenaeum", || {
        if opened.len() != before {
            if let Ok(bytes) = serde_json::to_vec_pretty(&opened) {
                let _ = std::fs::write(data.join(OPENED_FILE), bytes);
            }
        }
    })
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

/// Open Athenaeum's editor on `world`, drawn from the data set `dats`.
fn open_editor(
    shared: &Shared,
    c: &components::Shared,
    dats: &Path,
    world: &Path,
) -> Result<(), String> {
    let editor =
        components::program_of(c, Product::Athenaeum).ok_or("Download Athenaeum first.")?;
    let child = command(&editor)
        .arg(world)
        .arg("--dats")
        .arg(dats)
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
    let dats = dats(&a, &b, None)?;
    if !world.join("world.toml").is_file() {
        return Err("That folder holds no Athenaeum world (no world.toml).".into());
    }
    open_editor(&a, &c, &dats, &world)
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

/// Make a new world named `name` in the folder `parent` (Athenaeum's folder for worlds when none is
/// given), then open the editor on it.
#[tauri::command]
pub async fn new_athenaeum_world(
    a: State<'_, Shared>,
    c: State<'_, components::Shared>,
    b: State<'_, backend::Shared>,
    name: String,
    parent: Option<PathBuf>,
) -> Result<(), String> {
    let (a, c, b) = (a.inner().clone(), c.inner().clone(), b.inner().clone());
    tauri::async_runtime::spawn_blocking(move || {
        let dats = dats(&a, &b, None)?;
        let folder = components::world_folder_name(&name)
            .ok_or("Give the world a name with a letter or a number in it.")?;
        let parent = parent
            .or_else(|| lock(&a).home.clone())
            .ok_or("Choose where the world goes.")?;
        let world = parent.join(folder);
        if world.exists() {
            return Err(format!("{} is already there.", world.display()));
        }
        std::fs::create_dir_all(&parent).map_err(|e| format!("{}: {e}", parent.display()))?;
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
        open_editor(&a, &c, &dats, &world)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Ask where a new world goes, starting in Athenaeum's folder for worlds.
#[tauri::command]
pub async fn choose_athenaeum_parent(
    app: tauri::AppHandle,
    a: State<'_, Shared>,
) -> Result<Option<PathBuf>, String> {
    use tauri_plugin_dialog::DialogExt;
    let mut dialog = app
        .dialog()
        .file()
        .set_title("Choose where the new world goes");
    if let Some(home) = lock(&a).home.clone().filter(|h| h.is_dir()) {
        dialog = dialog.set_directory(home);
    }
    dialog
        .blocking_pick_folder()
        .map(|f| f.into_path().map_err(|e| e.to_string()))
        .transpose()
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

// ----- Export For ---------------------------------------------------------------------------------

/// The file that marks a folder as an Athenaeum world's export, so a later export may replace it.
const EXPORT_MARKER: &str = ".athenaeum-export";

/// The folder an export checks the world's build in, inside the export folder; gone when it ends.
const EXPORT_STAGING: &str = ".athenaeum-staging";

/// The folder an export of the world `name` goes in, in the folder the person chose: that folder
/// itself when it is empty or holds an earlier export, else a folder named for the world and
/// Yalain inside it, as a Lyceum world's export ([`components::export_folder`]).
fn export_folder(chosen: &Path, name: &str) -> PathBuf {
    let empty = std::fs::read_dir(chosen).is_ok_and(|mut d| d.next().is_none());
    if empty || chosen.join(EXPORT_MARKER).is_file() {
        chosen.to_path_buf()
    } else {
        chosen.join(format!("{name}-yalain"))
    }
}

/// What Yalain `program` says of itself in its release facts: its version, and the release of the
/// world database it pins.
fn yalain_facts(program: &Path) -> (Option<String>, Option<String>) {
    let facts = command(program)
        .arg("--release-facts")
        .stdin(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| serde_json::from_slice::<serde_json::Value>(o.stdout.trim_ascii()).ok());
    let said = |key: &str| {
        facts
            .as_ref()
            .and_then(|f| f.get(key)?.as_str().map(str::to_owned))
    };
    (said("version"), said("world_database"))
}

/// Export the Athenaeum world in the folder `world` for a Yalain that serves it elsewhere, into
/// `out`: a Yalain world database package. Its world file, its ids (`world.lock`) and its packs,
/// the server's rows, which are all Athenaeum changes (never the game's data files); the world
/// file names the release of the world database it lies over, the one it named or else the one
/// `yalain` pins; the copy is compiled against the data files `dats` as Yalain compiles it at its
/// start, and a world that does not compile is not exported; and a `README.txt` says how to serve
/// it. `out` is new, empty, or an earlier export, which it replaces. What the README names the
/// release by, in words for the page.
///
/// # Errors
/// A world that is gone, changes while it is copied or does not compile, an export folder that
/// holds something else or is in the world's own folder, and Yalain's own errors, in words for a
/// person.
fn export_world(world: &Path, out: &Path, yalain: &Path, dats: &Path) -> Result<String, String> {
    let file = crate::host_publication::athenaeum_world_file(world)
        .canonicalize()
        .map_err(|_| "That Athenaeum world is gone.")?;
    let folder = file.parent().ok_or("That Athenaeum world is gone.")?;
    // Where it goes, as far as it is there yet: never the world's own folder or in it.
    let there = if out.exists() {
        out
    } else {
        out.parent().unwrap_or(out)
    };
    if there.canonicalize().is_ok_and(|t| t.starts_with(folder)) {
        return Err("That folder is in the world's own folder. Export to another.".into());
    }
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    if std::fs::read_dir(out).is_ok_and(|mut d| d.next().is_some()) {
        if !out.join(EXPORT_MARKER).is_file() {
            return Err(format!(
                "{} holds other files. Export to a new or empty folder.",
                out.display()
            ));
        }
        std::fs::remove_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
        std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    }
    std::fs::write(
        out.join(EXPORT_MARKER),
        "Written by the launcher's Export For; a later export replaces this folder.\n",
    )
    .map_err(|e| e.to_string())?;
    let sources = crate::lyceum_world::digest(&file)?;
    crate::host_publication::copy_athenaeum_world(&file, out)?;
    if crate::lyceum_world::digest(&file)? != sources {
        return Err("The world changed during the export. Save in Athenaeum, then retry.".into());
    }
    let (version, pinned) = yalain_facts(yalain);
    let copied = out.join("world.toml");
    let text = std::fs::read_to_string(&copied).map_err(|e| e.to_string())?;
    let key = text
        .parse::<toml_edit::DocumentMut>()
        .ok()
        .and_then(|d| d.get("key")?.as_str().map(str::to_owned))
        .ok_or("The world has no key.")?;
    // A world that names no release lies over the one its server pins: the export names the one
    // it was checked over, so it is served over that one wherever it goes.
    let base = match crate::host_publication::athenaeum_base(&text) {
        Some(base) => base,
        None => {
            let base = pinned.ok_or("Yalain did not say which world database it pins.")?;
            std::fs::write(&copied, with_base(&text, &base)?).map_err(|e| e.to_string())?;
            base
        }
    };
    use std::ffi::OsStr;
    crate::host_publication::run_yalain(
        yalain,
        &[
            OsStr::new("world"),
            OsStr::new("fetch"),
            OsStr::new("--version"),
            OsStr::new(&base),
        ],
        out,
    )
    .map_err(|e| format!("Could not get the world database {base} the world lies over: {e}"))?;
    let staging = out.join(EXPORT_STAGING);
    let checked = crate::host_publication::run_yalain(
        yalain,
        &[
            OsStr::new("world"),
            OsStr::new("compile"),
            OsStr::new("--dats"),
            dats.as_os_str(),
            OsStr::new("--world-db"),
            out.as_os_str(),
            OsStr::new("--out"),
            staging.as_os_str(),
        ],
        out,
    );
    let _ = std::fs::remove_dir_all(&staging);
    checked.map_err(|e| format!("The world does not compile: {e}"))?;
    let packs = crate::lyceum_world::header(&copied)
        .folders
        .iter()
        .filter_map(|p| {
            Some(
                p.strip_prefix(out)
                    .ok()?
                    .to_string_lossy()
                    .replace('\\', "/"),
            )
        })
        .collect::<Vec<_>>();
    let readme = export_readme(&key, &packs, &base, version.as_deref());
    std::fs::write(out.join("README.txt"), readme).map_err(|e| e.to_string())?;
    Ok(base)
}

/// The world file `text` with `[base] version` set to the release `base`, its comments kept.
fn with_base(text: &str, base: &str) -> Result<String, String> {
    let mut doc = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| e.to_string())?;
    if doc.get("base").is_none() {
        doc["base"] = toml_edit::Item::Table(toml_edit::Table::new());
    }
    doc["base"]
        .as_table_like_mut()
        .ok_or("The world's base must be a table.")?
        .insert("version", toml_edit::value(base));
    Ok(doc.to_string())
}

/// What an export's `README.txt` says: what is in the folder, that the world changes only the
/// server's rows, and how a Yalain serves it, over the release `base` of the world database.
fn export_readme(key: &str, packs: &[String], base: &str, yalain: Option<&str>) -> String {
    let width = packs.iter().map(|p| p.len() + 1).max().unwrap_or(0).max(10);
    let mut files = format!(
        "  {:width$}  the world: its key, its packs, and the world database it lies over, {base}\n",
        "world.toml"
    );
    files.push_str(&format!(
        "  {:width$}  the ids its build gives its rows\n",
        "world.lock"
    ));
    for pack in packs {
        files.push_str(&format!(
            "  {:width$}  a pack: the server's rows it adds, changes and takes away\n",
            format!("{pack}/")
        ));
    }
    let checked = yalain.map_or_else(|| "Yalain".to_owned(), |v| format!("Yalain {v}"));
    format!(
        "{key}, an Athenaeum world, exported for Yalain.

{files}
Athenaeum changes only the server's rows, never the game's data files, so every client, retail
too, sees this world's changes from the server.

In yalain.toml (a path is relative to yalain.toml's folder; give full paths when it is
elsewhere):

  [server]
  world_database_path = \"<this folder>\"

Yalain serves it over the world database {base}. Put that release in the per-user cache before the
world starts:

  yalain world fetch --version {base}

or name a copy of it in yalain.toml's [server]:

  world_base_path = \"<that release's folder>\"

{checked} compiled it against the Asheron's Call 2 data files when it was exported. Check it
against yours:

  yalain world compile --dats <your data files> --world-db <this folder> --out <a scratch folder>
"
    )
}

/// The Yalain that checks an Athenaeum world: the one beside Athenaeum's editor, else Yalain's
/// download.
fn world_yalain(c: &components::Shared) -> Result<PathBuf, String> {
    let beside = components::program_of(c, Product::Athenaeum)
        .map(|editor| editor.with_file_name(format!("yalain{}", std::env::consts::EXE_SUFFIX)))
        .filter(|p| p.is_file());
    match beside {
        Some(p) => Ok(p),
        None => yalain_program(c),
    }
}

/// Export the Athenaeum world in the folder `world` for Yalain, into a folder the person chooses
/// ([`export_world`]). What it did, or why not, is the page's line.
#[tauri::command]
pub async fn export_athenaeum_world(
    app: tauri::AppHandle,
    a: State<'_, Shared>,
    c: State<'_, components::Shared>,
    b: State<'_, backend::Shared>,
    world: PathBuf,
) -> Result<(), String> {
    use tauri_plugin_dialog::DialogExt;
    if !crate::host_publication::athenaeum_world_file(&world).is_file() {
        return Err("That world is no longer there.".into());
    }
    let yalain = world_yalain(&c)?;
    let dats = dats(&a, &b, None)?;
    let Some(chosen) = app
        .dialog()
        .file()
        .set_title("Choose where the export for Yalain goes")
        .blocking_pick_folder()
    else {
        return Ok(());
    };
    let chosen = chosen.into_path().map_err(|e| e.to_string())?;
    let name = world
        .file_name()
        .map_or_else(|| "world".to_owned(), |n| n.to_string_lossy().into_owned());
    let out = export_folder(&chosen, &name);
    let a = a.inner().clone();
    {
        let mut store = lock(&a);
        if store.busy {
            return Err("Athenaeum is busy. Try again in a moment.".into());
        }
        store.busy = true;
        store.error = false;
        store.message = Some(format!("Exporting {name} for Yalain..."));
    }
    let to = out.clone();
    let result =
        tauri::async_runtime::spawn_blocking(move || export_world(&world, &to, &yalain, &dats))
            .await
            .map_err(|e| e.to_string())
            .and_then(|r| r);
    let mut store = lock(&a);
    store.busy = false;
    store.error = result.is_err();
    store.message = Some(match result {
        Ok(base) => format!(
            "Exported {name} for Yalain, over the world database {base}, to {}.",
            out.display()
        ),
        Err(error) => error,
    });
    Ok(())
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

    /// A world folder holding a `world.toml` and one pack, `main`.
    fn made_world(parent: &Path, name: &str) -> PathBuf {
        let w = parent.join(name);
        std::fs::create_dir_all(w.join("main")).unwrap();
        std::fs::write(
            w.join("world.toml"),
            format!("[world]\nkey = \"{name}\"\npacks = [\"main\"]\n"),
        )
        .unwrap();
        std::fs::write(w.join("main").join("pack.toml"), "name = 'main'\n").unwrap();
        w
    }

    /// As Lyceum's page: the worlds in Athenaeum's folder (those opened lately first, then the
    /// rest by name), then those opened elsewhere, newest first; each once, and a folder that is
    /// no world, or one part deleted, never.
    #[test]
    fn athenaeum_s_worlds_are_listed_as_lyceum_s_its_folder_first_then_those_opened_elsewhere() {
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path().join("data");
        let home = temp.path().join("Documents").join("Athenaeum");
        let (bravo, alpha) = (made_world(&home, "bravo"), made_world(&home, "alpha"));
        let charlie = made_world(&home, "charlie");
        std::fs::create_dir_all(home.join("notes")).unwrap();
        made_world(&home, ".deleting-gone-0");
        let elsewhere = made_world(&temp.path().join("elsewhere"), "far");
        remember(&data, &elsewhere);
        remember(&data, &charlie);
        let listed: Vec<(PathBuf, bool)> = list_worlds(Some(&home), &data)
            .into_iter()
            .map(|(w, h, _)| (w, h))
            .collect();
        let plain = |p: &Path| {
            components::list_worlds(None, &[p.to_path_buf()])[0]
                .0
                .parent()
                .unwrap()
                .to_path_buf()
        };
        assert_eq!(
            listed,
            [
                (plain(&charlie), true),
                (plain(&alpha), true),
                (plain(&bravo), true),
                (plain(&elsewhere), false),
            ]
        );
    }

    /// What `athenaeum facts` said of a world is shown while the world's files and the command
    /// are as they were when it was asked; a world never asked, or changed since, is asked again.
    /// With no command nothing is asked, and nothing shown.
    #[test]
    fn a_worlds_facts_are_asked_again_once_it_or_the_command_changes() {
        let then = Some(SystemTime::UNIX_EPOCH);
        let later = Some(SystemTime::UNIX_EPOCH + Duration::from_secs(60));
        let stamp = |n: u64| vec![(PathBuf::from("world.toml"), then, n)];
        let listed = vec![
            (PathBuf::from("w1"), true, stamp(1)),
            (PathBuf::from("w2"), false, stamp(2)),
        ];
        let facts = serde_json::json!({ "rows": { "object": 3 } });
        let mut said = HashMap::new();
        said.insert(PathBuf::from("w1"), ((then, stamp(1)), Some(facts.clone())));
        said.insert(PathBuf::from("w2"), ((then, stamp(1)), Some(facts.clone())));
        let (worlds, ask) = with_facts(listed.clone(), &said, Some(then));
        assert_eq!(worlds[0].facts, Some(facts.clone()));
        assert!(worlds[0].home && !worlds[1].home);
        assert_eq!(worlds[1].facts, None, "changed since");
        assert_eq!(ask, [(PathBuf::from("w2"), (then, stamp(2)))]);
        let (worlds, ask) = with_facts(listed.clone(), &said, Some(later));
        assert!(worlds.iter().all(|w| w.facts.is_none()));
        assert_eq!(ask.len(), 2, "a new command asks every world again");
        let (worlds, ask) = with_facts(listed, &said, None);
        assert!(worlds.iter().all(|w| w.facts.is_none()) && ask.is_empty());
    }

    /// Delete takes the world's whole folder and forgets it in the launcher's list; it refuses a
    /// folder that is no world, and Athenaeum's folder for worlds itself.
    #[test]
    fn deleting_a_world_takes_its_folder_and_forgets_it_and_nothing_else() {
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path().join("data");
        let home = temp.path().join("Documents").join("Athenaeum");
        let (gone, kept) = (made_world(&home, "gone"), made_world(&home, "kept"));
        std::fs::create_dir_all(gone.join(".playtest")).unwrap();
        std::fs::write(gone.join(".playtest").join("yalain.sqlite"), b"").unwrap();
        remember(&data, &kept);
        remember(&data, &gone);
        delete_world(&gone, Some(&home), &data).unwrap();
        assert!(!gone.exists());
        assert_eq!(opened_list(&data), std::slice::from_ref(&kept));
        assert_eq!(
            std::fs::read_dir(&home).unwrap().count(),
            1,
            "nothing left aside"
        );

        let plain = home.join("plain");
        std::fs::create_dir_all(&plain).unwrap();
        assert_eq!(
            delete_world(&plain, Some(&home), &data).unwrap_err(),
            "This folder does not appear to be an Athenaeum world"
        );
        assert!(plain.is_dir());
        std::fs::write(home.join("world.toml"), "[world]\n").unwrap();
        assert!(delete_world(&home, Some(&home), &data)
            .unwrap_err()
            .contains("Failed to delete Athenaeum world"));
        assert!(kept.join("world.toml").is_file());
    }

    /// A world one of whose files a program holds (an editor the launcher did not start) is left
    /// whole, and the launcher's list as it was.
    #[cfg(windows)]
    #[test]
    fn a_world_whose_file_is_held_open_is_not_deleted_at_all() {
        use std::os::windows::fs::OpenOptionsExt;
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path().join("data");
        let home = temp.path().join("Documents").join("Athenaeum");
        let world = made_world(&home, "held");
        remember(&data, &world);
        let _held = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(world.join("main").join("pack.toml"))
            .unwrap();
        assert_eq!(
            delete_world(&world, Some(&home), &data).unwrap_err(),
            STILL_OPEN
        );
        assert!(world.join("world.toml").is_file());
        assert_eq!(opened_list(&data), [world]);
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

    /// As on an Asheron's Call world, PLAY on an Asheron's Call 2 world with no password typed and
    /// none kept asks for one, and none is made up or kept.
    #[test]
    fn playing_with_no_password_typed_or_kept_asks_for_one() {
        let d = scratch("no-password");
        let b: backend::Shared = Arc::new(Mutex::new(backend::Backend::new(
            dereth_launch::folders::Folders::single(d.join("state")),
            Vec::new(),
            None,
            Box::new(dereth_launch::vault::MemoryVault::default()),
            "memory",
            None,
        )));
        let c = components::new(d.join("components"), &d, false);
        let a = new(d.join("data"), None);
        let choice = |password: Option<&str>| backend::Choice {
            world_slug: "shardfall".into(),
            account: "player".into(),
            password: password.map(Into::into),
            remember: true,
            client: ClientKind::Auberean,
            dat_set_id: None,
            classic_set_id: None,
        };
        for typed in [None, Some("")] {
            assert!(matches!(
                launch(&a, &c, &b, choice(typed)),
                backend::LaunchOutcome::NeedPassword { message } if message == "Enter the password for player."
            ));
        }
        assert!(!backend::lock(&b).has_password("shardfall", "player"));
        // A typed one goes on to the world, which this computer does not list.
        assert!(matches!(
            launch(&a, &c, &b, choice(Some("hunter2"))),
            backend::LaunchOutcome::Error { .. }
        ));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A Yalain that says its release facts (version 0.1.2, pinning v0.23.0) and answers every
    /// world command as `ok` says, keeping each command line in `said.txt` beside it.
    fn export_yalain(dir: &Path, ok: bool) -> PathBuf {
        std::fs::create_dir_all(dir).unwrap();
        let facts = r#"{"version":"0.1.2","world_database":"v0.23.0"}"#;
        if cfg!(windows) {
            let p = dir.join("yalain.cmd");
            let compile = if ok {
                ""
            } else {
                "@if \"%2\"==\"compile\" (echo yalain: a row names nothing 1>&2\r\nexit /b 1)\r\n"
            };
            std::fs::write(
                &p,
                format!(
                    "@echo %* >> \"%~dp0said.txt\"\r\n@if \"%1\"==\"--release-facts\" echo {facts}\r\n{compile}@exit /b 0\r\n"
                ),
            )
            .unwrap();
            p
        } else {
            let p = dir.join("yalain");
            let compile = if ok {
                ""
            } else {
                "if [ \"$2\" = compile ]; then echo 'yalain: a row names nothing' >&2; exit 1; fi\n"
            };
            std::fs::write(
                &p,
                format!(
                    "#!/bin/sh\necho \"$@\" >> \"$(dirname \"$0\")/said.txt\"\nif [ \"$1\" = --release-facts ]; then echo '{facts}'; fi\n{compile}exit 0\n"
                ),
            )
            .unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
            }
            p
        }
    }

    /// An Athenaeum world in `dir` as `athenaeum new` makes one, its base named by name only, with
    /// a row in its pack and what playing and building it leave beside it.
    fn export_world_at(dir: &Path) {
        std::fs::create_dir_all(dir.join("fill-frontier/server")).unwrap();
        std::fs::write(
            dir.join("world.toml"),
            "# An Asheron's Call 2 world.\nformat_version = 1\nkey = \"frontier\"\ngame = \"ac2\"\nera = \"eor\"\npacks = [\"fill-frontier\"]\n\n# The database under every pack.\n[base]\ndb = \"yalain-world\"\n",
        )
        .unwrap();
        std::fs::write(dir.join("world.lock"), "# ids\n").unwrap();
        std::fs::write(
            dir.join("fill-frontier/pack.toml"),
            "name = \"fill-frontier\"\n",
        )
        .unwrap();
        std::fs::write(dir.join("fill-frontier/server/people.toml"), "[[person]]\n").unwrap();
        for left in [".playtest", "build", "characters"] {
            std::fs::create_dir_all(dir.join(left)).unwrap();
            std::fs::write(dir.join(left).join("kept.txt"), "not the world's").unwrap();
        }
    }

    /// Export For writes an Athenaeum world as a Yalain world database package: its world file,
    /// ids and packs and nothing it leaves beside them, the release it was checked over named in
    /// its world file, a README that says how a Yalain serves it, the build checked by Yalain's
    /// compile; a later export replaces it. A folder holding other files, or in the world's own
    /// folder, is refused, and a world that does not compile is not exported.
    #[test]
    fn an_athenaeum_world_exports_for_yalain_as_a_world_database_package() {
        let d = scratch("export");
        let world = d.join("Athenaeum").join("frontier");
        export_world_at(&world);
        let yalain = export_yalain(&d.join("bin"), true);
        let dats = d.join("ac2");
        let out = d.join("out");
        assert_eq!(
            export_world(&world, &out, &yalain, &dats).as_deref(),
            Ok("v0.23.0")
        );
        for file in [
            "world.toml",
            "world.lock",
            "fill-frontier/pack.toml",
            "fill-frontier/server/people.toml",
            "README.txt",
            EXPORT_MARKER,
        ] {
            assert!(out.join(file).is_file(), "{file}");
        }
        for left in [".playtest", "build", "characters", EXPORT_STAGING] {
            assert!(!out.join(left).exists(), "{left}");
        }
        let file = std::fs::read_to_string(out.join("world.toml")).unwrap();
        assert!(file.contains("version = \"v0.23.0\""), "{file}");
        assert!(file.contains("# The database under every pack."), "{file}");
        assert!(
            !std::fs::read_to_string(world.join("world.toml"))
                .unwrap()
                .contains("version = \"v0"),
            "the world itself is not changed"
        );
        let readme = std::fs::read_to_string(out.join("README.txt")).unwrap();
        for said in [
            "frontier, an Athenaeum world, exported for Yalain.",
            "fill-frontier/",
            "never the game's data files",
            "world_database_path = \"<this folder>\"",
            "yalain world fetch --version v0.23.0",
            "Yalain 0.1.2 compiled it",
        ] {
            assert!(readme.contains(said), "{said}: {readme}");
        }
        let ran = std::fs::read_to_string(d.join("bin/said.txt")).unwrap();
        assert!(ran.contains("world fetch --version v0.23.0"), "{ran}");
        assert!(ran.contains("world compile --dats"), "{ran}");

        // Again: an earlier export is replaced whole.
        std::fs::write(out.join("stale.txt"), "x").unwrap();
        export_world(&world, &out, &yalain, &dats).unwrap();
        assert!(!out.join("stale.txt").exists());
        // A world that names its release keeps it.
        let named = d.join("named");
        export_world_at(&named);
        let text = std::fs::read_to_string(named.join("world.toml")).unwrap();
        std::fs::write(
            named.join("world.toml"),
            format!("{text}version = \"v0.22.0\"\n"),
        )
        .unwrap();
        assert_eq!(
            export_world(&named, &d.join("named-out"), &yalain, &dats).as_deref(),
            Ok("v0.22.0")
        );

        // Refused: a folder of other files, the world's own folder, a world that does not compile.
        let other = d.join("other");
        std::fs::create_dir_all(&other).unwrap();
        std::fs::write(other.join("mine.txt"), "x").unwrap();
        let refused = export_world(&world, &other, &yalain, &dats).unwrap_err();
        assert!(refused.contains("holds other files"), "{refused}");
        assert!(other.join("mine.txt").is_file());
        let inside = world.join("export");
        let refused = export_world(&world, &inside, &yalain, &dats).unwrap_err();
        assert!(refused.contains("world's own folder"), "{refused}");
        assert!(!inside.exists());
        let bad = export_yalain(&d.join("bad"), false);
        let refused = export_world(&world, &d.join("bad-out"), &bad, &dats).unwrap_err();
        assert!(
            refused.starts_with("The world does not compile"),
            "{refused}"
        );
        assert!(!d.join("bad-out/README.txt").exists());

        // Where it goes: the folder chosen when it is empty or an export, else a folder in it.
        let empty = d.join("empty");
        std::fs::create_dir_all(&empty).unwrap();
        assert_eq!(export_folder(&empty, "frontier"), empty);
        assert_eq!(export_folder(&out, "frontier"), out);
        assert_eq!(
            export_folder(&other, "frontier"),
            other.join("frontier-yalain")
        );
        let _ = std::fs::remove_dir_all(&d);
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
