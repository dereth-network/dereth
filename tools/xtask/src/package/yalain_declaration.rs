//! What upgrading to a Yalain release involves: the `upgrade` object of its `release.json`, which
//! the server's updater and a launcher hosting a world read before they install the release, as
//! Empyrean's ([`super::declaration`]).
//!
//! Derived from the code, at the release's commit and at the previous release's tag:
//!
//! - the database's schema version (`yalain-store`'s `SCHEMA_VERSION`), and the migration between
//!   the previous release and this one (Yalain backs its database up before it migrates);
//! - the world database release the build serves by default (`yalain-world-db`'s `PINNED_TAG`),
//!   and whether it changed, so that the release must be fetched (`yalain world fetch`) before
//!   the new program starts.
//!
//! Declared by hand in `yalain/releases.toml`, in the form Empyrean's file has: the oldest version
//! that can upgrade straight to the release, configuration keys removed or newly required, and
//! notes. Every first, minor and major release has an entry; a **patch release has none, and
//! neither migrates the database nor changes the configuration**. A patch may pin a newer world
//! database only when that is a patch of the world database too (`v0.23.0` to `v0.23.1`, fixes to
//! its rows): fetching it is a download, which a launcher does by itself. A newer minor or major
//! world database (new packs, new kinds of rows) needs a minor or major Yalain release, as a
//! changed world pack does Empyrean's.

use std::path::Path;

use serde_json::{json, Value};

use super::declaration::{core_text, parse_entries_from, previous_tag_of, Entry};
use super::version::{parse_release_version, YALAIN_TAG_PREFIX};

/// The hand-written declarations.
pub const RELEASES_FILE: &str = "yalain/releases.toml";
/// The database's schema version.
pub const STORE_SOURCE: &str = "yalain/crates/store/src/sqlite.rs";
/// The world database release the build serves.
pub const WORLD_DB_SOURCE: &str = "yalain/crates/world-db/src/release.rs";

/// What an upgrade depends on, as the code of one commit has it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Facts {
    pub store: i64,
    pub world_database: String,
}

/// The facts of a tree whose files `read` returns (by path from the workspace root).
pub fn facts(read: &dyn Fn(&str) -> Result<String, String>) -> Result<Facts, String> {
    let store = read(STORE_SOURCE)?
        .lines()
        .find_map(|l| l.trim().strip_prefix("pub const SCHEMA_VERSION: i64 = "))
        .and_then(|rest| rest.strip_suffix(';'))
        .and_then(|n| n.trim().replace('_', "").parse().ok())
        .ok_or_else(|| {
            format!("{STORE_SOURCE} has no `pub const SCHEMA_VERSION: i64 = <number>;` line")
        })?;
    let world_database = read(WORLD_DB_SOURCE)?
        .lines()
        .find_map(|l| l.trim().strip_prefix("pub const PINNED_TAG: &str = \""))
        .and_then(|rest| rest.strip_suffix("\";"))
        .map(str::to_owned)
        .ok_or_else(|| {
            format!("{WORLD_DB_SOURCE} has no `pub const PINNED_TAG: &str = \"...\";` line")
        })?;
    Ok(Facts {
        store,
        world_database,
    })
}

/// The declaration of `version`, with `previous` the previous release and its facts (`None` for
/// the first), `current` this tree's facts and `entries` the hand-written declarations. Every
/// rule it breaks, when it breaks any.
pub fn declare(
    version: &str,
    previous: Option<(&str, &Facts)>,
    current: &Facts,
    entries: &[Entry],
) -> Result<Value, Vec<String>> {
    let core = parse_release_version(version).map_err(|e| vec![e])?;
    let core_s = core_text(core);
    let mut problems = Vec::new();
    let entry = entries.iter().find(|e| e.version == core_s);
    let prev = match previous {
        Some((p, f)) => {
            let pc = parse_release_version(p).map_err(|e| vec![e])?;
            if pc >= core {
                return Err(vec![format!(
                    "the previous release {p} is not older than {version}"
                )]);
            }
            Some((pc, f))
        }
        None => None,
    };
    let kind = match &prev {
        None => "initial",
        Some((pc, _)) if pc[0] != core[0] => "major",
        Some((pc, _)) if pc[1] != core[1] => "minor",
        Some(_) => "patch",
    };
    let mut migrations = Vec::new();
    let mut fetch = false;
    if let Some((pc, pf)) = &prev {
        if current.store < pf.store {
            problems.push(format!(
                "the database schema went back from {} (in {}) to {}: a released migration is never removed",
                pf.store,
                core_text(*pc),
                current.store
            ));
        } else if current.store > pf.store {
            migrations.push(json!({"database": "store", "from": pf.store, "to": current.store}));
        }
        fetch = pf.world_database != current.world_database;
    }
    let config: Vec<Value> = entry
        .map(|e| {
            e.config
                .iter()
                .map(|c| {
                    let mut v = json!({"key": c.key, "change": c.change});
                    if !c.note.is_empty() {
                        v["note"] = json!(c.note);
                    }
                    v
                })
                .collect()
        })
        .unwrap_or_default();
    let upgrades_from = match (kind, &prev, entry) {
        ("patch", Some((pc, _)), e) => {
            if e.is_some() {
                problems.push(format!(
                    "{core_s} is a patch release, which declares nothing: remove its entry from {RELEASES_FILE}"
                ));
            }
            if !migrations.is_empty() {
                problems.push(format!(
                    "{core_s} is a patch release, but its database migrates ({} -> {})",
                    prev.as_ref().map_or(0, |p| p.1.store),
                    current.store
                ));
            }
            if let Some((_, pf)) = &prev {
                if fetch && !world_patch(&pf.world_database, &current.world_database) {
                    problems.push(format!(
                        "{core_s} is a patch release, but its world database goes from {} to {}, \
                         which is not a patch of the world database: that needs a minor release",
                        pf.world_database, current.world_database
                    ));
                }
            }
            // Whatever could upgrade straight to the previous release still can.
            entries
                .iter()
                .filter_map(|e| parse_release_version(&e.version).ok().map(|v| (v, e)))
                .filter(|(v, _)| v <= pc)
                .max_by_key(|(v, _)| *v)
                .map_or_else(|| core_text(*pc), |(_, e)| e.upgrades_from.clone())
        }
        (_, _, None) => {
            problems.push(format!(
                "{RELEASES_FILE} has no [[release]] for {core_s}: a {kind} release declares the oldest version that can upgrade straight to it"
            ));
            String::new()
        }
        (_, None, Some(e)) => {
            if e.upgrades_from != core_s {
                problems.push(format!(
                    "{core_s} is the first release, so nothing earlier upgrades to it: upgrades_from = \"{core_s}\""
                ));
            }
            e.upgrades_from.clone()
        }
        (_, Some((pc, _)), Some(e)) => {
            if parse_release_version(&e.upgrades_from).is_ok_and(|f| f > *pc) {
                problems.push(format!(
                    "{core_s}'s upgrades_from {} is newer than the previous release {}, which could then not upgrade to it",
                    e.upgrades_from,
                    core_text(*pc)
                ));
            }
            e.upgrades_from.clone()
        }
    };
    if !problems.is_empty() {
        return Err(problems);
    }
    Ok(json!({
        "kind": kind,
        "previous": prev.as_ref().map(|(pc, _)| core_text(*pc)),
        "upgrades_from": upgrades_from,
        "databases": {"store": current.store},
        "migrations": migrations,
        "world_database": current.world_database,
        "world_database_fetch": fetch,
        "config": config,
        "notes": entry.map(|e| e.notes.clone()).unwrap_or_default(),
    }))
}

/// Whether the world database `to` is a later patch of `from`: the same major and minor version,
/// a higher patch (`v0.23.0` to `v0.23.1`).
fn world_patch(from: &str, to: &str) -> bool {
    let parse = |t: &str| parse_release_version(t.strip_prefix('v').unwrap_or(t)).ok();
    match (parse(from), parse(to)) {
        (Some(a), Some(b)) => a[0] == b[0] && a[1] == b[1] && b[2] > a[2],
        _ => false,
    }
}

/// The declaration of `version` for the tree at `ws`, as the module documentation describes.
pub fn for_release(ws: &Path, version: &str) -> Result<Value, String> {
    let tags: Vec<String> = super::git(ws, &["tag", "--list", &format!("{YALAIN_TAG_PREFIX}*")])?
        .lines()
        .map(|l| l.trim().to_owned())
        .collect();
    let read_tree =
        |p: &str| std::fs::read_to_string(ws.join(p)).map_err(|e| format!("reading {p}: {e}"));
    let current = facts(&read_tree)?;
    let entries = parse_entries_from(&read_tree(RELEASES_FILE)?, RELEASES_FILE)?;
    let previous = match previous_tag_of(YALAIN_TAG_PREFIX, &tags, version)? {
        Some(p) => {
            let tag = format!("{YALAIN_TAG_PREFIX}{p}");
            let read_tag = |path: &str| super::git(ws, &["show", &format!("{tag}:{path}")]);
            Some((p, facts(&read_tag)?))
        }
        None => None,
    };
    declare(
        version,
        previous.as_ref().map(|(p, f)| (p.as_str(), f)),
        &current,
        &entries,
    )
    .map_err(|problems| {
        format!(
            "the upgrade declaration of Yalain {version} is refused:\n  {}",
            problems.join("\n  ")
        )
    })
}
