//! "Create desktop shortcut", the button beside the launcher's version under the rail.
//!
//! A release has no installer to make one, so the launcher offers it itself: a `.lnk` to itself on
//! Windows, a `.desktop` entry on Linux (pointing at the AppImage when it runs from one, since the
//! executable inside it is at a different path on every start). Pressing it again replaces the
//! shortcut. macOS has no such thing to offer: the player keeps `Dereth.app` in the Dock or in
//! Applications.

use std::path::{Path, PathBuf};

/// Whether this system has a desktop shortcut to offer.
pub const OFFERED: bool = cfg!(any(windows, target_os = "linux"));

/// What the shortcut starts: the launcher as the player sees it.
fn target() -> Result<PathBuf, String> {
    #[cfg(target_os = "linux")]
    if let Some(appimage) = std::env::var_os("APPIMAGE").filter(|v| !v.is_empty()) {
        return Ok(PathBuf::from(appimage));
    }
    std::env::current_exe().map_err(|e| format!("the launcher could not find itself: {e}"))
}

/// Make the shortcut in `desktop`, replacing one made before. Returns its path.
///
/// # Errors
/// This system has none to offer, or it could not be written.
pub fn create(desktop: &Path) -> Result<PathBuf, String> {
    let target = target()?;
    write(desktop, &target)
}

#[cfg(windows)]
fn write(desktop: &Path, target: &Path) -> Result<PathBuf, String> {
    let path = desktop.join("Dereth.lnk");
    let mut link = mslnk::ShellLink::new(target).map_err(|e| format!("the shortcut: {e}"))?;
    if let Some(dir) = target.parent() {
        link.set_working_dir(Some(dir.display().to_string()));
    }
    link.set_icon_location(Some(target.display().to_string()));
    link.create_lnk(&path)
        .map_err(|e| format!("could not write {}: {e}", path.display()))?;
    Ok(path)
}

#[cfg(target_os = "linux")]
fn write(desktop: &Path, target: &Path) -> Result<PathBuf, String> {
    use std::os::unix::fs::PermissionsExt;
    let path = desktop.join("dereth.desktop");
    std::fs::write(&path, crate::registration::desktop_entry(target))
        .map_err(|e| format!("could not write {}: {e}", path.display()))?;
    // Desktops start only an entry that is executable.
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
        .map_err(|e| format!("could not mark {} executable: {e}", path.display()))?;
    Ok(path)
}

#[cfg(not(any(windows, target_os = "linux")))]
fn write(_desktop: &Path, _target: &Path) -> Result<PathBuf, String> {
    Err("This system has no desktop shortcuts to make.".into())
}

#[cfg(test)]
mod tests {
    #[cfg(windows)]
    use super::*;

    /// Pressing the button again replaces the shortcut rather than failing on the one there.
    #[cfg(any(windows, target_os = "linux"))]
    #[test]
    fn a_second_shortcut_replaces_the_first() {
        let dir =
            std::env::temp_dir().join(format!("dereth-shortcut-again-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let exe = std::env::current_exe().unwrap();
        let first = super::write(&dir, &exe).unwrap();
        let second = super::write(&dir, &exe).unwrap();
        assert_eq!(first, second);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    fn a_windows_shortcut_is_a_link_file_to_the_launcher() {
        let dir = std::env::temp_dir().join(format!("dereth-shortcut-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = write(&dir, &std::env::current_exe().unwrap()).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        // A shell link starts with its header size, 0x4C, and the link class id.
        assert_eq!(&bytes[..4], &[0x4C, 0, 0, 0]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
