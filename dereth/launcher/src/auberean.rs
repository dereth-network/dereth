//! Auberean's realm: the player's Asheron's Call 2 install, a world on this computer run with
//! Yalain, the Auberean client started into a world, and Athenaeum's worlds.
//!
//! The three programs are components ([`crate::components`]): downloaded, updated and removed as
//! Empyrean and Lyceum are, or this checkout's builds in a development build. What this module adds
//! is what runs them, over what [`dereth_launch::auberean`] knows:
//!
//! - **The install.** The player names their own install's folder; the launcher checks its data
//!   files ([`dereth_launch::auberean::check_install`]) and keeps the folder in its state. Nothing
//!   of the game is downloaded or copied: the client and Yalain read the folder named.
//! - **A world on this computer.** Start fetches the world database Yalain's release pins into the
//!   player's cache when it is not there yet (`yalain world fetch`), then runs Yalain over the
//!   install, its accounts and characters in `auberean/yalain.sqlite` in the launcher's data
//!   folder, listening on this computer alone. Its output is kept in `auberean/yalain.log` there,
//!   whose last lines the page shows. Stop ends it at once.
//! - **Playing.** The client is started over the install, logging in to the world on this computer
//!   (started first, and waited for, when it is not running) or to a listed world by its address,
//!   with the player's account. The password, for a world that asks one, goes to the client and is
//!   never kept or shown.
//! - **Athenaeum's worlds.** New World makes one with `athenaeum new` in the player's Documents and
//!   opens the editor on it; Open World opens one by its folder. The editor finds Yalain and the
//!   client beside itself.

use std::net::{SocketAddr, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use dereth_launch::auberean::{self as ac2, InstallCheck};
use dereth_launch::components::Product;
use serde::Serialize;
use tauri::State;

use crate::backend;
use crate::components;

/// How long Play waits for a world on this computer to be ready before it gives up.
const READY_WAIT: Duration = Duration::from_secs(180);

/// How many of the log's last lines the page shows.
const LOG_LINES: usize = 200;

/// The most of the log the page reads: its end.
const LOG_READ: u64 = 64 * 1024;

/// The folder in the person's Documents that Athenaeum's new worlds go in.
pub const WORLDS_FOLDER: &str = "Athenaeum";

/// The launcher's own list of the Athenaeum worlds it made or opened, newest first.
const OPENED_FILE: &str = "athenaeum-worlds.json";

/// How many worlds that list keeps.
const OPENED_KEPT: usize = 20;

pub struct Store {
    /// The launcher's data folder.
    data: PathBuf,
    /// The world on this computer, while it runs.
    server: Option<Child>,
    /// Getting the world database, or waiting for the world to be ready.
    starting: bool,
    /// What the last action said, and whether it went wrong.
    message: Option<String>,
    error: bool,
    /// The clients the launcher started.
    clients: Vec<Child>,
    /// The editors the launcher started.
    editors: Vec<Child>,
    /// The last check of the install, by the folder it checked.
    check: Option<InstallCheck>,
    /// Where Athenaeum's new worlds go.
    home: Option<PathBuf>,
}

pub type Shared = Arc<Mutex<Store>>;

pub fn new(data: PathBuf) -> Shared {
    Arc::new(Mutex::new(Store {
        data,
        server: None,
        starting: false,
        message: None,
        error: false,
        clients: Vec::new(),
        editors: Vec::new(),
        check: None,
        home: None,
    }))
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
    pub server: ServerView,
    /// How many clients the launcher started are running.
    pub playing: usize,
    /// Whether an editor the launcher started is open.
    pub editing: bool,
    /// Athenaeum's worlds the launcher made or opened, newest first.
    pub worlds: Vec<PathBuf>,
    pub home: Option<PathBuf>,
    pub message: Option<String>,
    pub error: bool,
}

/// The world on this computer, as the page shows it.
#[derive(Clone, Serialize)]
pub struct ServerView {
    pub running: bool,
    /// Ready for logins: its log says where it listens.
    pub ready: bool,
    pub starting: bool,
    /// Where the client logs in.
    pub address: String,
    /// Its log's last lines.
    pub log: Vec<String>,
}

fn log_file(data: &Path) -> PathBuf {
    data.join("auberean").join("yalain.log")
}

/// The end of the log at `path`.
fn read_log(path: &Path) -> String {
    use std::io::{Read, Seek, SeekFrom};
    let Ok(mut f) = std::fs::File::open(path) else {
        return String::new();
    };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    let _ = f.seek(SeekFrom::Start(len.saturating_sub(LOG_READ)));
    let mut bytes = Vec::new();
    let _ = f.read_to_end(&mut bytes);
    String::from_utf8_lossy(&bytes).into_owned()
}

/// The address the client logs in to the world on this computer at.
fn local_address() -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], ac2::LOCAL_LOGIN_PORT))
}

/// The page's view, with the install the backend keeps checked again when it changed.
pub fn view(shared: &Shared, b: &backend::Shared) -> View {
    let settings = backend::lock(b).auberean_settings();
    let mut store = lock(shared);
    let running = store
        .server
        .as_mut()
        .is_some_and(|c| matches!(c.try_wait(), Ok(None)));
    if !running {
        store.server = None;
    }
    store
        .clients
        .retain_mut(|c| matches!(c.try_wait(), Ok(None)));
    store
        .editors
        .retain_mut(|c| matches!(c.try_wait(), Ok(None)));
    if let Some(install) = &settings.install {
        if store.check.as_ref().is_none_or(|c| &c.dir != install) {
            store.check = Some(ac2::check_install(install));
        }
    }
    let log = read_log(&log_file(&store.data));
    View {
        check: settings.install.as_ref().and(store.check.clone()),
        server: ServerView {
            running,
            ready: running && ac2::is_ready(&log),
            starting: store.starting,
            address: local_address().to_string(),
            log: ac2::tail(&log, LOG_LINES),
        },
        playing: store.clients.len(),
        editing: !store.editors.is_empty(),
        worlds: opened_worlds(&store.data),
        home: store.home.clone(),
        message: store.message.clone(),
        error: store.error,
    }
}

/// Say `text` on the page, as an error when `error`.
fn say(shared: &Shared, text: impl Into<String>, error: bool) {
    let mut store = lock(shared);
    store.message = Some(text.into());
    store.error = error;
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
    lock(shared).check = Some(check);
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
    backend::lock(&b).set_auberean_install(dir);
    lock(&a).check = Some(check.clone());
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

/// Get the world database and start the world on this computer, unless it is running. Waits until
/// it is started, not until it is ready: the page shows it getting ready.
fn start_server(
    shared: &Shared,
    c: &components::Shared,
    b: &backend::Shared,
) -> Result<(), String> {
    let install = install(shared, b)?;
    let yalain = yalain_program(c)?;
    if components::busy(c, Product::Yalain) {
        return Err("Yalain is downloading. Try again when it is done.".into());
    }
    {
        let mut store = lock(shared);
        if store.server.is_some() || store.starting {
            return Ok(());
        }
        store.starting = true;
        store.message = Some("Getting the world database...".into());
        store.error = false;
    }
    let started = (|| {
        let fetched = command(&yalain)
            .args(ac2::WORLD_FETCH_ARGS)
            .stdin(Stdio::null())
            .output()
            .map_err(|e| format!("Could not run Yalain: {e}"))?;
        if !fetched.status.success() {
            let said = String::from_utf8_lossy(&fetched.stderr);
            let said = said.lines().last().unwrap_or("").trim();
            return Err(format!("Could not get the world database. {said}"));
        }
        let data = lock(shared).data.clone();
        let database = ac2::local_database(&data);
        let log = log_file(&data);
        if let Some(dir) = log.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let _ = std::fs::rename(&log, log.with_extension("log.old"));
        let out = std::fs::File::create(&log).map_err(|e| e.to_string())?;
        let err = out.try_clone().map_err(|e| e.to_string())?;
        let child = command(&yalain)
            .args(ac2::yalain_args(&install, &database, ac2::LOCAL_LOGIN_PORT))
            .stdin(Stdio::null())
            .stdout(out)
            .stderr(err)
            .spawn()
            .map_err(|e| format!("Could not start Yalain: {e}"))?;
        Ok(child)
    })();
    let mut store = lock(shared);
    store.starting = false;
    match started {
        Ok(child) => {
            store.server = Some(child);
            store.message = None;
            Ok(())
        }
        Err(e) => {
            store.message = Some(e.clone());
            store.error = true;
            Err(e)
        }
    }
}

/// Start the world on this computer.
#[tauri::command]
pub async fn start_auberean_world(
    a: State<'_, Shared>,
    c: State<'_, components::Shared>,
    b: State<'_, backend::Shared>,
) -> Result<(), String> {
    let (a, c, b) = (a.inner().clone(), c.inner().clone(), b.inner().clone());
    tauri::async_runtime::spawn_blocking(move || start_server(&a, &c, &b))
        .await
        .map_err(|e| e.to_string())?
}

/// Stop the world on this computer, at once.
#[tauri::command]
pub fn stop_auberean_world(a: State<'_, Shared>) {
    stop(&a);
}

/// End the world on this computer, if it runs.
pub fn stop(shared: &Shared) {
    if let Some(mut child) = lock(shared).server.take() {
        let _ = child.kill();
        let _ = child.wait();
    }
}

/// Start the client into the world at `server` as `account` with `password`.
fn start_client(
    shared: &Shared,
    c: &components::Shared,
    install: &Path,
    server: SocketAddr,
    account: &str,
    password: &str,
) -> Result<(), String> {
    let client = components::program_of(c, Product::Auberean).ok_or("Download Auberean first.")?;
    let args = ac2::client_args(install, server, account, password);
    eprintln!(
        "launcher: {} {}",
        client.display(),
        ac2::shown_client_args(&args)
    );
    let child = command(&client)
        .args(&args)
        .stdin(Stdio::null())
        .spawn()
        .map_err(|e| format!("Could not start Auberean: {e}"))?;
    lock(shared).clients.push(child);
    Ok(())
}

/// Play the world on this computer as `account`: start it first when it is not running, wait for
/// it to be ready, then start the client into it.
#[tauri::command]
pub async fn play_auberean_local(
    a: State<'_, Shared>,
    c: State<'_, components::Shared>,
    b: State<'_, backend::Shared>,
    account: String,
) -> Result<(), String> {
    let account = ac2::account_name(&account)
        .map_err(|e| e.to_string())?
        .to_owned();
    let (a, c, b) = (a.inner().clone(), c.inner().clone(), b.inner().clone());
    tauri::async_runtime::spawn_blocking(move || {
        let install = install(&a, &b)?;
        components::program_of(&c, Product::Auberean).ok_or("Download Auberean first.")?;
        backend::lock(&b).set_auberean_account(&account);
        start_server(&a, &c, &b)?;
        let data = lock(&a).data.clone();
        say(&a, "Waiting for the world to be ready...", false);
        let began = Instant::now();
        loop {
            if !lock(&a)
                .server
                .as_mut()
                .is_some_and(|s| matches!(s.try_wait(), Ok(None)))
            {
                let e = "The world stopped before it was ready. Its log says why.".to_owned();
                say(&a, e.clone(), true);
                return Err(e);
            }
            if ac2::is_ready(&read_log(&log_file(&data))) {
                break;
            }
            if began.elapsed() > READY_WAIT {
                let e = "The world is taking too long to start. Its log says why.".to_owned();
                say(&a, e.clone(), true);
                return Err(e);
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        lock(&a).message = None;
        start_client(&a, &c, &install, local_address(), &account, "")
    })
    .await
    .map_err(|e| e.to_string())?
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

/// Play the listed world `slug` as `account` with `password`.
#[tauri::command]
pub async fn play_auberean_world(
    a: State<'_, Shared>,
    c: State<'_, components::Shared>,
    b: State<'_, backend::Shared>,
    slug: String,
    account: String,
    password: String,
) -> Result<(), String> {
    let account = ac2::account_name(&account)
        .map_err(|e| e.to_string())?
        .to_owned();
    let (a, c, b) = (a.inner().clone(), c.inner().clone(), b.inner().clone());
    tauri::async_runtime::spawn_blocking(move || {
        let install = install(&a, &b)?;
        let (host, port) = backend::lock(&b)
            .world_address(&slug)
            .ok_or("This world names no address to connect to.")?;
        let server = resolve(&host, port)?;
        backend::lock(&b).set_auberean_account(&account);
        start_client(&a, &c, &install, server, &account, &password)
    })
    .await
    .map_err(|e| e.to_string())?
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
    fn the_log_shown_is_the_end_of_the_file() {
        let d = scratch("log");
        let log = log_file(&d);
        std::fs::create_dir_all(log.parent().unwrap()).unwrap();
        let mut text = "x".repeat(LOG_READ as usize);
        text.push_str("\nlistening on 127.0.0.1:9200/9201 (login)\n");
        std::fs::write(&log, &text).unwrap();
        let read = read_log(&log);
        assert!(read.len() as u64 <= LOG_READ);
        assert!(ac2::is_ready(&read));
        assert_eq!(read_log(&d.join("none.log")), "");
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
