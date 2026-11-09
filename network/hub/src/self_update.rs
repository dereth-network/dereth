//! `dereth-network-hub self-update`: the hub installs its own releases, on its operator's command.
//! It is a product of the game servers' shared updater whose release is one binary
//! ([`dereth_server_updater::binary`], which says how a release is found, checked and installed);
//! this is what is the hub's own:
//!
//! - its releases are the source's ([`SOURCE_URL`], or `--source`) tags `network-hub-v<version>`,
//!   each with `dereth-network-hub-<version>-<target>` archives and `SHA256SUMS`;
//! - the new binary must say which registry schema it reads (`--registry-schema`): one older than
//!   the registry's would refuse to start on it, and is recorded as failed; under `--policy patch`
//!   one that reads another schema than this build's is refused, so that `patch` only ever changes
//!   the binary;
//! - before the binary is replaced, the registry is copied with the consistent copy `backup`
//!   makes, as `<registry>.backup-update-v<old>-v<new>-<UTC time>`, the newest [`BACKUPS_KEPT`]
//!   kept;
//! - the releases that failed here are recorded in [`STATE_FILE`] beside the binary.
//!
//! The updater replaces the file it runs from, so it needs write access to that file's folder; it
//! never starts or stops a hub, and the running hub never replaces itself. A service manager
//! restarts the hub after an update (`network/hub/DEPLOY.md`).

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use dereth_server_updater::binary::{self, Product, Refusal};
pub use dereth_server_updater::binary::{
    this_target, Check, Fetcher, Fetching, Https, Installed, Policy, Processes, Runner,
};
pub use dereth_server_updater::install::State;
pub use dereth_server_updater::source::Source;
pub use dereth_server_updater::verify::sha256_hex;
pub use dereth_server_updater::version::Version;

/// The repository this build's releases come from, when `--source` names no other. A release build
/// sets `DERETH_HUB_BUILD_SOURCE_URL` (at compile time) so that a fork's binaries look for the
/// fork's releases.
pub const SOURCE_URL: &str = match option_env!("DERETH_HUB_BUILD_SOURCE_URL") {
    Some(url) => url,
    None => "https://github.com/dereth-network/dereth",
};

/// The prefix of the hub's release tags (`network-hub-v0.1.1`).
pub const TAG_PREFIX: &str = "network-hub-v";

/// The program's name: its release archives' and its binary's.
pub const PROGRAM: &str = "dereth-network-hub";

/// The file beside the binary that records the releases that failed here.
pub const STATE_FILE: &str = ".dereth-network-hub-update.json";

/// The file the updater writes and removes to learn that it may write in the binary's folder.
pub const WRITE_CHECK_FILE: &str = ".dereth-network-hub.write-check";

/// How many of the registry backups the updater makes are kept: the newest.
pub const BACKUPS_KEPT: usize = 3;

/// The exit status of `self-update` when no release is to be installed (`0` when one was, or
/// would be).
pub const NOTHING_TO_INSTALL: u8 = 3;

/// The updater of an installed hub.
pub type Updater = binary::Updater<Hub>;

/// The hub, as the shared updater's product: the registry schema the installed binary reads, and
/// the registry, which is checked against the new binary and copied before it is installed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hub {
    /// The registry schema the installed binary reads.
    pub schema: i64,
    /// The registry.
    pub registry: PathBuf,
}

impl Product for Hub {
    const PROGRAM: &'static str = PROGRAM;
    const TAG_PREFIX: &'static str = TAG_PREFIX;
    const COMMAND: &'static str = "self-update";
    const POLICY_OPTION: &'static str = "--policy";
    const STATE_FILE: &'static str = STATE_FILE;
    const WRITE_CHECK_FILE: &'static str = WRITE_CHECK_FILE;

    /// No registry to back up first.
    fn blocked(&self) -> Option<String> {
        (!self.registry.is_file()).then(|| {
            format!(
                "there is no registry at {} to back up first: name it with --db or DERETH_HUB_DB",
                self.registry.display()
            )
        })
    }

    /// The new binary reads this hub's registry, and under `patch` the same schema as this build.
    fn accept(
        &self,
        runner: &dyn Runner,
        exe: &Path,
        version: &Version,
        policy: Policy,
    ) -> Result<(), Refusal> {
        let refuse = |reason: String, record: bool| Refusal { reason, record };
        let schema = runner
            .output(exe, "--registry-schema")
            .and_then(|t| {
                t.parse::<i64>()
                    .map_err(|_| format!("`{t}` is not a number"))
            })
            .map_err(|e| {
                refuse(
                    format!("{version}'s binary does not say which registry schema it reads: {e}"),
                    true,
                )
            })?;
        let registry_schema = crate::registry::schema_of_file(&self.registry)
            .map_err(|e| refuse(format!("{}: {}", self.registry.display(), e.message), false))?;
        if registry_schema > schema {
            return Err(refuse(
                format!(
                    "{version} reads registry schema {schema}, and the registry is at {registry_schema}: it would refuse to start"
                ),
                true,
            ));
        }
        if policy == Policy::Patch && schema != self.schema {
            return Err(refuse(
                format!(
                    "{version} reads registry schema {schema}, not this build's {}: --policy patch installs only a release that changes nothing but the binary; install it with --policy minor",
                    self.schema
                ),
                false,
            ));
        }
        Ok(())
    }

    /// Writes the registry's copy beside it, as `<registry>.backup-update-v<old>-v<new>-<UTC
    /// time>`, private to the registry's owner, and removes all but the newest [`BACKUPS_KEPT`] of
    /// those copies.
    fn back_up(
        &self,
        from: &Version,
        to: &Version,
        say: &mut dyn FnMut(&str),
    ) -> Result<PathBuf, String> {
        let dir = match self.registry.parent() {
            Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
            _ => PathBuf::from("."),
        };
        let name = self
            .registry
            .file_name()
            .ok_or("the registry has no file name")?
            .to_string_lossy()
            .into_owned();
        let stamp: String = crate::registry::iso(SystemTime::now())
            .chars()
            .take(19)
            .filter(|c| c.is_ascii_digit() || *c == 'T')
            .collect();
        let prefix = format!("{name}.backup-update-");
        let backup = dir.join(format!("{prefix}v{from}-v{to}-{stamp}Z"));
        say(&format!(
            "Backing up {} to {}",
            self.registry.display(),
            backup.display()
        ));
        crate::registry::backup_file(&self.registry, &backup).map_err(|e| {
            let _ = fs::remove_file(&backup);
            format!(
                "backing up {} failed ({})",
                self.registry.display(),
                e.message
            )
        })?;
        same_owner_private(&self.registry, &backup);
        let mut older: Vec<(String, PathBuf)> = fs::read_dir(&dir)
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .filter_map(|e| {
                        let n = e.file_name().to_string_lossy().into_owned();
                        let stamp = n.strip_prefix(&prefix)?.rsplit('-').next()?.to_owned();
                        Some((stamp, e.path()))
                    })
                    .collect()
            })
            .unwrap_or_default();
        older.sort();
        let excess = older.len().saturating_sub(BACKUPS_KEPT);
        for (_, path) in older.into_iter().take(excess) {
            let _ = fs::remove_file(path);
        }
        Ok(backup)
    }
}

/// This build's version.
#[must_use]
pub fn this_build() -> Version {
    Version::parse(env!("CARGO_PKG_VERSION")).expect("the crate's version is a release version")
}

/// Reads `patch` or `minor`, the policies `self-update` takes.
#[must_use]
pub fn parse_policy(text: &str) -> Option<Policy> {
    match text.trim() {
        "patch" => Some(Policy::Patch),
        "minor" => Some(Policy::Minor),
        _ => None,
    }
}

/// The hub's release archive for `target`: `dereth-network-hub-<version>-<target>.zip` on
/// Windows, `.tar.gz` elsewhere.
#[must_use]
pub fn archive_name(version: &Version, target: &str) -> String {
    binary::archive_name(PROGRAM, version, target)
}

/// The hub's binary's name in a release for `target`.
#[must_use]
pub fn binary_name(target: &str) -> String {
    binary::binary_name(PROGRAM, target)
}

/// The updater of the binary this process runs from, for the registry at `registry`.
///
/// # Errors
/// When the binary's own file cannot be found.
pub fn for_this_binary(source: Source, registry: &Path) -> Result<Updater, String> {
    let binary = std::env::current_exe()
        .map_err(|e| format!("this program's own file cannot be found: {e}"))?;
    Ok(Updater {
        source,
        fetcher: Box::new(Https::new(concat!(
            "dereth-network-hub/",
            env!("CARGO_PKG_VERSION")
        ))),
        runner: Box::new(Processes),
        current: this_build(),
        target: this_target().map(str::to_owned),
        binary,
        product: Hub {
            schema: crate::registry::SCHEMA_VERSION,
            registry: registry.to_path_buf(),
        },
    })
}

/// Gives `copy` the owner of `original` and makes it readable by that owner alone (Unix; where
/// this account may not, the copy stays this account's).
fn same_owner_private(original: &Path, copy: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let _ = fs::set_permissions(copy, fs::Permissions::from_mode(0o600));
        if let Ok(m) = fs::metadata(original) {
            let _ = std::os::unix::fs::chown(copy, Some(m.uid()), Some(m.gid()));
        }
    }
    #[cfg(not(unix))]
    let _ = (original, copy);
}

/// The lines `self-update` prints for `check`.
#[must_use]
pub fn describe(updater: &Updater, check: &Check) -> Vec<String> {
    let target = updater
        .target
        .as_deref()
        .unwrap_or("a target no release is built for");
    let hub = &updater.product;
    let registry = match crate::registry::schema_of_file(&hub.registry) {
        Ok(s) => format!("{} (schema {s})", hub.registry.display()),
        Err(_) => format!("{} (not found)", hub.registry.display()),
    };
    let mut lines = vec![
        format!("This hub: {PROGRAM} {} for {target}", updater.current),
        format!(
            "It reads registry schema {}; the registry: {registry}",
            hub.schema
        ),
        format!(
            "Releases of {}; --policy {}",
            updater.source.repo,
            check.policy.name()
        ),
    ];
    lines.extend(binary::describe(updater, check));
    if let Some(why) = &check.blocked {
        lines.push(format!("--apply would refuse: {why}"));
    }
    lines
}
