//! The launcher's entry in the Linux desktop's application list, the client window's entry, and
//! the icons they name.
//!
//! On every start the launcher makes sure of four files under the user's data folder, each written
//! only when its content differs: its own entry, the client window's hidden entry and their icons. The
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

/// The launcher's icon, 256 pixels square: the network's emblem.
const ICON_PNG: &[u8] = include_bytes!("../assets/icon/network.png");

/// The client's icon, 256 pixels square: the Dereth emblem, the one the client writes itself.
const CLIENT_ICON_PNG: &[u8] = include_bytes!("../../dereth/client/assets/dereth-256.png");

/// Both icons, as the entries name them.
const ICONS: desktop::Icons = desktop::Icons {
    launcher: ICON_PNG,
    client: CLIENT_ICON_PNG,
};

/// The launcher's `.desktop` entry that starts `exec`, named for the window it makes.
pub fn desktop_entry(exec: &Path) -> String {
    desktop::launcher_entry(exec)
}

/// Make sure the launcher's entry, the client's entry (both starting `exec`) and their `icons` are
/// in `data_home`, writing each file only when its content differs; the launcher's entry opens join
/// links when `links` (a release's). Returns the files written.
///
/// # Errors
/// A folder or file could not be written.
pub fn register(
    data_home: &Path,
    exec: &Path,
    icons: desktop::Icons,
    links: bool,
) -> std::io::Result<Vec<PathBuf>> {
    desktop::write_changed(&desktop::launcher_files_with(data_home, exec, icons, links))
}

/// What the entries start: the AppImage the launcher runs from, else the launcher itself.
pub fn launcher_path() -> Option<PathBuf> {
    let me = std::env::current_exe().ok()?;
    Some(desktop::entry_target(&|v| std::env::var_os(v), me))
}

/// On Linux: name the program [`APP_ID`], so the window's application id and class are that, and
/// register the entries and the icon, the launcher's entry opening join links for a release
/// (`links`). A failure is reported and otherwise ignored: the launcher runs without an icon rather
/// than not at all.
#[cfg(target_os = "linux")]
pub fn on_start(links: bool) {
    glib::set_prgname(Some(APP_ID));
    let (Some(home), Some(exec)) = (
        desktop::data_home(&|v| std::env::var_os(v)),
        launcher_path(),
    ) else {
        return;
    };
    if let Err(e) = register(&home, &exec, ICONS, links) {
        eprintln!("dereth: could not register the desktop entries: {e}");
    }
}

/// Elsewhere the system reads the name and the icon from the program itself.
#[cfg(not(target_os = "linux"))]
pub fn on_start(_links: bool) {}

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
        let written = register(&home, exec, ICONS, false).unwrap();
        let client = home.join("applications/network.dereth.dereth-client.desktop");
        assert_eq!(
            written,
            [
                home.join("applications/network.dereth.dereth.desktop"),
                client.clone(),
                home.join("icons/hicolor/256x256/apps/network.dereth.dereth.png"),
                home.join("icons/hicolor/256x256/apps/network.dereth.dereth-client.png"),
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
            client_entry.contains("\nIcon=network.dereth.dereth-client\n"),
            "{client_entry}"
        );
        // The same again writes nothing.
        assert!(register(&home, exec, ICONS, false).unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&home);
    }

    /// The launcher's icon is its own, the network's emblem, and the client keeps the Dereth one.
    #[test]
    fn the_launchers_icon_is_the_networks_emblem_and_the_clients_its_own() {
        assert_eq!(&ICON_PNG[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(&CLIENT_ICON_PNG[..8], b"\x89PNG\r\n\x1a\n");
        assert_ne!(ICON_PNG, CLIENT_ICON_PNG);
    }
}
