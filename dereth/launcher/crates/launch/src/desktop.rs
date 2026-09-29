//! The Linux desktop's entries for the launcher's window and the client's, and the icon they name.
//!
//! A Linux desktop finds a window's icon and name through a `.desktop` file whose name, or whose
//! `StartupWMClass`, matches the window's application id (on Wayland) or its window class (on
//! X11). A Wayland window has no icon of its own at all, and an AppImage or a tarball installs
//! nothing, so without these files both windows show a generic icon. Three files under the user's
//! data folder (`$XDG_DATA_HOME`, by default `~/.local/share`), each written only when its content
//! differs:
//!
//! - `applications/network.dereth.dereth.desktop`, the launcher's entry in the application list;
//! - `applications/network.dereth.dereth-client.desktop`, the client window's entry, hidden from
//!   the application list (`NoDisplay=true`) because a player starts the client from the launcher;
//! - `icons/hicolor/256x256/apps/network.dereth.dereth.png`, the icon both name.
//!
//! The launcher writes all three on every start; the client, started on its own, writes its entry
//! and the icon. Both compute the client's entry the same way ([`entry_target`]), so a client the
//! launcher started finds its entry already as it would write it and writes nothing.

use std::path::{Path, PathBuf};

/// The launcher's application id: its window's Wayland app id and X11 class, its `.desktop` file's
/// name and `StartupWMClass`, and the icon's name.
pub const LAUNCHER_APP_ID: &str = "network.dereth.dereth";

/// The client's application id: its window's Wayland app id and X11 class, and its `.desktop`
/// file's name and `StartupWMClass`.
pub const CLIENT_APP_ID: &str = "network.dereth.dereth-client";

/// The name both entries give as their `Icon`: one picture, the Dereth emblem.
pub const ICON_NAME: &str = LAUNCHER_APP_ID;

/// A path quoted as a `.desktop` file's `Exec` key takes one argument: in double quotes, with the
/// characters the quoting reserves escaped, then the string escapes applied, and `%` doubled so it
/// is not read as a field code.
fn exec_argument(path: &Path) -> String {
    let mut quoted = String::from("\"");
    for c in path.display().to_string().chars() {
        match c {
            '"' | '`' | '$' => {
                quoted.push_str("\\\\");
                quoted.push(c);
            }
            '\\' => quoted.push_str("\\\\\\\\"),
            '%' => quoted.push_str("%%"),
            _ => quoted.push(c),
        }
    }
    quoted.push('"');
    quoted
}

/// The launcher's `.desktop` entry, starting `exec`.
pub fn launcher_entry(exec: &Path) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName=Dereth\nComment=Worlds, accounts and the Dereth client\n\
         Exec={}\nIcon={ICON_NAME}\nTerminal=false\nCategories=Game;\nStartupWMClass={LAUNCHER_APP_ID}\n",
        exec_argument(exec)
    )
}

/// The client window's `.desktop` entry, starting `exec`. It is there for the window's icon and
/// name, not as a way in, so the application list does not show it.
pub fn client_entry(exec: &Path) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName=Dereth\nComment=The Dereth client\n\
         Exec={}\nIcon={ICON_NAME}\nTerminal=false\nNoDisplay=true\nCategories=Game;\n\
         StartupWMClass={CLIENT_APP_ID}\n",
        exec_argument(exec)
    )
}

/// The user's data folder: `$XDG_DATA_HOME` when it is an absolute path, else `~/.local/share`;
/// `None` when neither is known.
pub fn data_home(var: &dyn Fn(&str) -> Option<std::ffi::OsString>) -> Option<PathBuf> {
    if let Some(dir) = var("XDG_DATA_HOME").map(PathBuf::from) {
        if dir.is_absolute() {
            return Some(dir);
        }
    }
    let home = var("HOME").map(PathBuf::from).filter(|h| h.is_absolute())?;
    Some(home.join(".local/share"))
}

/// Where the entry for `app_id` goes, under `data_home`.
pub fn entry_path(data_home: &Path, app_id: &str) -> PathBuf {
    data_home
        .join("applications")
        .join(format!("{app_id}.desktop"))
}

/// Where the icon goes, under `data_home`.
pub fn icon_path(data_home: &Path) -> PathBuf {
    data_home
        .join("icons/hicolor/256x256/apps")
        .join(format!("{ICON_NAME}.png"))
}

/// What an entry starts: the AppImage this process runs from (`$APPIMAGE`, which a program the
/// launcher starts inherits), else `this_program`. The program inside an AppImage is at a
/// different path on every start, so the AppImage is the only path worth writing down.
pub fn entry_target(
    var: &dyn Fn(&str) -> Option<std::ffi::OsString>,
    this_program: PathBuf,
) -> PathBuf {
    var("APPIMAGE")
        .filter(|v| !v.is_empty())
        .map_or(this_program, PathBuf::from)
}

/// The launcher's three files, as (path, content), under `data_home`: its entry and the client's,
/// both starting `exec`, and the icon.
pub fn launcher_files(data_home: &Path, exec: &Path, icon: &[u8]) -> Vec<(PathBuf, Vec<u8>)> {
    vec![
        (
            entry_path(data_home, LAUNCHER_APP_ID),
            launcher_entry(exec).into_bytes(),
        ),
        (
            entry_path(data_home, CLIENT_APP_ID),
            client_entry(exec).into_bytes(),
        ),
        (icon_path(data_home), icon.to_vec()),
    ]
}

/// The client's two files, as (path, content), under `data_home`: its entry, starting `exec`, and
/// the icon.
pub fn client_files(data_home: &Path, exec: &Path, icon: &[u8]) -> Vec<(PathBuf, Vec<u8>)> {
    vec![
        (
            entry_path(data_home, CLIENT_APP_ID),
            client_entry(exec).into_bytes(),
        ),
        (icon_path(data_home), icon.to_vec()),
    ]
}

/// Make sure each of `files` is on disk with its content, writing only the ones whose content
/// differs (or that are missing). Returns the files written.
///
/// # Errors
/// A folder or file could not be written.
pub fn write_changed(files: &[(PathBuf, Vec<u8>)]) -> std::io::Result<Vec<PathBuf>> {
    let mut written = Vec::new();
    for (path, bytes) in files {
        if std::fs::read(path).is_ok_and(|on_disk| on_disk == *bytes) {
            continue;
        }
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, bytes)?;
        written.push(path.clone());
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ICON: &[u8] = b"\x89PNG\r\n\x1a\nicon";

    fn temp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("dereth-desktop-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn files_under(dir: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                out.extend(files_under(&p));
            } else {
                out.push(p);
            }
        }
        out.sort();
        out
    }

    fn lookup(pairs: Vec<(&'static str, String)>) -> impl Fn(&str) -> Option<std::ffi::OsString> {
        move |k| {
            pairs
                .iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| std::ffi::OsString::from(v))
        }
    }

    #[test]
    fn the_launcher_entry_names_the_window_class_and_the_icon_and_quotes_its_path() {
        let entry = launcher_entry(Path::new("/home/p/My Apps/Dereth.AppImage"));
        assert!(entry.starts_with("[Desktop Entry]\n"));
        assert!(
            entry.contains("\nExec=\"/home/p/My Apps/Dereth.AppImage\"\n"),
            "{entry}"
        );
        assert!(entry.contains("\nName=Dereth\n"));
        assert!(entry.contains("\nIcon=network.dereth.dereth\n"), "{entry}");
        assert!(
            entry.contains("\nStartupWMClass=network.dereth.dereth\n"),
            "{entry}"
        );
        assert!(entry.contains("\nType=Application\n"));
        assert!(!entry.contains("NoDisplay"), "{entry}");
    }

    #[test]
    fn the_client_entry_is_hidden_names_the_client_window_class_and_the_same_icon() {
        let entry = client_entry(Path::new("/opt/dereth/dereth-client"));
        assert!(entry.starts_with("[Desktop Entry]\n"));
        assert!(entry.contains("\nType=Application\n"));
        assert!(entry.contains("\nNoDisplay=true\n"), "{entry}");
        assert!(entry.contains("\nIcon=network.dereth.dereth\n"), "{entry}");
        assert!(
            entry.contains("\nStartupWMClass=network.dereth.dereth-client\n"),
            "{entry}"
        );
        assert!(
            entry.contains("\nExec=\"/opt/dereth/dereth-client\"\n"),
            "{entry}"
        );
    }

    #[test]
    fn reserved_characters_in_the_path_are_escaped() {
        let entry = launcher_entry(Path::new("/opt/100% $x \"y\""));
        assert!(
            entry.contains("\nExec=\"/opt/100%% \\\\$x \\\\\"y\\\\\"\"\n"),
            "{entry}"
        );
    }

    #[test]
    fn the_data_folder_is_xdg_data_home_else_under_home() {
        let root = if cfg!(windows) { "C:/" } else { "/" };
        let xdg = format!("{root}data");
        let home = format!("{root}home/p");
        let both = lookup(vec![("XDG_DATA_HOME", xdg.clone()), ("HOME", home.clone())]);
        assert_eq!(data_home(&both), Some(PathBuf::from(&xdg)));
        let relative = lookup(vec![
            ("XDG_DATA_HOME", "data".into()),
            ("HOME", home.clone()),
        ]);
        assert_eq!(
            data_home(&relative),
            Some(PathBuf::from(&home).join(".local/share"))
        );
        assert_eq!(data_home(&lookup(Vec::new())), None);
    }

    #[test]
    fn an_entry_starts_the_appimage_when_there_is_one_else_the_program() {
        let me = PathBuf::from("/opt/dereth/dereth-client");
        let appimage = lookup(vec![("APPIMAGE", "/apps/Dereth.AppImage".into())]);
        assert_eq!(
            entry_target(&appimage, me.clone()),
            PathBuf::from("/apps/Dereth.AppImage")
        );
        let empty = lookup(vec![("APPIMAGE", String::new())]);
        assert_eq!(entry_target(&empty, me.clone()), me);
        assert_eq!(entry_target(&lookup(Vec::new()), me.clone()), me);
    }

    #[test]
    fn the_launcher_writes_both_entries_and_the_icon_once_and_nothing_else() {
        let home = temp("launcher");
        let exec = Path::new("/apps/Dereth.AppImage");
        let files = launcher_files(&home, exec, ICON);
        let written = write_changed(&files).unwrap();
        let expected = vec![
            home.join("applications/network.dereth.dereth.desktop"),
            home.join("applications/network.dereth.dereth-client.desktop"),
            home.join("icons/hicolor/256x256/apps/network.dereth.dereth.png"),
        ];
        assert_eq!(written, expected);
        assert_eq!(files_under(&home), {
            let mut p = expected.clone();
            p.sort();
            p
        });
        assert_eq!(
            std::fs::read_to_string(&expected[0]).unwrap(),
            launcher_entry(exec)
        );
        assert_eq!(
            std::fs::read_to_string(&expected[1]).unwrap(),
            client_entry(exec)
        );
        assert_eq!(std::fs::read(&expected[2]).unwrap(), ICON);

        // The same again writes nothing.
        assert!(write_changed(&files).unwrap().is_empty());

        // The AppImage moved: only the two entries are rewritten.
        let moved = Path::new("/elsewhere/Dereth.AppImage");
        assert_eq!(
            write_changed(&launcher_files(&home, moved, ICON)).unwrap(),
            expected[..2]
        );
        assert!(std::fs::read_to_string(&expected[1])
            .unwrap()
            .contains("/elsewhere/"));
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn a_client_the_launcher_started_finds_its_files_already_written() {
        let home = temp("client");
        let appimage = Path::new("/apps/Dereth.AppImage");
        write_changed(&launcher_files(&home, appimage, ICON)).unwrap();
        // Inside the AppImage the client's entry target is the AppImage too.
        let env = lookup(vec![("APPIMAGE", appimage.display().to_string())]);
        let target = entry_target(&env, PathBuf::from("/tmp/.mount_x/usr/bin/dereth-client"));
        assert!(write_changed(&client_files(&home, &target, ICON))
            .unwrap()
            .is_empty());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn a_client_on_its_own_writes_its_entry_and_the_icon_and_nothing_else() {
        let home = temp("standalone");
        let exec = Path::new("/opt/dereth/dereth-client");
        let written = write_changed(&client_files(&home, exec, ICON)).unwrap();
        let expected = vec![
            home.join("applications/network.dereth.dereth-client.desktop"),
            home.join("icons/hicolor/256x256/apps/network.dereth.dereth.png"),
        ];
        assert_eq!(written, expected);
        assert_eq!(files_under(&home), {
            let mut p = expected.clone();
            p.sort();
            p
        });
        assert!(write_changed(&client_files(&home, exec, ICON))
            .unwrap()
            .is_empty());
        let _ = std::fs::remove_dir_all(&home);
    }
}
