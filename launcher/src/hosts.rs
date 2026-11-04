//! The servers a player runs: each in a folder of its own, `hosts/<id>` in the launcher's data
//! folder, with its configuration, databases, log and its own copy of the server it runs. A world
//! is Empyrean's, of Asheron's Call, or Yalain's, of Asheron's Call 2 ([`crate::host_yalain`]): the
//! two are made, listed, started, stopped and deleted alike, and each realm's page lists its own.
//!
//! Yalain's worlds are in `hosts-auberean/<id>` beside `hosts` ([`AUBEREAN_HOSTS`]), where a
//! launcher from before Auberean's realm (Dereth 0.4.9), which takes every world in `hosts` for
//! Empyrean's, never looks. One made in `hosts` by an earlier build is moved there at start while
//! it is stopped and runs no system service ([`move_yalain_worlds`]).

use crate::host_config::Settings;
use crate::host_runtime::{self, Definition, Status};
use dereth_launch::world::Game;
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
    /// The game the world is of: Empyrean's, or Yalain's.
    pub game: Game,
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
/// The folder of Yalain's worlds in the launcher's data folder, beside `hosts`.
pub const AUBEREAN_HOSTS: &str = "hosts-auberean";

/// The folder of the worlds of `game` whose servers' folder (Empyrean's) is `root`.
fn root_of(root: &Path, game: Game) -> PathBuf {
    match game {
        Game::Dereth => root.to_path_buf(),
        Game::Auberean => root.with_file_name(AUBEREAN_HOSTS),
    }
}

/// The store over the launcher's data folder `root`, Yalain's worlds made by an earlier build in
/// `hosts` moved to their own folder first.
pub fn new(root: PathBuf) -> Shared {
    let hosts = root.join("hosts");
    for said in move_yalain_worlds(&hosts, &root_of(&hosts, Game::Auberean)) {
        eprintln!("dereth-launcher: {said}");
    }
    Arc::new(Mutex::new(Store {
        root: hosts,
        busy: BTreeMap::new(),
        children: Vec::new(),
        messages: BTreeMap::new(),
        closing: false,
        creating: false,
    }))
}
/// Move each Yalain world in `hosts` to `to` (its own folder, [`AUBEREAN_HOSTS`]) while it is
/// stopped and runs no system service, whose registration names its folder: renamed in place,
/// then every path its files name inside its old folder named in the new one ([`relocate`]). What
/// was moved or left, in words for the launcher's log.
pub fn move_yalain_worlds(hosts: &Path, to: &Path) -> Vec<String> {
    let mut said = Vec::new();
    // One moved whose files still name its old folder (a move cut short) has them named again.
    for e in std::fs::read_dir(to).into_iter().flatten().flatten() {
        let Ok(definition) = host_runtime::read_definition(&e.path()) else {
            continue;
        };
        let old = hosts.join(&definition.id);
        if definition.wrapper.starts_with(&old) {
            if let Err(e) = relocate(&e.path(), &old) {
                said.push(format!(
                    "{}'s paths could not be named again: {e}",
                    definition.id
                ));
            }
        }
    }
    let Ok(entries) = std::fs::read_dir(hosts) else {
        return said;
    };
    for e in entries.flatten() {
        let from = e.path();
        let Ok(definition) = host_runtime::read_definition(&from) else {
            continue;
        };
        if definition.game != Game::Auberean {
            continue;
        }
        if host_runtime::active(&from) || crate::host_service::installed(&from) {
            said.push(format!(
                "{} is running or runs as a system service; it stays in {} until it does not",
                definition.id,
                hosts.display()
            ));
            continue;
        }
        let dest = to.join(&definition.id);
        let moved = std::fs::create_dir_all(to)
            .and_then(|()| std::fs::rename(&from, &dest))
            .map_err(|e| e.to_string())
            .and_then(|()| relocate(&dest, &from));
        said.push(match moved {
            Ok(()) => format!("{} moved to {}", definition.id, dest.display()),
            Err(e) => format!(
                "{} could not be moved to {}: {e}",
                definition.id,
                to.display()
            ),
        });
    }
    said
}

/// `value` named in `dir` when it names something in `old`, its old folder: the same place in it.
fn moved_path(value: &str, old: &Path, dir: &Path) -> Option<String> {
    let rest = Path::new(value).strip_prefix(old).ok()?;
    Some(dir.join(rest).to_string_lossy().into_owned())
}

/// Every path the world now in `dir` names inside `old`, its old folder, named in `dir`: in its
/// definition, its settings, its update records and its publications.
fn relocate(dir: &Path, old: &Path) -> Result<(), String> {
    fn json(v: &mut serde_json::Value, old: &Path, dir: &Path) {
        match v {
            serde_json::Value::String(s) => {
                if let Some(p) = moved_path(s, old, dir) {
                    *s = p;
                }
            }
            serde_json::Value::Array(a) => a.iter_mut().for_each(|v| json(v, old, dir)),
            serde_json::Value::Object(o) => o.values_mut().for_each(|v| json(v, old, dir)),
            _ => {}
        }
    }
    fn toml(t: &mut dyn toml_edit::TableLike, old: &Path, dir: &Path) {
        for (_, item) in t.iter_mut() {
            if let Some(t) = item.as_table_like_mut() {
                toml(t, old, dir);
            } else if let Some(v) = item.as_value_mut() {
                if let Some(p) = v.as_str().and_then(|s| moved_path(s, old, dir)) {
                    let decor = v.decor().clone();
                    *v = toml_edit::Value::from(p);
                    *v.decor_mut() = decor;
                }
            }
        }
    }
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .flatten()
        .map(|e| e.path())
        .collect();
    if let Ok(published) = std::fs::read_dir(dir.join("published")) {
        files.extend(
            published
                .flatten()
                .map(|e| e.path().join("publication.json")),
        );
    }
    for file in files.into_iter().filter(|f| f.is_file()) {
        let Some(kind) = file
            .extension()
            .and_then(|e| e.to_str())
            .filter(|k| ["json", "toml"].contains(k))
        else {
            continue;
        };
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        let out = match kind {
            "json" => {
                let Ok(mut v) = serde_json::from_str::<serde_json::Value>(&text) else {
                    continue;
                };
                json(&mut v, old, dir);
                serde_json::to_string(&v).map_err(|e| e.to_string())?
            }
            "toml" => {
                let Ok(mut doc) = text.parse::<toml_edit::DocumentMut>() else {
                    continue;
                };
                toml(doc.as_table_mut(), old, dir);
                doc.to_string()
            }
            _ => continue,
        };
        if out != text {
            std::fs::write(&file, out).map_err(|e| format!("{}: {e}", file.display()))?;
        }
    }
    Ok(())
}

/// The folder of the world `id`: in Empyrean's worlds' folder `root`, or in Yalain's beside it.
fn host_dir(root: &Path, id: &str) -> Result<PathBuf, String> {
    if id.is_empty() || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
        return Err("Invalid world identity.".into());
    }
    let own = root_of(root, Game::Auberean).join(id);
    let dir = if own.join("host.json").is_file() {
        own
    } else {
        root.join(id)
    };
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
    let entries = [Game::Dereth, Game::Auberean]
        .into_iter()
        .filter_map(|game| std::fs::read_dir(root_of(&store.root, game)).ok())
        .flatten();
    for e in entries.flatten() {
        let dir = e.path();
        let Ok(definition) = host_runtime::read_definition(&dir) else {
            continue;
        };
        // Its configuration file: Empyrean's empyrean.toml, or Yalain's yalain.toml in its form.
        let text = std::fs::read_to_string(dir.join(host_runtime::config_file(definition.game)))
            .unwrap_or_default();
        // A configuration that does not read is shown with the defaults, and the page says why.
        let read = host_runtime::read_settings(&dir, definition.game);
        let unread = read.as_ref().err().map(|e| {
            format!(
                "{} could not be read: {e}",
                host_runtime::config_file(definition.game)
            )
        });
        let settings = read.unwrap_or_default();
        let program = definition.program.clone();
        let logs = std::fs::read(dir.join("log.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        let publication = crate::host_publication::selected(&dir)
            .ok()
            .flatten()
            .map(|(_, p)| p);
        let source_gone = publication.as_ref().is_some_and(|p| !p.source.is_file());
        // The world file of a Lyceum world and of an Athenaeum world have the same shape: its
        // sources are it and its packs.
        let unpublished_changes = publication.as_ref().and_then(|p| {
            if p.sources.is_empty() || !p.source.is_file() {
                return None;
            }
            crate::lyceum_world::current_digest(&p.source).map(|now| now != p.sources)
        });
        views.push(View {
            busy: store.busy.get(&definition.id).cloned(),
            message: store.messages.get(&definition.id).cloned().or(unread),
            id: definition.id,
            game: definition.game,
            settings,
            version: definition.version,
            checkout: definition.checkout,
            status: host_runtime::status(&dir),
            logs,
            prepared: prepared(&dir, definition.game),
            service: crate::host_service::installed(&dir),
            update: crate::host_updates::view(&dir),
            publication,
            unpublished_changes,
            source_gone,
            federation_slug: federation_slug(&text),
            listed: false,
            plays: match definition.game {
                Game::Dereth => crate::host_config::plays(&text, &dir),
                Game::Auberean => crate::host_yalain::plays(&text, &dir, Some(&program)),
            },
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
/// Whether the world in `dir` has the world database it needs before its first start.
fn prepared(dir: &Path, game: Game) -> bool {
    match game {
        Game::Dereth => crate::host_publication::prepared(dir),
        Game::Auberean => crate::host_yalain::prepared(dir),
    }
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
    settings.status_port = free_status_port(&shared)?;
    let server = crate::components::server(&components)?;
    let write = move |program: &Path, dir: &Path| {
        validate_config(program, dir, &settings.update_config("")?)
    };
    create(shared.inner(), server, Game::Dereth, write).await
}

/// The status endpoint the launcher counts a new world's players at, on this computer: a port no
/// other world of either game names and nothing holds now, kept from here on.
fn free_status_port(shared: &Shared) -> Result<u16, String> {
    let taken: Vec<u16> = views(shared)
        .iter()
        .map(|h| h.settings.status_port)
        .collect();
    crate::host_config::free_status_port(&taken, |port| {
        std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).is_ok()
    })
    .ok_or_else(|| "No port is free on this computer for the world's status.".into())
}

/// Make a Yalain world with `settings`, as an Empyrean world is made: a folder of its own, its own
/// copy of Yalain, its settings.
#[tauri::command]
pub async fn create_yalain_host(
    shared: State<'_, Shared>,
    components: State<'_, crate::components::Shared>,
    mut settings: Settings,
) -> Result<String, String> {
    crate::host_yalain::validate(&settings)?;
    settings.status_port = free_status_port(&shared)?;
    let server =
        crate::components::server_of(&components, dereth_launch::components::Product::Yalain)?;
    let write = move |_: &Path, dir: &Path| crate::host_yalain::write(dir, &settings);
    create(shared.inner(), server, Game::Auberean, write).await
}

/// Make a world of `game` run by `server`: its folder, its copy of the server, the supervisor
/// that runs it, its settings (`write`, given the server's program and the folder), and its
/// definition. The world's identity.
async fn create(
    shared: &Shared,
    server: crate::components::Server,
    game: Game,
    write: impl FnOnce(&Path, &Path) -> Result<(), String> + Send + 'static,
) -> Result<String, String> {
    let root = {
        let mut store = lock(shared);
        if store.closing || store.creating {
            return Err("The launcher is busy. Try again in a moment.".into());
        }
        let root = std::path::absolute(root_of(&store.root, game)).map_err(|e| e.to_string())?;
        store.creating = true;
        root
    };
    let _creating = CreationGuard(shared.clone());
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
        write(&program, &dir)?;
        host_runtime::write_json(
            &dir.join("host.json"),
            &Definition {
                id: create_id,
                version: server.version,
                program,
                wrapper,
                checkout: server.checkout,
                game,
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
    let game = host_runtime::read_definition(&dir).map_or(Game::Dereth, |d| d.game);
    if game == Game::Auberean {
        if crate::host_yalain::prepared(&dir) {
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
            let result = host_runtime::read_definition(&dir)
                .and_then(|d| crate::host_yalain::fetch(&d.program, &dir));
            finish(&shared, &id, result, "The world database is ready.");
        });
        return Ok(());
    }
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
            // The era's own world database; the end of retail's is the default.
            let text =
                std::fs::read_to_string(dir.join("empyrean.toml")).map_err(|e| e.to_string())?;
            let era = Settings::from_config(&text)?.era;
            let mut fetch = crate::components::command(&import);
            fetch.arg("fetch");
            if era != "eor" {
                fetch.args(["--era", era.as_str()]);
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

/// Bring the supervisor of the stopped world in `dir` (`wrapper`, the launcher copied into the
/// world's `manager` folder when it was made) up to the launcher `launcher` when they differ, so a
/// world made by an older launcher runs this one's supervisor. A running world's (its run lock
/// held) is never touched. The launcher is written into the file that is there, which keeps what a
/// system service was granted on it. Whether it was brought up.
///
/// # Errors
/// A file that will not read or write.
fn refresh_supervisor(dir: &Path, wrapper: &Path, launcher: &Path) -> Result<bool, String> {
    if host_runtime::active(dir) {
        return Ok(false);
    }
    let new = std::fs::read(launcher).map_err(|e| format!("{}: {e}", launcher.display()))?;
    if std::fs::read(wrapper).is_ok_and(|old| old == new) {
        return Ok(false);
    }
    std::fs::write(wrapper, new)
        .map_err(|e| format!("Could not bring the world's supervisor up to date: {e}"))?;
    Ok(true)
}

#[tauri::command]
pub fn start_host(
    shared: State<'_, Shared>,
    sharing: State<'_, crate::share::Shared>,
    id: String,
) -> Result<(), String> {
    let mut store = lock(&shared);
    let dir = host_dir(&store.root, &id)?;
    if store.closing || store.busy.contains_key(&id) || host_runtime::active(&dir) {
        return Err("This world is busy or already running.".into());
    }
    let definition = host_runtime::read_definition(&dir)?;
    if !prepared(&dir, definition.game) {
        return Err("Get the world database first.".into());
    }
    // A Yalain world others may join is told the address they reach it at.
    if definition.game == Game::Auberean {
        crate::host_yalain::check_updates_off(
            &std::fs::read_to_string(dir.join(crate::host_yalain::CONFIG)).unwrap_or_default(),
        )?;
        let settings = crate::host_yalain::read(&dir)?;
        let here = crate::share::view(&sharing);
        let address = crate::host_yalain::advertised(
            &settings.bind,
            here.lan,
            here.public,
            here.shared_address,
        )?;
        crate::host_yalain::keep_advertised(&dir, address)?;
    }
    // A world made by an older launcher runs this one's supervisor.
    if let Err(e) = std::env::current_exe()
        .map_err(|e| e.to_string())
        .and_then(|me| refresh_supervisor(&dir, &definition.wrapper, &me))
    {
        eprintln!("dereth-launcher: {e}");
    }
    if crate::host_service::installed(&dir) {
        return crate::host_service::start(&dir);
    }
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
/// Save a Yalain world's settings. A running world takes them at its next start.
#[tauri::command]
pub async fn save_yalain_host(
    shared: State<'_, Shared>,
    id: String,
    settings: Settings,
) -> Result<(), String> {
    crate::host_yalain::validate(&settings)?;
    let shared = shared.inner().clone();
    let dir = begin(&shared, &id, "Saving the settings...")?;
    let text = std::fs::read_to_string(dir.join(crate::host_yalain::CONFIG)).unwrap_or_default();
    let result =
        if settings.federation.enabled && !crate::host_config::plays(&text, &dir).network_key {
            Err("Save the world's key before linking it to the Dereth Network.".to_owned())
        } else {
            crate::host_yalain::write(&dir, &settings)
        };
    let running = host_runtime::active(&dir);
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
        let config = host_runtime::config_file(definition.game);
        let text = std::fs::read_to_string(dir.join(config)).map_err(|e| e.to_string())?;
        let next = crate::host_config::save_network_key(&dir, &text, &key)?;
        if next == text {
            Ok(())
        } else if definition.game == Game::Auberean {
            crate::host_yalain::keep_config(&dir, &next)
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
    let config = host_runtime::config_file(host_runtime::read_definition(&dir)?.game);
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
        .arg(dir.join(config))
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}
/// Show the server's folder: its configuration, databases, log and backups.
#[tauri::command]
pub fn open_host_folder(shared: State<'_, Shared>, id: String) -> Result<(), String> {
    crate::components::open_folder(&dir_of(&shared, &id)?)
}

/// Show one of a world's files in the system's file browser: its `config` (`empyrean.toml` or
/// `yalain.toml`), its `log`, or its `folder`.
#[tauri::command]
pub fn reveal_host(shared: State<'_, Shared>, id: String, place: String) -> Result<(), String> {
    let dir = dir_of(&shared, &id)?;
    let path = match place.as_str() {
        "config" => dir.join(host_runtime::config_file(
            host_runtime::read_definition(&dir)?.game,
        )),
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
            let definition = host_runtime::read_definition(&dir)?;
            let game = definition.game;
            if !prepared(&dir, game) {
                return Err("Get the world database first.".into());
            }
            if definition.checkout {
                return Err(
                    "A world on this checkout's build runs only while the launcher is open.".into(),
                );
            }
            // A Windows service runs as an account of its own, which cannot read the world
            // database Yalain keeps in the player's own cache: the world keeps it in its folder.
            if cfg!(windows) && game == Game::Auberean {
                crate::host_yalain::keep_own_world_database(&definition.program, &dir)?;
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
/// Whether the world `id` is an Empyrean world: the only kind the launcher updates from a release
/// (a Yalain world runs the Yalain it was made with).
/// The server a world of `game` runs, by name.
fn server_name(game: Game) -> &'static str {
    match game {
        Game::Dereth => "Empyrean",
        Game::Auberean => "Yalain",
    }
}

/// Whether the world `id` is one whose server takes updates: every world the launcher keeps,
/// Empyrean's and Yalain's (a local build's says so when asked).
fn updatable(shared: &Shared, id: &str) -> bool {
    dir_of(shared, id)
        .and_then(|dir| host_runtime::read_definition(&dir))
        .is_ok()
}

fn check_update(shared: &Shared, id: &str) -> Result<(), String> {
    if !updatable(shared, id) {
        return Ok(());
    }
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
    if !updatable(&shared, &id) {
        return Err("No such world.".into());
    }
    let server = server_name(host_runtime::read_definition(&dir_of(&shared, &id)?)?.game);
    let dir = begin(&shared, &id, &format!("Downloading {server} {version}..."))?;
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
    let server = server_name(host_runtime::read_definition(&dir_of(&shared, &id)?)?.game);
    if !app
        .dialog()
        .message(format!("Go back to the previous {server} and the databases from before the update? Anything played since the update is lost."))
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
    let game = host_runtime::read_definition(&dir_of(&shared, &id)?)?.game;
    if game == Game::Auberean {
        return publish_athenaeum_world(app, shared, id, world).await;
    }
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

/// Publish the Athenaeum world `world` (its folder; asked for when `None`) to the Yalain world
/// `id`, with the world's own Yalain.
async fn publish_athenaeum_world(
    app: tauri::AppHandle,
    shared: State<'_, Shared>,
    id: String,
    world: Option<PathBuf>,
) -> Result<(), String> {
    let world = match world {
        Some(world) => world,
        None => match app
            .dialog()
            .file()
            .set_title("Choose the Athenaeum world to publish (its folder)")
            .blocking_pick_folder()
        {
            Some(folder) => folder.into_path().map_err(|e| e.to_string())?,
            None => return Ok(()),
        },
    };
    let dir = dir_of(&shared, &id)?;
    if let Some(why) = crate::host_publication::gone(&dir, &world) {
        return Err(why);
    }
    let yalain = host_runtime::read_definition(&dir)?.program;
    let shared = shared.inner().clone();
    let dir = begin(&shared, &id, "Publishing the world...")?;
    let result = tauri::async_runtime::spawn_blocking(move || {
        crate::host_publication::publish_athenaeum(&dir, &world, &yalain).map(|_| ())
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|r| r);
    finish(&shared, &id, result, "");
    Ok(())
}

/// Put the world back on its era's world database (a Yalain world, on the one its Yalain pins), so
/// it no longer plays the Lyceum or Athenaeum world published to it.
#[tauri::command]
pub async fn unpublish_host_world(shared: State<'_, Shared>, id: String) -> Result<(), String> {
    let game = host_runtime::read_definition(&dir_of(&shared, &id)?)?.game;
    let shared = shared.inner().clone();
    let dir = begin(&shared, &id, "Restoring the world base...")?;
    let result = tauri::async_runtime::spawn_blocking(move || match game {
        Game::Dereth => crate::host_publication::unpublish(&dir),
        Game::Auberean => crate::host_publication::unpublish_athenaeum(&dir),
    })
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
    // Each answers the status ping on its login port, Yalain as Empyrean.
    crate::backend::Backend::probe(&backend, vec![slug.clone()]);
    Ok(slug)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A world of `game` in `dir`, made as the launcher makes one: its definition naming its own
    /// copy of the server and its supervisor inside it.
    fn world_in(dir: &Path, game: Game) {
        std::fs::create_dir_all(dir.join("runtime/0.1.0")).unwrap();
        std::fs::create_dir_all(dir.join("manager")).unwrap();
        let id = dir.file_name().unwrap().to_string_lossy().into_owned();
        host_runtime::write_json(
            &dir.join("host.json"),
            &Definition {
                id,
                version: "0.1.0".into(),
                program: dir.join("runtime/0.1.0/server.exe"),
                wrapper: dir.join("manager/dereth-host.exe"),
                checkout: false,
                game,
            },
        )
        .unwrap();
    }

    /// A Yalain world an earlier build made in `hosts` is moved at start to Yalain's own folder,
    /// where a launcher from before Auberean's realm never looks, every path its files name inside
    /// its old folder following it; a running one, one run as a system service, and Empyrean's
    /// stay where they are. Both folders are listed, and each world is found by its identity.
    #[test]
    fn yalain_worlds_move_to_a_folder_of_their_own_while_stopped() {
        let root = tempfile::tempdir().unwrap();
        let hosts = root.path().join("hosts");
        let (yalain, empyrean) = (hosts.join("host-a"), hosts.join("host-b"));
        world_in(&yalain, Game::Auberean);
        world_in(&empyrean, Game::Dereth);
        // Its store, which is no text.
        std::fs::write(yalain.join("yalain.sqlite"), [0xFF_u8, 0xFE, 0x00, 0x9F]).unwrap();
        let served = yalain.join("published/snapshot-0/world");
        std::fs::create_dir_all(&served).unwrap();
        let elsewhere = root.path().join("ac2");
        let mut doc = toml_edit::DocumentMut::new();
        doc["server"] = toml_edit::Item::Table(toml_edit::Table::new());
        doc["server"]["world_database_path"] = toml_edit::value(served.to_string_lossy().as_ref());
        doc["server"]["world_base_path"] = toml_edit::value("world-database/v0.23.0");
        doc["server"]["dat_files_directory"] =
            toml_edit::value(elsewhere.to_string_lossy().as_ref());
        std::fs::write(yalain.join("yalain.toml"), doc.to_string()).unwrap();
        std::fs::write(
            yalain.join("update-rollback.json"),
            serde_json::json!({
                "program": yalain.join("runtime/0.0.9/yalain.exe"),
                "version": "0.0.9",
                "backup": yalain.join("yalain.sqlite.backup"),
                "fetched": "v0.23.0",
            })
            .to_string(),
        )
        .unwrap();
        // One running, and one run as a system service.
        let (running, service) = (hosts.join("host-c"), hosts.join("host-d"));
        world_in(&running, Game::Auberean);
        world_in(&service, Game::Auberean);
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(running.join("run.lock"))
            .unwrap();
        lock.try_lock().unwrap();
        std::fs::write(service.join("service.json"), "\"unit\"").unwrap();

        let own = root.path().join(AUBEREAN_HOSTS);
        // One whose move was cut short: in its new folder, its files naming the old.
        world_in(&hosts.join("host-e"), Game::Auberean);
        std::fs::create_dir_all(&own).unwrap();
        std::fs::rename(hosts.join("host-e"), own.join("host-e")).unwrap();
        let said = move_yalain_worlds(&hosts, &own);
        assert_eq!(said.len(), 3, "{said:?}");
        assert!(said.iter().all(|s| !s.contains("could not")), "{said:?}");
        assert_eq!(
            host_runtime::read_definition(&own.join("host-e"))
                .unwrap()
                .wrapper,
            own.join("host-e/manager/dereth-host.exe")
        );
        assert_eq!(
            std::fs::read(own.join("host-a/yalain.sqlite")).unwrap(),
            [0xFF, 0xFE, 0x00, 0x9F]
        );
        let moved = own.join("host-a");
        assert!(!yalain.exists() && moved.join("host.json").is_file());
        assert!(empyrean.join("host.json").is_file() && !own.join("host-b").exists());
        assert!(running.is_dir() && service.is_dir(), "left where they are");
        let definition = host_runtime::read_definition(&moved).unwrap();
        assert_eq!(definition.program, moved.join("runtime/0.1.0/server.exe"));
        assert_eq!(definition.wrapper, moved.join("manager/dereth-host.exe"));
        let doc: toml_edit::DocumentMut = std::fs::read_to_string(moved.join("yalain.toml"))
            .unwrap()
            .parse()
            .unwrap();
        let path = |k: &str| PathBuf::from(doc["server"][k].as_str().unwrap());
        assert_eq!(
            path("world_database_path"),
            moved.join("published/snapshot-0/world")
        );
        assert_eq!(
            path("world_base_path"),
            PathBuf::from("world-database/v0.23.0")
        );
        assert_eq!(path("dat_files_directory"), elsewhere);
        let undo: serde_json::Value =
            serde_json::from_slice(&std::fs::read(moved.join("update-rollback.json")).unwrap())
                .unwrap();
        assert_eq!(
            PathBuf::from(undo["program"].as_str().unwrap()),
            moved.join("runtime/0.0.9/yalain.exe")
        );
        assert_eq!(undo["version"], "0.0.9");

        // Both folders listed; each world found by its identity.
        drop(lock);
        let store = new(root.path().to_path_buf());
        let ids: Vec<String> = views(&store).into_iter().map(|v| v.id).collect();
        assert_eq!(ids, ["host-a", "host-b", "host-c", "host-d", "host-e"]);
        assert_eq!(dir_of(&store, "host-a").unwrap(), moved);
        assert_eq!(dir_of(&store, "host-b").unwrap(), empyrean);
        assert_eq!(
            dir_of(&store, "host-c").unwrap(),
            own.join("host-c"),
            "moved once stopped"
        );
        assert_eq!(
            dir_of(&store, "host-d").unwrap(),
            service,
            "still a service"
        );
    }

    /// A world made by an older launcher has its supervisor brought up to this launcher when it
    /// starts, once; a running world's supervisor is left as it is.
    #[test]
    fn a_stopped_worlds_old_supervisor_is_replaced_at_start_and_a_running_ones_is_not() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path();
        let wrapper = dir.join("manager").join("dereth-host.exe");
        std::fs::create_dir_all(wrapper.parent().unwrap()).unwrap();
        std::fs::write(&wrapper, b"an older launcher").unwrap();
        let launcher = dir.join("dereth.exe");
        std::fs::write(&launcher, b"this launcher").unwrap();

        // Running: its supervisor holds the run lock.
        let held = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(dir.join("run.lock"))
            .unwrap();
        held.try_lock().unwrap();
        assert!(!refresh_supervisor(dir, &wrapper, &launcher).unwrap());
        assert_eq!(std::fs::read(&wrapper).unwrap(), b"an older launcher");
        drop(held);

        // Stopped: brought up, then left alone while it is the same.
        assert!(refresh_supervisor(dir, &wrapper, &launcher).unwrap());
        assert_eq!(std::fs::read(&wrapper).unwrap(), b"this launcher");
        assert!(!refresh_supervisor(dir, &wrapper, &launcher).unwrap());
    }

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
