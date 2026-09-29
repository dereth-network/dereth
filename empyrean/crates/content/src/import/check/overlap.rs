//! `empyrean-import --check --overlap`: the records an upstream change reaches that our own
//! content also sets.
//!
//! Not ACE-derived. A content patch that replaces an object (`DELETE` + `INSERT`) replaces it
//! whole, so when upstream fixes the same object the patch silently keeps the old version; an
//! overlay record does the same; a correction entry or rule rewrites a value as it is read. This
//! lists every record the two packs disagree on that one of ours touches, so each can be looked
//! at by hand.

use std::collections::BTreeMap;
use std::path::Path;

use super::TableDiff;
use crate::corrections::{self, report::property_text};
use crate::error::{ImportError, PackError};
use crate::import::patch::{self, Input};
use crate::models::world::Weenie;
use crate::overlay::file::OverlayFile;
use crate::pack::{Pack, TableId};

/// Our records: `(table, key)` and what of ours touches it, e.g. `patch content/33862.sql`.
pub type Ours = Vec<((TableId, u64), String)>;

/// The records the content files in `inputs` reach when applied over the dump at `base`: every
/// record a file's statements touch, whether or not it ends up changed.
pub fn ours_from_patches(
    base: &Path,
    inputs: &[Input],
    now: empyrean_common::dotnet::DotNetDateTime,
) -> Result<Ours, ImportError> {
    let file = std::fs::File::open(base).map_err(|e| ImportError::io(base, e))?;
    let loaded = crate::import::load_store(file, patch::sources(inputs), now)?;
    Ok(loaded
        .applied
        .iter()
        .flat_map(|a| {
            a.touched
                .iter()
                .map(move |&k| (k, format!("patch {}", a.path)))
        })
        .collect())
}

/// The records an overlay file sets or deletes.
///
/// # Errors
/// When the file is missing or is not an overlay.
pub fn ours_from_overlay(path: &Path) -> Result<Ours, String> {
    if !path.is_file() {
        return Err(format!("{}: no such overlay", path.display()));
    }
    let file = OverlayFile::open(path).map_err(|e| e.to_string())?;
    file.meta().map_err(|e| e.to_string())?;
    let layer = file.layer().map_err(|e| e.to_string())?;
    let shown = format!("overlay {}", path.display());
    Ok(layer
        .iter()
        .map(|(t, k, _)| ((t, k), shown.clone()))
        .collect())
}

/// The records this build's corrections touch among `diffs`' changed weenies and spells: each
/// entry's weenie or spell, and each weenie a rule changes in either pack.
pub fn ours_from_corrections(
    diffs: &[TableDiff],
    old: &Pack,
    new: &Pack,
) -> Result<Ours, PackError> {
    let mut out: Ours = Vec::new();
    for c in corrections::WEENIE_CORRECTIONS {
        out.push((
            (TableId::WEENIE, u64::from(c.weenie_class_id)),
            format!("correction {} {}", c.divergence, property_text(c.stored)),
        ));
    }
    for c in corrections::SPELL_CORRECTIONS {
        out.push((
            (TableId::SPELL, u64::from(c.spell_id)),
            format!("correction {} spell wcid", c.divergence),
        ));
    }
    for d in diffs.iter().filter(|d| d.id == TableId::WEENIE.0) {
        for &key in d.added.iter().chain(&d.changed).chain(&d.removed) {
            let (mut script, mut motion) = (false, false);
            for p in [old, new] {
                if let Some(w) = p.get::<Weenie>(TableId::WEENIE, key)? {
                    script |= corrections::play_script_shift(&w).is_some();
                    motion |= !corrections::emote_motion_shifts_of(&w).is_empty();
                }
            }
            if script {
                out.push((
                    (TableId::WEENIE, key),
                    format!(
                        "rule play_script_shift {}",
                        corrections::PLAY_SCRIPT_SHIFT_DIVERGENCE
                    ),
                ));
            }
            if motion {
                out.push((
                    (TableId::WEENIE, key),
                    format!(
                        "rule emote_motion_shift {}",
                        corrections::EMOTE_MOTION_SHIFT_DIVERGENCE
                    ),
                ));
            }
        }
    }
    Ok(out)
}

/// One record upstream changed that ours touches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Overlap {
    pub table: &'static str,
    pub id: u16,
    pub key: u64,
    /// `+` added, `~` changed, `-` removed upstream.
    pub mark: char,
    /// What of ours touches it, sorted, without repeats.
    pub ours: Vec<String>,
}

/// The records of `diffs` that `ours` touches, in table and key order (`weenie_index` is left
/// out: it follows `weenie`).
#[must_use]
pub fn overlap(diffs: &[TableDiff], ours: &Ours) -> Vec<Overlap> {
    let mut by_key: BTreeMap<(u16, u64), Vec<String>> = BTreeMap::new();
    for ((t, k), what) in ours {
        by_key.entry((t.0, *k)).or_default().push(what.clone());
    }
    let mut out = Vec::new();
    for d in diffs.iter().filter(|d| d.id != TableId::WEENIE_INDEX.0) {
        let mut hits: Vec<(u64, char)> = Vec::new();
        for (mark, keys) in [('+', &d.added), ('~', &d.changed), ('-', &d.removed)] {
            hits.extend(
                keys.iter()
                    .filter(|k| by_key.contains_key(&(d.id, **k)))
                    .map(|&k| (k, mark)),
            );
        }
        hits.sort_unstable();
        for (key, mark) in hits {
            let mut ours = by_key[&(d.id, key)].clone();
            ours.sort();
            ours.dedup();
            out.push(Overlap {
                table: d.table,
                id: d.id,
                key,
                mark,
                ours,
            });
        }
    }
    out
}
