//! The launcher updating itself, and with it the Dereth client that ships beside it.
//!
//! At start the launcher reads `latest.json` from the repository's latest GitHub release (the
//! endpoint in `tauri.conf.json`, or `DERETH_UPDATE_URL`), and when it names a newer version for
//! this platform, downloads that release in the background. The download is checked against the
//! update-signing public key in `tauri.conf.json`: an unsigned or wrongly signed file is refused.
//! Once it is in, the page offers a restart.
//!
//! Installing differs by system. On macOS and Linux the updater replaces the app bundle or the
//! AppImage itself. On Windows the release is a zip with no installer, which the updater cannot
//! install, so the launcher does it: the zip is unpacked beside the launcher, its files are swapped
//! in by renaming the running ones aside ([`dereth_launch::swap`]), and the launcher starts again
//! from the new files; the next start removes the old ones. Nothing here touches the launcher's
//! state, which lives in its own folders.
//!
//! Only a launcher that is a release looks for updates: one with the Dereth client beside it. A
//! launcher built from the repository has none there and never checks (unless `DERETH_UPDATE_URL`
//! says to).

use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

/// Where an update stands, as the page shows it.
#[derive(Debug, Clone, Serialize, Default)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UpdateView {
    /// Nothing to show: not checked, not a release, or already current.
    #[default]
    None,
    Downloading {
        version: String,
        done: u64,
        total: Option<u64>,
    },
    /// Downloaded and verified; a restart installs it.
    Ready {
        version: String,
    },
    Failed {
        reason: String,
    },
}

/// The update state the app keeps: what the page sees, and the download waiting to be installed.
#[derive(Default)]
pub struct Updates {
    pub view: Mutex<UpdateView>,
    pending: Mutex<Option<(Update, Vec<u8>)>>,
}

fn set(app: &AppHandle, view: UpdateView) {
    let updates = app.state::<Updates>();
    *updates
        .view
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = view;
}

/// Check for a newer launcher and download it, in the background. `is_release` says whether this
/// launcher is a release (the Dereth client ships beside it).
pub fn check(app: &AppHandle, is_release: bool) {
    let url = std::env::var("DERETH_UPDATE_URL")
        .ok()
        .filter(|u| !u.is_empty());
    if !is_release && url.is_none() {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(reason) = check_and_download(&app, url).await {
            eprintln!("dereth: the update failed: {reason}");
            if let Some(b) = app.try_state::<crate::backend::Shared>() {
                crate::backend::lock(&b).notify_error(format!("Dereth could not update: {reason}"));
            }
            set(&app, UpdateView::Failed { reason });
        }
    });
}

async fn check_and_download(app: &AppHandle, url: Option<String>) -> Result<(), String> {
    let mut builder = app.updater_builder();
    if let Some(url) = url {
        let url = url.parse().map_err(|e| format!("DERETH_UPDATE_URL: {e}"))?;
        builder = builder.endpoints(vec![url]).map_err(|e| e.to_string())?;
    }
    let updater = builder.build().map_err(|e| e.to_string())?;
    // Not reaching GitHub, or a repository with no release yet, is no reason to bother the
    // player: the check is tried again on the next start.
    let update = match updater.check().await {
        Ok(Some(update)) => update,
        Ok(None) => return Ok(()),
        Err(e) => {
            eprintln!("dereth: no update information: {e}");
            return Ok(());
        }
    };
    let version = update.version.clone();
    set(
        app,
        UpdateView::Downloading {
            version: version.clone(),
            done: 0,
            total: None,
        },
    );
    let mut done = 0u64;
    let progress = app.clone();
    let progress_version = version.clone();
    let bytes = update
        .download(
            move |chunk, total| {
                done += chunk as u64;
                set(
                    &progress,
                    UpdateView::Downloading {
                        version: progress_version.clone(),
                        done,
                        total,
                    },
                );
            },
            || {},
        )
        .await
        .map_err(|e| e.to_string())?;
    let updates = app.state::<Updates>();
    *updates
        .pending
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some((update, bytes));
    set(app, UpdateView::Ready { version });
    Ok(())
}

/// Install the downloaded update and start again.
///
/// # Errors
/// Nothing was downloaded, or installing failed.
pub fn install_and_restart(app: &AppHandle) -> Result<(), String> {
    let pending = app
        .state::<Updates>()
        .pending
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take();
    let Some((update, bytes)) = pending else {
        return Err("No update has been downloaded.".into());
    };
    #[cfg(windows)]
    {
        let _ = update;
        let exe = windows::install(&bytes)?;
        std::process::Command::new(&exe)
            .current_dir(exe.parent().unwrap_or(std::path::Path::new(".")))
            .spawn()
            .map_err(|e| format!("the updated launcher could not start: {e}"))?;
        app.exit(0);
        Ok(())
    }
    #[cfg(not(windows))]
    {
        update.install(bytes).map_err(|e| e.to_string())?;
        app.restart()
    }
}

/// The Windows install: the release zip, unpacked and swapped in beside the running launcher.
#[cfg(windows)]
pub mod windows {
    use std::path::{Path, PathBuf};

    use dereth_launch::swap;

    /// The launcher's file name, which a release must hold.
    pub const EXE: &str = "dereth.exe";

    /// Unpack `zip` into the staging folder in `dir`.
    ///
    /// # Errors
    /// The archive is not a zip, names a path outside the folder, or could not be written.
    pub fn unpack(zip: &[u8], dir: &Path) -> Result<PathBuf, String> {
        let staging = dir.join(swap::STAGING_DIR);
        let _ = std::fs::remove_dir_all(&staging);
        std::fs::create_dir_all(&staging).map_err(|e| format!("{}: {e}", staging.display()))?;
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip))
            .map_err(|e| format!("the update is not a zip: {e}"))?;
        for i in 0..archive.len() {
            let mut entry = archive
                .by_index(i)
                .map_err(|e| format!("the update's zip: {e}"))?;
            let Some(rel) = entry.enclosed_name() else {
                return Err(format!(
                    "the update names a path outside its folder: {}",
                    entry.name()
                ));
            };
            let out = staging.join(rel);
            if entry.is_dir() {
                std::fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
                continue;
            }
            if let Some(parent) = out.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("{}: {e}", parent.display()))?;
            }
            let mut file =
                std::fs::File::create(&out).map_err(|e| format!("{}: {e}", out.display()))?;
            std::io::copy(&mut entry, &mut file).map_err(|e| format!("{}: {e}", out.display()))?;
        }
        Ok(staging)
    }

    /// Put the release in `zip` in place of the files in `dir`. Returns the new launcher's path.
    ///
    /// # Errors
    /// Unpacking failed, the archive holds no launcher, or a file could not be swapped (in which
    /// case `dir` is as it was).
    pub fn install_into(zip: &[u8], dir: &Path) -> Result<PathBuf, String> {
        let staging = unpack(zip, dir)?;
        let result = (|| {
            let release = swap::release_root(&staging, EXE)
                .ok_or_else(|| format!("the update holds no {EXE}"))?;
            swap::swap_in(dir, &release)
                .map_err(|e| format!("the update could not replace the launcher's files: {e}"))
        })();
        let _ = std::fs::remove_dir_all(&staging);
        result.map(|_| dir.join(EXE))
    }

    /// [`install_into`] the running launcher's own folder.
    pub fn install(zip: &[u8]) -> Result<PathBuf, String> {
        let exe = std::env::current_exe()
            .map_err(|e| format!("the launcher could not find itself: {e}"))?;
        let dir = exe.parent().ok_or("the launcher is in no folder")?;
        install_into(zip, dir)
    }
}

#[cfg(test)]
mod tests {
    use tauri_plugin_updater::RemoteRelease;

    /// `latest.json` as the release writes it: one entry per platform, each the download and its
    /// signature. The keys are the updater's `<os>-<arch>` targets.
    const LATEST: &str = r#"{
        "version": "0.2.0",
        "notes": "Dereth 0.2.0",
        "pub_date": "2026-10-01T12:00:00Z",
        "platforms": {
            "windows-x86_64": {
                "signature": "c2lnbmF0dXJlIGZvciB3aW5kb3dz",
                "url": "https://github.com/dereth-network/dereth/releases/download/dereth-v0.2.0/dereth-0.2.0-windows-x86_64.zip"
            },
            "darwin-aarch64": {
                "signature": "c2lnbmF0dXJlIGZvciBtYWNPUw==",
                "url": "https://github.com/dereth-network/dereth/releases/download/dereth-v0.2.0/Dereth-0.2.0-macos-aarch64.app.tar.gz"
            },
            "linux-x86_64": {
                "signature": "c2lnbmF0dXJlIGZvciBMaW51eA==",
                "url": "https://github.com/dereth-network/dereth/releases/download/dereth-v0.2.0/Dereth-0.2.0-linux-x86_64.AppImage"
            }
        }
    }"#;

    #[test]
    fn latest_json_reads_as_a_version_and_one_download_per_platform() {
        let release: RemoteRelease = serde_json::from_str(LATEST).unwrap();
        assert_eq!(release.version.to_string(), "0.2.0");
        assert_eq!(release.notes.as_deref(), Some("Dereth 0.2.0"));
        let windows = release.download_url("windows-x86_64").unwrap();
        assert!(
            windows.path().ends_with(".zip"),
            "Windows updates from the release zip: {windows}"
        );
        assert_eq!(
            release.signature("linux-x86_64").unwrap(),
            "c2lnbmF0dXJlIGZvciBMaW51eA=="
        );
        assert!(release
            .download_url("darwin-aarch64")
            .unwrap()
            .path()
            .ends_with(".app.tar.gz"));
        assert!(
            release.download_url("darwin-x86_64").is_err(),
            "a platform the release lacks has no update"
        );
    }

    #[test]
    fn the_configured_endpoint_is_the_repository_latest_release() {
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert_eq!(
            conf["plugins"]["updater"]["endpoints"],
            serde_json::json!([
                "https://github.com/dereth-network/dereth/releases/latest/download/latest.json"
            ])
        );
        assert!(conf["plugins"]["updater"]["pubkey"]
            .as_str()
            .is_some_and(|k| !k.is_empty()));
    }

    /// The layered icon's files exist only where Xcode compiled them, on macOS: the shared
    /// configuration names only the icons every system builds, and the macOS one adds the rest.
    #[test]
    fn only_the_macos_configuration_names_the_icon_files_only_macos_builds() {
        let base: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let icons = base["bundle"]["icon"].as_array().unwrap();
        assert!(icons.iter().all(|i| {
            let i = i.as_str().unwrap();
            i == "icons/icon.png" || i == "icons/icon.ico"
        }));
        assert!(base["bundle"]["macOS"]["files"].is_null());
        let mac: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.macos.conf.json")).unwrap();
        assert_eq!(
            mac["bundle"]["icon"],
            serde_json::json!(["icons/Dereth.icns", "icons/icon.png", "icons/icon.ico"])
        );
        assert_eq!(
            mac["bundle"]["macOS"]["files"]["Resources/Assets.car"],
            "icons/Assets.car"
        );
    }

    #[cfg(windows)]
    #[test]
    fn a_windows_update_zip_is_unpacked_and_swapped_in_beside_the_launcher() {
        use std::io::Write;
        let dir = std::env::temp_dir().join(format!("dereth-update-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("dereth.exe"), "launcher 1").unwrap();
        std::fs::write(dir.join("dereth-client.exe"), "client 1").unwrap();

        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let opts = zip::write::SimpleFileOptions::default();
        for (name, text) in [
            ("dereth-0.2.0/dereth.exe", "launcher 2"),
            ("dereth-0.2.0/dereth-client.exe", "client 2"),
        ] {
            zip.start_file(name, opts).unwrap();
            zip.write_all(text.as_bytes()).unwrap();
        }
        let bytes = zip.finish().unwrap().into_inner();

        let exe = super::windows::install_into(&bytes, &dir).unwrap();
        assert_eq!(exe, dir.join("dereth.exe"));
        assert_eq!(
            std::fs::read_to_string(dir.join("dereth.exe")).unwrap(),
            "launcher 2"
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("dereth-client.exe")).unwrap(),
            "client 2"
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("dereth.exe.dereth-old")).unwrap(),
            "launcher 1"
        );
        assert!(
            !dir.join(dereth_launch::swap::STAGING_DIR).exists(),
            "the staging folder is removed"
        );
        assert_eq!(dereth_launch::swap::clean_up(&dir), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
