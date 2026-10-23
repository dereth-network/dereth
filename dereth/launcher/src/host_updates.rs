//! A server's updates: which newer Empyrean release it may take, downloading and checking it, and
//! handing it to Empyrean to install at the next stop or start.
//!
//! A newer release is found in the repository's releases ([`components::release_list`]). Its
//! `release.json`, checked against the release's `SHA256SUMS`, says what upgrading to it involves.
//! A patch of the same minor version is downloaded by itself; any other release waits for the
//! person to approve that exact version. Empyrean then backs up the databases, installs, trial
//! starts the new server and restores everything when that fails.

use crate::{components, host_runtime};
use dereth_launch::components::{self as metadata, Product, Release};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct View {
    pub version: Option<String>,
    pub notes: String,
    pub pending: bool,
    pub message: Option<String>,
    pub rollback: bool,
    #[serde(default)]
    pub skipped_version: Option<String>,
    /// The version the last check found to be the newest there is, when it found no update.
    #[serde(default)]
    pub latest: Option<String>,
}

/// A newer release this server may update to, with the files that say so.
#[derive(Serialize, Deserialize)]
struct Offer {
    release: Release,
    /// The release's `release.json`, as downloaded.
    index: Vec<u8>,
    /// The release's `SHA256SUMS`, as downloaded.
    sums: String,
    automatic: bool,
}

pub fn view(dir: &Path) -> View {
    std::fs::read(dir.join("update.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn save(dir: &Path, view: &View) -> Result<(), String> {
    host_runtime::write_json(&dir.join("update.json"), view)
}

pub fn control(
    dir: &Path,
    action: &str,
    extra: &[&std::ffi::OsStr],
) -> Result<serde_json::Value, String> {
    let definition = host_runtime::read_definition(dir)?;
    let output = components::command(&definition.program)
        .args(["managed-update", action, "--config"])
        .arg(dir.join("empyrean.toml"))
        .args(extra)
        .current_dir(dir)
        .output()
        .map_err(|e| e.to_string())?;
    let response = String::from_utf8_lossy(&output.stdout)
        .lines()
        .rev()
        .find_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .ok_or_else(|| {
            "This world's Empyrean is too old to update from the launcher. Make a new world."
                .to_owned()
        })?;
    if !output.status.success() || response["ok"] != true {
        return Err(response["error"]
            .as_str()
            .unwrap_or("The update did not complete.")
            .into());
    }
    Ok(response["result"].clone())
}

fn sync_version(dir: &Path) -> Result<(), String> {
    let mut definition = host_runtime::read_definition(dir)?;
    let output = components::command(&definition.program)
        .arg("--release-facts")
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err("Empyrean could not say its version.".into());
    }
    let facts: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|e| e.to_string())?;
    let version = facts["version"]
        .as_str()
        .filter(|v| metadata::version(v).is_some())
        .ok_or("Empyrean said an invalid version.")?;
    definition.version = version.into();
    host_runtime::write_json(&dir.join("host.json"), &definition)
}

/// Look for a newer release now. A patch downloads at once; anything else waits for approval.
pub fn check(dir: &Path) -> Result<(), String> {
    let result = check_offer(dir);
    if let Err(error) = &result {
        let mut state = view(dir);
        state.message = Some(error.clone());
        save(dir, &state)?;
    }
    result
}

/// A release's `SHA256SUMS` and `release.json`, the second checked against the first.
fn release_files(release: &Release) -> Result<(String, Vec<u8>), String> {
    let sums = String::from_utf8(components::download(
        release.asset(metadata::SUMS_ASSET)?,
        1024 * 1024,
        &mut |_, _| {},
    )?)
    .map_err(|_| "The release's checksums could not be read.")?;
    let index = components::download(
        release.asset(metadata::FACTS_ASSET)?,
        1024 * 1024,
        &mut |_, _| {},
    )?;
    metadata::check_sum(metadata::FACTS_ASSET, &index, &metadata::parse_sums(&sums))?;
    Ok((sums, index))
}

fn check_offer(dir: &Path) -> Result<(), String> {
    let old = view(dir);
    if old.pending {
        return Ok(());
    }
    let definition = host_runtime::read_definition(dir)?;
    if definition.checkout {
        return save(
            dir,
            &View {
                message: Some("This world runs this checkout's build.".into()),
                ..Default::default()
            },
        );
    }
    let current = metadata::version(&definition.version).ok_or("Invalid Empyrean version")?;
    let mut releases: Vec<_> =
        metadata::releases(&components::release_list(false)?, Product::Empyrean)?
            .into_iter()
            .filter(|r| metadata::version(&r.version).is_some_and(|v| v > current))
            .collect();
    // Take the newest patch first; otherwise offer the next declared upgrade step.
    releases.sort_by_key(|r| {
        let version = metadata::version(&r.version).expect("filtered version");
        let patch = version.major == current.major && version.minor == current.minor;
        (
            if patch { 0 } else { 1 },
            if patch {
                std::cmp::Reverse(version)
            } else {
                std::cmp::Reverse(current)
            },
        )
    });
    for release in releases {
        let (sums, index) = release_files(&release)?;
        let declaration: serde_json::Value =
            serde_json::from_slice(&index).map_err(|e| e.to_string())?;
        if declaration["version"] != release.version {
            return Err("The release's release.json names another version.".into());
        }
        let version = metadata::version(&release.version).ok_or("Invalid release version")?;
        if declaration["upgrade"]["upgrades_from"]
            .as_str()
            .and_then(metadata::version)
            .is_none_or(|minimum| minimum > current)
        {
            continue;
        }
        let automatic = version.major == current.major
            && version.minor == current.minor
            && declaration["upgrade"]["kind"] == "patch";
        if !automatic && declaration["upgrade"]["previous"] != definition.version {
            continue;
        }
        let offered = release.version.clone();
        let notes = release.notes.clone();
        let download_now = automatic && old.skipped_version.as_deref() != Some(offered.as_str());
        host_runtime::write_json(
            &dir.join("update-offer.json"),
            &Offer {
                release,
                index,
                sums,
                automatic,
            },
        )?;
        save(
            dir,
            &View {
                version: Some(offered.clone()),
                notes,
                message: Some(if download_now {
                    format!("Downloading Empyrean {offered}...")
                } else {
                    format!("Empyrean {offered} is out. Read what changed, then update.")
                }),
                rollback: old.rollback,
                skipped_version: old.skipped_version.clone(),
                ..Default::default()
            },
        )?;
        return if download_now {
            stage(dir, None)
        } else {
            Ok(())
        };
    }
    save(
        dir,
        &View {
            message: Some(format!("Empyrean {} is the newest.", definition.version)),
            latest: Some(definition.version.clone()),
            rollback: old.rollback,
            skipped_version: old.skipped_version,
            ..Default::default()
        },
    )
}

/// The SHA-256 `release.json` gives this system's file.
fn declared_sum(index: &[u8], target: &str) -> Option<String> {
    let index: serde_json::Value = serde_json::from_slice(index).ok()?;
    index["assets"]
        .as_array()?
        .iter()
        .find(|a| a["target"] == target)?["sha256"]
        .as_str()
        .map(str::to_ascii_lowercase)
}

/// Download the offered release, check it and hand it to Empyrean, which prepares it for the next
/// stop or start. `approved` is the version the person approved, for an update that needs it.
pub fn stage(dir: &Path, approved: Option<&str>) -> Result<(), String> {
    let offer: Offer = serde_json::from_slice(
        &std::fs::read(dir.join("update-offer.json"))
            .map_err(|_| "Check for updates again first.".to_owned())?,
    )
    .map_err(|_| "Check for updates again first.".to_owned())?;
    let version = offer.release.version.clone();
    if !offer.automatic && approved != Some(version.as_str()) {
        return Err("Approve this exact release first.".into());
    }
    let sums = metadata::parse_sums(&offer.sums);
    metadata::check_sum(metadata::FACTS_ASSET, &offer.index, &sums)?;
    let (name, url) = offer
        .release
        .archive(Product::Empyrean, components::target())?;
    let bytes = components::download(url, metadata::MAX_ARCHIVE_BYTES, &mut |_, _| {})?;
    metadata::check_sum(&name, &bytes, &sums)?;
    if declared_sum(&offer.index, components::target())
        .is_some_and(|sum| sum != metadata::sha256_hex(&bytes))
    {
        return Err(format!(
            "{name} is not the file the release's release.json describes."
        ));
    }
    let folder = dir.join("update-download");
    if folder.exists() {
        if std::fs::symlink_metadata(&folder)
            .map_err(|e| e.to_string())?
            .file_type()
            .is_symlink()
        {
            return Err("The download folder must not be a link.".into());
        }
        std::fs::remove_dir_all(&folder).map_err(|e| e.to_string())?;
    }
    std::fs::create_dir(&folder).map_err(|e| e.to_string())?;
    crate::component_archive::extract(&bytes, &name, &folder)?;
    let exe = components::find_executable(&folder, Product::Empyrean)?;
    let candidate = exe.parent().ok_or("Missing candidate folder")?;
    let index_path = folder.join("checked-release.json");
    std::fs::write(&index_path, &offer.index).map_err(|e| e.to_string())?;
    let mut extra = vec![
        std::ffi::OsStr::new("--candidate"),
        candidate.as_os_str(),
        std::ffi::OsStr::new("--index"),
        index_path.as_os_str(),
    ];
    if let Some(version) = approved {
        extra.extend([
            std::ffi::OsStr::new("--approve"),
            std::ffi::OsStr::new(version),
        ]);
    }
    control(dir, "stage", &extra)?;
    let mut state = view(dir);
    state.pending = true;
    state.skipped_version = None;
    state.message = Some(format!(
        "Empyrean {version} is downloaded. It goes in the next time this world stops or starts."
    ));
    save(dir, &state)
}

// Restore the control executable first: an interrupted copy may have left it unlaunchable.
// The host lock is held and the journal remains unfinished until Empyrean restores everything.
fn recover_program(program: &Path, record: &serde_json::Value) -> Result<(), String> {
    let files = record["files"].as_array().ok_or("Missing recovery files")?;
    let backup = files
        .iter()
        .find_map(|entry| {
            let target = Path::new(entry[0].as_str()?);
            (target == program).then(|| entry[1].as_str()).flatten()
        })
        .ok_or("Missing Empyrean's recovery backup")?;
    let backup = Path::new(backup);
    let root = program
        .parent()
        .ok_or("Missing runtime folder")?
        .join(".empyrean-update")
        .canonicalize()
        .map_err(|e| e.to_string())?;
    if !backup
        .canonicalize()
        .map_err(|e| e.to_string())?
        .starts_with(root)
    {
        return Err("Empyrean's backup is outside the recovery folder.".into());
    }
    let expected = record["hashes"]
        .as_array()
        .and_then(|hashes| {
            hashes.iter().find_map(|entry| {
                (Path::new(entry[0].as_str()?) == backup)
                    .then(|| entry[1].as_str())
                    .flatten()
            })
        })
        .ok_or("Missing the checksum of Empyrean's backup")?;
    let size = std::fs::metadata(backup).map_err(|e| e.to_string())?.len();
    if size > 256 * 1024 * 1024 {
        return Err("Empyrean's backup is too large.".into());
    }
    let bytes = std::fs::read(backup).map_err(|e| e.to_string())?;
    if metadata::sha256_hex(&bytes) != expected.to_ascii_lowercase() {
        return Err("Empyrean's backup is damaged.".into());
    }
    let temp = program.with_extension("recovery-tmp");
    // copy preserves executable permissions on Unix.
    std::fs::copy(backup, &temp).map_err(|e| e.to_string())?;
    std::fs::OpenOptions::new()
        .write(true)
        .open(&temp)
        .and_then(|file| file.sync_all())
        .map_err(|e| e.to_string())?;
    std::fs::rename(temp, program).map_err(|e| e.to_string())
}

/// Called with the host lock held, before start or after a clean stop.
pub fn apply_pending(dir: &Path) -> Result<(), String> {
    let mut state: View = match std::fs::read(dir.join("update.json")) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|e| format!("The update record is damaged: {e}"))?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => View::default(),
        Err(e) => return Err(e.to_string()),
    };
    let definition = host_runtime::read_definition(dir)?;
    if definition.checkout {
        return Ok(());
    }
    let journal = definition
        .program
        .parent()
        .ok_or("Missing runtime folder")?
        .join(".empyrean-update/managed-recovery.json");
    let interrupted = match std::fs::read(journal) {
        Ok(bytes) => {
            let record: serde_json::Value = serde_json::from_slice(&bytes)
                .map_err(|e| format!("The recovery journal is damaged: {e}"))?;
            let interrupted = match record["state"].as_str() {
                Some("applying" | "restoring") => true,
                Some("installed" | "restored") => false,
                _ => return Err("The recovery journal has an invalid state.".into()),
            };
            if interrupted {
                recover_program(&definition.program, &record)?;
            }
            interrupted
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        Err(e) => return Err(e.to_string()),
    };
    let prepared = definition
        .program
        .parent()
        .ok_or("Missing runtime folder")?
        .join(".empyrean-update/managed-pending.json")
        .try_exists()
        .map_err(|e| e.to_string())?;
    if !state.pending && !interrupted && !prepared {
        return Ok(());
    }
    let result = control(dir, "apply", &[]);
    let version_result = sync_version(dir);
    match result {
        Ok(_) => {
            version_result?;
            state.pending = false;
            state.rollback = true;
            state.message = Some(format!(
                "Updated to Empyrean {}.",
                host_runtime::read_definition(dir)?.version
            ));
            save(dir, &state)
        }
        Err(error) => {
            if control(dir, "status", &[]).is_ok_and(|v| v.is_null()) {
                state.pending = false;
            }
            state.message = Some(error.clone());
            save(dir, &state)?;
            Err(error)
        }
    }
}

pub fn rollback(dir: &Path) -> Result<(), String> {
    control(dir, "rollback", &[])?;
    sync_version(dir)?;
    save(
        dir,
        &View {
            message: Some(format!(
                "Back on Empyrean {}, with the databases as they were before the update.",
                host_runtime::read_definition(dir)?.version
            )),
            ..Default::default()
        },
    )
}

pub fn cancel(dir: &Path) -> Result<(), String> {
    control(dir, "cancel", &[])?;
    let mut state = view(dir);
    state.pending = false;
    state.message = Some("Update canceled.".into());
    state.skipped_version = state.version.clone();
    save(dir, &state)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn host() -> tempfile::TempDir {
        let temp = tempfile::tempdir().unwrap();
        host_runtime::write_json(
            &temp.path().join("host.json"),
            &host_runtime::Definition {
                id: "host-test".into(),
                version: "1.0.0".into(),
                program: temp.path().join("missing-server"),
                wrapper: temp.path().join("wrapper"),
                checkout: false,
            },
        )
        .unwrap();
        temp
    }
    #[test]
    fn no_queued_update_does_not_invoke_a_server_program() {
        let host = host();
        apply_pending(host.path()).unwrap();
    }
    #[test]
    fn a_damaged_update_record_refuses_to_start_instead_of_forgetting_the_update() {
        let host = host();
        std::fs::write(host.path().join("update.json"), b"partial").unwrap();
        assert!(apply_pending(host.path())
            .unwrap_err()
            .contains("record is damaged"));
    }
    #[test]
    fn a_damaged_recovery_journal_refuses_to_start_even_without_a_queue_marker() {
        let host = host();
        let dir = host.path().join(".empyrean-update");
        std::fs::create_dir(&dir).unwrap();
        std::fs::write(dir.join("managed-recovery.json"), b"partial").unwrap();
        assert!(apply_pending(host.path())
            .unwrap_err()
            .contains("journal is damaged"));
    }
    #[test]
    fn interrupted_executable_copy_is_restored_from_its_checked_backup() {
        let host = host();
        let program = host.path().join("missing-server");
        let root = host.path().join(".empyrean-update");
        std::fs::create_dir(&root).unwrap();
        let backup = root.join("server-backup");
        std::fs::write(&backup, b"abc").unwrap();
        std::fs::write(&program, b"partial executable").unwrap();
        let record = serde_json::json!({
            "files": [[program, backup]],
            "hashes": [[backup, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"]]
        });
        recover_program(&program, &record).unwrap();
        assert_eq!(std::fs::read(&program).unwrap(), b"abc");
        std::fs::write(&backup, b"damaged").unwrap();
        assert!(recover_program(&program, &record).is_err());
        assert_eq!(std::fs::read(&program).unwrap(), b"abc");
    }
    #[test]
    fn a_server_on_this_checkouts_build_asks_github_nothing() {
        let host = host();
        let mut definition = host_runtime::read_definition(host.path()).unwrap();
        definition.checkout = true;
        host_runtime::write_json(&host.path().join("host.json"), &definition).unwrap();
        check(host.path()).unwrap();
        assert_eq!(
            view(host.path()).message.as_deref(),
            Some("This world runs this checkout's build.")
        );
        // Nothing beside the checkout's program is read or recovered, damaged or not.
        let dir = host.path().join(".empyrean-update");
        std::fs::create_dir(&dir).unwrap();
        std::fs::write(dir.join("managed-recovery.json"), b"partial").unwrap();
        apply_pending(host.path()).unwrap();
    }
    #[test]
    fn the_downloaded_file_must_also_be_the_one_release_json_describes() {
        let index = serde_json::to_vec(&serde_json::json!({
            "version": "1.0.1",
            "assets": [
                {"target": "x86_64-pc-windows-msvc", "file": "a.zip", "sha256": "AB12"},
                {"target": "aarch64-apple-darwin", "file": "b.tar.gz", "sha256": "cd34"}
            ]
        }))
        .unwrap();
        assert_eq!(
            declared_sum(&index, "x86_64-pc-windows-msvc").as_deref(),
            Some("ab12")
        );
        assert_eq!(declared_sum(&index, "x86_64-unknown-linux-gnu"), None);
        assert_eq!(declared_sum(b"not json", "x86_64-pc-windows-msvc"), None);
    }
}
