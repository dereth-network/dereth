//! Where the launcher keeps what it knows.
//!
//! Nothing is kept beside the launcher: a release is a folder (or an app, or an AppImage) the
//! player may put anywhere, replace or delete. There are two folders instead, one for settings and
//! one for large, rebuildable data:
//!
//! | system | settings: [`STATE_FILE`](crate::state::STATE_FILE) | data: the private dat sets, the web view's cache |
//! |---|---|---|
//! | Windows | `%APPDATA%\Dereth\launcher` | `%LOCALAPPDATA%\Dereth\launcher` |
//! | macOS | `~/Library/Application Support/Dereth/launcher` | the same folder |
//! | Linux | `$XDG_CONFIG_HOME/dereth/launcher` (`~/.config`) | `$XDG_DATA_HOME/dereth/launcher` (`~/.local/share`) |
//!
//! The client keeps its own files beside these, in `Dereth/client` (`dereth/client` on Linux).
//! The launcher writes only inside its own `launcher` folders: anything else in `Dereth` (or
//! `dereth`) is the client's. `DERETH_STATE_DIR` puts both of the launcher's folders in the one
//! directory it names.
//!
//! Resolution ([`resolve`]) is a pure function of the system and its environment variables, so
//! each system's answer is tested on every system.

use std::ffi::OsString;
use std::path::PathBuf;

/// The variable that puts both folders in one named directory, for development and tests.
pub const STATE_DIR_VAR: &str = "DERETH_STATE_DIR";

/// The folder, under each system's settings and data folders, that is the Dereth product's.
const PRODUCT_DIR: &str = "Dereth";

/// The same on Linux, where the folders under `~/.config` and `~/.local/share` are lower case.
const PRODUCT_DIR_LINUX: &str = "dereth";

/// The launcher's own folder inside the product's; the client's is `client`.
const LAUNCHER_DIR: &str = "launcher";

/// The private dat sets' folder inside the data folder.
pub const LIBRARY_DIR: &str = "library";

/// A system, for resolving its folders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum System {
    Windows,
    MacOs,
    /// Linux and the other Unix desktops, which follow the XDG base directories.
    Linux,
}

impl System {
    /// The system this was built for.
    pub const HOST: System = if cfg!(windows) {
        System::Windows
    } else if cfg!(target_os = "macos") {
        System::MacOs
    } else {
        System::Linux
    };
}

/// The launcher's two folders.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Folders {
    /// Settings: the state file (the library, accounts, favourites, recent combinations).
    pub config: PathBuf,
    /// Large or rebuildable data: the private dat sets, the web view's cache.
    pub data: PathBuf,
}

impl Folders {
    /// Both folders in one directory.
    pub fn single(dir: impl Into<PathBuf>) -> Self {
        let dir = dir.into();
        Self {
            config: dir.clone(),
            data: dir,
        }
    }

    /// Where private dat sets go unless the state says otherwise.
    pub fn library(&self) -> PathBuf {
        self.data.join(LIBRARY_DIR)
    }

    /// Where the web view keeps its cache.
    pub fn webview(&self) -> PathBuf {
        self.data.join("webview")
    }
}

/// A variable's value when it is set and not empty.
fn var_of(var: &dyn Fn(&str) -> Option<OsString>, name: &str) -> Option<PathBuf> {
    var(name).filter(|v| !v.is_empty()).map(PathBuf::from)
}

/// An XDG base directory: the variable when it holds an absolute path (the specification says to
/// ignore any other value), else `$HOME` joined with the default.
fn xdg(var: &dyn Fn(&str) -> Option<OsString>, name: &str, default: &str) -> Option<PathBuf> {
    var_of(var, name)
        .filter(|p| p.has_root())
        .or_else(|| var_of(var, "HOME").map(|h| h.join(default)))
}

/// A Windows known folder by its variable, else under `%USERPROFILE%`.
fn windows_dir(
    var: &dyn Fn(&str) -> Option<OsString>,
    name: &str,
    under_profile: &str,
) -> Option<PathBuf> {
    var_of(var, name).or_else(|| var_of(var, "USERPROFILE").map(|p| p.join(under_profile)))
}

/// The launcher's folders on `system`, whose environment variables `var` reads. `None` when the
/// environment names no home folder at all.
pub fn resolve(system: System, var: &dyn Fn(&str) -> Option<OsString>) -> Option<Folders> {
    if let Some(dir) = var_of(var, STATE_DIR_VAR) {
        return Some(Folders::single(dir));
    }
    match system {
        System::Windows => Some(Folders {
            config: windows_dir(var, "APPDATA", r"AppData\Roaming")?
                .join(PRODUCT_DIR)
                .join(LAUNCHER_DIR),
            data: windows_dir(var, "LOCALAPPDATA", r"AppData\Local")?
                .join(PRODUCT_DIR)
                .join(LAUNCHER_DIR),
        }),
        System::MacOs => {
            let dir = var_of(var, "HOME")?
                .join("Library/Application Support")
                .join(PRODUCT_DIR)
                .join(LAUNCHER_DIR);
            Some(Folders::single(dir))
        }
        System::Linux => Some(Folders {
            config: xdg(var, "XDG_CONFIG_HOME", ".config")?
                .join(PRODUCT_DIR_LINUX)
                .join(LAUNCHER_DIR),
            data: xdg(var, "XDG_DATA_HOME", ".local/share")?
                .join(PRODUCT_DIR_LINUX)
                .join(LAUNCHER_DIR),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> {
        let pairs: Vec<(String, String)> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        move |name| {
            pairs
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| OsString::from(v))
        }
    }

    #[test]
    fn windows_keeps_settings_in_roaming_app_data_and_the_data_sets_in_local() {
        let e = env(&[
            ("APPDATA", r"C:\Users\p\AppData\Roaming"),
            ("LOCALAPPDATA", r"C:\Users\p\AppData\Local"),
        ]);
        let f = resolve(System::Windows, &e).unwrap();
        assert_eq!(
            f.config,
            Path::new(r"C:\Users\p\AppData\Roaming")
                .join("Dereth")
                .join("launcher")
        );
        assert_eq!(
            f.data,
            Path::new(r"C:\Users\p\AppData\Local")
                .join("Dereth")
                .join("launcher")
        );
        assert_eq!(f.library(), f.data.join("library"));
    }

    #[test]
    fn windows_without_the_app_data_variables_falls_back_to_the_profile() {
        let e = env(&[("USERPROFILE", r"C:\Users\p")]);
        let f = resolve(System::Windows, &e).unwrap();
        assert_eq!(
            f.config,
            Path::new(r"C:\Users\p")
                .join(r"AppData\Roaming")
                .join("Dereth")
                .join("launcher")
        );
        assert_eq!(
            f.data,
            Path::new(r"C:\Users\p")
                .join(r"AppData\Local")
                .join("Dereth")
                .join("launcher")
        );
        assert_eq!(resolve(System::Windows, &env(&[])), None, "no home at all");
    }

    #[test]
    fn macos_keeps_everything_in_application_support() {
        let e = env(&[("HOME", "/Users/p")]);
        let f = resolve(System::MacOs, &e).unwrap();
        let want = Path::new("/Users/p")
            .join("Library/Application Support")
            .join("Dereth")
            .join("launcher");
        assert_eq!(f, Folders::single(want));
    }

    #[test]
    fn linux_follows_the_xdg_folders_and_their_defaults() {
        let e = env(&[("HOME", "/home/p")]);
        let f = resolve(System::Linux, &e).unwrap();
        assert_eq!(
            f.config,
            Path::new("/home/p")
                .join(".config")
                .join("dereth")
                .join("launcher")
        );
        assert_eq!(
            f.data,
            Path::new("/home/p")
                .join(".local/share")
                .join("dereth")
                .join("launcher")
        );

        let e = env(&[
            ("HOME", "/home/p"),
            ("XDG_CONFIG_HOME", "/cfg"),
            ("XDG_DATA_HOME", "/data"),
        ]);
        let f = resolve(System::Linux, &e).unwrap();
        assert_eq!(f.config, Path::new("/cfg").join("dereth").join("launcher"));
        assert_eq!(f.data, Path::new("/data").join("dereth").join("launcher"));

        let e = env(&[
            ("HOME", "/home/p"),
            ("XDG_CONFIG_HOME", "relative"),
            ("XDG_DATA_HOME", ""),
        ]);
        let f = resolve(System::Linux, &e).unwrap();
        assert_eq!(
            f.config,
            Path::new("/home/p")
                .join(".config")
                .join("dereth")
                .join("launcher"),
            "a relative value is ignored"
        );
        assert_eq!(
            f.data,
            Path::new("/home/p")
                .join(".local/share")
                .join("dereth")
                .join("launcher"),
            "an empty one too"
        );
    }

    #[test]
    fn the_state_dir_variable_puts_both_folders_in_one_place() {
        for system in [System::Windows, System::MacOs, System::Linux] {
            let e = env(&[
                ("DERETH_STATE_DIR", "/somewhere"),
                ("HOME", "/home/p"),
                ("LOCALAPPDATA", r"C:\L"),
            ]);
            assert_eq!(resolve(system, &e).unwrap(), Folders::single("/somewhere"));
        }
    }
}
