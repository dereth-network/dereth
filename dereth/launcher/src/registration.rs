//! The launcher's entry in the Linux desktop's application list, the client window's entry, and
//! the icon both name.
//!
//! On every start the launcher makes sure of three files under the user's data folder, each written
//! only when its content differs: its own entry, the client window's hidden entry and the icon. The
//! entries, where they go and the only-when-different rule are [`dereth_launch::desktop`]'s, which
//! the client uses too when it is started on its own.
//!
//! The launcher window's application id and class are [`APP_ID`]: the program name is set to it
//! before the window system starts, and both derive from it.

use std::path::{Path, PathBuf};

use dereth_launch::desktop;

/// The launcher's application id: the window's Wayland app id and X11 class, the `.desktop` file's
/// name and `StartupWMClass`, and the icon's name.
pub const APP_ID: &str = desktop::LAUNCHER_APP_ID;

/// The icon, 256 pixels square: the Dereth emblem the client uses too.
const ICON_PNG: &[u8] = include_bytes!("../../client/assets/dereth-256.png");

/// The launcher's `.desktop` entry that starts `exec`, named for the window it makes.
pub fn desktop_entry(exec: &Path) -> String {
    desktop::launcher_entry(exec)
}

/// Make sure the launcher's entry, the client's entry (both starting `exec`) and `icon` are in
/// `data_home`, writing each file only when its content differs. Returns the files written.
///
/// # Errors
/// A folder or file could not be written.
pub fn register(data_home: &Path, exec: &Path, icon: &[u8]) -> std::io::Result<Vec<PathBuf>> {
    desktop::write_changed(&desktop::launcher_files(data_home, exec, icon))
}

/// What the entries start: the AppImage the launcher runs from, else the launcher itself.
pub fn launcher_path() -> Option<PathBuf> {
    let me = std::env::current_exe().ok()?;
    Some(desktop::entry_target(&|v| std::env::var_os(v), me))
}

/// On Linux: name the program [`APP_ID`], so the window's application id and class are that, and
/// register the entries and the icon. A failure is reported and otherwise ignored: the launcher
/// runs without an icon rather than not at all.
#[cfg(target_os = "linux")]
pub fn on_start() {
    glib::set_prgname(Some(APP_ID));
    let (Some(home), Some(exec)) = (
        desktop::data_home(&|v| std::env::var_os(v)),
        launcher_path(),
    ) else {
        return;
    };
    if let Err(e) = register(&home, &exec, ICON_PNG) {
        eprintln!("dereth: could not register the desktop entries: {e}");
    }
}

/// Elsewhere the system reads the name and the icon from the program itself.
#[cfg(not(target_os = "linux"))]
pub fn on_start() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dereth-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_launcher_entry_names_the_launcher_window_class() {
        let entry = desktop_entry(Path::new("/apps/Dereth.AppImage"));
        assert!(
            entry.contains("\nStartupWMClass=network.dereth.dereth\n"),
            "{entry}"
        );
        assert!(entry.contains("\nIcon=network.dereth.dereth\n"), "{entry}");
    }

    #[test]
    fn registration_writes_the_launcher_entry_the_hidden_client_entry_and_the_icon_once() {
        let home = temp("registration");
        let exec = Path::new("/apps/Dereth.AppImage");
        let written = register(&home, exec, ICON_PNG).unwrap();
        let client = home.join("applications/network.dereth.dereth-client.desktop");
        assert_eq!(
            written,
            [
                home.join("applications/network.dereth.dereth.desktop"),
                client.clone(),
                home.join("icons/hicolor/256x256/apps/network.dereth.dereth.png"),
            ]
        );
        let client_entry = std::fs::read_to_string(&client).unwrap();
        assert!(
            client_entry.contains("\nNoDisplay=true\n"),
            "{client_entry}"
        );
        assert!(
            client_entry.contains("\nStartupWMClass=network.dereth.dereth-client\n"),
            "{client_entry}"
        );
        assert!(
            client_entry.contains("\nIcon=network.dereth.dereth\n"),
            "{client_entry}"
        );
        // The same again writes nothing.
        assert!(register(&home, exec, ICON_PNG).unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn the_icon_is_a_png() {
        assert_eq!(&ICON_PNG[..8], b"\x89PNG\r\n\x1a\n");
    }
}
