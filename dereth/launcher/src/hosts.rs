//! The servers a player runs: each in a folder of its own, `hosts/<id>` in the launcher's data
//! folder, with its configuration, databases, log and its own copy of the server it runs.

use crate::host_config::Settings;
use crate::host_runtime::{self, Definition, Status};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Child;
use std::sync::{Arc, Mutex};
use tauri::State;
use tauri_plugin_dialog::DialogExt;

pub struct Store {
    root: PathBuf,
    busy: BTreeMap<String, String>,
    children: Vec<(String, Child)>,
    messages: BTreeMap<String, String>,
    closing: bool,
    creating: bool,
}
pub type Shared = Arc<Mutex<Store>>;
struct CreationGuard(Shared);
impl Drop for CreationGuard {
    fn drop(&mut self) {
        lock(&self.0).creating = false;
    }
}
#[derive(Serialize)]
pub struct View {
    pub id: String,
    pub settings: Settings,
    pub version: String,
    /// The server runs this checkout's build.
    pub checkout: bool,
    pub status: Status,
    pub busy: Option<String>,
    pub message: Option<String>,
    pub logs: Vec<String>,
    pub prepared: bool,
    pub service: bool,
    pub update: crate::host_updates::View,
    pub publication: Option<crate::host_publication::Publication>,
    /// Its folder, where its configuration, databases and log are.
    pub folder: PathBuf,
    /// Its slug on the Dereth Network, as its configuration gives it (`[federation] slug`).
    pub federation_slug: Option<String>,
    /// That slug is on the Dereth Network's list.
    pub listed: bool,
    /// What it plays besides its settings: its world database, overlays and network key.
    pub plays: crate::host_config::Plays,
    /// For a world that plays a published Lyceum world: whether that world's saved sources differ
    /// from the ones it was published from. `None` while that is not known yet.
    pub unpublished_changes: Option<bool>,
    /// The Lyceum world it plays was published from a world file that is gone.
    pub source_gone: bool,
}

/// Whether a world whose configuration names `slug` is on the Dereth Network's list, whose
/// worlds' slugs are `hub`.
#[must_use]
pub fn listed(slug: Option<&str>, hub: &std::collections::HashSet<String>) -> bool {
    slug.map(str::trim)
        .filter(|s| !s.is_empty())
        .is_some_and(|s| hub.iter().any(|h| h.eq_ignore_ascii_case(s)))
}

/// `[federation] slug` in a configuration's text; none when it is not there or is blank.
#[must_use]
pub fn federation_slug(text: &str) -> Option<String> {
    let table: toml::Table = text.parse().ok()?;
    table
        .get("federation")?
        .get("slug")?
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}
fn lock(shared: &Shared) -> std::sync::MutexGuard<'_, Store> {
    shared
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
pub fn new(root: PathBuf) -> Shared {
    Arc::new(Mutex::new(Store {
        root: root.join("hosts"),
        busy: BTreeMap::new(),
        children: Vec::new(),
        messages: BTreeMap::new(),
        closing: false,
        creating: false,
    }))
}
fn host_dir(root: &Path, id: &str) -> Result<PathBuf, String> {
    if id.is_empty() || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
        return Err("Invalid world identity.".into());
    }
    let dir = root.join(id);
    let definition = host_runtime::read_definition(&dir)?;
    if definition.id != id {
        return Err("The world's identity does not match its folder.".into());
    }
    Ok(dir)
}
pub fn views(shared: &Shared) -> Vec<View> {
    let mut store = lock(shared);
    store
        .children
        .retain_mut(|(_, c)| !matches!(c.try_wait(), Ok(Some(_))));
    let mut views = Vec::new();
    let Ok(entries) = std::fs::read_dir(&store.root) else {
        return views;
    };
    for e in entries.flatten() {
        let dir = e.path();
        let Ok(definition) = host_runtime::read_definition(&dir) else {
            continue;
        };
        let text = std::fs::read_to_string(dir.join("empyrean.toml")).unwrap_or_default();
        let settings = Settings::from_config(&text).unwrap_or_default();
        let logs = std::fs::read(dir.join("log.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        let publication = crate::host_publication::selected(&dir)
            .ok()
            .flatten()
            .map(|(_, p)| p);
        let source_gone = publication.as_ref().is_some_and(|p| !p.source.is_file());
        let unpublished_changes = publication.as_ref().and_then(|p| {
            if p.sources.is_empty() || !p.source.is_file() {
                return None;
            }
            crate::lyceum_world::current_digest(&p.source).map(|now| now != p.sources)
        });
        views.push(View {
            busy: store.busy.get(&definition.id).cloned(),
            message: store.messages.get(&definition.id).cloned(),
            id: definition.id,
            settings,
            version: definition.version,
            checkout: definition.checkout,
            status: host_runtime::status(&dir),
            logs,
            prepared: crate::host_publication::prepared(&dir),
            service: crate::host_service::installed(&dir),
            update: crate::host_updates::view(&dir),
            publication,
            unpublished_changes,
            source_gone,
            federation_slug: federation_slug(&text),
            listed: false,
            plays: crate::host_config::plays(&text, &dir),
            folder: dir,
        });
    }
    views.sort_by(|a, b| a.id.cmp(&b.id));
    views
}
pub(crate) fn copy_tree(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|e| e.to_string())?;
    for e in std::fs::read_dir(from).map_err(|e| e.to_string())? {
        let e = e.map_err(|e| e.to_string())?;
        let kind = e.file_type().map_err(|e| e.to_string())?;
        if kind.is_symlink() {
            return Err("Empyrean's folder contains a link.".into());
        }
        if kind.is_dir() {
            copy_tree(&e.path(), &to.join(e.file_name()))?;
        } else if kind.is_file() {
            std::fs::copy(e.path(), to.join(e.file_name())).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
pub(crate) fn validate_config(program: &Path, dir: &Path, text: &str) -> Result<(), String> {
    crate::host_config::validate_managed(text, dir)?;
    let pending = dir.join("pending.toml");
    let checked = dir.join("checked.toml");
    std::fs::write(&pending, text).map_err(|e| e.to_string())?;
    let result = crate::components::command(program)
        .arg("--write-config")
        .arg("--config")
        .arg(&pending)
        .arg("--out")
        .arg(&checked)
        .current_dir(dir)
        .output()
        .map_err(|e| e.to_string());
    let _ = std::fs::remove_file(checked);
    let result = result?;
    if !result.status.success() {
        let _ = std::fs::remove_file(pending);
        return Err(crate::host_config::redact(&String::from_utf8_lossy(
            &result.stderr,
        )));
    }
    std::fs::OpenOptions::new()
        .write(true)
        .open(&pending)
        .and_then(|f| f.sync_all())
        .map_err(|e| e.to_string())?;
    std::fs::rename(pending, dir.join("empyrean.toml")).map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn choose_host_dats(app: tauri::AppHandle) -> Result<Option<PathBuf>, String> {
    app.dialog()
        .file()
        .set_title("Choose the folder holding your game's data files")
        .blocking_pick_folder()
        .map(|f| f.into_path().map_err(|e| e.to_string()))
        .transpose()
}
#[tauri::command]
pub async fn create_host(
    shared: State<'_, Shared>,
    components: State<'_, crate::components::Shared>,
    mut settings: Settings,
) -> Result<String, String> {
    settings.validate()?;
    if !settings.dats.is_dir() {
        return Err("The game data folder does not exist.".into());
    }
    crate::host_config::check_game_data(&settings.era, &settings.dats)?;
    // The status endpoint the launcher counts the world's players at, on this computer: a port
    // no other world names and nothing holds now, kept from here on.
    let taken: Vec<u16> = views(&shared)
        .iter()
        .map(|h| h.settings.status_port)
        .collect();
    settings.status_port = crate::host_config::free_status_port(&taken, |port| {
        std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).is_ok()
    })
    .ok_or("No port is free on this computer for the world's status.")?;
    let server = crate::components::server(&components)?;
    let root = {
        let mut store = lock(&shared);
        if store.closing || store.creating {
            return Err("The launcher is busy. Try again in a moment.".into());
        }
        let root = std::path::absolute(&store.root).map_err(|e| e.to_string())?;
        store.creating = true;
        root
    };
    let _creating = CreationGuard(shared.inner().clone());
    let id = format!(
        "host-{:x}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos()
    );
    let dir = root.join(&id);
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    std::fs::create_dir(&dir).map_err(|e| e.to_string())?;
    let cleanup_dir = dir.clone();
    let create_id = id.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        // A server keeps its own copy of the server it runs, which its updates change; this
        // checkout's build is run where it is, so each start runs the newest build.
        let program = if server.checkout {
            server.program.clone()
        } else {
            let runtime = dir.join("runtime").join(&server.version);
            copy_tree(
                server.program.parent().ok_or("Invalid server path")?,
                &runtime,
            )?;
            runtime.join(
                server
                    .program
                    .file_name()
                    .ok_or("Invalid server filename")?,
            )
        };
        let manager = dir.join("manager");
        std::fs::create_dir(&manager).map_err(|e| e.to_string())?;
        let wrapper = manager.join(if cfg!(windows) {
            "dereth-host.exe"
        } else {
            "dereth-host"
        });
        std::fs::copy(
            std::env::current_exe().map_err(|e| e.to_string())?,
            &wrapper,
        )
        .map_err(|e| e.to_string())?;
        validate_config(&program, &dir, &settings.update_config("")?)?;
        host_runtime::write_json(
            &dir.join("host.json"),
            &Definition {
                id: create_id,
                version: server.version,
                program,
                wrapper,
                checkout: server.checkout,
            },
        )
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|result| result);
    if let Err(error) = result {
        // This folder was created exclusively above and has never hosted a running world.
        let _ = std::fs::remove_dir_all(cleanup_dir);
        return Err(format!("Could not make the world: {error}"));
    }
    Ok(id)
}
fn begin(shared: &Shared, id: &str, message: &str) -> Result<PathBuf, String> {
    let mut store = lock(shared);
    let dir = host_dir(&store.root, id)?;
    if store.closing {
        return Err("The launcher is waiting for your worlds to stop.".into());
    }
    if store.busy.contains_key(id) {
        return Err("This world is busy. Try again in a moment.".into());
    }
    store.busy.insert(id.into(), message.into());
    store.messages.remove(id);
    Ok(dir)
}
fn finish(shared: &Shared, id: &str, result: Result<(), String>, message: &str) {
    let mut store = lock(shared);
    store.busy.remove(id);
    match result {
        Err(error) => {
            store.messages.insert(id.into(), error);
        }
        Ok(()) if !message.is_empty() => {
            store.messages.insert(id.into(), message.into());
        }
        Ok(()) => {}
    }
}
#[tauri::command]
pub fn prepare_host(shared: State<'_, Shared>, id: String) -> Result<(), String> {
    let shared = shared.inner().clone();
    let dir = begin(&shared, &id, "Downloading the world database...")?;
    if crate::host_publication::prepared(&dir) {
        finish(&shared, &id, Ok(()), "The world database is ready.");
        return Ok(());
    }
    if host_runtime::active(&dir) {
        finish(
            &shared,
            &id,
            Err("Stop the world before getting its world database.".into()),
            "",
        );
        return Ok(());
    }
    std::thread::spawn(move || {
        let result = (|| {
            let definition = host_runtime::read_definition(&dir)?;
            let import = definition
                .program
                .with_file_name(format!("empyrean-import{}", std::env::consts::EXE_SUFFIX));
            if !import.is_file() {
                return Err(if definition.checkout {
                    "This checkout has no build of empyrean-import beside empyrean-server."
                        .to_owned()
                } else {
                    "empyrean-import is missing beside empyrean-server.".to_owned()
                });
            }
            // The era's own world database: ACE's world for the end of retail, its Infiltration
            // world for that era.
            let text =
                std::fs::read_to_string(dir.join("empyrean.toml")).map_err(|e| e.to_string())?;
            let era = Settings::from_config(&text)?.era;
            let mut fetch = crate::components::command(&import);
            fetch.arg("fetch");
            if era == "infiltration" {
                fetch.args(["--world", "16py"]);
            }
            let pack = crate::host_publication::pack_path(&dir);
            let output = fetch
                .args(["--pack", "--out"])
                .arg(&pack)
                .current_dir(&dir)
                .output()
                .map_err(|e| e.to_string())?;
            if output.status.success() {
                // The release the world database was made from, as the import tool named it,
                // kept beside it for the world's page.
                let record = crate::host_config::base_record(&pack);
                match crate::host_config::fetched_dump(&String::from_utf8_lossy(&output.stdout)) {
                    Some(dump) => {
                        std::fs::write(record, format!("{dump}\n")).map_err(|e| e.to_string())?;
                    }
                    None => {
                        let _ = std::fs::remove_file(record);
                    }
                }
                Ok(())
            } else {
                Err(crate::host_config::redact(&String::from_utf8_lossy(
                    &output.stderr,
                )))
            }
        })();
        finish(&shared, &id, result, "The world database is ready.");
    });
    Ok(())
}
#[tauri::command]
pub fn start_host(shared: State<'_, Shared>, id: String) -> Result<(), String> {
    let mut store = lock(&shared);
    let dir = host_dir(&store.root, &id)?;
    if store.closing || store.busy.contains_key(&id) || host_runtime::active(&dir) {
        return Err("This world is busy or already running.".into());
    }
    if !crate::host_publication::prepared(&dir) {
        return Err("Get the world database first.".into());
    }
    if crate::host_service::installed(&dir) {
        return crate::host_service::start(&dir);
    }
    let definition = host_runtime::read_definition(&dir)?;
    let _ = std::fs::remove_file(dir.join("stop.request"));
    let child = crate::components::command(&definition.wrapper)
        .arg("--run-host")
        .arg(&dir)
        .arg(std::process::id().to_string())
        .current_dir(&dir)
        .spawn()
        .map_err(|e| e.to_string())?;
    store.children.push((id.clone(), child));
    store.messages.remove(&id);
    Ok(())
}
#[tauri::command]
pub fn stop_host(shared: State<'_, Shared>, id: String) -> Result<(), String> {
    let store = lock(&shared);
    let dir = host_dir(&store.root, &id)?;
    host_runtime::request_stop(&dir)
}
#[tauri::command]
pub async fn save_host(
    shared: State<'_, Shared>,
    id: String,
    settings: Settings,
) -> Result<(), String> {
    settings.validate()?;
    let shared = shared.inner().clone();
    let dir = begin(&shared, &id, "Checking the settings...")?;
    let result = tauri::async_runtime::spawn_blocking(move || {
        let definition = host_runtime::read_definition(&dir)?;
        let text = std::fs::read_to_string(dir.join("empyrean.toml")).map_err(|e| e.to_string())?;
        let before = Settings::from_config(&text)?;
        if cfg!(windows) && crate::host_service::installed(&dir) && before.dats != settings.dats {
            return Err(
                "Turn off running in the background before changing the game data folder.".into(),
            );
        }
        validate_config(
            &definition.program,
            &dir,
            &crate::host_config::save_over(&text, &dir, &settings)?,
        )
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|result| result);
    let running = dir_of(&shared, &id).is_ok_and(|dir| host_runtime::active(&dir));
    finish(
        &shared,
        &id,
        result,
        if running {
            "Saved. Restart the world to use the new settings."
        } else {
            "Saved."
        },
    );
    Ok(())
}
/// Keep the world's Dereth Network key, pasted from the network's registration, in its folder,
/// and name that file in its configuration ([`crate::host_config::save_network_key`]). A paste
/// that is not a key is refused in plain words. The key is never logged or put in a message.
#[tauri::command]
pub async fn save_host_key(
    shared: State<'_, Shared>,
    id: String,
    key: String,
) -> Result<(), String> {
    crate::host_config::network_key(&key)?;
    let shared = shared.inner().clone();
    let dir = begin(&shared, &id, "Saving the key...")?;
    let result = tauri::async_runtime::spawn_blocking(move || {
        let definition = host_runtime::read_definition(&dir)?;
        let text = std::fs::read_to_string(dir.join("empyrean.toml")).map_err(|e| e.to_string())?;
        let next = crate::host_config::save_network_key(&dir, &text, &key)?;
        if next == text {
            Ok(())
        } else {
            validate_config(&definition.program, &dir, &next)
        }
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|result| result);
    finish(&shared, &id, Ok(()), "");
    result
}
fn dir_of(shared: &Shared, id: &str) -> Result<PathBuf, String> {
    host_dir(&lock(shared).root, id)
}
#[tauri::command]
pub fn open_host_config(shared: State<'_, Shared>, id: String) -> Result<(), String> {
    let dir = dir_of(&shared, &id)?;
    let editor = if cfg!(windows) {
        "notepad.exe"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let mut command = std::process::Command::new(editor);
    if cfg!(target_os = "macos") {
        // A .toml file has no application of its own on a Mac: open it as text.
        command.arg("-t");
    }
    command
        .arg(dir.join("empyrean.toml"))
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}
/// Show the server's folder: its configuration, databases, log and backups.
#[tauri::command]
pub fn open_host_folder(shared: State<'_, Shared>, id: String) -> Result<(), String> {
    crate::components::open_folder(&dir_of(&shared, &id)?)
}

/// Show one of a world's files in the system's file browser: its `config` (`empyrean.toml`), its
/// `log`, or its `folder`.
#[tauri::command]
pub fn reveal_host(shared: State<'_, Shared>, id: String, place: String) -> Result<(), String> {
    let dir = dir_of(&shared, &id)?;
    let path = match place.as_str() {
        "config" => dir.join("empyrean.toml"),
        "log" => dir.join(host_runtime::LOG_FILE),
        "folder" => dir,
        _ => return Err("The launcher does not know where that is.".into()),
    };
    crate::local::reveal(&path)
}

/// Pass a line to a running world's console. Its answer comes back in the world's log.
#[tauri::command]
pub fn host_command(shared: State<'_, Shared>, id: String, line: String) -> Result<(), String> {
    host_runtime::queue_command(&dir_of(&shared, &id)?, &line)
}
#[tauri::command]
pub async fn export_host_logs(
    app: tauri::AppHandle,
    shared: State<'_, Shared>,
    id: String,
) -> Result<(), String> {
    let dir = dir_of(&shared, &id)?;
    let log = std::fs::read_to_string(dir.join(host_runtime::LOG_FILE)).unwrap_or_else(|_| {
        views(&shared)
            .into_iter()
            .find(|h| h.id == id)
            .map(|h| h.logs.join("\n"))
            .unwrap_or_default()
    });
    let Some(file) = app
        .dialog()
        .file()
        .set_file_name("server.log")
        .blocking_save_file()
    else {
        return Ok(());
    };
    std::fs::write(file.into_path().map_err(|e| e.to_string())?, log).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_host_service(
    shared: State<'_, Shared>,
    id: String,
    enabled: bool,
) -> Result<(), String> {
    let shared = shared.inner().clone();
    let dir = begin(
        &shared,
        &id,
        if enabled {
            "Setting the world up to run in the background..."
        } else {
            "Stopping it running in the background..."
        },
    )?;
    let result = tauri::async_runtime::spawn_blocking(move || {
        if enabled {
            if !crate::host_publication::prepared(&dir) {
                return Err("Get the world database first.".into());
            }
            if host_runtime::read_definition(&dir)?.checkout {
                return Err(
                    "A world on this checkout's build runs only while the launcher is open.".into(),
                );
            }
            crate::host_service::install(&dir)
        } else {
            crate::host_service::remove(&dir)
        }
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|result| result);
    finish(
        &shared,
        &id,
        result,
        if enabled {
            "Runs in the background from now on."
        } else {
            "Runs only while the launcher is open from now on."
        },
    );
    Ok(())
}

/// Delete a stopped server, its databases and everything else in its folder. The page has the
/// person confirm it in place first.
#[tauri::command]
pub async fn remove_host(shared: State<'_, Shared>, id: String) -> Result<(), String> {
    let dir = dir_of(&shared, &id)?;
    if host_runtime::active(&dir) {
        return Err("Stop the world before deleting it.".into());
    }
    if crate::host_service::installed(&dir) {
        return Err("Turn off running in the background before deleting the world.".into());
    }
    let dir = begin(&shared, &id, "Deleting...")?;
    // The servers' folder is `hosts` in the launcher's data folder, which keeps which worlds in
    // the list are which servers.
    let data = lock(&shared).root.parent().map(Path::to_path_buf);
    let result = data
        .ok_or_else(|| "Invalid worlds folder".to_owned())
        .and_then(|data| crate::host_publication::forget_host(&data, &id))
        .and_then(|()| {
            std::fs::remove_dir_all(&dir).map_err(|e| format!("Could not delete the world: {e}"))
        });
    let mut store = lock(&shared);
    store.busy.remove(&id);
    if let Err(error) = &result {
        store.messages.insert(id, error.clone());
    }
    result
}

/// Before the window closes: the worlds the launcher runs (its own, and any it started from a
/// folder it knows) are asked to stop, once the person agrees, and the launcher quits when they
/// have. True while the window must stay open.
pub fn confirm_close(
    app: &tauri::AppHandle,
    shared: &Shared,
    local: &crate::local::Shared,
) -> bool {
    let directories = {
        let mut store = lock(shared);
        if store.closing {
            return true;
        }
        if store.creating || !store.busy.is_empty() {
            app.dialog()
                .message("A world is still busy. Wait for it to finish before quitting.")
                .show(|_| {});
            return true;
        }
        store
            .children
            .retain_mut(|(_, child)| !matches!(child.try_wait(), Ok(Some(_))));
        if store.children.is_empty() && !crate::local::any_started(local) {
            return false;
        }
        store.closing = true;
        store
            .children
            .iter()
            .filter_map(|(id, _)| host_dir(&store.root, id).ok())
            .collect::<Vec<_>>()
    };
    let app = app.clone();
    let shared = shared.clone();
    let local = local.clone();
    app.dialog()
        .message("Stop the worlds you are hosting and quit? Saving may take a moment.")
        .title("Your worlds are running")
        .buttons(tauri_plugin_dialog::MessageDialogButtons::OkCancel)
        .show(move |confirmed| {
            if !confirmed {
                lock(&shared).closing = false;
                return;
            }
            for dir in &directories {
                if host_runtime::request_stop(dir).is_err() {
                    lock(&shared).closing = false;
                    return;
                }
            }
            crate::local::stop_all(&local);
            std::thread::spawn(move || loop {
                let done = {
                    let mut store = lock(&shared);
                    store
                        .children
                        .retain_mut(|(_, child)| !matches!(child.try_wait(), Ok(Some(_))));
                    store.children.is_empty() && !crate::local::any_started(&local)
                };
                if done {
                    app.exit(0);
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(200));
            });
        });
    true
}

/// Look for an update for one server. What it found, or why it could not look, is the update's
/// own message; the server's line is left alone.
fn check_update(shared: &Shared, id: &str) -> Result<(), String> {
    let dir = begin(shared, id, "Checking for updates...")?;
    let result = crate::host_updates::check(&dir);
    finish(shared, id, Ok(()), "");
    result
}

/// While the launcher is open, look for server updates once an hour.
pub fn watch(shared: Shared) {
    std::thread::spawn(move || loop {
        for host in views(&shared) {
            let _ = check_update(&shared, &host.id);
        }
        std::thread::sleep(std::time::Duration::from_secs(3600));
    });
}

#[tauri::command]
pub fn check_host_update(shared: State<'_, Shared>, id: String) -> Result<(), String> {
    let shared = shared.inner().clone();
    std::thread::spawn(move || {
        let _ = check_update(&shared, &id);
    });
    Ok(())
}

#[tauri::command]
pub async fn cancel_host_update(shared: State<'_, Shared>, id: String) -> Result<(), String> {
    let shared = shared.inner().clone();
    let dir = begin(&shared, &id, "Canceling the update...")?;
    let result = tauri::async_runtime::spawn_blocking(move || crate::host_updates::cancel(&dir))
        .await
        .map_err(|e| e.to_string())
        .and_then(|r| r);
    finish(&shared, &id, result, "");
    Ok(())
}

#[tauri::command]
pub async fn approve_host_update(
    shared: State<'_, Shared>,
    id: String,
    version: String,
) -> Result<(), String> {
    let shared = shared.inner().clone();
    let dir = begin(&shared, &id, &format!("Downloading Empyrean {version}..."))?;
    let result = tauri::async_runtime::spawn_blocking(move || {
        crate::host_updates::stage(&dir, Some(&version))
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|r| r);
    finish(&shared, &id, result, "");
    Ok(())
}

#[tauri::command]
pub async fn rollback_host_update(
    app: tauri::AppHandle,
    shared: State<'_, Shared>,
    id: String,
) -> Result<(), String> {
    if !app
        .dialog()
        .message("Go back to the previous Empyrean and the databases from before the update? Anything played since the update is lost.")
        .title("Undo update")
        .buttons(tauri_plugin_dialog::MessageDialogButtons::OkCancel)
        .blocking_show()
    {
        return Ok(());
    }
    let shared = shared.inner().clone();
    let dir = begin(&shared, &id, "Undoing the update...")?;
    let result = tauri::async_runtime::spawn_blocking(move || {
        let guard = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(dir.join("run.lock"))
            .map_err(|e| e.to_string())?;
        guard
            .try_lock()
            .map_err(|_| "Stop the world before undoing its update.")?;
        crate::host_updates::rollback(&dir)
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|r| r);
    finish(&shared, &id, result, "");
    Ok(())
}

#[tauri::command]
pub async fn publish_host_world(
    app: tauri::AppHandle,
    shared: State<'_, Shared>,
    components: State<'_, crate::components::Shared>,
    id: String,
    world: Option<PathBuf>,
) -> Result<(), String> {
    let world = match world {
        Some(world) => world,
        None => match app
            .dialog()
            .file()
            .set_title("Choose the Lyceum world to publish")
            .add_filter("Lyceum world", &["toml"])
            .blocking_pick_file()
        {
            Some(file) => file.into_path().map_err(|e| e.to_string())?,
            None => return Ok(()),
        },
    };
    if let Some(why) = crate::host_publication::gone(&dir_of(&shared, &id)?, &world) {
        return Err(why);
    }
    let builder = crate::components::begin_build(&components)?;
    let dir = match begin(&shared, &id, "Publishing the world...") {
        Ok(dir) => dir,
        Err(error) => {
            crate::components::finish_build(&components);
            return Err(error);
        }
    };
    let result = tauri::async_runtime::spawn_blocking(move || {
        crate::host_publication::publish(&dir, &world, &builder).map(|_| ())
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|r| r);
    crate::components::finish_build(&components);
    // What the world plays now is on its page; only a failure is said.
    finish(&shared, &id, result, "");
    Ok(())
}

/// Put the world back on its era's world database, so it no longer plays the Lyceum world
/// published to it.
#[tauri::command]
pub async fn unpublish_host_world(shared: State<'_, Shared>, id: String) -> Result<(), String> {
    let shared = shared.inner().clone();
    let dir = begin(&shared, &id, "Restoring the world base...")?;
    let result =
        tauri::async_runtime::spawn_blocking(move || crate::host_publication::unpublish(&dir))
            .await
            .map_err(|e| e.to_string())
            .and_then(|r| r);
    finish(&shared, &id, result, "");
    Ok(())
}

#[tauri::command]
pub fn open_host_world(
    shared: State<'_, Shared>,
    backend: State<'_, crate::backend::Shared>,
    id: String,
) -> Result<String, String> {
    let dir = dir_of(&shared, &id)?;
    let slug = crate::backend::lock(&backend).open_host_world(&dir)?;
    crate::backend::Backend::probe(&backend, vec![slug.clone()]);
    Ok(slug)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A world is listed on the Dereth Network when the slug its configuration names is on the
    /// hub's list, read here from a stand-in hub's page; a world with no slug, or one the hub does
    /// not list, keeps its List button.
    #[test]
    fn a_world_is_listed_when_its_slug_is_on_the_hubs_list() {
        let page = serde_json::json!({
            "data": [
                { "slug": "coldeve", "name": "Coldeve", "endpoint": { "address": "203.0.113.9", "port": 9000 } },
                { "slug": "frostfell", "name": "Frostfell", "endpoint": { "address": "frost.example", "port": 9050 } },
            ],
            "nextOffset": null,
        })
        .to_string();
        let hub: std::collections::HashSet<String> =
            dereth_launch::serverlist::parse_hub_pages(&[page])
                .unwrap()
                .into_iter()
                .filter_map(|w| w.hub_slug)
                .collect();
        let slug = |text: &str| federation_slug(text);
        let coldeve = slug("[server]\nworld_name = 'Mine'\n[federation]\nslug = 'coldeve'\n");
        assert_eq!(coldeve.as_deref(), Some("coldeve"));
        assert!(listed(coldeve.as_deref(), &hub));
        assert!(
            listed(Some("Coldeve"), &hub),
            "a slug is matched whatever its case"
        );
        assert!(!listed(
            slug("[federation]\nslug = 'elsewhere'\n").as_deref(),
            &hub
        ));
        assert_eq!(slug("[server]\nworld_name = 'Mine'\n"), None);
        assert_eq!(slug("[federation]\nslug = '  '\n"), None);
        assert!(!listed(None, &hub));
        assert!(
            !listed(Some("coldeve"), &std::collections::HashSet::new()),
            "no hub list"
        );
    }
}
