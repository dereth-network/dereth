//! Installing a staged release over a stopped server, and taking it back out when it fails: the
//! shared transaction ([`dereth_server_update::install`]), with what is Empyrean's: its programs'
//! file names, how a release's programs are run, and the pack rebuild.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use dereth_server_update::install::{process, release_facts, run_trial};

use super::release::Facts;
use super::Empyrean;

pub use dereth_server_update::install::{aside_name, Failure, State};

/// Where the server is installed and what an update touches.
pub type Layout = dereth_server_update::install::Layout<Empyrean>;

/// A release downloaded, checked and unpacked, ready to install.
pub type Staged = dereth_server_update::install::Staged<Facts>;

/// What a successful install did.
pub type Installed = dereth_server_update::install::Installed<Facts>;

/// The server binary's file name on this platform.
#[must_use]
pub fn server_file_name() -> String {
    format!("empyrean-server{}", std::env::consts::EXE_SUFFIX)
}

/// The importer's file name on this platform.
#[must_use]
pub fn import_file_name() -> String {
    format!("empyrean-import{}", std::env::consts::EXE_SUFFIX)
}

/// The folder, under the installation, where the updater keeps its downloads and its record.
pub const WORK_DIR: &str = ".empyrean-update";

/// Installs `staged` over the stopped server at `layout`, which runs `from`, as
/// [`dereth_server_update::install`] describes.
///
/// # Errors
/// Why it failed; everything was rolled back (the message says what could not be).
pub fn apply(
    layout: &Layout,
    staged: &Staged,
    from: &super::version::Version,
    build: &(dyn Build + 'static),
) -> Result<Installed, String> {
    dereth_server_update::install::apply::<Empyrean>(layout, staged, from, build)
}

/// The dump `world.pack` was built from and the patches over it (`server.world_base_sql`,
/// `server.world_base_patches`), which a release that needs a new pack rebuilds it from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackInputs {
    pub sql: PathBuf,
    /// Each patch input, `true` for ACE JSON content (`json:<path>`).
    pub patches: Vec<(bool, PathBuf)>,
}

/// Runs the programs of a release: its facts, its health check, its importer.
pub trait Build: std::fmt::Debug + Send + Sync {
    /// What the server binary `exe` says it is (`--release-facts`).
    ///
    /// # Errors
    /// When it does not run or answers something else.
    fn facts(&self, exe: &Path) -> Result<Facts, String>;

    /// Starts the installed server `exe`, waits until it is ready, and stops it: the health check.
    /// What it reported when ready.
    ///
    /// # Errors
    /// When it does not come up.
    fn trial(&self, exe: &Path) -> Result<Facts, String>;

    /// Builds a world pack at `out` with the importer `importer` from `inputs`.
    ///
    /// # Errors
    /// When the importer fails.
    fn rebuild_pack(&self, importer: &Path, inputs: &PackInputs, out: &Path) -> Result<(), String>;
}

/// [`Build`] by running the programs.
#[derive(Debug, Clone)]
pub struct Processes {
    /// The configuration the server runs with (`--config`), when there is a file.
    pub config: Option<PathBuf>,
    /// The folder the server is started in.
    pub current_dir: PathBuf,
    /// How long the health check may take to come up and stop.
    pub timeout: Duration,
    /// Where the ready report is written.
    pub work_dir: PathBuf,
}

impl Build for Processes {
    fn facts(&self, exe: &Path) -> Result<Facts, String> {
        release_facts(exe)
    }

    fn trial(&self, exe: &Path) -> Result<Facts, String> {
        let mut args = Vec::new();
        if let Some(config) = &self.config {
            args.push("--config".into());
            args.push(config.into());
        }
        run_trial(exe, &args, &self.current_dir, self.timeout, &self.work_dir)
    }

    fn rebuild_pack(&self, importer: &Path, inputs: &PackInputs, out: &Path) -> Result<(), String> {
        let mut command = process(importer);
        command.arg("--sql").arg(&inputs.sql);
        for (json, path) in &inputs.patches {
            command
                .arg(if *json { "--json" } else { "--patches" })
                .arg(path);
        }
        command.arg("--out").arg(out);
        let status = command
            .current_dir(&self.current_dir)
            .stdin(Stdio::null())
            .status()
            .map_err(|e| format!("{} could not be run: {e}", importer.display()))?;
        if status.success() && out.is_file() {
            Ok(())
        } else {
            Err(format!(
                "the new importer could not rebuild world.pack ({status}): its output above says why"
            ))
        }
    }
}
