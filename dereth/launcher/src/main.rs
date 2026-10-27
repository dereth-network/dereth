//! Dereth, the launcher, as a Tauri web app.
//!
//! The page (`ui/`) is the interface; [`backend`] is everything behind it, and `dereth_launch` is
//! what the launcher knows. Each command below is one thing the page can ask.
//!
//! Before the window opens, the launcher finds its folders ([`dereth_launch::folders`]) and removes
//! what its last update set aside. Started with a join link while another launcher is open, it
//! hands the link to that one and ends ([`join`]). On Windows it first makes sure the WebView2
//! runtime is there to draw the page with, and says where to get it when it is not; on Linux it
//! registers its desktop entry and icon ([`registration`]); a release registers the `dereth://`
//! scheme. Once it runs, [`discord`] keeps the Discord app told of the game it started.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod backend;
mod component_archive;
mod components;
mod discord;
mod host_config;
mod host_publication;
mod host_runtime;
mod host_service;
mod host_updates;
mod hosts;
mod join;
mod local;
mod lyceum_world;
mod pad;
// Only Linux registers a desktop entry; elsewhere the module is its tests and a no-op.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
mod registration;
mod services;
mod share;
mod shortcut;
mod update;
#[cfg(any(windows, target_os = "macos", target_os = "linux"))]
mod vault_keyring;

use std::path::{Path, PathBuf};
use std::time::Duration;

use backend::{lock, Backend, Choice, LaunchOutcome, Shared, Snapshot, WorldView};
use dereth_launch::folders::{self, Folders, System};
use dereth_launch::library::FolderFind;
use dereth_launch::serverlist;
use dereth_launch::state::Favourite;
use tauri::{Manager, State};
use tauri_plugin_dialog::DialogExt;

type Cmd<T> = Result<T, String>;

/// What the page draws: the backend's snapshot, where a launcher update stands, and what the
/// footnote under the rail says about the launcher itself.
#[derive(serde::Serialize)]
struct PageSnapshot {
    #[serde(flatten)]
    snapshot: Snapshot,
    update: update::UpdateView,
    components: Vec<components::View>,
    hosts: Vec<hosts::View>,
    /// The worlds already running on this computer that the launcher did not make, and those it
    /// remembers.
    local: Vec<local::View>,
    /// This computer's address and what its router says, for sharing a world.
    sharing: share::View,
    /// A join link came in, for the page to take ([`join::take_join`]).
    join_waiting: bool,
    version: &'static str,
    /// Whether "Create desktop shortcut" is offered on this system.
    shortcut: bool,
    /// Whether this launcher uses its checkout's own server and editor: a development build not
    /// started with `--downloads`.
    development: bool,
    /// Whether the web view's developer tools are on, which is [`DEVTOOLS`]. The page leaves the
    /// web view's own right-click menu (Reload, Inspect) alone only when they are; a text box keeps its
    /// own either way.
    devtools: bool,
}

/// The web view's developer tools, and with them its right-click menu, exist only in a debug
/// build. A release build turns them off at the web view as well: Tauri's `devtools` feature is not
/// enabled, and the window is built with them off.
const DEVTOOLS: bool = cfg!(debug_assertions);

#[tauri::command]
fn snapshot(
    b: State<'_, Shared>,
    u: State<'_, update::Updates>,
    c: State<'_, components::Shared>,
    h: State<'_, hosts::Shared>,
    l: State<'_, local::Shared>,
    s: State<'_, share::Shared>,
    j: State<'_, join::Pending>,
) -> PageSnapshot {
    let update = u
        .view
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    let (snapshot, hub) = {
        let backend = lock(&b);
        (backend.snapshot(), backend.hub_slugs())
    };
    let mut hosts = hosts::views(&h);
    for v in &mut hosts {
        v.listed = hosts::listed(v.federation_slug.as_deref(), &hub);
    }
    let mut local = local::views(&l);
    for v in &mut local {
        v.listed = hosts::listed(v.federation_slug.as_deref(), &hub);
    }
    PageSnapshot {
        snapshot,
        components: components::views(&c),
        hosts,
        local,
        sharing: share::view(&s),
        join_waiting: j
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_some(),
        update,
        version: env!("CARGO_PKG_VERSION"),
        shortcut: shortcut::OFFERED,
        development: components::checkout_builds(&c),
        devtools: DEVTOOLS,
    }
}

/// Make a desktop shortcut to the launcher. Returns where it was made.
#[tauri::command]
fn create_desktop_shortcut(app: tauri::AppHandle) -> Cmd<String> {
    let desktop = app
        .path()
        .desktop_dir()
        .map_err(|e| format!("There is no desktop folder: {e}"))?;
    shortcut::create(&desktop).map(|p| p.display().to_string())
}

/// Install the downloaded launcher update and start again.
#[tauri::command]
fn restart_to_update(app: tauri::AppHandle) -> Cmd<()> {
    update::install_and_restart(&app)
}

/// Refresh: read every plugin source's list again and apply the updates the setting allows.
#[tauri::command]
async fn refresh_plugins(b: State<'_, Shared>) -> Cmd<String> {
    let shared = b.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        Backend::refresh_plugins(&shared, &backend::fetch_plugin_file)
    })
    .await
    .map_err(|e| e.to_string())
}

/// Add a plugin source by its list's address, and read it.
#[tauri::command]
async fn add_plugin_source(b: State<'_, Shared>, url: String) -> Cmd<String> {
    let shared = b.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        Backend::add_plugin_source(&shared, &backend::fetch_plugin_file, &url)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn remove_plugin_source(b: State<'_, Shared>, url: String) -> Cmd<()> {
    lock(&b).remove_plugin_source(&url)
}

/// Install a plugin from the source that lists it.
#[tauri::command]
async fn install_plugin(b: State<'_, Shared>, source: String, id: String) -> Cmd<String> {
    let shared = b.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        Backend::install_plugin(&shared, &backend::fetch_plugin_file, &source, &id)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Update an installed plugin from its own source.
#[tauri::command]
async fn update_plugin(b: State<'_, Shared>, id: String) -> Cmd<String> {
    let shared = b.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        Backend::update_plugin(&shared, &backend::fetch_plugin_file, &id)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn remove_plugin(b: State<'_, Shared>, id: String) -> Cmd<()> {
    lock(&b).remove_plugin(&id)
}

#[tauri::command]
fn set_plugin_updates(b: State<'_, Shared>, setting: String) -> Cmd<()> {
    lock(&b).set_plugin_updates(&setting)
}

/// Build a new plugin: the name and the repository are checked, then the player chooses an empty
/// folder, where the template is written, or a plugin repository, which gets the new plugin's
/// folder. `None` when the player cancels.
#[tauri::command]
async fn create_plugin(
    app: tauri::AppHandle,
    b: State<'_, Shared>,
    name: String,
    github: Option<String>,
) -> Cmd<Option<backend::MadePlugin>> {
    backend::check_new_plugin(&name, github.as_deref())?;
    let picked = app
        .dialog()
        .file()
        .set_title("Choose an empty folder, or a plugin repository to add the plugin to")
        .blocking_pick_folder();
    let Some(dir) = picked.and_then(|p| p.into_path().ok()) else {
        return Ok(None);
    };
    lock(&b)
        .make_plugin(&dir, &name, github.as_deref())
        .map(Some)
}

/// Choose a folder on this computer for the Plugins page's Add Plugin Folder, which adds it at
/// once: one plugin's folder or a plugin repository. `None` when the player cancels.
#[tauri::command]
async fn pick_plugin_folder(app: tauri::AppHandle) -> Cmd<Option<PathBuf>> {
    let picked = app
        .dialog()
        .file()
        .set_title("Choose a plugin's folder, or a repository of plugins")
        .blocking_pick_folder();
    Ok(picked.and_then(|p| p.into_path().ok()))
}

/// Add a folder on this computer, whose plugins the client is offered as they are on disk.
#[tauri::command]
fn add_plugin_folder(b: State<'_, Shared>, folder: PathBuf) -> Cmd<String> {
    lock(&b).add_plugin_folder(&folder)
}

/// Take a folder off the folders on this computer; nothing in it is deleted.
#[tauri::command]
fn remove_plugin_folder(b: State<'_, Shared>, folder: PathBuf) -> Cmd<()> {
    lock(&b).remove_plugin_folder(&folder)
}

/// Show a plugin's folder, or a plugin repository the Plugins page made, in the system's file
/// manager.
#[tauri::command]
fn open_plugin_folder(folder: PathBuf) -> Cmd<()> {
    if !folder.is_dir() {
        return Err(format!("{} is not there any more.", folder.display()));
    }
    components::open_folder(&folder)
}

/// Fetch the world list again, whatever its copy's age.
#[tauri::command]
fn refresh(b: State<'_, Shared>) {
    Backend::refresh(&b);
}

#[tauri::command]
fn world_view(b: State<'_, Shared>, slug: String) -> Cmd<WorldView> {
    lock(&b).world_view(&slug)
}

#[tauri::command]
fn has_password(b: State<'_, Shared>, slug: String, username: String) -> bool {
    lock(&b).has_password(&slug, &username)
}

#[tauri::command]
fn launch(b: State<'_, Shared>, choice: Choice) -> LaunchOutcome {
    lock(&b).launch(choice)
}

/// Ask for a folder and read it. `None` when the player cancels.
#[tauri::command]
async fn pick_folder(
    app: tauri::AppHandle,
    b: State<'_, Shared>,
    title: String,
) -> Cmd<Option<FolderFind>> {
    let picked = app.dialog().file().set_title(title).blocking_pick_folder();
    let Some(path) = picked.and_then(|p| p.into_path().ok()) else {
        return Ok(None);
    };
    Ok(Some(lock(&b).read_folder(&path)))
}

#[tauri::command]
fn add_folder(b: State<'_, Shared>, find: FolderFind) {
    lock(&b).add_folder(find);
}

#[tauri::command]
fn set_default_set(b: State<'_, Shared>, id: String) -> Cmd<()> {
    lock(&b).set_default_set(&id)
}

#[tauri::command]
async fn add_custom_dats(app: tauri::AppHandle, b: State<'_, Shared>, slug: String) -> Cmd<bool> {
    let picked = app
        .dialog()
        .file()
        .set_title("Choose the folder holding this world's data files")
        .blocking_pick_folder();
    let Some(path) = picked.and_then(|p| p.into_path().ok()) else {
        return Ok(false);
    };
    lock(&b).add_custom_dats(&slug, &path).map(|()| true)
}

#[tauri::command]
fn create_private_copy(b: State<'_, Shared>, slug: String) {
    Backend::create_private_copy(&b, &slug);
}

#[tauri::command]
fn reset_set(b: State<'_, Shared>, id: String) {
    Backend::reset_set(&b, &id);
}

#[tauri::command]
fn delete_set(b: State<'_, Shared>, id: String) -> Cmd<()> {
    lock(&b).delete_set(&id)
}

#[tauri::command]
fn forget_retail(b: State<'_, Shared>) {
    lock(&b).forget_retail();
}

#[tauri::command]
fn verify_retail(b: State<'_, Shared>) {
    Backend::verify_retail(&b);
}

#[tauri::command]
fn save_favourite(b: State<'_, Shared>, favourite: Favourite) {
    lock(&b).save_favourite(favourite);
}

#[tauri::command]
fn remove_favourite(b: State<'_, Shared>, id: String) {
    lock(&b).remove_favourite(&id);
}

#[tauri::command]
fn forget_account(b: State<'_, Shared>, slug: String, username: String) {
    lock(&b).forget_account(&slug, &username);
}

#[tauri::command]
fn set_remember(b: State<'_, Shared>, slug: String, username: String, remember: bool) {
    lock(&b).set_remember(&slug, &username, remember);
}

#[tauri::command]
fn add_custom_world(b: State<'_, Shared>, server: backend::NewServer) -> Cmd<String> {
    let slug = lock(&b).add_custom_world(&server)?;
    Backend::probe(&b, vec![slug.clone()]);
    Ok(slug)
}

/// Play on a world already running on this computer: it joins the player's own worlds, at this
/// computer's address.
#[tauri::command]
fn open_local_world(b: State<'_, Shared>, l: State<'_, local::Shared>, key: String) -> Cmd<String> {
    let view = local::views(&l)
        .into_iter()
        .find(|v| v.key == key)
        .ok_or("That world is no longer here.")?;
    let port = view
        .port
        .ok_or("The launcher does not know this world's port.")?;
    let slug = lock(&b).add_custom_world(&backend::NewServer {
        name: view.name,
        host: dereth_launch::local::ask_at(view.host.as_deref()).to_string(),
        port: port.to_string(),
        ruleset: None,
        emulator: match view.software {
            Some(dereth_launch::local::Software::Empyrean) => {
                dereth_launch::world::Emulator::Empyrean
            }
            Some(dereth_launch::local::Software::Ace) => dereth_launch::world::Emulator::Ace,
            Some(dereth_launch::local::Software::Gdle) => dereth_launch::world::Emulator::Gdle,
            None => dereth_launch::world::Emulator::Unknown,
        },
    })?;
    Backend::probe(&b, vec![slug.clone()]);
    Ok(slug)
}

/// Ask one world whether it is up, as its page opens.
#[tauri::command]
fn probe_world(b: State<'_, Shared>, slug: String) {
    Backend::probe(&b, vec![slug]);
}

#[tauri::command]
fn remove_custom_world(b: State<'_, Shared>, slug: String) {
    lock(&b).remove_custom_world(&slug);
}

#[tauri::command]
fn forget_all_accounts(b: State<'_, Shared>) {
    lock(&b).forget_all_accounts();
}

#[tauri::command]
fn forget_all_passwords(b: State<'_, Shared>) {
    lock(&b).forget_all_passwords();
}

/// Start the Dereth client in this interface from now on: written into the client's preferences.
#[tauri::command]
fn set_interface(b: State<'_, Shared>, name: String) -> Cmd<()> {
    lock(&b).set_interface(&name)
}

/// "Show what I'm playing on Discord", on or off.
#[tauri::command]
fn set_discord_presence(b: State<'_, Shared>, on: bool) {
    lock(&b).set_discord_presence(on);
}

#[tauri::command]
fn dismiss_message(b: State<'_, Shared>) {
    lock(&b).dismiss_message();
}

#[tauri::command]
fn open_url(url: String) -> Cmd<()> {
    backend::open_url(&url)
}

/// The Dereth client's file name, as a release ships it and as cargo builds it.
const CLIENT_NAMES: [&str; 2] = ["dereth-client.exe", "dereth-client"];

/// The Dereth client beside the launcher in `here`, which is where a release puts it (in
/// `Dereth.app`, both are in `Contents/MacOS`).
fn client_beside(here: &Path) -> Option<PathBuf> {
    CLIENT_NAMES
        .iter()
        .map(|n| here.join(n))
        .find(|p| p.is_file())
}

/// The Dereth client: `DERETH_CLIENT` if set, else the one beside the launcher. A launcher built
/// from the repository has none beside it, so it falls back to the workspace's own build,
/// `target/release/dereth-client` (then the debug one), in the workspace that holds it.
fn find_client(here: &Path) -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("DERETH_CLIENT").map(PathBuf::from) {
        return p.is_file().then_some(p);
    }
    if let Some(p) = client_beside(here) {
        return Some(p);
    }
    let workspace = here
        .ancestors()
        .find(|a| a.join("Cargo.toml").is_file() && a.join("dereth/client/Cargo.toml").is_file())?;
    ["release", "debug"]
        .iter()
        .flat_map(|profile| {
            CLIENT_NAMES
                .iter()
                .map(move |n| workspace.join("target").join(profile).join(n))
        })
        .find(|p| p.is_file())
}

/// The plugin author's tool this launcher ships with: the one beside it, where a release puts it
/// (`dereth-plugin.exe` beside `dereth.exe`; in `Dereth.app`, in `Contents/MacOS`; in the AppImage,
/// in its `usr/bin`, read from the running image). A launcher built from the repository has none
/// beside it, and takes the one beside the Dereth client it found, when that build made one.
fn find_tool(here: &Path, client: Option<&Path>) -> Option<PathBuf> {
    [Some(here), client.and_then(Path::parent)]
        .into_iter()
        .flatten()
        .map(|dir| dir.join(dereth_launch::plugins::TOOL_FILE))
        .find(|p| p.is_file())
}

/// The launcher's folders. Falls back to a folder beside the launcher only when the environment
/// names no home folder at all.
fn find_folders(here: &Path) -> Folders {
    folders::resolve(System::HOST, &|name| std::env::var_os(name))
        .unwrap_or_else(|| Folders::single(here.join("launcher-data")))
}

/// Where the WebView2 runtime is downloaded from.
#[cfg(windows)]
const WEBVIEW2_URL: &str = "https://developer.microsoft.com/microsoft-edge/webview2/";

/// On Windows the page is drawn by the WebView2 runtime, which Windows 11 has and Windows 10
/// almost always has. Without it the window cannot open at all, so say so, with where to get it,
/// rather than fail silently. True when the launcher can go on.
fn webview_present() -> bool {
    #[cfg(windows)]
    if tauri::webview_version().is_err() {
        let open = rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Error)
            .set_title("Dereth")
            .set_description(
                "Dereth needs the Microsoft Edge WebView2 Runtime to show its window, and it is not \
                 installed on this computer.\n\nInstall the Evergreen runtime from Microsoft, then \
                 start Dereth again. Open the download page now?",
            )
            .set_buttons(rfd::MessageButtons::YesNo)
            .show();
        if open == rfd::MessageDialogResult::Yes {
            let _ = backend::open_url(WEBVIEW2_URL);
        }
        return false;
    }
    true
}

fn vault() -> (Box<dyn dereth_launch::vault::Vault + Send>, &'static str) {
    #[cfg(target_os = "macos")]
    {
        (Box::new(vault_keyring::SystemVault), "the macOS Keychain")
    }
    #[cfg(windows)]
    {
        (
            Box::new(vault_keyring::SystemVault),
            "Windows Credential Manager",
        )
    }
    #[cfg(target_os = "linux")]
    {
        (Box::new(vault_keyring::SystemVault), "the desktop keyring")
    }
    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
    {
        (
            Box::new(dereth_launch::vault::MemoryVault::default()),
            "memory only",
        )
    }
}

/// What `dereth --version` prints: the version, and the commit and target a release build was
/// made from (`unknown` in a build that did not say).
fn version_text() -> String {
    format!(
        "dereth {}\ncommit: {}\ntarget: {}\n",
        env!("CARGO_PKG_VERSION"),
        option_env!("DERETH_BUILD_COMMIT").unwrap_or("unknown"),
        option_env!("DERETH_BUILD_TARGET").unwrap_or("unknown"),
    )
}

fn main() {
    // `--version` alone: what this build is, with no window and nothing read or written.
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args == ["--version"] {
        print!("{}", version_text());
        return;
    }
    #[cfg(windows)]
    if args.len() == 3 && args[0] == "--grant-service" {
        // The elevated half of "Let the launcher start and stop this service".
        if let Err(error) = services::grant_elevated(&args[1], &args[2]) {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return;
    }
    #[cfg(windows)]
    if args.len() == 2
        && [
            "--host-service",
            "--install-host-service",
            "--remove-host-service",
        ]
        .contains(&args[0].as_str())
    {
        let dir = PathBuf::from(&args[1]);
        let result = if args[0] == "--host-service" {
            host_service::dispatch(dir.clone())
        } else {
            host_service::install_elevated(&dir, args[0] == "--remove-host-service")
        };
        if let Err(error) = result {
            let _ = std::fs::write(dir.join("service-error.txt"), error);
            std::process::exit(1);
        }
        return;
    }
    if (args.len() == 2 || args.len() == 3) && args[0] == "--run-host" {
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let signal = stop.clone();
        if let Err(error) =
            ctrlc::set_handler(move || signal.store(true, std::sync::atomic::Ordering::SeqCst))
        {
            eprintln!("Could not install the host stop handler: {error}");
            std::process::exit(1);
        }
        match host_runtime::run(
            Path::new(&args[1]),
            &stop,
            args.get(2).and_then(|s| s.parse().ok()),
        ) {
            Ok(()) => return,
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
    }
    if args.first().is_some_and(|a| a == "--publish-host") {
        if args.len() != 4 {
            eprintln!("usage: dereth --publish-host <host-folder> <world.toml> <lyceum-program>");
            std::process::exit(2);
        }
        match host_publication::publish(
            Path::new(&args[1]),
            Path::new(&args[2]),
            Path::new(&args[3]),
        ) {
            Ok(publication) => println!(
                "{}",
                serde_json::to_string(&publication).expect("publication JSON")
            ),
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        return;
    }
    let exe = std::env::current_exe().unwrap_or_default();
    let here = exe.parent().map(Path::to_path_buf).unwrap_or_default();
    let folders = find_folders(&here);
    // A join link for a launcher already open goes to that one.
    let link = dereth_launch::join::from_args(&args).map(str::to_owned);
    if link
        .as_deref()
        .is_some_and(|l| join::hand_off(&folders.data, l))
    {
        return;
    }
    if !webview_present() {
        std::process::exit(1);
    }
    // On Linux, the window's application id and the desktop entry that gives it its icon.
    registration::on_start(!components::DEVELOPMENT);
    join::register(!components::DEVELOPMENT);
    // What the last update set aside is no longer held open by anything.
    dereth_launch::swap::clean_up(&here);
    let pending: join::Pending = std::sync::Arc::default();
    if let Some(l) = &link {
        join::offer(&pending, l);
    }
    let running = std::sync::Mutex::new(Some(join::Running::mark(&folders.data)));
    let is_release = client_beside(&here).is_some();
    let client = find_client(&here);
    let tool = find_tool(&here, client.as_deref());
    // The preferences file the Dereth client reads when it is started from its own folder, which
    // names the interface it starts in.
    let client_preferences = dereth_launch::interface::preferences_file(
        System::HOST,
        &|name| std::env::var_os(name),
        client.as_deref().and_then(Path::parent),
    );

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_deep_link::init())
        .manage(pending)
        .manage(update::Updates::default())
        .manage(components::new(
            folders.data.clone(),
            &here,
            components::uses_checkout(components::DEVELOPMENT, &args),
        ))
        .manage(hosts::new(folders.data.clone()))
        .manage(local::new(folders.data.clone()))
        .manage(share::Shared::default())
        .setup(move |app| {
            // The window is made here rather than from the configuration so that the web view keeps
            // its cache in the launcher's data folder, not in one named after the app's identifier.
            let config = app
                .config()
                .app
                .windows
                .first()
                .cloned()
                .ok_or("no window in the configuration")?;
            tauri::WebviewWindowBuilder::from_config(app.handle(), &config)?
                .data_directory(folders.webview())
                .devtools(DEVTOOLS)
                .build()?;

            // The hub's list (another hub's when `DERETH_HUB_LIST` names it), then the community's
            // own when the hub does not answer with one; a list named by `DERETH_SERVERS_LIST` is
            // read alone.
            let list_sources = serverlist::sources_from(
                std::env::var("DERETH_SERVERS_LIST").ok().as_deref(),
                std::env::var("DERETH_HUB_LIST").ok().as_deref(),
            );
            let (vault, name) = vault();
            let shared: Shared = std::sync::Arc::new(std::sync::Mutex::new(
                Backend::new(
                    folders.clone(),
                    list_sources,
                    client.clone(),
                    vault,
                    name,
                    client_preferences.clone(),
                )
                .with_tool(tool.clone()),
            ));
            Backend::load_list(&shared);
            // Every plugin source's list is read at start, and the updates the setting allows are
            // applied.
            let checking = shared.clone();
            std::thread::spawn(move || {
                Backend::refresh_plugins_at_start(&checking, &backend::fetch_plugin_file);
            });
            // The author's tool's copy in each folder Make Your Own wrote in is this launcher's
            // tool again: after an update, the new release's.
            let keeping = shared.clone();
            std::thread::spawn(move || Backend::keep_tools_current(&keeping));
            let ticker = shared.clone();
            std::thread::spawn(move || loop {
                Backend::tick(&ticker);
                std::thread::sleep(Duration::from_secs(1));
            });
            // What a game it started is showing on Discord, when Discord runs here.
            discord::watch(shared.clone(), discord::Presence::from_env());
            // A pad moving about the page.
            pad::watch(app.handle().clone());
            app.manage(shared);
            // Lyceum's worlds go in a Lyceum folder in the person's Documents unless they choose
            // another; the editor's New world starts there too.
            let documents = app.path().document_dir().ok().or_else(|| {
                app.path()
                    .home_dir()
                    .ok()
                    .map(|home| home.join("Documents"))
            });
            components::set_worlds_home(
                app.state::<components::Shared>().inner(),
                documents.map(|d| d.join(components::WORLDS_FOLDER)),
            );
            update::check(app.handle(), is_release);
            hosts::watch(app.state::<hosts::Shared>().inner().clone());
            // Join links: one a second launcher leaves, and on macOS one the system hands over.
            join::watch(app.handle().clone(), folders.data.clone());
            {
                use tauri_plugin_deep_link::DeepLinkExt;
                let pending = app.state::<join::Pending>().inner().clone();
                let handle = app.handle().clone();
                app.deep_link().on_open_url(move |event| {
                    for url in event.urls() {
                        join::offer(&pending, url.as_str());
                    }
                    if let Some(window) = handle.get_webview_window("main") {
                        let _ = window.unminimize();
                        let _ = window.set_focus();
                    }
                });
                if let Ok(Some(urls)) = app.deep_link().get_current() {
                    let pending = app.state::<join::Pending>();
                    for url in urls {
                        join::offer(pending.inner(), url.as_str());
                    }
                }
            }
            Ok(())
        })
        // The window is never maximized. `maximizable: false` in the configuration removes the
        // button on Windows and macOS (and with it the title bar's double-click there), but GTK
        // has no such setting, so on Linux the window manager's button and double-click, and on
        // any system a snap to the top edge, are undone here. Resizing by the edges is untouched.
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if hosts::confirm_close(
                    window.app_handle(),
                    &window.state::<hosts::Shared>(),
                    &window.state::<local::Shared>(),
                ) {
                    api.prevent_close();
                }
            }
            if matches!(event, tauri::WindowEvent::Resized(_))
                && window.is_maximized().unwrap_or(false)
            {
                let _ = window.unmaximize();
            }
        })
        .invoke_handler(tauri::generate_handler![
            snapshot,
            restart_to_update,
            refresh,
            world_view,
            has_password,
            launch,
            pick_folder,
            add_folder,
            set_default_set,
            add_custom_dats,
            create_private_copy,
            reset_set,
            delete_set,
            forget_retail,
            verify_retail,
            save_favourite,
            remove_favourite,
            forget_account,
            set_remember,
            forget_all_passwords,
            forget_all_accounts,
            add_custom_world,
            probe_world,
            remove_custom_world,
            set_interface,
            set_discord_presence,
            dismiss_message,
            open_url,
            create_desktop_shortcut,
            pad::pad_target,
            components::check_components,
            components::get_component,
            components::remove_component,
            components::open_lyceum,
            components::open_worlds_folder,
            components::choose_lyceum_world,
            components::choose_lyceum_parent,
            components::new_lyceum_world,
            components::upgrade_lyceum_world,
            components::delete_lyceum_world,
            components::export_lyceum_world,
            hosts::check_host_update,
            hosts::publish_host_world,
            hosts::unpublish_host_world,
            hosts::open_host_world,
            hosts::cancel_host_update,
            hosts::approve_host_update,
            hosts::rollback_host_update,
            hosts::choose_host_dats,
            hosts::create_host,
            hosts::prepare_host,
            hosts::start_host,
            hosts::stop_host,
            hosts::save_host,
            hosts::save_host_key,
            hosts::open_host_config,
            hosts::export_host_logs,
            hosts::open_host_folder,
            hosts::remove_host,
            hosts::set_host_service,
            hosts::host_command,
            hosts::reveal_host,
            local::scan_local_worlds,
            local::start_local_world,
            local::stop_local_world,
            local::local_command,
            local::forget_local_world,
            local::reveal_local_world,
            open_local_world,
            local::allow_service_control,
            share::check_sharing,
            share::check_from_outside,
            join::take_join,
            refresh_plugins,
            add_plugin_source,
            remove_plugin_source,
            install_plugin,
            update_plugin,
            remove_plugin,
            set_plugin_updates,
            create_plugin,
            pick_plugin_folder,
            add_plugin_folder,
            remove_plugin_folder,
            open_plugin_folder,
        ])
        .build(tauri::generate_context!())
        .expect("the launcher window could not start");
    app.run(move |_, event| {
        if matches!(event, tauri::RunEvent::Exit) {
            // No launcher is open for a join link once this one has closed.
            running
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take();
        }
    });
}

#[cfg(test)]
mod tests {
    /// The release smoke run reads the first line: the program's name and its version.
    #[test]
    fn the_version_text_names_the_launcher_and_its_version_first() {
        let text = super::version_text();
        assert_eq!(
            text.lines().next().unwrap(),
            format!("dereth {}", env!("CARGO_PKG_VERSION"))
        );
        assert!(text.lines().any(|l| l.starts_with("target: ")), "{text}");
    }

    /// A release build has no developer tools, and the page then swallows every right-click
    /// outside a text box, so the web view's own menu (Reload, Inspect) never opens there. The
    /// page's handler is keyed on the snapshot's `devtools` and on the `contextmenu` event, and a
    /// text box keeps its own cut, copy and paste menu.
    #[test]
    fn a_release_build_has_no_devtools_and_the_page_hides_the_web_views_menu() {
        assert_eq!(
            super::DEVTOOLS,
            cfg!(debug_assertions),
            "developer tools only in a debug build"
        );
        let page = include_str!("../ui/app.js");
        let handler = page
            .find("addEventListener(\"contextmenu\"")
            .expect("the page handles the right-click menu");
        let rest = &page[handler..];
        let body = &rest[..rest.find("});").unwrap_or(rest.len())];
        assert!(body.contains("preventDefault()"), "{body}");
        assert!(body.contains("devtools"), "{body}");
        assert!(
            body.contains("input, textarea"),
            "text boxes keep their menu: {body}"
        );
    }

    /// Every command the page calls is one the app registers, and one the demo backend answers, so
    /// a new control never reaches the player calling nothing.
    #[test]
    fn every_command_the_page_calls_is_registered_and_in_the_demo() {
        let page = include_str!("../ui/app.js");
        let demo = include_str!("../ui/demo.js");
        let main = include_str!("main.rs");
        let handler = &main[main.find("generate_handler![").expect("the handler list")..];
        let handler = &handler[..handler.find(']').unwrap()];
        let mut called = std::collections::BTreeSet::new();
        for opener in ["api(\"", "act(\"", "pluginFetch(\""] {
            for (at, _) in page.match_indices(opener) {
                let rest = &page[at + opener.len()..];
                called.insert(&rest[..rest.find('"').unwrap()]);
            }
        }
        assert!(
            called.contains("add_custom_world") && called.contains("snapshot"),
            "{called:?}"
        );
        for cmd in called {
            assert!(
                handler
                    .split(|c: char| !(c.is_alphanumeric() || c == '_'))
                    .any(|w| w == cmd),
                "the page calls {cmd}, which the app does not register"
            );
            assert!(
                demo.contains(&format!("{cmd}:")),
                "the demo backend does not answer {cmd}"
            );
        }
    }

    /// The launcher's window can be resized by its edges and minimized, and never maximized.
    #[test]
    fn the_window_resizes_and_does_not_maximize() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).expect("the configuration");
        let windows = config["app"]["windows"].as_array().expect("a window list");
        assert_eq!(windows.len(), 1);
        let main = &windows[0];
        assert_eq!(main["label"], "main");
        assert_eq!(main["maximizable"], false, "{main}");
        assert_eq!(main["resizable"], true, "{main}");
        assert_ne!(main["minimizable"], false, "{main}");
    }

    /// The text of the page's function `name`, up to the next top-level function.
    fn page_function(name: &str) -> &'static str {
        let page = include_str!("../ui/app.js");
        let at = page
            .find(&format!("\nfunction {name}("))
            .or_else(|| page.find(&format!("\nasync function {name}(")))
            .unwrap_or_else(|| panic!("the page has {name}"));
        let rest = &page[at + 1..];
        let end = ["\nfunction ", "\nasync function "]
            .iter()
            .filter_map(|f| rest[1..].find(f))
            .min()
            .map_or(rest.len(), |e| e + 1);
        &rest[..end]
    }

    /// A world's log stays at its newest line as lines come, and stays put while the person reads
    /// further up or selects in it: the page notes each log's place before it draws anew, and puts
    /// it back after, and does not draw anew under a selection. (The same is driven in a browser
    /// against the demo page; this keeps the page wired that way.)
    #[test]
    fn the_log_follows_its_newest_line_unless_the_person_is_reading_or_selecting_in_it() {
        let render = page_function("render");
        let noted = render.find("logPlaces()").expect("noted before drawing");
        let drawn = render.find("main.innerHTML =").expect("drawn");
        let kept = render
            .find("keepLogPlaces(")
            .expect("put back after drawing");
        assert!(noted < drawn && drawn < kept, "{render}");
        let held = render
            .find("selectingInLog()")
            .expect("held under a selection");
        assert!(held < drawn, "{render}");
        let keep = page_function("keepLogPlaces");
        assert!(
            keep.contains("p.atEnd") && keep.contains("el.scrollTop = el.scrollHeight"),
            "a log at its end stays at its end: {keep}"
        );
        assert!(
            keep.contains("p.anchor.text"),
            "a log scrolled up keeps its line: {keep}"
        );
        let console = page_function("consoleBlock");
        assert!(console.contains(r#"class="log" data-log="#), "{console}");
    }

    /// The Plugins section is always there, with no switch to start the launcher with: the rail
    /// lists it, the shoulder buttons step to it, and its screen draws.
    #[test]
    fn the_plugins_section_is_always_there() {
        let app = include_str!("../ui/app.js");
        assert!(page_function("rail").contains(r#"    item("plugins", "PLUGINS"),"#));
        assert!(!app.contains("ui.snap?.plugins") && !app.contains("ui.snap.plugins ?"));
        assert!(page_function("padSection").contains("const sections = PAD_SECTIONS;"));
        assert!(page_function("render").contains(" plugins, \"first-run\": firstRun"));
        assert!(!include_str!("main.rs").contains(&["\"--", "plugins\""].concat()));
    }

    /// The Plugins page installs only from sources: a listed plugin is installed by its source and
    /// its id, with no box for a zip's address or its hash; a source other than Dereth's is added
    /// only once the player confirms it, on a line under its address rather than a panel above.
    #[test]
    fn the_plugins_page_installs_from_sources_and_confirms_another_source() {
        let row = page_function("availableRow");
        assert!(
            row.contains(r#"data-plugin-install="${esc(a.source)}" data-plugin-id="${esc(a.id)}""#),
            "{row}"
        );
        let page = page_function("plugins");
        assert!(
            !page.contains("SHA-256") && !page.contains("plugin.zip"),
            "{page}"
        );
        assert!(
            page.contains("Dereth does not review plugins from other sources.")
                && page.contains(r#"data-act="plugin-source-confirm""#)
                && !page.contains("panel pending"),
            "{page}"
        );
        let add = page_function("addPluginSource");
        assert!(
            add.contains("!confirmed && url !== ui.snap.plugins.first_party"),
            "{add}"
        );
        let asked = add
            .find("ui.plugin.confirm = true")
            .expect("the confirmation");
        let added = add
            .find(r#"pluginFetch("add_plugin_source", { url })"#)
            .expect("the call");
        assert!(asked < added, "{add}");
    }

    /// The Plugins page's update setting is the shared one, by the names the shared code reads,
    /// with its caption; no plugin has a choice of its own, and the page's top is left free.
    #[test]
    fn the_plugins_page_offers_the_shared_update_setting_and_no_choice_per_plugin() {
        use dereth_launch::plugins::AutoUpdate;
        let app = include_str!("../ui/app.js");
        let choices = r#"const PLUGIN_UPDATES = [["none", "None"], ["all", "All"], ["first_party", "Only Dereth"]];"#;
        assert!(app.contains(choices));
        for (name, setting) in [
            ("none", AutoUpdate::None),
            ("all", AutoUpdate::All),
            ("first_party", AutoUpdate::FirstParty),
        ] {
            assert_eq!(AutoUpdate::parse(name), Some(setting));
        }
        let page = page_function("plugins");
        assert!(page.contains(r#"<div class="name">Automatically update plugins</div>"#));
        assert!(page.contains(
            "Plugin updates that ask for additional permissions will never automatically install."
        ));
        assert!(!app.contains("set_plugin_auto_update") && !app.contains("data-plugin-auto"));
        assert!(!page.contains("Plugins run in the Dereth client only"));
        assert!(include_str!("../ui/demo.js").contains(r#"auto_update: "first_party","#));
    }

    /// The Plugins page's four sections fold by their headings, Installed open at first and the
    /// rest folded, and a plugin's row opens in place, one at a time, as the Worlds list's do.
    #[test]
    fn the_plugins_pages_sections_fold_and_one_plugin_row_opens_at_a_time() {
        let app = include_str!("../ui/app.js");
        assert!(app
            .contains("folded: { installed: false, available: true, sources: true, build: true }"));
        assert!(app.contains(
            "if (d.pluginRow) { ui.plugin.open = ui.plugin.open === d.pluginRow ? null : d.pluginRow;"
        ));
        assert!(app.contains(
            "if (d.pluginFold) { ui.plugin.folded[d.pluginFold] = !ui.plugin.folded[d.pluginFold];"
        ));
        assert!(page_function("pluginSection").contains(r#"(open ? body : "")"#));
        assert!(page_function("pluginRow").contains("const open = ui.plugin.open === key;"));
        let page = page_function("plugins");
        for key in ["installed", "available", "sources", "build"] {
            assert!(page.contains(&format!("pluginSection(\"{key}\"")), "{key}");
        }
    }

    /// The rail: Play, Worlds, Accounts and, while plugins are on, Plugins; then Empyrean and
    /// Lyceum; then the Library, each group apart. The shoulder buttons step through the sections
    /// in the same order.
    #[test]
    fn the_rail_has_plugins_under_accounts_and_above_empyrean() {
        let rail = page_function("rail");
        let listed = &rail[rail.find("return [").expect("the rail's list")..];
        let items: Vec<&str> = listed
            .match_indices("item(\"")
            .map(|(at, opener)| {
                let id = &listed[at + opener.len()..];
                &id[..id.find('"').unwrap_or(0)]
            })
            .collect();
        let order = [
            "home", "worlds", "accounts", "plugins", "server", "make", "library",
        ];
        assert_eq!(items, order);
        let at = |s: &str| listed.find(s).unwrap_or_else(|| panic!("{s}"));
        let divider = at("divider,");
        assert!(
            at(r#"item("plugins", "PLUGINS")"#) < divider
                && divider < at(r#"item("server", "EMPYREAN")"#),
            "{listed}"
        );
        let pad = order.map(|s| format!("\"{s}\"")).join(", ");
        assert!(
            include_str!("../ui/app.js").contains(&format!("const PAD_SECTIONS = [{pad}];")),
            "the shoulder buttons step in the rail's order"
        );
    }

    /// The Plugins page ends with Make Your Own: a name and an optional GitHub repository, and
    /// Create, which the launcher answers by checking them, asking for an empty folder or a plugin
    /// repository and writing the template there; the line under it says what was made, in the
    /// author's tool's words, and Open folder shows it.
    #[test]
    fn the_plugins_page_ends_with_make_your_own() {
        let page = page_function("plugins");
        let sources = page.find(r#"${pluginSection("sources""#).expect("Sources");
        let build = page
            .find(r#"${pluginSection("build", "MAKE YOUR OWN", "", build)}"#)
            .expect("Make Your Own");
        assert!(!include_str!("../ui/app.js").contains("BUILD PLUGINS"));
        assert!(sources < build, "{page}");
        for control in [
            r#"id="plugin-make-name""#,
            r#"id="plugin-make-github""#,
            r#"data-act="plugin-create" ${busy || !m.name.trim() ? "disabled" : ""}>Create</button>"#,
            r#"<span title="${esc(m.made.folder)}">${esc(m.made.said)}</span>"#,
            r#"data-act="plugin-open-made">Open folder</button>"#,
        ] {
            assert!(page.contains(control), "{control}");
        }
        let make = page_function("createPlugin");
        assert!(
            make.contains(r#"api("create_plugin", { name, github: m.github.trim() || null })"#),
            "{make}"
        );
        assert!(include_str!("../ui/app.js").contains(
            r#"case "plugin-open-made": await act("open_plugin_folder", { folder: ui.plugin.make.made.folder });"#
        ));
    }

    /// An open plugin row's labelled lines (what it uses, what it also asks for, what changed) put
    /// the value right after its label, at the facts' gap, wrapping under itself, rather than in a
    /// column of its own far to the right.
    #[test]
    fn an_open_plugin_rows_lines_put_each_value_right_after_its_label() {
        let style = include_str!("../ui/style.css");
        assert!(style.contains(
            ".plugin-detail .said { display: grid; grid-template-columns: max-content minmax(0, 1fr); column-gap: 7px;"
        ));
        assert!(
            style.contains(".strip span { display: inline-flex; align-items: center; gap: 7px; }")
        );
        assert!(page_function("pluginDetail").contains(r#"<div class="said"><b>USES</b>${esc("#));
    }

    /// The Plugins page reads every source again with one button, Refresh, which says what is new
    /// (or that nothing is); there is no other check for updates.
    #[test]
    fn the_plugins_page_refreshes_every_source_with_one_button() {
        let page = page_function("plugins");
        assert!(
            page.contains(
                r#"data-act="plugins-refresh" ${busy ? "disabled" : ""}>Refresh</button>"#
            ),
            "{page}"
        );
        let app = include_str!("../ui/app.js");
        assert!(app.contains(
            r#"case "plugins-refresh": await pluginFetch("refresh_plugins", {}); break;"#
        ));
        assert!(
            page_function("pluginFetch").contains(r#"cmd === "refresh_plugins" ? "Nothing new.""#)
        );
        assert!(!page.contains("Check for updates"));
    }

    /// Installed lists the plugins in folders on this computer, marked Local, and both rows of an
    /// id loaded from this computer say so. A Local row has no buttons of its own: open, its
    /// LOCATION line ends with Open and Remove, at the line's right. Add Plugin Folder, one button
    /// at the start of its line under the list, opens the system's picker and adds the folder
    /// chosen at once, with no question first and no box repeating the folder. With nothing
    /// installed, or nothing available, the line that says so sits at the rows' indent. A folder
    /// whose copy of the author's tool could not be brought up to date says so on its row.
    #[test]
    fn the_plugins_page_lists_folders_on_this_computer_and_adds_one_in_one_step() {
        let local = page_function("localPluginRow");
        for said in [
            r#"source_name: "Local""#,
            r#"<b>LOCATION</b><span class="path">${esc(x.folder)}</span>"#,
            r#"<span class="btns"><button class="btn" data-plugin-open="${esc(x.folder)}">Open</button><button class="btn" data-plugin-folder-remove="${esc(x.listed)}">Remove</button></span></div>"#,
            r#"const detail = (x.problem ? "" : pluginDetail(p)) + location;"#,
            r#"return pluginRow(`l:${x.folder}`, p, "", line + tool, detail);"#,
            r#"const tool = x.tool_problem ? `<div class="sub warn">${esc(x.tool_problem)}</div>` : "";"#,
            "x.in_place_of_installed ? `<div class=\"sub\">Loaded in place of the installed one.</div>`",
        ] {
            assert!(local.contains(said), "{said}");
        }
        assert!(
            !local.contains("FOLDER") && !local.contains("Open folder"),
            "{local}"
        );
        assert!(page_function("pluginRow")
            .contains(r#"${buttons ? `<div class="btns">${buttons}</div>` : ""}</div>`;"#));
        let style = include_str!("../ui/style.css");
        assert!(style.contains(
            ".plugin-location .btns { flex: none; flex-wrap: nowrap; margin-left: auto;"
        ));
        assert!(style.contains(".plugins-page .list > p.empty { margin: 6px 15px; }"));
        assert!(page_function("installedRow").contains("The local one is loaded in its place."));

        let page = page_function("plugins");
        assert!(page.contains("v.local.map(localPluginRow)"), "{page}");
        assert!(page.contains(
            r#"`<p class="muted empty">No plugins installed. Install one from the list below.</p>`"#
        ));
        // Available's lines for an empty list sit at the same indent.
        assert!(page.contains(
            r#"`<p class="muted empty">${q ? "No plugin matches." : listed ? "Every plugin the sources list is installed." : "No plugins listed yet. Refresh to read the sources."}</p>`"#
        ));
        assert!(
            page.contains(
                r#"const addFolder = `<div class="plugin-line add"><button class="btn" data-act="plugin-folder-add" ${busy ? "disabled" : ""}>Add Plugin Folder</button></div>`;"#
            ),
            "{page}"
        );
        let installed = page
            .find(r#"${pluginSection("installed""#)
            .expect("Installed");
        assert!(
            page[installed..].starts_with(
                r#"${pluginSection("installed", "INSTALLED", v.installed.length + v.local.length, `<div class="list">${installed}</div>${addFolder}${setting}`)}"#
            ),
            "the button sits under Installed's list"
        );
        let app = include_str!("../ui/app.js");
        for gone in [
            "plugin-folder-confirm",
            "plugin-folder-cancel",
            "plugin-folder-pick",
            r#"id="plugin-folder""#,
            "Plugins in this folder run with",
            "ui.plugin.folder",
            "pickPluginFolder",
        ] {
            assert!(!app.contains(gone), "{gone}");
        }
        let add = page_function("addPluginFolder");
        let picked = add
            .find(r#"folder = await api("pick_plugin_folder");"#)
            .expect("the picker");
        let added = add
            .find(r#"if (folder) await pluginFetch("add_plugin_folder", { folder });"#)
            .expect("added at once");
        assert!(picked < added, "{add}");
        assert!(app.contains(r#"case "plugin-folder-add": await addPluginFolder(); break;"#));
        assert!(app.contains(
            r#"if (d.pluginFolderRemove) { await act("remove_plugin_folder", { folder: d.pluginFolderRemove }); return; }"#
        ));
    }

    /// The emblems are on Empyrean's and Lyceum's headings, and on no button.
    #[test]
    fn each_emblem_is_on_its_pages_heading_and_on_no_button() {
        let page = include_str!("../ui/app.js");
        assert!(
            !page.contains("rail-icon"),
            "no emblem on the rail's buttons"
        );
        assert!(!page_function("rail").contains("<img"));
        let used: Vec<&str> = page
            .match_indices("brand(\"")
            .map(|(at, opener)| {
                let name = &page[at + opener.len()..];
                &name[..name.find('"').unwrap_or(0)]
            })
            .collect();
        assert_eq!(used, ["empyrean", "lyceum"], "used once each");
        assert!(page_function("serverList")
            .contains(r#"brand("empyrean")}<h1>EMPYREAN: HOST A WORLD SERVER"#));
        assert!(page_function("makePage").contains(r#"brand("lyceum")}<h1>LYCEUM: BUILD A WORLD"#));
    }

    /// What's new shows its links as links that open in the system's browser, and it and the log
    /// use the page's own scroll bar.
    #[test]
    fn whats_new_links_open_in_the_browser_and_it_and_the_log_use_the_pages_scroll_bar() {
        let line = page_function("programLine");
        assert!(line.contains("linkify(notes)"), "{line}");
        let linkify = page_function("linkify");
        assert!(
            linkify.contains("data-url="),
            "opened through open_url: {linkify}"
        );
        let style = include_str!("../ui/style.css");
        for scrolled in [
            "details.notes pre::-webkit-scrollbar-thumb",
            ".log::-webkit-scrollbar-thumb",
        ] {
            assert!(style.contains(scrolled), "{scrolled}");
        }
    }

    /// A join link opens Worlds with Add world filled in, for the person to add; the macOS bundle
    /// carries the scheme.
    #[test]
    fn a_join_link_fills_add_world_and_the_bundle_carries_the_scheme() {
        let take = page_function("takeJoin");
        assert!(
            take.contains(r#"api("take_join")"#) && take.contains("ui.addServer = {"),
            "{take}"
        );
        assert!(
            !take.contains("add_custom_world"),
            "the person adds it: {take}"
        );
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).expect("the configuration");
        assert_eq!(
            config["plugins"]["deep-link"]["desktop"]["schemes"],
            serde_json::json!(["dereth"])
        );
    }
}
