//! Not ACE: the server's updater, `empyrean-server update` and `server.update`.
//!
//! ACE only looked for a newer ACE release and logged it. Empyrean installs its own releases,
//! within what the operator allows. The steps are the shared updater's
//! ([`dereth_server_update`]); this is what is Empyrean's: its two databases and its world pack
//! ([`release::Facts`], [`release::Declaration`]), the programs of a release
//! ([`install::Build`]), the pack rebuild, and where the configuration says everything is.

pub mod install;
pub mod managed;
pub mod policy;
pub mod release;
pub mod source;
pub mod version;

pub use dereth_server_update::{archive, handover, verify};
pub use dereth_server_update::{describe, HEALTH_CHECK_TIMEOUT};

use std::path::{Path, PathBuf};

use dereth_server_update::Product;
use empyrean_common::config_paths::PathBase;
use empyrean_common::master_configuration::MasterConfiguration;
use empyrean_store::upgrade::{self, UpgradePolicy};
use install::{Build, Layout, PackInputs};
use policy::{Needs, Policy};
use release::{Declaration, Facts};
use version::Version;

/// Empyrean, as the shared updater knows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Empyrean;

impl Product for Empyrean {
    type Facts = Facts;
    type Declaration = Declaration;
    type Build = dyn Build;
    type WorldInputs = PackInputs;

    const NAME: &'static str = "Empyrean";
    const TAG_PREFIX: &'static str = version::TAG_PREFIX;
    const WORK_DIR: &'static str = install::WORK_DIR;

    fn server_file_name() -> String {
        install::server_file_name()
    }

    fn executables() -> Vec<String> {
        vec![install::server_file_name(), install::import_file_name()]
    }

    fn facts(build: &Self::Build, exe: &Path) -> Result<Facts, String> {
        build.facts(exe)
    }

    fn trial(build: &Self::Build, exe: &Path) -> Result<Facts, String> {
        build.trial(exe)
    }

    fn prepare_world(
        updater: &Updater,
        dir: &Path,
        version: &Version,
        needs: &Needs,
    ) -> Result<Option<PathBuf>, String> {
        if !needs.world_pack_rebuild {
            return Ok(None);
        }
        let inputs = updater
            .pack_inputs
            .as_ref()
            .ok_or("world.pack must be rebuilt, and no dump is configured")?;
        let out: PathBuf = updater
            .layout
            .work_dir
            .join("staged")
            .join(format!("{version}.world.pack"));
        let _ = std::fs::remove_file(&out);
        log::info!(
            "Update: rebuilding world.pack for {version} from {}",
            inputs.sql.display()
        );
        updater
            .build
            .rebuild_pack(&dir.join(install::import_file_name()), inputs, &out)?;
        Ok(Some(out))
    }

    fn backup(db: &Path, label: &str) -> Result<PathBuf, String> {
        upgrade::snapshot(db, label, &UpgradePolicy::default())
    }

    fn restore(backup: &Path, db: &Path) -> std::io::Result<()> {
        upgrade::restore(backup, db)
    }
}

/// A newer release, and what its `release.json` said.
pub type Found = dereth_server_update::Found<Declaration>;

/// What [`Updater::check`] found and decided.
pub type Check = dereth_server_update::Check<Declaration>;

/// The updater of one installation.
pub type Updater = dereth_server_update::Updater<Empyrean>;

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
        source: source::configured(&s.update_source)?,
        agent: source::agent(),
        mine: Facts::this_build(),
        layout,
        build: Box::new(build),
        pack_inputs,
    })
}
