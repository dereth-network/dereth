//! A product whose release is one program, which installs a newer release over its own binary on
//! its operator's command: the network's hub (`self-update`). The game servers' updater
//! ([`crate::Updater`]) reads a release's `release.json` and starts the new server for a health
//! check; a product here has neither, and its new binary is checked by running it.
//!
//! 1. **Find** ([`Updater::check`]): the releases of the source, tagged with the product's prefix.
//!    The policy picks the newest it allows ([`select`]): `patch` takes releases of this build's
//!    major and minor version, `minor` those of its major version. A major release, a pre-release,
//!    one without an archive for this build's target or without `SHA256SUMS`, and one that already
//!    failed here are reported and never installed.
//! 2. **Install** ([`Updater::apply`]): download this target's archive and `SHA256SUMS`, and refuse
//!    the archive unless its SHA-256 is the one listed (nothing is signed, so this catches a
//!    damaged or altered download, not a release published by someone else); unpack the binary
//!    beside the installed one; run it, which must name the release's version, and let the
//!    product check it further ([`Product::accept`]); let the product keep a copy of what the
//!    release may change ([`Product::back_up`]); then move the installed binary aside as
//!    `<name>.previous-v<old>` and the new one into its place.
//!
//! A new binary that does not run here, names another version, or that the product refuses for
//! good is recorded as failed, in a file beside the binary ([`Product::STATE_FILE`]), and not tried
//! again until that file is removed or a newer release installs.
//!
//! The updater replaces the file it runs from, so it needs write access to that file's folder; it
//! never starts or stops the program.

use std::fmt::Debug;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::archive;
use crate::install::{aside_name, remove_old_asides, Failure, State};
pub use crate::policy::Policy;
use crate::source::{self, Published, Source};
use crate::verify;
use crate::version::Version;

/// How long a new binary may take to answer one question (`--version`).
pub const RUN_TIMEOUT: Duration = Duration::from_secs(30);

/// What one such product gives the updater.
pub trait Product: Debug {
    /// The program's name: its archives' and its binary's (`dereth-network-hub`).
    const PROGRAM: &'static str;
    /// What its release tags begin with (`network-hub-v`).
    const TAG_PREFIX: &'static str;
    /// The command that updates it, as its messages name it (`self-update`).
    const COMMAND: &'static str;
    /// The option that names the policy, as its messages name it (`--policy`).
    const POLICY_OPTION: &'static str;
    /// The file beside the binary that records the releases that failed here.
    const STATE_FILE: &'static str;
    /// The file the updater writes and removes to learn that it may write in the binary's folder.
    const WRITE_CHECK_FILE: &'static str;

    /// Why [`Updater::apply`] would refuse before downloading anything, beyond a target no
    /// archive is built for and a folder this account cannot write.
    fn blocked(&self) -> Option<String> {
        None
    }

    /// Checks the new binary `exe`, which names itself release `version`, before anything is
    /// changed: that it suits this installation under `policy`.
    ///
    /// # Errors
    /// Why not, and whether that is recorded as the release failing here.
    fn accept(
        &self,
        runner: &dyn Runner,
        exe: &Path,
        version: &Version,
        policy: Policy,
    ) -> Result<(), Refusal>;

    /// Keeps a copy of what the release may change, before the binary is replaced, as the update
    /// from `from` to `to`. Where the copy is.
    ///
    /// # Errors
    /// When the copy cannot be made; nothing is installed.
    fn back_up(
        &self,
        from: &Version,
        to: &Version,
        say: &mut dyn FnMut(&str),
    ) -> Result<PathBuf, String>;
}

/// Why [`Product::accept`] refused a new binary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub reason: String,
    /// The release failed here, and is not taken again.
    pub record: bool,
}

// ---------------------------------------------------------------------------------------------
// Targets and archives

/// The target a release archive is built for, for a binary of `arch`, `os` and `env` (as
/// `std::env::consts` and `cfg(target_env)` name them); `None` where no archive is built.
#[must_use]
pub fn target_for(arch: &str, os: &str, env: &str) -> Option<&'static str> {
    match (arch, os, env) {
        ("x86_64", "linux", "gnu") => Some("x86_64-unknown-linux-gnu"),
        ("aarch64", "linux", "gnu") => Some("aarch64-unknown-linux-gnu"),
        ("x86_64", "windows", "msvc") => Some("x86_64-pc-windows-msvc"),
        ("aarch64", "macos", _) => Some("aarch64-apple-darwin"),
        _ => None,
    }
}

/// The target this binary was built for, as release archives name it.
#[must_use]
pub fn this_target() -> Option<&'static str> {
    let env = if cfg!(target_env = "gnu") {
        "gnu"
    } else if cfg!(target_env = "msvc") {
        "msvc"
    } else if cfg!(target_env = "musl") {
        "musl"
    } else {
        ""
    };
    target_for(std::env::consts::ARCH, std::env::consts::OS, env)
}

/// The release archive of `program` for `target`: `<program>-<version>-<target>.zip` on Windows,
/// `.tar.gz` elsewhere.
#[must_use]
pub fn archive_name(program: &str, version: &Version, target: &str) -> String {
    let ext = if target.contains("windows") {
        "zip"
    } else {
        "tar.gz"
    };
    format!("{program}-{version}-{target}.{ext}")
}

/// The binary's name in a release of `program` for `target`.
#[must_use]
pub fn binary_name(program: &str, target: &str) -> String {
    if target.contains("windows") {
        format!("{program}.exe")
    } else {
        program.to_owned()
    }
}

/// The binary `binary` in the archive `archive` (named as [`archive_name`] names it): the file of
/// that name in the archive's folder, which must hold plain files only ([`archive::read`]).
///
/// # Errors
/// When the archive does not read, or holds no such file.
pub fn binary_from_archive(archive: &str, bytes: &[u8], binary: &str) -> Result<Vec<u8>, String> {
    let root = archive
        .strip_suffix(".zip")
        .or_else(|| archive.strip_suffix(".tar.gz"))
        .ok_or_else(|| format!("{archive} is neither a .zip nor a .tar.gz"))?;
    archive::read(archive, bytes, root)?
        .into_iter()
        .find(|f| f.name == binary)
        .map(|f| f.bytes)
        .ok_or_else(|| format!("{archive} holds no {root}/{binary}: refused"))
}

// ---------------------------------------------------------------------------------------------
// Where releases come from

/// What is being fetched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fetching {
    /// The releases API's list.
    List,
    /// A release's file.
    Download,
}

/// Reads a URL: the body of a `200` answer.
pub trait Fetcher: Debug {
    /// The body `url` answers with.
    ///
    /// # Errors
    /// When it does not answer `200`, or answers more than is read.
    fn get(&self, url: &str, what: Fetching) -> Result<Vec<u8>, String>;

    /// A page of the releases API's list: its body, and the next page's address when the answer's
    /// `Link` header names one ([`source::next_page`]). A fetcher that reads no headers names none.
    ///
    /// # Errors
    /// As [`Fetcher::get`].
    fn list_page(&self, url: &str) -> Result<(Vec<u8>, Option<String>), String> {
        Ok((self.get(url, Fetching::List)?, None))
    }
}

/// [`Fetcher`] over HTTPS (or plain HTTP, for a mirror the operator names), the agent the game
/// servers' updater uses.
#[derive(Debug, Clone)]
pub struct Https(pub ureq::Agent);

impl Https {
    /// An agent naming itself `user_agent`.
    #[must_use]
    pub fn new(user_agent: &str) -> Self {
        Self(source::agent(user_agent))
    }
}

impl Fetcher for Https {
    fn get(&self, url: &str, what: Fetching) -> Result<Vec<u8>, String> {
        match what {
            Fetching::Download => source::download(&self.0, url),
            Fetching::List => self.list_page(url).map(|(body, _)| body),
        }
    }

    fn list_page(&self, url: &str) -> Result<(Vec<u8>, Option<String>), String> {
        source::list_page(&self.0, url).map(|(text, next)| (text.into_bytes(), next))
    }
}

/// The releases of `source` tagged `prefix`, read a page at a time until a page holds one at or
/// below `current` ([`source::list_pages`]).
///
/// # Errors
/// When a page cannot be fetched or read.
pub fn list_releases(
    fetcher: &dyn Fetcher,
    source: &Source,
    prefix: &str,
    current: &Version,
) -> Result<Vec<Published>, String> {
    source::list_pages(source, prefix, current, &mut |url| {
        fetcher
            .list_page(url)
            .map(|(body, next)| (String::from_utf8_lossy(&body).into_owned(), next))
    })
}

// ---------------------------------------------------------------------------------------------
// Running a new binary

/// Runs a binary once with one argument.
pub trait Runner: Debug {
    /// What `exe arg` prints, trimmed.
    ///
    /// # Errors
    /// When it does not run, fails, or takes too long.
    fn output(&self, exe: &Path, arg: &str) -> Result<String, String>;
}

/// [`Runner`] by starting the program, with its input closed, for at most [`RUN_TIMEOUT`].
#[derive(Debug, Clone, Copy, Default)]
pub struct Processes;

impl Runner for Processes {
    fn output(&self, exe: &Path, arg: &str) -> Result<String, String> {
        let mut child = Command::new(exe)
            .arg(arg)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("{} does not run here: {e}", exe.display()))?;
        let out = child.stdout.take();
        let reader = std::thread::spawn(move || {
            let mut text = Vec::new();
            if let Some(out) = out {
                let _ = out.take(64 * 1024).read_to_end(&mut text);
            }
            text
        });
        let end = Instant::now() + RUN_TIMEOUT;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if Instant::now() >= end => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!(
                        "{} {arg} did not finish within {} s",
                        exe.display(),
                        RUN_TIMEOUT.as_secs()
                    ));
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(20)),
                Err(e) => return Err(format!("waiting for {}: {e}", exe.display())),
            }
        };
        let text = reader.join().unwrap_or_default();
        if !status.success() {
            return Err(format!("{} {arg} failed ({status})", exe.display()));
        }
        Ok(String::from_utf8_lossy(&text).trim().to_owned())
    }
}

// ---------------------------------------------------------------------------------------------
// Choosing a release

/// The policy's decision.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selection {
    /// The release to install.
    pub take: Option<Version>,
    /// Every other newer release, oldest first, with why it is not installed.
    pub reported: Vec<(Version, String)>,
}

/// Chooses among `found` (any order; those not newer than `current` are ignored) the newest
/// release of `P` that `policy` takes on a binary running `current` built for `target` (`None`
/// when no archive is built for it), where the releases `failed` failed before.
#[must_use]
pub fn select<P: Product>(
    current: &Version,
    target: Option<&str>,
    policy: Policy,
    failed: &[Failure],
    found: &[Published],
) -> Selection {
    let mut newer: Vec<&Published> = found.iter().filter(|p| p.version > *current).collect();
    newer.sort_by(|a, b| a.version.cmp(&b.version));
    newer.dedup_by(|a, b| a.version == b.version);
    let mut eligible = Vec::new();
    let mut reported = Vec::new();
    for p in newer {
        match verdict::<P>(current, target, policy, failed, p) {
            Ok(()) => eligible.push(p.version.clone()),
            Err(why) => reported.push((p.version.clone(), why)),
        }
    }
    let take = eligible.pop();
    if let Some(newest) = &take {
        for v in eligible {
            reported.push((v, format!("{newest} is newer and is taken instead")));
        }
    }
    reported.sort_by(|a, b| a.0.cmp(&b.0));
    Selection { take, reported }
}

fn verdict<P: Product>(
    current: &Version,
    target: Option<&str>,
    policy: Policy,
    failed: &[Failure],
    p: &Published,
) -> Result<(), String> {
    let v = &p.version;
    let (command, option) = (P::COMMAND, P::POLICY_OPTION);
    if p.prerelease || v.is_prerelease() {
        return Err(format!("a pre-release: {command} never installs one"));
    }
    if v.major != current.major {
        return Err(format!(
            "a major release: {command} never installs one; install it by hand"
        ));
    }
    match policy {
        Policy::Off => return Err(format!("{option} off")),
        Policy::Patch if v.minor != current.minor => {
            return Err(format!(
                "a minor release: {option} patch takes only {}.{}.x",
                current.major, current.minor
            ))
        }
        _ => {}
    }
    if let Some(f) = failed.iter().find(|f| f.version == v.to_string()) {
        return Err(format!("it failed here before: {}", f.reason));
    }
    if let Some(t) = target {
        let archive = archive_name(P::PROGRAM, v, t);
        if p.asset(&archive).is_none() {
            return Err(format!("it publishes no {archive}"));
        }
    }
    if p.asset("SHA256SUMS").is_none() {
        return Err("it publishes no SHA256SUMS".to_owned());
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// The updater

/// What [`Updater::check`] found and decided.
#[derive(Debug, Clone)]
pub struct Check {
    pub policy: Policy,
    /// The newer releases.
    pub found: Vec<Published>,
    pub selection: Selection,
    /// Why [`Updater::apply`] would refuse before downloading anything, if it would.
    pub blocked: Option<String>,
}

/// What [`Updater::apply`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    pub version: Version,
    /// The archive and its SHA-256.
    pub archive: String,
    pub sha256: String,
    /// The product's copy ([`Product::back_up`]), made before the binary was replaced.
    pub backup: PathBuf,
    /// The binary that was replaced, moved aside.
    pub previous: PathBuf,
}

/// The updater of one installed binary of `P`.
#[derive(Debug)]
pub struct Updater<P: Product> {
    pub source: Source,
    pub fetcher: Box<dyn Fetcher>,
    pub runner: Box<dyn Runner>,
    /// The installed binary's version.
    pub current: Version,
    /// The target it was built for (`None` where no archive is built).
    pub target: Option<String>,
    /// The binary: the file replaced.
    pub binary: PathBuf,
    pub product: P,
}

impl<P: Product> Updater<P> {
    /// The binary's folder.
    #[must_use]
    pub fn folder(&self) -> PathBuf {
        match self.binary.parent() {
            Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
            _ => PathBuf::from("."),
        }
    }

    /// The updater's record ([`Product::STATE_FILE`] in the binary's folder).
    #[must_use]
    pub fn state_file(&self) -> PathBuf {
        self.folder().join(P::STATE_FILE)
    }

    /// Why [`Updater::apply`] would refuse before downloading anything: no archive for this
    /// target, a binary's folder this account cannot write, or the product's own reason.
    #[must_use]
    pub fn blocked(&self) -> Option<String> {
        if self.target.is_none() {
            return Some(format!(
                "no release archive is built for {}-{}",
                std::env::consts::ARCH,
                std::env::consts::OS
            ));
        }
        if let Err(e) = writable(&self.folder(), P::WRITE_CHECK_FILE, P::COMMAND) {
            return Some(e);
        }
        self.product.blocked()
    }

    /// Lists the newer releases and decides which, if any, `policy` takes.
    ///
    /// # Errors
    /// When the releases cannot be listed.
    pub fn check(&self, policy: Policy) -> Result<Check, String> {
        let all = list_releases(
            self.fetcher.as_ref(),
            &self.source,
            P::TAG_PREFIX,
            &self.current,
        )?;
        let found: Vec<Published> = all
            .into_iter()
            .filter(|p| p.version > self.current)
            .collect();
        let failed = State::load_file(&self.state_file()).failed;
        let selection = select::<P>(
            &self.current,
            self.target.as_deref(),
            policy,
            &failed,
            &found,
        );
        Ok(Check {
            policy,
            found,
            selection,
            blocked: self.blocked(),
        })
    }

    /// Installs the release `check` takes, as the module documentation describes. `None` when it
    /// takes none. `say` is told each step as it starts.
    ///
    /// # Errors
    /// Why nothing was installed (or, when moving the new binary in failed, what was put back).
    pub fn apply(
        &self,
        check: &Check,
        say: &mut dyn FnMut(&str),
    ) -> Result<Option<Installed>, String> {
        let Some(version) = &check.selection.take else {
            return Ok(None);
        };
        if let Some(why) = self.blocked() {
            return Err(format!("{why}: nothing was installed"));
        }
        let target = self.target.as_deref().unwrap_or_default();
        let release = check
            .found
            .iter()
            .find(|p| &p.version == version)
            .ok_or("the release taken is not among those found")?;
        let archive = archive_name(P::PROGRAM, version, target);
        let (Some(sums_url), Some(archive_url)) =
            (release.asset("SHA256SUMS"), release.asset(&archive))
        else {
            return Err(format!(
                "{version} publishes no {archive} or no SHA256SUMS: refused"
            ));
        };

        say(&format!("Downloading {archive_url}"));
        let sums = verify::parse_sums(&String::from_utf8_lossy(
            &self.fetcher.get(sums_url, Fetching::Download)?,
        ));
        let bytes = self.fetcher.get(archive_url, Fetching::Download)?;
        let sha256 = verify::check_sum(&archive, &bytes, &sums)
            .map_err(|e| format!("{e}; checked by SHA-256 only"))?;
        say(&format!(
            "{archive}: its SHA-256 {sha256} is the one SHA256SUMS lists (checked by SHA-256 only: releases are not signed)"
        ));
        let binary = binary_from_archive(&archive, &bytes, &binary_name(P::PROGRAM, target))?;

        // The new binary goes beside the installed one, so that it is moved into place, not
        // copied, and runs here from the folder it will run from. One an interrupted run left
        // there goes first.
        let staged_prefix = format!(".{}.new-v", P::PROGRAM);
        if let Ok(entries) = std::fs::read_dir(self.folder()) {
            for e in entries.filter_map(Result::ok) {
                if e.file_name().to_string_lossy().starts_with(&staged_prefix) {
                    let _ = std::fs::remove_file(e.path());
                }
            }
        }
        let staged = self.folder().join(format!(
            "{staged_prefix}{version}{}",
            std::env::consts::EXE_SUFFIX
        ));
        std::fs::write(&staged, &binary).map_err(|e| format!("{}: {e}", staged.display()))?;
        let refuse = |why: String, record: bool| {
            let _ = std::fs::remove_file(&staged);
            if record {
                let mut state = State::load_file(&self.state_file());
                state.record(version, &why);
                let _ = state.save_file(&self.state_file());
            }
            format!("{why}: nothing was installed")
        };
        if let Err(e) = archive::make_executable(&staged) {
            return Err(refuse(e, false));
        }

        // It runs here, is the release, and suits this installation.
        let program = P::PROGRAM;
        let named = match self.runner.output(&staged, "--version") {
            Ok(text) => text.lines().next().unwrap_or_default().trim().to_owned(),
            Err(e) => return Err(refuse(format!("{version}'s binary: {e}"), true)),
        };
        if named != format!("{program} {version}") {
            return Err(refuse(
                format!("{version}'s binary names itself `{named}`, not `{program} {version}`"),
                true,
            ));
        }
        if let Err(r) = self
            .product
            .accept(self.runner.as_ref(), &staged, version, check.policy)
        {
            return Err(refuse(r.reason, r.record));
        }

        // The product's copy, then the swap.
        let backup = match self.product.back_up(&self.current, version, &mut *say) {
            Ok(b) => b,
            Err(e) => return Err(refuse(e, false)),
        };
        let previous = match self.swap(&staged) {
            Ok(p) => p,
            Err(e) => return Err(refuse(e, false)),
        };
        let mut state = State::load_file(&self.state_file());
        if !state.failed.is_empty() {
            state.failed.clear();
            let _ = state.save_file(&self.state_file());
        }
        Ok(Some(Installed {
            version: version.clone(),
            archive,
            sha256,
            backup,
            previous,
        }))
    }

    /// Moves the installed binary aside as `<name>.previous-v<old>` (removing the ones earlier
    /// updates left, where they can be) and `staged` into its place; puts the old one back when the
    /// new one cannot be moved in. Where the old binary now is.
    fn swap(&self, staged: &Path) -> Result<PathBuf, String> {
        let dir = self.folder();
        let name = self
            .binary
            .file_name()
            .ok_or("the binary has no file name")?
            .to_string_lossy()
            .into_owned();
        remove_old_asides(&dir, &name);
        let previous = aside_name(&self.binary, &self.current);
        std::fs::rename(&self.binary, &previous).map_err(|e| {
            format!(
                "{} cannot be moved aside ({e}): is {} writable by this account?",
                self.binary.display(),
                dir.display()
            )
        })?;
        if let Err(e) = std::fs::rename(staged, &self.binary) {
            let back = std::fs::rename(&previous, &self.binary);
            return Err(match back {
                Ok(()) => format!(
                    "the new binary could not be moved to {} ({e}); the old one is back",
                    self.binary.display()
                ),
                Err(b) => format!(
                    "the new binary could not be moved to {} ({e}), and the old one could not be moved back from {} ({b}): move it back by hand",
                    self.binary.display(),
                    previous.display()
                ),
            });
        }
        Ok(previous)
    }
}

/// Whether this account can make a file in `dir`, by making and removing `probe` there.
///
/// # Errors
/// The refusal, naming the folder and saying to run `command` as the binary's owner.
pub fn writable(dir: &Path, probe: &str, command: &str) -> Result<(), String> {
    let probe = dir.join(probe);
    match std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&probe)
    {
        Ok(f) => {
            drop(f);
            let _ = std::fs::remove_file(&probe);
            Ok(())
        }
        Err(e) => Err(format!(
            "this account cannot write in {}, the folder the binary is in ({e}): run {command} as the account that installed the binary (root, for one in /usr/local/bin)",
            dir.display()
        )),
    }
}

/// The lines saying what `check` found and decided, after the product's own lines about itself:
/// the newer releases, why each is not installed, and the one that would be.
#[must_use]
pub fn describe<P: Product>(updater: &Updater<P>, check: &Check) -> Vec<String> {
    let mut lines = Vec::new();
    if check.found.is_empty() {
        lines.push("No newer release is published.".to_owned());
    }
    let mut failed_any = false;
    for (v, why) in &check.selection.reported {
        failed_any |= why.starts_with("it failed here before");
        lines.push(format!("{v}: not installed: {why}"));
    }
    if failed_any {
        lines.push(format!(
            "A release that failed here is tried again once {} is removed.",
            updater.state_file().display()
        ));
    }
    match &check.selection.take {
        Some(v) => lines.push(format!("{v}: would be installed")),
        None => lines.push("Nothing would be installed.".to_owned()),
    }
    lines
}

#[cfg(test)]
mod tests;
