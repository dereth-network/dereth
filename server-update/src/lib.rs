//! A game server's updater: finding, checking and installing its own releases within what its
//! operator allows, and undoing one that does not come up.
//!
//! **Depends on** an HTTP client (`ureq`, over rustls), the archive readers (`zip`, `flate2`) and
//! `sha2`, `serde`, `serde_json` and `tracing`. **Used by** the game servers: `empyrean-server` and
//! `yalain`, each giving it what is its own as a [`Product`].
//!
//! **Must never** know one server's databases, world data or configuration file: those come through
//! [`Product`] and the facts and declaration types it names. It holds no game, and it never decides
//! on anything but what a release declares and the operator's policy.
//!
//! The steps:
//!
//! 1. **Find** ([`Updater::check`]): the GitHub releases of the repository the build names as its
//!    source ([`source`]); each newer release's `release.json` says what upgrading to it involves
//!    ([`release::UpgradeDeclaration`]). The operator's policy picks one or none ([`policy`]).
//! 2. **Stage** ([`Updater::stage`]), while the server keeps running: download this platform's
//!    archive and check it against `SHA256SUMS` and `release.json` ([`verify`]); unpack it
//!    ([`archive`]); run its server binary once to confirm it runs here and is what the release
//!    declares; prepare the world data the release needs ([`Product::prepare_world`]).
//! 3. **Install** ([`install::apply`]), with the server stopped: back up the databases, swap the
//!    files, start the new server for a health check, and roll everything back when it fails.
//!
//! The running server does all three by itself when `server.update` allows ([`running`]: warning
//! players with the shutdown countdown between 2 and 3, and starting the new or restored server
//! afterwards, [`handover`]); the server's `update --check` shows what step 1 decides, and its
//! `update --apply` runs all three by hand on a stopped server ([`command`]).

pub mod archive;
pub mod command;
pub mod handover;
pub mod install;
pub mod policy;
pub mod release;
pub mod running;
pub mod source;
#[cfg(feature = "testing")]
pub mod testing;
pub mod verify;
pub mod version;

use std::fmt::Debug;
use std::path::{Path, PathBuf};

use install::{Layout, Staged, State};
use policy::{Candidate, Needs, Policy, Selection, Situation};
use release::{BuildFacts, ReleaseIndex, UpgradeDeclaration};
use source::{Published, Source};
use version::Version;

/// What one game server gives the updater.
pub trait Product: Clone + Debug + Eq + Send + Sync + Sized + 'static {
    /// What its builds say they are (`--release-facts`).
    type Facts: BuildFacts;
    /// The `upgrade` object of its `release.json`.
    type Declaration: UpgradeDeclaration<Self::Facts>;
    /// What runs a release's programs: the real processes, or a stand-in.
    type Build: ?Sized + Debug + Send + Sync;
    /// What its world data is rebuilt from, when it has any.
    type WorldInputs: Debug + Send + Sync;

    /// Its name in what the updater says (`Empyrean`).
    const NAME: &'static str;
    /// What its release tags begin with (`empyrean-v`).
    const TAG_PREFIX: &'static str;
    /// The folder, under the installation, where the updater keeps its downloads and its record.
    const WORK_DIR: &'static str;

    /// The server binary's file name on this platform.
    fn server_file_name() -> String;
    /// The programs a release must hold, marked executable when they are installed.
    fn executables() -> Vec<String>;
    /// What the server binary `exe` says it is.
    ///
    /// # Errors
    /// When it does not run or answers something else.
    fn facts(build: &Self::Build, exe: &Path) -> Result<Self::Facts, String>;
    /// Starts the installed server `exe`, waits until it is ready, and stops it: the health check.
    /// What it reported when ready.
    ///
    /// # Errors
    /// When it does not come up.
    fn trial(build: &Self::Build, exe: &Path) -> Result<Self::Facts, String>;
    /// Gets the world data ready for the release of `version` unpacked in `dir`, with the server
    /// still running. The world pack to install with it, when there is one.
    ///
    /// # Errors
    /// When it cannot be got ready; nothing is installed.
    fn prepare_world(
        updater: &Updater<Self>,
        dir: &Path,
        version: &Version,
        needs: &Needs,
    ) -> Result<Option<PathBuf>, String>;
    /// Backs up the database file `db`, keeping the store's newest few; the backup's path.
    ///
    /// # Errors
    /// When it cannot be copied; no backup is left behind.
    fn backup(db: &Path, label: &str) -> Result<PathBuf, String>;
    /// Puts the backup back as the database file `db`, which must not be open.
    ///
    /// # Errors
    /// When a file cannot be removed or copied.
    fn restore(backup: &Path, db: &Path) -> std::io::Result<()>;
}

/// A newer release, and what its `release.json` said.
#[derive(Debug, Clone)]
pub struct Found<D> {
    pub published: Published,
    /// Its `release.json`, when it was read.
    pub index: Option<ReleaseIndex<D>>,
    /// The bytes of its `release.json`, checked against `SHA256SUMS` when it is staged.
    pub index_bytes: Option<Vec<u8>>,
    /// Why its `release.json` was not read.
    pub index_error: Option<String>,
}

/// What [`Updater::check`] found and decided.
#[derive(Debug, Clone)]
pub struct Check<D> {
    pub policy: Policy,
    pub found: Vec<Found<D>>,
    pub selection: Selection,
}

/// The updater of one installation.
#[derive(Debug)]
pub struct Updater<P: Product> {
    pub source: Source,
    pub agent: ureq::Agent,
    /// This build.
    pub mine: P::Facts,
    pub layout: Layout<P>,
    pub build: Box<P::Build>,
    /// What `world.pack` is rebuilt from, when the configuration says.
    pub pack_inputs: Option<P::WorldInputs>,
}

impl<P: Product> Updater<P> {
    /// This build's version.
    ///
    /// # Errors
    /// When the build's version is not a version.
    pub fn current(&self) -> Result<Version, String> {
        Version::parse(self.mine.version())
    }

    /// Finds the newer releases and decides which, if any, `policy` takes.
    ///
    /// # Errors
    /// When the releases cannot be listed.
    pub fn check(&self, policy: Policy) -> Result<Check<P::Declaration>, String> {
        let current = self.current()?;
        let mut found = Vec::new();
        for published in source::list(&self.agent, &self.source, P::TAG_PREFIX, &current)? {
            if published.version <= current {
                continue;
            }
            // Only a release the policy could take needs its declaration read.
            let wanted = !published.prerelease
                && !published.version.is_prerelease()
                && published.version.major == current.major;
            let mut f = Found {
                published,
                index: None,
                index_bytes: None,
                index_error: None,
            };
            if wanted {
                match f.published.asset("release.json") {
                    None => f.index_error = Some("it has no release.json".to_owned()),
                    Some(url) => match source::download(&self.agent, url) {
                        Err(e) => f.index_error = Some(e),
                        Ok(bytes) => match ReleaseIndex::parse(&String::from_utf8_lossy(&bytes)) {
                            Ok(index) => {
                                f.index = Some(index);
                                f.index_bytes = Some(bytes);
                            }
                            Err(e) => f.index_error = Some(e),
                        },
                    },
                }
            }
            found.push(f);
        }
        let candidates: Vec<Candidate<P::Declaration>> = found
            .iter()
            .map(|f| Candidate {
                version: f.published.version.clone(),
                prerelease: f.published.prerelease,
                declaration: f.index.as_ref().and_then(|i| i.upgrade.clone()),
            })
            .collect();
        let failed = State::load(&self.layout.work_dir).failed_versions();
        let selection = policy::select(
            &Situation {
                mine: &self.mine,
                policy,
                failed: &failed,
                can_rebuild_world_pack: self.pack_inputs.is_some(),
            },
            &candidates,
        );
        Ok(Check {
            policy,
            found,
            selection,
        })
    }

    /// Downloads, checks and unpacks the release `check` takes, and gets its world data ready.
    /// `None` when `check` takes nothing.
    ///
    /// # Errors
    /// When the release fails a check or cannot be staged; nothing is installed.
    pub fn stage(&self, check: &Check<P::Declaration>) -> Result<Option<Staged<P::Facts>>, String> {
        let Some((version, needs)) = &check.selection.take else {
            return Ok(None);
        };
        let found = check
            .found
            .iter()
            .find(|f| &f.published.version == version)
            .ok_or("the release taken is not among those found")?;
        let (Some(index), Some(index_bytes)) = (&found.index, &found.index_bytes) else {
            return Err(format!("{version}: its release.json was not read"));
        };
        let target = self.mine.target();
        let asset = index.asset_for(target).ok_or_else(|| {
            format!("{version} has no archive for {target}, the target this server was built for")
        })?;

        // The checksums: release.json and the archive.
        let sums_url = found
            .published
            .asset("SHA256SUMS")
            .ok_or_else(|| format!("{version} publishes no SHA256SUMS: refused"))?;
        let sums = verify::parse_sums(&String::from_utf8_lossy(&source::download(
            &self.agent,
            sums_url,
        )?));
        verify::check_sum("release.json", index_bytes, &sums)?;
        let archive_url = found
            .published
            .asset(&asset.file)
            .ok_or_else(|| format!("{version}: {} is not among its downloads", asset.file))?;
        tracing::info!("Update: downloading {archive_url}");
        let bytes = source::download(&self.agent, archive_url)?;
        let sha = verify::check_sum(&asset.file, &bytes, &sums)?;
        if sha != asset.sha256.to_ascii_lowercase() {
            return Err(format!(
                "{}: SHA-256 {sha} is not the {} release.json lists: refused",
                asset.file, asset.sha256
            ));
        }
        tracing::info!(
            "Update: {version}: {} (SHA-256 {sha}) and release.json match SHA256SUMS; checked by SHA-256 only (releases are not signed)",
            asset.file
        );

        let root = asset
            .file
            .strip_suffix(".zip")
            .or_else(|| asset.file.strip_suffix(".tar.gz"))
            .unwrap_or(&asset.file);
        let files = archive::read(&asset.file, &bytes, root)?;
        for needed in P::executables() {
            if !files.iter().any(|f| f.name == needed) {
                return Err(format!("{}: it holds no {needed}: refused", asset.file));
            }
        }
        let dir = self
            .layout
            .work_dir
            .join("staged")
            .join(version.to_string());
        if dir.exists() {
            std::fs::remove_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        archive::write(&files, &dir)?;

        // The new binary runs here and is what the release declares.
        let facts = P::facts(&self.build, &dir.join(P::server_file_name()))?;
        if facts.version() != version.to_string() {
            return Err(format!(
                "{}'s server reports version {}, not {version}: refused",
                asset.file,
                facts.version()
            ));
        }
        if let Some(d) = &index.upgrade {
            if let Some(mismatch) = d.mismatch(&facts) {
                return Err(format!("{version}'s {mismatch}: refused"));
            }
        }

        let world_pack = P::prepare_world(self, &dir, version, needs)?;

        Ok(Some(Staged {
            version: version.clone(),
            dir,
            files: files.into_iter().map(|f| f.name).collect(),
            facts,
            world_pack,
            needs: needs.describe(),
        }))
    }

    /// Installs `staged` over the stopped server ([`install::apply`]).
    ///
    /// # Errors
    /// As [`install::apply`]; everything was rolled back.
    pub fn install(
        &self,
        staged: &Staged<P::Facts>,
    ) -> Result<install::Installed<P::Facts>, String> {
        install::apply::<P>(&self.layout, staged, &self.current()?, &self.build)
    }
}

/// How long the new server's health check may take: to open and migrate its databases, load
/// its world, bind its listeners and stop again.
pub const HEALTH_CHECK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15 * 60);

/// The lines `update --check` prints, and the running server logs, for `check`.
#[must_use]
pub fn describe<P: Product>(mine: &P::Facts, check: &Check<P::Declaration>) -> Vec<String> {
    let mut lines = vec![format!(
        "This server: {} {} for {}; server.update = \"{}\"",
        P::NAME,
        mine.version(),
        mine.target(),
        check.policy.name()
    )];
    if check.found.is_empty() {
        lines.push("No newer release is published.".to_owned());
    }
    for (v, why) in &check.selection.reported {
        let extra = check
            .found
            .iter()
            .find(|f| &f.published.version == v)
            .and_then(|f| f.index_error.as_ref())
            .map(|e| format!(" ({e})"))
            .unwrap_or_default();
        lines.push(format!("{v}: not installed: {why}{extra}"));
    }
    match &check.selection.take {
        Some((v, needs)) => {
            lines.push(format!("{v}: would be installed:"));
            lines.extend(needs.describe().into_iter().map(|l| format!("  - {l}")));
        }
        None => lines.push("Nothing would be installed.".to_owned()),
    }
    lines
}
