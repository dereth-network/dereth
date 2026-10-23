//! Prepared local updates for a launcher that verifies its downloaded release before staging.
//! Database migration and trial-start rollback use the same transaction as server updates.

use super::install::{self, Staged};
use super::policy;
use super::release::{Facts, Kind, ReleaseIndex};
use super::version::Version;
use super::Updater;
use empyrean_store::upgrade::{self, UpgradePolicy};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// A checked update's compatibility and approval requirements.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub from: String,
    pub version: String,
    pub automatic: bool,
    pub needs: Vec<String>,
    pub rebuild_pack: bool,
}

/// Checks an explicit candidate without weakening automatic update policy.
///
/// # Errors
/// An unsupported upgrade, mismatched binary, downgrade or missing required configuration.
pub fn inspect(
    mine: &Facts,
    index: &ReleaseIndex,
    facts: &Facts,
    can_rebuild: bool,
) -> Result<Plan, String> {
    let current = Version::parse(&mine.version)?;
    let target = Version::parse(&index.version)?;
    if index.prerelease || target.is_prerelease() || target <= current {
        return Err("The candidate must be a newer stable release.".into());
    }
    let declaration = index
        .upgrade
        .as_ref()
        .ok_or("The release has no upgrade declaration.")?;
    if declaration.upgrades_from()? > current {
        return Err(format!(
            "Install {} before this release.",
            declaration.upgrades_from
        ));
    }
    if facts.version != index.version
        || facts.target != mine.target
        || facts.databases != declaration.databases
        || facts.world_pack != declaration.world_pack
    {
        return Err(
            "The candidate binary does not match its release declaration or this platform.".into(),
        );
    }
    if facts.databases.auth < mine.databases.auth || facts.databases.shard < mine.databases.shard {
        return Err("The candidate would downgrade the character databases.".into());
    }
    let needs = policy::needs(mine, declaration, declaration.config.clone());
    let automatic = current.major == target.major
        && current.minor == target.minor
        && declaration.kind == Kind::Patch
        && needs.is_empty();
    if !automatic && declaration.previous.as_deref() != Some(mine.version.as_str()) {
        return Err(format!(
            "Install {} first so every intervening migration and configuration change is checked.",
            declaration
                .previous
                .as_deref()
                .unwrap_or("the preceding release")
        ));
    }
    if let Some(required) = needs.config.iter().find(|change| change.is_required()) {
        return Err(format!(
            "This release requires configuration changes to {}: {}",
            required.key, required.note
        ));
    }
    if needs.world_pack_rebuild && !can_rebuild {
        return Err("This update needs the source SQL configured to rebuild world.pack.".into());
    }
    Ok(Plan {
        from: mine.version.clone(),
        version: index.version.clone(),
        automatic,
        needs: needs.describe(),
        rebuild_pack: needs.world_pack_rebuild,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct File {
    name: String,
    sha256: String,
}

/// A complete, checked staging receipt, written only after every file is ready.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prepared {
    pub plan: Plan,
    dir: PathBuf,
    files: Vec<File>,
    facts: Facts,
    world_pack: Option<(PathBuf, String)>,
}

fn hash(path: &Path) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut digest = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(format!("{:x}", digest.finalize()))
}
fn save(path: &Path, value: &impl Serialize) -> Result<(), String> {
    use std::io::Write;
    let temp = path.with_extension("tmp");
    let mut file = std::fs::File::create(&temp).map_err(|e| e.to_string())?;
    file.write_all(&serde_json::to_vec(value).map_err(|e| e.to_string())?)
        .and_then(|()| file.sync_all())
        .map_err(|e| e.to_string())?;
    drop(file);
    std::fs::rename(temp, path).map_err(|e| e.to_string())
}
fn receipt(updater: &Updater) -> PathBuf {
    updater.layout.work_dir.join("managed-pending.json")
}

fn fresh_dir(root: &Path, prefix: &str) -> Result<PathBuf, String> {
    for n in 0..10_000 {
        let path = root.join(format!("{prefix}-{n}"));
        match std::fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    Err("Too many retained update folders. Archive older updates before continuing.".into())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Recovery {
    state: String,
    from: String,
    to: String,
    files: Vec<(PathBuf, Option<PathBuf>)>,
    databases: Vec<(PathBuf, Option<PathBuf>)>,
    hashes: Vec<(PathBuf, String)>,
}
fn recovery_path(updater: &Updater) -> PathBuf {
    updater.layout.work_dir.join("managed-recovery.json")
}
fn recovery(updater: &Updater) -> Result<Option<Recovery>, String> {
    match std::fs::read(recovery_path(updater)) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|e| e.to_string()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}
fn snapshot(updater: &Updater, staged: &Staged, config: Option<&Path>) -> Result<Recovery, String> {
    let dir = fresh_dir(&updater.layout.work_dir, "managed-recovery")?;
    let mut files = Vec::new();
    let mut hashes = Vec::new();
    let mut paths: Vec<_> = staged
        .files
        .iter()
        .map(|n| updater.layout.install_dir.join(n))
        .collect();
    paths.push(updater.layout.world_pack.clone());
    if let Some(config) = config {
        paths.push(config.to_path_buf());
    }
    paths.sort();
    paths.dedup();
    for (i, path) in paths.into_iter().enumerate() {
        let backup = if path.is_file() {
            let backup = dir.join(format!("file-{i}"));
            std::fs::copy(&path, &backup).map_err(|e| e.to_string())?;
            hashes.push((backup.clone(), hash(&backup)?));
            Some(backup)
        } else {
            None
        };
        files.push((path, backup));
    }
    let mut databases = Vec::new();
    for db in &updater.layout.databases {
        if db.is_file() {
            let backup = upgrade::snapshot(db, "managed-recovery", &UpgradePolicy::default())?;
            // Keep this recovery point independently of the normal rolling backup retention.
            let retained = dir.join(format!("database-{}", databases.len()));
            std::fs::copy(&backup, &retained).map_err(|e| e.to_string())?;
            hashes.push((retained.clone(), hash(&retained)?));
            databases.push((db.clone(), Some(retained)));
        } else {
            databases.push((db.clone(), None));
        }
    }
    for (path, _) in &hashes {
        std::fs::OpenOptions::new()
            .write(true)
            .open(path)
            .and_then(|file| file.sync_all())
            .map_err(|e| e.to_string())?;
    }
    let record = Recovery {
        state: "applying".into(),
        from: updater.mine.version.clone(),
        to: staged.version.to_string(),
        files,
        databases,
        hashes,
    };
    save(&recovery_path(updater), &record)?;
    Ok(record)
}

/// Restores the retained binaries and database snapshot while the host is stopped.
/// An explicit rollback after a successful update discards progress since that snapshot.
///
/// # Errors
/// No recoverable transaction or an incomplete restoration; the journal remains for retry.
pub fn rollback(updater: &Updater) -> Result<String, String> {
    let mut record = recovery(updater)?.ok_or("There is no update backup to restore.")?;
    if record.state == "restored" {
        return Ok(record.from);
    }
    for (path, expected) in &record.hashes {
        if hash(path)? != *expected {
            return Err("A recovery backup is damaged; no files were restored.".into());
        }
    }
    record.state = "restoring".into();
    save(&recovery_path(updater), &record)?;
    let from = Version::parse(&record.from)?;
    for (path, backup) in &record.files {
        if path.exists() {
            let aside = install::aside_name(path, &from);
            std::fs::rename(path, aside).map_err(|e| e.to_string())?;
        }
        if let Some(backup) = backup {
            std::fs::copy(backup, path).map_err(|e| e.to_string())?;
        }
    }
    for (db, backup) in &record.databases {
        if let Some(backup) = backup {
            upgrade::restore(backup, db).map_err(|e| e.to_string())?;
        } else if db.exists() {
            std::fs::remove_file(db).map_err(|e| e.to_string())?;
        }
    }
    record.state = "restored".into();
    save(&recovery_path(updater), &record)?;
    match std::fs::remove_file(receipt(updater)) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.to_string()),
    }
    Ok(record.from)
}

/// Copies a verified candidate into a private staging directory while the world may run.
///
/// # Errors
/// Incompatible content, missing approval, unreadable files or a failed pack rebuild.
pub fn prepare(
    updater: &Updater,
    candidate: &Path,
    index: &ReleaseIndex,
    approved: Option<&str>,
) -> Result<Plan, String> {
    let facts = updater
        .build
        .facts(&candidate.join(install::server_file_name()))?;
    let plan = inspect(&updater.mine, index, &facts, updater.pack_inputs.is_some())?;
    if !plan.automatic && approved != Some(plan.version.as_str()) {
        return Err(
            "Review this release and approve its exact version before preparing it.".into(),
        );
    }
    if recovery(updater)?.is_some_and(|r| matches!(r.state.as_str(), "applying" | "restoring")) {
        return Err("Recover the interrupted update before preparing another release.".into());
    }
    if super::install::State::load(&updater.layout.work_dir)
        .failed_versions()
        .contains(&Version::parse(&plan.version)?)
        && approved != Some(plan.version.as_str())
    {
        return Err(
            "This release failed here previously. Review and approve it before retrying.".into(),
        );
    }
    std::fs::create_dir_all(&updater.layout.work_dir).map_err(|e| e.to_string())?;
    let dir = fresh_dir(
        &updater.layout.work_dir,
        &format!("managed-stage-{}", plan.version),
    )?;
    let mut files = Vec::new();
    for entry in std::fs::read_dir(candidate).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        if kind.is_symlink() {
            return Err("A release file must not be a link.".into());
        }
        if !kind.is_file() {
            continue;
        }
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "Invalid release filename")?;
        std::fs::copy(entry.path(), dir.join(&name)).map_err(|e| e.to_string())?;
        files.push(File {
            sha256: hash(&dir.join(&name))?,
            name,
        });
    }
    for name in [install::server_file_name(), install::import_file_name()] {
        if !files.iter().any(|f| f.name == name) {
            return Err(format!("The candidate has no {name}."));
        }
    }
    let world_pack = if plan.rebuild_pack {
        let path = dir.join("rebuilt.world.pack");
        updater.build.rebuild_pack(
            &dir.join(install::import_file_name()),
            updater.pack_inputs.as_ref().ok_or("Missing pack inputs")?,
            &path,
        )?;
        Some((path.clone(), hash(&path)?))
    } else {
        None
    };
    save(
        &receipt(updater),
        &Prepared {
            plan: plan.clone(),
            dir,
            files,
            facts,
            world_pack,
        },
    )?;
    Ok(plan)
}

/// Reads the prepared update without changing the running world.
///
/// # Errors
/// A damaged staging receipt.
pub fn pending(updater: &Updater) -> Result<Option<Prepared>, String> {
    let path = receipt(updater);
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|e| e.to_string()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

/// Cancels a prepared update before installation starts, keeping its downloaded files.
///
/// # Errors
/// An install already needs recovery, or its receipt cannot be removed.
pub fn discard(updater: &Updater) -> Result<(), String> {
    if recovery(updater)?.is_some_and(|r| matches!(r.state.as_str(), "applying" | "restoring")) {
        return Err("Recover the interrupted update before canceling it.".into());
    }
    match std::fs::remove_file(receipt(updater)) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

/// Installs the prepared update through the existing backup and health-check transaction.
/// The caller must hold the host lock and establish that the game server is stopped.
///
/// # Errors
/// Changed staging files, an outdated receipt, or an unsuccessful install/rollback.
pub fn apply(updater: &Updater) -> Result<Option<Facts>, String> {
    apply_with_config(updater, None)
}

/// Installs a prepared update, retaining the configuration with the database and content backup.
///
/// # Errors
/// As [`apply`], including an unreadable configuration snapshot.
pub fn apply_with_config(
    updater: &Updater,
    config: Option<&Path>,
) -> Result<Option<Facts>, String> {
    if recovery(updater)?.is_some_and(|r| matches!(r.state.as_str(), "applying" | "restoring")) {
        let restored = rollback(updater)?;
        return Err(format!(
            "An interrupted update was restored to {restored}. Start the host again."
        ));
    }
    let Some(prepared) = pending(updater)? else {
        return Ok(None);
    };
    if prepared.plan.version == updater.mine.version
        && recovery(updater)?
            .is_some_and(|r| r.state == "installed" && r.to == updater.mine.version)
    {
        std::fs::remove_file(receipt(updater)).map_err(|e| e.to_string())?;
        return Ok(Some(updater.mine.clone()));
    }
    if prepared.plan.from != updater.mine.version {
        return Err("The prepared update belongs to a different installed version.".into());
    }
    for file in &prepared.files {
        if !Path::new(&file.name)
            .components()
            .all(|c| matches!(c, std::path::Component::Normal(_)))
            || hash(&prepared.dir.join(&file.name))? != file.sha256
        {
            return Err("A staged release file changed after verification.".into());
        }
    }
    if let Some((path, expected)) = &prepared.world_pack {
        if hash(path)? != *expected {
            return Err("The staged world pack changed after verification.".into());
        }
    }
    let staged = Staged {
        version: Version::parse(&prepared.plan.version)?,
        dir: prepared.dir,
        files: prepared.files.into_iter().map(|f| f.name).collect(),
        facts: prepared.facts,
        world_pack: prepared.world_pack.map(|(p, _)| p),
        needs: prepared.plan.needs,
    };
    let mut record = snapshot(updater, &staged, config)?;
    let result = match updater.install(&staged) {
        Ok(result) => result,
        Err(error) => {
            return match rollback(updater) {
                Ok(version) => Err(format!("{error}; restored to {version}")),
                Err(recovery_error) => Err(format!(
                    "{error}; recovery needs attention: {recovery_error}"
                )),
            };
        }
    };
    record.state = "installed".into();
    save(&recovery_path(updater), &record)?;
    std::fs::remove_file(receipt(updater)).map_err(|e| e.to_string())?;
    Ok(Some(result.facts))
}
