//! The interface the Dereth client starts in, which the launcher asks for once.
//!
//! The client keeps its interface in its preferences file, `UserPreferences.ini`, as `Interface=`
//! under `[UI]`, and a client started on its own whose file names none starts in the modern
//! interface. While the file the client will read ([`preferences_file`]) names no interface, the
//! launcher asks which to play in, offering the three ([`OFFERED`]) with Horizon first and chosen
//! to begin with ([`FIRST_CHOICE`]).
//! It writes the answer into that file with the client's own reader and writer ([`save`]), every
//! other line kept, so the client starts in it; once the file names one, the launcher never asks
//! again. The player changes it later in the client's options, as before, or from the launcher.
//!
//! # Which file
//!
//! The one the client reads when the launcher starts it: the folder `DERETH_SETTINGS_DIR` names,
//! when it is set (a relative value is taken against the client's folder, where the client runs);
//! else a `UserPreferences.ini` in the client's folder, which makes a portable install; else the
//! client's settings folder:
//!
//! | system | the client's settings folder |
//! |---|---|
//! | Windows | `%APPDATA%\Dereth\client` |
//! | macOS | `~/Library/Application Support/Dereth/client` |
//! | Linux | `$XDG_CONFIG_HOME/dereth/client` (`~/.config`) |
//!
//! `DERETH_STATE_DIR` moves the launcher's own folders ([`crate::folders`]), never the client's.
//! Resolution is a pure function of the system, its environment and the client's folder, so each
//! system's answer is tested on every system.

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

use dereth_client_contract::options::interface::{named_in, write_in, NEW_PLAYER};
use dereth_client_contract::persist::preferences::UserPreferences;
use serde::Serialize;

pub use dereth_client_contract::options::interface::Interface;

use crate::folders::System;

/// The variable that names the client's settings folder outright, which the client reads.
pub const SETTINGS_DIR_VAR: &str = "DERETH_SETTINGS_DIR";

/// The client's preferences file, in its settings folder.
pub const PREFERENCES_FILE: &str = dereth_client_contract::persist::preferences::FILE_NAME;

/// The client's own folder inside the product's; the launcher's is `launcher`.
const CLIENT_DIR: &str = "client";

/// The interfaces the launcher offers, in the order it lists them.
pub const OFFERED: [Interface; 3] = [Interface::Horizon, Interface::Modern, Interface::Classic];

/// The one chosen when the question opens: the one a client started without the launcher starts a
/// new player in.
pub const FIRST_CHOICE: Interface = NEW_PLAYER;

/// What an interface is, in one line.
#[must_use]
pub const fn description(interface: Interface) -> &'static str {
    match interface {
        Interface::Horizon => "Dereth's own interface, with hotbars, a minimap and nameplates.",
        Interface::Modern => {
            "The interface the game had from its 2005 redesign to the end of retail."
        }
        Interface::Classic => {
            "The interface the game had before its 2005 redesign; it needs Classic data files."
        }
    }
}

/// One interface as the launcher's page lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Offer {
    /// Its name, which is also the word the preferences file holds.
    pub name: &'static str,
    /// What it is, in one line.
    pub line: &'static str,
}

/// [`OFFERED`], as the page lists them.
#[must_use]
pub fn offers() -> Vec<Offer> {
    OFFERED
        .iter()
        .map(|&i| Offer {
            name: i.label(),
            line: description(i),
        })
        .collect()
}

/// The offered interface named `name`, in any case.
#[must_use]
pub fn by_name(name: &str) -> Option<Interface> {
    OFFERED
        .into_iter()
        .find(|i| i.label().eq_ignore_ascii_case(name.trim()))
}

/// A variable's value as a path, when it is set and absolute, as the client takes it.
fn absolute(var: &dyn Fn(&str) -> Option<OsString>, name: &str) -> Option<PathBuf> {
    var(name).map(PathBuf::from).filter(|p| p.is_absolute())
}

/// The client's settings folder on `system`, whose environment variables `var` reads, when no
/// variable names it outright. `None` when the environment names no home at all, in which case the
/// client keeps no preferences.
fn client_settings_dir(system: System, var: &dyn Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    let root = match system {
        System::Windows => absolute(var, "APPDATA")
            .or_else(|| absolute(var, "USERPROFILE").map(|h| h.join("AppData").join("Roaming")))?
            .join("Dereth"),
        System::MacOs => absolute(var, "HOME")?
            .join("Library")
            .join("Application Support")
            .join("Dereth"),
        System::Linux => absolute(var, "XDG_CONFIG_HOME")
            .or_else(|| absolute(var, "HOME").map(|h| h.join(".config")))?
            .join("dereth"),
    };
    Some(root.join(CLIENT_DIR))
}

/// The preferences file the Dereth client in `client_dir` reads when the launcher starts it there,
/// on `system` with the environment `var` reads (see the module documentation). `None` when there
/// is none: then the client keeps no preferences, and the launcher does not ask.
#[must_use]
pub fn preferences_file(
    system: System,
    var: &dyn Fn(&str) -> Option<OsString>,
    client_dir: Option<&Path>,
) -> Option<PathBuf> {
    if let Some(dir) = var(SETTINGS_DIR_VAR).filter(|v| !v.is_empty()) {
        let dir = PathBuf::from(dir);
        let dir = client_dir.map_or_else(|| dir.clone(), |c| c.join(&dir));
        return Some(dir.join(PREFERENCES_FILE));
    }
    if let Some(beside) = client_dir
        .map(|c| c.join(PREFERENCES_FILE))
        .filter(|p| p.exists())
    {
        return Some(beside);
    }
    client_settings_dir(system, var).map(|d| d.join(PREFERENCES_FILE))
}

/// The interface the preferences file at `path` names, read as the client reads it. `None` when
/// there is no file, it cannot be read, or it names no interface the client knows: then the
/// launcher asks.
#[must_use]
pub fn saved(path: &Path) -> Option<Interface> {
    let text = std::fs::read_to_string(path).ok()?;
    named_in(&UserPreferences::parse(&text).ok()?)
}

/// Write `choice` into the preferences file at `path` as the client's own save writes it: the
/// file's other lines and line ends kept, or, when there is no file yet, a new one with Windows
/// line ends as the client's first save makes. Atomically: a temporary file beside it, then a
/// rename over it.
///
/// # Errors
/// A file that is there but cannot be read (it is not text, or not readable) is left alone and
/// its error returned, as is any error making the folder or writing.
pub fn save(path: &Path, choice: Interface) -> io::Result<()> {
    let mut ini = match std::fs::read_to_string(path) {
        Ok(text) => UserPreferences::parse(&text).map_err(io::Error::other)?,
        Err(e) if e.kind() == io::ErrorKind::NotFound => UserPreferences {
            crlf: true,
            ..UserPreferences::default()
        },
        Err(e) => return Err(e),
    };
    write_in(&mut ini, choice);
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".launcher");
    let tmp = PathBuf::from(tmp);
    {
        use std::io::Write;
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(ini.to_text().as_bytes())?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the launcher's first-start interface question)
    use super::*;
    use crate::datset::tests::tmp;

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

    /// A path that is absolute on the host running the test, whatever that host is.
    fn abs(tail: &str) -> String {
        let base = if cfg!(windows) { "C:\\home" } else { "/home" };
        format!("{base}{}{tail}", std::path::MAIN_SEPARATOR)
    }

    #[test]
    fn horizon_is_offered_first_and_chosen_to_begin_with_as_a_new_player_starts_in_it() {
        assert_eq!(
            offers().iter().map(|o| o.name).collect::<Vec<_>>(),
            ["Horizon", "Modern", "Classic"]
        );
        assert_eq!(FIRST_CHOICE, Interface::Horizon);
        assert_eq!(FIRST_CHOICE, OFFERED[0]);
        for o in offers() {
            assert!(!o.line.is_empty() && !o.line.contains('\n'), "{o:?}");
            assert_eq!(
                by_name(&o.name.to_ascii_lowercase()).map(Interface::label),
                Some(o.name)
            );
        }
        assert_eq!(by_name("Retail"), None);
    }

    #[test]
    fn the_file_is_the_clients_settings_folders_on_each_system() {
        let roaming = abs("roaming");
        let user = abs("user");
        let xdg = abs("xdg");
        let file = |root: &str, parts: &[&str]| {
            parts
                .iter()
                .fold(PathBuf::from(root), |p, part| p.join(part))
                .join("UserPreferences.ini")
        };
        let at = |system, pairs: &[(&str, &str)]| preferences_file(system, &env(pairs), None);

        assert_eq!(
            at(
                System::Windows,
                &[("APPDATA", &roaming), ("USERPROFILE", &user)]
            ),
            Some(file(&roaming, &["Dereth", "client"]))
        );
        assert_eq!(
            at(System::Windows, &[("USERPROFILE", &user)]),
            Some(file(&user, &["AppData", "Roaming", "Dereth", "client"]))
        );
        assert_eq!(
            at(System::MacOs, &[("HOME", &user)]),
            Some(file(
                &user,
                &["Library", "Application Support", "Dereth", "client"]
            ))
        );
        assert_eq!(
            at(System::Linux, &[("HOME", &user)]),
            Some(file(&user, &[".config", "dereth", "client"]))
        );
        assert_eq!(
            at(System::Linux, &[("HOME", &user), ("XDG_CONFIG_HOME", &xdg)]),
            Some(file(&xdg, &["dereth", "client"]))
        );
        assert_eq!(
            at(
                System::Linux,
                &[("HOME", &user), ("XDG_CONFIG_HOME", "relative")]
            ),
            Some(file(&user, &[".config", "dereth", "client"])),
            "a relative value is ignored, as the client ignores it"
        );
        for system in [System::Windows, System::MacOs, System::Linux] {
            assert_eq!(at(system, &[]), None, "no home at all");
            assert_eq!(
                at(
                    system,
                    &[
                        ("HOME", &user),
                        ("APPDATA", &roaming),
                        ("DERETH_STATE_DIR", &xdg)
                    ]
                )
                .and_then(|p| p.parent().map(Path::to_path_buf))
                .and_then(|p| p.file_name().map(ToOwned::to_owned)),
                Some(OsString::from("client")),
                "the launcher's own folder variable does not move the client's"
            );
        }
    }

    #[test]
    fn the_settings_variable_and_a_file_beside_the_client_win_as_they_do_for_the_client() {
        let user = abs("user");
        let named = abs("named");
        let client = tmp("interface-portable");
        std::fs::create_dir_all(&client).unwrap();
        let home = [("APPDATA", user.as_str()), ("HOME", user.as_str())];
        let settings = preferences_file(System::HOST, &env(&home), Some(&client)).unwrap();
        assert!(settings.starts_with(&user), "{}", settings.display());

        // A file beside the client is a portable install.
        std::fs::write(client.join("UserPreferences.ini"), "[UI]\n").unwrap();
        assert_eq!(
            preferences_file(System::HOST, &env(&home), Some(&client)),
            Some(client.join("UserPreferences.ini"))
        );

        // The variable wins over both; a relative one is the client's folder's.
        let with = |dir: &str| {
            let mut pairs = home.to_vec();
            pairs.push(("DERETH_SETTINGS_DIR", dir));
            preferences_file(System::HOST, &env(&pairs), Some(&client))
        };
        assert_eq!(
            with(&named),
            Some(PathBuf::from(&named).join("UserPreferences.ini"))
        );
        assert_eq!(
            with("mine"),
            Some(client.join("mine").join("UserPreferences.ini"))
        );
        assert_eq!(
            with(""),
            Some(client.join("UserPreferences.ini")),
            "an empty one is unset"
        );
        let _ = std::fs::remove_dir_all(client);
    }

    #[test]
    fn the_launcher_asks_while_no_interface_is_saved_and_never_once_one_is() {
        let dir = tmp("interface-asks");
        let path = dir.join("client").join("UserPreferences.ini");
        assert_eq!(saved(&path), None, "no file yet: asks");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "[Display]\r\nFullScreen=True\r\n").unwrap();
        assert_eq!(
            saved(&path),
            None,
            "the original game's file names none: asks"
        );
        std::fs::write(&path, "[UI]\nInterface=Retail\n").unwrap();
        assert_eq!(saved(&path), None, "a word the client does not read: asks");
        std::fs::write(&path, "[UI]\nInterface=classic\n").unwrap();
        assert_eq!(
            saved(&path),
            Some(Interface::Classic),
            "one is saved: never asks"
        );

        std::fs::remove_file(&path).unwrap();
        save(&path, FIRST_CHOICE).unwrap();
        assert_eq!(
            saved(&path),
            Some(Interface::Horizon),
            "the answer is saved"
        );
        save(&path, Interface::Modern).unwrap();
        assert_eq!(saved(&path), Some(Interface::Modern), "and changed");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_choice_is_written_as_the_client_reads_it_and_the_rest_of_the_file_is_kept() {
        use dereth_client_contract::options::{interface, store};
        let dir = tmp("interface-writes");
        let path = dir.join("client").join("UserPreferences.ini");
        // The client's start, as it reads its file: the store loaded from it, then a new player
        // started in Horizon unless the file names an interface.
        let client_starts_in = |path: &Path| {
            store::init();
            let text = std::fs::read_to_string(path).unwrap();
            let ini = UserPreferences::parse(&text).unwrap();
            store::load(&ini);
            interface::start_new_player(Some(&ini), NEW_PLAYER);
            Interface::chosen()
        };

        for choice in OFFERED {
            let _ = std::fs::remove_dir_all(&dir);
            save(&path, choice).unwrap();
            assert_eq!(
                std::fs::read_to_string(&path).unwrap(),
                format!("[UI]\r\nInterface={}\r\n", choice.label()),
                "a new file, as the client's first save writes one"
            );
            assert_eq!(client_starts_in(&path), choice);
        }

        let file = "[Display]\nFullScreen=True\n\n; a note\n[UI]\nChatFontSize=12\n[Net]\nUserName=keep-me\n";
        std::fs::write(&path, file).unwrap();
        save(&path, Interface::Classic).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "[Display]\nFullScreen=True\n\n; a note\n[UI]\nChatFontSize=12\nInterface=Classic\n[Net]\nUserName=keep-me\n"
        );
        assert_eq!(client_starts_in(&path), Interface::Classic);
        assert!(
            !dir.join("client")
                .join("UserPreferences.ini.launcher")
                .exists(),
            "the temporary file is renamed away"
        );

        // A file that is not text is not the launcher's to replace.
        std::fs::write(&path, [0xff, 0xfe, 0x00, b'[']).unwrap();
        assert!(save(&path, Interface::Modern).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), [0xff, 0xfe, 0x00, b'[']);
        let _ = std::fs::remove_dir_all(dir);
    }
}
