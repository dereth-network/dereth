//! `yalain update`, and the running server's own updates (`server.update`), as Empyrean's: the same
//! command, keys, defaults and steps, from the same updater (`dereth_server_update`). What is
//! Yalain's is here: its database and world database ([`Facts`], [`Declaration`]), how its programs
//! are run ([`Programs`]), and the world's loop answering the updater ([`Ask`]).
//!
//! **While the server runs**, with `server.update` set to `patch` or `minor`, a thread checks for a
//! new release a minute after start-up and then every `server.update_check_hours`. When the policy
//! takes one it is staged (downloaded, checked, unpacked, the world database it serves fetched by
//! its own program) with the world still running, then players get the shutdown countdown of
//! `server.update_warning_seconds`. `cancel-shutdown` cancels the update too (the next check stages
//! it again). When the countdown ends and the world has saved and stopped, the release is installed
//! and health-checked, and the process hands over to the new server, or, when the health check
//! failed, to the restored old one. A server stopped by Ctrl-C or a signal installs nothing.
//!
//! **By hand:** `yalain update --check` prints what would be installed and why each newer release
//! would or would not be; `yalain update --apply` installs it on a stopped server (backup, swap,
//! health check, rollback) and leaves the server stopped for the operator or the service manager to
//! start. Both read the same `yalain.toml` as the server (`--config`); `--policy` overrides
//! `server.update`, and when neither names one, by hand means `minor`.

use std::ffi::OsString;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{ExitCode, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;

use dereth_server_update::command::{self, Request};
use dereth_server_update::handover::{self, ForwardConsole};
use dereth_server_update::install::{process, release_facts, run_trial, Layout};
use dereth_server_update::policy::Needs;
use dereth_server_update::release::{
    target_triple, BuildFacts, ConfigChange, Kind, Migration, UpgradeDeclaration,
};
use dereth_server_update::running::{self, Schedule, World};
use dereth_server_update::source::{self, Source};
use dereth_server_update::version::Version;
use dereth_server_update::{Product, HEALTH_CHECK_TIMEOUT};
use serde::{Deserialize, Serialize};
use yalain_net::ServerNet;
use yalain_store::Store;
use yalain_world::shutdown::{self, Shutdown};
use yalain_world::{Clock, Server};

/// The repository this build's releases come from. A release build sets `YALAIN_BUILD_SOURCE_URL`
/// (at compile time) so that a fork's programs look for the fork's releases.
pub const SOURCE_URL: &str = match option_env!("YALAIN_BUILD_SOURCE_URL") {
    Some(url) => url,
    None => "https://github.com/dereth-network/dereth",
};

/// The program's file name on this platform.
fn program_file_name() -> String {
    format!("yalain{}", std::env::consts::EXE_SUFFIX)
}

/// The schema version a build brings its database to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Databases {
    pub store: i64,
}

/// What a build is, as far as an upgrade is concerned: what `yalain --release-facts` prints.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Facts {
    pub version: String,
    pub target: String,
    pub databases: Databases,
    /// The world database release it serves by default.
    pub world_database: String,
}

impl Facts {
    /// This build's facts.
    #[must_use]
    pub fn this_build() -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION").to_owned(),
            target: target_triple(option_env!("YALAIN_BUILD_TARGET")),
            databases: Databases {
                store: yalain_store::SCHEMA_VERSION,
            },
            world_database: yalain_world_db::release::PINNED_TAG.to_owned(),
        }
    }

    /// As one line of JSON (what `--release-facts` prints).
    #[must_use]
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

impl BuildFacts for Facts {
    fn version(&self) -> &str {
        &self.version
    }

    fn target(&self) -> &str {
        &self.target
    }

    fn set_version(&mut self, version: &str) {
        version.clone_into(&mut self.version);
    }
}

/// The `upgrade` object of a release's `release.json`: what the updater reads of it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Declaration {
    pub kind: Kind,
    /// The oldest version that can upgrade straight to this one.
    pub upgrades_from: String,
    /// The schema version this release brings the database to.
    pub databases: Databases,
    /// The world database release it serves by default.
    pub world_database: String,
    /// The configuration changes since the previous release.
    #[serde(default)]
    pub config: Vec<ConfigChange>,
}

impl UpgradeDeclaration<Facts> for Declaration {
    fn kind(&self) -> Kind {
        self.kind
    }

    fn upgrades_from(&self) -> &str {
        &self.upgrades_from
    }

    fn config(&self) -> &[ConfigChange] {
        &self.config
    }

    fn databases_older_than(&self, mine: &Facts) -> bool {
        self.databases.store < mine.databases.store
    }

    fn migrations_from(&self, mine: &Facts) -> Vec<Migration> {
        if self.databases.store == mine.databases.store {
            return Vec::new();
        }
        vec![Migration {
            database: "store".to_owned(),
            from: mine.databases.store,
            to: self.databases.store,
        }]
    }

    fn world_database_fetch_from(&self, mine: &Facts) -> Option<String> {
        (self.world_database != mine.world_database).then(|| self.world_database.clone())
    }

    fn mismatch(&self, facts: &Facts) -> Option<String> {
        (facts.databases != self.databases || facts.world_database != self.world_database).then(
            || {
                format!(
                    "server reports database schema {} and world database {}, but its release declares {} and {}",
                    facts.databases.store,
                    facts.world_database,
                    self.databases.store,
                    self.world_database
                )
            },
        )
    }
}

/// Runs the program of a release: its facts, its health check, its fetch of the world database.
pub trait Programs: std::fmt::Debug + Send + Sync {
    /// What the program `exe` says it is (`--release-facts`).
    ///
    /// # Errors
    /// When it does not run or answers something else.
    fn facts(&self, exe: &Path) -> Result<Facts, String>;

    /// Starts the installed program `exe`, waits until it is ready, and stops it: the health check.
    /// What it reported when ready.
    ///
    /// # Errors
    /// When it does not come up.
    fn trial(&self, exe: &Path) -> Result<Facts, String>;

    /// Has the program `exe` fetch the world database it serves.
    ///
    /// # Errors
    /// When the fetch fails.
    fn fetch_world_database(&self, exe: &Path) -> Result<(), String>;
}

/// [`Programs`] by running them.
#[derive(Debug, Clone)]
pub struct Processes {
    /// The options the server runs with, which its health check runs with too.
    pub args: Vec<OsString>,
    /// The folder the server is started in.
    pub current_dir: PathBuf,
    /// How long the health check may take to come up and stop.
    pub timeout: Duration,
    /// Where the ready report is written.
    pub work_dir: PathBuf,
    /// The world served is a folder of its own, which the release this build pins is no part of.
    pub own_world: bool,
}

impl Programs for Processes {
    fn facts(&self, exe: &Path) -> Result<Facts, String> {
        release_facts(exe)
    }

    fn trial(&self, exe: &Path) -> Result<Facts, String> {
        run_trial(
            exe,
            &self.args,
            &self.current_dir,
            self.timeout,
            &self.work_dir,
        )
    }

    fn fetch_world_database(&self, exe: &Path) -> Result<(), String> {
        if self.own_world {
            tracing::info!("Update: this world serves its own folder: no world database to fetch");
            return Ok(());
        }
        let status = process(exe)
            .args(["world", "fetch"])
            .current_dir(&self.current_dir)
            .stdin(Stdio::null())
            .status()
            .map_err(|e| format!("{} could not be run: {e}", exe.display()))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!(
                "the new program could not fetch its world database ({status}): its output above says why"
            ))
        }
    }
}

/// Yalain, as the shared updater knows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Yalain;

impl Product for Yalain {
    type Facts = Facts;
    type Declaration = Declaration;
    type Build = dyn Programs;
    type WorldInputs = ();

    const NAME: &'static str = "Yalain";
    const TAG_PREFIX: &'static str = "yalain-v";
    const WORK_DIR: &'static str = ".yalain-update";

    fn server_file_name() -> String {
        program_file_name()
    }

    fn executables() -> Vec<String> {
        vec![program_file_name()]
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
        if let Some(tag) = &needs.world_database_fetch {
            tracing::info!("Update: fetching the world database {tag} for {version}");
            updater
                .build
                .fetch_world_database(&dir.join(program_file_name()))?;
        }
        Ok(None)
    }

    fn backup(db: &Path, label: &str) -> Result<PathBuf, String> {
        yalain_store::snapshot(db, label).map_err(|e| e.to_string())
    }

    fn restore(backup: &Path, db: &Path) -> std::io::Result<()> {
        yalain_store::restore(backup, db)
    }
}

/// The updater of one installation.
pub type Updater = dereth_server_update::Updater<Yalain>;

/// The running server's updater.
pub type AutoUpdate = running::AutoUpdate<Yalain>;

/// The updater's settings, from `[server]` of the configuration file or the command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// `server.update`: `off`, `patch` or `minor`.
    pub update: String,
    /// `server.update_check_hours`.
    pub check_hours: u32,
    /// `server.update_warning_seconds`.
    pub warning_seconds: u32,
    /// `server.update_source`: empty means the repository the build came from.
    pub source: String,
    /// The world served does not use the release this build pins ([`crate::world::uses_the_pin`]).
    pub own_world: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            update: "off".to_owned(),
            check_hours: 6,
            warning_seconds: 300,
            source: String::new(),
            own_world: false,
        }
    }
}

/// The updater of the installation this process runs from: a server keeping its accounts and
/// characters in `database` (none: in memory), running with the options `args`.
///
/// # Errors
/// When the running executable's folder or the source repository cannot be told.
pub fn for_this_installation(
    settings: &Settings,
    database: Option<&Path>,
    args: Vec<OsString>,
) -> Result<Updater, String> {
    let exe = std::env::current_exe()
        .map_err(|e| format!("the server's own executable cannot be found: {e}"))?;
    let install_dir = exe
        .parent()
        .ok_or("the server's executable has no folder")?
        .to_path_buf();
    let databases = database
        .map(|d| std::path::absolute(d).unwrap_or_else(|_| d.to_path_buf()))
        .into_iter()
        .collect();
    // No world pack beside the server: its world database is in the per-user cache, by tag.
    let layout = Layout::new(&install_dir, databases, PathBuf::new());
    let build = Processes {
        args,
        current_dir: std::env::current_dir().unwrap_or_else(|_| install_dir.clone()),
        timeout: HEALTH_CHECK_TIMEOUT,
        work_dir: layout.work_dir.clone(),
        own_world: settings.own_world,
    };
    Ok(Updater {
        source: Source::configured(&settings.source, SOURCE_URL)?,
        agent: source::agent(concat!("yalain/", env!("CARGO_PKG_VERSION"))),
        mine: Facts::this_build(),
        layout,
        build: Box::new(build),
        pack_inputs: None,
    })
}

/// What the updater thread asks the world's loop.
#[derive(Debug)]
pub enum Ask {
    /// Begin the shutdown countdown of these seconds: whether it began.
    BeginCountdown(u32, Sender<bool>),
    /// Whether a shutdown is under way.
    CountingDown(Sender<bool>),
}

/// Answers what the updater thread asked, on the world's loop.
pub fn answer<S: Store>(ask: Ask, server: &mut Server<S>, net: &mut ServerNet, clock: Clock) {
    match ask {
        Ask::BeginCountdown(seconds, tx) => {
            let _ = tx.send(shutdown::begin_warning(server, net, clock, seconds));
        }
        Ask::CountingDown(tx) => {
            let _ = tx.send(server.shutdown != Shutdown::No);
        }
    }
}

/// The world, as the updater thread reaches it.
#[derive(Debug)]
struct WorldLoop(Sender<Ask>);

impl WorldLoop {
    /// How long the world's loop may take to answer.
    const ANSWER_WITHIN: Duration = Duration::from_secs(60);

    fn ask(&self, ask: impl FnOnce(Sender<bool>) -> Ask) -> Option<bool> {
        let (tx, rx) = mpsc::channel();
        self.0.send(ask(tx)).ok()?;
        rx.recv_timeout(Self::ANSWER_WITHIN).ok()
    }
}

impl World for WorldLoop {
    fn begin_countdown(&self, seconds: u32) -> Option<bool> {
        self.ask(|tx| Ask::BeginCountdown(seconds, tx))
    }

    fn counting_down(&self) -> Option<bool> {
        self.ask(Ask::CountingDown)
    }
}

/// Starts the updater thread when `server.update` is not `off`. `database` is the server's file
/// (none: in memory), and `test_version` is the hidden test option's value.
pub fn start(
    settings: &Settings,
    database: Option<&Path>,
    test_version: Option<&str>,
    asks: Sender<Ask>,
) -> Option<Arc<AutoUpdate>> {
    let policy = running::policy_to_run(&settings.update)?;
    let args = running::passed_on(std::env::args_os().skip(1));
    let updater = match for_this_installation(settings, database, args) {
        Ok(u) => u,
        Err(e) => {
            tracing::warn!("Update: automatic updates are off: {e}");
            return None;
        }
    };
    running::start(
        updater,
        Schedule {
            policy,
            check_hours: settings.check_hours,
            warning_seconds: settings.warning_seconds,
        },
        test_version,
        WorldLoop(asks),
    )
}

/// After the server has stopped, its database closed and its ports free: installs the staged release
/// when the world ended by itself (its countdown ran out) and hands the process over to the new
/// server, or to the restored old one when the new one failed its health check. `None` when there
/// is nothing to do. `typed` is the console's lines, which go on to the new server where the process
/// waits for it.
pub fn after_shutdown(
    auto: Option<&AutoUpdate>,
    world_ended: bool,
    typed: Receiver<String>,
) -> Option<ExitCode> {
    let exe = auto?.install_staged(world_ended, || {})?;
    let args = running::passed_on(std::env::args_os().skip(1));
    Some(handover::relaunch(
        &exe,
        &args,
        Some(forward_console(typed)),
    ))
}

/// The console's reader cannot stop reading, so it forwards what it reads to the new process.
fn forward_console(typed: Receiver<String>) -> ForwardConsole {
    Box::new(move |mut stdin| {
        std::thread::spawn(move || {
            for line in typed {
                if writeln!(stdin, "{line}")
                    .and_then(|()| stdin.flush())
                    .is_err()
                {
                    return;
                }
            }
        });
    })
}

/// The health check's report (`--update-trial`): the server is up, so this build's facts are written
/// to `report`. Nothing is written when the server does not get this far, which fails the check.
pub fn report_ready(report: &Path) {
    match std::fs::write(report, Facts::this_build().to_json()) {
        Ok(()) => tracing::info!("Health check: ready; shutting down"),
        Err(e) => tracing::error!("Health check: {}: {e}", report.display()),
    }
}

const USAGE: &str =
    "usage: yalain update --check|--apply [--config <yalain.toml>] [--policy <off|patch|minor>]";

/// `yalain update ...`.
pub fn command(args: impl Iterator<Item = String>) -> ExitCode {
    let request = match Request::parse(args) {
        Ok(r) => r,
        Err(why) => {
            eprintln!("{why}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let mut server_args = Vec::new();
    if let Some(config) = &request.config {
        server_args.push("--config".to_owned());
        server_args.push(config.clone());
    }
    let (o, notes, file) = match crate::read_options(server_args) {
        Ok(read) => read,
        Err(e) => {
            eprintln!("yalain update: {e}");
            return ExitCode::FAILURE;
        }
    };
    crate::start_logging(o.verbose);
    crate::log_notes(&notes);
    if request.apply && o.dats.is_none() {
        tracing::error!(
            "Update: --apply starts the new server for its health check, and no {} says where its install is: give --config",
            crate::config::FILE
        );
        return ExitCode::FAILURE;
    }
    let policy = request.policy(&o.update.update);
    let trial_args = file
        .map(|f| {
            vec![
                OsString::from("--config"),
                std::path::absolute(&f).unwrap_or(f).into_os_string(),
            ]
        })
        .unwrap_or_default();
    let database = (!o.memory).then_some(o.data.as_path());
    let updater = match for_this_installation(&o.update, database, trial_args) {
        Ok(u) => u,
        Err(e) => {
            tracing::error!("{e}");
            return ExitCode::FAILURE;
        }
    };
    command::run(&updater, policy, request.apply, &|| {
        command::port_in_use(o.bind, o.login_port)
    })
}

#[cfg(test)]
mod tests;
