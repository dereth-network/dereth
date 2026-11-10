//! The interface the Dereth client starts in, which the launcher asks for once.
//!
//! The launcher's interface is the default for every world: it keeps it in its own state
//! ([`crate::state::Settings::interface`]) and tells the client it, with the world's name, each
//! time it starts the client ([`args`]). A world starts in the interface the player last chose in
//! game there, else in the default; the client keeps a choice made in game for that world alone,
//! in its preferences file (`[WorldInterface]`), so a world never changed in game follows the
//! default, whatever it becomes. A world kept in the classic interface starts in the default when
//! there are no Classic data files.
//!
//! Until the player has chosen a default the launcher asks, offering the three ([`OFFERED`]) with
//! Horizon first and chosen to begin with ([`FIRST_CHOICE`]). A launcher from before the default
//! was its own wrote the answer into the client's preferences file, as `Interface=` under `[UI]`;
//! that answer becomes the default ([`adopt`]), so such a player is not asked again.
//!
//! # Which file
//!
//! The one the client reads when the launcher starts it ([`preferences_file`]): the folder `DERETH_SETTINGS_DIR` names,
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
use std::path::{Path, PathBuf};

use dereth_client_contract::options::interface::{named_in, NEW_PLAYER};
use dereth_client_contract::persist::preferences::UserPreferences;
use serde::Serialize;

pub use dereth_client_contract::options::interface::Interface;

use crate::folders::System;
use crate::state::LauncherState;

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

/// The interface every world starts in until the player changes it there in game; `None` until
/// the player has chosen one, and the launcher asks.
#[must_use]
pub fn default_of(state: &LauncherState) -> Option<Interface> {
    state.settings.interface.as_deref().and_then(by_name)
}

/// Make `choice` the interface every world starts in until the player changes it there in game.
pub fn set_default(state: &mut LauncherState, choice: Interface) {
    state.settings.interface = Some(choice.label().to_owned());
}

/// Take `saved`, the interface the client's preferences name ([`saved`]), as the default, for a
/// state that has none: the answer an earlier launcher wrote there. True when it was taken.
pub fn adopt(state: &mut LauncherState, saved: Option<Interface>) -> bool {
    match saved {
        Some(choice) if default_of(state).is_none() => {
            set_default(state, choice);
            true
        }
        _ => false,
    }
}

/// What the Dereth client is told of its interface when it is started on the world `slug`: the
/// default, when there is one, and the world's name, which a choice made in game is kept under.
#[must_use]
pub fn args(slug: &str, default: Option<Interface>) -> Vec<String> {
    let mut args = Vec::new();
    if let Some(default) = default {
        args.extend(["--interface".to_owned(), default.label().to_owned()]);
    }
    if !slug.trim().is_empty() {
        args.extend(["--world-slug".to_owned(), slug.to_owned()]);
    }
    args
}

/// Whether the Dereth client whose preferences file is `preferences` refuses the world
/// `world_key`'s data: the world is on its overlay blocklist, the file beside that one. A client
/// with no blocklist refuses none.
#[must_use]
pub fn refuses_world(preferences: &Path, world_key: &str) -> bool {
    use dereth_client_contract::persist::overlay_blocklist;
    let list = preferences.parent().map_or_else(
        || PathBuf::from(overlay_blocklist::FILE),
        |d| d.join(overlay_blocklist::FILE),
    );
    std::fs::read_to_string(list)
        .is_ok_and(|text| overlay_blocklist::parse(&text).contains(world_key))
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
    fn the_launcher_asks_until_it_has_a_default_and_an_answer_an_earlier_one_wrote_is_it() {
        let dir = tmp("interface-asks");
        let path = dir.join("client").join("UserPreferences.ini");
        let mut state = LauncherState::default();
        assert_eq!(saved(&path), None, "no file yet");
        assert!(!adopt(&mut state, saved(&path)));
        assert_eq!(default_of(&state), None, "asks");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            "[Display]
FullScreen=True
",
        )
        .unwrap();
        assert_eq!(saved(&path), None, "the original game's file names none");
        std::fs::write(
            &path,
            "[UI]
Interface=Retail
",
        )
        .unwrap();
        assert_eq!(saved(&path), None, "a word the client does not read");
        assert!(!adopt(&mut state, saved(&path)));
        assert_eq!(default_of(&state), None, "still asks");

        // An earlier launcher's answer, in the client's file, is the default from now on, and
        // the file is not read for it again.
        std::fs::write(
            &path,
            "[UI]
Interface=classic
",
        )
        .unwrap();
        assert!(adopt(&mut state, saved(&path)));
        assert_eq!(default_of(&state), Some(Interface::Classic));
        std::fs::write(
            &path,
            "[UI]
Interface=Modern
",
        )
        .unwrap();
        assert!(!adopt(&mut state, saved(&path)));
        assert_eq!(default_of(&state), Some(Interface::Classic));

        set_default(&mut state, FIRST_CHOICE);
        assert_eq!(default_of(&state), Some(Interface::Horizon));
        let json = serde_json::to_string(&state).unwrap();
        assert!(json.contains("\"interface\":\"Horizon\""), "{json}");
        let read: LauncherState = serde_json::from_str(&json).unwrap();
        assert_eq!(default_of(&read), Some(Interface::Horizon), "kept");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The client's start on world `slug` from the launcher's state, as it reads its command line
    /// and its preferences `ini`: the interface it starts in, and whether the classic interface
    /// fell back.
    fn starts_in(
        state: &LauncherState,
        slug: &str,
        ini: &UserPreferences,
        classic_files: bool,
    ) -> (Interface, bool) {
        use dereth_client_contract::options::interface::world_start;
        let argv = args(slug, default_of(state));
        let cfg = dereth_client_runtime::config::Config::from_args_and_prefs_with(
            &argv,
            &dereth_client_runtime::config::Preferences::default(),
        )
        .unwrap();
        let start = world_start(
            Some(ini),
            cfg.world_slug.as_deref(),
            cfg.interface,
            classic_files,
        );
        (start.interface.unwrap(), start.classic_missing)
    }

    /// A run on `slug` in which the player switched to `to` in game, saved into `ini`.
    fn switch_in_game(state: &LauncherState, slug: &str, ini: &mut UserPreferences, to: Interface) {
        use dereth_client_contract::options::interface::WorldChoice;
        let mut run = WorldChoice::new(slug, starts_in(state, slug, ini, true).0);
        run.shown(run.started);
        run.shown(to);
        assert!(run.keep(ini));
    }

    #[test]
    fn the_default_starts_a_fresh_world_and_a_change_in_game_is_that_worlds_alone() {
        let mut state = LauncherState::default();
        set_default(&mut state, Interface::Horizon);
        let mut ini = UserPreferences::parse(
            "[UI]
Interface=Modern
",
        )
        .unwrap();
        assert_eq!(
            starts_in(&state, "eor", &ini, true),
            (Interface::Horizon, false),
            "a fresh world starts in the default"
        );

        switch_in_game(&state, "ac1999", &mut ini, Interface::Classic);
        assert_eq!(
            starts_in(&state, "ac1999", &ini, true),
            (Interface::Classic, false)
        );
        assert_eq!(
            starts_in(&state, "eor", &ini, true),
            (Interface::Horizon, false),
            "another world is untouched"
        );

        // A new default moves every world but the one with its own choice.
        set_default(&mut state, Interface::Modern);
        assert_eq!(
            starts_in(&state, "eor", &ini, true),
            (Interface::Modern, false)
        );
        assert_eq!(
            starts_in(&state, "ac1999", &ini, true),
            (Interface::Classic, false)
        );
    }

    #[test]
    fn a_world_kept_in_classic_starts_in_the_default_without_classic_files() {
        let mut state = LauncherState::default();
        set_default(&mut state, Interface::Horizon);
        let mut ini = UserPreferences::default();
        switch_in_game(&state, "ac1999", &mut ini, Interface::Classic);
        assert_eq!(
            starts_in(&state, "ac1999", &ini, false),
            (Interface::Horizon, true)
        );
        assert_eq!(
            starts_in(&state, "ac1999", &ini, true),
            (Interface::Classic, false),
            "and in Classic again once the files are back"
        );
    }

    #[test]
    fn the_client_is_told_the_default_only_once_there_is_one_and_always_the_world() {
        assert_eq!(args("eor", None), ["--world-slug", "eor"]);
        assert_eq!(
            args("eor", Some(Interface::Modern)),
            ["--interface", "Modern", "--world-slug", "eor"]
        );
        assert_eq!(
            args(" ", Some(Interface::Modern)),
            ["--interface", "Modern"]
        );
    }
}
