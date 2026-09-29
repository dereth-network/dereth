//! Not ACE: `empyrean-server update`, and the running server's own updates (`server.update`).
//! The updater itself is [`empyrean_server::update`]; this is where the server's process meets
//! it.
//!
//! **While the server runs**, with `server.update` set to `patch` or `minor`, a thread checks for
//! a new release a minute after start-up and then every `server.update_check_hours`. When the
//! policy takes one it is staged (downloaded, checked, unpacked, its world pack rebuilt) with the
//! world still running, then players get ACE's shutdown countdown of
//! `server.update_warning_seconds`. An operator's `@cancelshutdown` cancels the update too (the
//! next check stages it again). When the countdown ends and the world has saved and stopped, the
//! release is installed and health-checked, and the process hands over
//! ([`empyrean_server::update::handover`]) to the new server, or, when the health check failed, to
//! the restored old one. A server stopped by Ctrl-C or a signal installs nothing.
//!
//! **By hand:** `empyrean-server update --check` prints what would be installed and why each
//! newer release would or would not be; `empyrean-server update --apply` installs it on a stopped
//! server (backup, swap, health check, rollback) and leaves the server stopped for the operator
//! or the service manager to start. Both read the same `empyrean.toml` as the server
//! (`--config`); `--policy` overrides `server.update`, and when neither names one, by hand means
//! `minor`.

use std::ffi::OsString;
use std::net::{IpAddr, UdpSocket};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use empyrean_common::config_paths::PathBase;
use empyrean_common::master_configuration::MasterConfiguration;
use empyrean_server::update::install::Staged;
use empyrean_server::update::policy::Policy;
use empyrean_server::update::{self, handover, Updater};
use empyrean_world::managers::server_manager;
use empyrean_world::World;

use crate::program::{StopCause, WorldCommand};

/// Set once the process has handed over to another server process (Windows): Ctrl-C is then
/// that server's to handle.
static HANDED_OVER: AtomicBool = AtomicBool::new(false);

/// Whether this process has handed over to another server process.
pub fn handed_over() -> bool {
    HANDED_OVER.load(Ordering::SeqCst)
}

/// The first check after start-up waits this long, so a server that is restarted repeatedly does
/// not ask GitHub each time it starts.
const FIRST_CHECK_DELAY: Duration = Duration::from_secs(60);

/// How often the countdown is looked at, to notice it being cancelled.
const COUNTDOWN_POLL: Duration = Duration::from_secs(5);

/// The running server's updater: the release staged for installation, when there is one.
#[derive(Debug)]
pub struct AutoUpdate {
    updater: Updater,
    staged: Mutex<Option<Staged>>,
}

/// The hidden option of the updater's own tests (`--update-test-version <version>`): the updater
/// takes this process for that version, and checks a second after start-up. It is not passed on
/// to the server that takes over after an update.
pub const TEST_VERSION_FLAG: &str = "--update-test-version";

/// Starts the updater thread when `server.update` is not `off`. `test_version` is
/// [`TEST_VERSION_FLAG`]'s value.
pub fn start(
    config: &MasterConfiguration,
    paths: &PathBase,
    config_file: Option<&std::path::Path>,
    commands: &Sender<WorldCommand>,
    test_version: Option<&str>,
) -> Option<Arc<AutoUpdate>> {
    let policy = match update::configured_policy(config) {
        Ok(Policy::Off) => return None,
        Ok(p) => p,
        Err(value) => {
            log::warn!(
                "Configuration: server.update = \"{value}\" is not one of off, patch, minor: updates are off"
            );
            return None;
        }
    };
    let mut updater = match update::for_this_installation(config, paths, config_file) {
        Ok(u) => u,
        Err(e) => {
            log::warn!("Update: automatic updates are off: {e}");
            return None;
        }
    };
    let first_check = match test_version {
        Some(v) => {
            log::info!("Update: taken for version {v} ({TEST_VERSION_FLAG})");
            updater.mine.version = v.to_owned();
            Duration::from_secs(1)
        }
        None => FIRST_CHECK_DELAY,
    };
    let auto = Arc::new(AutoUpdate {
        updater,
        staged: Mutex::new(None),
    });
    let interval = Duration::from_secs(u64::from(config.server.update_check_hours.max(1)) * 3600);
    let warning = config.server.update_warning_seconds;
    let thread_auto = Arc::clone(&auto);
    let commands = commands.clone();
    log::info!(
        "Update: server.update = \"{}\": checking {} for new releases every {} h",
        policy.name(),
        thread_auto.updater.source.repo,
        interval.as_secs() / 3600
    );
    let spawned = std::thread::Builder::new()
        .name("Updater".to_owned())
        .spawn(move || {
            std::thread::sleep(first_check);
            loop {
                match cycle(&thread_auto, policy, warning, &commands) {
                    Cycle::Again => std::thread::sleep(interval),
                    Cycle::WorldGone => return,
                }
            }
        });
    if let Err(e) = spawned {
        log::error!("Update: the updater thread could not start: {e}");
        return None;
    }
    Some(auto)
}

enum Cycle {
    Again,
    WorldGone,
}

/// Asks the world thread for `f`'s answer; `None` when the world is gone.
fn ask<R: Send + 'static>(
    commands: &Sender<WorldCommand>,
    f: impl FnOnce(&mut World) -> R + Send + 'static,
) -> Option<R> {
    let (tx, rx) = mpsc::channel();
    commands
        .send(Box::new(move |w: &mut World| {
            let _ = tx.send(f(w));
        }))
        .ok()?;
    rx.recv_timeout(Duration::from_secs(60)).ok()
}

fn cycle(
    auto: &AutoUpdate,
    policy: Policy,
    warning: u32,
    commands: &Sender<WorldCommand>,
) -> Cycle {
    let updater = &auto.updater;
    let check = match updater.check(policy) {
        Ok(c) => c,
        Err(e) => {
            log::warn!("Update: the check failed: {e}");
            return Cycle::Again;
        }
    };
    for line in update::describe(&updater.mine, &check) {
        log::info!("Update: {line}");
    }
    let staged = match updater.stage(&check) {
        Ok(Some(s)) => s,
        Ok(None) => return Cycle::Again,
        Err(e) => {
            log::error!("Update: not installed: {e}");
            return Cycle::Again;
        }
    };
    let version = staged.version.clone();
    *auto.staged.lock().unwrap_or_else(PoisonError::into_inner) = Some(staged);
    log::info!(
        "Update: {version} is staged; restarting into it after a {warning} s shutdown countdown"
    );
    // ACE's shutdown, with its countdown broadcasts, as `@shutdown` starts it.
    let started = ask(commands, move |w| {
        if w.server_manager.shutdown_initiated {
            return false;
        }
        server_manager::set_shutdown_interval(w, warning);
        server_manager::begin_shutdown(w);
        true
    });
    match started {
        None => return Cycle::WorldGone,
        Some(false) => {
            log::info!(
                "Update: a shutdown is already under way; {version} is installed when it ends"
            );
            return Cycle::WorldGone;
        }
        Some(true) => {}
    }
    loop {
        std::thread::sleep(COUNTDOWN_POLL);
        match ask(commands, |w| w.server_manager.shutdown_initiated) {
            None => return Cycle::WorldGone,
            Some(true) => {}
            Some(false) => {
                auto.staged
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .take();
                log::info!("Update: the shutdown was cancelled, and {version} with it; the next check stages it again");
                return Cycle::Again;
            }
        }
    }
}

/// After the server has shut down: installs the staged release when the world ended by itself
/// (its countdown ran out) and hands the process over to the new server, or to the restored old
/// one when the new one failed its health check. `None` when there is nothing to do.
pub fn after_shutdown(auto: Option<&AutoUpdate>, cause: StopCause) -> Option<ExitCode> {
    let auto = auto?;
    let staged = auto
        .staged
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take()?;
    if cause != StopCause::WorldEnded {
        log::info!(
            "Update: {} was staged but is not installed: the server was stopped from outside",
            staged.version
        );
        return None;
    }
    empyrean_command::console::stop();
    // What is typed on the console from now on is the next server's.
    empyrean_command::command_manager::hold_console_input();
    match auto.updater.install(&staged) {
        Ok(_) => log::info!("Update: starting {}", staged.version),
        Err(e) => log::error!("Update: {e}; starting the previous release again"),
    }
    let args = passed_on(std::env::args_os().skip(1));
    HANDED_OVER.store(true, Ordering::SeqCst);
    Some(handover::relaunch(&auto.updater.layout.server_exe(), &args))
}

/// The command line the next server gets: this one's, without [`TEST_VERSION_FLAG`] and its value.
fn passed_on(args: impl Iterator<Item = OsString>) -> Vec<OsString> {
    let mut out = Vec::new();
    let mut args = args;
    while let Some(a) = args.next() {
        if a == TEST_VERSION_FLAG {
            args.next();
        } else {
            out.push(a);
        }
    }
    out
}

const USAGE: &str = "usage: empyrean-server update --check|--apply [--config <empyrean.toml>] [--policy <off|patch|minor>]";

/// `empyrean-server update ...`.
pub fn command(args: impl Iterator<Item = String>) -> ExitCode {
    let mut apply = None;
    let mut config = None;
    let mut policy = None;
    let mut args = args;
    while let Some(a) = args.next() {
        match a.as_str() {
            "--check" if apply.is_none() => apply = Some(false),
            "--apply" if apply.is_none() => apply = Some(true),
            "--config" => match args.next() {
                Some(p) => config = Some(p),
                None => return usage("--config needs a path"),
            },
            "--policy" => match args.next().as_deref().and_then(Policy::parse) {
                Some(p) => policy = Some(p),
                None => return usage("--policy takes off, patch or minor"),
            },
            other => return usage(&format!("unexpected argument `{other}`")),
        }
    }
    let Some(apply) = apply else {
        return usage("give --check or --apply");
    };
    let (config, paths, file) = match crate::program::load_config(config.as_deref()) {
        Ok(loaded) => loaded,
        Err(e) => {
            log::error!("Unable to load the configuration: {e}");
            return ExitCode::FAILURE;
        }
    };
    let policy = match policy {
        Some(p) => p,
        None => match update::configured_policy(&config) {
            Ok(Policy::Off) | Err(_) => Policy::Minor,
            Ok(p) => p,
        },
    };
    let updater = match update::for_this_installation(&config, &paths, file.as_deref()) {
        Ok(u) => u,
        Err(e) => {
            log::error!("{e}");
            return ExitCode::FAILURE;
        }
    };
    let check = match updater.check(policy) {
        Ok(c) => c,
        Err(e) => {
            log::error!("Update: the check failed: {e}");
            return ExitCode::FAILURE;
        }
    };
    for line in update::describe(&updater.mine, &check) {
        println!("{line}");
    }
    if !apply || check.selection.take.is_none() {
        return ExitCode::SUCCESS;
    }
    if let Some(why) = server_running(&config) {
        log::error!("Update: {why}: stop the server first, then run update --apply again");
        return ExitCode::FAILURE;
    }
    let staged = match updater.stage(&check) {
        Ok(Some(s)) => s,
        Ok(None) => return ExitCode::SUCCESS,
        Err(e) => {
            log::error!("Update: not installed: {e}");
            return ExitCode::FAILURE;
        }
    };
    match updater.install(&staged) {
        Ok(installed) => {
            println!(
                "Installed Empyrean {} in {}; it passed its health check. Start the server.",
                staged.version,
                updater.layout.install_dir.display()
            );
            for (db, backup) in &installed.backups {
                println!("  {} was backed up to {}", db.display(), backup.display());
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            log::error!("Update: {e}");
            ExitCode::FAILURE
        }
    }
}

fn usage(why: &str) -> ExitCode {
    eprintln!("{why}\n{USAGE}");
    ExitCode::from(2)
}

/// Why the server looks to be running (its game port is taken), or `None`.
fn server_running(config: &MasterConfiguration) -> Option<String> {
    let n = &config.server.network;
    let host: IpAddr = n.host.parse().ok()?;
    let port = u16::try_from(n.port).ok()?;
    UdpSocket::bind((host, port)).err().map(|e| {
        format!(
            "the server's port {host}:{port} is in use ({e}), so the server looks to be running"
        )
    })
}
