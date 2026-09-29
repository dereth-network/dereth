//! Not ACE: the server's updater, `empyrean-server update` and `server.update`.
//!
//! ACE only looked for a newer ACE release and logged it. Empyrean installs its own releases,
//! within what the operator allows:
//!
//! 1. **Find** ([`Updater::check`]): the GitHub releases of the repository this build names as
//!    its source ([`source`]); each newer release's `release.json` says what upgrading to it
//!    involves ([`release::Declaration`]). The operator's policy picks one or none
//!    ([`policy`]).
//! 2. **Stage** ([`Updater::stage`]), while the server keeps running: download this platform's
//!    archive and check it against `SHA256SUMS` and `release.json` ([`verify`]); unpack it ([`archive`]); run its server binary once to confirm it runs here and
//!    is what the release declares; rebuild `world.pack` with its importer when it needs one.
//! 3. **Install** ([`install::apply`]), with the server stopped: back up the databases, swap the
//!    files, start the new server for a health check, and roll everything back when it fails.
//!
//! The running server does all three by itself when `server.update` allows (warning players with
//! the shutdown countdown between 2 and 3, and starting the new or restored server afterwards,
//! [`handover`]); `empyrean-server update --check` shows what step 1 decides, and
//! `empyrean-server update --apply` runs all three by hand on a stopped server.

pub mod archive;
pub mod handover;
pub mod install;
pub mod policy;
pub mod release;
pub mod source;
pub mod verify;
pub mod version;

use std::path::{Path, PathBuf};

use empyrean_common::config_paths::PathBase;
use empyrean_common::master_configuration::MasterConfiguration;
use install::{Build, Layout, PackInputs, Staged, State};
use policy::{Candidate, Policy, Selection, Situation};
use release::{Facts, ReleaseIndex};
use source::{Published, Source};
use version::Version;

/// A newer release, and what its `release.json` said.
#[derive(Debug, Clone)]
pub struct Found {
    pub published: Published,
    /// Its `release.json`, when it was read.
    pub index: Option<ReleaseIndex>,
    /// The bytes of its `release.json`, checked against `SHA256SUMS` when it is staged.
    pub index_bytes: Option<Vec<u8>>,
    /// Why its `release.json` was not read.
    pub index_error: Option<String>,
}

/// What [`Updater::check`] found and decided.
#[derive(Debug, Clone)]
pub struct Check {
    pub policy: Policy,
    pub found: Vec<Found>,
    pub selection: Selection,
}

/// The updater of one installation.
#[derive(Debug)]
pub struct Updater {
    pub source: Source,
    pub agent: ureq::Agent,
    /// This build.
    pub mine: Facts,
    pub layout: Layout,
    pub build: Box<dyn Build>,
    /// What `world.pack` is rebuilt from, when the configuration says.
    pub pack_inputs: Option<PackInputs>,
}

impl Updater {
    /// This build's version.
    ///
    /// # Errors
    /// When [`Facts::version`] is not a version.
    pub fn current(&self) -> Result<Version, String> {
        Version::parse(&self.mine.version)
    }

    /// Finds the newer releases and decides which, if any, `policy` takes.
    ///
    /// # Errors
    /// When the releases cannot be listed.
    pub fn check(&self, policy: Policy) -> Result<Check, String> {
        let current = self.current()?;
        let mut found = Vec::new();
        for published in source::list(&self.agent, &self.source)? {
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
        let candidates: Vec<Candidate> = found
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

    /// Downloads, checks and unpacks the release `check` takes, and rebuilds `world.pack` for it
    /// when it needs one. `None` when `check` takes nothing.
    ///
    /// # Errors
    /// When the release fails a check or cannot be staged; nothing is installed.
    pub fn stage(&self, check: &Check) -> Result<Option<Staged>, String> {
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
        let target = &self.mine.target;
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
        log::info!("Update: downloading {archive_url}");
        let bytes = source::download(&self.agent, archive_url)?;
        let sha = verify::check_sum(&asset.file, &bytes, &sums)?;
        if sha != asset.sha256.to_ascii_lowercase() {
            return Err(format!(
                "{}: SHA-256 {sha} is not the {} release.json lists: refused",
                asset.file, asset.sha256
            ));
        }
        log::info!(
            "Update: {version}: {} (SHA-256 {sha}) and release.json match SHA256SUMS; checked by SHA-256 only (releases are not signed)",
            asset.file
        );

        let root = asset
            .file
            .strip_suffix(".zip")
            .or_else(|| asset.file.strip_suffix(".tar.gz"))
            .unwrap_or(&asset.file);
        let files = archive::read(&asset.file, &bytes, root)?;
        for needed in [install::server_file_name(), install::import_file_name()] {
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
        let facts = self.build.facts(&dir.join(install::server_file_name()))?;
        if facts.version != version.to_string() {
            return Err(format!(
                "{}'s server reports version {}, not {version}: refused",
                asset.file, facts.version
            ));
        }
        if let Some(d) = &index.upgrade {
            if facts.databases != d.databases || facts.world_pack != d.world_pack {
                return Err(format!(
                    "{version}'s server reports databases {:?} and world pack {:?}, but its release declares {:?} and {:?}: refused",
                    facts.databases, facts.world_pack, d.databases, d.world_pack
                ));
            }
        }

        let world_pack = if needs.world_pack_rebuild {
            let inputs = self
                .pack_inputs
                .as_ref()
                .ok_or("world.pack must be rebuilt, and no dump is configured")?;
            let out: PathBuf = self
                .layout
                .work_dir
                .join("staged")
                .join(format!("{version}.world.pack"));
            let _ = std::fs::remove_file(&out);
            log::info!(
                "Update: rebuilding world.pack for {version} from {}",
                inputs.sql.display()
            );
            self.build
                .rebuild_pack(&dir.join(install::import_file_name()), inputs, &out)?;
            Some(out)
        } else {
            None
        };

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
    pub fn install(&self, staged: &Staged) -> Result<install::Installed, String> {
        install::apply(&self.layout, staged, &self.current()?, &*self.build)
    }
}

/// How long the new server's health check may take: to open and migrate its databases, load
/// its world, bind its listeners and stop again.
pub const HEALTH_CHECK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15 * 60);

/// The policy `server.update` names.
///
/// # Errors
/// The value, when it is not `off`, `patch` or `minor`.
pub fn configured_policy(config: &MasterConfiguration) -> Result<Policy, String> {
    Policy::parse(&config.server.update).ok_or_else(|| config.server.update.clone())
}

/// The updater of the installation this process runs from, configured by `config` (read from
/// `config_file`, whose folder `paths` resolves against).
///
/// # Errors
/// When the running executable's folder or the build's source repository cannot be told.
pub fn for_this_installation(
    config: &MasterConfiguration,
    paths: &PathBase,
    config_file: Option<&Path>,
) -> Result<Updater, String> {
    let exe = std::env::current_exe()
        .map_err(|e| format!("the server's own executable cannot be found: {e}"))?;
    let install_dir = exe
        .parent()
        .ok_or("the server's executable has no folder")?
        .to_path_buf();
    let layout = Layout::new(
        &install_dir,
        vec![
            crate::database_manager::configured_shard_db_path(config, paths),
            crate::database_manager::configured_auth_db_path(config, paths),
        ],
        crate::world_pack::configured_world_pack_path(config, paths),
    );
    let s = &config.server;
    let pack_inputs = (!s.world_base_sql.trim().is_empty()).then(|| PackInputs {
        sql: paths.resolve(&s.world_base_sql),
        patches: s
            .world_base_patches
            .iter()
            .map(|e| match e.trim().strip_prefix("json:") {
                Some(p) => (true, paths.resolve(p)),
                None => (false, paths.resolve(e)),
            })
            .collect(),
    });
    let build = install::Processes {
        config: config_file.map(|p| std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf())),
        current_dir: std::env::current_dir().unwrap_or_else(|_| install_dir.clone()),
        timeout: HEALTH_CHECK_TIMEOUT,
        work_dir: layout.work_dir.clone(),
    };
    Ok(Updater {
        source: Source::configured(&s.update_source)?,
        agent: source::agent(),
        mine: Facts::this_build(),
        layout,
        build: Box::new(build),
        pack_inputs,
    })
}

/// The lines `update --check` prints, and the running server logs, for `check`.
#[must_use]
pub fn describe(mine: &Facts, check: &Check) -> Vec<String> {
    let mut lines = vec![format!(
        "This server: Empyrean {} for {}; server.update = \"{}\"",
        mine.version,
        mine.target,
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
