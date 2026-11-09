//! The server's `update` command, by hand: `update --check` prints what would be installed and why
//! each newer release would or would not be; `update --apply` installs it on a stopped server
//! (backup, swap, health check, rollback) and leaves the server stopped for the operator or the
//! service manager to start. `--policy` overrides `server.update`, and when neither names one, by
//! hand means `minor`.

use std::net::{IpAddr, UdpSocket};
use std::process::ExitCode;

use crate::policy::Policy;
use crate::{describe, Product, Updater};

/// What `update`'s command line asks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    /// `--apply`, not `--check`.
    pub apply: bool,
    /// `--config`.
    pub config: Option<String>,
    /// `--policy`.
    pub policy: Option<Policy>,
}

impl Request {
    /// Reads `update`'s command line.
    ///
    /// # Errors
    /// What is wrong with it, for the line above the usage.
    pub fn parse(args: impl Iterator<Item = String>) -> Result<Self, String> {
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
                    None => return Err("--config needs a path".to_owned()),
                },
                "--policy" => match args.next().as_deref().and_then(Policy::parse) {
                    Some(p) => policy = Some(p),
                    None => return Err("--policy takes off, patch or minor".to_owned()),
                },
                other => return Err(format!("unexpected argument `{other}`")),
            }
        }
        let Some(apply) = apply else {
            return Err("give --check or --apply".to_owned());
        };
        Ok(Self {
            apply,
            config,
            policy,
        })
    }

    /// The policy to run with: `--policy`, else a `server.update` of `configured` that is not `off`,
    /// else `minor`.
    #[must_use]
    pub fn policy(&self, configured: &str) -> Policy {
        match self.policy {
            Some(p) => p,
            None => match Policy::parse(configured) {
                Some(Policy::Off) | None => Policy::Minor,
                Some(p) => p,
            },
        }
    }
}

/// Why the server looks to be running (its game port on `host` is taken), or `None`.
#[must_use]
pub fn port_in_use(host: IpAddr, port: u16) -> Option<String> {
    UdpSocket::bind((host, port)).err().map(|e| {
        format!(
            "the server's port {host}:{port} is in use ({e}), so the server looks to be running"
        )
    })
}

/// Runs `update` with `updater`: `running` says why the server looks to be running, when it does.
pub fn run<P: Product>(
    updater: &Updater<P>,
    policy: Policy,
    apply: bool,
    running: &dyn Fn() -> Option<String>,
) -> ExitCode {
    let check = match updater.check(policy) {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("Update: the check failed: {e}");
            return ExitCode::FAILURE;
        }
    };
    for line in describe::<P>(&updater.mine, &check) {
        println!("{line}");
    }
    if !apply || check.selection.take.is_none() {
        return ExitCode::SUCCESS;
    }
    if let Some(why) = running() {
        tracing::error!("Update: {why}: stop the server first, then run update --apply again");
        return ExitCode::FAILURE;
    }
    let staged = match updater.stage(&check) {
        Ok(Some(s)) => s,
        Ok(None) => return ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!("Update: not installed: {e}");
            return ExitCode::FAILURE;
        }
    };
    match updater.install(&staged) {
        Ok(installed) => {
            println!(
                "Installed {} {} in {}; it passed its health check. Start the server.",
                P::NAME,
                staged.version,
                updater.layout.install_dir.display()
            );
            for (db, backup) in &installed.backups {
                println!("  {} was backed up to {}", db.display(), backup.display());
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            tracing::error!("Update: {e}");
            ExitCode::FAILURE
        }
    }
}
