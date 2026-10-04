//! Dereth, the launcher, as a Tauri web app.
//!
//! The page (`ui/`) is the interface; [`backend`] is everything behind it, and `dereth_launch` is
//! what the launcher knows. Each command below is one thing the page can ask.
//!
//! Before the window opens, the launcher finds its folders ([`dereth_launch::folders`]) and removes
//! what its last update set aside. On Windows it first makes sure the WebView2 runtime is there to
//! draw the page with, and says where to get it when it is not; on Linux it registers its desktop
//! entry and icon ([`registration`]).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod backend;
// Only Linux registers a desktop entry; elsewhere the module is its tests and a no-op.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
mod registration;
mod shortcut;
mod update;
#[cfg(any(windows, target_os = "macos", target_os = "linux"))]
mod vault_keyring;

use std::path::{Path, PathBuf};
use std::time::Duration;

use backend::{lock, Backend, Choice, LaunchOutcome, Shared, Snapshot, WorldView};
use dereth_launch::folders::{self, Folders, System};
use dereth_launch::library::FolderFind;
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
    version: &'static str,
    /// Whether "Create desktop shortcut" is offered on this system.
    shortcut: bool,
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
fn snapshot(b: State<'_, Shared>, u: State<'_, update::Updates>) -> PageSnapshot {
    let update = u
        .view
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    PageSnapshot {
        snapshot: lock(&b).snapshot(),
        update,
        version: env!("CARGO_PKG_VERSION"),
        shortcut: shortcut::OFFERED,
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

/// Fetch the world list again, whatever its copy's age.
#[tauri::command]
fn refresh(b: State<'_, Shared>) {
    Backend::refresh(&b);
}

/// Choose the era of a world that does not say its own; `None` unchooses it.
#[tauri::command]
fn set_world_era(b: State<'_, Shared>, slug: String, era: Option<String>) -> Cmd<()> {
    lock(&b).set_world_era(&slug, era.as_deref())
}

/// Turn one of a world's systems on or off, for a world that does not say its own.
#[tauri::command]
fn set_world_feature(b: State<'_, Shared>, slug: String, name: String, on: bool) -> Cmd<()> {
    lock(&b).set_world_feature(&slug, &name, on)
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
    if !webview_present() {
        std::process::exit(1);
    }
    // On Linux, the window's application id and the desktop entry that gives it its icon.
    registration::on_start();
    let exe = std::env::current_exe().unwrap_or_default();
    let here = exe.parent().map(Path::to_path_buf).unwrap_or_default();
    // What the last update set aside is no longer held open by anything.
    dereth_launch::swap::clean_up(&here);
    let folders = find_folders(&here);
    let is_release = client_beside(&here).is_some();
    let client = find_client(&here);

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(update::Updates::default())
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

            let servers_list = std::env::var("DERETH_SERVERS_LIST")
                .unwrap_or_else(|_| backend::DEFAULT_SERVERS_LIST.into());
            let (vault, name) = vault();
            let shared: Shared = std::sync::Arc::new(std::sync::Mutex::new(Backend::new(
                folders.clone(),
                servers_list,
                client.clone(),
                vault,
                name,
            )));
            Backend::load_list(&shared);
            let ticker = shared.clone();
            std::thread::spawn(move || loop {
                Backend::tick(&ticker);
                std::thread::sleep(Duration::from_secs(1));
            });
            app.manage(shared);
            update::check(app.handle(), is_release);
            Ok(())
        })
        // The window is never maximized. `maximizable: false` in the configuration removes the
        // button on Windows and macOS (and with it the title bar's double-click there), but GTK
        // has no such setting, so on Linux the window manager's button and double-click, and on
        // any system a snap to the top edge, are undone here. Resizing by the edges is untouched.
        .on_window_event(|window, event| {
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
            set_world_era,
            set_world_feature,
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
            dismiss_message,
            open_url,
            create_desktop_shortcut,
        ])
        .run(tauri::generate_context!())
        .expect("the launcher window could not start");
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
        for opener in ["api(\"", "act(\""] {
            for (at, _) in page.match_indices(opener) {
                let rest = &page[at + opener.len()..];
                called.insert(&rest[..rest.find('"').unwrap()]);
            }
        }
        assert!(
            called.contains("set_world_era") && called.contains("snapshot"),
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
}
