//! What upgrading to a release involves: the `upgrade` object of its `release.json`, which the
//! server's updater reads before it installs anything.
//!
//! Derived from the code, at the release's commit and at the previous release's tag:
//!
//! - the shard and authentication schema versions (the migrations `empyrean-store` lists), and
//!   the migrations between the previous release and this one;
//! - the `world.pack` container format and record schema `empyrean-content` reads, and whether the
//!   pack must be rebuilt since the previous release;
//! - the configuration keys this release renames (`toml_config::RENAMED` entries whose `since` is
//!   this version).
//!
//! Declared by hand in `empyrean/releases.toml`, for what the code cannot say: the oldest version
//! that can upgrade straight to the release, configuration keys removed or newly required, and
//! notes. Every minor, major and first release has an entry there; a **patch release has none and
//! changes none of the above**: a patch release with an entry, a migration, a pack change or a
//! renamed key is refused.
//!
//! The previous release is the newest `empyrean-v*` tag (not a pre-release) older than this
//! version's `MAJOR.MINOR.PATCH`; a pre-release carries the declaration of the release it
//! precedes.

use std::path::Path;

use serde_json::{json, Value};

use super::version::{parse_release_version, TAG_PREFIX};

/// The hand-written declarations.
pub const RELEASES_FILE: &str = "empyrean/releases.toml";
/// The shard schema's migrations.
pub const SHARD_SOURCE: &str = "empyrean/crates/store/src/sqlite_shard.rs";
/// The authentication schema's migrations.
pub const AUTH_SOURCE: &str = "empyrean/crates/store/src/sqlite_auth.rs";
/// The world pack's format and record schema.
pub const PACK_SOURCE: &str = "empyrean/crates/content/src/pack/format.rs";
/// The configuration's renamed keys.
pub const CONFIG_SOURCE: &str = "empyrean/crates/common/src/toml_config.rs";

/// A key renamed in the configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Renamed {
    pub old: String,
    pub new: String,
    pub since: String,
}

/// What an upgrade depends on, as the code of one commit has it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Facts {
    pub shard: i64,
    pub auth: i64,
    pub pack_format: u32,
    pub pack_schema: u32,
    pub renamed: Vec<Renamed>,
}

/// How many `include_str!("schema/<prefix>...")` migrations `text` lists.
fn migrations(text: &str, prefix: &str) -> i64 {
    let needle = format!("include_str!(\"schema/{prefix}");
    i64::try_from(text.matches(&needle).count()).unwrap_or(i64::MAX)
}

/// The value of `pub const <name>: u32 = <n>;` in `text`.
fn const_u32(text: &str, name: &str, file: &str) -> Result<u32, String> {
    let head = format!("pub const {name}: u32 = ");
    text.lines()
        .find_map(|l| l.trim().strip_prefix(head.as_str()))
        .and_then(|rest| rest.strip_suffix(';'))
        .and_then(|n| n.trim().replace('_', "").parse().ok())
        .ok_or_else(|| format!("{file} has no `{head}<number>;` line"))
}

/// The entries of `pub const RENAMED: &[Renamed] = &[ ... ];` in `text`.
fn renamed(text: &str) -> Result<Vec<Renamed>, String> {
    let head = "pub const RENAMED: &[Renamed] = &[";
    let start = text
        .find(head)
        .ok_or_else(|| format!("{CONFIG_SOURCE} has no `{head}`"))?
        + head.len();
    let body = &text[start..];
    let end = body
        .find("];")
        .ok_or_else(|| format!("{CONFIG_SOURCE}: RENAMED is not closed"))?;
    let body = &body[..end];
    let field = |name: &str| -> Vec<String> {
        let key = format!("{name}: \"");
        body.match_indices(&key)
            .filter_map(|(i, _)| {
                let rest = &body[i + key.len()..];
                rest.find('"').map(|j| rest[..j].to_owned())
            })
            .collect()
    };
    let (old, new, since) = (field("old"), field("new"), field("since"));
    if old.len() != new.len() || old.len() != since.len() {
        return Err(format!(
            "{CONFIG_SOURCE}: each RENAMED entry needs old, new and since, written as `old: \"...\"`"
        ));
    }
    Ok(old
        .into_iter()
        .zip(new)
        .zip(since)
        .map(|((old, new), since)| Renamed { old, new, since })
        .collect())
}

/// The facts of a tree whose files `read` returns (by path from the workspace root).
pub fn facts(read: &dyn Fn(&str) -> Result<String, String>) -> Result<Facts, String> {
    let pack = read(PACK_SOURCE)?;
    let f = Facts {
        shard: migrations(&read(SHARD_SOURCE)?, "shard_v"),
        auth: migrations(&read(AUTH_SOURCE)?, "auth_v"),
        pack_format: const_u32(&pack, "FORMAT_VERSION", PACK_SOURCE)?,
        pack_schema: const_u32(&pack, "SCHEMA_VERSION", PACK_SOURCE)?,
        renamed: renamed(&read(CONFIG_SOURCE)?)?,
    };
    if f.shard == 0 || f.auth == 0 {
        return Err(format!(
            "no migrations found in {SHARD_SOURCE} or {AUTH_SOURCE} (each lists them as include_str!(\"schema/...\"))"
        ));
    }
    Ok(f)
}

/// A configuration key a release removes or newly requires.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigEntry {
    pub key: String,
    pub change: String,
    pub note: String,
}

/// One `[[release]]` of `releases.toml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub version: String,
    pub upgrades_from: String,
    pub config: Vec<ConfigEntry>,
    pub notes: String,
}

/// Reads `releases.toml`.
pub fn parse_entries(text: &str) -> Result<Vec<Entry>, String> {
    let table: toml::Table = toml::from_str(text).map_err(|e| format!("{RELEASES_FILE}: {e}"))?;
    let mut out: Vec<Entry> = Vec::new();
    let list = match table.get("release") {
        None => return Ok(out),
        Some(toml::Value::Array(a)) => a,
        Some(_) => {
            return Err(format!(
                "{RELEASES_FILE}: `release` must be [[release]] tables"
            ))
        }
    };
    for item in list {
        let t = item
            .as_table()
            .ok_or_else(|| format!("{RELEASES_FILE}: `release` must be [[release]] tables"))?;
        let s = |k: &str| t.get(k).and_then(toml::Value::as_str).map(str::to_owned);
        let version =
            s("version").ok_or_else(|| format!("{RELEASES_FILE}: a [[release]] has no version"))?;
        let at = |what: &str| format!("{RELEASES_FILE}, release {version}: {what}");
        for key in t.keys() {
            if !["version", "upgrades_from", "config", "notes"].contains(&key.as_str()) {
                return Err(at(&format!("unknown key `{key}`")));
            }
        }
        let core = parse_release_version(&version).map_err(|e| at(&e))?;
        if version.contains('-') {
            return Err(at(
                "a pre-release carries the declaration of its release: name the release",
            ));
        }
        let upgrades_from = s("upgrades_from").ok_or_else(|| at("no upgrades_from"))?;
        let from = parse_release_version(&upgrades_from).map_err(|e| at(&e))?;
        if upgrades_from.contains('-') || from > core {
            return Err(at(&format!(
                "upgrades_from {upgrades_from} must be a release no newer than {version}"
            )));
        }
        let mut config = Vec::new();
        if let Some(v) = t.get("config") {
            let arr = v.as_array().ok_or_else(|| at("config must be a list"))?;
            for c in arr {
                let ct = c
                    .as_table()
                    .ok_or_else(|| at("each config change is a table"))?;
                let g = |k: &str| ct.get(k).and_then(toml::Value::as_str).map(str::to_owned);
                let key = g("key").ok_or_else(|| at("a config change has no key"))?;
                let change = g("change").unwrap_or_default();
                if change != "removed" && change != "required" {
                    return Err(at(&format!(
                        "config change of {key}: change is \"removed\" or \"required\" (a rename is declared in RENAMED)"
                    )));
                }
                config.push(ConfigEntry {
                    key,
                    change,
                    note: g("note").unwrap_or_default(),
                });
            }
        }
        if out.iter().any(|e| e.version == version) {
            return Err(at("declared twice"));
        }
        out.push(Entry {
            version,
            upgrades_from,
            config,
            notes: s("notes").unwrap_or_default(),
        });
    }
    Ok(out)
}

fn core_text(v: [u64; 3]) -> String {
    format!("{}.{}.{}", v[0], v[1], v[2])
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
    let mut pack_rebuild = false;
    if let Some((pc, pf)) = &prev {
        for (db, from, to) in [
            ("shard", pf.shard, current.shard),
            ("authentication", pf.auth, current.auth),
        ] {
            if to < from {
                problems.push(format!(
                    "the {db} schema went back from {from} (in {}) to {to}: a released migration is never removed",
                    core_text(*pc)
                ));
            } else if to > from {
                migrations.push(json!({"database": db, "from": from, "to": to}));
            }
        }
        pack_rebuild =
            (pf.pack_format, pf.pack_schema) != (current.pack_format, current.pack_schema);
    }
    let mut config: Vec<Value> = current
        .renamed
        .iter()
        .filter(|r| r.since == core_s)
        .map(|r| json!({"key": r.old, "change": "renamed", "to": r.new}))
        .collect();
    let renames = config.len();
    if let Some(e) = entry {
        config.extend(e.config.iter().map(|c| {
            let mut v = json!({"key": c.key, "change": c.change});
            if !c.note.is_empty() {
                v["note"] = json!(c.note);
            }
            v
        }));
    }

    let upgrades_from = match (kind, &prev, entry) {
        ("patch", Some((pc, _)), e) => {
            if e.is_some() {
                problems.push(format!(
                    "{core_s} is a patch release, which declares nothing: remove its entry from {RELEASES_FILE}"
                ));
            }
            if !migrations.is_empty() {
                problems.push(format!(
                    "{core_s} is a patch release, but its databases migrate: {}",
                    migrations
                        .iter()
                        .map(|m| format!(
                            "{} {} -> {}",
                            m["database"].as_str().unwrap_or(""),
                            m["from"],
                            m["to"]
                        ))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            if pack_rebuild {
                problems.push(format!(
                    "{core_s} is a patch release, but world.pack changes (format/schema {}/{} -> {}/{})",
                    prev.as_ref().map_or(0, |p| p.1.pack_format),
                    prev.as_ref().map_or(0, |p| p.1.pack_schema),
                    current.pack_format,
                    current.pack_schema
                ));
            }
            if renames > 0 {
                problems.push(format!(
                    "{core_s} is a patch release, but it renames configuration keys (RENAMED since {core_s})"
                ));
            }
            // Inherited: whatever could upgrade straight to the previous release still can.
            entries
                .iter()
                .filter_map(|e| parse_release_version(&e.version).ok().map(|v| (v, e)))
                .filter(|(v, _)| v <= pc)
                .max_by_key(|(v, _)| *v)
                .map_or_else(|| core_text(*pc), |(_, e)| e.upgrades_from.clone())
        }
        (_, _, None) => {
            problems.push(format!(
                "{RELEASES_FILE} has no [[release]] for {core_s}: a {kind} release declares the oldest version that can upgrade straight to it (and any configuration keys it removes or requires)"
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
        "databases": {"shard": current.shard, "auth": current.auth},
        "world_pack": {"format": current.pack_format, "schema": current.pack_schema},
        "migrations": migrations,
        "world_pack_rebuild": pack_rebuild,
        "config": config,
        "notes": entry.map(|e| e.notes.clone()).unwrap_or_default(),
    }))
}

/// The newest release tag among `tags` (not a pre-release) older than `version`'s core.
pub fn previous_tag(tags: &[String], version: &str) -> Result<Option<String>, String> {
    let core = parse_release_version(version)?;
    Ok(tags
        .iter()
        .filter_map(|t| {
            let v = t.strip_prefix(TAG_PREFIX)?;
            if v.contains('-') {
                return None;
            }
            let c = parse_release_version(v).ok()?;
            (c < core).then(|| (c, v.to_owned()))
        })
        .max_by_key(|(c, _)| *c)
        .map(|(_, v)| v))
}

/// The declaration of `version` for the tree at `ws`, as the module documentation describes.
pub fn for_release(ws: &Path, version: &str) -> Result<Value, String> {
    let tags: Vec<String> = super::git(ws, &["tag", "--list", "empyrean-v*"])?
        .lines()
        .map(|l| l.trim().to_owned())
        .collect();
    let read_tree =
        |p: &str| std::fs::read_to_string(ws.join(p)).map_err(|e| format!("reading {p}: {e}"));
    let current = facts(&read_tree)?;
    let entries = parse_entries(&read_tree(RELEASES_FILE)?)?;
    let previous = match previous_tag(&tags, version)? {
        Some(p) => {
            let tag = format!("{TAG_PREFIX}{p}");
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
            "the upgrade declaration of {version} is refused:\n  {}",
            problems.join("\n  ")
        )
    })
}
