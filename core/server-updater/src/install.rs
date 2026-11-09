//! Installing a staged release over a stopped server, and taking it back out when it fails.
//!
//! [`apply`], with the server stopped:
//!
//! 1. backs up each database file with SQLite's online backup, as
//!    `<file>.backup-update-v<old>-v<new>-<UTC timestamp>` (the store's backup, [`Product::backup`];
//!    the newest three are kept). A backup that fails stops here, with nothing changed;
//! 2. moves each installed file the release replaces aside, as `<name>.previous-v<old>`, and
//!    copies the release's file in. Windows cannot overwrite a running executable but can rename
//!    one, so this works while the old server's process is still running the update;
//! 3. moves `world.pack` aside and puts the rebuilt one in, when the release needs one;
//! 4. starts the new server for a health check ([`Product::trial`]): it must open its databases
//!    (migrating them), load its world, bind its listeners, report itself ready, and stop cleanly;
//! 5. on any failure after step 1 rolls everything back: the release's files removed and the old
//!    ones moved back, the old `world.pack` back, each database restored from its backup, and the
//!    version recorded as failed so it is not tried again automatically.
//!
//! The files moved aside by an earlier update are removed at the start of the next one (on
//! Windows, only once no process runs them).

use std::ffi::OsString;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::archive;
use crate::release::BuildFacts;
use crate::version::Version;
use crate::Product;

/// Where the server is installed and what an update touches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout<P> {
    /// The folder the binaries are in.
    pub install_dir: PathBuf,
    /// The updater's own folder ([`Product::WORK_DIR`] under the installation).
    pub work_dir: PathBuf,
    /// The database files (one file may hold several databases).
    pub databases: Vec<PathBuf>,
    /// The world pack; empty for a server that keeps no pack beside itself.
    pub world_pack: PathBuf,
    product: PhantomData<P>,
}

impl<P: Product> Layout<P> {
    /// The layout of an installation in `install_dir`.
    #[must_use]
    pub fn new(install_dir: &Path, databases: Vec<PathBuf>, world_pack: PathBuf) -> Self {
        let mut unique: Vec<PathBuf> = Vec::new();
        for d in databases {
            if !unique.contains(&d) {
                unique.push(d);
            }
        }
        Self {
            install_dir: install_dir.to_path_buf(),
            work_dir: install_dir.join(P::WORK_DIR),
            databases: unique,
            world_pack,
            product: PhantomData,
        }
    }

    /// The installed server binary.
    #[must_use]
    pub fn server_exe(&self) -> PathBuf {
        self.install_dir.join(P::server_file_name())
    }
}

/// `exe` as a command, without a console window of its own on Windows.
#[must_use]
pub fn process(exe: &Path) -> Command {
    #[allow(unused_mut)]
    let mut command = Command::new(exe);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    command
}

/// What the server binary `exe` says it is (`--release-facts`).
///
/// # Errors
/// When it does not run or answers something else.
pub fn release_facts<F: BuildFacts>(exe: &Path) -> Result<F, String> {
    let out = process(exe)
        .arg("--release-facts")
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("{} does not run here: {e}", exe.display()))?;
    if !out.status.success() {
        return Err(format!(
            "{} --release-facts failed ({}): {}",
            exe.display(),
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    facts_from_json(&String::from_utf8_lossy(&out.stdout))
}

/// Reads facts printed as one line of JSON.
///
/// # Errors
/// When `text` is not such JSON.
pub fn facts_from_json<F: BuildFacts>(text: &str) -> Result<F, String> {
    serde_json::from_str(text.trim()).map_err(|e| format!("not a build's facts: {e}"))
}

/// The health check: starts the installed server `exe` with `--update-trial` and `args`, in
/// `current_dir`, waits until it has reported itself ready in `work_dir` and stopped, for at most
/// `timeout`. What it reported when ready.
///
/// # Errors
/// When it does not come up.
pub fn run_trial<F: BuildFacts>(
    exe: &Path,
    args: &[OsString],
    current_dir: &Path,
    timeout: Duration,
    work_dir: &Path,
) -> Result<F, String> {
    std::fs::create_dir_all(work_dir).map_err(|e| format!("{}: {e}", work_dir.display()))?;
    let ready = work_dir.join("ready.json");
    let _ = std::fs::remove_file(&ready);
    let mut command = process(exe);
    command.arg("--update-trial").arg(&ready);
    command.args(args);
    let mut child = command
        .current_dir(current_dir)
        .stdin(Stdio::null())
        .spawn()
        .map_err(|e| format!("{} could not be started: {e}", exe.display()))?;
    let end = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= end => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "the new server did not come up and stop within {} s",
                    timeout.as_secs()
                ));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(200)),
            Err(e) => return Err(format!("waiting for the new server: {e}")),
        }
    };
    let report = std::fs::read_to_string(&ready).ok();
    let _ = std::fs::remove_file(&ready);
    match report {
        Some(text) if status.success() => facts_from_json(&text),
        Some(_) => Err(format!(
            "the new server came up but did not stop cleanly ({status})"
        )),
        None => Err(format!(
            "the new server stopped ({status}) before it was ready: its log above says why"
        )),
    }
}

/// A release downloaded, checked and unpacked, ready to install.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Staged<F> {
    pub version: Version,
    /// The folder the release's files are in.
    pub dir: PathBuf,
    /// The release's files (names within [`Staged::dir`]).
    pub files: Vec<String>,
    /// What its server binary reports.
    pub facts: F,
    /// The rebuilt world pack, when the release needs one.
    pub world_pack: Option<PathBuf>,
    /// What installing it involves, one line each.
    pub needs: Vec<String>,
}

/// The updater's record: the releases that failed here.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    #[serde(default)]
    pub failed: Vec<Failure>,
}

/// A release that failed its health check here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Failure {
    pub version: String,
    pub reason: String,
}

impl State {
    fn path(work_dir: &Path) -> PathBuf {
        work_dir.join("state.json")
    }

    /// The record in `work_dir` (empty when there is none).
    #[must_use]
    pub fn load(work_dir: &Path) -> Self {
        std::fs::read_to_string(Self::path(work_dir))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    /// Writes the record.
    ///
    /// # Errors
    /// When it cannot be written.
    pub fn save(&self, work_dir: &Path) -> Result<(), String> {
        std::fs::create_dir_all(work_dir).map_err(|e| format!("{}: {e}", work_dir.display()))?;
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())? + "\n";
        std::fs::write(Self::path(work_dir), text).map_err(|e| format!("{e}"))
    }

    /// The versions that failed.
    #[must_use]
    pub fn failed_versions(&self) -> Vec<Version> {
        self.failed
            .iter()
            .filter_map(|f| Version::parse(&f.version).ok())
            .collect()
    }
}

/// What a successful install did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed<F> {
    /// The database backups made first, as (database, backup).
    pub backups: Vec<(PathBuf, PathBuf)>,
    /// What the new server reported when it was ready.
    pub facts: F,
}

/// A file moved aside, for the rollback.
#[derive(Debug)]
struct Moved {
    installed: PathBuf,
    aside: Option<PathBuf>,
}

/// A free name to move `file` aside to: `<file>.previous-v<from>` (with `.2`, `.3`, ... when that
/// name is taken by a file that cannot be removed, such as a still-running executable).
#[must_use]
pub fn aside_name(file: &Path, from: &Version) -> PathBuf {
    let base = format!(
        "{}.previous-v{from}",
        file.file_name().unwrap_or_default().to_string_lossy()
    );
    let dir = file.parent().unwrap_or(Path::new("."));
    let mut candidate = dir.join(&base);
    let mut n = 2;
    while candidate.exists() && std::fs::remove_file(&candidate).is_err() {
        candidate = dir.join(format!("{base}.{n}"));
        n += 1;
    }
    candidate
}

/// Removes the files earlier updates moved aside in `dir` whose names start with `prefix` (those
/// that can be removed).
fn remove_old_asides(dir: &Path, prefix: &str) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.filter_map(Result::ok) {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with(prefix) && name.contains(".previous-v") && e.path().is_file() {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

/// Moves `installed` aside (when it exists) and copies `new` in its place.
fn replace(
    installed: &Path,
    new: &Path,
    from: &Version,
    moved: &mut Vec<Moved>,
) -> Result<(), String> {
    let aside = if installed.exists() {
        let aside = aside_name(installed, from);
        std::fs::rename(installed, &aside).map_err(|e| {
            format!(
                "{} cannot be moved aside ({e}); is the folder writable by the server's account?",
                installed.display()
            )
        })?;
        Some(aside)
    } else {
        None
    };
    moved.push(Moved {
        installed: installed.to_path_buf(),
        aside,
    });
    std::fs::copy(new, installed).map_err(|e| format!("{}: {e}", installed.display()))?;
    Ok(())
}

/// Puts back what [`replace`] moved, newest first. What could not be put back.
fn put_back(moved: Vec<Moved>) -> Vec<String> {
    let mut problems = Vec::new();
    for m in moved.into_iter().rev() {
        match std::fs::remove_file(&m.installed) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => problems.push(format!("removing {}: {e}", m.installed.display())),
        }
        if let Some(aside) = m.aside {
            if let Err(e) = std::fs::rename(&aside, &m.installed) {
                problems.push(format!(
                    "moving {} back to {}: {e}",
                    aside.display(),
                    m.installed.display()
                ));
            }
        }
    }
    problems
}

/// Installs `staged` over the stopped server at `layout`, which runs `from`, as the module
/// documentation describes.
///
/// # Errors
/// Why it failed; everything was rolled back (the message says what could not be).
pub fn apply<P: Product>(
    layout: &Layout<P>,
    staged: &Staged<P::Facts>,
    from: &Version,
    build: &P::Build,
) -> Result<Installed<P::Facts>, String> {
    let to = &staged.version;
    let label = format!("update-v{from}-v{to}");
    let mut backups = Vec::new();
    for db in &layout.databases {
        if !db.is_file() {
            continue;
        }
        let backup = P::backup(db, &label).map_err(|e| {
            format!(
                "backing up {} failed, so nothing was installed: {e}",
                db.display()
            )
        })?;
        tracing::info!("Update: backed up {} to {}", db.display(), backup.display());
        backups.push((db.clone(), backup));
    }

    remove_old_asides(&layout.install_dir, "");
    if let (Some(dir), Some(name)) = (layout.world_pack.parent(), layout.world_pack.file_name()) {
        remove_old_asides(dir, &name.to_string_lossy());
    }
    let mut moved = Vec::new();
    let executables = P::executables();
    let installed = (|| -> Result<P::Facts, String> {
        for name in &staged.files {
            let dest = layout.install_dir.join(name);
            replace(&dest, &staged.dir.join(name), from, &mut moved)?;
            if executables.contains(name) {
                archive::make_executable(&dest)?;
            }
        }
        if let Some(pack) = &staged.world_pack {
            replace(&layout.world_pack, pack, from, &mut moved)?;
        }
        tracing::info!(
            "Update: {to} installed in {}; starting it for its health check",
            layout.install_dir.display()
        );
        let facts = P::trial(build, &layout.server_exe())?;
        if facts.version() != to.to_string() {
            return Err(format!(
                "the server that came up reports version {}, not {to}",
                facts.version()
            ));
        }
        Ok(facts)
    })();

    match installed {
        Ok(facts) => {
            let mut state = State::load(&layout.work_dir);
            state.failed.clear();
            let _ = state.save(&layout.work_dir);
            tracing::info!("Update: {to} passed its health check");
            Ok(Installed { backups, facts })
        }
        Err(reason) => {
            tracing::error!("Update: {to} failed: {reason}. Rolling back to {from}");
            let mut problems = put_back(moved);
            for (db, backup) in &backups {
                match P::restore(backup, db) {
                    Ok(()) => tracing::info!(
                        "Update: restored {} from {}",
                        db.display(),
                        backup.display()
                    ),
                    Err(e) => problems.push(format!(
                        "restoring {} from {}: {e}",
                        db.display(),
                        backup.display()
                    )),
                }
            }
            let mut state = State::load(&layout.work_dir);
            state.failed.retain(|f| f.version != to.to_string());
            state.failed.push(Failure {
                version: to.to_string(),
                reason: reason.clone(),
            });
            if let Err(e) = state.save(&layout.work_dir) {
                problems.push(format!("recording the failure: {e}"));
            }
            if problems.is_empty() {
                tracing::info!("Update: rolled back to {from}");
                Err(format!("{reason}; rolled back to {from}"))
            } else {
                for p in &problems {
                    tracing::error!("Update: the rollback could not finish: {p}");
                }
                Err(format!(
                    "{reason}; the rollback to {from} could not finish: {}",
                    problems.join("; ")
                ))
            }
        }
    }
}
