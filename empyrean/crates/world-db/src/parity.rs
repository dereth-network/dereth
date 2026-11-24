//! A world's layer held against the world it stands for, before a server is switched to it.
//!
//! - **A checkpoint** against the dump its `checkpoint.toml` pins: the dump read in the layer's
//!   era, its placements given the lineage's keys (its `keys.toml`), numbered by rule, then compared
//!   record by record, ids aside ([`crate::compare::differences`]). The keys are what a checkpoint
//!   changes of its source, so a checkpoint that is its source has no difference; anything left is
//!   a finding.
//! - **Any layer** against a pack (`against`): Dereth's base against the Launch world it was written
//!   from, say. The world numbers its records by rule and keeps its source's guids, so the two have
//!   no difference.
//!
//! What it says is a list of differences, one a line, the comparison's; none is parity.

use std::path::Path;

use empyrean_common::era::EraId;
use empyrean_content::pack::Pack;
use empyrean_content::WorldContent;

use crate::checkpoint::{apply_keys, Keys};
use crate::compare::{dataset_of, differences};
use crate::files::{default_stamp, write};
use crate::value::R;

/// `c`'s pack, numbered by rule, its dataset id from its rows written.
///
/// # Errors
/// Content the pack refuses.
pub fn pack_of(c: &WorldContent) -> R<Pack> {
    let files = write(c, default_stamp())?;
    let (bytes, _) = c.to_pack(dataset_of(&files)).map_err(|e| e.to_string())?;
    Pack::from_bytes(bytes).map_err(|e| e.to_string())
}

/// The world `source` stands for once the lineage keys it: numbered by rule, its placements given
/// `keys`.
#[must_use]
pub fn keyed(mut source: WorldContent, keys: Option<&Keys>) -> WorldContent {
    crate::canon::number(&mut source);
    if let Some(k) = keys {
        apply_keys(&mut source, &k.map);
    }
    source
}

/// `world` (a layer's pack) against `source` (the world it stands for, as read), keyed by `keys`:
/// every difference, ids aside.
///
/// # Errors
/// A pack that doesn't read.
pub fn against_source(world: &Pack, source: WorldContent, keys: Option<&Keys>) -> R<Vec<String>> {
    let expected = pack_of(&keyed(source, keys))?;
    differences(&expected, world).map_err(|e| e.to_string())
}

/// `world` (a layer's pack) against the dump at `dump`, read in `era` and keyed by `keys`.
///
/// # Errors
/// A dump that doesn't read, or a pack that doesn't.
pub fn against_dump(world: &Pack, dump: &Path, era: EraId, keys: Option<&Keys>) -> R<Vec<String>> {
    let file = std::fs::File::open(dump).map_err(|e| format!("{}: {e}", dump.display()))?;
    let (source, _) = empyrean_content::import::content_for(file, era)
        .map_err(|e| format!("{}: {e}", dump.display()))?;
    against_source(world, source, keys)
}
