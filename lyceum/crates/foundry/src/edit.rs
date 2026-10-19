//! An edit session: a world open for editing, the operations that change it, undo and redo, the
//! landblocks an operation reaches compiled again on their own, and the sources saved.
//!
//! A session holds the world's packs as read, the world compiled over the files on this machine
//! ([`crate::compile`]), and the text of every source file it has changed. An operation ([`Op`])
//! names things as the compiled world holds them (a static by its place in its landblock's
//! compiled statics record, a vertex by its world grid point) and becomes a change to one pack's
//! sources, the **target** pack's:
//!
//! - a static the target pack places is changed where it is written, its own `[[static]]`;
//! - a static of the base is moved or removed by its index, another pack's by its symbol;
//! - a vertex field goes into the file of the target pack that already sets it, else into the
//!   landblock where the vertex's local coordinates are 0 to 7, so one vertex is never set from two
//!   files;
//! - a spawn is one SQL file of its own, `server/sql/landblocks/XXYY <name>.sql`.
//!
//! The files are edited in place (`toml_edit`): comments, order and the layout of everything else
//! stay. New entries are written in one form: meters to three decimals, degrees to two, ids in
//! hexadecimal.
//!
//! After each operation only the landblocks it reaches are compiled again
//! ([`crate::compile::recompile_landblocks`]): those whose files changed, and every landblock
//! holding a copy of a vertex whose value changed. The result is the bytes a full build makes of
//! the same sources. Undo puts back the text each operation changed and compiles those landblocks
//! again; redo puts the operation's text back.
//!
//! [`Session::save`] writes the target pack's files the session changed, and only where their text
//! differs from the disk's, and the lockfile when an id is new.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use dereth_assets::world::{CellLandblock, LandblockInfo, ObjectEntry};
use dereth_assets::{Decode, Region};
use dereth_dat::ModernDat;
use dereth_primitives::{DataId, Frame, Quat, Vec3};
use toml_edit::{ArrayOfTables, DocumentMut, Item, Table, Value};

use crate::compile::{
    self, copies, gather_vertices, read_compiled, Compiled, Field, Records, Staged, StaticOrigin,
    VertexSets, NEW_RECORD_VERSION,
};
use crate::dats::DatSet;
use crate::error::{Error, Loc};
use crate::lock::Lock;
use crate::source::{self, Height, Pack, Place, Rotation, SqlFile};
use crate::sql_ids;
use crate::symbol;
use crate::world::World;

/// A weenie a spawn places: a pack's, by its symbol in full, or a class id of the server's own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Weenie {
    Symbol(String),
    Class(u32),
}

/// One field of one world vertex set to a value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VertexSet {
    /// The world grid point: `landblock x * 8 + local x`, the same for y.
    pub gx: u32,
    pub gy: u32,
    pub field: Field,
    pub value: u16,
}

/// A change to the world, as the editor asks for it.
#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    /// Place `model` in `landblock` at `at`, turned `rotation`.
    PlaceStatic {
        landblock: u16,
        model: DataId,
        at: Place,
        rotation: Rotation,
    },
    /// Move or turn a static of `landblock`, named by where it came from.
    MoveStatic {
        landblock: u16,
        which: StaticOrigin,
        at: Option<Place>,
        rotation: Option<Rotation>,
    },
    /// Take a static of `landblock` away.
    RemoveStatic { landblock: u16, which: StaticOrigin },
    /// Set fields of world vertices.
    SetVertices(Vec<VertexSet>),
    /// Place an instance of `weenie` in `landblock` at `origin` (landblock meters), facing
    /// `heading` degrees clockwise from north.
    PlaceSpawn {
        landblock: u16,
        weenie: Weenie,
        origin: Vec3,
        heading: f32,
    },
    /// Place an instance of `weenie` in indoor cell `cell` of `landblock`, at `origin`
    /// (landblock meters), facing `heading` degrees clockwise from north.
    PlaceSpawnInCell {
        landblock: u16,
        cell: u16,
        weenie: Weenie,
        origin: Vec3,
        heading: f32,
    },
    /// Stand `model` in the pack's new cell `cell` (its symbol, in full) of `landblock`, at `at`
    /// (landblock meters) turned `rotation`.
    PlaceCellStatic {
        landblock: u16,
        cell: String,
        model: DataId,
        at: [f32; 3],
        rotation: Rotation,
    },
    /// Move the spawn `guid` (as its SQL writes it) of `file` to `origin` (meters of its
    /// landblock), facing `heading` degrees clockwise from north. An outdoor spawn takes the
    /// outdoor cell it now stands in; one in a building's or a dungeon's cell keeps its cell.
    MoveSpawn {
        file: PathBuf,
        guid: String,
        origin: Vec3,
        heading: f32,
    },
    /// Take away the spawn written in `file`, which must place no other.
    RemoveSpawn { file: PathBuf },
    /// Take away the server's own spawn `guid`, its change written in `file`, its landblock's
    /// override file ([`Session::override_file`]).
    RemoveServerSpawn { file: PathBuf, guid: String },
    /// Write the target pack's SQL file `file` (under its `server/sql`) as `text`, or take it
    /// away: a weenie, a quest or a spawn as a form leaves it.
    WriteSql { file: PathBuf, text: Option<String> },
    /// Place piece `piece` of `environment` as a new cell of `landblock`, drawn with
    /// `surfaces`, at `at` (landblock meters) turned `rotation`. Each of the piece's portals, in
    /// its order, joins another new cell's (its symbol, in full, and that cell's portal, whose
    /// own link is written back), opens to the outdoors, or stays shut.
    PlaceCell {
        landblock: u16,
        environment: DataId,
        piece: u16,
        surfaces: Vec<DataId>,
        at: [f32; 3],
        rotation: Rotation,
        links: Vec<source::Link>,
    },
    /// Take a new cell of `landblock` away (its symbol, in full); the portals joined to it shut.
    RemoveCell { landblock: u16, name: String },
    /// Reshape cell `cell` of `landblock`'s own: piece `piece` of `environment` in its place,
    /// drawn with `surfaces`, at `at` (landblock meters) turned `rotation`; each of the piece's
    /// portals keeping a way the cell had, shut, or joined to a new cell (whose own link is
    /// written back). A cell reshaped before is reshaped anew.
    ReplaceCell {
        landblock: u16,
        cell: u16,
        environment: DataId,
        piece: u16,
        surfaces: Vec<DataId>,
        at: [f32; 3],
        rotation: Rotation,
        links: Vec<source::Link>,
    },
    /// Move or turn the static standing at `index` (its place among indoor cell `cell`'s statics
    /// as compiled) of `landblock` to `at` (landblock meters), turned `rotation` when given.
    MoveCellStatic {
        landblock: u16,
        cell: u16,
        index: usize,
        at: [f32; 3],
        rotation: Option<Rotation>,
    },
    /// Take the static standing at `index` of indoor cell `cell` of `landblock` away.
    RemoveCellStatic {
        landblock: u16,
        cell: u16,
        index: usize,
    },
    /// Move building `index` of `landblock` (the base's) to `at`, turned `rotation`, with every
    /// cell inside it.
    MoveBuilding {
        landblock: u16,
        index: usize,
        at: [f32; 3],
        rotation: Rotation,
    },
    /// Several changes as one, each over what the ones before it left.
    Batch(Vec<Op>),
}

/// What an operation, an undo or a redo reached.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Touched {
    /// The landblocks whose records may have changed: to draw again.
    pub landblocks: BTreeSet<u16>,
    /// Whether the server SQL changed.
    pub sql: bool,
}

impl Touched {
    fn add(&mut self, o: Self) {
        self.landblocks.extend(o.landblocks);
        self.sql |= o.sql;
    }
}

/// What an operation did: what it reached, and the static it placed, when it placed one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Applied {
    pub touched: Touched,
    /// The static placed, as its landblock and its origin, for the editor to select.
    pub placed: Option<(u16, StaticOrigin)>,
    /// The spawn placed, as its file.
    pub spawned: Option<PathBuf>,
}

/// One step of the history: the text of each file it changed, before and after.
#[derive(Clone, Debug)]
struct Step {
    before: BTreeMap<PathBuf, Option<String>>,
    after: BTreeMap<PathBuf, Option<String>>,
}

/// A spawn the packs' SQL places.
#[derive(Clone, Debug, PartialEq)]
pub struct Spawn {
    /// The pack whose SQL places it.
    pub pack: usize,
    pub file: PathBuf,
    /// The guid and the weenie as the SQL writes them.
    pub guid: String,
    pub weenie: String,
    pub cell: u32,
    /// Landblock meters and rotation.
    pub frame: Frame,
    /// The setup the weenie is drawn with, when the session knows it.
    pub setup: Option<DataId>,
    /// The line of `file` its values start on.
    pub line: usize,
    /// Whether it is one of the server's own the packs leave as it is: `file` is then the
    /// target pack's file a change to it would be written in, which places nothing of it yet.
    pub server: bool,
}

impl Spawn {
    /// The landblock it stands in.
    #[must_use]
    pub fn landblock(&self) -> u16 {
        u16::try_from(self.cell >> 16).unwrap_or(0)
    }
}

/// A world vertex as the compiled world holds it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vertex {
    pub height: u8,
    /// The height in meters, through the region's height table.
    pub z: f32,
    pub terrain: u16,
    pub scene: u16,
    pub road: u16,
}

/// A fault or a warning the checks found, with where in the world it is, when it has a place.
#[derive(Clone, Debug, PartialEq)]
pub struct Fault {
    pub text: String,
    pub warning: bool,
    /// The landblock and the landblock meters it is at.
    pub at: Option<(u16, Vec3)>,
}

/// A world open for editing.
#[derive(Debug)]
pub struct Session {
    world: World,
    base: DatSet,
    packs: Vec<Pack>,
    target: usize,
    opened_lock: Lock,
    compiled: Compiled,
    region: Option<Region>,
    /// Every source file the session has read or changed, its text now (`None`: no file).
    texts: BTreeMap<PathBuf, Option<String>>,
    /// Those files' text on disk, as last read or saved.
    disk: BTreeMap<PathBuf, Option<String>>,
    undo: Vec<Step>,
    redo: Vec<Step>,
    /// The date an SQL row is stamped with, `YYYY-MM-DD 00:00:00`.
    today: String,
    /// What the server's own weenies are, by class id, when the session was given them.
    retail: BTreeMap<u32, sql_ids::WeenieFacts>,
    /// The server's own instances by landblock, and its generators' links (parent, child).
    retail_instances: BTreeMap<u16, Vec<sql_ids::Instance>>,
    retail_links: Vec<(u32, u32)>,
    /// The landblocks whose server spawns are shown.
    shown: BTreeSet<u16>,
    /// The server's quests and death treasures.
    retail_quests: BTreeMap<String, sql_ids::Quest>,
    retail_treasure: BTreeMap<u32, sql_ids::Treasure>,
    /// The server's names, which the checks of the packs' SQL read when given.
    schema: Option<crate::content::Schema>,
    /// How many times a landblock's cells were read.
    cell_reads: std::cell::Cell<u64>,
}

/// What a spawn makes as a generator: how far around it scatters what it makes, each profile
/// of its weenie, and the instances the world links to it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Generator {
    pub radius: Option<f32>,
    pub profiles: Vec<Profile>,
    /// The guids (as [`Session::spawns`] writes them) of the instances it owns.
    pub children: Vec<String>,
}

/// One generator profile: what it makes, how many, and where.
#[derive(Clone, Debug, PartialEq)]
pub struct Profile {
    /// The class it makes, as written.
    pub class: String,
    pub init: i32,
    pub max: i32,
    /// Where it makes it (`where_Create`): bit 2 scattered around the generator, bit 4 at its
    /// own place.
    pub place: u32,
    /// Its own place, when it has one: the cell and the meters of its landblock.
    pub cell: Option<u32>,
    pub origin: Option<Vec3>,
}

/// A guid as an override file writes it.
#[must_use]
pub fn guid_text(g: u32) -> String {
    format!("0x{g:08X}")
}

/// One server spawn an override file changes: written again whole (with the instances its
/// generator links), or taken away.
#[derive(Clone, Debug)]
enum Override {
    Placed(sql_ids::Instance, Vec<u32>),
    Removed { link_child: bool },
}

/// An override file: the server's own spawns of one landblock a pack changes.
#[derive(Clone, Debug, Default)]
struct Overrides {
    entries: BTreeMap<u32, Override>,
}

impl Overrides {
    /// What `text` (an override file, or none yet) changes.
    fn read(text: &str) -> Self {
        let mut entries = BTreeMap::new();
        let mut links: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
        let mut unlinked: BTreeSet<u32> = BTreeSet::new();
        sql_ids::rows(text, |row| {
            if row.table == "landblock_instance_link" {
                let g = |c: &str| row.get(c).and_then(sql_ids::number);
                if let (Some(p), Some(c)) = (g("parent_GUID"), g("child_GUID")) {
                    links.entry(p).or_default().push(c);
                }
            }
        });
        for line in text.lines() {
            let l = line.replace('`', "").to_ascii_lowercase();
            if let Some(v) = l
                .strip_prefix("delete from landblock_instance_link where child_guid = ")
                .and_then(|v| sql_ids::number(v.trim_end_matches(';').trim()))
            {
                unlinked.insert(v);
            }
        }
        for g in sql_ids::deleted_instances(text) {
            if let Some(g) = sql_ids::number(&g) {
                entries.insert(
                    g,
                    Override::Removed {
                        link_child: unlinked.contains(&g),
                    },
                );
            }
        }
        for i in sql_ids::instances(text) {
            if let Some(g) = sql_ids::number(&i.guid) {
                let children = links.remove(&g).unwrap_or_default();
                entries.insert(g, Override::Placed(i, children));
            }
        }
        Self { entries }
    }

    /// The file's text: a header, then each spawn by guid, its old rows deleted, then written
    /// again, or not.
    fn render(&self, lb: u16, today: &str, name: &dyn Fn(&str) -> Option<String>) -> String {
        let f = |v: f32| trimmed(format!("{v:.6}"));
        let mut out = format!(
            "/* The server's own spawns of landblock {lb:04X} this pack changes: each one written \
             again whole, or taken away. */\n"
        );
        for (g, o) in &self.entries {
            let guid = guid_text(*g);
            out.push_str(&format!(
                "\nDELETE FROM `landblock_instance` WHERE `guid` = {guid};\n"
            ));
            match o {
                Override::Removed { link_child } => {
                    if *link_child {
                        out.push_str(&format!(
                            "DELETE FROM `landblock_instance_link` WHERE `child_GUID` = {guid};\n"
                        ));
                    }
                }
                Override::Placed(i, children) => {
                    let comment = name(&i.weenie)
                        .map(|n| format!(" /* {} */", n.replace("*/", "* /")))
                        .unwrap_or_default();
                    let [w, x, y, z] = i.angles;
                    out.push_str(&format!(
                        "\nINSERT INTO `landblock_instance` (`guid`, `weenie_Class_Id`, \
                         `obj_Cell_Id`, `origin_X`, `origin_Y`, `origin_Z`, `angles_W`, \
                         `angles_X`, `angles_Y`, `angles_Z`, `is_Link_Child`, `last_Modified`)\n\
                         VALUES ({guid}, {}, 0x{:08X}, {}, {}, {}, {}, {}, {}, {}, {}, '{today}'){comment};\n",
                        i.weenie,
                        i.cell,
                        f(i.origin[0]),
                        f(i.origin[1]),
                        f(i.origin[2]),
                        f(w),
                        f(x),
                        f(y),
                        f(z),
                        if i.link_child { "True" } else { "False" },
                    ));
                    if !children.is_empty() {
                        out.push_str(
                            "\nINSERT INTO `landblock_instance_link` (`parent_GUID`, `child_GUID`, \
                             `last_Modified`)\nVALUES ",
                        );
                        let rows: Vec<String> = children
                            .iter()
                            .map(|c| format!("({guid}, {}, '{today}')", guid_text(*c)))
                            .collect();
                        out.push_str(&rows.join("\n     , "));
                        out.push_str(";\n");
                    }
                }
            }
        }
        out
    }
}

/// A number of meters as the session writes it: three decimals, trailing zeros dropped, one kept.
#[must_use]
pub fn meters(v: f32) -> String {
    trimmed(format!("{v:.3}"))
}

/// A number of degrees as the session writes it: two decimals, trailing zeros dropped, one kept.
#[must_use]
pub fn degrees(v: f32) -> String {
    trimmed(format!("{v:.2}"))
}

fn trimmed(s: String) -> String {
    let s = if s.starts_with("-0") && s.trim_start_matches(['-', '0', '.']).is_empty() {
        s.trim_start_matches('-').to_owned()
    } else {
        s
    };
    let t = s.trim_end_matches('0');
    if t.ends_with('.') {
        format!("{t}0")
    } else {
        t.to_owned()
    }
}

fn parse_value(text: &str) -> Value {
    text.parse::<Value>()
        .unwrap_or_else(|_| Value::from(text.to_owned()))
}

fn place_value(p: &Place) -> Value {
    let z = match p.z {
        Height::At(z) => meters(z),
        Height::Ground => "\"ground\"".to_owned(),
    };
    parse_value(&format!("[{}, {}, {z}]", meters(p.x), meters(p.y)))
}

fn set_rotation(t: &mut Table, r: Rotation) {
    match r {
        Rotation::Heading(h) => {
            t.remove("rotation");
            t.insert("heading", Item::Value(parse_value(&degrees(h))));
        }
        Rotation::Quat([w, x, y, z]) => {
            t.remove("heading");
            let f = |v: f32| trimmed(format!("{v:.6}"));
            t.insert(
                "rotation",
                Item::Value(parse_value(&format!(
                    "[{}, {}, {}, {}]",
                    f(w),
                    f(x),
                    f(y),
                    f(z)
                ))),
            );
        }
    }
}

fn hex(id: u32) -> Value {
    parse_value(&format!("0x{id:08X}"))
}

/// A portal's link as a cell's row writes it; `bare` takes the pack off a symbol of its own.
fn link_text(l: &source::Link, bare: &dyn Fn(&str) -> String) -> String {
    match l {
        source::Link::Shut => String::new(),
        source::Link::Outside => "outside".to_owned(),
        source::Link::Keep => "keep".to_owned(),
        source::Link::Cell { cell, portal } => format!("{}:{portal}", bare(cell)),
        source::Link::Base { cell, portal } => format!("0x{cell:04X}:{portal}"),
    }
}

/// A cell's number as the session writes it: `0x0105`.
fn hex16(n: u16) -> Value {
    parse_value(&format!("0x{n:04X}"))
}

/// A document's text.
fn render(doc: &DocumentMut) -> String {
    doc.to_string()
}

/// Add `t` as the last entry of the array `key`: set off by a blank line, unless it is the first
/// thing in the file.
fn push(doc: &mut DocumentMut, key: &str, mut t: Table) {
    if doc.to_string().trim().is_empty() {
        t.decor_mut().set_prefix("");
    }
    tables(doc, key).push(t);
}

/// Remove entry `k` of the array `key`. The comment written right before it goes with it; a
/// paragraph before that, set off by a blank line (a file's heading), stays, before the entry
/// that follows.
fn remove_table(doc: &mut DocumentMut, key: &str, k: usize) {
    let prefix = tables(doc, key)
        .get(k)
        .and_then(|t| t.decor().prefix())
        .and_then(|p| p.as_str())
        .unwrap_or("")
        .to_owned();
    let kept = prefix
        .rfind("\n\n")
        .map_or_else(String::new, |i| prefix[..i + 2].to_owned());
    let aot = tables(doc, key);
    aot.remove(k);
    if kept.is_empty() {
        return;
    }
    if let Some(next) = aot.get_mut(k) {
        let own = next
            .decor()
            .prefix()
            .and_then(|p| p.as_str())
            .unwrap_or("")
            .to_owned();
        let own = own.strip_prefix('\n').unwrap_or(&own).to_owned();
        next.decor_mut().set_prefix(format!("{kept}{own}"));
    } else {
        let trailing = doc.trailing().as_str().unwrap_or("").to_owned();
        doc.set_trailing(format!("{kept}{trailing}"));
    }
}

/// A table that starts a new entry of an array, set off by a blank line.
fn entry() -> Table {
    let mut t = Table::new();
    t.decor_mut().set_prefix("\n");
    t
}

fn tables<'d>(doc: &'d mut DocumentMut, key: &str) -> &'d mut ArrayOfTables {
    if !doc.contains_key(key) {
        doc.insert(key, Item::ArrayOfTables(ArrayOfTables::new()));
    }
    doc[key]
        .as_array_of_tables_mut()
        .expect("an operation key is an array of tables")
}

fn int(t: &Table, key: &str) -> Option<i64> {
    t.get(key).and_then(Item::as_integer)
}

fn string<'t>(t: &'t Table, key: &str) -> Option<&'t str> {
    t.get(key).and_then(Item::as_str)
}

/// The civil date of a count of days since 1970-01-01.
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = u32::try_from(doy - (153 * mp + 2) / 5 + 1).unwrap_or(1);
    let m = u32::try_from(if mp < 10 { mp + 3 } else { mp - 9 }).unwrap_or(1);
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn today() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let (y, m, d) = civil(i64::try_from(secs / 86_400).unwrap_or(0));
    format!("{y:04}-{m:02}-{d:02} 00:00:00")
}

/// The landblock that owns world vertex `(gx, gy)` when no file sets it yet, and its local
/// coordinates there: local 0 to 7 inside a landblock, so an east or north edge belongs to the
/// next one; the world's own far edge to the last.
#[must_use]
pub fn vertex_owner(gx: u32, gy: u32) -> (u16, u8, u8) {
    let bx = (gx / 8).min(0xFE);
    let by = (gy / 8).min(0xFE);
    let lb = u16::try_from((bx << 8) | by).unwrap_or(0);
    (
        lb,
        u8::try_from(gx - bx * 8).unwrap_or(8),
        u8::try_from(gy - by * 8).unwrap_or(8),
    )
}

/// The outdoor cell of landblock meters `(x, y)`, `1` to `0x40`.
#[must_use]
pub fn outdoor_cell(x: f32, y: f32) -> u32 {
    let cell = |v: f32| {
        u32::try_from(dereth_primitives::num::floor_to_i32(v / 24.0).clamp(0, 7)).unwrap_or(0)
    };
    1 + cell(x) * 8 + cell(y)
}

/// A token's symbol, qualified for the pack whose file names it: `{{wcid:guide}}` in pack
/// `holtburg` is `holtburg:guide`. `None` for a value that is no token.
fn token_symbol(pack: &str, v: &str) -> Option<String> {
    let inner = v.trim().strip_prefix("{{")?.strip_suffix("}}")?;
    let (_, sym) = inner.split_once(':')?;
    symbol::qualify(pack, sym.trim()).ok()
}

impl Session {
    /// Open the world at `world_path` over the files in `dats`, editing the pack named `target`
    /// (the world's last when `None`).
    ///
    /// # Errors
    /// As a build: a world or pack that does not read or hold together, the data files' errors;
    /// a world with no pack, or no pack by that name.
    pub fn open(world_path: &Path, dats: &Path, target: Option<&str>) -> Result<Self, Error> {
        let world = World::read(world_path)?;
        let packs = world
            .packs
            .iter()
            .map(|p| source::read_pack(p))
            .collect::<Result<Vec<_>, _>>()?;
        let mut names = BTreeSet::new();
        for p in &packs {
            if !names.insert(p.name.clone()) {
                return Err(Error::source(
                    &Loc::new(&world.path, 0),
                    format!("two packs are named {}", p.name),
                ));
            }
        }
        let target = match target {
            Some(name) => packs
                .iter()
                .position(|p| p.name == name)
                .ok_or_else(|| Error::Refused(format!("the world has no pack named {name}")))?,
            None => packs
                .len()
                .checked_sub(1)
                .ok_or_else(|| Error::Refused("the world has no pack to edit".into()))?,
        };
        let base = DatSet::open(dats)?;
        let opened_lock = Lock::read(&world.lock_path())?;
        let compiled = compile::compile(&world, &packs, &base, &opened_lock)?;
        let region = read_compiled(&compiled, &base, ModernDat::Portal, compile::REGION.raw())
            .and_then(|b| Region::decode_payload(compile::REGION, &b).ok());
        Ok(Self {
            world,
            base,
            packs,
            target,
            opened_lock,
            compiled,
            region,
            texts: BTreeMap::new(),
            disk: BTreeMap::new(),
            undo: Vec::new(),
            redo: Vec::new(),
            today: today(),
            retail: BTreeMap::new(),
            retail_instances: BTreeMap::new(),
            retail_links: Vec::new(),
            shown: BTreeSet::new(),
            retail_quests: BTreeMap::new(),
            retail_treasure: BTreeMap::new(),
            schema: None,
            cell_reads: std::cell::Cell::new(0),
        })
    }

    /// How many times a landblock's indoor cells were read ([`Self::cells`]): the work an
    /// editor's frame does, counted.
    #[must_use]
    pub fn cells_read(&self) -> u64 {
        self.cell_reads.get()
    }

    /// The world.
    #[must_use]
    pub fn world(&self) -> &World {
        &self.world
    }

    /// The data files the world is compiled over.
    #[must_use]
    pub fn base(&self) -> &DatSet {
        &self.base
    }

    /// The packs, in load order.
    #[must_use]
    pub fn packs(&self) -> &[Pack] {
        &self.packs
    }

    /// The pack the session writes into, by its place in [`Self::packs`].
    #[must_use]
    pub fn target(&self) -> usize {
        self.target
    }

    /// The world as compiled now.
    #[must_use]
    pub fn compiled(&self) -> &Compiled {
        &self.compiled
    }

    /// The region record: the height table and the terrain types.
    #[must_use]
    pub fn region(&self) -> Option<&Region> {
        self.region.as_ref()
    }

    /// Stamp SQL rows the session writes with `date` (`YYYY-MM-DD hh:mm:ss`) rather than today.
    pub fn set_today(&mut self, date: &str) {
        date.clone_into(&mut self.today);
    }

    /// Learn what the server's own weenies are from `text`, ACE SQL (an export or a dump): what
    /// a spawn of one is drawn with, and what the model browser calls a setup and files it under.
    pub fn learn_weenies(&mut self, text: &str) {
        self.learn_server(sql_ids::server_content(text));
    }

    /// Learn what the server's own world holds, read already: its weenies, its instances and
    /// its generators' links.
    pub fn learn_server(&mut self, content: sql_ids::ServerContent) {
        self.retail.extend(content.weenies);
        for i in content.instances {
            if let Ok(lb) = u16::try_from(i.cell >> 16) {
                self.retail_instances.entry(lb).or_default().push(i);
            }
        }
        self.retail_links.extend(content.links);
        self.retail_quests.extend(content.quests);
        self.retail_treasure.extend(content.treasure);
    }

    /// Check the packs' SQL against the server's names `schema` too.
    pub fn set_schema(&mut self, schema: Option<crate::content::Schema>) {
        self.schema = schema;
    }

    /// Quest `name` as registered, and where: the pack's file that registers it (a later
    /// pack's over an earlier's), or the server's own (`None`).
    #[must_use]
    pub fn quest(&self, name: &str) -> Option<(sql_ids::Quest, Option<PathBuf>)> {
        let key = crate::content_check::quest_key(name);
        for p in self.packs.iter().rev() {
            for f in &p.sql {
                for (n, q) in sql_ids::quests_in(&f.text) {
                    if crate::content_check::quest_key(&n) == key {
                        return Some((q, Some(f.path.clone())));
                    }
                }
            }
        }
        self.retail_quests
            .iter()
            .find(|(n, _)| crate::content_check::quest_key(n) == key)
            .map(|(_, q)| (q.clone(), None))
    }

    /// The file the pack being edited registers quest `name` in.
    #[must_use]
    pub fn quest_file(&self, name: &str) -> PathBuf {
        let stem: String = name
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        self.pack_dir()
            .join("server")
            .join("sql")
            .join("quests")
            .join(format!("{stem}.sql"))
    }

    /// Quest `name` registered as `q`, as the file [`Self::quest_file`] holds it.
    #[must_use]
    pub fn quest_text(&self, name: &str, q: &sql_ids::Quest) -> String {
        let n = crate::content::quote(name);
        format!(
            "DELETE FROM `quest` WHERE `name` = {n};\n\n\
             INSERT INTO `quest` (`name`, `min_Delta`, `max_Solves`, `message`, `last_Modified`)\n\
             VALUES ({n}, {}, {}, {}, '{}');\n",
            q.min_delta,
            q.max_solves,
            crate::content::quote(&q.message),
            self.today
        )
    }

    /// The server's death treasure of treasure type `id`.
    #[must_use]
    pub fn treasure(&self, id: u32) -> Option<&sql_ids::Treasure> {
        self.retail_treasure.get(&id)
    }

    /// Every death treasure the server's database holds, by its id.
    #[must_use]
    pub fn treasures(&self) -> &BTreeMap<u32, sql_ids::Treasure> {
        &self.retail_treasure
    }

    /// Whether the session knows what server weenie `class` is drawn with.
    #[must_use]
    pub fn knows_weenie(&self, class: u32) -> bool {
        self.retail.get(&class).is_some_and(|f| f.setup.is_some())
    }

    /// What the server's own weenies are, by class id, as the session learned them.
    #[must_use]
    pub fn server_weenies(&self) -> &BTreeMap<u32, sql_ids::WeenieFacts> {
        &self.retail
    }

    /// What the SQL says of server class `class`: a pack's copy of it (which takes its place on
    /// the server), else the server's own.
    #[must_use]
    pub fn class_facts(&self, class: u32) -> Option<sql_ids::WeenieFacts> {
        self.pack_classes()
            .remove(&class)
            .or_else(|| self.retail.get(&class).cloned())
    }

    /// The server classes the packs write again, and what their SQL says of each (a later
    /// pack's over an earlier's).
    #[must_use]
    pub fn pack_classes(&self) -> BTreeMap<u32, sql_ids::WeenieFacts> {
        let mut out = BTreeMap::new();
        for p in &self.packs {
            for f in &p.sql {
                for (w, x) in sql_ids::weenie_facts(&f.text) {
                    if let Some(c) = sql_ids::number(&w) {
                        out.insert(c, x);
                    }
                }
            }
        }
        out
    }

    /// The SQL file that writes weenie `w`'s rows, and whether it is the pack being edited's:
    /// the pack's file of its symbol, or of a server class the pack writes again.
    #[must_use]
    pub fn weenie_file(&self, w: &Weenie) -> Option<(PathBuf, bool)> {
        for (pi, p) in self.packs.iter().enumerate().rev() {
            for f in &p.sql {
                let writes = sql_ids::weenie_facts(&f.text).keys().any(|k| match w {
                    Weenie::Class(c) => sql_ids::number(k) == Some(*c),
                    Weenie::Symbol(s) => token_symbol(&p.name, k).as_deref() == Some(s.as_str()),
                }) && f.text.contains("INSERT INTO `weenie` (");
                if writes {
                    return Some((f.path.clone(), pi == self.target));
                }
            }
        }
        None
    }

    /// The weenie the packs' SQL file `path` writes, when it writes one.
    #[must_use]
    pub fn weenie_of_file(&self, path: &Path) -> Option<Weenie> {
        let (p, f) = self
            .packs
            .iter()
            .find_map(|p| p.sql.iter().find(|f| f.path == path).map(|f| (p, f)))?;
        if !f.text.contains("INSERT INTO `weenie` (") {
            return None;
        }
        let first = sql_ids::weenie_facts(&f.text).into_keys().next()?;
        match sql_ids::number(&first) {
            Some(c) => Some(Weenie::Class(c)),
            None => token_symbol(&p.name, &first).map(Weenie::Symbol),
        }
    }

    /// The SQL text of `path` as the session holds it now.
    pub fn sql_text(&mut self, path: &Path) -> Option<String> {
        self.text(path)
    }

    /// The file a new weenie of the pack being edited with symbol `name` is written in, and its
    /// full symbol.
    ///
    /// # Errors
    /// A name that is not a symbol.
    pub fn new_weenie_file(&self, name: &str) -> Result<(PathBuf, String), Error> {
        let pack = &self.packs[self.target].name;
        let sym = symbol::qualify(pack, name).map_err(Error::Refused)?;
        if sym.split_once(':').is_some_and(|(p, _)| p != pack) || name.contains('@') {
            return Err(Error::Refused(format!(
                "{name} is not a name for a weenie of {pack}"
            )));
        }
        let file = self
            .pack_dir()
            .join("server")
            .join("sql")
            .join("weenies")
            .join(format!("{name}.sql"));
        Ok((file, sym))
    }

    /// The file a pack's copy of server class `class` named `name` is written in.
    #[must_use]
    pub fn class_file(&self, class: u32, name: &str) -> PathBuf {
        let name: String = name
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == ' ' || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        self.pack_dir()
            .join("server")
            .join("sql")
            .join("weenies")
            .join(format!("{class} {}.sql", name.trim()))
    }

    /// What the packs' own weenies are, by symbol (`pack:name`), as their SQL says.
    #[must_use]
    pub fn pack_weenies(&self) -> BTreeMap<String, sql_ids::WeenieFacts> {
        let mut out: BTreeMap<String, sql_ids::WeenieFacts> = BTreeMap::new();
        for p in &self.packs {
            for f in &p.sql {
                for (w, x) in sql_ids::weenie_facts(&f.text) {
                    let Some(sym) = token_symbol(&p.name, &w) else {
                        continue;
                    };
                    let e = out.entry(sym).or_default();
                    e.name = x.name.or(e.name.take());
                    e.setup = x.setup.or(e.setup);
                    e.kind = x.kind.or(e.kind);
                    e.stuck = x.stuck.or(e.stuck);
                    e.attackable = x.attackable.or(e.attackable);
                }
            }
        }
        out
    }

    /// The weenies drawn with each setup: the server's own the session learned and the packs'
    /// (from the packs' SQL), each with what its SQL says of it, in name order.
    #[must_use]
    pub fn weenies_by_setup(&self) -> BTreeMap<u32, Vec<sql_ids::WeenieFacts>> {
        let mut out: BTreeMap<u32, Vec<sql_ids::WeenieFacts>> = BTreeMap::new();
        let mut add = |f: &sql_ids::WeenieFacts| {
            if let (Some(setup), Some(_)) = (f.setup, &f.name) {
                out.entry(setup).or_default().push(f.clone());
            }
        };
        for f in self.retail.values() {
            add(f);
        }
        for p in &self.packs {
            let mut facts: BTreeMap<String, sql_ids::WeenieFacts> = BTreeMap::new();
            for f in &p.sql {
                for (w, x) in sql_ids::weenie_facts(&f.text) {
                    let e = facts.entry(w).or_default();
                    e.name = x.name.or(e.name.take());
                    e.setup = x.setup.or(e.setup);
                    e.kind = x.kind.or(e.kind);
                    e.stuck = x.stuck.or(e.stuck);
                    e.attackable = x.attackable.or(e.attackable);
                }
            }
            for f in facts.values() {
                add(f);
            }
        }
        for v in out.values_mut() {
            v.sort_by(|a, b| a.name.cmp(&b.name));
            v.dedup();
        }
        out
    }

    /// The names of the weenies drawn with each setup: the packs' weenies (by their names in
    /// the packs' SQL) and the server's own the session learned, in name order.
    #[must_use]
    pub fn weenie_names_by_setup(&self) -> BTreeMap<u32, Vec<String>> {
        self.weenies_by_setup()
            .into_iter()
            .map(|(k, v)| {
                let names: BTreeSet<String> = v.into_iter().filter_map(|f| f.name).collect();
                (k, names.into_iter().collect())
            })
            .collect()
    }

    // ---------------------------------------------------------------------------------------
    // the world as compiled
    // ---------------------------------------------------------------------------------------

    /// The record `id` of the cell file as the compiled world reads it.
    fn cell_record(&self, id: u32) -> Option<Vec<u8>> {
        read_compiled(&self.compiled, &self.base, ModernDat::Cell, id)
    }

    /// Landblock `lb`'s terrain as the compiled world holds it.
    #[must_use]
    pub fn terrain(&self, lb: u16) -> Option<CellLandblock> {
        let id = (u32::from(lb) << 16) | 0xFFFF;
        CellLandblock::decode_payload(DataId(id), &self.cell_record(id)?).ok()
    }

    /// Landblock `lb`'s statics record as the compiled world holds it.
    #[must_use]
    pub fn statics(&self, lb: u16) -> Option<LandblockInfo> {
        let id = (u32::from(lb) << 16) | 0xFFFE;
        LandblockInfo::decode_payload(DataId(id), &self.cell_record(id)?).ok()
    }

    /// Landblock `lb`'s indoor cells as compiled, by index.
    #[must_use]
    pub fn cells(&self, lb: u16) -> BTreeMap<u16, dereth_assets::world::EnvCell> {
        self.cell_reads.set(self.cell_reads.get() + 1);
        let mut out = BTreeMap::new();
        let mut n = crate::cells::FIRST_CELL;
        loop {
            let id = (u32::from(lb) << 16) | u32::from(n);
            let Some(bytes) = self.cell_record(id) else {
                break;
            };
            match dereth_assets::world::EnvCell::decode_payload(DataId(id), &bytes) {
                Ok(c) => {
                    out.insert(n, c);
                }
                Err(_) => break,
            }
            n += 1;
        }
        out
    }

    /// The packs' new cells of landblock `lb`: each one's symbol, by its index.
    #[must_use]
    pub fn cell_names(&self, lb: u16) -> BTreeMap<u16, String> {
        self.compiled
            .cells
            .get(&lb)
            .map(|m| m.iter().map(|(name, c)| (*c, name.clone())).collect())
            .unwrap_or_default()
    }

    /// The environment `id`, as the files hold it.
    #[must_use]
    pub fn environment(&self, id: DataId) -> Option<dereth_assets::Environment> {
        let bytes = self.base.read_portal(id)?;
        dereth_assets::Environment::decode_payload(id, &bytes).ok()
    }

    /// The walk checks of landblock `lb`'s new cells ([`crate::walk`]).
    #[must_use]
    pub fn walk(&self, lb: u16) -> Vec<crate::walk::Finding> {
        let cells = self.cells(lb);
        let which: BTreeSet<u16> = self.cell_names(lb).keys().copied().collect();
        if which.is_empty() {
            return Vec::new();
        }
        crate::walk::check(&cells, &which, which.iter().next().copied(), &|id| {
            self.environment(id)
        })
    }

    /// Where a ray from `from` going `dir` (landblock `lb`'s meters, `dir` of length one) first
    /// meets a wall or floor of one of the landblock's indoor cells: the cell and the point.
    #[must_use]
    pub fn cell_hit(&self, lb: u16, from: Vec3, dir: Vec3) -> Option<(u16, Vec3)> {
        crate::walk::cell_hit(&self.cells(lb), from, dir, &|id| self.environment(id))
    }

    /// The indoor cell of landblock `lb` point `p` (landblock meters) stands in, and the floor's
    /// height under it.
    #[must_use]
    pub fn cell_at(&self, lb: u16, p: Vec3) -> Option<(u16, f32)> {
        crate::walk::cell_at(&self.cells(lb), p, &|id| self.environment(id))
    }

    /// The world's own dungeons: every landblock of the files whose first cell is a dungeon's,
    /// with its number of cells ([`crate::dungeons::retail`]). A read of every landblock record,
    /// to be kept.
    #[must_use]
    pub fn retail_dungeons(&self) -> BTreeMap<u16, u32> {
        crate::dungeons::retail(&|id| self.base.cell.read(id).ok())
    }

    /// The dungeons: the world's own (`retail`, as [`Self::retail_dungeons`] found them) and every
    /// landblock a pack places cells in, each named by the portals the server knows that lead
    /// into it, in landblock order.
    #[must_use]
    pub fn dungeons(&self, retail: &BTreeMap<u16, u32>) -> Vec<crate::dungeons::Dungeon> {
        let names = crate::dungeons::portal_names(&self.retail);
        let mut out: BTreeMap<u16, crate::dungeons::Dungeon> = retail
            .iter()
            .map(|(lb, n)| {
                (
                    *lb,
                    crate::dungeons::Dungeon {
                        landblock: *lb,
                        name: names.get(lb).cloned(),
                        cells: *n,
                        pack: false,
                    },
                )
            })
            .collect();
        for (lb, cells) in &self.compiled.cells {
            let d = out.entry(*lb).or_insert_with(|| crate::dungeons::Dungeon {
                landblock: *lb,
                name: names.get(lb).cloned(),
                cells: 0,
                pack: true,
            });
            d.pack = true;
            d.cells = d.cells.max(u32::try_from(cells.len()).unwrap_or(u32::MAX));
        }
        out.into_values().collect()
    }

    /// The rooms of landblock `lb`'s indoor cells, as their cell trees bound them: which one a
    /// point is in ([`crate::walk::CellShape::holds`]).
    #[must_use]
    pub fn cell_shapes(&self, lb: u16) -> Vec<crate::walk::CellShape> {
        crate::walk::cell_shapes(&self.cells(lb), &|id| self.environment(id))
    }

    /// Building `index` of landblock `lb` moved to `at` (landblock meters) and turned `rotation`,
    /// the spawns that go with it ([`Self::building_spawns`]) carried along: one step.
    #[must_use]
    pub fn move_building(
        &self,
        lb: u16,
        index: usize,
        at: [f32; 3],
        rotation: Rotation,
    ) -> Option<Op> {
        let from = self.statics(lb)?.buildings.get(index)?.frame;
        let to = Frame::new(
            Vec3::new(at[0], at[1], at[2]),
            crate::compile::quat_of(rotation),
        );
        let mut ops = vec![Op::MoveBuilding {
            landblock: lb,
            index,
            at,
            rotation,
        }];
        for s in self.building_spawns(lb, index) {
            let c = crate::cells::carried(&s.frame, &from, &to);
            ops.push(Op::MoveSpawn {
                file: s.file,
                guid: s.guid,
                origin: c.origin,
                heading: dereth_primitives::frame::get_heading(&Frame::new(Vec3::ZERO, c.rotation)),
            });
        }
        Some(Op::Batch(ops))
    }

    /// The spawns that move with building `index` of landblock `lb`: those in its cells, and
    /// those outdoors over its footprint (its cells' extent and two meters more), as
    /// [`Self::spawns`] lists them.
    #[must_use]
    pub fn building_spawns(&self, lb: u16, index: usize) -> Vec<Spawn> {
        let Some(info) = self.statics(lb) else {
            return Vec::new();
        };
        let Some(b) = info.buildings.get(index) else {
            return Vec::new();
        };
        let cells = self.cells(lb);
        let doorways: Vec<u16> = b.portals.iter().map(|p| p.other_cell_id).collect();
        let inside = crate::cells::building_cells(&doorways, &cells);
        let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
        for c in &inside {
            let Some(cell) = cells.get(c) else { continue };
            let Some(e) = self.environment(cell.environment) else {
                continue;
            };
            let Some(cs) = e.cells.get(usize::from(cell.cell_struct)) else {
                continue;
            };
            for v in &cs.vertex_array.vertices {
                let p = dereth_primitives::frame::localtoglobal(&cell.frame, v.position);
                lo = [lo[0].min(p.x), lo[1].min(p.y)];
                hi = [hi[0].max(p.x), hi[1].max(p.y)];
            }
        }
        let over = |s: &Spawn| {
            let o = s.frame.origin;
            o.x >= lo[0] - 2.0 && o.x <= hi[0] + 2.0 && o.y >= lo[1] - 2.0 && o.y <= hi[1] + 2.0
        };
        self.spawns()
            .into_iter()
            .filter(|s| s.landblock() == lb)
            .filter(|s| {
                #[allow(clippy::cast_possible_truncation)]
                let cell = (s.cell & 0xFFFF) as u16;
                if cell >= crate::cells::FIRST_CELL {
                    inside.contains(&cell)
                } else {
                    over(s)
                }
            })
            .collect()
    }

    /// Where each static of landblock `lb`'s compiled statics record came from, in its order.
    #[must_use]
    pub fn origins(&self, lb: u16) -> Vec<StaticOrigin> {
        if let Some(o) = self.compiled.statics.get(&lb) {
            return o.clone();
        }
        let n = self.statics(lb).map_or(0, |i| i.objects.len());
        (0..n).map(StaticOrigin::Base).collect()
    }

    /// The ground's height at landblock `lb`'s meters `(x, y)`, on the compiled terrain.
    #[must_use]
    pub fn ground(&self, lb: u16, x: f32, y: f32) -> Option<f32> {
        let t = self.terrain(lb)?;
        Some(compile::ground(&t, self.region.as_ref()?, lb, x, y))
    }

    /// World vertex `(gx, gy)` as the compiled world holds it.
    #[must_use]
    pub fn vertex(&self, gx: u32, gy: u32) -> Option<Vertex> {
        let (lb, x, y) = vertex_owner(gx, gy);
        let t = self.terrain(lb)?;
        let (x, y) = (usize::from(x), usize::from(y));
        let height = t.height[x * 9 + y];
        let z = self.region.as_ref().map_or(0.0, |r| {
            dereth_terrain::land::mesh::height_table(r)[usize::from(height)]
        });
        Some(Vertex {
            height,
            z,
            terrain: t.terrain_type(x, y),
            scene: t.scene_type(x, y),
            road: t.road(x, y),
        })
    }

    /// Every spawn the packs' SQL places, with the setup each is drawn with when the session
    /// knows it.
    #[must_use]
    pub fn spawns(&self) -> Vec<Spawn> {
        let mut setups: BTreeMap<String, u32> = BTreeMap::new();
        for p in &self.packs {
            for f in &p.sql {
                for (w, s) in sql_ids::weenie_setups(&f.text) {
                    if let Some(sym) = token_symbol(&p.name, &w) {
                        setups.insert(sym, s);
                    }
                }
            }
        }
        let classes = self.pack_classes();
        let class_setup = |c: u32| {
            classes
                .get(&c)
                .or_else(|| self.retail.get(&c))
                .and_then(|f| f.setup)
        };
        let mut out = Vec::new();
        for (pi, p) in self.packs.iter().enumerate() {
            for f in &p.sql {
                for i in sql_ids::instances(&f.text) {
                    let setup = match token_symbol(&p.name, &i.weenie) {
                        Some(sym) => setups.get(&sym).copied(),
                        None => sql_ids::number(&i.weenie).and_then(class_setup),
                    };
                    let [w, x, y, z] = i.angles;
                    // A pack's cell, named by its symbol: the number the lockfile gave it.
                    let cell = i
                        .cell_symbol
                        .as_deref()
                        .and_then(|s| symbol::qualify(&p.name, s).ok())
                        .and_then(|s| {
                            let lb = u16::try_from(i.cell >> 16).ok()?;
                            self.compiled
                                .lock
                                .get(symbol::Kind::Cell, &symbol::canonical_instance(&s, lb))
                        })
                        .unwrap_or(i.cell);
                    out.push(Spawn {
                        pack: pi,
                        file: f.path.clone(),
                        guid: i.guid,
                        weenie: i.weenie,
                        cell,
                        frame: Frame::new(
                            Vec3::new(i.origin[0], i.origin[1], i.origin[2]),
                            Quat::new(w, x, y, z),
                        ),
                        setup: setup.map(DataId),
                        line: i.line,
                        server: false,
                    });
                }
            }
        }
        // The server's own in the landblocks shown, but those a pack writes again or takes away.
        let mut taken: BTreeSet<u32> = BTreeSet::new();
        for p in &self.packs {
            for f in &p.sql {
                taken.extend(
                    sql_ids::instances(&f.text)
                        .iter()
                        .filter_map(|i| sql_ids::number(&i.guid)),
                );
                taken.extend(
                    sql_ids::deleted_instances(&f.text)
                        .iter()
                        .filter_map(|g| sql_ids::number(g)),
                );
            }
        }
        for lb in &self.shown {
            for i in self.retail_instances.get(lb).into_iter().flatten() {
                let Some(g) = sql_ids::number(&i.guid).filter(|g| !taken.contains(g)) else {
                    continue;
                };
                let [w, x, y, z] = i.angles;
                out.push(Spawn {
                    pack: self.target,
                    file: self.override_file(*lb),
                    guid: guid_text(g),
                    weenie: i.weenie.clone(),
                    cell: i.cell,
                    frame: Frame::new(
                        Vec3::new(i.origin[0], i.origin[1], i.origin[2]),
                        Quat::new(w, x, y, z),
                    ),
                    setup: sql_ids::number(&i.weenie).and_then(class_setup).map(DataId),
                    line: 0,
                    server: true,
                });
            }
        }
        out
    }

    /// The landblocks a weenie of one of `classes` stands in: among the server's own spawns,
    /// and the packs' as compiled.
    #[must_use]
    pub fn landblocks_standing(&self, classes: &BTreeSet<u32>) -> BTreeSet<u16> {
        let of = |w: &str| sql_ids::number(w).is_some_and(|w| classes.contains(&w));
        let mut out: BTreeSet<u16> = self
            .retail_instances
            .iter()
            .filter(|(_, is)| is.iter().any(|i| of(&i.weenie)))
            .map(|(lb, _)| *lb)
            .collect();
        for text in self.compiled().sql.values() {
            out.extend(
                sql_ids::instances(text)
                    .iter()
                    .filter(|i| of(&i.weenie))
                    .filter_map(|i| u16::try_from(i.cell >> 16).ok()),
            );
        }
        out
    }

    /// The weenie spawn `s` places: a server class, or a pack's symbol.
    #[must_use]
    pub fn spawn_weenie(&self, s: &Spawn) -> Option<Weenie> {
        if let Some(c) = sql_ids::number(&s.weenie) {
            return Some(Weenie::Class(c));
        }
        let pack = self.packs.get(s.pack)?;
        token_symbol(&pack.name, &s.weenie).map(Weenie::Symbol)
    }

    /// What spawn `s` makes as a generator, when it is one: its weenie's generator radius and
    /// profiles (a pack's SQL of it), and the instances linked to it.
    pub fn generator(&mut self, s: &Spawn) -> Option<Generator> {
        let w = self.spawn_weenie(s)?;
        let mut g = Generator::default();
        if let Some((file, _)) = self.weenie_file(&w) {
            let text = self.text(&file)?;
            let doc = crate::content::SqlDoc::parse(&text);
            g.radius = doc
                .property(crate::content::PropertyKind::Float, 43)
                .and_then(|v| v.trim().parse().ok());
            for r in doc.rows("weenie_properties_generator") {
                let num = |c: &str| doc.cell(r, c).and_then(|v| v.trim().parse::<f32>().ok());
                let origin = match (num("origin_X"), num("origin_Y"), num("origin_Z")) {
                    (Some(x), Some(y), Some(z)) => Some(Vec3::new(x, y, z)),
                    _ => None,
                };
                #[allow(clippy::cast_possible_truncation)] // counts, as the SQL writes them
                g.profiles.push(Profile {
                    class: doc.cell(r, "weenie_Class_Id").unwrap_or_default(),
                    init: num("init_Create").map_or(1, |v| v as i32),
                    max: num("max_Create").map_or(1, |v| v as i32),
                    place: doc
                        .cell(r, "where_Create")
                        .and_then(|v| sql_ids::number(&v))
                        .unwrap_or(0),
                    cell: doc.cell(r, "obj_Cell_Id").and_then(|v| sql_ids::number(&v)),
                    origin,
                });
            }
        } else if let Weenie::Class(c) = &w {
            g.radius = self.retail.get(c).and_then(|f| f.generator_radius);
        }
        if let Some(guid) = sql_ids::number(&s.guid) {
            let mut kids: Vec<u32> = self
                .retail_links
                .iter()
                .filter(|(p, _)| *p == guid)
                .map(|(_, c)| *c)
                .collect();
            kids.sort_unstable();
            kids.dedup();
            g.children = kids.into_iter().map(guid_text).collect();
        }
        (g.radius.is_some() || !g.profiles.is_empty() || !g.children.is_empty()).then_some(g)
    }

    /// The target pack's file the changes to the server's own spawns of landblock `lb` are
    /// written in.
    #[must_use]
    pub fn override_file(&self, lb: u16) -> PathBuf {
        self.pack_dir()
            .join("server")
            .join("sql")
            .join("landblocks")
            .join(format!("{lb:04X} overrides.sql"))
    }

    /// The landblock whose server spawns `file` changes, when it is such a file.
    #[must_use]
    pub fn override_landblock(&self, file: &Path) -> Option<u16> {
        let name = file.file_name()?.to_str()?;
        let lb = u16::from_str_radix(name.strip_suffix(" overrides.sql")?, 16).ok()?;
        (self.override_file(lb) == file).then_some(lb)
    }

    /// Show the server's own spawns of landblocks `lbs` too: drawn, and among
    /// [`Self::spawns`]. The landblocks newly shown that have some, to draw again.
    pub fn show_server_spawns(&mut self, lbs: impl IntoIterator<Item = u16>) -> BTreeSet<u16> {
        let mut out = BTreeSet::new();
        for lb in lbs {
            if self.shown.insert(lb) && self.retail_instances.contains_key(&lb) {
                out.insert(lb);
            }
        }
        out
    }

    /// `text`, the override file of landblock `lb`, with the server's own spawn `guid` written
    /// in it whole as the server has it (its generator's links too), unless the file has it.
    fn overridden(&self, lb: u16, text: &str, guid: u32) -> Result<String, Error> {
        let mut o = Overrides::read(text);
        if let std::collections::btree_map::Entry::Vacant(e) = o.entries.entry(guid) {
            let i = self
                .retail_instances
                .get(&lb)
                .and_then(|l| l.iter().find(|i| sql_ids::number(&i.guid) == Some(guid)))
                .ok_or_else(|| {
                    Error::Refused(format!(
                        "the server places no spawn {} in {lb:04X}",
                        guid_text(guid)
                    ))
                })?;
            let children = self
                .retail_links
                .iter()
                .filter(|(p, _)| *p == guid)
                .map(|(_, c)| *c)
                .collect();
            e.insert(Override::Placed(i.clone(), children));
        }
        Ok(o.render(lb, &self.today, &|w| self.weenie_name(w)))
    }

    /// The name the SQL gives weenie `w` (a class id or a symbol's token), when it gives one.
    fn weenie_name(&self, w: &str) -> Option<String> {
        if let Some(c) = sql_ids::number(w) {
            return self.retail.get(&c).and_then(|f| f.name.clone());
        }
        None
    }

    // ---------------------------------------------------------------------------------------
    // sources
    // ---------------------------------------------------------------------------------------

    /// The text of `path` as the session holds it.
    fn text(&mut self, path: &Path) -> Option<String> {
        if let Some(t) = self.texts.get(path) {
            return t.clone();
        }
        let t = std::fs::read_to_string(path).ok();
        self.texts.insert(path.to_path_buf(), t.clone());
        self.disk.insert(path.to_path_buf(), t.clone());
        t
    }

    fn pack_dir(&self) -> &Path {
        &self.packs[self.target].dir
    }

    /// The target pack's file for landblock `lb`'s operations: the one it has, else a new one.
    fn edits_file(&self, lb: u16) -> PathBuf {
        self.packs[self.target]
            .edits
            .iter()
            .find(|e| e.landblock == lb)
            .map_or_else(
                || {
                    self.pack_dir()
                        .join("edits")
                        .join("landblocks")
                        .join(format!("{lb:04X}.toml"))
                },
                |e| e.file.clone(),
            )
    }

    /// Put `texts` in place of the files' texts, and read them into the target pack again.
    fn set_texts(&mut self, texts: &BTreeMap<PathBuf, Option<String>>) -> Result<(), Error> {
        let sql_dir = self.pack_dir().join("server").join("sql");
        for (path, text) in texts {
            if !self.disk.contains_key(path) {
                self.disk
                    .insert(path.clone(), std::fs::read_to_string(path).ok());
            }
            self.texts.insert(path.clone(), text.clone());
            if let Ok(rel) = path.strip_prefix(&sql_dir) {
                let rel = rel.to_path_buf();
                let sql = &mut self.packs[self.target].sql;
                sql.retain(|f| f.path != *path);
                if let Some(text) = text {
                    sql.push(SqlFile {
                        rel,
                        path: path.clone(),
                        text: text.clone(),
                    });
                    sql.sort_by_key(|f| {
                        f.rel
                            .components()
                            .map(|c| c.as_os_str().to_string_lossy().into_owned())
                            .collect::<Vec<_>>()
                    });
                }
            } else {
                let parsed = match text {
                    Some(text) => Some(source::parse_edits(&self.packs[self.target], path, text)?),
                    None => None,
                };
                let lb = source::landblock_of_file(path)?;
                let edits = &mut self.packs[self.target].edits;
                edits.retain(|e| e.landblock != lb);
                if let Some(e) = parsed {
                    edits.push(e);
                    edits.sort_by_key(|e| e.landblock);
                }
            }
        }
        Ok(())
    }

    /// The landblocks a change of the vertex sets from `before` to `after` reaches.
    fn vertex_change(before: &VertexSets, after: &VertexSets) -> BTreeSet<u16> {
        let mut out = BTreeSet::new();
        let keys: BTreeSet<&(u32, u32, Field)> = before.keys().chain(after.keys()).collect();
        for k in keys {
            if before.get(k).map(|v| v.0) != after.get(k).map(|v| v.0) {
                out.extend(copies(k.0, k.1).into_iter().map(|(lb, _, _)| lb));
            }
        }
        out
    }

    /// Change the files to `texts` and compile what that reaches; on any error, put them back.
    fn change(&mut self, texts: &BTreeMap<PathBuf, Option<String>>) -> Result<Touched, Error> {
        let before: BTreeMap<PathBuf, Option<String>> =
            texts.keys().map(|p| (p.clone(), self.text(p))).collect();
        let vertices_before = gather_vertices(&self.packs, &self.compiled.overrides).0;
        let compiled_before = self.compiled.clone();
        let result = (|| {
            self.set_texts(texts)?;
            let vertices_after = gather_vertices(&self.packs, &self.compiled.overrides).0;
            let mut touched = Touched {
                landblocks: Self::vertex_change(&vertices_before, &vertices_after),
                sql: false,
            };
            let sql_dir = self.pack_dir().join("server").join("sql");
            for path in texts.keys() {
                if path.starts_with(&sql_dir) {
                    touched.sql = true;
                } else {
                    touched.landblocks.insert(source::landblock_of_file(path)?);
                }
            }
            if touched.sql {
                // A spawn's preview stands in its landblock: before and after.
                for f in texts.keys().filter(|p| p.starts_with(&sql_dir)) {
                    for t in [before.get(f).cloned().flatten(), texts[f].clone()]
                        .into_iter()
                        .flatten()
                    {
                        for i in sql_ids::instances(&t) {
                            if let Ok(lb) = u16::try_from(i.cell >> 16) {
                                touched.landblocks.insert(lb);
                            }
                        }
                    }
                }
            }
            let cells_before = self.compiled.cells.clone();
            compile::recompile_landblocks(
                &self.packs,
                &self.base,
                &mut self.compiled,
                &touched.landblocks,
            )?;
            // Cells come or gone: the SQL that names them is filled again.
            if touched.sql || self.compiled.cells != cells_before {
                compile::refresh_sql(
                    &self.world,
                    &self.packs,
                    &self.base,
                    &self.opened_lock,
                    &mut self.compiled,
                )?;
            }
            // A change that sets something another source sets otherwise, with no word from the
            // world on which wins, is refused; those the world had already stand.
            let before = compiled_before.conflicts();
            let new: Vec<crate::conflict::Conflict> = self
                .compiled
                .conflicts()
                .into_iter()
                .filter(|c| !before.iter().any(|b| b.same_as(c)))
                .cloned()
                .collect();
            if !new.is_empty() {
                return Err(Error::Conflicts(new));
            }
            Ok(touched)
        })();
        if result.is_err() {
            let _ = self.set_texts(&before);
            self.compiled = compiled_before;
        }
        result
    }

    /// Apply `op` as one step of the history.
    ///
    /// # Errors
    /// An operation the sources cannot take (a static that is not there, an unnamed static of
    /// another pack), or a compile error it would cause; nothing changes then.
    pub fn apply(&mut self, op: &Op) -> Result<Applied, Error> {
        let start = self.undo.len();
        let applied = self.apply_steps(op)?;
        self.merge_since(start);
        self.redo.clear();
        Ok(applied)
    }

    /// Apply `op` into the history's last step, so one undo takes both back: the steps of one
    /// stroke of a brush or one drag of a gizmo.
    ///
    /// # Errors
    /// As [`Self::apply`].
    pub fn apply_into_last(&mut self, op: &Op) -> Result<Applied, Error> {
        let start = self.undo.len().saturating_sub(1);
        let applied = self.apply_steps(op)?;
        self.merge_since(start);
        self.redo.clear();
        Ok(applied)
    }

    /// Merge the history's steps from `start` on into one.
    fn merge_since(&mut self, start: usize) {
        if self.undo.len() <= start + 1 {
            return;
        }
        let steps: Vec<Step> = self.undo.drain(start..).collect();
        let mut merged = Step {
            before: BTreeMap::new(),
            after: BTreeMap::new(),
        };
        for s in steps {
            for (p, t) in s.before {
                merged.before.entry(p).or_insert(t);
            }
            merged.after.extend(s.after);
        }
        self.undo.push(merged);
    }

    fn apply_steps(&mut self, op: &Op) -> Result<Applied, Error> {
        if let Op::Batch(ops) = op {
            let start = self.undo.len();
            let mut all = Applied::default();
            for o in ops {
                match self.apply_steps(o) {
                    Ok(a) => {
                        all.touched.add(a.touched);
                        all.placed = a.placed.or(all.placed);
                        all.spawned = a.spawned.or(all.spawned);
                    }
                    Err(e) => {
                        // Take back what the batch did so far.
                        while self.undo.len() > start {
                            let _ = self.undo_step();
                        }
                        return Err(e);
                    }
                }
            }
            return Ok(all);
        }
        let (texts, placed, spawned) = self.draft(op)?;
        let before: BTreeMap<PathBuf, Option<String>> =
            texts.keys().map(|p| (p.clone(), self.text(p))).collect();
        let touched = self.change(&texts)?;
        self.undo.push(Step {
            before,
            after: texts,
        });
        let placed = placed.map(|(lb, name)| {
            let origin = self
                .origins(lb)
                .into_iter()
                .rev()
                .find(|o| {
                    matches!(o, StaticOrigin::Pack { pack, name: n, .. }
                        if *pack == self.target && n.as_deref() == Some(name.as_str()))
                })
                .unwrap_or(StaticOrigin::Base(usize::MAX));
            (lb, origin)
        });
        Ok(Applied {
            touched,
            placed,
            spawned,
        })
    }

    fn undo_step(&mut self) -> Result<Touched, Error> {
        let Some(step) = self.undo.pop() else {
            return Ok(Touched::default());
        };
        match self.change(&step.before) {
            Ok(t) => {
                self.redo.push(step);
                Ok(t)
            }
            Err(e) => {
                self.undo.push(step);
                Err(e)
            }
        }
    }

    /// Take the last step back.
    ///
    /// # Errors
    /// A compile error, when a file changed under the session; the step stays.
    pub fn undo(&mut self) -> Result<Touched, Error> {
        self.undo_step()
    }

    /// Do the last step taken back again.
    ///
    /// # Errors
    /// As [`Self::undo`].
    pub fn redo(&mut self) -> Result<Touched, Error> {
        let Some(step) = self.redo.pop() else {
            return Ok(Touched::default());
        };
        match self.change(&step.after) {
            Ok(t) => {
                self.undo.push(step);
                Ok(t)
            }
            Err(e) => {
                self.redo.push(step);
                Err(e)
            }
        }
    }

    /// How many steps undo and redo can take.
    #[must_use]
    pub fn history(&self) -> (usize, usize) {
        (self.undo.len(), self.redo.len())
    }

    /// Whether any file differs from the disk's.
    #[must_use]
    pub fn unsaved(&self) -> bool {
        self.texts
            .iter()
            .any(|(p, t)| self.disk.get(p).is_some_and(|d| d != t))
    }

    /// Write every file of the target pack the session changed whose text differs from the
    /// disk's (and remove one the session made and took away again), and the lockfile when its
    /// ids changed. Answers the files written or removed.
    ///
    /// # Errors
    /// A file that will not write.
    pub fn save(&mut self) -> Result<Vec<PathBuf>, Error> {
        let mut written = Vec::new();
        for (path, text) in &self.texts {
            let disk = self.disk.get(path).cloned().flatten();
            if *text == disk {
                continue;
            }
            match text {
                Some(t) => {
                    if let Some(parent) = path.parent() {
                        std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
                    }
                    std::fs::write(path, t).map_err(|e| Error::io(path, e))?;
                }
                None => std::fs::remove_file(path).map_err(|e| Error::io(path, e))?,
            }
            written.push(path.clone());
        }
        for path in &written {
            self.disk.insert(path.clone(), self.texts[path].clone());
        }
        let lock_path = self.world.lock_path();
        let lock = self.compiled.lock.render();
        if std::fs::read_to_string(&lock_path).ok().as_deref() != Some(lock.as_str())
            && (lock_path.is_file() || !self.compiled.lock.is_empty())
        {
            std::fs::write(&lock_path, &lock).map_err(|e| Error::io(&lock_path, e))?;
            written.push(lock_path);
        }
        Ok(written)
    }

    // ---------------------------------------------------------------------------------------
    // operations as source changes
    // ---------------------------------------------------------------------------------------

    fn doc(
        &mut self,
        path: &Path,
        drafts: &BTreeMap<PathBuf, Option<String>>,
    ) -> Result<DocumentMut, Error> {
        let text = match drafts.get(path) {
            Some(t) => t.clone(),
            None => self.text(path),
        };
        text.unwrap_or_default()
            .parse::<DocumentMut>()
            .map_err(|e| Error::source(&Loc::new(path, 0), e.to_string()))
    }

    /// The file changes `op` makes, the static it places (landblock and symbol), the spawn file
    /// it writes.
    #[allow(clippy::type_complexity)]
    fn draft(
        &mut self,
        op: &Op,
    ) -> Result<
        (
            BTreeMap<PathBuf, Option<String>>,
            Option<(u16, String)>,
            Option<PathBuf>,
        ),
        Error,
    > {
        let mut out: BTreeMap<PathBuf, Option<String>> = BTreeMap::new();
        let mut placed = None;
        let mut spawned = None;
        match op {
            Op::PlaceStatic {
                landblock,
                model,
                at,
                rotation,
            } => {
                let path = self.edits_file(*landblock);
                let mut doc = self.doc(&path, &out)?;
                let used: BTreeSet<String> = self.packs[self.target]
                    .edits
                    .iter()
                    .find(|e| e.landblock == *landblock)
                    .map(|e| {
                        e.statics
                            .iter()
                            .filter_map(|s| s.item.name.clone())
                            .collect()
                    })
                    .unwrap_or_default();
                let pack = self.packs[self.target].name.clone();
                let name = (1..)
                    .map(|n| format!("static-{n}"))
                    .find(|n| !used.contains(&format!("{pack}:{n}")))
                    .unwrap_or_default();
                let mut t = entry();
                t.insert("name", Item::Value(Value::from(name.clone())));
                t.insert("model", Item::Value(hex(model.raw())));
                t.insert("at", Item::Value(place_value(at)));
                set_rotation(&mut t, *rotation);
                push(&mut doc, "static", t);
                out.insert(path, Some(render(&doc)));
                placed = Some((*landblock, format!("{pack}:{name}")));
            }
            Op::MoveStatic {
                landblock,
                which,
                at,
                rotation,
            } => {
                let path = self.edits_file(*landblock);
                let mut doc = self.doc(&path, &out)?;
                self.move_static(&mut doc, *landblock, which, at.as_ref(), *rotation)?;
                out.insert(path, Some(render(&doc)));
            }
            Op::RemoveStatic { landblock, which } => {
                let path = self.edits_file(*landblock);
                let mut doc = self.doc(&path, &out)?;
                self.remove_static(&mut doc, *landblock, which)?;
                out.insert(path, Some(render(&doc)));
            }
            Op::SetVertices(sets) => {
                for s in sets {
                    self.set_vertex(&mut out, *s)?;
                }
            }
            Op::PlaceSpawn {
                landblock,
                weenie,
                origin,
                heading,
            } => {
                let (path, text) = self.spawn_sql(*landblock, None, weenie, *origin, *heading);
                out.insert(path.clone(), Some(text));
                spawned = Some(path);
            }
            Op::PlaceSpawnInCell {
                landblock,
                cell,
                weenie,
                origin,
                heading,
            } => {
                let (path, text) =
                    self.spawn_sql(*landblock, Some(*cell), weenie, *origin, *heading);
                out.insert(path.clone(), Some(text));
                spawned = Some(path);
            }
            Op::PlaceCellStatic {
                landblock,
                cell,
                model,
                at,
                rotation,
            } => {
                let path = self.edits_file(*landblock);
                let mut doc = self.doc(&path, &out)?;
                let pack = self.packs[self.target].name.clone();
                let bare = cell
                    .strip_prefix(&format!("{pack}:"))
                    .unwrap_or(cell)
                    .to_owned();
                let Some(ct) = tables(&mut doc, "cell")
                    .iter_mut()
                    .find(|t| string(t, "name") == Some(bare.as_str()))
                else {
                    return Err(Error::Refused(format!(
                        "no cell {bare} of this pack stands in landblock {landblock:04X}"
                    )));
                };
                let mut t = Table::new();
                t.insert("model", Item::Value(hex(model.raw())));
                t.insert(
                    "at",
                    Item::Value(parse_value(&format!(
                        "[{}, {}, {}]",
                        meters(at[0]),
                        meters(at[1]),
                        meters(at[2])
                    ))),
                );
                set_rotation(&mut t, *rotation);
                let item = ct
                    .entry("static")
                    .or_insert(Item::ArrayOfTables(ArrayOfTables::new()));
                if let Some(aot) = item.as_array_of_tables_mut() {
                    aot.push(t);
                }
                out.insert(path, Some(render(&doc)));
            }
            Op::MoveCellStatic {
                landblock,
                cell,
                index,
                at,
                rotation,
            } => {
                let path = self.edits_file(*landblock);
                let mut doc = self.doc(&path, &out)?;
                self.cell_static(&mut doc, *landblock, *cell, *index, Some((*at, *rotation)))?;
                out.insert(path, Some(render(&doc)));
            }
            Op::RemoveCellStatic {
                landblock,
                cell,
                index,
            } => {
                let path = self.edits_file(*landblock);
                let mut doc = self.doc(&path, &out)?;
                self.cell_static(&mut doc, *landblock, *cell, *index, None)?;
                out.insert(path, Some(render(&doc)));
            }
            Op::MoveSpawn {
                file,
                guid,
                origin,
                heading,
            } => {
                let text = match self.override_landblock(file) {
                    // One of the server's own: written into the file whole first.
                    Some(lb) => {
                        let g = sql_ids::number(guid).ok_or_else(|| {
                            Error::Refused(format!("{guid} is not one of the server's spawns"))
                        })?;
                        let now = self.text(file).unwrap_or_default();
                        self.overridden(lb, &now, g)?
                    }
                    None => self.spawn_file(file)?,
                };
                out.insert(
                    file.clone(),
                    Some(Self::moved_spawn(file, &text, guid, *origin, *heading)?),
                );
            }
            Op::RemoveServerSpawn { file, guid } => {
                let lb = self.override_landblock(file).ok_or_else(|| {
                    Error::Refused(format!("{} is not a server spawn's file", file.display()))
                })?;
                let g = sql_ids::number(guid).ok_or_else(|| {
                    Error::Refused(format!("{guid} is not one of the server's spawns"))
                })?;
                let now = self.text(file).unwrap_or_default();
                let mut o = Overrides::read(&now);
                let link_child = self
                    .retail_instances
                    .get(&lb)
                    .and_then(|l| l.iter().find(|i| sql_ids::number(&i.guid) == Some(g)))
                    .is_some_and(|i| i.link_child);
                o.entries.insert(g, Override::Removed { link_child });
                out.insert(
                    file.clone(),
                    Some(o.render(lb, &self.today, &|w| self.weenie_name(w))),
                );
            }
            Op::RemoveSpawn { file } => {
                let text = self.spawn_file(file)?;
                let n = sql_ids::instances(&text).len();
                if n > 1 {
                    return Err(Error::Refused(format!(
                        "{} places {n} spawns; it is taken away only whole, by hand",
                        file.display()
                    )));
                }
                out.insert(file.clone(), None);
            }
            Op::WriteSql { file, text } => {
                let dir = self.pack_dir().join("server").join("sql");
                if !file.starts_with(&dir)
                    || file.extension().and_then(|e| e.to_str()) != Some("sql")
                {
                    return Err(Error::Refused(format!(
                        "{} is not an SQL file of the pack being edited",
                        file.display()
                    )));
                }
                if text.is_none() && self.text(file).is_none() {
                    return Err(Error::Refused(format!("{} is not there", file.display())));
                }
                out.insert(file.clone(), text.clone());
            }
            Op::PlaceCell {
                landblock,
                environment,
                piece,
                surfaces,
                at,
                rotation,
                links,
            } => {
                let path = self.edits_file(*landblock);
                let mut doc = self.doc(&path, &out)?;
                let pack = self.packs[self.target].name.clone();
                let used: BTreeSet<String> = tables(&mut doc, "cell")
                    .iter()
                    .filter_map(|t| string(t, "name").map(str::to_owned))
                    .collect();
                // A name no cell of the landblock has held: the lockfile keeps a cell taken
                // away, and a new cell does not take its number.
                let held = |n: &str| {
                    self.compiled
                        .lock
                        .get(
                            symbol::Kind::Cell,
                            &compile::cell_symbol(&format!("{pack}:{n}"), *landblock),
                        )
                        .is_some()
                };
                let name = (1..)
                    .map(|n| format!("cell-{n}"))
                    .find(|n| !used.contains(n) && !held(n))
                    .unwrap_or_default();
                let bare = |full: &str| {
                    full.strip_prefix(&format!("{pack}:"))
                        .unwrap_or(full)
                        .to_owned()
                };
                let mut t = entry();
                t.insert("name", Item::Value(Value::from(name.clone())));
                t.insert("environment", Item::Value(hex(environment.raw())));
                t.insert("piece", Item::Value(Value::from(i64::from(*piece))));
                let list = surfaces
                    .iter()
                    .map(|s| format!("0x{:08X}", s.raw()))
                    .collect::<Vec<_>>()
                    .join(", ");
                t.insert("surfaces", Item::Value(parse_value(&format!("[{list}]"))));
                t.insert(
                    "at",
                    Item::Value(parse_value(&format!(
                        "[{}, {}, {}]",
                        meters(at[0]),
                        meters(at[1]),
                        meters(at[2])
                    ))),
                );
                set_rotation(&mut t, *rotation);
                let written: Vec<String> = links.iter().map(|l| link_text(l, &bare)).collect();
                let mut arr = toml_edit::Array::new();
                for w in &written {
                    arr.push(w.as_str());
                }
                t.insert("portals", Item::Value(Value::Array(arr)));
                // The partners link back: a reshaped cell's in its row.
                for (slot, l) in links.iter().enumerate() {
                    if let source::Link::Base { cell, portal } = l {
                        let n = i64::from(*cell);
                        let Some(rt) = tables(&mut doc, "replace-cell")
                            .iter_mut()
                            .find(|t| int(t, "cell") == Some(n))
                        else {
                            return Err(Error::Refused(format!(
                                "this pack does not reshape cell {cell:04X} of landblock {landblock:04X}"
                            )));
                        };
                        let Some(v) = rt
                            .get_mut("portals")
                            .and_then(Item::as_array_mut)
                            .and_then(|p| p.get_mut(*portal))
                        else {
                            return Err(Error::Refused(format!(
                                "cell {cell:04X} has no portal {portal}"
                            )));
                        };
                        *v = Value::from(format!("{name}:{slot}"));
                        continue;
                    }
                    let source::Link::Cell { cell, portal } = l else {
                        continue;
                    };
                    let partner = bare(cell);
                    let Some(pt) = tables(&mut doc, "cell")
                        .iter_mut()
                        .find(|t| string(t, "name") == Some(partner.as_str()))
                    else {
                        return Err(Error::Refused(format!(
                            "no cell {partner} of this pack stands in landblock {landblock:04X}"
                        )));
                    };
                    let Some(ports) = pt.get_mut("portals").and_then(Item::as_array_mut) else {
                        return Err(Error::Refused(format!("cell {partner} has no portals")));
                    };
                    let Some(v) = ports.get_mut(*portal) else {
                        return Err(Error::Refused(format!(
                            "cell {partner} has no portal {portal}"
                        )));
                    };
                    *v = Value::from(format!("{name}:{slot}"));
                }
                push(&mut doc, "cell", t);
                out.insert(path, Some(render(&doc)));
                placed = Some((*landblock, format!("{pack}:{name}")));
            }
            Op::ReplaceCell {
                landblock,
                cell,
                environment,
                piece,
                surfaces,
                at,
                rotation,
                links,
            } => {
                let path = self.edits_file(*landblock);
                let mut doc = self.doc(&path, &out)?;
                let pack = self.packs[self.target].name.clone();
                let bare = |full: &str| {
                    full.strip_prefix(&format!("{pack}:"))
                        .unwrap_or(full)
                        .to_owned()
                };
                let n = i64::from(*cell);
                // Reshaped anew: the row before goes, and the new cells it joined are shut.
                let before = tables(&mut doc, "replace-cell")
                    .iter()
                    .position(|t| int(t, "cell") == Some(n));
                if let Some(k) = before {
                    remove_table(&mut doc, "replace-cell", k);
                }
                let here = format!("0x{cell:04X}:");
                if doc.contains_key("cell") {
                    for t in tables(&mut doc, "cell").iter_mut() {
                        if let Some(ports) = t.get_mut("portals").and_then(Item::as_array_mut) {
                            for v in ports.iter_mut() {
                                if v.as_str().is_some_and(|s| s.starts_with(&here)) {
                                    *v = Value::from("");
                                }
                            }
                        }
                    }
                }
                let mut t = entry();
                t.insert("cell", Item::Value(hex16(*cell)));
                t.insert("environment", Item::Value(hex(environment.raw())));
                t.insert("piece", Item::Value(Value::from(i64::from(*piece))));
                let list = surfaces
                    .iter()
                    .map(|s| format!("0x{:08X}", s.raw()))
                    .collect::<Vec<_>>()
                    .join(", ");
                t.insert("surfaces", Item::Value(parse_value(&format!("[{list}]"))));
                t.insert(
                    "at",
                    Item::Value(parse_value(&format!(
                        "[{}, {}, {}]",
                        meters(at[0]),
                        meters(at[1]),
                        meters(at[2])
                    ))),
                );
                set_rotation(&mut t, *rotation);
                let mut arr = toml_edit::Array::new();
                for l in links {
                    arr.push(link_text(l, &bare).as_str());
                }
                t.insert("portals", Item::Value(Value::Array(arr)));
                push(&mut doc, "replace-cell", t);
                out.insert(path, Some(render(&doc)));
            }
            Op::RemoveCell { landblock, name } => {
                let path = self.edits_file(*landblock);
                let mut doc = self.doc(&path, &out)?;
                let pack = self.packs[self.target].name.clone();
                let bare = name
                    .strip_prefix(&format!("{pack}:"))
                    .unwrap_or(name)
                    .to_owned();
                let k = tables(&mut doc, "cell")
                    .iter()
                    .position(|t| string(t, "name") == Some(bare.as_str()))
                    .ok_or_else(|| Error::Refused(format!("this pack places no cell {bare}")))?;
                remove_table(&mut doc, "cell", k);
                for key in ["cell", "replace-cell"] {
                    if !doc.contains_key(key) {
                        continue;
                    }
                    for t in tables(&mut doc, key).iter_mut() {
                        if let Some(ports) = t.get_mut("portals").and_then(Item::as_array_mut) {
                            for v in ports.iter_mut() {
                                let joined = v.as_str().is_some_and(|s| {
                                    s.rsplit_once(':').is_some_and(|(c, _)| c == bare)
                                });
                                if joined {
                                    *v = Value::from("");
                                }
                            }
                        }
                    }
                }
                out.insert(path, Some(render(&doc)));
            }
            Op::MoveBuilding {
                landblock,
                index,
                at,
                rotation,
            } => {
                let path = self.edits_file(*landblock);
                let mut doc = self.doc(&path, &out)?;
                let i = i64::try_from(*index).unwrap_or(i64::MAX);
                let aot = tables(&mut doc, "move-building");
                let existing = aot.iter().position(|t| int(t, "index") == Some(i));
                let at_value = parse_value(&format!(
                    "[{}, {}, {}]",
                    meters(at[0]),
                    meters(at[1]),
                    meters(at[2])
                ));
                match existing {
                    Some(k) => {
                        let t = aot.get_mut(k).expect("found above");
                        t.insert("at", Item::Value(at_value));
                        set_rotation(t, *rotation);
                    }
                    None => {
                        let mut t = entry();
                        t.insert("index", Item::Value(Value::from(i)));
                        t.insert("at", Item::Value(at_value));
                        set_rotation(&mut t, *rotation);
                        push(&mut doc, "move-building", t);
                    }
                }
                out.insert(path, Some(render(&doc)));
            }
            Op::Batch(_) => unreachable!("a batch is applied op by op"),
        }
        Ok((out, placed, spawned))
    }

    /// The `[[static]]` entry `entry` of the target pack's file, and its symbol.
    fn own_static(doc: &mut DocumentMut, entry: usize) -> Result<&mut Table, Error> {
        tables(doc, "static")
            .get_mut(entry)
            .ok_or_else(|| Error::Refused(format!("the file has no static {entry}")))
    }

    /// The `[[move-static]]` of `doc` that names the same static as `matches` says.
    fn find_move(doc: &DocumentMut, matches: &dyn Fn(&Table) -> bool) -> Option<usize> {
        doc.get("move-static")
            .and_then(Item::as_array_of_tables)
            .and_then(|a| a.iter().position(matches))
    }

    fn move_static(
        &self,
        doc: &mut DocumentMut,
        lb: u16,
        which: &StaticOrigin,
        at: Option<&Place>,
        rotation: Option<Rotation>,
    ) -> Result<(), Error> {
        let pack = self.packs[self.target].name.clone();
        let moved = |t: &mut Table| {
            if let Some(p) = at {
                t.insert("at", Item::Value(place_value(p)));
            }
            if let Some(r) = rotation {
                set_rotation(t, r);
            }
        };
        match which {
            StaticOrigin::Pack {
                pack: p,
                name,
                entry,
            } if *p == self.target => {
                // A move of it written in the same file comes after its entry: change that one.
                if let Some(n) = name {
                    let bare = n.split_once(':').map_or(n.as_str(), |(_, b)| b).to_owned();
                    let full = n.clone();
                    let names = move |t: &Table| {
                        string(t, "static").is_some_and(|s| s == bare || s == full)
                    };
                    if let Some(i) = Self::find_move(doc, &names) {
                        moved(tables(doc, "move-static").get_mut(i).expect("found"));
                        return Ok(());
                    }
                }
                moved(Self::own_static(doc, *entry)?);
            }
            StaticOrigin::Pack { name: Some(n), .. } => {
                let full = n.clone();
                let names = move |t: &Table| string(t, "static") == Some(full.as_str());
                if let Some(i) = Self::find_move(doc, &names) {
                    moved(tables(doc, "move-static").get_mut(i).expect("found"));
                } else {
                    let mut t = entry();
                    t.insert("static", Item::Value(Value::from(n.clone())));
                    moved(&mut t);
                    push(doc, "move-static", t);
                }
            }
            StaticOrigin::Pack { name: None, .. } => {
                return Err(Error::Refused(format!(
                    "the static is another pack's and has no name; only {pack} can move it"
                )));
            }
            StaticOrigin::Base(i) => {
                let index = i64::try_from(*i).unwrap_or(i64::MAX);
                let names = move |t: &Table| int(t, "index") == Some(index);
                if let Some(k) = Self::find_move(doc, &names) {
                    moved(tables(doc, "move-static").get_mut(k).expect("found"));
                } else {
                    let base_len = self.base_statics_len(lb);
                    if *i >= base_len {
                        return Err(Error::Refused(format!(
                            "landblock {lb:04X} has {base_len} statics of its own; there is no {i}"
                        )));
                    }
                    let mut t = entry();
                    t.insert("index", Item::Value(Value::from(index)));
                    moved(&mut t);
                    push(doc, "move-static", t);
                }
            }
        }
        Ok(())
    }

    /// Where static `index` of indoor cell `cell` of landblock `lb`, as compiled, came from: its
    /// place among the cell's statics as the cell's source holds them.
    #[must_use]
    pub fn cell_static_origin(&self, lb: u16, cell: u16, index: usize) -> usize {
        let id = (u32::from(lb) << 16) | u32::from(cell);
        self.compiled
            .cell_statics
            .get(&id)
            .and_then(|o| o.get(index).copied())
            .unwrap_or(index)
    }

    /// Move (`to`: where, and the turn when given) or take away (`None`) static `index` of indoor
    /// cell `cell` of landblock `lb`: in the target pack's own cell, its entry in the cell; in any
    /// other, a `move-cell-static` or a `remove-cell-static` of the landblock's file.
    fn cell_static(
        &self,
        doc: &mut DocumentMut,
        lb: u16,
        cell: u16,
        index: usize,
        to: Option<([f32; 3], Option<Rotation>)>,
    ) -> Result<(), Error> {
        let origin = self.cell_static_origin(lb, cell, index);
        let pack = self.packs[self.target].name.clone();
        let symbol = self.cell_names(lb).get(&cell).cloned();
        let at_value = |p: [f32; 3]| {
            parse_value(&format!(
                "[{}, {}, {}]",
                meters(p[0]),
                meters(p[1]),
                meters(p[2])
            ))
        };
        // The target pack's own cell: its static entry, changed or taken out.
        if let Some(bare) = symbol
            .as_deref()
            .and_then(|s| s.strip_prefix(&format!("{pack}:")))
        {
            let Some(ct) = tables(doc, "cell")
                .iter_mut()
                .find(|t| string(t, "name") == Some(bare))
            else {
                return Err(Error::Refused(format!(
                    "no cell {bare} of this pack stands in landblock {lb:04X}"
                )));
            };
            let Some(statics) = ct
                .get_mut("static")
                .and_then(Item::as_array_of_tables_mut)
                .filter(|a| origin < a.len())
            else {
                return Err(Error::Refused(format!(
                    "cell {bare} has no static {origin}"
                )));
            };
            match to {
                Some((p, rotation)) => {
                    let t = statics.get_mut(origin).expect("checked");
                    t.insert("at", Item::Value(at_value(p)));
                    if let Some(r) = rotation {
                        set_rotation(t, r);
                    }
                }
                None => {
                    statics.remove(origin);
                }
            }
            return Ok(());
        }
        // Any other cell: by its number, or another pack's by its symbol.
        let key: Value = match &symbol {
            Some(s) => Value::from(s.clone()),
            None => hex16(cell),
        };
        let index_value = i64::try_from(origin).unwrap_or(i64::MAX);
        let same = |t: &Table| {
            int(t, "index") == Some(index_value)
                && match &symbol {
                    Some(s) => string(t, "cell") == Some(s.as_str()),
                    None => int(t, "cell") == Some(i64::from(cell)),
                }
        };
        match to {
            Some((p, rotation)) => {
                let found = tables(doc, "move-cell-static").iter().position(&same);
                if let Some(k) = found {
                    let t = tables(doc, "move-cell-static").get_mut(k).expect("found");
                    t.insert("at", Item::Value(at_value(p)));
                    if let Some(r) = rotation {
                        set_rotation(t, r);
                    }
                } else {
                    let mut t = entry();
                    t.insert("cell", Item::Value(key));
                    t.insert("index", Item::Value(Value::from(index_value)));
                    t.insert("at", Item::Value(at_value(p)));
                    if let Some(r) = rotation {
                        set_rotation(&mut t, r);
                    }
                    push(doc, "move-cell-static", t);
                }
            }
            None => {
                if let Some(a) = doc
                    .get_mut("move-cell-static")
                    .and_then(Item::as_array_of_tables_mut)
                {
                    a.retain(|t| !same(t));
                }
                let model = self
                    .cells(lb)
                    .get(&cell)
                    .and_then(|c| c.static_objects.get(index))
                    .map(|o| o.id.raw());
                let mut t = entry();
                t.insert("cell", Item::Value(key));
                t.insert("index", Item::Value(Value::from(index_value)));
                if let Some(m) = model {
                    t.insert("model", Item::Value(hex(m)));
                }
                push(doc, "remove-cell-static", t);
            }
        }
        Ok(())
    }

    /// How many statics landblock `lb` holds under the packs (the imports over the base).
    fn base_statics_len(&self, lb: u16) -> usize {
        let id = (u32::from(lb) << 16) | 0xFFFE;
        let under = Compiled {
            imported: self.compiled.imported.clone(),
            records: self.compiled.imported.clone(),
            tombstones: self.compiled.tombstones.clone(),
            ..Compiled::default()
        };
        read_compiled(&under, &self.base, ModernDat::Cell, id)
            .and_then(|b| LandblockInfo::decode_payload(DataId(id), &b).ok())
            .map_or(0, |i| i.objects.len())
    }

    fn remove_static(
        &self,
        doc: &mut DocumentMut,
        lb: u16,
        which: &StaticOrigin,
    ) -> Result<(), Error> {
        let drop_moves = |doc: &mut DocumentMut, names: &dyn Fn(&Table) -> bool| {
            if let Some(a) = doc
                .get_mut("move-static")
                .and_then(Item::as_array_of_tables_mut)
            {
                a.retain(|t| !names(t));
            }
        };
        match which {
            StaticOrigin::Pack {
                pack: p,
                name,
                entry,
            } if *p == self.target => {
                if let Some(n) = name {
                    let bare = n.split_once(':').map_or(n.as_str(), |(_, b)| b).to_owned();
                    let full = n.clone();
                    drop_moves(doc, &|t: &Table| {
                        string(t, "static").is_some_and(|s| s == bare || s == full)
                    });
                }
                if *entry >= tables(doc, "static").len() {
                    return Err(Error::Refused(format!("the file has no static {entry}")));
                }
                remove_table(doc, "static", *entry);
            }
            StaticOrigin::Pack { name: Some(n), .. } => {
                let full = n.clone();
                drop_moves(doc, &|t: &Table| string(t, "static") == Some(full.as_str()));
                let mut t = entry();
                t.insert("static", Item::Value(Value::from(n.clone())));
                push(doc, "remove-static", t);
            }
            StaticOrigin::Pack { name: None, .. } => {
                return Err(Error::Refused(
                    "the static is another pack's and has no name".into(),
                ));
            }
            StaticOrigin::Base(i) => {
                let index = i64::try_from(*i).unwrap_or(i64::MAX);
                let base_len = self.base_statics_len(lb);
                if *i >= base_len {
                    return Err(Error::Refused(format!(
                        "landblock {lb:04X} has {base_len} statics of its own; there is no {i}"
                    )));
                }
                drop_moves(doc, &|t: &Table| int(t, "index") == Some(index));
                let model = self.base_static_model(lb, *i);
                let mut t = entry();
                t.insert("index", Item::Value(Value::from(index)));
                if let Some(m) = model {
                    t.insert("model", Item::Value(hex(m)));
                }
                push(doc, "remove-static", t);
            }
        }
        Ok(())
    }

    fn base_static_model(&self, lb: u16, i: usize) -> Option<u32> {
        let id = (u32::from(lb) << 16) | 0xFFFE;
        let under = Compiled {
            imported: self.compiled.imported.clone(),
            records: self.compiled.imported.clone(),
            tombstones: self.compiled.tombstones.clone(),
            ..Compiled::default()
        };
        let info = LandblockInfo::decode_payload(
            DataId(id),
            &read_compiled(&under, &self.base, ModernDat::Cell, id)?,
        )
        .ok()?;
        info.objects.get(i).map(|o| o.id.raw())
    }

    /// Set one field of one world vertex in the target pack's files: in the file that already
    /// sets it, else in the vertex's owning landblock's.
    fn set_vertex(
        &mut self,
        out: &mut BTreeMap<PathBuf, Option<String>>,
        s: VertexSet,
    ) -> Result<(), Error> {
        let key = match s.field {
            Field::Height => "height",
            Field::Terrain => "terrain",
            Field::Scene => "scene",
            Field::Road => "road",
        };
        // Every entry of the target pack's files that sets this field of this vertex: each is
        // changed, so the files never come to disagree.
        let mut found: Vec<(PathBuf, usize, (u8, u8), bool)> = Vec::new();
        let files: Vec<PathBuf> = self.packs[self.target]
            .edits
            .iter()
            .map(|e| e.file.clone())
            .chain(
                out.keys()
                    .filter(|p| source::landblock_of_file(p).is_ok())
                    .cloned(),
            )
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        for path in &files {
            let lb = source::landblock_of_file(path)?;
            let (bx, by) = (u32::from(lb >> 8), u32::from(lb & 0xFF));
            let doc = self.doc(path, out)?;
            let Some(vs) = doc.get("vertex").and_then(Item::as_array_of_tables) else {
                continue;
            };
            for (k, t) in vs.iter().enumerate() {
                let xy = |a: Option<&toml_edit::Array>| -> Option<(u32, u32)> {
                    let a = a?;
                    let x = a.get(0)?.as_integer()?;
                    let y = a.get(1)?.as_integer()?;
                    Some((u32::try_from(x).ok()?, u32::try_from(y).ok()?))
                };
                let Some(from) = xy(t.get("at").and_then(Item::as_array)) else {
                    continue;
                };
                let till = xy(t.get("to").and_then(Item::as_array)).unwrap_or(from);
                let covers = (bx * 8 + from.0..=bx * 8 + till.0).contains(&s.gx)
                    && (by * 8 + from.1..=by * 8 + till.1).contains(&s.gy);
                if covers && t.contains_key(key) {
                    let local = (
                        u8::try_from(s.gx - bx * 8).unwrap_or(0),
                        u8::try_from(s.gy - by * 8).unwrap_or(0),
                    );
                    found.push((path.clone(), k, local, till == from));
                }
            }
        }
        let value = Item::Value(Value::from(i64::from(s.value)));
        if !found.is_empty() {
            // Within a rectangle, a later entry of the same file wins for its one vertex.
            let mut appended: BTreeSet<PathBuf> = BTreeSet::new();
            for (path, k, local, single) in found {
                let mut doc = self.doc(&path, out)?;
                if single {
                    tables(&mut doc, "vertex")
                        .get_mut(k)
                        .expect("found")
                        .insert(key, value.clone());
                } else if appended.insert(path.clone()) {
                    let mut t = entry();
                    t.insert(
                        "at",
                        Item::Value(parse_value(&format!("[{}, {}]", local.0, local.1))),
                    );
                    t.insert(key, value.clone());
                    push(&mut doc, "vertex", t);
                }
                out.insert(path, Some(render(&doc)));
            }
            return Ok(());
        }
        let (lb, x, y) = vertex_owner(s.gx, s.gy);
        let path = self.edits_file(lb);
        let path = out
            .keys()
            .find(|p| source::landblock_of_file(p).ok() == Some(lb))
            .cloned()
            .unwrap_or(path);
        let mut doc = self.doc(&path, out)?;
        let single = doc
            .get("vertex")
            .and_then(Item::as_array_of_tables)
            .and_then(|vs| {
                vs.iter().position(|t| {
                    !t.contains_key("to")
                        && t.get("at").and_then(Item::as_array).is_some_and(|a| {
                            a.get(0).and_then(Value::as_integer) == Some(i64::from(x))
                                && a.get(1).and_then(Value::as_integer) == Some(i64::from(y))
                        })
                })
            });
        match single {
            Some(k) => {
                tables(&mut doc, "vertex")
                    .get_mut(k)
                    .expect("found")
                    .insert(key, value);
            }
            None => {
                let mut t = entry();
                t.insert("at", Item::Value(parse_value(&format!("[{x}, {y}]"))));
                t.insert(key, value);
                push(&mut doc, "vertex", t);
            }
        }
        out.insert(path, Some(render(&doc)));
        Ok(())
    }

    /// The text of `file`, a spawn file of the pack being edited.
    fn spawn_file(&mut self, file: &Path) -> Result<String, Error> {
        if !self.packs[self.target].sql.iter().any(|f| f.path == file) {
            return Err(Error::Refused(format!(
                "{} is not a spawn of the pack being edited",
                file.display()
            )));
        }
        self.text(file)
            .ok_or_else(|| Error::Refused(format!("{} is not there", file.display())))
    }

    /// `text` (the file `file`) with the instance `guid` moved to `origin` and turned to
    /// `heading`: its cell, origin and angles rewritten where they are written, everything else
    /// as it was.
    fn moved_spawn(
        file: &Path,
        text: &str,
        guid: &str,
        origin: Vec3,
        heading: f32,
    ) -> Result<String, Error> {
        let spans = sql_ids::instance_values(text, guid)
            .ok_or_else(|| Error::Refused(format!("{} places no spawn {guid}", file.display())))?;
        let cell_text = spans
            .get("obj_Cell_Id")
            .map(|r| text[r.clone()].to_owned())
            .ok_or_else(|| Error::Refused(format!("{}: {guid} has no cell", file.display())))?;
        // A pack's cell, named by its symbol, stays the spawn's cell.
        let named = sql_ids::cell_token(&cell_text).is_some();
        let cell = if named {
            0x0100
        } else {
            sql_ids::number(&cell_text)
                .ok_or_else(|| Error::Refused(format!("{}: {guid} has no cell", file.display())))?
        };
        if !(0.0..192.0).contains(&origin.x) || !(0.0..192.0).contains(&origin.y) {
            return Err(Error::Refused(format!(
                "a spawn moves within its landblock: {:.1}, {:.1} is outside it",
                origin.x, origin.y
            )));
        }
        let cell = if cell & 0xFFFF < 0x100 {
            (cell & 0xFFFF_0000) | outdoor_cell(origin.x, origin.y)
        } else {
            cell
        };
        let q = compile::heading_quat(heading);
        let f = |v: f32| trimmed(format!("{v:.6}"));
        let values = [
            (
                "obj_Cell_Id",
                if named {
                    cell_text
                } else {
                    format!("0x{cell:08X}")
                },
            ),
            ("origin_X", meters(origin.x)),
            ("origin_Y", meters(origin.y)),
            ("origin_Z", meters(origin.z)),
            ("angles_W", f(q.w)),
            ("angles_X", f(q.x)),
            ("angles_Y", f(q.y)),
            ("angles_Z", f(q.z)),
        ];
        let mut edits: Vec<(std::ops::Range<usize>, String)> = Vec::new();
        for (column, value) in values {
            let span = spans.get(column).ok_or_else(|| {
                Error::Refused(format!("{}: {guid} writes no {column}", file.display()))
            })?;
            edits.push((span.clone(), value));
        }
        edits.sort_by_key(|(r, _)| std::cmp::Reverse(r.start));
        let mut out = text.to_owned();
        for (r, v) in edits {
            out.replace_range(r, &v);
        }
        Ok(out)
    }

    /// A spawn's SQL file: its path in the target pack and its text.
    fn spawn_sql(
        &mut self,
        lb: u16,
        in_cell: Option<u16>,
        weenie: &Weenie,
        origin: Vec3,
        heading: f32,
    ) -> (PathBuf, String) {
        let pack = self.packs[self.target].name.clone();
        let stem = match weenie {
            Weenie::Symbol(s) => match s.split_once(':') {
                Some((p, n)) if p == pack => n.to_owned(),
                Some((p, n)) => format!("{p}-{n}"),
                None => s.clone(),
            },
            Weenie::Class(c) => format!("weenie-{c}"),
        };
        let token = match weenie {
            Weenie::Symbol(s) => match s.split_once(':') {
                Some((p, n)) if p == pack => format!("{{{{wcid:{n}}}}}"),
                _ => format!("{{{{wcid:{s}}}}}"),
            },
            Weenie::Class(c) => c.to_string(),
        };
        let dir = self
            .pack_dir()
            .join("server")
            .join("sql")
            .join("landblocks");
        let taken: BTreeSet<String> = self.packs[self.target]
            .sql
            .iter()
            .flat_map(|f| sql_ids::instances(&f.text))
            .map(|i| i.guid)
            .collect();
        let name = (1..)
            .map(|n| {
                if n == 1 {
                    stem.clone()
                } else {
                    format!("{stem}-{n}")
                }
            })
            .find(|n| {
                !dir.join(format!("{lb:04X} {n}.sql")).exists()
                    && !self
                        .texts
                        .contains_key(&dir.join(format!("{lb:04X} {n}.sql")))
                    && !taken.contains(&format!("{{{{guid:{n}@{lb:04X}}}}}"))
            })
            .unwrap_or_default();
        let guid = format!("{{{{guid:{name}@{lb:04X}}}}}");
        // A pack's new cell by its symbol, so it stays its cell whatever is numbered after;
        // a cell of the base, or the outdoors, by its id.
        let pack_cell = in_cell.and_then(|n| {
            self.compiled
                .cells
                .get(&lb)?
                .iter()
                .find(|(_, m)| **m == n)
                .map(|(s, _)| s.clone())
        });
        let cell = match pack_cell {
            Some(s) => {
                let bare = s
                    .strip_prefix(&format!("{pack}:"))
                    .map_or_else(|| s.clone(), str::to_owned);
                format!("{{{{cell:{bare}@{lb:04X}}}}}")
            }
            None => format!(
                "0x{:08X}",
                (u32::from(lb) << 16)
                    | in_cell.map_or_else(|| outdoor_cell(origin.x, origin.y), u32::from)
            ),
        };
        let q = compile::heading_quat(heading);
        let f = |v: f32| trimmed(format!("{v:.6}"));
        let text = format!(
            "/* {name}, placed in landblock {lb:04X}. One instance: no other instance of the \
             landblock is touched. */\n\
             DELETE FROM `landblock_instance` WHERE `guid` = {guid};\n\
             \n\
             INSERT INTO `landblock_instance` (`guid`, `weenie_Class_Id`, `obj_Cell_Id`, \
             `origin_X`, `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`, \
             `is_Link_Child`, `last_Modified`)\n\
             VALUES ({guid}, {token}, {cell}, {}, {}, {}, {}, {}, {}, {}, False, '{}');\n",
            meters(origin.x),
            meters(origin.y),
            meters(origin.z),
            f(q.w),
            f(q.x),
            f(q.y),
            f(q.z),
            self.today
        );
        (dir.join(format!("{lb:04X} {name}.sql")), text)
    }

    // ---------------------------------------------------------------------------------------
    // the scratch overlay and its checks
    // ---------------------------------------------------------------------------------------

    /// The overlay the editor draws: the compiled world, with every spawn whose setup the
    /// session knows stood where it will stand: outdoors in its landblock's statics record, in a
    /// building's or a dungeon's cell among that cell's statics, which the client draws with the
    /// cell. Never a build's: a build places no spawn in the files.
    #[must_use]
    pub fn scratch_records(&self) -> Records {
        self.scratch_records_in(None)
    }

    /// [`Self::scratch_records`] of the landblocks `only`: each one's terrain and statics records
    /// the scratch overlay holds. Every record when `None`.
    #[must_use]
    pub fn scratch_records_in(&self, only: Option<&BTreeSet<u16>>) -> Records {
        let mut records = match only {
            None => self.compiled.records.clone(),
            Some(blocks) => {
                // Each landblock's records: its two own and its cells'.
                let mut r = Records::new();
                if let Some(cells) = self.compiled.records.get(&ModernDat::Cell) {
                    for lb in blocks {
                        let first = u32::from(*lb) << 16;
                        for (id, s) in cells.range(first..=first | 0xFFFF) {
                            r.entry(ModernDat::Cell).or_default().insert(*id, s.clone());
                        }
                    }
                }
                r
            }
        };
        let mut by_block: BTreeMap<u16, Vec<ObjectEntry>> = BTreeMap::new();
        let mut by_cell: BTreeMap<u32, Vec<ObjectEntry>> = BTreeMap::new();
        for s in self.spawns() {
            if only.is_some_and(|o| !o.contains(&s.landblock())) {
                continue;
            }
            if let Some(setup) = s.setup {
                let o = ObjectEntry {
                    id: setup,
                    frame: s.frame,
                };
                if s.cell & 0xFFFF >= u32::from(crate::cells::FIRST_CELL) {
                    by_cell.entry(s.cell).or_default().push(o);
                } else {
                    by_block.entry(s.landblock()).or_default().push(o);
                }
            }
        }
        for (id, extra) in by_cell {
            let version = records
                .get(&ModernDat::Cell)
                .and_then(|r| r.get(&id))
                .map(|s| s.version)
                .or_else(|| {
                    self.base
                        .cell
                        .base_entry(DataId(id))
                        .map(|e| u16::try_from(e.bits >> 16).unwrap_or(1).max(1))
                });
            let Some((version, bytes)) = version.zip(self.cell_record(id)) else {
                continue;
            };
            let Ok(mut cell) = dereth_assets::world::EnvCell::decode_payload(DataId(id), &bytes)
            else {
                continue;
            };
            cell.static_objects.extend(extra);
            cell.flags |= 2;
            records.entry(ModernDat::Cell).or_default().insert(
                id,
                Staged {
                    bytes: cell.encode(),
                    version,
                },
            );
        }
        for (lb, extra) in by_block {
            let lbi = (u32::from(lb) << 16) | 0xFFFE;
            let (mut info, version, made) = match self.statics(lb) {
                Some(i) => {
                    let v = records
                        .get(&ModernDat::Cell)
                        .and_then(|r| r.get(&lbi))
                        .map_or_else(
                            || {
                                self.base
                                    .cell
                                    .base_entry(DataId(lbi))
                                    .map_or(NEW_RECORD_VERSION, |e| {
                                        u16::try_from(e.bits >> 16).unwrap_or(1).max(1)
                                    })
                            },
                            |s| s.version,
                        );
                    (i, v, false)
                }
                None => (
                    LandblockInfo {
                        id: DataId(lbi),
                        num_cells: 0,
                        objects: Vec::new(),
                        pack_mask: 0,
                        buildings: Vec::new(),
                        restrictions: None,
                    },
                    NEW_RECORD_VERSION,
                    true,
                ),
            };
            info.objects.extend(extra);
            let cells = records.entry(ModernDat::Cell).or_default();
            cells.insert(
                lbi,
                Staged {
                    bytes: info.encode(),
                    version,
                },
            );
            if made {
                if let Some(mut t) = self.terrain(lb) {
                    t.lbi_exists = 1;
                    let id = (u32::from(lb) << 16) | 0xFFFF;
                    let version = cells.get(&id).map_or_else(
                        || {
                            self.base
                                .cell
                                .base_entry(DataId(id))
                                .map_or(1, |e| u16::try_from(e.bits >> 16).unwrap_or(1).max(1))
                        },
                        |s| s.version,
                    );
                    cells.insert(
                        id,
                        Staged {
                            bytes: t.encode(),
                            version,
                        },
                    );
                }
            }
        }
        records
    }

    /// Write the scratch overlay ([`Self::scratch_records`]) into `dir`, made fresh.
    ///
    /// # Errors
    /// The writer's errors.
    pub fn write_scratch(&self, dir: &Path) -> Result<(), Error> {
        compile::write_records(
            &self.scratch_records(),
            &self.compiled.tombstones,
            &self.base,
            &self.world.key,
            dir,
        )
    }

    /// The checks over the compiled world, as a build runs them, with the overlay in `dir` (a
    /// scratch overlay this session wrote) laid over the files: each fault and warning, placed
    /// where it is when it names a landblock record or an SQL line.
    ///
    /// # Errors
    /// An overlay that will not open.
    pub fn check(&self, dir: &Path) -> Result<Vec<Fault>, Error> {
        let layered = self.base.with_overlay(dir, Some(&self.world.key))?;
        let mut report =
            crate::validate::validate(&layered, &crate::validate::Scope::of(&self.compiled));
        report.problems.extend(sql_ids::check(
            &layered,
            &compile::sql_named(&self.packs, &self.compiled),
        ));
        // The packs' server content: every class, quest and spell it names there, the server's
        // own classes and quests among them once the session has learned them.
        let learned = !self.retail.is_empty();
        let known = crate::content_check::Known {
            world_classes: learned.then(|| self.retail.keys().copied().collect()),
            quests: self
                .retail_quests
                .keys()
                .map(|n| crate::content_check::quest_key(n))
                .collect(),
            world_quests: learned,
            schema: self.schema.clone(),
            ..Default::default()
        };
        for x in crate::content_check::check_packs(&self.packs, &self.compiled, &self.base, &known)
        {
            let line = format!("{}: {}", x.at, x.text);
            if x.warning {
                report.warnings.push(line);
            } else {
                report.problems.push(line);
            }
        }
        // The buildings the packs moved.
        let mut moved: BTreeMap<u16, BTreeSet<usize>> = BTreeMap::new();
        for p in &self.packs {
            for e in &p.edits {
                for m in &e.buildings {
                    moved.entry(e.landblock).or_default().insert(m.item.index);
                }
            }
        }
        for (lb, which) in &moved {
            report
                .warnings
                .extend(crate::moves::check(&self.base, &layered, *lb, which));
        }
        // The walk over the packs' new cells.
        for lb in self.compiled.cells.keys() {
            for f in self.walk(*lb) {
                report.problems.push(format!(
                    "cell 0x{:08X}: {}",
                    (u32::from(*lb) << 16) | u32::from(f.cell),
                    f.text
                ));
            }
        }
        let spawns = self.spawns();
        let place = |text: &str| -> Option<(u16, Vec3)> {
            if let Some(rest) = text.strip_prefix("0x") {
                let id = u32::from_str_radix(rest.get(..8)?, 16).ok()?;
                let lb = u16::try_from(id >> 16).ok()?;
                if !matches!(id & 0xFFFF, 0xFFFE | 0xFFFF) {
                    return None;
                }
                let index = |word: &str| {
                    text.split_once(word)
                        .and_then(|(_, r)| r.split(|c: char| !c.is_ascii_digit()).next())
                        .and_then(|n| n.parse::<usize>().ok())
                };
                let at = index("static ")
                    .and_then(|i| self.statics(lb)?.objects.get(i).map(|o| o.frame.origin))
                    .or_else(|| {
                        index(": building ").and_then(|i| {
                            self.statics(lb)?.buildings.get(i).map(|b| b.frame.origin)
                        })
                    })
                    .unwrap_or(Vec3::new(96.0, 96.0, 0.0));
                return Some((lb, at));
            }
            spawns.iter().find_map(|s| {
                let prefix = format!("{}:{}:", s.file.display(), s.line);
                text.starts_with(&prefix)
                    .then(|| (s.landblock(), s.frame.origin))
            })
        };
        // The conflicts first: each on its line, on the ground where it is, or in the middle of
        // its landblock.
        let on_ground = |lb: u16, x: f32, y: f32| {
            let z = self
                .terrain(lb)
                .zip(self.region())
                .map_or(0.0, |(t, r)| compile::ground(&t, r, lb, x, y));
            (lb, Vec3::new(x, y, z))
        };
        let mut out: Vec<Fault> = self
            .compiled
            .conflicts()
            .into_iter()
            .map(|c| Fault {
                text: c.line(),
                warning: false,
                at: c
                    .place
                    .map(|(lb, [x, y])| on_ground(lb, x, y))
                    .or_else(|| c.landblocks.first().map(|lb| on_ground(*lb, 96.0, 96.0))),
            })
            .collect();
        for (texts, warning) in [(report.problems, false), (report.warnings, true)] {
            for text in texts {
                let at = place(&text);
                out.push(Fault { text, warning, at });
            }
        }
        Ok(out)
    }
}
