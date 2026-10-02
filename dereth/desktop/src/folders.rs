//! **Where the client keeps its own files**, and the first-run copy of the original game's.
//!
//! Everything the player saves lands in one directory, the *settings directory*, through the
//! parent of the preferences file:
//!
//! | file | built by |
//! |---|---|
//! | `UserPreferences.ini` | [`default_preferences_file`] |
//! | `dereth.keymap` and any saved-as keymap | the front end's `keymap_path_for` |
//! | `UI-Default.txt`, `UI-<char>-<world>-<h>-<w>.txt` | `dereth_client::persist::layout_path` |
//! | `ScreenShot%05d.png` | `dereth_client::app::App::screenshot_path` |
//! | `Journal-<world>-<char>.txt` | `dereth_client_contract::journal::JournalIdentity::client_path` |
//!
//! The crash logs are here too, in [`crash_log_dir`], though they are written before anything else
//! starts: the directory is computed from the environment alone, so a log about a start-up that
//! failed does not depend on start-up having succeeded. A run that ends cleanly removes its own,
//! and each start keeps only the newest [`CRASH_LOGS_KEPT`] ([`prune_crash_logs`]). `@log`'s chat
//! log is the one write that keeps the original rule and is resolved against the working directory.
//!
//! # The per-platform place
//!
//! Dereth's state lives under one folder per platform, shared by the launcher and the client, each
//! in a folder of its own inside it. The client's is `client`:
//!
//! | platform | the client's settings directory |
//! |---|---|
//! | Windows | `%APPDATA%\Dereth\client` |
//! | macOS | `~/Library/Application Support/Dereth/client` |
//! | Linux and other Unix | `$XDG_CONFIG_HOME/dereth/client`, or `~/.config/dereth/client` |
//!
//! The folder is named for this client and not for the original game (client divergence CD-007):
//! this client's files are not the original's, and a folder of its own keeps them from sharing a
//! directory with a retail installation. The client keeps nothing large or rebuildable, so it has
//! no cache folder.
//!
//! **`DERETH_SETTINGS_DIR`** ([`SETTINGS_DIR_ENV`]) names the running product's settings directory
//! outright, in place of every rule in this section and of a `UserPreferences.ini` in the working
//! directory; only `-prefs` still wins over it. A relative value is taken against the working
//! directory. It is how a test, a script or a second copy of the client runs with folders of its
//! own and leaves the player's untouched, crash logs included.
//!
//! On Windows `%APPDATA%` is the roaming profile folder, so a player's settings follow them the way
//! the rest of their profile does; a program that ships as a zip keeps nothing beside itself.
//! On Linux the base is the XDG base directory specification's, whose rule for a relative or unset
//! `XDG_CONFIG_HOME` is to ignore it and fall back.
//!
//! This is the desktop host's question: resolving a place from the environment is platform I/O,
//! and the runtime below takes the answer as a path.
//!
//! # The original game's settings
//!
//! On Windows the first run **copies** the original game's `Documents\Asheron's Call` in
//! ([`copy_settings_dir`]) when the client's directory still holds nothing: a retail installation
//! is still using that directory, so it is left exactly as it was.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use dereth_client_runtime::config::{Config, ConfigError, PREFERENCES_FILE_NAME};

/// The Dereth client's folder inside the state root. The launcher keeps its own beside it, and
/// another product on this host keeps its own beside both (`Product::SETTINGS_DIR_NAME`).
pub const CLIENT_DIR_NAME: &str = "client";

/// The folder inside the settings directory that holds the crash logs, one
/// `dereth-client-<pid>.log` per run.
pub const CRASH_LOG_DIR_NAME: &str = "crash-logs";

/// How many crash logs [`prune_crash_logs`] keeps at each start.
pub const CRASH_LOGS_KEPT: usize = 20;

/// The environment variable that names the settings directory outright. See the module
/// documentation.
pub const SETTINGS_DIR_ENV: &str = "DERETH_SETTINGS_DIR";

/// The original game's settings directory under `Documents`, which a retail installation writes.
pub const RETAIL_SETTINGS_DIR_NAME: &str = "Asheron's Call";

/// The environment, as the resolvers below read it: one variable by name.
type Env<'a> = &'a dyn Fn(&str) -> Option<OsString>;

/// A variable's value as a path, when it is set, non-empty and absolute. A relative value is no
/// answer at all: it would resolve against whatever the working directory happens to be.
fn absolute(env: Env<'_>, name: &str) -> Option<PathBuf> {
    env(name).map(PathBuf::from).filter(|p| p.is_absolute())
}

/// Windows' state root, `%APPDATA%\Dereth`, or `%USERPROFILE%\AppData\Roaming\Dereth` when
/// `APPDATA` is unset. `None` when neither is.
fn windows_state_root(env: Env<'_>) -> Option<PathBuf> {
    absolute(env, "APPDATA")
        .or_else(|| absolute(env, "USERPROFILE").map(|h| h.join("AppData").join("Roaming")))
        .map(|base| base.join("Dereth"))
}

/// macOS' state root, `~/Library/Application Support/Dereth`.
fn macos_state_root(env: Env<'_>) -> Option<PathBuf> {
    absolute(env, "HOME").map(|h| h.join("Library").join("Application Support").join("Dereth"))
}

/// The XDG state root, `$XDG_CONFIG_HOME/dereth`, or `~/.config/dereth`. Lower case, as the
/// convention there is.
fn xdg_state_root(env: Env<'_>) -> Option<PathBuf> {
    absolute(env, "XDG_CONFIG_HOME")
        .or_else(|| absolute(env, "HOME").map(|h| h.join(".config")))
        .map(|base| base.join("dereth"))
}

/// `%USERPROFILE%\Documents`, spelled the way the original resolves its documents folder.
fn windows_documents(env: Env<'_>) -> Option<PathBuf> {
    absolute(env, "USERPROFILE").map(|h| h.join("Documents"))
}

/// The process environment.
fn process_env(name: &str) -> Option<OsString> {
    std::env::var_os(name)
}

/// The folder Dereth's launcher and client keep their settings under, on this platform. `None`
/// when the environment names no home at all, in which case the client saves nothing.
#[must_use]
pub fn state_root() -> Option<PathBuf> {
    let env: Env<'_> = &process_env;
    if cfg!(windows) {
        windows_state_root(env)
    } else if cfg!(target_os = "macos") {
        macos_state_root(env)
    } else {
        xdg_state_root(env)
    }
}

/// **The one directory every setting a product writes lives in**: [`SETTINGS_DIR_ENV`] when it
/// is set, otherwise its folder `dir_name` (for the Dereth client, [`CLIENT_DIR_NAME`]) in
/// [`state_root`]. See the module documentation for each platform's answer.
#[must_use]
pub fn default_settings_dir(dir_name: &str) -> Option<PathBuf> {
    settings_dir_override().or_else(|| state_root().map(|root| root.join(dir_name)))
}

/// [`SETTINGS_DIR_ENV`]'s directory, made absolute against the working directory; `None` when the
/// variable is unset or empty.
fn settings_dir_override() -> Option<PathBuf> {
    process_env(SETTINGS_DIR_ENV)
        .filter(|v| !v.is_empty())
        .and_then(|v| std::path::absolute(v).ok())
}

/// Where the client binary writes its crash logs: [`CRASH_LOG_DIR_NAME`] in
/// [`default_settings_dir`], whatever `-prefs` says, because the log is opened before the command
/// line is read.
#[must_use]
pub fn crash_log_dir(dir_name: &str) -> Option<PathBuf> {
    default_settings_dir(dir_name).map(|dir| dir.join(CRASH_LOG_DIR_NAME))
}

/// Remove all but the newest `keep` crash logs of the binary `binary` (`<binary>-*.log`) in `dir`,
/// newest by modification time, and return how many were removed. Nothing else in `dir` is
/// touched, and a file that cannot be read or removed is left where it is: this runs at start-up
/// and must never stop a run.
pub fn prune_crash_logs(dir: &Path, binary: &str, keep: usize) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    let prefix = format!("{binary}-");
    let mut logs: Vec<(std::time::SystemTime, PathBuf)> = entries
        .flatten()
        .filter(|e| {
            let name = e.file_name();
            let name = name.to_string_lossy();
            name.starts_with(&prefix) && name.ends_with(".log")
        })
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            meta.is_file()
                .then(|| (meta.modified().unwrap_or(std::time::UNIX_EPOCH), e.path()))
        })
        .collect();
    // Newest first; a tie falls to the name, so the order is the same on every listing.
    logs.sort_by(|a, b| b.cmp(a));
    logs.iter()
        .skip(keep)
        .filter(|(_, path)| std::fs::remove_file(path).is_ok())
        .count()
}

/// `%USERPROFILE%\Documents\Asheron's Call`, the original game's settings directory. Windows only;
/// `None` everywhere else, because the original never ran anywhere else.
#[must_use]
pub fn retail_settings_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        windows_documents(&process_env).map(|d| d.join(RETAIL_SETTINGS_DIR_NAME))
    } else {
        None
    }
}

/// Choose the one in [`SETTINGS_DIR_ENV`]'s folder when that is set, else
/// `<cwd>/UserPreferences.ini` when it exists, otherwise the one in [`default_settings_dir`];
/// `None` when there is none.
///
/// The cwd probe is the original's first branch and is kept on every platform: it is what makes a
/// portable install -- drop a `UserPreferences.ini` beside the binary and the whole settings
/// directory moves with it -- and it is the override that needs no new flag. `-prefs <file>`
/// replaces the answer outright.
#[must_use]
pub fn default_preferences_file(dir_name: &str) -> Option<PathBuf> {
    // A folder named outright is a stronger request than a file that happens to be in the cwd.
    if let Some(dir) = settings_dir_override() {
        return Some(dir.join(PREFERENCES_FILE_NAME));
    }
    let cwd = std::env::current_dir()
        .unwrap_or_default()
        .join(PREFERENCES_FILE_NAME);
    if cwd.exists() {
        return Some(cwd);
    }
    default_settings_dir(dir_name).map(|dir| dir.join(PREFERENCES_FILE_NAME))
}

/// Whether `dir` already holds settings: anything but the crash logs, which are written before the
/// first-run copy runs and so do not mean it has run.
fn holds_settings(dir: &Path) -> bool {
    std::fs::read_dir(dir).map_or(dir.exists(), |entries| {
        entries
            .flatten()
            .any(|e| e.file_name() != CRASH_LOG_DIR_NAME)
    })
}

/// **The first-run copy of the original game's settings**, `from` being [`retail_settings_dir`]
/// and `to` [`default_settings_dir`]. Returns how many files were copied.
///
/// It **copies and does not move**: `from` is `Documents\Asheron's Call`, which a live retail
/// installation is also using, and moving it would take the retail client's own preferences,
/// keymap, journals and screenshots away from it. Nothing is ever written into `from`.
///
/// "One time" is expressed by the state on disk rather than by a stamp file: **`to` must hold no
/// settings.** A `to` holding only [`CRASH_LOG_DIR_NAME`] still counts as new, because the crash
/// log is written before this runs; after a copy `to` holds the player's files, so a later start
/// cannot re-enter, and a player who already has this client's files keeps them. Subdirectories
/// are carried too, and an entry already present in `to` is left alone.
///
/// `Ok(0)` covers every reason not to act -- `to` already holding settings, `from` not there, or
/// not a directory. A copy that fails part-way leaves what it managed and reports the error; the
/// caller tolerates it, because a client that could not carry settings over must still run.
///
/// # Errors
/// The first `std::fs` error from creating `to` or walking and copying `from`.
pub fn copy_settings_dir(from: &Path, to: &Path) -> std::io::Result<u32> {
    if holds_settings(to) || !from.is_dir() {
        return Ok(0);
    }
    std::fs::create_dir_all(to)?;
    copy_dir_contents(from, to)
}

/// What the first-run copy did: from where, to where, and how many files (or why it failed).
pub type FirstRunCopy = (PathBuf, PathBuf, std::io::Result<u32>);

/// The command line parsed and the preferences loaded, **after** the first-run copy.
///
/// The copy runs when this run loads the settings folder's own preferences file (no `-prefs`, no
/// `UserPreferences.ini` in the working directory, not a headless run that named none) and that
/// folder holds nothing yet. When it copied anything, the configuration is read again, so the
/// window and the options screen both start from the copied settings rather than one of them from
/// the defaults loaded before the copy.
///
/// # Errors
/// [`ConfigError`] for a command line that does not parse.
pub fn load_config(
    argv: &[String],
    default_preferences: &Path,
    settings_dir: Option<&Path>,
    original_settings_dir: Option<&Path>,
) -> Result<(Config, Option<FirstRunCopy>), ConfigError> {
    let load = || -> Result<Config, ConfigError> {
        let mut cfg = Config::from_args_and_prefs_at(argv, default_preferences)?;
        if cfg.preferences_file.as_os_str().is_empty() && !cfg.headless {
            cfg.preferences_file = default_preferences.to_path_buf();
        }
        Ok(cfg)
    };
    let mut cfg = load()?;
    // `default_preferences` answers the working directory's file when there is one, so a path
    // equal to the settings folder's own file means no `-prefs`, no file beside the binary, and a
    // load from the settings folder: the three conditions in one comparison.
    let copied = settings_dir
        .filter(|to| cfg.preferences_file == to.join(PREFERENCES_FILE_NAME))
        .and_then(|to| {
            original_settings_dir.map(|from| {
                let outcome = copy_settings_dir(from, to);
                (from.to_path_buf(), to.to_path_buf(), outcome)
            })
        });
    if matches!(copied, Some((_, _, Ok(n))) if n > 0) {
        cfg = load()?;
    }
    Ok((cfg, copied))
}

/// [`copy_settings_dir`]'s recursive half.
fn copy_dir_contents(from: &Path, to: &Path) -> std::io::Result<u32> {
    let mut copied = 0;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            if !target.exists() {
                std::fs::create_dir_all(&target)?;
            }
            copied += copy_dir_contents(&entry.path(), &target)?;
        } else if !target.exists() {
            std::fs::copy(entry.path(), &target)?;
            copied += 1;
        }
    }
    Ok(copied)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An environment of the given variables and nothing else.
    fn env_of(vars: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> {
        let vars: Vec<(String, OsString)> = vars
            .iter()
            .map(|(k, v)| ((*k).to_string(), OsString::from(v)))
            .collect();
        move |name| vars.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone())
    }

    /// An absolute path that is absolute on the host running the test, whatever that host is.
    fn abs(tail: &str) -> String {
        let base = if cfg!(windows) { "C:\\home" } else { "/home" };
        format!("{base}{}{tail}", std::path::MAIN_SEPARATOR)
    }

    /// A scratch folder under the system temporary folder, removed when dropped.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(what: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "dereth-folders-{what}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_nanos())
            ));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("a scratch folder");
            Self(path)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Each platform's rule, asserted on every host: the resolvers take the environment as an
    /// argument, so all three are pure path arithmetic.
    ///
    /// Behaviour: presentation.settings.the-client-keeps-its-files-in-a-folder-of-its-own
    #[test]
    fn the_settings_directory_follows_the_platform_convention() {
        let user = abs("user");
        let roaming = abs("roaming");
        let xdg = abs("xdg");

        // Windows: `%APPDATA%\Dereth`, else the roaming folder under the profile.
        let env = env_of(&[("APPDATA", &roaming), ("USERPROFILE", &user)]);
        assert_eq!(
            windows_state_root(&env),
            Some(PathBuf::from(&roaming).join("Dereth"))
        );
        let env = env_of(&[("USERPROFILE", &user)]);
        assert_eq!(
            windows_state_root(&env),
            Some(
                PathBuf::from(&user)
                    .join("AppData")
                    .join("Roaming")
                    .join("Dereth")
            )
        );

        // macOS: Application Support, capitalised.
        let env = env_of(&[("HOME", &user)]);
        assert_eq!(
            macos_state_root(&env),
            Some(
                PathBuf::from(&user)
                    .join("Library")
                    .join("Application Support")
                    .join("Dereth")
            )
        );

        // XDG: `$XDG_CONFIG_HOME/dereth` when set and absolute, else `~/.config/dereth`.
        let env = env_of(&[("XDG_CONFIG_HOME", &xdg), ("HOME", &user)]);
        assert_eq!(
            xdg_state_root(&env),
            Some(PathBuf::from(&xdg).join("dereth"))
        );
        let env = env_of(&[("XDG_CONFIG_HOME", "relative/config"), ("HOME", &user)]);
        assert_eq!(
            xdg_state_root(&env),
            Some(PathBuf::from(&user).join(".config").join("dereth")),
            "a relative XDG_CONFIG_HOME is ignored"
        );

        // No home at all is no directory, rather than one relative to the working directory.
        let empty = env_of(&[]);
        assert_eq!(windows_state_root(&empty), None);
        assert_eq!(macos_state_root(&empty), None);
        assert_eq!(xdg_state_root(&empty), None);
        let env = env_of(&[("HOME", "relative")]);
        assert_eq!(macos_state_root(&env), None);

        // And this host's answer is the client's folder inside its own platform's root, unless the
        // environment running the test names a folder outright.
        if let Some(dir) =
            default_settings_dir(CLIENT_DIR_NAME).filter(|_| settings_dir_override().is_none())
        {
            assert_eq!(
                dir.file_name().and_then(|n| n.to_str()),
                Some(CLIENT_DIR_NAME)
            );
            assert!(dir.is_absolute(), "{} is not absolute", dir.display());
            let root = dir
                .parent()
                .and_then(Path::file_name)
                .and_then(|n| n.to_str());
            if cfg!(any(windows, target_os = "macos")) {
                assert_eq!(root, Some("Dereth"));
            } else {
                assert_eq!(root, Some("dereth"));
            }
        }
    }

    /// Only Windows has the original game's folder to copy from, because the original never ran
    /// anywhere else.
    #[test]
    fn only_windows_has_the_original_games_settings_directory() {
        #[cfg(windows)]
        assert_eq!(
            retail_settings_dir().and_then(|d| d.file_name().map(ToOwned::to_owned)),
            Some(OsString::from(RETAIL_SETTINGS_DIR_NAME))
        );
        #[cfg(not(windows))]
        assert_eq!(retail_settings_dir(), None);
    }

    /// The preferences file is the settings directory's `UserPreferences.ini` -- unless the
    /// working directory has one, which is the original's first branch and the portable-install
    /// override.
    #[test]
    fn the_preferences_file_is_the_settings_directorys_unless_the_cwd_has_one() {
        let cwd_file = std::env::current_dir()
            .unwrap_or_default()
            .join(PREFERENCES_FILE_NAME);
        let actual = default_preferences_file(CLIENT_DIR_NAME);
        if let Some(dir) = settings_dir_override() {
            assert_eq!(
                actual,
                Some(dir.join(PREFERENCES_FILE_NAME)),
                "a named folder wins"
            );
        } else if cwd_file.exists() {
            assert_eq!(actual, Some(cwd_file), "a cwd UserPreferences.ini wins");
        } else {
            assert_eq!(
                actual,
                default_settings_dir(CLIENT_DIR_NAME).map(|d| d.join(PREFERENCES_FILE_NAME)),
                "otherwise the settings directory's"
            );
        }
    }

    /// Only the newest [`CRASH_LOGS_KEPT`] crash logs survive a start, oldest removed first, and
    /// nothing in the folder that is not one of the binary's crash logs is touched.
    ///
    /// Behaviour: none (tooling: how many diagnostic logs the client keeps).
    #[test]
    fn pruning_keeps_the_newest_twenty_crash_logs() {
        let scratch = Scratch::new("prune");
        let dir = &scratch.0;
        let base = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        for n in 0..25u64 {
            let path = dir.join(format!("dereth-client-{}.log", 1000 + n));
            put(&path, b"[START]");
            let f = std::fs::File::options()
                .write(true)
                .open(&path)
                .expect("opens");
            // The pid order is the reverse of the age order, so a name sort would keep the wrong
            // twenty.
            f.set_modified(base + std::time::Duration::from_secs(100 - n))
                .expect("a settable time");
        }
        put(&dir.join("notes.txt"), b"not a log");
        // Another product's log in the same folder is that product's to prune.
        put(&dir.join("other-product-1.log"), b"[START]");
        assert_eq!(CRASH_LOGS_KEPT, 20);
        assert_eq!(prune_crash_logs(dir, "dereth-client", CRASH_LOGS_KEPT), 5);
        let mut left: Vec<String> = std::fs::read_dir(dir)
            .expect("listed")
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        let mut want: Vec<String> = (0..20u64)
            .map(|n| format!("dereth-client-{}.log", 1000 + n))
            .collect();
        want.push("notes.txt".to_owned());
        want.push("other-product-1.log".to_owned());
        want.sort();
        assert_eq!(
            left, want,
            "the five oldest went, and the other files stayed"
        );
        assert_eq!(
            prune_crash_logs(dir, "dereth-client", CRASH_LOGS_KEPT),
            0,
            "a second pass removes nothing"
        );
    }

    /// Write `bytes` at `path`, making its folder.
    fn put(path: &Path, bytes: &[u8]) {
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("a folder");
        std::fs::write(path, bytes).expect("writable");
    }

    /// A machine with no original game's folder is the ordinary case; it is not a failure and it
    /// makes nothing.
    #[test]
    fn a_copy_with_nothing_to_copy_is_not_a_failure() {
        let scratch = Scratch::new("none");
        let to = scratch.0.join("new");
        assert_eq!(
            copy_settings_dir(&scratch.0.join("absent"), &to).expect("not an error"),
            0
        );
        assert!(!to.exists(), "and it did not create the destination");
    }

    /// The original game's folder is copied, everything including subfolders, and **left exactly
    /// as it was found** -- a retail installation is still using it. Only while the new folder
    /// holds nothing.
    #[test]
    fn the_first_run_copy_of_the_originals_folder_copies_everything_and_takes_nothing() {
        let scratch = Scratch::new("copy");
        let (from, to) = (scratch.0.join("Asheron's Call"), scratch.0.join("client"));
        // This run's own crash log, written before the copy, does not count as settings.
        put(
            &to.join(CRASH_LOG_DIR_NAME).join("dereth-client-2.log"),
            b"new",
        );
        put(
            &from.join("UserPreferences.ini"),
            b"[Net]\r\nUserName=keep-me\r\n",
        );
        put(&from.join("acclient.keymap"), b"keymap");
        put(&from.join("shots").join("ScreenShot00000.png"), b"png");

        assert_eq!(copy_settings_dir(&from, &to).expect("the copy runs"), 3);
        assert!(to.join("UserPreferences.ini").is_file());
        assert!(to.join("shots").join("ScreenShot00000.png").is_file());
        assert!(
            from.join("UserPreferences.ini").is_file(),
            "the source keeps its files"
        );
        assert_eq!(std::fs::read_dir(&from).expect("listed").count(), 3);

        put(
            &to.join("UserPreferences.ini"),
            b"[Net]\r\nUserName=mine\r\n",
        );
        assert_eq!(
            copy_settings_dir(&from, &to).expect("refused, not failed"),
            0
        );
        assert_eq!(
            std::fs::read(to.join("UserPreferences.ini")).expect("still there"),
            b"[Net]\r\nUserName=mine\r\n"
        );
    }

    /// The first run's copy is what the run starts from: the original game's `FullScreen=True`
    /// and resolution reach the loaded configuration, not the defaults read before the copy.
    #[test]
    fn the_first_run_loads_the_settings_it_just_copied() {
        let root =
            std::env::temp_dir().join(format!("dereth-first-run-load-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let original = root.join("Asheron's Call");
        let settings = root.join("Dereth").join("client");
        std::fs::create_dir_all(&original).unwrap();
        std::fs::write(
            original.join(PREFERENCES_FILE_NAME),
            "[Display]
FullScreen=True
",
        )
        .unwrap();
        let prefs = settings.join(PREFERENCES_FILE_NAME);
        let (cfg, copied) = load_config(&[], &prefs, Some(&settings), Some(&original)).unwrap();
        assert!(matches!(copied, Some((_, _, Ok(1)))), "{copied:?}");
        assert!(
            cfg.display.full_screen,
            "the copied FullScreen=True was loaded"
        );
        assert_eq!(cfg.preferences_file, prefs);
        // A second run copies nothing and loads the same file.
        let (again, copied) = load_config(&[], &prefs, Some(&settings), Some(&original)).unwrap();
        assert!(matches!(copied, Some((_, _, Ok(0)))), "{copied:?}");
        assert!(again.display.full_screen);
        let _ = std::fs::remove_dir_all(&root);
    }
}
