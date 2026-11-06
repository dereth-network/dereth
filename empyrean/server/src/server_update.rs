//! Not ACE: `empyrean-server update`, and the running server's own updates (`server.update`).
//! The updater itself is [`empyrean_server::update`]; this is where the server's process meets
//! it.
//!
//! **While the server runs**, with `server.update` set to `patch` or `minor`, a thread checks for
//! a new release a minute after start-up and then every `server.update_check_hours`
//! ([`dereth_server_update::running`]). When the policy takes one it is staged (downloaded, checked,
//! unpacked, its world pack rebuilt) with the world still running, then players get ACE's shutdown
//! countdown of `server.update_warning_seconds`. An operator's `@cancelshutdown` cancels the update
//! too (the next check stages it again). When the countdown ends and the world has saved and
//! stopped, the release is installed and health-checked, and the process hands over
//! ([`empyrean_server::update::handover`]) to the new server, or, when the health check failed, to
//! the restored old one. A server stopped by Ctrl-C or a signal installs nothing.
//!
//! **By hand:** `empyrean-server update --check` prints what would be installed and why each
//! newer release would or would not be; `empyrean-server update --apply` installs it on a stopped
//! server (backup, swap, health check, rollback) and leaves the server stopped for the operator
//! or the service manager to start. Both read the same `empyrean.toml` as the server
//! (`--config`); `--policy` overrides `server.update`, and when neither names one, by hand means
//! `minor`.

use std::net::IpAddr;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::Arc;
use std::time::Duration;

use dereth_server_update::command::{self, Request};
use dereth_server_update::running::{self, Schedule, World as UpdaterWorld};
use empyrean_common::config_paths::PathBase;
use empyrean_common::master_configuration::MasterConfiguration;
use empyrean_server::update::{self, handover, Empyrean};
use empyrean_world::managers::server_manager;
use empyrean_world::World;

use crate::program::{StopCause, WorldCommand};

pub use dereth_server_update::running::TEST_VERSION_FLAG;

/// Set once the process has handed over to another server process (Windows): Ctrl-C is then
/// that server's to handle.
static HANDED_OVER: AtomicBool = AtomicBool::new(false);

/// Whether this process has handed over to another server process.
pub fn handed_over() -> bool {
    HANDED_OVER.load(Ordering::SeqCst)
}

/// The running server's updater.
pub type AutoUpdate = running::AutoUpdate<Empyrean>;

/// Starts the updater thread when `server.update` is not `off`. `test_version` is
/// [`TEST_VERSION_FLAG`]'s value.
pub fn start(
    config: &MasterConfiguration,
    paths: &PathBase,
    config_file: Option<&std::path::Path>,
    commands: &Sender<WorldCommand>,
    test_version: Option<&str>,
) -> Option<Arc<AutoUpdate>> {
    let policy = running::policy_to_run(&config.server.update)?;
    let updater = match update::for_this_installation(config, paths, config_file) {
        Ok(u) => u,
        Err(e) => {
            log::warn!("Update: automatic updates are off: {e}");
            return None;
        }
    };
    running::start(
        updater,
        Schedule {
            policy,
            check_hours: config.server.update_check_hours,
            warning_seconds: config.server.update_warning_seconds,
        },
        test_version,
        WorldThread(commands.clone()),
    )
}

/// The world, as the updater thread reaches it.
struct WorldThread(Sender<WorldCommand>);

impl WorldThread {
    /// Asks the world thread for `f`'s answer; `None` when the world is gone.
    fn ask<R: Send + 'static>(
        &self,
        f: impl FnOnce(&mut World) -> R + Send + 'static,
    ) -> Option<R> {
        let (tx, rx) = mpsc::channel();
        self.0
            .send(Box::new(move |w: &mut World| {
                let _ = tx.send(f(w));
            }))
            .ok()?;
        rx.recv_timeout(Duration::from_secs(60)).ok()
    }
}

impl UpdaterWorld for WorldThread {
    fn begin_countdown(&self, seconds: u32) -> Option<bool> {
        // ACE's shutdown, with its countdown broadcasts, as `@shutdown` starts it.
        self.ask(move |w| {
            if w.server_manager.shutdown_initiated {
                return false;
            }
            server_manager::set_shutdown_interval(w, seconds);
            server_manager::begin_shutdown(w);
            true
        })
    }

    fn counting_down(&self) -> Option<bool> {
        self.ask(|w| w.server_manager.shutdown_initiated)
    }
}

/// After the server has shut down: installs the staged release when the world ended by itself
/// (its countdown ran out) and hands the process over to the new server, or to the restored old
/// one when the new one failed its health check. `None` when there is nothing to do.
pub fn after_shutdown(auto: Option<&AutoUpdate>, cause: StopCause) -> Option<ExitCode> {
    let exe = auto?.install_staged(cause == StopCause::WorldEnded, || {
        empyrean_command::console::stop();
        // What is typed on the console from now on is the next server's.
        empyrean_command::command_manager::hold_console_input();
    })?;
    let args = running::passed_on(std::env::args_os().skip(1));
    HANDED_OVER.store(true, Ordering::SeqCst);
    Some(handover::relaunch(&exe, &args, forward_console()))
}

/// A console reading a pipe or a file cannot stop reading, so it forwards what it reads to the new
/// process (holding what came in since the world stopped); a terminal's line editor has already
/// ended and the new process reads the terminal itself.
fn forward_console() -> Option<handover::ForwardConsole> {
    use empyrean_command::command_manager;
    command_manager::line_reader_running().then(|| {
        Box::new(|stdin| {
            command_manager::forward_console_input(stdin);
        }) as handover::ForwardConsole
    })
}

const USAGE: &str = "usage: empyrean-server update --check|--apply [--config <empyrean.toml>] [--policy <off|patch|minor>]";

/// `empyrean-server update ...`.
pub fn command(args: impl Iterator<Item = String>) -> ExitCode {
    let request = match Request::parse(args) {
        Ok(r) => r,
        Err(why) => {
            eprintln!("{why}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let (config, paths, file) = match crate::program::load_config(request.config.as_deref()) {
        Ok(loaded) => loaded,
        Err(e) => {
            log::error!("Unable to load the configuration: {e}");
            return ExitCode::FAILURE;
        }
    };
    let policy = request.policy(&config.server.update);
    let updater = match update::for_this_installation(&config, &paths, file.as_deref()) {
        Ok(u) => u,
        Err(e) => {
            log::error!("{e}");
            return ExitCode::FAILURE;
        }
    };
    command::run(&updater, policy, request.apply, &|| server_running(&config))
}

/// Machine-readable controls for a separate launcher; no game world is started here.
pub fn managed_command(args: impl Iterator<Item = String>) -> ExitCode {
    let result = managed_request(args);
    let success = result.is_ok();
    let value = match result {
        Ok(value) => serde_json::json!({ "ok": true, "result": value }),
        Err(error) => serde_json::json!({ "ok": false, "error": error }),
    };
    // A trial server can leave its console prompt unterminated on inherited stdout.
    println!("\n{value}");
    if success {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn managed_request(mut args: impl Iterator<Item = String>) -> Result<serde_json::Value, String> {
    use empyrean_server::update::managed;
    let action = args
        .next()
        .ok_or("managed-update needs check, stage, apply, status, cancel or rollback")?;
    let mut config = None;
    let mut candidate = None;
    let mut index_path = None;
    let mut approved = None;
    while let Some(flag) = args.next() {
        let value = args.next().ok_or_else(|| format!("{flag} needs a value"))?;
        match flag.as_str() {
            "--config" => config = Some(value),
            "--candidate" => candidate = Some(std::path::PathBuf::from(value)),
            "--index" => index_path = Some(std::path::PathBuf::from(value)),
            "--approve" => approved = Some(value),
            _ => return Err(format!("Unknown managed-update flag: {flag}")),
        }
    }
    let config = config.ok_or("managed-update requires an explicit --config")?;
    let (config, paths, file) =
        crate::program::load_config(Some(&config)).map_err(|e| e.to_string())?;
    let updater = update::for_this_installation(&config, &paths, file.as_deref())?;
    std::fs::create_dir_all(&updater.layout.work_dir).map_err(|e| e.to_string())?;
    let guard = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(updater.layout.work_dir.join("managed.lock"))
        .map_err(|e| e.to_string())?;
    guard
        .try_lock()
        .map_err(|_| "Another managed update is in progress.")?;
    match action.as_str() {
        "cancel" => {
            managed::discard(&updater)?;
            Ok(serde_json::Value::Null)
        }
        "status" => serde_json::to_value(managed::pending(&updater)?).map_err(|e| e.to_string()),
        "check" | "stage" => {
            let candidate = candidate.ok_or("--candidate is required")?;
            let index_path = index_path.ok_or("--index is required")?;
            let index = update::release::ReleaseIndex::parse(
                &std::fs::read_to_string(index_path).map_err(|e| e.to_string())?,
            )?;
            let plan = if action == "check" {
                let facts = updater
                    .build
                    .facts(&candidate.join(update::install::server_file_name()))?;
                managed::inspect(&updater.mine, &index, &facts, updater.pack_inputs.is_some())?
            } else {
                managed::prepare(&updater, &candidate, &index, approved.as_deref())?
            };
            serde_json::to_value(plan).map_err(|e| e.to_string())
        }
        "apply" | "rollback" => {
            if let Some(why) = server_running(&config) {
                return Err(format!("Stop the server first: {why}"));
            }
            if action == "apply" {
                serde_json::to_value(managed::apply_with_config(&updater, file.as_deref())?)
                    .map_err(|e| e.to_string())
            } else {
                Ok(serde_json::json!({ "version": managed::rollback(&updater)? }))
            }
        }
        _ => Err(format!("Unknown managed-update action: {action}")),
    }
}

/// Why the server looks to be running (its game port is taken), or `None`.
fn server_running(config: &MasterConfiguration) -> Option<String> {
    let n = &config.server.network;
    let host: IpAddr = n.host.parse().ok()?;
    let port = u16::try_from(n.port).ok()?;
    command::port_in_use(host, port)
}
