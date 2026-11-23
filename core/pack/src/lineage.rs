//! A world's patch layers: the lineage its world file lists, each patch's folder and what it says
//! of itself, and a checkpoint's pin and its acknowledged drift.
//!
//! A world whose game has them ([`Dialect::NAMES_LINEAGE`]) is ordered layers. The first, the
//! **base**, is the world's own packs (`packs` and `[[override]]` in `world.toml`); each later
//! layer is a **patch**, the game as it stood just before the next monthly patch, listed oldest
//! first in `lineage`. The world at a layer is every layer up to it, folded ([`crate::fold`]).
//!
//! ```text
//! world.toml                              packs (the base's), [[override]], lineage = ["1999-12-dark-thaw", ...]
//! patches/<yyyy-mm>-<name>/patch.toml     id, title, date, after, packs, [[override]], <dat>_iteration
//! patches/<yyyy-mm>-<name>/packs/<pack>/  the patch's packs, in the order patch.toml lists them
//! patches/<yyyy-mm>-<name>/checkpoint.toml  for a checkpoint: the snapshot it pins
//! patches/<yyyy-mm>-<name>/drift.toml       for a checkpoint: every disagreement, kept or given to the snapshot
//! ```
//!
//! ```toml
//! # patches/1999-12-dark-thaw/patch.toml
//! id = "1999-12-dark-thaw"       # its folder's name: the month it is the game of, and a name
//! title = "Dark Thaw"
//! date = "1999-12-20"            # the day it took effect, in that month
//! after = "1999-11-launch"       # the layer before it: the previous patch, or "" for the first
//! packs = ["lifestones"]         # its packs, in load order, under packs/
//! portal_iteration = 112         # each of the data files' iterations it serves, when known
//!
//! [[override]]                   # within the layer, which of two packs wins ([`crate::conflict`])
//! pack = "lifestones"
//! over = "town-fixes"
//! ```
//!
//! A **checkpoint** is a patch with a `checkpoint.toml`: a known world pinned to its source, whose
//! rows the game's builder converts when a build needs them, never stored. What it changes is
//! computed: what its snapshot changed since the previous checkpoint's ([`crate::fold`]).
//!
//! ```toml
//! # patches/2005-02-infiltration/checkpoint.toml
//! source = "ACE-World-16PY"      # the project, as provenance names it (`from = "source"`)
//! revision = "v0.8.8"            # its release or commit
//! url = "https://..."            # where it is fetched from
//! sha256 = "..."                 # what it must hash to
//! converter = "..."              # the converter's commit
//! # any other keys are the game's own, kept for its builder
//! ```
//!
//! Every field where the world folded up to a checkpoint and the checkpoint's snapshot disagree is
//! acknowledged in its `drift.toml`, or the check fails:
//!
//! ```toml
//! [[drift]]
//! item = "class 509"             # the item, as an override names it
//! field = "use"                  # one of its fields; left out, the whole item
//! take = "keep"                  # keep: the folded world stands; snapshot: the snapshot's wins
//! note = "the snapshot never had this fix"
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use toml::Spanned;

use crate::conflict::{Override, Scope};
use crate::dialect::{check_game, Dialect};
use crate::error::{Error, Loc};
use crate::store::{DiskStore, SourceStore};
use crate::symbol;
use crate::world::{overrides as read_overrides, OverrideRow, World};

/// The id the base layer goes by in reports.
pub const BASE: &str = "base";

/// The folder a world's patches are in, beside its world file.
pub const PATCHES: &str = "patches";

/// The folder a patch's packs are in, inside the patch's own.
pub const PACKS: &str = "packs";

/// A world's layers, read: the base, then each patch, oldest first.
#[derive(Clone, Debug, PartialEq)]
pub struct Lineage {
    pub layers: Vec<LayerDef>,
}

/// One layer, as its files describe it.
#[derive(Clone, Debug, PartialEq)]
pub struct LayerDef {
    /// The patch's id, its folder's name; [`BASE`] for the base.
    pub id: String,
    /// Its folder: the world's for the base, `patches/<id>` for a patch.
    pub dir: PathBuf,
    /// What a patch says of itself; `None` for the base.
    pub patch: Option<Patch>,
    /// Its packs' folders, in load order.
    pub packs: Vec<PathBuf>,
    /// Which of its packs wins where two set one field differently.
    pub overrides: Vec<Override>,
    /// The snapshot a checkpoint pins; `None` for a delta.
    pub checkpoint: Option<Pin>,
    /// A checkpoint's acknowledged drift.
    pub drift: Vec<Ack>,
}

impl LayerDef {
    /// Whether the layer is a checkpoint.
    #[must_use]
    pub fn is_checkpoint(&self) -> bool {
        self.checkpoint.is_some()
    }
}

/// A patch's `patch.toml`, read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Patch {
    pub id: String,
    pub title: String,
    /// The day it took effect, `YYYY-MM-DD`.
    pub date: String,
    /// The layer before it; empty for the first patch.
    pub after: String,
    /// The data files' iterations it serves, by file (`portal` for `portal_iteration`).
    pub iterations: BTreeMap<String, u32>,
}

/// What a checkpoint pins: the snapshot its rows are converted from.
#[derive(Clone, Debug, PartialEq)]
pub struct Pin {
    /// The project, as a row's provenance names it.
    pub source: String,
    /// Its release or commit.
    pub revision: String,
    pub url: Option<String>,
    pub sha256: Option<String>,
    /// The converter's commit.
    pub converter: Option<String>,
    /// The game's own keys, for its builder.
    pub rest: toml::Table,
}

/// Which side of a drift stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Take {
    /// The folded world stands.
    Keep,
    /// The snapshot's value wins.
    Snapshot,
}

/// One acknowledged drift of a checkpoint: an item, or one field of it, and which side stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ack {
    /// The item, in the one form reports name it.
    pub item: String,
    /// One field; `None` for the whole item, every field of it.
    pub field: Option<String>,
    pub take: Take,
    pub note: Option<String>,
    /// Where `drift.toml` says it.
    pub at: Loc,
}

#[derive(Deserialize)]
struct LineageOf {
    game: Option<String>,
    #[serde(default)]
    packs: Vec<String>,
    #[serde(default, rename = "override")]
    overrides: Vec<Spanned<OverrideRow>>,
    #[serde(default)]
    lineage: Vec<String>,
}

// Not `deny_unknown_fields`, which a flattened table defeats: every other key is checked to be
// an iteration.
#[derive(Deserialize)]
struct PatchFile {
    #[serde(default, rename = "format_version")]
    _format_version: u32,
    id: String,
    title: String,
    date: String,
    #[serde(default)]
    after: String,
    #[serde(default)]
    packs: Vec<String>,
    #[serde(default, rename = "override")]
    overrides: Vec<Spanned<OverrideRow>>,
    #[serde(flatten)]
    rest: BTreeMap<String, toml::Value>,
}

#[derive(Deserialize)]
struct PinFile {
    source: String,
    revision: String,
    url: Option<String>,
    sha256: Option<String>,
    converter: Option<String>,
    #[serde(flatten)]
    rest: toml::Table,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DriftFile {
    #[serde(default)]
    drift: Vec<Spanned<DriftRow>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DriftRow {
    item: String,
    field: Option<String>,
    take: Take,
    note: Option<String>,
}

/// Whether `id` names a patch: `<yyyy-mm>-<name>`, the month a year and a month of it, the name
/// lowercase letters, digits and hyphens.
#[must_use]
pub fn is_patch_id(id: &str) -> bool {
    month_of(id).is_some()
}

/// The `yyyy-mm` a patch id starts with, when it is one.
fn month_of(id: &str) -> Option<&str> {
    let b = id.as_bytes();
    let digits =
        |r: std::ops::Range<usize>| b.get(r).is_some_and(|d| d.iter().all(u8::is_ascii_digit));
    if b.len() < 9 || !digits(0..4) || b[4] != b'-' || !digits(5..7) || b[7] != b'-' {
        return None;
    }
    let month: u32 = id[5..7].parse().ok()?;
    ((1..=12).contains(&month) && symbol::is_name(&id[8..])).then(|| &id[..7])
}

/// Whether `date` is a day, `YYYY-MM-DD`.
fn is_date(date: &str) -> bool {
    let b = date.as_bytes();
    let digits =
        |r: std::ops::Range<usize>| b.get(r).is_some_and(|d| d.iter().all(u8::is_ascii_digit));
    if b.len() != 10
        || !digits(0..4)
        || b[4] != b'-'
        || !digits(5..7)
        || b[7] != b'-'
        || !digits(8..10)
    {
        return false;
    }
    let (m, d): (u32, u32) = (
        date[5..7].parse().unwrap_or(0),
        date[8..10].parse().unwrap_or(0),
    );
    (1..=12).contains(&m) && (1..=31).contains(&d)
}

impl Lineage {
    /// The layers of `world`, read from disk: its own packs as the base, then each patch it lists.
    ///
    /// # Errors
    /// As [`Lineage::read`].
    pub fn of_world<D: Dialect>(world: &World<D>) -> Result<Self, Error> {
        Self::of_world_in(&DiskStore, world)
    }

    /// The layers of `world`, read from `store`.
    ///
    /// # Errors
    /// As [`Lineage::read`].
    pub fn of_world_in<D: Dialect>(
        store: &dyn SourceStore,
        world: &World<D>,
    ) -> Result<Self, Error> {
        let dir = world.path.parent().unwrap_or(Path::new("."));
        Self::read::<D>(
            store,
            dir,
            world.packs.clone(),
            world.overrides.clone(),
            &world.lineage,
        )
    }

    /// The layers a world file names, read from `store`, for a game whose world file says more
    /// than [`World`] reads: only its `packs`, `[[override]]` and `lineage` are read here (and
    /// `game`, which must be `D`'s), the rest left to the game's builder.
    ///
    /// # Errors
    /// As [`Lineage::read`], and a world file that does not read.
    pub fn read_world_file<D: Dialect>(
        store: &dyn SourceStore,
        path: &Path,
    ) -> Result<Self, Error> {
        let text = store.read_to_string(path).map_err(|e| Error::io(path, e))?;
        crate::format::check::<D>(path, &text)?;
        let file: LineageOf = toml::from_str(&text).map_err(|e| Error::toml(path, &text, &e))?;
        check_game::<D>(file.game.as_deref(), "world")
            .map_err(|why| Error::source(&Loc::new(path, 0), why))?;
        let overrides = read_overrides::<D>(path, &text, &file.overrides)?;
        let dir = path.parent().unwrap_or(Path::new("."));
        let packs = file.packs.iter().map(|p| dir.join(p)).collect();
        Self::read::<D>(store, dir, packs, overrides, &file.lineage)
    }

    /// The layers of a world in `dir`: the base of `packs` and `overrides`, then each patch of
    /// `lineage` from `dir/patches/<id>/`.
    ///
    /// # Errors
    /// A patch listed twice or not named `<yyyy-mm>-<name>`; a `patch.toml` that does not read,
    /// names another id, comes `after` a layer other than the one before it, or is dated outside
    /// its month or before the patch before it; an iteration that is not one; a pack folder that
    /// is not a name; a `checkpoint.toml` or `drift.toml` that does not read; a drift that names
    /// no item; a `drift.toml` beside no `checkpoint.toml`.
    pub fn read<D: Dialect>(
        store: &dyn SourceStore,
        dir: &Path,
        packs: Vec<PathBuf>,
        overrides: Vec<Override>,
        lineage: &[String],
    ) -> Result<Self, Error> {
        let mut layers = vec![LayerDef {
            id: BASE.to_owned(),
            dir: dir.to_path_buf(),
            patch: None,
            packs,
            overrides,
            checkpoint: None,
            drift: Vec::new(),
        }];
        let world = Loc::new(&dir.join("world.toml"), 0);
        let mut last_date: Option<String> = None;
        for (i, id) in lineage.iter().enumerate() {
            let Some(month) = month_of(id) else {
                return Err(Error::source(
                    &world,
                    format!("lineage: {id:?} is not a patch: <yyyy-mm>-<name>, the name lowercase letters, digits and hyphens"),
                ));
            };
            if lineage[..i].contains(id) {
                return Err(Error::source(
                    &world,
                    format!("lineage: {id} is listed twice"),
                ));
            }
            let pdir = dir.join(PATCHES).join(id);
            let path = pdir.join("patch.toml");
            let text = store
                .read_to_string(&path)
                .map_err(|e| Error::io(&path, e))?;
            crate::format::check::<D>(&path, &text)?;
            let file: PatchFile =
                toml::from_str(&text).map_err(|e| Error::toml(&path, &text, &e))?;
            let whole = Loc::new(&path, 0);
            if file.id != *id {
                return Err(Error::source(
                    &whole,
                    format!("id: {:?}, but the patch's folder is {id}", file.id),
                ));
            }
            let before = if i == 0 { "" } else { lineage[i - 1].as_str() };
            if file.after != before {
                return Err(Error::source(
                    &whole,
                    format!(
                        "after: {:?}, but the layer before it in the lineage is {}",
                        file.after,
                        if before.is_empty() {
                            "the base, written \"\"".to_owned()
                        } else {
                            before.to_owned()
                        }
                    ),
                ));
            }
            if !is_date(&file.date) {
                return Err(Error::source(
                    &whole,
                    format!("date: {:?} is not a day, YYYY-MM-DD", file.date),
                ));
            }
            if &file.date[..7] != month {
                return Err(Error::source(
                    &whole,
                    format!(
                        "date: {} is not in {month}, the month the patch's folder names",
                        file.date
                    ),
                ));
            }
            if let Some(last) = last_date.as_deref().filter(|l| *l > file.date.as_str()) {
                return Err(Error::source(
                    &whole,
                    format!(
                        "date: {} is before {last}, the date of the patch before it",
                        file.date
                    ),
                ));
            }
            last_date = Some(file.date.clone());
            let mut iterations = BTreeMap::new();
            for (key, value) in &file.rest {
                let line = text
                    .lines()
                    .position(|l| l.trim_start().starts_with(key.as_str()))
                    .map_or(0, |n| n + 1);
                let at = Loc::new(&path, line);
                let Some(dat) = key.strip_suffix("_iteration").filter(|d| !d.is_empty()) else {
                    return Err(Error::source(
                        &at,
                        format!("unknown field `{key}`, expected `id`, `title`, `date`, `after`, `packs`, `override` or a data file's `<name>_iteration`"),
                    ));
                };
                let n = value
                    .as_integer()
                    .and_then(|n| u32::try_from(n).ok())
                    .ok_or_else(|| {
                        Error::source(&at, format!("{key}: an iteration, a whole number from 0"))
                    })?;
                iterations.insert(dat.to_owned(), n);
            }
            let mut packs = Vec::new();
            for p in &file.packs {
                if !symbol::is_name(p) {
                    return Err(Error::source(
                        &whole,
                        format!("packs: {p:?} is not a pack folder (lowercase letters, digits and hyphens)"),
                    ));
                }
                packs.push(pdir.join(PACKS).join(p));
            }
            let overrides = read_overrides::<D>(&path, &text, &file.overrides)?;
            let checkpoint = read_pin(store, &pdir.join("checkpoint.toml"))?;
            let drift_path = pdir.join("drift.toml");
            let drift = if store.is_file(&drift_path) {
                if checkpoint.is_none() {
                    return Err(Error::source(
                        &Loc::new(&drift_path, 0),
                        "drift is acknowledged only at a checkpoint: this patch has no checkpoint.toml",
                    ));
                }
                read_drift::<D>(store, &drift_path)?
            } else {
                Vec::new()
            };
            layers.push(LayerDef {
                id: id.clone(),
                dir: pdir,
                patch: Some(Patch {
                    id: file.id,
                    title: file.title,
                    date: file.date,
                    after: file.after,
                    iterations,
                }),
                packs,
                overrides,
                checkpoint,
                drift,
            });
        }
        Ok(Self { layers })
    }

    /// The layer `id`'s place in the lineage: 0 for the base.
    #[must_use]
    pub fn position(&self, id: &str) -> Option<usize> {
        self.layers.iter().position(|l| l.id == id)
    }
}

/// The pin at `path`, when there is one.
fn read_pin(store: &dyn SourceStore, path: &Path) -> Result<Option<Pin>, Error> {
    if !store.is_file(path) {
        return Ok(None);
    }
    let text = store.read_to_string(path).map_err(|e| Error::io(path, e))?;
    let pin: PinFile = toml::from_str(&text).map_err(|e| Error::toml(path, &text, &e))?;
    for (key, value) in [("source", &pin.source), ("revision", &pin.revision)] {
        if value.trim().is_empty() {
            return Err(Error::source(
                &Loc::new(path, 0),
                format!("{key}: a checkpoint names its snapshot's {key}"),
            ));
        }
    }
    Ok(Some(Pin {
        source: pin.source,
        revision: pin.revision,
        url: pin.url,
        sha256: pin.sha256,
        converter: pin.converter,
        rest: pin.rest,
    }))
}

/// The acknowledgements of the `drift.toml` at `path`.
fn read_drift<D: Dialect>(store: &dyn SourceStore, path: &Path) -> Result<Vec<Ack>, Error> {
    let text = store.read_to_string(path).map_err(|e| Error::io(path, e))?;
    let file: DriftFile = toml::from_str(&text).map_err(|e| Error::toml(path, &text, &e))?;
    let mut out: Vec<Ack> = Vec::new();
    for row in file.drift {
        let at = Loc::at(path, &text, row.span().start);
        let r = row.into_inner();
        let item = match Scope::parse::<D>(&r.item).map_err(|why| Error::source(&at, why))? {
            Scope::Item(name) => name,
            Scope::Landblock(_) => {
                return Err(Error::source(
                    &at,
                    "a drift names one item, not a landblock",
                ));
            }
        };
        if let Some(prev) = out.iter().find(|a| a.item == item && a.field == r.field) {
            return Err(Error::source(
                &at,
                format!("this drift is acknowledged already ({})", prev.at),
            ));
        }
        out.push(Ack {
            item,
            field: r.field,
            take: r.take,
            note: r.note,
            at,
        });
    }
    Ok(out)
}
