//! The running server's own updates (`server.update`).
//!
//! With `server.update` set to `patch` or `minor`, a thread checks for a new release a minute after
//! start-up and then every `server.update_check_hours`. When the policy takes one it is staged
//! (downloaded, checked, unpacked, its world data got ready) with the world still running, then
//! players get the server's shutdown countdown of `server.update_warning_seconds`. An operator who
//! cancels the shutdown cancels the update too (the next check stages it again). When the countdown
//! ends and the world has saved and stopped, the release is installed and health-checked, and the
//! process hands over ([`crate::handover`]) to the new server, or, when the health check failed, to
//! the restored old one. A server stopped by Ctrl-C or a signal installs nothing.
//!
//! The thread reaches the world through [`World`] and waits through what it is given, so all of this
//! runs under test with no world and no clock.

use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use crate::install::Staged;
use crate::policy::Policy;
use crate::release::BuildFacts;
use crate::{describe, Product, Updater};

/// The first check after start-up waits this long, so a server that is restarted repeatedly does
/// not ask GitHub each time it starts.
pub const FIRST_CHECK_DELAY: Duration = Duration::from_secs(60);

/// How often the countdown is looked at, to notice it being cancelled.
pub const COUNTDOWN_POLL: Duration = Duration::from_secs(5);

/// The hidden option of the updater's own tests (`--update-test-version <version>`): the updater
/// takes this process for that version, and checks a second after start-up. It is not passed on
/// to the server that takes over after an update.
pub const TEST_VERSION_FLAG: &str = "--update-test-version";

/// What the updater asks of the world, on whatever thread the world runs on. `None` from either
/// when the world is gone.
pub trait World: Send {
    /// Begins the shutdown countdown of `seconds`, as the operator's shutdown does: whether it
    /// began (not when a shutdown is already under way).
    fn begin_countdown(&self, seconds: u32) -> Option<bool>;
    /// Whether the shutdown is still under way.
    fn counting_down(&self) -> Option<bool>;
}

/// The running server's updater: the release staged for installation, when there is one.
#[derive(Debug)]
pub struct AutoUpdate<P: Product> {
    pub updater: Updater<P>,
    staged: Mutex<Option<Staged<P::Facts>>>,
}

/// How the running server checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Schedule {
    /// `server.update`.
    pub policy: Policy,
    /// `server.update_check_hours`.
    pub check_hours: u32,
    /// `server.update_warning_seconds`.
    pub warning_seconds: u32,
}

/// The policy to run with for a `server.update` of `value`: `None`, with a warning when it is not a
/// policy, for no updates at all.
#[must_use]
pub fn policy_to_run(value: &str) -> Option<Policy> {
    match Policy::parse(value) {
        Some(Policy::Off) => None,
        Some(p) => Some(p),
        None => {
            tracing::warn!(
                "Configuration: server.update = \"{value}\" is not one of off, patch, minor: updates are off"
            );
            None
        }
    }
}

/// Starts the updater thread. `test_version` is [`TEST_VERSION_FLAG`]'s value.
pub fn start<P: Product, W: World + 'static>(
    mut updater: Updater<P>,
    schedule: Schedule,
    test_version: Option<&str>,
    world: W,
) -> Option<Arc<AutoUpdate<P>>> {
    let first_check = match test_version {
        Some(v) => {
            tracing::info!("Update: taken for version {v} ({TEST_VERSION_FLAG})");
            updater.mine.set_version(v);
            Duration::from_secs(1)
        }
        None => FIRST_CHECK_DELAY,
    };
    let auto = Arc::new(AutoUpdate {
        updater,
        staged: Mutex::new(None),
    });
    let interval = Duration::from_secs(u64::from(schedule.check_hours.max(1)) * 3600);
    let thread_auto = Arc::clone(&auto);
    tracing::info!(
        "Update: server.update = \"{}\": checking {} for new releases every {} h",
        schedule.policy.name(),
        thread_auto.updater.source.repo,
        interval.as_secs() / 3600
    );
    let spawned = std::thread::Builder::new()
        .name("Updater".to_owned())
        .spawn(move || {
            std::thread::sleep(first_check);
            let mut wait = std::thread::sleep;
            loop {
                match cycle(
                    &thread_auto,
                    schedule.policy,
                    schedule.warning_seconds,
                    &world,
                    &mut wait,
                ) {
                    Cycle::Again => std::thread::sleep(interval),
                    Cycle::WorldGone => return,
                }
            }
        });
    if let Err(e) = spawned {
        tracing::error!("Update: the updater thread could not start: {e}");
        return None;
    }
    Some(auto)
}

/// How one check ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cycle {
    /// Check again after the interval.
    Again,
    /// The world has gone or is going: the thread's work is done.
    WorldGone,
}

/// One check: finds a release, stages it, and runs the world's countdown for it. `wait` is the
/// thread's sleep between looks at the countdown.
pub fn cycle<P: Product>(
    auto: &AutoUpdate<P>,
    policy: Policy,
    warning: u32,
    world: &dyn World,
    wait: &mut dyn FnMut(Duration),
) -> Cycle {
    let updater = &auto.updater;
    let check = match updater.check(policy) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("Update: the check failed: {e}");
            return Cycle::Again;
        }
    };
    for line in describe::<P>(&updater.mine, &check) {
        tracing::info!("Update: {line}");
    }
    let staged = match updater.stage(&check) {
        Ok(Some(s)) => s,
        Ok(None) => return Cycle::Again,
        Err(e) => {
            tracing::error!("Update: not installed: {e}");
            return Cycle::Again;
        }
    };
    let version = staged.version.clone();
    *auto.staged.lock().unwrap_or_else(PoisonError::into_inner) = Some(staged);
    tracing::info!(
        "Update: {version} is staged; restarting into it after a {warning} s shutdown countdown"
    );
    match world.begin_countdown(warning) {
        None => return Cycle::WorldGone,
        Some(false) => {
            tracing::info!(
                "Update: a shutdown is already under way; {version} is installed when it ends"
            );
            return Cycle::WorldGone;
        }
        Some(true) => {}
    }
    loop {
        wait(COUNTDOWN_POLL);
        match world.counting_down() {
            None => return Cycle::WorldGone,
            Some(true) => {}
            Some(false) => {
                auto.staged
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .take();
                tracing::info!("Update: the shutdown was cancelled, and {version} with it; the next check stages it again");
                return Cycle::Again;
            }
        }
    }
}

impl<P: Product> AutoUpdate<P> {
    /// The updater with nothing staged, for a server that runs its checks by [`cycle`] itself.
    #[must_use]
    pub fn new(updater: Updater<P>) -> Self {
        Self {
            updater,
            staged: Mutex::new(None),
        }
    }

    /// The version staged for installation, when there is one.
    #[must_use]
    pub fn staged_version(&self) -> Option<String> {
        self.staged
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .map(|s| s.version.to_string())
    }

    /// After the server has shut down: installs the staged release when the world ended by itself
    /// (its countdown ran out), `before` running first. The server binary to hand the process over
    /// to, which is the new server, or the restored old one when the new one failed its health
    /// check. `None` when there is nothing to do.
    pub fn install_staged(&self, world_ended: bool, before: impl FnOnce()) -> Option<PathBuf> {
        let staged = self
            .staged
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()?;
        if !world_ended {
            tracing::info!(
                "Update: {} was staged but is not installed: the server was stopped from outside",
                staged.version
            );
            return None;
        }
        before();
        match self.updater.install(&staged) {
            Ok(_) => tracing::info!("Update: starting {}", staged.version),
            Err(e) => tracing::error!("Update: {e}; starting the previous release again"),
        }
        Some(self.updater.layout.server_exe())
    }
}

/// The command line the next server gets: this one's, without [`TEST_VERSION_FLAG`] and its value.
pub fn passed_on(args: impl Iterator<Item = OsString>) -> Vec<OsString> {
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
