//! A server's updates: which newer Empyrean (or Yalain) release it may take, downloading and
//! checking it, and installing it at the next stop or start.
//!
//! A newer release is found in the repository's releases ([`components::release_list`]). Its
//! `release.json`, checked against the release's `SHA256SUMS`, says what upgrading to it involves.
//! A patch of the same minor version is downloaded by itself; any other release waits for the
//! person to approve that exact version. Empyrean then backs up the databases, installs, trial
//! starts the new server and restores everything when that fails.
//!
//! A Yalain world takes its releases the same way, and installs as Yalain's releases say: with the
//! world stopped, the program is replaced by the new release's ([`install_yalain`]), after the
//! world database it pins is fetched when it pins another (`world_database_fetch`) and its store
//! (`yalain.sqlite`, accounts and characters) is backed up. Undo puts the previous program and
//! that backup back.

use crate::{components, host_runtime};
use dereth_launch::components::{self as metadata, Product, Release};
use dereth_launch::world::Game;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The server a world of `game` runs, whose releases its updates are, and its name.
fn server_of(game: Game) -> (Product, &'static str) {
    match game {
        Game::Dereth => (Product::Empyrean, "Empyrean"),
        Game::Auberean => (Product::Yalain, "Yalain"),
    }
}

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
    let (product, server) = server_of(definition.game);
    let current = metadata::version(&definition.version)
        .ok_or_else(|| format!("Invalid {server} version"))?;
    let list = components::release_list(false, &[product.releases_since(current)])?;
    let mut releases: Vec<_> = metadata::releases(&list, product)?
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
                    format!("Downloading {server} {offered}...")
                } else {
                    format!("{server} {offered} is out. Read what changed, then update.")
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
            message: Some(format!("{server} {} is the newest.", definition.version)),
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
/// stop or start (a Yalain world's waits in the world's folder for [`install_yalain`]). `approved`
/// is the version the person approved, for an update that needs it.
pub fn stage(dir: &Path, approved: Option<&str>) -> Result<(), String> {
    let (product, server) = server_of(host_runtime::read_definition(dir)?.game);
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
    let (name, url) = offer.release.archive(product, components::target())?;
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
    let exe = components::find_executable(&folder, product)?;
    let candidate = exe.parent().ok_or("Missing candidate folder")?;
    let index_path = folder.join("checked-release.json");
    std::fs::write(&index_path, &offer.index).map_err(|e| e.to_string())?;
    if product == Product::Empyrean {
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
    }
    let mut state = view(dir);
    state.pending = true;
    state.skipped_version = None;
    state.message = Some(format!(
        "{server} {version} is downloaded. It goes in the next time this world stops or starts."
    ));
    save(dir, &state)
}

/// What undoing a Yalain world's last update puts back: the program and version it ran, its store
/// as it was, and the world database release its fetch had found.
#[derive(Serialize, Deserialize)]
struct YalainRollback {
    program: PathBuf,
    version: String,
    backup: PathBuf,
    fetched: Option<String>,
}

/// The file a Yalain world keeps what its last update replaced in, for Undo.
const YALAIN_ROLLBACK: &str = "update-rollback.json";

/// How many backups of a Yalain world's store its updates keep, newest first.
const KEPT_BACKUPS: usize = 3;

/// The Yalain world in `dir`'s store: `[database] shard_db_path` in its configuration (relative to
/// its folder), else `yalain.sqlite` there.
fn yalain_store(dir: &Path) -> PathBuf {
    let named = std::fs::read_to_string(dir.join(crate::host_yalain::CONFIG))
        .ok()
        .and_then(|t| t.parse::<toml::Table>().ok())
        .and_then(|t| {
            t.get("database")?
                .get("shard_db_path")?
                .as_str()
                .map(str::to_owned)
        })
        .filter(|p| !p.trim().is_empty())
        .unwrap_or_else(|| crate::host_yalain::DATABASE.to_owned());
    let path = Path::new(&named);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        dir.join(path)
    }
}

/// `store` with `suffix` after its name: its `-wal` and `-shm` companions, and its backups.
fn beside(store: &Path, suffix: &str) -> PathBuf {
    let mut name = store.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    store.with_file_name(name)
}

/// Seconds since 1970 as the timestamp a Yalain store's backup is named by: `YYYYMMDDTHHMMSSZ`.
fn backup_stamp(unix: u64) -> String {
    let (days, secs) = (i64::try_from(unix / 86_400).unwrap_or(0), unix % 86_400);
    // Days since 1970 to a civil date (the proleptic Gregorian calendar).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}{month:02}{day:02}T{:02}{:02}{:02}Z",
        secs / 3600,
        secs / 60 % 60,
        secs % 60
    )
}

/// What a backup's name orders it by: its timestamp, and its number within that second. A name
/// from before backups were named as Yalain names them, ending in plain seconds, reads as the same
/// time. `None` for anything else (a backup's `-wal`).
fn backup_time(rest: &str) -> Option<(String, u32)> {
    let last = rest.rsplit('-').next()?;
    if let Ok(unix) = last.parse::<u64>() {
        return Some((backup_stamp(unix), 1));
    }
    let (stamp, n) = last
        .split_once('.')
        .map_or((last, Some(1)), |(s, n)| (s, n.parse().ok()));
    let ok = stamp.len() == 16
        && stamp.as_bytes()[8] == b'T'
        && stamp.ends_with('Z')
        && stamp[..8].bytes().all(|b| b.is_ascii_digit())
        && stamp[9..15].bytes().all(|b| b.is_ascii_digit());
    ok.then(|| (stamp.to_owned(), n.unwrap_or(1)))
}

/// Back up the Yalain world's store (`store`, and its `-wal` and `-shm` when it has them) before
/// the update from `from` to `to`, named as Yalain names its own backups, so that it lists and trims
/// them with its own, and keep the newest [`KEPT_BACKUPS`] of them all. The backup's path.
fn backup_store(store: &Path, from: &str, to: &str) -> Result<PathBuf, String> {
    let at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let stamp = backup_stamp(at);
    let named = format!(".backup-update-v{from}-v{to}-{stamp}");
    // Two in one second are told apart by a number, as Yalain tells its own.
    let backup = std::iter::once(beside(store, &named))
        .chain((2..).map(|n| beside(store, &format!("{named}.{n}"))))
        .find(|p| !p.exists())
        .unwrap_or_else(|| beside(store, &named));
    if store.is_file() {
        std::fs::copy(store, &backup).map_err(|e| format!("Could not back up the store: {e}"))?;
    } else {
        // A world that never started has no store yet: an empty backup stands for none.
        std::fs::write(&backup, b"").map_err(|e| e.to_string())?;
    }
    for companion in ["-wal", "-shm"] {
        let from = beside(store, companion);
        if from.is_file() {
            std::fs::copy(&from, beside(&backup, companion))
                .map_err(|e| format!("Could not back up the store: {e}"))?;
        }
    }
    let prefix = format!(
        "{}.backup-",
        store.file_name().unwrap_or_default().to_string_lossy()
    );
    let names: Vec<String> = std::fs::read_dir(store.parent().unwrap_or(store))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with(&prefix))
        .collect();
    let mut backups: Vec<((String, u32), &String)> = names
        .iter()
        .filter_map(|n| Some((backup_time(n.strip_prefix(&prefix)?)?, n)))
        .collect();
    backups.sort_by(|a, b| b.0.cmp(&a.0));
    let dir = store.parent().unwrap_or(store);
    for (_, old) in backups.iter().skip(KEPT_BACKUPS) {
        let old = dir.join(old);
        let _ = std::fs::remove_file(&old);
        for companion in ["-wal", "-shm"] {
            let _ = std::fs::remove_file(beside(&old, companion));
        }
    }
    // Yalain trims a backup without its companions: those it left go too.
    for name in &names {
        for companion in ["-wal", "-shm"] {
            if let Some(whose) = name.strip_suffix(companion) {
                if !dir.join(whose).exists() {
                    let _ = std::fs::remove_file(dir.join(name));
                }
            }
        }
    }
    Ok(backup)
}

/// Install the Yalain release downloaded into the stopped world in `dir`, as its `release.json`
/// says: its program copied into `runtime/<version>`, the world database it pins fetched by it
/// first when the release says so (`upgrade.world_database_fetch`), the store backed up; then the
/// world runs it. Until the last step nothing the world runs changes, and a failure on the way puts
/// the fetch's record back. The version installed.
///
/// # Errors
/// No download, or a step that failed, in words for a person.
pub fn install_yalain(dir: &Path) -> Result<String, String> {
    let folder = dir.join("update-download");
    let index: serde_json::Value = serde_json::from_slice(
        &std::fs::read(folder.join("checked-release.json"))
            .map_err(|_| "The downloaded update is gone: check for updates again.".to_owned())?,
    )
    .map_err(|e| e.to_string())?;
    let version = index["version"]
        .as_str()
        .filter(|v| metadata::version(v).is_some())
        .ok_or("The update's release.json names no version.")?
        .to_owned();
    let exe = components::find_executable(&folder, Product::Yalain)?;
    let mut definition = host_runtime::read_definition(dir)?;
    let runtime = dir.join("runtime").join(&version);
    if runtime.exists() {
        std::fs::remove_dir_all(&runtime).map_err(|e| e.to_string())?;
    }
    crate::hosts::copy_tree(exe.parent().ok_or("Missing candidate folder")?, &runtime)?;
    let program = runtime.join(exe.file_name().ok_or("Invalid program name")?);
    let fetched_before = std::fs::read_to_string(dir.join(crate::host_yalain::FETCHED)).ok();
    let installed = (|| {
        if index["upgrade"]["world_database_fetch"] == true {
            crate::host_yalain::fetch(&program, dir)?;
        }
        let backup = backup_store(&yalain_store(dir), &definition.version, &version)?;
        host_runtime::write_json(
            &dir.join(YALAIN_ROLLBACK),
            &YalainRollback {
                program: definition.program.clone(),
                version: definition.version.clone(),
                backup,
                fetched: fetched_before.clone(),
            },
        )?;
        definition.program = program.clone();
        definition.version = version.clone();
        host_runtime::write_json(&dir.join("host.json"), &definition)
    })();
    if let Err(error) = installed {
        if let Some(before) = &fetched_before {
            let _ = std::fs::write(dir.join(crate::host_yalain::FETCHED), before);
        }
        // The new release's copy goes too: the world never ran it.
        let _ = std::fs::remove_dir_all(&runtime);
        return Err(error);
    }
    let _ = std::fs::remove_dir_all(&folder);
    Ok(version)
}

/// Undo the stopped Yalain world in `dir`'s last update: the program it ran before, its store from
/// the backup made then, and the world database release it played. The version it is back on.
///
/// # Errors
/// No update to undo, or a file that cannot be put back.
pub fn rollback_yalain(dir: &Path) -> Result<String, String> {
    let record: YalainRollback = serde_json::from_slice(
        &std::fs::read(dir.join(YALAIN_ROLLBACK))
            .map_err(|_| "There is no update to undo.".to_owned())?,
    )
    .map_err(|e| format!("The undo record is damaged: {e}"))?;
    let store = yalain_store(dir);
    let empty = std::fs::metadata(&record.backup)
        .map_err(|_| "The store's backup is gone.".to_owned())?
        .len()
        == 0;
    for companion in ["-wal", "-shm"] {
        let _ = std::fs::remove_file(beside(&store, companion));
    }
    if empty {
        let _ = std::fs::remove_file(&store);
    } else {
        std::fs::copy(&record.backup, &store).map_err(|e| e.to_string())?;
    }
    for companion in ["-wal", "-shm"] {
        let kept = beside(&record.backup, companion);
        if kept.is_file() {
            std::fs::copy(&kept, beside(&store, companion)).map_err(|e| e.to_string())?;
        }
    }
    if let Some(fetched) = &record.fetched {
        std::fs::write(dir.join(crate::host_yalain::FETCHED), fetched)
            .map_err(|e| e.to_string())?;
    }
    let mut definition = host_runtime::read_definition(dir)?;
    definition.program = record.program;
    definition.version = record.version.clone();
    host_runtime::write_json(&dir.join("host.json"), &definition)?;
    let _ = std::fs::remove_file(dir.join(YALAIN_ROLLBACK));
    Ok(record.version)
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
    if definition.game == Game::Auberean {
        if !state.pending {
            return Ok(());
        }
        state.pending = false;
        // A release that will not go in leaves the world on the one it ran, which starts as before.
        match install_yalain(dir) {
            Ok(version) => {
                state.rollback = true;
                state.message = Some(format!("Updated to Yalain {version}."));
            }
            Err(error) => {
                let _ = std::fs::remove_dir_all(dir.join("update-download"));
                // No longer offered as it was: the page says why it did not go in, and the next
                // check for updates finds it again.
                state.version = None;
                state.message = Some(format!(
                    "The update did not go in: {error} The world runs Yalain {} as before.",
                    definition.version
                ));
            }
        }
        return save(dir, &state);
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
    if host_runtime::read_definition(dir)?.game == Game::Auberean {
        let version = rollback_yalain(dir)?;
        return save(
            dir,
            &View {
                message: Some(format!(
                    "Back on Yalain {version}, with its accounts and characters as they were before the update."
                )),
                ..Default::default()
            },
        );
    }
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
    if host_runtime::read_definition(dir)?.game == Game::Auberean {
        let _ = std::fs::remove_dir_all(dir.join("update-download"));
    } else {
        control(dir, "cancel", &[])?;
    }
    let mut state = view(dir);
    state.pending = false;
    state.message = Some("Update canceled.".into());
    state.skipped_version = state.version.clone();
    save(dir, &state)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stopped Yalain world on 0.1.0, with characters in its store and the pin its fetch found,
    /// and the release `version` downloaded for it, whose `release.json` says whether it fetches
    /// another world database first.
    fn yalain_world(version: &str, fetch: bool) -> tempfile::TempDir {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path();
        let exe = format!("yalain{}", std::env::consts::EXE_SUFFIX);
        let old = dir.join("runtime/0.1.0").join(&exe);
        std::fs::create_dir_all(old.parent().unwrap()).unwrap();
        std::fs::write(&old, b"the old program").unwrap();
        host_runtime::write_json(
            &dir.join("host.json"),
            &host_runtime::Definition {
                id: "host-test".into(),
                version: "0.1.0".into(),
                program: old,
                wrapper: dir.join("wrapper"),
                checkout: false,
                game: Game::Auberean,
            },
        )
        .unwrap();
        std::fs::write(dir.join(crate::host_yalain::CONFIG), "[server]\n").unwrap();
        std::fs::write(dir.join(crate::host_yalain::DATABASE), b"characters before").unwrap();
        std::fs::write(dir.join(crate::host_yalain::FETCHED), "v0.21.0").unwrap();
        let download = dir.join("update-download");
        let new = download.join(format!("yalain-{version}")).join(&exe);
        std::fs::create_dir_all(new.parent().unwrap()).unwrap();
        // Not a program: a release that must fetch cannot, and says why.
        std::fs::write(&new, b"the new program").unwrap();
        std::fs::write(
            download.join("checked-release.json"),
            serde_json::json!({
                "version": version,
                "upgrade": { "kind": "minor", "previous": "0.1.0", "upgrades_from": "0.1.0",
                             "world_database": "v0.23.0", "world_database_fetch": fetch },
            })
            .to_string(),
        )
        .unwrap();
        temp
    }

    /// A Yalain world installs a downloaded release as its `release.json` says: its store backed
    /// up, the new program run from `runtime/<version>`; and Undo puts the old program and the
    /// store as it was before the update back.
    #[test]
    fn a_yalain_world_installs_a_release_and_undo_puts_the_old_one_and_its_store_back() {
        let temp = yalain_world("0.2.0", false);
        let dir = temp.path();
        save(
            dir,
            &View {
                pending: true,
                ..View::default()
            },
        )
        .unwrap();
        apply_pending(dir).unwrap();
        let definition = host_runtime::read_definition(dir).unwrap();
        assert_eq!(definition.version, "0.2.0");
        assert!(definition.program.starts_with(dir.join("runtime/0.2.0")));
        assert_eq!(
            std::fs::read(&definition.program).unwrap(),
            b"the new program"
        );
        let state = view(dir);
        assert!(!state.pending && state.rollback);
        assert_eq!(state.message.as_deref(), Some("Updated to Yalain 0.2.0."));
        assert!(!dir.join("update-download").exists());

        // Played on the new release, then undone.
        std::fs::write(dir.join(crate::host_yalain::DATABASE), b"characters after").unwrap();
        rollback(dir).unwrap();
        let definition = host_runtime::read_definition(dir).unwrap();
        assert_eq!(definition.version, "0.1.0");
        assert_eq!(
            std::fs::read(&definition.program).unwrap(),
            b"the old program"
        );
        assert_eq!(
            std::fs::read(dir.join(crate::host_yalain::DATABASE)).unwrap(),
            b"characters before"
        );
        assert!(view(dir)
            .message
            .unwrap()
            .starts_with("Back on Yalain 0.1.0"));
        assert_eq!(
            rollback_yalain(dir).unwrap_err(),
            "There is no update to undo."
        );
    }

    /// A release that must fetch the world database it pins and cannot does not go in: the world
    /// runs the release it ran, with its fetch's record as it was, and starts as before.
    #[test]
    fn a_yalain_release_that_cannot_fetch_its_world_database_leaves_the_world_as_it_was() {
        let temp = yalain_world("0.3.0", true);
        let dir = temp.path();
        save(
            dir,
            &View {
                version: Some("0.3.0".into()),
                pending: true,
                ..View::default()
            },
        )
        .unwrap();
        apply_pending(dir).unwrap();
        let definition = host_runtime::read_definition(dir).unwrap();
        assert_eq!(definition.version, "0.1.0");
        assert_eq!(
            std::fs::read(&definition.program).unwrap(),
            b"the old program"
        );
        assert_eq!(
            std::fs::read_to_string(dir.join(crate::host_yalain::FETCHED)).unwrap(),
            "v0.21.0"
        );
        let state = view(dir);
        assert!(!state.pending && !state.rollback);
        // Not shown as offered, so the page says why it did not go in.
        assert_eq!(state.version, None);
        assert!(state
            .message
            .unwrap()
            .starts_with("The update did not go in:"));
        assert!(!dir.join(YALAIN_ROLLBACK).exists());
        assert!(!dir.join("runtime/0.3.0").exists(), "its copy is gone");
        assert!(dir.join("runtime/0.1.0").exists());
    }

    /// Each update backs the store up beside it under the versions and the time, with its WAL,
    /// and the newest three are kept.
    #[test]
    fn a_yalain_store_s_update_backups_keep_the_newest_three() {
        let temp = tempfile::tempdir().unwrap();
        let store = temp.path().join("yalain.sqlite");
        std::fs::write(&store, b"store").unwrap();
        std::fs::write(beside(&store, "-wal"), b"wal").unwrap();
        for (n, at) in [100u64, 200, 300, 400].iter().enumerate() {
            let old = beside(
                &store,
                &format!(".backup-update-v0.{n}.0-v0.{}.0-{at}", n + 1),
            );
            std::fs::write(&old, b"old").unwrap();
            std::fs::write(beside(&old, "-wal"), b"old wal").unwrap();
        }
        let backup = backup_store(&store, "0.4.0", "0.5.0").unwrap();
        assert_eq!(std::fs::read(&backup).unwrap(), b"store");
        assert_eq!(std::fs::read(beside(&backup, "-wal")).unwrap(), b"wal");
        let mut left: Vec<String> = std::fs::read_dir(temp.path())
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains(".backup-update-") && !n.ends_with("-wal"))
            .collect();
        left.sort();
        assert_eq!(left.len(), 3, "{left:?}");
        assert!(
            left.iter().any(|n| n.ends_with("-400")) && left.iter().any(|n| n.ends_with("-300"))
        );
        assert!(
            left.iter().any(|n| n.ends_with('Z')),
            "the new one named by the time: {left:?}"
        );
        assert!(!temp
            .path()
            .join("yalain.sqlite.backup-update-v0.0.0-v0.1.0-100")
            .exists());
        assert!(!temp
            .path()
            .join("yalain.sqlite.backup-update-v0.0.0-v0.1.0-100-wal")
            .exists());
    }

    /// A store's backup is named as Yalain names its own, so Yalain lists and trims it: the time
    /// in UTC to the second, and a number after it for a second one in that second.
    #[test]
    fn a_yalain_store_s_backup_is_named_by_the_time_as_yalain_names_its_own() {
        assert_eq!(backup_stamp(0), "19700101T000000Z");
        assert_eq!(backup_stamp(951_782_400), "20000229T000000Z");
        assert_eq!(backup_stamp(1_793_581_052), "20261102T005732Z");
        let temp = tempfile::tempdir().unwrap();
        let store = temp.path().join("yalain.sqlite");
        std::fs::write(&store, b"store").unwrap();
        let first = backup_store(&store, "0.1.0", "0.2.0").unwrap();
        let name = first.file_name().unwrap().to_string_lossy().into_owned();
        let rest = name.strip_prefix("yalain.sqlite.backup-").unwrap();
        assert!(rest.starts_with("update-v0.1.0-v0.2.0-"), "{rest}");
        let time = backup_time(rest).expect("the name reads").0;
        assert!(rest.ends_with(&time), "{rest}");
    }

    /// The newest three of all a store's backups are kept, Yalain's own among them, whichever way
    /// each is named; a companion whose backup Yalain took away goes too.
    #[test]
    fn a_yalain_store_keeps_three_backups_of_all_kinds_and_no_lost_companions() {
        let temp = tempfile::tempdir().unwrap();
        let store = temp.path().join("yalain.sqlite");
        std::fs::write(&store, b"store").unwrap();
        for old in [
            ".backup-update-v0.1.0-v0.2.0-100",
            ".backup-v3-v4-20000101T000000Z",
            ".backup-v4-v5-20010101T000000Z",
            ".backup-v5-v6-20010101T000000Z.2",
        ] {
            std::fs::write(beside(&store, old), b"old").unwrap();
        }
        let lost = beside(&store, ".backup-update-v0.0.1-v0.1.0-50-wal");
        std::fs::write(&lost, b"wal").unwrap();
        let backup = backup_store(&store, "0.4.0", "0.5.0").unwrap();
        assert!(backup.exists());
        assert!(beside(&store, ".backup-v5-v6-20010101T000000Z.2").exists());
        assert!(beside(&store, ".backup-v4-v5-20010101T000000Z").exists());
        assert!(!beside(&store, ".backup-v3-v4-20000101T000000Z").exists());
        assert!(!beside(&store, ".backup-update-v0.1.0-v0.2.0-100").exists());
        assert!(!lost.exists());
    }

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
                game: dereth_launch::world::Game::Dereth,
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
