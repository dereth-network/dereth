//! Which release, if any, the server installs: the operator's `server.update` against what each
//! newer release declares.
//!
//! - A pre-release and a release of another major version are never installed automatically;
//!   they are reported.
//! - `off` installs nothing. `patch` takes a patch release of this build's major and minor
//!   version whose declaration changes nothing: the same database schemas and world pack as this
//!   build, and no configuration change. `minor` also takes minor releases of this major version,
//!   with their database migrations and world-pack rebuilds.
//! - A release is taken only when this build's version is at least its `upgrades_from`; a newer
//!   release that needs an intermediate one first waits until that one is installed.
//! - Never taken automatically: a release without a declaration, one that needs a configuration
//!   key the operator must set, one whose databases are older than this build's, one that needs
//!   `world.pack` rebuilt when the configuration does not name the dump it was built from, and
//!   one that already failed its health check on this server.
//! - Of the releases that can be taken, the newest is.

use super::release::{ConfigChange, Declaration, Facts, Kind, Migration};
use super::version::Version;

/// `server.update`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Policy {
    Off,
    Patch,
    Minor,
}

impl Policy {
    /// Reads `off`, `patch` or `minor` (any case, trimmed).
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "off" => Some(Self::Off),
            "patch" => Some(Self::Patch),
            "minor" => Some(Self::Minor),
            _ => None,
        }
    }

    /// Its name in the configuration.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Patch => "patch",
            Self::Minor => "minor",
        }
    }
}

/// A newer release, as the updater found it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub version: Version,
    /// Published as a pre-release (or its version has a pre-release suffix).
    pub prerelease: bool,
    /// Its declaration; `None` when its `release.json` has none or could not be read.
    pub declaration: Option<Declaration>,
}

/// What installing a release involves on this server.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Needs {
    /// The databases this server's files will be migrated on (from this build's schema).
    pub migrations: Vec<Migration>,
    /// `world.pack` must be rebuilt.
    pub world_pack_rebuild: bool,
    /// The configuration changes of every release from this one to the target.
    pub config: Vec<ConfigChange>,
}

impl Needs {
    /// Whether installing the release changes nothing but the binaries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.migrations.is_empty() && !self.world_pack_rebuild && self.config.is_empty()
    }

    /// One line per need, for the log and `update --check`.
    #[must_use]
    pub fn describe(&self) -> Vec<String> {
        let mut lines = Vec::new();
        for m in &self.migrations {
            lines.push(format!(
                "the {} database is migrated from schema {} to {} (backed up first)",
                m.database, m.from, m.to
            ));
        }
        if self.world_pack_rebuild {
            lines.push("world.pack is rebuilt from the dump it was built from".to_owned());
        }
        for c in &self.config {
            let mut line = match (&*c.change, &c.to) {
                ("renamed", Some(to)) => format!("configuration: {} is renamed {to}", c.key),
                (change, _) => format!("configuration: {} is {change}", c.key),
            };
            if !c.note.is_empty() {
                line.push_str(": ");
                line.push_str(&c.note);
            }
            lines.push(line);
        }
        if lines.is_empty() {
            lines.push("nothing but the binaries changes".to_owned());
        }
        lines
    }
}

/// The updater's decision.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selection {
    /// The release to install, and what that involves.
    pub take: Option<(Version, Needs)>,
    /// Every other newer release, oldest first, with why it is not installed.
    pub reported: Vec<(Version, String)>,
}

/// What the selection needs to know about this server beyond the releases.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Situation<'a> {
    /// This build.
    pub mine: &'a Facts,
    pub policy: Policy,
    /// Releases that failed their health check here.
    pub failed: &'a [Version],
    /// The configuration names the dump `world.pack` was built from, so it can be rebuilt.
    pub can_rebuild_world_pack: bool,
}

/// What installing a release declared as `d` involves on a server running `mine`, with `config`
/// the configuration changes of every release up to it.
#[must_use]
pub fn needs(mine: &Facts, d: &Declaration, config: Vec<ConfigChange>) -> Needs {
    let mut migrations = Vec::new();
    for (database, from, to) in [
        ("shard", mine.databases.shard, d.databases.shard),
        ("authentication", mine.databases.auth, d.databases.auth),
    ] {
        if to != from {
            migrations.push(Migration {
                database: database.to_owned(),
                from,
                to,
            });
        }
    }
    Needs {
        migrations,
        world_pack_rebuild: d.world_pack != mine.world_pack,
        config,
    }
}

/// Chooses among `candidates` (any order; those not newer than this build are ignored).
#[must_use]
pub fn select(s: &Situation<'_>, candidates: &[Candidate]) -> Selection {
    let Ok(current) = Version::parse(&s.mine.version) else {
        return Selection::default();
    };
    let mut newer: Vec<&Candidate> = candidates.iter().filter(|c| c.version > current).collect();
    newer.sort_by(|a, b| a.version.cmp(&b.version));
    newer.dedup_by(|a, b| a.version == b.version);

    let mut eligible: Vec<(Version, Needs)> = Vec::new();
    let mut reported: Vec<(Version, String)> = Vec::new();
    for c in &newer {
        match verdict(s, &current, c, &newer) {
            Ok(needs) => eligible.push((c.version.clone(), needs)),
            Err(why) => reported.push((c.version.clone(), why)),
        }
    }
    let take = eligible.pop();
    if let Some((newest, _)) = &take {
        for (v, _) in eligible {
            reported.push((v, format!("{newest} is newer and is taken instead")));
        }
    }
    reported.sort_by(|a, b| a.0.cmp(&b.0));
    Selection { take, reported }
}

fn verdict(
    s: &Situation<'_>,
    current: &Version,
    c: &Candidate,
    newer: &[&Candidate],
) -> Result<Needs, String> {
    let v = &c.version;
    if c.prerelease || v.is_prerelease() {
        return Err("a pre-release: never installed automatically".to_owned());
    }
    if v.major != current.major {
        return Err(
            "a major release: never installed automatically; install it by hand".to_owned(),
        );
    }
    match s.policy {
        Policy::Off => return Err("server.update is \"off\"".to_owned()),
        Policy::Patch if v.minor != current.minor => {
            return Err(format!(
                "a minor release: server.update = \"patch\" takes only {}.{}.x",
                current.major, current.minor
            ))
        }
        _ => {}
    }
    if s.failed.contains(v) {
        return Err(
            "failed its health check on this server and was rolled back; not retried automatically"
                .to_owned(),
        );
    }
    let Some(d) = &c.declaration else {
        return Err("its release.json declares nothing about upgrading to it".to_owned());
    };
    let from = d.upgrades_from()?;
    if &from > current {
        return Err(format!(
            "upgrading straight to it needs {from} or newer installed first"
        ));
    }
    if d.databases.shard < s.mine.databases.shard || d.databases.auth < s.mine.databases.auth {
        return Err("its database schemas are older than this build's".to_owned());
    }
    // Every release from this one up to the candidate says what it changes in the configuration.
    let mut config = Vec::new();
    for between in newer
        .iter()
        .filter(|b| !b.prerelease && !b.version.is_prerelease() && b.version <= *v)
    {
        match &between.declaration {
            Some(bd) => config.extend(bd.config.iter().cloned()),
            None => {
                return Err(format!(
                    "{} (between this build and it) declares nothing about upgrading",
                    between.version
                ))
            }
        }
    }
    let needs = needs(s.mine, d, config);
    if s.policy == Policy::Patch && (d.kind != Kind::Patch || !needs.is_empty()) {
        return Err(format!(
            "it changes more than the binaries ({}); server.update = \"patch\" takes only releases that change nothing else",
            needs.describe().join("; ")
        ));
    }
    if let Some(required) = needs.config.iter().find(|c| c.is_required()) {
        return Err(format!(
            "it needs the configuration key {} set{}; set it and install by hand",
            required.key,
            if required.note.is_empty() {
                String::new()
            } else {
                format!(" ({})", required.note)
            }
        ));
    }
    if needs.world_pack_rebuild && !s.can_rebuild_world_pack {
        return Err(
            "it needs world.pack rebuilt, and server.world_base_sql does not name the dump it was built from; rebuild by hand"
                .to_owned(),
        );
    }
    Ok(needs)
}
