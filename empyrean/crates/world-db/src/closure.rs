//! Closure: every record a world names is in the data files of its layer.
//!
//! A layer serves the data files of the iterations its `patch.toml` names (`portal_iteration`,
//! `cell_iteration`, and `highres_iteration` for the high-resolution portal); the base serves those
//! its world file's `[land]` names the same way. The files are given to the check as folders
//! ([`Dats::scan`]), each file known by what its header says it is and its iteration, so a folder
//! of captures from many months serves every layer whose iterations it holds.
//!
//! What a world names, and where it is:
//!
//! | Named by | Record | File |
//! |---|---|---|
//! | a weenie's data ids that name records (`did.Setup`, `did.Icon`, `did.ClothingBase`, ...) | the id | portal (or the high-resolution portal) |
//! | a weenie's `palette`, `texture_map` and `anim_part` rows | their palette, both textures, the part's object | portal |
//! | a placement's cell, a weenie's positions, a generator row's or an emote action's cell | the cell's landblock (`LLLLFFFF`), and for a cell inside (`LLLL0100` and up) the cell | cell |
//! | an encounter | its landblock | cell |
//!
//! Only the records the world names are looked up; what those records name in turn is not
//! followed. A record a layer's files lack is one line, the first thing that names it and how
//! many more do. A layer whose files are unknown (it names no iteration of a file, or no folder
//! given holds that iteration) is not checked, and says so in one line.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use dereth_dat::container::{CELL_DATFILE, LOCAL_DATFILE, PORTAL_DATFILE};
use dereth_dat::DatFile;
use empyrean_content::WorldContent;
use empyrean_entity::enums::PropertyDataId;

use crate::names::{self, Kind};
use crate::value::R;

/// The data file a record a world names is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum File {
    Portal,
    Cell,
}

impl File {
    /// Its name, as a layer's `<name>_iteration` says it.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Portal => "portal",
            Self::Cell => "cell",
        }
    }
}

impl fmt::Display for File {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// The high-resolution portal's name: a portal file whose header names a subset.
pub const HIGHRES: &str = "highres";

/// The data ids of a weenie whose values are records of the portal file a player's client reads:
/// what it draws, animates and plays. The rest hold motions, sounds, scripts, classes, spells,
/// treasure types, houses, or the filters only a server reads.
pub const RECORD_DIDS: &[PropertyDataId] = &[
    PropertyDataId::Setup,
    PropertyDataId::MotionTable,
    PropertyDataId::SoundTable,
    PropertyDataId::CombatTable,
    PropertyDataId::PaletteBase,
    PropertyDataId::ClothingBase,
    PropertyDataId::Icon,
    PropertyDataId::EyesTexture,
    PropertyDataId::NoseTexture,
    PropertyDataId::MouthTexture,
    PropertyDataId::DefaultEyesTexture,
    PropertyDataId::DefaultNoseTexture,
    PropertyDataId::DefaultMouthTexture,
    PropertyDataId::HairPalette,
    PropertyDataId::EyesPalette,
    PropertyDataId::SkinPalette,
    PropertyDataId::HeadObject,
    PropertyDataId::PhysicsEffectTable,
    PropertyDataId::IconOverlay,
    PropertyDataId::IconOverlaySecondary,
    PropertyDataId::IconUnderlay,
];

/// Who names a record: the first thing that does, and how many more.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Namer {
    pub first: String,
    pub more: u32,
}

/// Every record a world names, each with what names it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Named {
    pub records: BTreeMap<(File, u32), Namer>,
}

impl Named {
    fn add(&mut self, file: File, id: u32, by: impl FnOnce() -> String) {
        // A record's type with no index (`0x04000000`) names none.
        if id & 0x00FF_FFFF == 0 {
            return;
        }
        self.records
            .entry((file, id))
            .and_modify(|n| n.more += 1)
            .or_insert_with(|| Namer {
                first: by(),
                more: 0,
            });
    }

    /// The records a cell needs: its landblock, and the cell itself when it is inside.
    fn cell(&mut self, cell: u32, by: impl Fn() -> String) {
        if cell == 0 {
            return;
        }
        self.add(File::Cell, cell | 0xFFFF, &by);
        if (0x0100..0xFFFE).contains(&(cell & 0xFFFF)) {
            self.add(File::Cell, cell, by);
        }
    }

    /// The records `c` names.
    #[must_use]
    pub fn of(c: &WorldContent) -> Self {
        let mut n = Self::default();
        let did = names::of(Kind::Did);
        let position = names::of(Kind::Position);
        for w in &c.weenies {
            let me = w.class_id;
            for d in &w.weenie_properties_did {
                if RECORD_DIDS.iter().any(|r| r.0 == d.r#type) {
                    n.add(File::Portal, d.value, || {
                        format!("weenie {me} did.{}", did.name(i64::from(d.r#type)))
                    });
                }
            }
            for (i, p) in w.weenie_properties_palette.iter().enumerate() {
                n.add(File::Portal, p.sub_palette_id, || {
                    format!("weenie {me} palette[{i}]")
                });
            }
            for (i, t) in w.weenie_properties_texture_map.iter().enumerate() {
                for id in [t.old_id, t.new_id] {
                    n.add(File::Portal, id, || format!("weenie {me} texture_map[{i}]"));
                }
            }
            for (i, a) in w.weenie_properties_anim_part.iter().enumerate() {
                n.add(File::Portal, a.animation_id, || {
                    format!("weenie {me} anim_part[{i}]")
                });
            }
            for p in &w.weenie_properties_position {
                n.cell(p.obj_cell_id, || {
                    format!(
                        "weenie {me} position.{}",
                        position.name(i64::from(p.position_type))
                    )
                });
            }
            for (i, g) in w.weenie_properties_generator.iter().enumerate() {
                if let Some(cell) = g.obj_cell_id {
                    n.cell(cell, || format!("weenie {me} generator[{i}]"));
                }
            }
            for (i, e) in w.weenie_properties_emote.iter().enumerate() {
                for (j, a) in e.weenie_properties_emote_action.iter().enumerate() {
                    if let Some(cell) = a.obj_cell_id {
                        n.cell(cell, || format!("weenie {me} emote[{i}].action[{j}]"));
                    }
                }
            }
        }
        for i in &c.landblock_instances {
            n.cell(i.obj_cell_id, || format!("spawn {:#010X}", i.guid));
        }
        for e in &c.encounters {
            #[allow(clippy::cast_sign_loss)]
            let lb = (e.landblock as u32) & 0xFFFF;
            n.add(File::Cell, (lb << 16) | 0xFFFF, || {
                format!("encounter {lb:04X} {},{}", e.cell_x, e.cell_y)
            });
        }
        n
    }
}

/// Where a data file's records come from: a file on disk, or the ids themselves.
#[derive(Clone, Debug)]
enum Source {
    Path(PathBuf),
    Ids(Arc<Vec<u32>>),
}

/// The data files a check is given, each by its name (`portal`, `cell`, `highres`, `local`) and
/// iteration.
#[derive(Clone, Debug, Default)]
pub struct Dats {
    files: BTreeMap<(String, u32), Source>,
}

/// A file's name from what its header says: the data set, and for the portal its subset.
fn name_of(data_set: u32, subset: u32) -> Option<&'static str> {
    match (data_set, subset) {
        (PORTAL_DATFILE, 0) => Some(File::Portal.name()),
        (PORTAL_DATFILE, _) => Some(HIGHRES),
        (CELL_DATFILE, _) => Some(File::Cell.name()),
        (LOCAL_DATFILE, _) => Some("local"),
        _ => None,
    }
}

/// How deep below a folder given the scan looks for data files.
const DEPTH: usize = 4;

impl Dats {
    /// Every data file (`*.dat`) in `folders` and the folders below them, each known by its header
    /// and its iteration; the first of two of one name and iteration serves.
    ///
    /// # Errors
    /// A folder that doesn't read, and a `.dat` file that isn't a data file.
    pub fn scan(folders: &[PathBuf]) -> R<Self> {
        let mut d = Self::default();
        for f in folders {
            d.scan_dir(f, DEPTH)?;
        }
        Ok(d)
    }

    fn scan_dir(&mut self, dir: &Path, depth: usize) -> R<()> {
        let read = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let mut paths: Vec<PathBuf> = read.filter_map(|e| e.ok().map(|e| e.path())).collect();
        paths.sort();
        for p in paths {
            if p.is_dir() {
                if depth > 0 {
                    self.scan_dir(&p, depth - 1)?;
                }
                continue;
            }
            let is_dat = p.extension().is_some_and(|x| x.eq_ignore_ascii_case("dat"));
            if !is_dat {
                continue;
            }
            let it = DatFile::read_iterations(&p).map_err(|e| format!("{}: {e}", p.display()))?;
            if let Some(name) = name_of(it.data_set, it.data_subset) {
                self.files
                    .entry((name.to_owned(), it.highest))
                    .or_insert(Source::Path(p));
            }
        }
        Ok(())
    }

    /// These, and the file `name` at `iteration` holding the records `ids`.
    #[must_use]
    pub fn with(mut self, name: &str, iteration: u32, ids: impl IntoIterator<Item = u32>) -> Self {
        let mut ids: Vec<u32> = ids.into_iter().collect();
        ids.sort_unstable();
        ids.dedup();
        self.files
            .insert((name.to_owned(), iteration), Source::Ids(Arc::new(ids)));
        self
    }

    /// Whether no data file is given.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// The files given, as one line: each name and its iterations.
    #[must_use]
    pub fn summary(&self) -> String {
        let mut by: BTreeMap<&str, Vec<String>> = BTreeMap::new();
        for (name, n) in self.files.keys() {
            by.entry(name).or_default().push(n.to_string());
        }
        let parts: Vec<String> = by
            .iter()
            .map(|(name, ns)| format!("{name} {}", ns.join(", ")))
            .collect();
        format!("data files: {}", parts.join("; "))
    }

    fn has(&self, name: &str, iteration: u32) -> bool {
        self.files.contains_key(&(name.to_owned(), iteration))
    }

    /// The records of the file `name` at `iteration`, ascending, read from its file the first time
    /// `held` is asked for them; `held` keeps only the files of the last layer asked for.
    fn ids(&self, name: &str, iteration: u32, held: &mut Held) -> R<Option<Arc<Vec<u32>>>> {
        let key = (name.to_owned(), iteration);
        if let Some(ids) = held.0.get(&key) {
            return Ok(Some(ids.clone()));
        }
        let Some(source) = self.files.get(&key) else {
            return Ok(None);
        };
        let ids = match source {
            Source::Ids(ids) => ids.clone(),
            Source::Path(p) => {
                let f = DatFile::open(p).map_err(|e| format!("{}: {e}", p.display()))?;
                Arc::new(f.iter_ids().map(|i| i.raw()).collect())
            }
        };
        held.0.insert(key, ids.clone());
        Ok(Some(ids))
    }
}

/// The data files' records read so far: those of the last layer that asked.
#[derive(Debug, Default)]
pub struct Held(BTreeMap<(String, u32), Arc<Vec<u32>>>);

/// What closure finds in one layer: the records its files lack, each as a line; and the files it
/// couldn't check, each said once.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Closure {
    pub missing: BTreeMap<(File, u32), String>,
    /// The files checked.
    pub checked: BTreeSet<File>,
    pub skipped: Vec<String>,
}

/// `named` looked up in the files `iterations` names (`portal`, `highres` and `cell`), among
/// `dats`.
///
/// # Errors
/// A data file that doesn't read.
pub fn check(
    named: &Named,
    iterations: &BTreeMap<String, u32>,
    dats: &Dats,
    held: &mut Held,
) -> R<Closure> {
    let mut out = Closure::default();
    let mut keep = BTreeMap::new();
    let wanted = |file: File| named.records.keys().any(|(f, _)| *f == file);
    if iterations.is_empty() {
        out.skipped
            .push("closure not checked: the layer names none of its data files' iterations".into());
        return Ok(out);
    }
    for file in [File::Portal, File::Cell] {
        let Some(&n) = iterations.get(file.name()) else {
            if wanted(file) {
                out.skipped.push(format!(
                    "closure of {file} records not checked: the layer names no {file} iteration"
                ));
            }
            continue;
        };
        let Some(ids) = dats.ids(file.name(), n, held)? else {
            out.skipped.push(format!(
                "closure of {file} records not checked: no {file} file of iteration {n} among the data files given"
            ));
            continue;
        };
        keep.insert((file.name().to_owned(), n), ids.clone());
        // The high-resolution portal's records are the portal's too, where the layer names it.
        let extra = match (file, iterations.get(HIGHRES)) {
            (File::Portal, Some(&h)) if dats.has(HIGHRES, h) => {
                let ids = dats.ids(HIGHRES, h, held)?;
                if let Some(ids) = &ids {
                    keep.insert((HIGHRES.to_owned(), h), ids.clone());
                }
                ids
            }
            _ => None,
        };
        out.checked.insert(file);
        let has = |id: &u32| {
            ids.binary_search(id).is_ok()
                || extra.as_ref().is_some_and(|e| e.binary_search(id).is_ok())
        };
        for ((f, id), by) in named.records.range((file, 0)..=(file, u32::MAX)) {
            debug_assert_eq!(*f, file);
            if !has(id) {
                let more = match by.more {
                    0 => String::new(),
                    m => format!(" and {m} more"),
                };
                out.missing.insert(
                    (file, *id),
                    format!(
                        "{file} {id:#010X}: in no {file} file of iteration {n} (named by {}{more})",
                        by.first
                    ),
                );
            }
        }
    }
    held.0 = keep;
    Ok(out)
}

/// What one layer's closure says: a record it lacks is the layer's error where the layer's rows
/// name it newly or the files below it held it; the world below's, a warning, where the world
/// below named it and no files below were checked for it; a snapshot's own, a warning, at the base
/// or a checkpoint. A record lacked below too was said there.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Said {
    pub errors: Vec<String>,
    pub below: Vec<String>,
    pub inherited: Vec<String>,
    pub reports: Vec<String>,
}

/// What closure knew of the layers below: the records the world named there, those its files
/// held, and those they lacked, each file as last checked.
#[derive(Debug, Default)]
pub struct Below {
    named: BTreeSet<(File, u32)>,
    held: BTreeSet<(File, u32)>,
    lacked: BTreeSet<(File, u32)>,
}

impl Below {
    /// `found`, the closure of a layer whose world names `named`, said as [`Said`] says; and the
    /// layer is below the next.
    pub fn say(&mut self, named: &Named, found: Closure, snapshot: bool) -> Said {
        let mut out = Said {
            reports: found.skipped,
            ..Said::default()
        };
        for (k, line) in &found.missing {
            if self.lacked.contains(k) {
                continue;
            }
            if snapshot {
                out.inherited.push(line.clone());
            } else if !self.named.contains(k) || self.held.contains(k) {
                out.errors.push(line.clone());
            } else {
                out.below.push(format!(
                    "{line}; named below, where no files were checked for it"
                ));
            }
        }
        for f in &found.checked {
            self.lacked.retain(|(g, _)| g != f);
            self.held.retain(|(g, _)| g != f);
            for k in named.records.keys().filter(|(g, _)| g == f) {
                if found.missing.contains_key(k) {
                    self.lacked.insert(*k);
                } else {
                    self.held.insert(*k);
                }
            }
        }
        self.named = named.records.keys().copied().collect();
        out
    }
}
