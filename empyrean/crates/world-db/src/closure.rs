//! Closure: every record a world names is in the data files of its layer; and the rooms it names
//! that change under it.
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
//! | a placement's cell, a weenie's positions, a generator row's or an emote action's cell | the cell's landblock (`LLLLFFFF`), and for a room (`LLLL0100` and up) the room | cell |
//! | an encounter | its landblock | cell |
//!
//! Only the records the world names are looked up; what those records name in turn is not
//! followed. A record a layer's files lack is one line, the first thing that names it and how
//! many more do. A layer whose files are unknown (it names no iteration of a file, or no folder
//! given holds that iteration) is not checked, and says so in one line.
//!
//! **Rooms.** A room is in a cell file when its landblock's info (`LLLLFFFE`) claims it: its index
//! (`LLLL0100` is the first) is below the room count the info gives. A room record no info claims
//! is a leftover, not a room.
//!
//! **Whole and partial cell files.** A cell file a player's client saved holds only what that
//! player visited. A whole one holds the land of every landblock, 255 by 255; one that doesn't is
//! partial. What a partial file lacks says nothing, but each record it holds is whole: a land it
//! holds is there, and a landblock info it holds says which of the landblock's rooms are there and
//! which are gone, whether or not it holds the rooms. So a partial file dates what its records
//! show, and a record it can't speak for keeps what the files below said of it.
//!
//! **Rooms that change under the world.** A room the world names can stay in the files and still
//! change: its landblock's info and the room record give it an environment, which of the
//! environment's cells it is, and where it stands; the environment (in the portal file) gives the
//! cell's walls. Where a layer's files (its own, or the latest below it for a file it doesn't name)
//! give a named room another shape or place than the files below did, every point the world names
//! in it is tested against the room's walls now: a point is in the room when no wall of the room's
//! cell has it more than [`SLACK`] behind it, the same test as the game's own for which room a
//! point is in, with a little slack for a point set on a floor or a wall. The destination of a
//! placed portal that is outside its room now is the layer's error; a portal's destination that
//! nothing places, a warning; and every changed room is reported, by landblock, with how many
//! points are in it and how many of them are outside it now.
//!
//! A portal is placed when a placement or a generator makes its class, or an emote summons it: an
//! emote action that casts a spell summoning a portal (a spell with a `link`) makes the portal the
//! caster's link names (`LinkedPortalOne` for link 1, `LinkedPortalTwo` for 2). A record named only
//! by the destinations of portals nothing places is a warning where the files lack it, not an
//! error.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use dereth_assets::{Decode, EnvCell, Environment, Plane};
use dereth_dat::container::{CELL_DATFILE, LOCAL_DATFILE, PORTAL_DATFILE};
use dereth_dat::DatFile;
use dereth_primitives::{DataId, Frame, Quat, Vec3};
use empyrean_content::WorldContent;
use empyrean_entity::enums::{EmoteType, PositionType, PropertyDataId, WeenieType};

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

/// How many landblocks a whole cell file holds the land of: 255 by 255.
pub const LANDBLOCKS: usize = 255 * 255;

/// How far behind a room's wall a point may be and still be in the room, in meters.
pub const SLACK: f32 = 0.25;

/// How far a room may move, in meters, and still be where it was.
const MOVED: f32 = 0.01;

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

/// Who names a record: the first thing that does, and how many more; and whether all of them are
/// destinations of portals nothing places.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Namer {
    pub first: String,
    pub more: u32,
    pub only_unplaced: bool,
}

/// What a point the world names in a room is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum What {
    /// Where a portal sends a player; `placed` when something places the portal.
    Destination { placed: bool },
    /// A placement, or any other position.
    Other,
}

/// What names a point: a placement by its guid, or a weenie's position, generator row or emote
/// action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum By {
    Spawn(u32),
    Position { class: u32, kind: u16 },
    Generator { class: u32, row: u32 },
    Emote { class: u32, emote: u32, action: u32 },
}

impl fmt::Display for By {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Spawn(guid) => write!(f, "spawn {guid:#010X}"),
            Self::Position { class, kind } => write!(
                f,
                "weenie {class} position.{}",
                names::of(Kind::Position).name(i64::from(kind))
            ),
            Self::Generator { class, row } => write!(f, "weenie {class} generator[{row}]"),
            Self::Emote {
                class,
                emote,
                action,
            } => write!(f, "weenie {class} emote[{emote}].action[{action}]"),
        }
    }
}

/// A point the world names in a room, in its landblock's frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub at: Vec3,
    pub what: What,
    pub by: By,
}

/// Every record a world names, each with what names it; and the points it names in rooms, by room.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Named {
    pub records: BTreeMap<(File, u32), Namer>,
    pub points: BTreeMap<u32, Vec<Point>>,
}

/// The portal classes emotes summon: a weenie whose emote action casts a spell with a `link` (a
/// spell summoning a portal) makes the portal its link names, `LinkedPortalOne` for link 1 (or
/// less) and `LinkedPortalTwo` for 2, as the cast does.
fn summoned(c: &WorldContent) -> BTreeSet<u32> {
    let links: BTreeMap<u32, i32> = c
        .spells
        .iter()
        .filter_map(|s| s.link.map(|l| (s.id, l)))
        .collect();
    let casts = [EmoteType::CastSpell.0, EmoteType::CastSpellInstant.0];
    let mut out = BTreeSet::new();
    for w in &c.weenies {
        for a in w
            .weenie_properties_emote
            .iter()
            .flat_map(|e| &e.weenie_properties_emote_action)
        {
            let cast = i32::try_from(a.r#type).is_ok_and(|t| casts.contains(&t));
            let link = a
                .spell_id
                .and_then(|s| u32::try_from(s).ok())
                .and_then(|s| links.get(&s));
            let (true, Some(&link)) = (cast, link) else {
                continue;
            };
            let did = if link <= 1 {
                PropertyDataId::LinkedPortalOne
            } else {
                PropertyDataId::LinkedPortalTwo
            };
            out.extend(
                w.weenie_properties_did
                    .iter()
                    .filter(|d| d.r#type == did.0)
                    .map(|d| d.value),
            );
        }
    }
    out
}

/// Whether `cell` is a room: `LLLL0100` and up, below the landblock's own records.
fn is_room(cell: u32) -> bool {
    (0x0100..0xFFFE).contains(&(cell & 0xFFFF))
}

impl Named {
    fn add(&mut self, file: File, id: u32, unplaced: bool, by: impl FnOnce() -> String) {
        // A record's type with no index (`0x04000000`) names none.
        if id & 0x00FF_FFFF == 0 {
            return;
        }
        self.records
            .entry((file, id))
            .and_modify(|n| {
                n.more += 1;
                n.only_unplaced &= unplaced;
            })
            .or_insert_with(|| Namer {
                first: by(),
                more: 0,
                only_unplaced: unplaced,
            });
    }

    /// The records a cell needs: its landblock, and the cell itself when it is a room; and the
    /// point in it, when there is one.
    fn cell(&mut self, cell: u32, at: Option<Vec3>, what: What, by: By) {
        if cell == 0 {
            return;
        }
        let unplaced = what == What::Destination { placed: false };
        self.add(File::Cell, cell | 0xFFFF, unplaced, || by.to_string());
        if is_room(cell) {
            self.add(File::Cell, cell, unplaced, || by.to_string());
            if let Some(at) = at {
                self.points
                    .entry(cell)
                    .or_default()
                    .push(Point { at, what, by });
            }
        }
    }

    /// The records `c` names.
    #[must_use]
    pub fn of(c: &WorldContent) -> Self {
        let mut n = Self::default();
        let did = names::of(Kind::Did);
        // The classes something places: a placement, a generator's row, or an emote's summoning.
        let placed: BTreeSet<u32> = c
            .landblock_instances
            .iter()
            .map(|i| i.weenie_class_id)
            .chain(c.weenies.iter().flat_map(|w| {
                w.weenie_properties_generator
                    .iter()
                    .map(|g| g.weenie_class_id)
            }))
            .chain(summoned(c))
            .collect();
        for w in &c.weenies {
            let me = w.class_id;
            let portal = u32::try_from(w.r#type).is_ok_and(|t| t == WeenieType::Portal.0);
            for d in &w.weenie_properties_did {
                if RECORD_DIDS.iter().any(|r| r.0 == d.r#type) {
                    n.add(File::Portal, d.value, false, || {
                        format!("weenie {me} did.{}", did.name(i64::from(d.r#type)))
                    });
                }
            }
            for (i, p) in w.weenie_properties_palette.iter().enumerate() {
                n.add(File::Portal, p.sub_palette_id, false, || {
                    format!("weenie {me} palette[{i}]")
                });
            }
            for (i, t) in w.weenie_properties_texture_map.iter().enumerate() {
                for id in [t.old_id, t.new_id] {
                    n.add(File::Portal, id, false, || {
                        format!("weenie {me} texture_map[{i}]")
                    });
                }
            }
            for (i, a) in w.weenie_properties_anim_part.iter().enumerate() {
                n.add(File::Portal, a.animation_id, false, || {
                    format!("weenie {me} anim_part[{i}]")
                });
            }
            for p in &w.weenie_properties_position {
                let what = if portal && p.position_type == PositionType::Destination.0 {
                    What::Destination {
                        placed: placed.contains(&me),
                    }
                } else {
                    What::Other
                };
                n.cell(
                    p.obj_cell_id,
                    Some(Vec3::new(p.origin_x, p.origin_y, p.origin_z)),
                    what,
                    By::Position {
                        class: me,
                        kind: p.position_type,
                    },
                );
            }
            for (i, g) in w.weenie_properties_generator.iter().enumerate() {
                if let Some(cell) = g.obj_cell_id {
                    n.cell(
                        cell,
                        point(g.origin_x, g.origin_y, g.origin_z),
                        What::Other,
                        By::Generator {
                            class: me,
                            row: index(i),
                        },
                    );
                }
            }
            for (i, e) in w.weenie_properties_emote.iter().enumerate() {
                for (j, a) in e.weenie_properties_emote_action.iter().enumerate() {
                    if let Some(cell) = a.obj_cell_id {
                        n.cell(
                            cell,
                            point(a.origin_x, a.origin_y, a.origin_z),
                            What::Other,
                            By::Emote {
                                class: me,
                                emote: index(i),
                                action: index(j),
                            },
                        );
                    }
                }
            }
        }
        for i in &c.landblock_instances {
            n.cell(
                i.obj_cell_id,
                Some(Vec3::new(i.origin_x, i.origin_y, i.origin_z)),
                What::Other,
                By::Spawn(i.guid),
            );
        }
        for e in &c.encounters {
            #[allow(clippy::cast_sign_loss)]
            let lb = (e.landblock as u32) & 0xFFFF;
            n.add(File::Cell, (lb << 16) | 0xFFFF, false, || {
                format!("encounter {lb:04X} {},{}", e.cell_x, e.cell_y)
            });
        }
        n
    }
}

fn point(x: Option<f32>, y: Option<f32>, z: Option<f32>) -> Option<Vec3> {
    Some(Vec3::new(x?, y?, z?))
}

fn index(i: usize) -> u32 {
    u32::try_from(i).unwrap_or(u32::MAX)
}

/// The data files a check is given, each by its name (`portal`, `cell`, `highres`, `local`) and
/// iteration.
#[derive(Clone, Debug, Default)]
pub struct Dats {
    files: BTreeMap<(String, u32), PathBuf>,
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
    /// and its iteration. Of two of one name and iteration, the one holding more records serves (a
    /// player's partial cell file and a whole one can be of one iteration), and of two holding as
    /// many, the first.
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
            let Some(name) = name_of(it.data_set, it.data_subset) else {
                continue;
            };
            let key = (name.to_owned(), it.highest);
            match self.files.get(&key) {
                None => {
                    self.files.insert(key, p);
                }
                Some(first) => {
                    let count = |p: &Path| {
                        DatFile::open(p)
                            .map(|f| f.len())
                            .map_err(|e| format!("{}: {e}", p.display()))
                    };
                    if count(&p)? > count(first)? {
                        self.files.insert(key, p);
                    }
                }
            }
        }
        Ok(())
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
}

/// A data file opened: its name and iteration, its records, and (a cell file) whether it is whole.
#[derive(Debug)]
struct Opened {
    name: String,
    iteration: u32,
    dat: DatFile,
    whole: bool,
}

impl Opened {
    fn open(name: &str, iteration: u32, path: &Path) -> R<Self> {
        let dat = DatFile::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let whole = name != File::Cell.name()
            || dat
                .iter_ids()
                .filter(|i| i.raw() & 0xFFFF == 0xFFFF)
                .count()
                >= LANDBLOCKS;
        Ok(Self {
            name: name.to_owned(),
            iteration,
            dat,
            whole,
        })
    }

    fn holds(&self, id: u32) -> bool {
        self.dat.contains(DataId(id))
    }

    fn read(&self, id: u32) -> Option<Vec<u8>> {
        self.dat.read(DataId(id)).ok()
    }

    /// `name iteration`, and `(partial)` for a partial cell file.
    fn label(&self) -> String {
        if self.whole {
            format!("{} {}", self.name, self.iteration)
        } else {
            format!("{} {} (partial)", self.name, self.iteration)
        }
    }
}

/// What a file says of a record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Is {
    Held,
    /// Gone: a whole file lacks it, or its landblock's info doesn't claim the room.
    Lacked(Why),
    /// A partial file can't say.
    Unknown,
}

/// Why a cell file lacks a record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Why {
    NotHeld,
    /// The room's landblock info claims fewer rooms.
    Unclaimed(u32),
    /// The room's landblock has no info.
    NoInfo,
}

/// The rooms each landblock's info claims in one cell file, read as asked for.
#[derive(Default)]
struct Claims(BTreeMap<u32, Option<u32>>);

impl Claims {
    /// The room count the info of `room`'s landblock gives in `f`: the second word of the record;
    /// `None` where `f` holds no info.
    fn of(&mut self, f: &Opened, room: u32) -> Option<u32> {
        let info = (room & 0xFFFF_0000) | 0xFFFE;
        *self.0.entry(info).or_insert_with(|| {
            let b = f.read(info)?;
            Some(u32::from_le_bytes(b.get(4..8)?.try_into().ok()?))
        })
    }

    /// What the cell file `f` says of the cell record `id`.
    fn is(&mut self, f: &Opened, id: u32) -> Is {
        if !is_room(id) {
            return match (f.holds(id), f.whole) {
                (true, _) => Is::Held,
                (false, true) => Is::Lacked(Why::NotHeld),
                (false, false) => Is::Unknown,
            };
        }
        match (self.of(f, id), f.whole) {
            (Some(n), _) if (id & 0xFFFF) - 0x0100 < n => Is::Held,
            (Some(n), _) => Is::Lacked(Why::Unclaimed(n)),
            (None, true) => Is::Lacked(Why::NoInfo),
            (None, false) => Is::Unknown,
        }
    }
}

/// What closure finds in one layer: the records its files lack, each as a line; those a partial
/// file can't say of; the files it checked; and the files it couldn't check, each said once.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Closure {
    pub missing: BTreeMap<(File, u32), String>,
    pub unknown: BTreeSet<(File, u32)>,
    pub checked: BTreeSet<File>,
    pub skipped: Vec<String>,
}

/// What one layer's data files say: errors, warnings, a snapshot's own problems, and reports.
///
/// Closure: a record a layer's files lack is the layer's error where the layer's rows name it
/// newly or the files below it held it; the world below's, a warning, where the world below named
/// it and no files below were checked for it; a warning where only the destinations of portals
/// nothing places name it; a snapshot's own, at the base or a checkpoint. A record lacked below
/// too was said there.
///
/// Rooms: a placed portal's destination outside its changed room is the layer's error (a
/// snapshot's own at the base or a checkpoint); the destination of a portal nothing places, a
/// warning; each landblock whose named rooms changed, a report.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Said {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub inherited: Vec<String>,
    pub reports: Vec<String>,
}

/// What closure knew of the layers below: the records the world named there, and of those, what
/// the files last said of each (held, or not).
#[derive(Debug, Default)]
struct Below {
    named: BTreeSet<(File, u32)>,
    known: BTreeMap<(File, u32), bool>,
}

impl Below {
    /// `found`, the closure of a layer whose world names `named`, said as [`Said`] says; and the
    /// layer is below the next.
    fn say(&mut self, named: &Named, found: &Closure, snapshot: bool, out: &mut Said) {
        for (k, line) in &found.missing {
            let known = self.known.get(k).copied();
            if known == Some(false) {
                continue;
            }
            if snapshot {
                out.inherited.push(line.clone());
            } else if named.records.get(k).is_some_and(|n| n.only_unplaced) {
                out.warnings.push(format!(
                    "{line}; named only by the destination of a portal nothing places"
                ));
            } else if !self.named.contains(k) || known == Some(true) {
                out.errors.push(line.clone());
            } else {
                out.warnings.push(format!(
                    "{line}; named below, where no files were checked for it"
                ));
            }
        }
        for f in &found.checked {
            let before = std::mem::take(&mut self.known);
            let (this, other): (BTreeMap<_, _>, BTreeMap<_, _>) =
                before.into_iter().partition(|((g, _), _)| g == f);
            self.known = other;
            for k in named.records.keys().filter(|(g, _)| g == f) {
                let now = if found.missing.contains_key(k) {
                    Some(false)
                } else if found.unknown.contains(k) {
                    this.get(k).copied()
                } else {
                    Some(true)
                };
                if let Some(held) = now {
                    self.known.insert(*k, held);
                }
            }
        }
        self.named = named.records.keys().copied().collect();
    }
}

/// One cell of an environment: the planes of its walls, each facing in, and a digest of its shape.
#[derive(Debug, PartialEq)]
struct Walls {
    planes: Vec<Plane>,
    digest: [u8; 32],
}

impl Walls {
    /// The cell `index` of `env`: its cell tree's planes, from the root along each node's front
    /// child; its digest, of those and its vertices to the millimeter.
    fn of(env: &Environment, index: u16) -> Option<Self> {
        let cell = env
            .cells
            .iter()
            .find(|c| c.cellstruct_id == u32::from(index))?;
        let mut planes = Vec::new();
        let mut at = (!cell.cell_bsp.nodes.is_empty()).then_some(0usize);
        let mut steps = 0;
        while let Some(i) = at {
            let node = cell.cell_bsp.nodes.get(i)?;
            if let Some(p) = node.plane {
                planes.push(p);
            }
            at = node.pos_child.map(|c| c as usize);
            steps += 1;
            if steps > cell.cell_bsp.nodes.len() {
                return None;
            }
        }
        let mut h = blake3::Hasher::new();
        #[allow(clippy::cast_possible_truncation)]
        let mm = |v: f32| ((v * 1000.0).round() as i32).to_le_bytes();
        for p in &planes {
            for v in [p.normal.x, p.normal.y, p.normal.z, p.d] {
                h.update(&mm(v));
            }
        }
        for v in &cell.vertex_array.vertices {
            for x in [v.position.x, v.position.y, v.position.z] {
                h.update(&mm(x));
            }
        }
        Some(Self {
            planes,
            digest: *h.finalize().as_bytes(),
        })
    }

    /// Whether `local`, a point in the cell's own frame, is in it: behind no wall by more than
    /// [`SLACK`].
    fn contains(&self, local: Vec3) -> bool {
        self.planes
            .iter()
            .all(|p| p.normal.dot(local) + p.d >= -SLACK)
    }
}

/// `q` scaled to a unit rotation; the identity for none.
fn unit(q: Quat) -> Quat {
    let n = (q.w * q.w + q.x * q.x + q.y * q.y + q.z * q.z).sqrt();
    if n > 0.0 {
        Quat::new(q.w / n, q.x / n, q.y / n, q.z / n)
    } else {
        Quat::IDENTITY
    }
}

/// `p`, a point in a landblock's frame, in the frame `f` places a room at.
fn local(f: &Frame, p: Vec3) -> Vec3 {
    let v = Vec3::new(p.x - f.origin.x, p.y - f.origin.y, p.z - f.origin.z);
    // Rotated by the rotation's inverse: the conjugate of the unit rotation.
    let q = unit(f.rotation);
    let (qx, qy, qz, w) = (-q.x, -q.y, -q.z, q.w);
    let t = Vec3::new(
        2.0 * (qy * v.z - qz * v.y),
        2.0 * (qz * v.x - qx * v.z),
        2.0 * (qx * v.y - qy * v.x),
    );
    Vec3::new(
        v.x + w * t.x + (qy * t.z - qz * t.y),
        v.y + w * t.y + (qz * t.x - qx * t.z),
        v.z + w * t.z + (qx * t.y - qy * t.x),
    )
}

/// A room as a pair of files give it: its environment, which of its cells, where it stands, and
/// that cell's walls (none where the portal file isn't known or doesn't give them).
#[derive(Debug, Clone)]
struct Shape {
    env: u32,
    cell: u16,
    frame: Frame,
    walls: Option<Arc<Walls>>,
}

impl Shape {
    /// How `self` changed from `before`; `None` where it didn't.
    fn change_from(&self, before: &Self) -> Option<Change> {
        let reshaped = if (self.env, self.cell) != (before.env, before.cell) {
            Some(format!(
                "its shape changed, environment {:#010X} cell {} to {:#010X} cell {}",
                before.env, before.cell, self.env, self.cell
            ))
        } else {
            match (&before.walls, &self.walls) {
                (Some(a), Some(b)) if a.digest != b.digest => {
                    Some(format!("its environment {:#010X} changed", self.env))
                }
                _ => None,
            }
        };
        let (a, b) = (before.frame.origin, self.frame.origin);
        let moved = ((a.x - b.x).powi(2) + (a.y - b.y).powi(2) + (a.z - b.z).powi(2)).sqrt();
        let (q, r) = (unit(before.frame.rotation), unit(self.frame.rotation));
        let dot = (q.w * r.w + q.x * r.x + q.y * r.y + q.z * r.z).abs();
        let c = Change {
            reshaped,
            moved: (moved > MOVED).then_some(moved),
            turned: dot < 1.0 - 1e-6,
        };
        (c.reshaped.is_some() || c.moved.is_some() || c.turned).then_some(c)
    }

    /// Whether `p` is in the room; `None` where its walls aren't known.
    fn holds(&self, p: Vec3) -> Option<bool> {
        Some(self.walls.as_ref()?.contains(local(&self.frame, p)))
    }
}

/// How a room changed: its shape, how far it moved, whether it turned.
#[derive(Debug, Clone, PartialEq)]
struct Change {
    reshaped: Option<String>,
    moved: Option<f32>,
    turned: bool,
}

impl fmt::Display for Change {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut how: Vec<String> = self.reshaped.iter().cloned().collect();
        how.extend(self.moved.map(|m| format!("it moved {m:.2} m")));
        if self.turned {
            how.push("it turned".to_owned());
        }
        f.write_str(&how.join(", "))
    }
}

/// A cell file and the portal file in force with it, as a layer's rooms are read from.
#[derive(Debug, Clone)]
struct Pair {
    cell: Arc<Opened>,
    portal: Option<Arc<Opened>>,
}

impl Pair {
    fn label(&self) -> String {
        match &self.portal {
            Some(p) => format!("{} with {}", self.cell.label(), p.label()),
            None => self.cell.label(),
        }
    }

    fn same(&self, o: &Self) -> bool {
        Arc::ptr_eq(&self.cell, &o.cell)
            && match (&self.portal, &o.portal) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            }
    }
}

/// The rooms of one pair of files as asked for: each landblock's claims, and each environment's
/// cells.
struct Reader<'a> {
    pair: &'a Pair,
    claims: Claims,
    envs: BTreeMap<u32, Option<Arc<Environment>>>,
    walls: BTreeMap<(u32, u16), Option<Arc<Walls>>>,
}

impl<'a> Reader<'a> {
    fn new(pair: &'a Pair) -> Self {
        Self {
            pair,
            claims: Claims::default(),
            envs: BTreeMap::new(),
            walls: BTreeMap::new(),
        }
    }

    /// The room `id` as the pair gives it: `Ok(None)` where a partial file holds it but not its
    /// record, or a record that doesn't read; `Err` where it isn't there.
    fn shape(&mut self, id: u32) -> Result<Option<Shape>, ()> {
        let f = &self.pair.cell;
        match self.claims.is(f, id) {
            Is::Lacked(_) => return Err(()),
            Is::Unknown => return Ok(None),
            Is::Held => {}
        }
        let Some(bytes) = f.read(id) else {
            return Ok(None);
        };
        let Ok(c) = EnvCell::decode_payload_in(f.dat.era(), DataId(id), &bytes) else {
            return Ok(None);
        };
        let env = c.environment.raw();
        let walls = self.walls_of(env, c.cell_struct);
        Ok(Some(Shape {
            env,
            cell: c.cell_struct,
            frame: c.frame,
            walls,
        }))
    }

    fn walls_of(&mut self, env: u32, cell: u16) -> Option<Arc<Walls>> {
        if let Some(w) = self.walls.get(&(env, cell)) {
            return w.clone();
        }
        let portal = self.pair.portal.as_ref();
        let e = self
            .envs
            .entry(env)
            .or_insert_with(|| {
                let p = portal?;
                let bytes = p.read(env)?;
                Environment::decode_payload_in(p.dat.era(), DataId(env), &bytes)
                    .ok()
                    .map(Arc::new)
            })
            .clone();
        let w = e.and_then(|e| Walls::of(&e, cell)).map(Arc::new);
        self.walls.insert((env, cell), w.clone());
        w
    }
}

/// What the rooms check knew of the layers below: the shape the files last gave each room the
/// world names, and the files they were.
#[derive(Debug, Default)]
struct Rooms {
    shapes: BTreeMap<u32, Shape>,
    pair: Option<Pair>,
}

/// How many points a landblock's changed rooms hold, and how many of them are outside their room
/// now, or can't be tested.
#[derive(Default)]
struct Count {
    rooms: u32,
    reshaped: u32,
    moved: u32,
    turned: u32,
    points: u32,
    outside: u32,
    untested: u32,
}

impl Count {
    /// How the rooms changed, in words: how many changed shape, moved and turned.
    fn how(&self) -> String {
        let mut how = Vec::new();
        for (n, what) in [
            (self.reshaped, "changed shape"),
            (self.moved, "moved"),
            (self.turned, "turned"),
        ] {
            if n > 0 {
                how.push(format!("{n} {what}"));
            }
        }
        how.join(", ")
    }
}

impl Rooms {
    /// The rooms `named` names, read from `now` and held to the shapes the files below gave them.
    fn look(&mut self, named: &Named, now: &Pair, snapshot: bool, out: &mut Said) {
        let before = self.pair.replace(now.clone());
        let mut was = before.as_ref().map(Reader::new);
        let mut is = Reader::new(now);
        let mut by_block: BTreeMap<u32, Count> = BTreeMap::new();
        let mut shapes = BTreeMap::new();
        for (&room, points) in &named.points {
            let Ok(shape) = is.shape(room) else {
                // Gone: closure says so.
                continue;
            };
            let old = self
                .shapes
                .remove(&room)
                .or_else(|| was.as_mut().and_then(|r| r.shape(room).ok().flatten()));
            let Some(shape) = shape else {
                // A partial file can't say: the shape below stands.
                if let Some(old) = old {
                    shapes.insert(room, old);
                }
                continue;
            };
            if let (Some(old), Some(b)) = (&old, &before) {
                if let Some(how) = shape.change_from(old) {
                    let between = format!("between {} and {}", b.label(), now.label());
                    let c = by_block.entry(room >> 16).or_default();
                    c.rooms += 1;
                    c.reshaped += u32::from(how.reshaped.is_some());
                    c.moved += u32::from(how.moved.is_some());
                    c.turned += u32::from(how.turned);
                    for p in points {
                        c.points += 1;
                        let inside = shape.holds(p.at);
                        match inside {
                            None => c.untested += 1,
                            Some(false) => c.outside += 1,
                            Some(true) => {}
                        }
                        let What::Destination { placed } = p.what else {
                            continue;
                        };
                        if inside != Some(false) {
                            continue;
                        }
                        let then = match old.holds(p.at) {
                            Some(true) => "; it was in it before",
                            Some(false) => "; it was outside it before too",
                            None => "",
                        };
                        let line = format!(
                            "room {room:#010X} changed {between} ({how}): {}, {}, is outside it now{then}",
                            p.by,
                            if placed {
                                "a placed portal's destination"
                            } else {
                                "the destination of a portal nothing places"
                            }
                        );
                        if !placed {
                            out.warnings.push(line);
                        } else if snapshot {
                            out.inherited.push(line);
                        } else {
                            out.errors.push(line);
                        }
                    }
                }
            }
            shapes.insert(room, shape);
        }
        self.shapes = shapes;
        if let Some(b) = &before {
            for (block, c) in by_block {
                let mut line = format!(
                    "landblock {:04X}: {} of the rooms the world names changed between {} and {} ({}); {} points in them, {} of them outside their room now",
                    block,
                    c.rooms,
                    b.label(),
                    now.label(),
                    c.how(),
                    c.points,
                    c.outside
                );
                if c.untested > 0 {
                    line.push_str(&format!(
                        ", {} in rooms whose walls aren't known",
                        c.untested
                    ));
                }
                out.reports.push(line);
            }
        }
    }
}

/// Closure and the rooms check over a world's layers, bottom up: the data files given, those open
/// (the last layer's and the pair its rooms were read from), and what the layers below said.
#[derive(Debug, Default)]
pub struct Checker {
    dats: Dats,
    open: BTreeMap<(String, u32), Arc<Opened>>,
    below: Below,
    rooms: Rooms,
    portal: Option<Arc<Opened>>,
    cell: Option<Arc<Opened>>,
}

impl Checker {
    /// A check of the layers' records in `dats`.
    #[must_use]
    pub fn new(dats: Dats) -> Self {
        Self {
            dats,
            ..Self::default()
        }
    }

    /// The file `name` at `iteration`, opened; `None` where it isn't given.
    fn file(&mut self, name: &str, iteration: u32) -> R<Option<Arc<Opened>>> {
        let key = (name.to_owned(), iteration);
        if let Some(f) = self.open.get(&key) {
            return Ok(Some(f.clone()));
        }
        let Some(path) = self.dats.files.get(&key) else {
            return Ok(None);
        };
        let f = Arc::new(Opened::open(name, iteration, path)?);
        self.open.insert(key, f.clone());
        Ok(Some(f))
    }

    /// The layer whose world names `named` and whose files are `iterations` (`portal`, `highres`
    /// and `cell`): closure, said as [`Said`] says, and its rooms held to the files below's. A
    /// snapshot's problems are its own.
    ///
    /// # Errors
    /// A data file that doesn't read.
    pub fn layer(
        &mut self,
        named: &Named,
        iterations: &BTreeMap<String, u32>,
        snapshot: bool,
    ) -> R<Said> {
        let mut out = Said::default();
        let found = self.closure(named, iterations)?;
        out.reports.extend(found.skipped.iter().cloned());
        self.below.say(named, &found, snapshot, &mut out);
        // The rooms, where the layer's pair of files is another than the last looked at.
        if let Some(cell) = self.cell.clone() {
            let now = Pair {
                cell,
                portal: self.portal.clone(),
            };
            if !self.rooms.pair.as_ref().is_some_and(|p| p.same(&now)) {
                self.rooms.look(named, &now, snapshot, &mut out);
            }
        }
        // Only the files in force, and those the rooms were last read from, stay open.
        let mut keep: Vec<Arc<Opened>> = self.cell.iter().chain(&self.portal).cloned().collect();
        if let Some(p) = &self.rooms.pair {
            keep.push(p.cell.clone());
            keep.extend(p.portal.clone());
        }
        for (name, n) in iterations {
            if let Some(f) = self.open.get(&(name.clone(), *n)) {
                keep.push(f.clone());
            }
        }
        self.open
            .retain(|_, f| keep.iter().any(|k| Arc::ptr_eq(k, f)));
        Ok(out)
    }

    /// What the layer's files say of `named`.
    fn closure(&mut self, named: &Named, iterations: &BTreeMap<String, u32>) -> R<Closure> {
        let mut out = Closure::default();
        let wanted = |file: File| named.records.keys().any(|(f, _)| *f == file);
        if iterations.is_empty() {
            out.skipped.push(
                "closure not checked: the layer names none of its data files' iterations".into(),
            );
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
            let Some(f) = self.file(file.name(), n)? else {
                out.skipped.push(format!(
                    "closure of {file} records not checked: no {file} file of iteration {n} among the data files given"
                ));
                // The layer's own files aren't known: nothing below stands for them.
                match file {
                    File::Portal => self.portal = None,
                    File::Cell => self.cell = None,
                }
                continue;
            };
            // The high-resolution portal's records are the portal's too, where the layer names it.
            let extra = match (file, iterations.get(HIGHRES)) {
                (File::Portal, Some(&h)) if self.dats.has(HIGHRES, h) => self.file(HIGHRES, h)?,
                _ => None,
            };
            out.checked.insert(file);
            let mut claims = Claims::default();
            let mut unknown = 0u32;
            for ((g, id), by) in named.records.range((file, 0)..=(file, u32::MAX)) {
                debug_assert_eq!(*g, file);
                let is = match file {
                    File::Portal => {
                        if f.holds(*id) || extra.as_ref().is_some_and(|e| e.holds(*id)) {
                            Is::Held
                        } else {
                            Is::Lacked(Why::NotHeld)
                        }
                    }
                    File::Cell => claims.is(&f, *id),
                };
                let why = match is {
                    Is::Held => continue,
                    Is::Unknown => {
                        out.unknown.insert((file, *id));
                        unknown += 1;
                        continue;
                    }
                    Is::Lacked(why) => why,
                };
                let more = match by.more {
                    0 => String::new(),
                    m => format!(" and {m} more"),
                };
                let lb = id >> 16;
                let lacks = match why {
                    Why::NotHeld => format!("in no {file} file of iteration {n}"),
                    Why::Unclaimed(c) => format!(
                        "not among the {c} rooms landblock {lb:04X}'s info claims in the cell file of iteration {n}"
                    ),
                    Why::NoInfo => format!(
                        "landblock {lb:04X} has no rooms in the cell file of iteration {n}"
                    ),
                };
                out.missing.insert(
                    (file, *id),
                    format!("{file} {id:#010X}: {lacks} (named by {}{more})", by.first),
                );
            }
            if unknown > 0 {
                out.skipped.push(format!(
                    "{unknown} cell records the partial cell file of iteration {n} can't say of: what the files below said of them stands"
                ));
            }
            match file {
                File::Portal => self.portal = Some(f),
                File::Cell => self.cell = Some(f),
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the walls of a room and whether a point is in it, and which
    //! portals something places)
    use super::*;
    use empyrean_content::models::world::{
        Spell, Weenie, WeeniePropertiesDID, WeeniePropertiesEmote, WeeniePropertiesEmoteAction,
        WeeniePropertiesPosition,
    };

    /// Whether the destination `Named::of` finds in room `cell` is a placed portal's.
    fn destination_placed(c: &WorldContent, cell: u32) -> Option<bool> {
        Named::of(c)
            .points
            .get(&cell)?
            .iter()
            .find_map(|p| match p.what {
                What::Destination { placed } => Some(placed),
                What::Other => None,
            })
    }

    /// A world of a portal (class 30) sending its user to room `A9B40100`, and a weenie (class 40)
    /// linked to it as its portal `link` names, whose emote casts `spell` (with `link`) by `cast`.
    fn summoning(spell_link: Option<i32>, link: u16, cast: i32) -> WorldContent {
        let portal = Weenie {
            class_id: 30,
            r#type: 7,
            weenie_properties_position: vec![WeeniePropertiesPosition {
                position_type: PositionType::Destination.0,
                obj_cell_id: 0xA9B4_0100,
                angles_w: 1.0,
                ..Default::default()
            }],
            ..Default::default()
        };
        let summoner = Weenie {
            class_id: 40,
            r#type: 10,
            weenie_properties_did: vec![WeeniePropertiesDID {
                r#type: link,
                value: 30,
                ..Default::default()
            }],
            weenie_properties_emote: vec![WeeniePropertiesEmote {
                weenie_properties_emote_action: vec![WeeniePropertiesEmoteAction {
                    r#type: u32::try_from(cast).expect("an emote type"),
                    spell_id: Some(157),
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        };
        WorldContent {
            weenies: vec![portal, summoner],
            spells: vec![Spell {
                id: 157,
                link: spell_link,
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn a_portal_an_emote_summons_is_placed() {
        let one = PropertyDataId::LinkedPortalOne.0;
        let two = PropertyDataId::LinkedPortalTwo.0;
        let (cast, instant) = (EmoteType::CastSpell.0, EmoteType::CastSpellInstant.0);
        // Summoned by a cast of a spell with link 1, the caster's first linked portal, or of one
        // with link 2, its second: placed.
        assert_eq!(
            destination_placed(&summoning(Some(1), one, cast), 0xA9B4_0100),
            Some(true)
        );
        assert_eq!(
            destination_placed(&summoning(Some(2), two, instant), 0xA9B4_0100),
            Some(true)
        );
        // The other link, a spell that summons nothing, an emote that casts nothing: not placed.
        assert_eq!(
            destination_placed(&summoning(Some(1), two, cast), 0xA9B4_0100),
            Some(false)
        );
        assert_eq!(
            destination_placed(&summoning(None, one, cast), 0xA9B4_0100),
            Some(false)
        );
        assert_eq!(
            destination_placed(&summoning(Some(1), one, EmoteType::Say.0), 0xA9B4_0100),
            Some(false)
        );
    }

    #[test]
    fn a_point_is_in_a_room_when_no_wall_has_it_further_behind_than_the_slack() {
        // A box from -1 to 1 on each axis, its walls facing in.
        let planes = [
            (Vec3::new(1.0, 0.0, 0.0), 1.0),
            (Vec3::new(-1.0, 0.0, 0.0), 1.0),
            (Vec3::new(0.0, 1.0, 0.0), 1.0),
            (Vec3::new(0.0, -1.0, 0.0), 1.0),
            (Vec3::new(0.0, 0.0, 1.0), 1.0),
            (Vec3::new(0.0, 0.0, -1.0), 1.0),
        ]
        .map(|(normal, d)| Plane { normal, d })
        .to_vec();
        let w = Walls {
            planes,
            digest: [0; 32],
        };
        assert!(w.contains(Vec3::new(0.0, 0.0, 0.0)));
        assert!(w.contains(Vec3::new(0.0, 0.0, -1.0 - SLACK + 0.01)));
        assert!(!w.contains(Vec3::new(0.0, 0.0, -1.0 - SLACK - 0.01)));
        assert!(!w.contains(Vec3::new(3.0, 0.0, 0.0)));
        // A room turned a quarter about z and moved: a point is taken into its own frame first.
        let s = Shape {
            env: 0x0D00_0001,
            cell: 0,
            frame: Frame {
                origin: Vec3::new(10.0, 0.0, 0.0),
                rotation: Quat::new(
                    std::f32::consts::FRAC_1_SQRT_2,
                    0.0,
                    0.0,
                    std::f32::consts::FRAC_1_SQRT_2,
                ),
            },
            walls: Some(Arc::new(Walls {
                planes: vec![Plane {
                    normal: Vec3::new(1.0, 0.0, 0.0),
                    d: 0.0,
                }],
                digest: [0; 32],
            })),
        };
        // Local +x is the landblock's +y once turned.
        assert_eq!(s.holds(Vec3::new(10.0, 2.0, 0.0)), Some(true));
        assert_eq!(s.holds(Vec3::new(10.0, -2.0, 0.0)), Some(false));
    }
}
