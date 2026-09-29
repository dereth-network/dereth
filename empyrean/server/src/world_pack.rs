//! Not ACE: the world database at boot. ACE's `DatabaseManager.World` is a
//! `WorldDatabaseWithEntityCache` over MySQL; here it is [`PackContent`] over `world.pack`, which
//! `empyrean-import` builds from the ACE world-database dump.
//!
//! The pack's path is `server.world_pack_path` in `empyrean.toml`
//! (`GameConfiguration::world_pack_path`), else `./world.pack` ([`DEFAULT_WORLD_PACK_PATH`]). It
//! resolves as every path in `empyrean.toml` does ([`empyrean_common::config_paths`]): `~` is the home
//! directory, a relative path is relative to the configuration file's folder (the working
//! directory without a file). The default value alone is also looked for beside the server
//! executable, so a folder holding the binary and its `world.pack` works however the server is
//! started; a path the operator wrote names one file. No environment variable overrides it.
//!
//! The pack is mapped, its structure checked and its BLAKE3 content hash verified. A pack that is
//! missing or fails a check is logged and gives empty content ([`MemContent`]); the server then
//! refuses to start ([`unusable_pack_message`]). A pack another release wrote (a format or schema
//! version this build does not read) is not rebuilt automatically: the server cannot know which
//! inputs built it (a dump release, patches, JSON content, a published overlay), and rebuilding from
//! the pinned dump alone would silently replace an operator's content. The message names the exact
//! command instead ([`rebuild_command`]), from the dump `empyrean-import fetch` cached when it is
//! there; the importer keeps the old pack as a backup before replacing it.
//! DIVERGE: ACE's `WorldDatabase.Exists(true)` retries the MySQL connection every 5 s until it
//! succeeds; a file either opens or it does not, so there is nothing to wait for.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use empyrean_common::config_paths::PathBase;
use empyrean_common::master_configuration::MasterConfiguration;
use empyrean_common::world_release;
use empyrean_content::corrections::{
    self,
    report::{CorrectionsReport, EntryState},
};
use empyrean_content::import::patch::{Input, InputKind};
use empyrean_content::overlay::{self, BaseInputs, ContentOverlay};
use empyrean_content::pack::hex;
use empyrean_content::{MemContent, PackContent, PackError, WorldDatabase};

/// The pack's path when the configuration names none.
pub use empyrean_common::game_configuration::DEFAULT_WORLD_PACK_PATH;

/// What [`open_world_database`] found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorldPackStatus {
    /// The pack opened and its hash verified.
    Loaded {
        /// Records in the pack (its index entries).
        records: u64,
        /// The header's BLAKE3 content hash, as hex.
        content_hash: String,
    },
    /// No file at the path.
    Missing,
    /// The file is there but is not a valid pack.
    Invalid(String),
    /// The file is a pack another Empyrean release wrote: its format or schema version is not
    /// this build's.
    WrongVersion {
        /// What differs.
        reason: String,
        /// The command that rebuilds it ([`rebuild_command`]).
        rebuild: String,
    },
}

/// The command that rebuilds the pack at `pack` for this build: from the cached ACE-World dump
/// when `cached_sql` names it (`empyrean-import fetch` put it there), else fetching it first.
#[must_use]
pub fn rebuild_command(pack: &Path, cached_sql: Option<&Path>) -> String {
    match cached_sql {
        Some(sql) => format!(
            "empyrean-import --sql \"{}\" --out \"{}\"",
            sql.display(),
            pack.display()
        ),
        None => format!("empyrean-import fetch --pack --out \"{}\"", pack.display()),
    }
}

/// Why the server cannot start on the pack at `path`, and what to do about it; `None` when it
/// loaded.
#[must_use]
pub fn unusable_pack_message(path: &Path, status: &WorldPackStatus) -> Option<String> {
    match status {
        WorldPackStatus::Loaded { .. } => None,
        WorldPackStatus::WrongVersion { reason, rebuild } => Some(format!(
            "{} was built for another Empyrean release ({reason}). Rebuild it for this one with:\n    {rebuild}\nThe importer keeps the old pack beside it as {}.backup-<UTC timestamp>. If the old pack was built with --patches or --json inputs, add the same ones. Empyrean will now abort startup.",
            path.display(),
            path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
        )),
        WorldPackStatus::Missing | WorldPackStatus::Invalid(_) => Some(format!(
            "No usable world database at {}. Build one with `empyrean-import fetch --pack` (it downloads ACE's world database and writes world.pack in the current folder; SETUP.md section 2), then put world.pack beside empyrean.toml or beside the server executable, or point server.world_pack_path in empyrean.toml at it. Empyrean will now abort startup.",
            path.display()
        )),
    }
}

/// The pack's path as configured: `config_key` (`server.world_pack_path`), else
/// [`DEFAULT_WORLD_PACK_PATH`] when it is blank.
#[must_use]
pub fn world_pack_path(config_key: &str) -> PathBuf {
    let key = config_key.trim();
    PathBuf::from(if key.is_empty() {
        DEFAULT_WORLD_PACK_PATH
    } else {
        key
    })
}

/// The pack's file for `config_key` under `base`: the default value (or a blank key) is looked for
/// in the base folder, then beside the executable (`exists` stands for the file system); any other
/// value is resolved by [`PathBase::resolve`] alone.
#[must_use]
pub fn resolve_world_pack_path(
    config_key: &str,
    base: &PathBase,
    exists: &dyn Fn(&Path) -> bool,
) -> PathBuf {
    let key = config_key.trim();
    if key.is_empty() || key == DEFAULT_WORLD_PACK_PATH {
        base.search(DEFAULT_WORLD_PACK_PATH, exists)
    } else {
        base.resolve(key)
    }
}

/// [`resolve_world_pack_path`] for this process: the loaded configuration under `base`, checked on
/// disk.
#[must_use]
pub fn configured_world_pack_path(config: &MasterConfiguration, base: &PathBase) -> PathBuf {
    resolve_world_pack_path(&config.server.world_pack_path, base, &|p| p.exists())
}

/// Opens the world database over the pack at `path`, logging the outcome. A missing or invalid
/// pack gives empty content.
#[must_use]
pub fn open_world_database(path: &Path) -> (Arc<dyn WorldDatabase>, WorldPackStatus) {
    let (content, status, _) = open_world_database_with_overlay(path, None);
    (content, status)
}

/// Not ACE: the content overlay the configuration names (`Server.WorldOverlayPath`),
/// with the base inputs it writes over (`Server.WorldBaseSql`, `Server.WorldBasePatches`), each
/// resolved under `base` ([`PathBase::resolve`]; a `json:` patch entry resolves the path after the
/// marker). `None` when no overlay is configured.
#[must_use]
pub fn configured_overlay(
    config: &MasterConfiguration,
    base: &PathBase,
) -> Option<(PathBuf, BaseInputs)> {
    let s = &config.server;
    if s.world_overlay_path.trim().is_empty() {
        return None;
    }
    let patches = s
        .world_base_patches
        .iter()
        .map(|e| match e.trim().strip_prefix("json:") {
            Some(p) => Input {
                kind: InputKind::Json,
                path: base.resolve(p),
            },
            None => Input {
                kind: InputKind::Sql,
                path: base.resolve(e),
            },
        })
        .collect();
    Some((
        base.resolve(&s.world_overlay_path),
        BaseInputs {
            sql: base.resolve(&s.world_base_sql),
            patches,
        },
    ))
}

/// [`open_world_database`], then, when `overlay` names one and the pack loaded, the content
/// overlay opened (created when new) and put in front of the pack. The third value is `None`
/// without an overlay, else the overlay's outcome: the number of records it holds, or why it
/// could not be opened (the server then refuses to start rather than lose edits).
#[must_use]
pub fn open_world_database_with_overlay(
    path: &Path,
    overlay: Option<(PathBuf, BaseInputs)>,
) -> (
    Arc<dyn WorldDatabase>,
    WorldPackStatus,
    Option<Result<usize, String>>,
) {
    if !path.exists() {
        log::warn!(
            "World database: {} not found (set server.world_pack_path in empyrean.toml); the world has no content",
            path.display()
        );
        return (Arc::new(MemContent::new()), WorldPackStatus::Missing, None);
    }

    let start = Instant::now();
    match open_verified(path) {
        Ok(content) => {
            let header = content.base().pack().header();
            let records = header.index_count;
            let tables = header.table_count;
            let content_hash = hex(&header.content_hash);
            log::info!(
                "World database: opened {} ({records} records in {tables} tables) in {:.2}s",
                path.display(),
                start.elapsed().as_secs_f64()
            );
            // The pack's hash covers the data as stored; the corrections digest, what this build
            // changes as it reads it. The two together name the world the game sees.
            log::info!(
                "World database: content hash {content_hash} (verified), corrections {}",
                corrections::digest()
            );
            let overlay = overlay.map(|(file, base)| attach_overlay(&content, &file, base));
            let content = Arc::new(content);
            report_corrections(Arc::clone(&content));
            (
                content,
                WorldPackStatus::Loaded {
                    records,
                    content_hash,
                },
                overlay,
            )
        }
        Err(e @ (PackError::FormatVersion { .. } | PackError::SchemaVersion { .. })) => {
            let reason = e.to_string();
            log::error!(
                "World database: {} was built for another Empyrean release: {reason}; the world has no content",
                path.display()
            );
            let cached = world_release::cached_sql(world_release::PINNED_TAG);
            (
                Arc::new(MemContent::new()),
                WorldPackStatus::WrongVersion {
                    reason,
                    rebuild: rebuild_command(path, cached.as_deref()),
                },
                None,
            )
        }
        Err(e) => {
            let message = e.to_string();
            log::error!(
                "World database: {} is not a usable world.pack: {message}; the world has no content",
                path.display()
            );
            (
                Arc::new(MemContent::new()),
                WorldPackStatus::Invalid(message),
                None,
            )
        }
    }
}

/// Not ACE: open the overlay at `file` over `content`'s pack and attach it.
fn attach_overlay(content: &PackContent, file: &Path, base: BaseInputs) -> Result<usize, String> {
    let header = content.base().pack().header();
    let overlay =
        ContentOverlay::open(file, base, header, overlay::default_now()).map_err(|e| {
            log::error!(
                "World database: overlay {} could not be opened: {e}",
                file.display()
            );
            e.to_string()
        })?;
    let records = overlay.layer().len();
    let journal = overlay.journal().map(|j| j.len()).unwrap_or(0);
    log::info!(
        "World database: overlay {} ({journal} content file(s) applied, {records} record(s) over world.pack)",
        file.display()
    );
    if overlay.base().sql.as_os_str().is_empty() || !overlay.base().sql.exists() {
        log::warn!(
            "World database: the overlay has no base dump (Server.WorldBaseSql = {}); content commands that write will fail",
            overlay.base().sql.display()
        );
    }
    content
        .base()
        .attach_overlay(Arc::new(overlay))
        .map_err(|_| "an overlay is already attached".to_owned())?;
    Ok(records)
}

/// Not ACE: logs what the corrections do to the loaded content (overlay included), on a thread of
/// its own since it reads every weenie: how many entries apply, and how many values each rule
/// changes (info), and each entry that is stale or absent (warn: the content no longer holds the
/// value the entry corrects, see `empyrean-import --corrections`).
fn report_corrections(content: Arc<PackContent>) {
    let spawned = std::thread::Builder::new()
        .name("corrections-report".to_owned())
        .spawn(move || {
            let r = CorrectionsReport::of(content.base());
            log::info!("World database: {}", corrections_summary(&r));
            for e in r
                .weenie_entries
                .iter()
                .filter(|e| e.state != EntryState::Applies)
            {
                let c = e.correction;
                log::warn!(
                    "World database: correction {} for weenie {} ({}) is {}: {}",
                    c.divergence,
                    c.weenie_class_id,
                    corrections::report::property_text(c.stored),
                    e.state.label(),
                    match &e.state {
                        EntryState::Stale(found) => format!("the content stores {found}"),
                        _ => "the content has no such weenie".to_owned(),
                    }
                );
            }
            for e in r
                .spell_entries
                .iter()
                .filter(|e| e.state != EntryState::Applies)
            {
                log::warn!(
                    "World database: correction {} for spell {} is {}",
                    e.correction.divergence,
                    e.correction.spell_id,
                    e.state.label()
                );
            }
        });
    if let Err(e) = spawned {
        log::warn!("World database: the corrections report could not start: {e}");
    }
}

/// One line: the digest, the entries that apply, and what each rule changes.
#[must_use]
pub fn corrections_summary(r: &CorrectionsReport) -> String {
    let s = r.summary();
    format!(
        "corrections {}: {} of {} entries apply ({} stale, {} absent); the rules change {} default scripts and {} emote motions",
        r.digest, s.applies, s.entries, s.stale, s.absent, s.play_script_shifts, s.emote_motion_shifts
    )
}

fn open_verified(path: &Path) -> Result<PackContent, PackError> {
    let content = PackContent::open(path)?;
    content.base().pack().verify_hash()?;
    Ok(content)
}
