//! The desktop host: the window and its event loop, the message pump, the settings folders, the
//! sound device, the clipboard, the cursors, the log and the crash log, for any product that runs
//! the client shell on a desktop.
//!
//! **Depends on** the client shell (`dereth-client-shell`), whose `Host` this implements, the
//! runtime it starts (`dereth-client-runtime`), the contract (`dereth-client-contract`) and
//! `dereth-primitives`, the device crate's window arithmetic (`dereth-render`), the device
//! input's key tables (`dereth-input`), the dat locator (`dereth-dat`), the two unsafe hops
//! (`dereth-clipboard`, `dereth-console`), and the system fonts it hands the classic interface
//! (`dereth-classic-gdi`, in the atlas format of `dereth-classic-dat`). **Used by** the desktop client
//! (`dereth-client`), and by any other desktop product built on the client shell.
//!
//! **Must never** hold a UI, a screen or a game rule: it is the platform under the application, and
//! a product is the [`Product`] it names -- its binary, its settings folder, its window's title and
//! icon, and the few choices a product makes about its window.
//!
//! A product names itself once, as a type implementing [`Product`], and takes
//! [`Desktop<P>`](Desktop) as its shell's host. Its `main` is [`crashlog::install`], [`start`], its
//! own bring-up and frame loop, and [`crashlog::finished`].

use dereth_client_runtime::config::Config;
use dereth_client_shell::cursor::CursorImages;

// The sound device: the runtime's mixer over `cpal`.
pub mod audio;
// The clipboard bridge over the system clipboard.
pub mod clipboard;
// The cursor state over the system cursors.
/// The crash log: a record on disk of every run that goes wrong, written from `main`.
pub mod crashlog;
pub mod cursor;
// Where a product keeps its files on this platform, and the one-time copy of the original game's.
pub mod folders;
mod host;
// The HUD model's platform answers.
pub mod hud;
/// Opening the window, and handing a URL to the desktop.
pub mod launch;
/// The log subscriber every crate's events go to.
pub mod logging;
/// The operating system's half of the platform: the window and its event loop, the local time zone
/// every date the client draws needs, and the OS error box.
pub mod platform;
// The message pump: `winit` events in, Win32 messages out, the window procedure in the middle.
pub mod pump;

pub use host::Desktop;

/// A product on the desktop host: what makes it itself rather than another client on the same
/// shell. Everything has the Dereth client's answer except the names and the pictures.
pub trait Product: 'static {
    /// The executable's name: its log file (`<binary>.log`) and crash logs (`<binary>-<pid>.log`).
    const BINARY_NAME: &'static str;
    /// The running program's name and version, as `@version` prints it.
    const BUILD_ID: &'static str;
    /// The folder, in the platform's Dereth settings folder, this product keeps its files in.
    const SETTINGS_DIR_NAME: &'static str;
    /// The window's title, before the account name the title adds.
    const TITLE: &'static str;
    /// The window's icon, an 8-bit RGBA PNG, for the window systems that take one as pixels.
    const ICON_PNG: &'static [u8];
    /// The application id a Linux desktop finds the window's `.desktop` entry by.
    const APP_ID: &'static str;
    /// Whether frames slow down while the window is in the background, as the original's did.
    const THROTTLE_IN_BACKGROUND: bool = true;

    /// Anything to do before the window opens: on Linux, the Dereth client writes its `.desktop`
    /// entry here, which is where a Wayland desktop takes the window's icon from.
    fn before_window() {}

    /// The cursor images, put on `window` (the native handle, `None` without one): the system
    /// cursors made from the dat's, unless the product draws its own.
    fn cursor_images(window: Option<isize>) -> Box<dyn CursorImages> {
        cursor::desktop_cursor_images(window)
    }
}

/// The start of a desktop product's `main`, before the client exists: parse `argv` (without the
/// program name), make the first-run copy and load the preferences, find the retail dats, make the
/// settings folder and install the log. Answers the configuration to bring the client up with, or
/// the sentence to end the run with.
///
/// # Errors
/// The text the product shows and exits with: a command line that does not parse, or no retail dats.
pub fn start<P: Product>(argv: &[String]) -> Result<Config, String> {
    // Step 9: parse the command line. On failure -> corestrings 205 -> exit.
    let default_preferences =
        crate::folders::default_preferences_file(P::SETTINGS_DIR_NAME).unwrap_or_default();
    // Parse, make the first-run copy, and load the preferences after it (`crate::folders::load_config`),
    // before the directory is created: "the new directory holds nothing yet" is exactly what makes
    // this the first run. On Windows the original game's settings are copied (never moved) into it.
    // The outcome is logged below, once the log (which may write into that directory) is installed.
    let settings_dir = crate::folders::default_settings_dir(P::SETTINGS_DIR_NAME);
    let original_settings_dir = crate::folders::import_source();
    let (cfg, copied) = crate::folders::load_config(
        argv,
        &default_preferences,
        settings_dir.as_deref(),
        original_settings_dir.as_deref(),
    )
    .map_err(|e| e.to_string())?;
    // The retail dats, before anything else is started: `--dat-dir`, else the working directory,
    // else the executable's directory. A run that has none says where it looked and how to say
    // where they are. The install is then read-only for the run: a data-patch message that would
    // save into it is refused.
    if !dereth_dat::holds_retail_dats(&cfg.dat_dir) && !dereth_dat::holds_pre_tod_dats(&cfg.dat_dir)
    {
        let mut searched = dereth_client_runtime::config::dat_dir_candidates();
        if !searched.contains(&cfg.dat_dir) {
            searched = vec![cfg.dat_dir.clone()];
        }
        let e = dereth_dat::locate_retail_dats(&searched)
            .err()
            .map_or_else(String::new, |e| e.to_string());
        return Err(format!(
            "{} ({e}; pass --dat-dir <dir> naming the folder that holds them)",
            dereth_client_runtime::corestrings::display_string(
                dereth_client_runtime::corestrings::ID_CANT_OPEN_DATA_FILES,
                &[]
            )
        ));
    }
    dereth_dat::protect_install(&cfg.dat_dir);
    // The settings directory is the installer's job in retail and there is no installer here, so
    // the binary makes it. This is the *only* place it is made: `App` must not, and
    // `physical_window_resize_reaches_the_backbuffer_and_ui_without_changing_preferences` names a
    // directory that does not exist precisely to prove that nothing along that path creates one.
    //
    // It matters on every platform: the product's folder under
    // `%APPDATA%\Dereth`, `~/Library/Application Support/Dereth` or `~/.config/dereth` does not
    // exist until something makes it.
    // Without this, every preference, keymap and screen layout the player saved would fail to
    // write with `NotFound` and say nothing -- the same silent-save failure as the original.
    //
    // A failure is ignored for the same reason startup ignores preference initialization errors: a client
    // that cannot write settings must still run. The first save then fails as it did before.
    if let Some(dir) = cfg.preferences_file.parent() {
        if !dir.as_os_str().is_empty() {
            let _ = std::fs::create_dir_all(dir);
        }
    }
    crate::logging::install::<P>(&cfg);
    // The crash log's path is not logged: it holds the pid, and stderr stays byte-identical
    // across runs. The pid is on every line of the file itself, which makes each run's record
    // independently identifiable.
    if let Some((from, to, outcome)) = copied {
        match outcome {
            Ok(0) => {}
            Ok(n) => tracing::info!(
                "first run -- copied {n} file(s) from {} to {}",
                from.display(),
                to.display()
            ),
            // Ignored for the same reason the create above is: a client that could not carry
            // the old settings over must still start, with the defaults it would have had.
            Err(e) => tracing::warn!("could not carry settings over from {}: {e}", from.display()),
        }
    }
    Ok(cfg)
}
