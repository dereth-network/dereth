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
//! - a spawn is one SQL file of its own, `server/sql/landblocks/XXYY <name>.sql`;
//! - a link between two spawns is written with its parent ([`links`]).
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
//!
//! The text, the history and the save are the shared edit session core
//! (`dereth_pack::session`); the operations, what a change compiles again and what it draws are
//! Lyceum's.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use dereth_assets::world::{CellLandblock, LandblockInfo, ObjectEntry};
use dereth_assets::Region;
use dereth_dat::ModernDat;
use dereth_pack::session::{History, Sources, Step};
use dereth_pack::store::DiskStore;
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

pub mod links;

/// A weenie a spawn places: a pack's, by its symbol in full, or a class id of the server's own.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Weenie {
    Symbol(String),
    Class(u32),
}

/// A kit piece placed whole ([`crate::kit`]): its environment, the surfaces its parts are drawn
/// with, where it stands and its turn, and each part (its place in the environment) with its
/// portals' links in order.
#[derive(Clone, Debug, PartialEq)]
pub struct NewPiece {
    pub environment: DataId,
    pub surfaces: Vec<DataId>,
    pub at: [f32; 3],
    pub rotation: Rotation,
    pub parts: Vec<(u16, Vec<source::Link>)>,
}

/// A portal of a part of the pieces one placement makes: the piece, the part (its place in that
/// piece's parts), the portal.
pub type PartPortal = (usize, usize, usize);

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
    /// Place `model` in `landblock` at `at`, turned `rotation`: named `name` (a bare symbol of
    /// the target pack's), or `static-1`, `static-2`, ... the first no static of the landblock's
    /// file holds.
    PlaceStatic {
        landblock: u16,
        model: DataId,
        at: Place,
        rotation: Rotation,
        name: Option<String>,
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
    /// Take the target pack's own setting of fields of world vertices away (where a row sets that
    /// one vertex): each then shows what the packs under it and the world set.
    UnsetVertices(Vec<(u32, u32, Field)>),
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
    /// Make the spawn `guid` of `file` (a lever, a pressure plate, a door, a creature) set off the
    /// spawn `target` (its file and guid: a door, a switch or a pressure plate), or nothing: the
    /// link the server reads, the target its parent, written where the target is written, the
    /// spawn marked its child where it is; the link it had before taken away ([`links`]).
    Link {
        file: PathBuf,
        guid: String,
        target: Option<(PathBuf, String)>,
    },
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
    /// Place kit pieces in `landblock`, each part a new cell of the target pack's, its portals
    /// linked as its parts give them (another new cell's, a reshaped cell's, or shut) and, where
    /// `joins` pairs two portals of the parts placed together, to each other.
    PlacePieces {
        landblock: u16,
        pieces: Vec<NewPiece>,
        joins: Vec<(PartPortal, PartPortal)>,
    },
    /// Take landblock `landblock`'s own cells `cells` away: each closed, the statics in them gone
    /// and the spawns standing in them taken away; every way into them shut, the cells beyond
    /// reshaped as they are so those ways are free doorways to build on.
    TakeCells { landblock: u16, cells: Vec<u16> },
    /// Draw cell `cell` of `landblock` with `surfaces`: a new cell of the target pack's in its
    /// row, any other reshaped as it stands with them.
    DressCell {
        landblock: u16,
        cell: u16,
        surfaces: Vec<DataId>,
    },
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
    /// Add a static of `model` to indoor cell `cell` of `landblock`, one not of the target
    /// pack's own (the world's, or another pack's), at `at` (landblock meters) turned
    /// `rotation`.
    AddCellStatic {
        landblock: u16,
        cell: u16,
        model: DataId,
        at: [f32; 3],
        rotation: Rotation,
    },
    /// Take the static standing at `index` of indoor cell `cell` of `landblock` away.
    RemoveCellStatic {
        landblock: u16,
        cell: u16,
        index: usize,
    },
    /// Move building `index` of `landblock` (its place in the compiled building list) to `at`,
    /// turned `rotation`, with every cell inside it. One of the world's own is moved by a
    /// `[[move-building]]` row; a dropped one by its `[[add-building]]` row, its set, doors,
    /// outdoor statics, the spawns and statics in its rooms and its level ground with it
    /// ([`crate::drops`]).
    MoveBuilding {
        landblock: u16,
        index: usize,
        at: [f32; 3],
        rotation: Rotation,
    },
    /// Move the pack's new cell `name` (its symbol, in full) of `landblock` to `at` (landblock
    /// meters) turned `rotation`, its portals joined anew by `links` (as [`Op::PlaceCell`]'s):
    /// the statics and the spawns standing in it go with it, and the portals it joined before
    /// are shut.
    MoveCell {
        landblock: u16,
        name: String,
        at: [f32; 3],
        rotation: Rotation,
        links: Vec<source::Link>,
    },
    /// Draw the pack's new cell `name` (its symbol, in full) of `landblock` with `surfaces`, by
    /// its piece's own indices.
    SetCellSurfaces {
        landblock: u16,
        name: String,
        surfaces: Vec<DataId>,
    },
    /// Name the dungeon of `landblock` and choose the look its pieces are drawn in (a dungeon's
    /// landblock whose look it shares); `None` leaves each unsaid.
    SetDungeon {
        landblock: u16,
        name: Option<String>,
        look: Option<u16>,
    },
    /// Add a copy of a retail building to `landblock`, with its rooms ([`crate::drops`]); or,
    /// for a name the target pack drops there already, write that row anew.
    AddBuilding {
        landblock: u16,
        building: source::AddBuilding,
    },
    /// Add a copy of a retail dungeon to `landblock`, every cell of it ([`crate::drops`]); or,
    /// for a name the target pack copies there already, write that row anew.
    AddDungeon {
        landblock: u16,
        dungeon: source::AddDungeon,
    },
    /// Take dropped building `index` of `landblock` (its place in the compiled building list)
    /// away: its row, so its set and rooms, its doors and outdoor statics, the spawns and statics
    /// in its rooms and in a house's basement, and the level ground it stood on
    /// ([`crate::drops`]).
    RemoveBuilding { landblock: u16, index: usize },
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
    pub(crate) fn add(&mut self, o: Self) {
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
    /// What it warns of, which stops nothing: a dropped building reaching further than retail's.
    pub warnings: Vec<String>,
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

/// One of the server's own spawns as its world places it: the instance, the instances it links as
/// a generator, and the generators that link it.
#[derive(Clone, Debug, PartialEq)]
pub struct ServerSpawn {
    pub instance: sql_ids::Instance,
    pub children: Vec<u32>,
    pub parents: Vec<u32>,
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

/// The server's own instances by the class each places: each one's landblock and its place in
/// that landblock's list; and those whose class is not a number.
#[derive(Debug, Default)]
struct RetailByClass {
    classes: BTreeMap<u32, Vec<(u16, usize)>>,
    others: Vec<(u16, usize)>,
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
    /// Every source file the session has read or changed: its text now, and on disk as last
    /// read or saved.
    sources: Sources,
    history: History,
    /// The date an SQL row is stamped with, `YYYY-MM-DD 00:00:00`.
    today: String,
    /// What the server's own weenies are, by class id, when the session was given them.
    retail: BTreeMap<u32, sql_ids::WeenieFacts>,
    /// The server's own instances by landblock, and its generators' links: each parent's
    /// children, in the order the server lists them.
    retail_instances: BTreeMap<u16, Vec<sql_ids::Instance>>,
    /// The server's own instances by the class each places (their landblock and place in its
    /// list), and those of no class number; kept as they are learned.
    retail_by_class: RetailByClass,
    retail_links: BTreeMap<u32, Vec<u32>>,
    /// The landblocks whose server spawns are shown.
    shown: BTreeSet<u16>,
    /// The server's quests and death treasures.
    retail_quests: BTreeMap<String, sql_ids::Quest>,
    retail_treasure: BTreeMap<u32, sql_ids::Treasure>,
    /// The server's wielded treasures, and who names each loot table among its own weenies.
    retail_wielded: BTreeMap<u32, Vec<sql_ids::WieldedItem>>,
    retail_loot_uses: BTreeMap<u32, Vec<(u32, sql_ids::LootUse)>>,
    /// The packs' loot tables, read from their files the first time they are wanted after a
    /// change.
    pack_loot: std::cell::OnceCell<crate::loot::PackLoot>,
    /// The file that writes each weenie, and whether it is the target pack's (the last pack's
    /// first file that writes it): read the first time it is wanted after a change.
    weenie_files: std::cell::OnceCell<std::collections::HashMap<Weenie, (PathBuf, bool)>>,
    /// Who names each of the server's classes among its own weenies, and each body part's
    /// typical row.
    retail_uses: BTreeMap<u32, Vec<(u32, sql_ids::Use)>>,
    retail_body_parts: BTreeMap<u32, Vec<(String, String)>>,
    /// Who names each quest among the server's own weenies, by the quest as registered.
    retail_quest_uses: BTreeMap<String, sql_ids::QuestNamers>,
    /// The server's names, which the checks of the packs' SQL read when given.
    schema: Option<crate::content::Schema>,
    /// How many times a landblock's cells were read, and how many of the server's own
    /// instances were read as spawns.
    cell_reads: std::cell::Cell<u64>,
    server_reads: std::cell::Cell<u64>,
    /// The world's buildings, when the session was given them: what a drop copies.
    buildings: Option<crate::buildings::Catalog>,
    /// Every link between spawns, gathered the first time it is wanted after a change.
    link_table: std::cell::OnceCell<links::Table>,
    /// What the packs' SQL says of the server's classes it writes again, and of its own weenies
    /// by symbol: read the first time it is wanted after a change.
    pack_class_facts: std::cell::OnceCell<BTreeMap<u32, sql_ids::WeenieFacts>>,
    pack_weenie_facts: std::cell::OnceCell<BTreeMap<String, sql_ids::WeenieFacts>>,
}

/// What a spawn makes as a generator: how far around it scatters what it makes, each profile
/// of its weenie, its timing and limits as its weenie's properties set them, and the instances
/// the world links to it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Generator {
    /// Float 43, `GeneratorRadius`.
    pub radius: Option<f32>,
    pub profiles: Vec<Profile>,
    /// Float 41, `RegenerationInterval`: how often, in seconds, it looks to make more.
    pub interval: Option<f32>,
    /// Float 121, `GeneratorInitialDelay`: how long after its landblock wakes it starts.
    pub initial_delay: Option<f32>,
    /// Int 81 and 82, `MaxGeneratedObjects` and `InitGeneratedObjects`.
    pub most: Option<i32>,
    pub first: Option<i32>,
    /// Int 142, `GeneratorTimeType`: 1 between its set times (Int 143 and 144), 3 during its
    /// event, 4 at night, 5 by day; unset, or any other value, always.
    pub time: Option<i32>,
    /// The guids (as [`Session::spawns`] writes them) of the instances it owns.
    pub children: Vec<String>,
}

/// One generator profile: what it makes, how many, where, when, and how likely.
#[derive(Clone, Debug, PartialEq)]
pub struct Profile {
    /// The class it makes, as written.
    pub class: String,
    pub init: i32,
    pub max: i32,
    /// Where it makes it (`where_Create`): bit 1 on the generator, bit 2 scattered around it,
    /// bit 4 at its own place, bit 8 inside it, bit 16 held by it, bit 32 for sale, bit 64 as
    /// treasure.
    pub place: u32,
    /// Its own place, when it has one: the cell and the meters of its landblock.
    pub cell: Option<u32>,
    pub origin: Option<Vec3>,
    /// When one is made again (`when_Create`): bit 1 once one is destroyed, bit 2 picked up,
    /// bit 4 killed.
    pub when: u32,
    /// How long after that, in seconds (`delay`).
    pub delay: Option<f32>,
    /// Its share of the draw, as the generator's running total writes it (`probability`), or
    /// `-1` for always.
    pub probability: Option<f32>,
}

impl Profile {
    /// Whether it makes treasure: its `class` is then a loot table's id, not a weenie's.
    #[must_use]
    pub const fn makes_treasure(&self) -> bool {
        self.place & sql_ids::TREASURE_PLACE != 0
    }
}

/// The generator weenie SQL `doc` describes: its properties and profiles (no children).
#[must_use]
pub fn generator_of(doc: &crate::content::SqlDoc) -> Generator {
    use crate::content::PropertyKind;
    let float = |id: u32| {
        doc.property(PropertyKind::Float, id)
            .and_then(|v| v.trim().parse::<f32>().ok())
    };
    let int = |id: u32| {
        doc.property(PropertyKind::Int, id)
            .and_then(|v| v.trim().parse::<i32>().ok())
    };
    let mut g = Generator {
        radius: float(43),
        interval: float(41),
        initial_delay: float(121),
        most: int(81),
        first: int(82),
        time: int(142),
        ..Generator::default()
    };
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
            when: doc
                .cell(r, "when_Create")
                .and_then(|v| sql_ids::number(&v))
                .unwrap_or(0),
            delay: num("delay"),
            probability: num("probability"),
        });
    }
    g
}

/// Everywhere a weenie is named ([`Session::named_anywhere`]): its spawns, the weenies that name
/// it and how, and the wielded loot tables that hold it, by id.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Named {
    pub spawns: Vec<Spawn>,
    pub by: Vec<(Weenie, sql_ids::Use)>,
    pub loot: Vec<u32>,
}

impl Named {
    /// Whether nothing names it.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.spawns.is_empty() && self.by.is_empty() && self.loot.is_empty()
    }

    /// How many places name it: each spawn, each weenie that names it (once however many ways),
    /// and each table.
    #[must_use]
    pub fn places(&self) -> usize {
        let by: std::collections::HashSet<&Weenie> = self.by.iter().map(|(w, _)| w).collect();
        self.spawns.len() + by.len() + self.loot.len()
    }
}

/// A portal standing in the world that leads into a dungeon: its spawn's guid, its landblock and
/// where it stands there (landblock meters).
#[derive(Clone, Debug, PartialEq)]
pub struct Entrance {
    pub guid: String,
    pub landblock: u16,
    pub at: Vec3,
    /// The cell it stands in: outdoors, or a room (another dungeon's, a building's).
    pub cell: u32,
    /// Where it sends who uses it: the cell, and the place in that cell's landblock.
    pub to: Option<(u32, Vec3)>,
}

impl Entrance {
    /// Whether it stands outdoors.
    #[must_use]
    pub fn outdoors(&self) -> bool {
        self.cell & 0xFFFF < crate::cells::FIRST_CELL.into()
    }
}

/// Where the portals `entrances` send who uses them: of those that stand outdoors if any do,
/// the destination most of them share (to the centimeter), the first such by cell and place.
#[must_use]
pub fn drop_point(entrances: &[Entrance]) -> Option<(u32, Vec3)> {
    let outdoors = entrances.iter().any(|e| e.outdoors() && e.to.is_some());
    #[allow(clippy::cast_possible_truncation)] // centimeters of a landblock, a few thousand
    let key = |(c, p): (u32, Vec3)| {
        (
            c,
            (p.x * 100.0).round() as i32,
            (p.y * 100.0).round() as i32,
            (p.z * 100.0).round() as i32,
        )
    };
    type Spot = (u32, i32, i32, i32);
    let mut counts: BTreeMap<Spot, (usize, (u32, Vec3))> = BTreeMap::new();
    for e in entrances.iter().filter(|e| !outdoors || e.outdoors()) {
        if let Some(to) = e.to {
            counts.entry(key(to)).or_insert((0, to)).0 += 1;
        }
    }
    let most = counts.values().map(|(n, _)| *n).max()?;
    counts
        .into_values()
        .find(|(n, _)| *n == most)
        .map(|(_, to)| to)
}

/// A guid as an override file writes it.
#[must_use]
pub fn guid_text(g: u32) -> String {
    format!("0x{g:08X}")
}

/// One server spawn an override file changes: written again whole (with the instances it links,
/// each guid as written: a number, or a token of the pack's own spawn), or taken away.
#[derive(Clone, Debug)]
enum Override {
    Placed(sql_ids::Instance, Vec<String>),
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
        let mut links: BTreeMap<u32, Vec<String>> = BTreeMap::new();
        let mut unlinked: BTreeSet<u32> = BTreeSet::new();
        sql_ids::rows(text, |row| {
            if row.table == "landblock_instance_link" {
                let parent = row.get("parent_GUID").and_then(sql_ids::number);
                if let (Some(p), Some(c)) = (parent, row.get("child_GUID")) {
                    let c = sql_ids::number(c).map_or_else(|| c.trim().to_owned(), guid_text);
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
                            .map(|c| format!("({guid}, {c}, '{today}')"))
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
        source::Link::Many(all) => all
            .iter()
            .map(|one| link_text(one, bare))
            .collect::<Vec<_>>()
            .join(", "),
    }
}

/// A portal's link as written with every cell `drop` says taken out of it: of a link that joins
/// several, the rest; of one, itself or nothing.
fn without(v: &str, drop: &dyn Fn(&str) -> bool) -> String {
    v.split(',')
        .map(str::trim)
        .filter(|one| !one.is_empty() && !drop(one))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Every portal of `doc`'s rows of `keys` with the cells `drop` says taken out of its link.
fn unlink(doc: &mut DocumentMut, keys: &[&str], drop: &dyn Fn(&str) -> bool) {
    for key in keys {
        if !doc.contains_key(key) {
            continue;
        }
        for t in tables(doc, key).iter_mut() {
            if let Some(ports) = t.get_mut("portals").and_then(Item::as_array_mut) {
                for v in ports.iter_mut() {
                    let Some(now) = v.as_str().map(str::to_owned) else {
                        continue;
                    };
                    let kept = without(&now, drop);
                    if kept != now.trim() {
                        *v = Value::from(kept);
                    }
                }
            }
        }
    }
}

/// Every portal of `doc`'s cells (new and reshaped) joined to the pack's cell `bare`, shut.
fn unjoin(doc: &mut DocumentMut, bare: &str) {
    unlink(doc, &["cell", "replace-cell"], &|one| {
        one.rsplit_once(':').is_some_and(|(c, _)| c == bare)
    });
}

/// The partners `links` names, each of the pack's cell `name`'s portals in order, linked back to
/// it: a new cell's in its row, a reshaped cell's in its row.
///
/// # Errors
/// A partner the landblock's file does not hold.
fn link_partners(
    doc: &mut DocumentMut,
    name: &str,
    links: &[source::Link],
    landblock: u16,
    bare: &dyn Fn(&str) -> String,
) -> Result<(), Error> {
    let each = links.iter().enumerate().flat_map(|(slot, l)| match l {
        source::Link::Many(all) => all.iter().map(|one| (slot, one)).collect::<Vec<_>>(),
        one => vec![(slot, one)],
    });
    for (slot, l) in each {
        let (what, row) = match l {
            source::Link::Base { cell, portal } => {
                let n = i64::from(*cell);
                let row = tables(doc, "replace-cell")
                    .iter_mut()
                    .find(|t| int(t, "cell") == Some(n));
                let Some(rt) = row else {
                    return Err(Error::Refused(format!(
                        "this pack does not reshape cell {cell:04X} of landblock {landblock:04X}"
                    )));
                };
                (format!("cell {cell:04X}"), port(rt, *portal))
            }
            source::Link::Cell { cell, portal } => {
                let partner = bare(cell);
                let row = tables(doc, "cell")
                    .iter_mut()
                    .find(|t| string(t, "name") == Some(partner.as_str()));
                let Some(pt) = row else {
                    return Err(Error::Refused(format!(
                        "no cell {partner} of this pack stands in landblock {landblock:04X}"
                    )));
                };
                (format!("cell {partner}"), port(pt, *portal))
            }
            _ => continue,
        };
        let Some(v) = row else {
            return Err(Error::Refused(format!("{what} has no such portal")));
        };
        // A portal joined to new cells already joins this one too, through another part of it.
        let back = format!("{name}:{slot}");
        let now = v.as_str().unwrap_or("").trim().to_owned();
        let joins_new = !now.is_empty()
            && now.split(',').all(|one| one.trim().contains(':'))
            && !now.split(',').any(|one| one.trim() == back);
        *v = Value::from(if joins_new {
            format!("{now}, {back}")
        } else {
            back
        });
    }
    Ok(())
}

/// Portal `k` of a cell's row.
fn port(t: &mut Table, k: usize) -> Option<&mut Value> {
    t.get_mut("portals")
        .and_then(Item::as_array_mut)
        .and_then(|p| p.get_mut(k))
}

/// A rotation's heading, degrees clockwise from north, when it turns about the vertical.
pub(crate) fn heading_of(q: Quat) -> f32 {
    (-2.0 * dereth_primitives::num::math::atan2f(q.z, q.w)).to_degrees()
}

/// The rotation a row writes: its heading, or its quaternion; none turns it not at all.
fn row_rotation(t: &Table) -> Rotation {
    let num = |v: &Value| v.as_float().or_else(|| v.as_integer().map(|i| i as f64));
    #[allow(clippy::cast_possible_truncation)]
    if let Some(h) = t.get("heading").and_then(Item::as_value).and_then(num) {
        return Rotation::Heading(h as f32);
    }
    let q: Option<Vec<f32>> = t.get("rotation").and_then(Item::as_array).map(|a| {
        #[allow(clippy::cast_possible_truncation)]
        a.iter().filter_map(num).map(|v| v as f32).collect()
    });
    match q.as_deref() {
        Some([w, x, y, z]) => Rotation::Quat([*w, *x, *y, *z]),
        _ => Rotation::Heading(0.0),
    }
}

/// A rotation as a row writes it: a heading when it turns about the vertical alone.
fn upright(q: Quat) -> Rotation {
    if q.x.abs() < 1e-4 && q.y.abs() < 1e-4 {
        Rotation::Heading(heading_of(q).rem_euclid(360.0))
    } else {
        Rotation::Quat([q.w, q.x, q.y, q.z])
    }
}

/// A dropped building's `[[add-building]]` row, in pack `pack`: a set's models and templates as
/// lists, one building's as one value each.
fn building_row(b: &source::AddBuilding, pack: &str) -> Table {
    let list = |items: Vec<String>| -> Value {
        if let [one] = &items[..] {
            parse_value(one)
        } else {
            parse_value(&format!("[{}]", items.join(", ")))
        }
    };
    let mut t = entry();
    let bare = b.name.strip_prefix(&format!("{pack}:")).unwrap_or(&b.name);
    t.insert("name", Item::Value(Value::from(bare)));
    t.insert(
        "model",
        Item::Value(list(
            b.models
                .iter()
                .map(|m| format!("0x{:08X}", m.raw()))
                .collect(),
        )),
    );
    t.insert(
        "template",
        Item::Value(list(
            b.templates.iter().map(|s| format!("\"{s}\"")).collect(),
        )),
    );
    t.insert("at", Item::Value(place_value(&b.at)));
    set_rotation(&mut t, b.rotation);
    match &b.furnished {
        source::Furnished::Template => {}
        source::Furnished::Bare => {
            t.insert("furnished", Item::Value(Value::from("bare")));
        }
        source::Furnished::Like(sites) => {
            t.insert(
                "furnished",
                Item::Value(list(sites.iter().map(|s| format!("\"{s}\"")).collect())),
            );
        }
    }
    if !b.house.is_empty() {
        let bare = |h: &String| h.strip_prefix(&format!("{pack}:")).unwrap_or(h).to_owned();
        t.insert(
            "house",
            Item::Value(list(
                b.house.iter().map(|h| format!("\"{}\"", bare(h))).collect(),
            )),
        );
    }
    t
}

/// A copied dungeon's `[[add-dungeon]]` row, in pack `pack`.
fn dungeon_row(d: &source::AddDungeon, pack: &str) -> Table {
    let bare = |n: &str| n.strip_prefix(&format!("{pack}:")).unwrap_or(n).to_owned();
    let mut t = entry();
    t.insert("name", Item::Value(Value::from(bare(&d.name))));
    t.insert("template", Item::Value(hex16(d.template)));
    if let Some(h) = &d.house {
        t.insert("house", Item::Value(Value::from(bare(h))));
    }
    t
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

/// The rows of `doc` that add, move or take away a static in one of `cells`, removed.
fn unwrite_cell_statics(doc: &mut DocumentMut, cells: &BTreeSet<u16>) {
    let in_cells = |t: &Table| {
        int(t, "cell")
            .and_then(|c| u16::try_from(c).ok())
            .is_some_and(|c| cells.contains(&c))
    };
    for key in ["add-cell-static", "move-cell-static", "remove-cell-static"] {
        while let Some(k) = doc
            .get(key)
            .and_then(Item::as_array_of_tables)
            .and_then(|rows| rows.iter().position(in_cells))
        {
            remove_table(doc, key, k);
        }
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
    crate::brush::WORLD.owner(gx, gy)
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
pub(crate) fn token_symbol(pack: &str, v: &str) -> Option<String> {
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
        let base = DatSet::for_world(dats, world.era)?;
        let opened_lock = Lock::read(&world.lock_path())?;
        let compiled = compile::compile(&world, &packs, &base, &opened_lock)?;
        let region = read_compiled(&compiled, &base, ModernDat::Portal, compile::REGION.raw())
            .and_then(|b| base.decode::<Region>(compile::REGION, &b).ok());
        Ok(Self {
            world,
            base,
            packs,
            target,
            opened_lock,
            compiled,
            region,
            sources: Sources::default(),
            history: History::default(),
            today: today(),
            retail: BTreeMap::new(),
            retail_instances: BTreeMap::new(),
            retail_by_class: RetailByClass::default(),
            retail_links: BTreeMap::new(),
            shown: BTreeSet::new(),
            retail_quests: BTreeMap::new(),
            retail_treasure: BTreeMap::new(),
            retail_wielded: BTreeMap::new(),
            retail_loot_uses: BTreeMap::new(),
            pack_loot: std::cell::OnceCell::new(),
            weenie_files: std::cell::OnceCell::new(),
            retail_uses: BTreeMap::new(),
            retail_body_parts: BTreeMap::new(),
            retail_quest_uses: BTreeMap::new(),
            schema: None,
            cell_reads: std::cell::Cell::new(0),
            server_reads: std::cell::Cell::new(0),
            buildings: None,
            link_table: std::cell::OnceCell::new(),
            pack_class_facts: std::cell::OnceCell::new(),
            pack_weenie_facts: std::cell::OnceCell::new(),
        })
    }

    /// Learn the world's buildings, read already ([`crate::buildings::Catalog::read`]): what a
    /// drop copies ([`Self::drop_building`]).
    pub fn learn_buildings(&mut self, catalog: crate::buildings::Catalog) {
        self.buildings = Some(catalog);
    }

    /// The world's buildings, when the session learned them.
    #[must_use]
    pub fn buildings(&self) -> Option<&crate::buildings::Catalog> {
        self.buildings.as_ref()
    }

    /// How many instances the server's own world places in landblock `lb`, as the session
    /// learned them.
    #[must_use]
    pub fn server_instances(&self, lb: u16) -> usize {
        self.retail_instances.get(&lb).map_or(0, Vec::len)
    }

    /// The date the SQL rows the session writes are stamped with.
    #[must_use]
    pub fn stamp(&self) -> &str {
        &self.today
    }

    /// How many times a landblock's indoor cells were read ([`Self::cells`]): the work an
    /// editor's frame does, counted.
    #[must_use]
    pub fn cells_read(&self) -> u64 {
        self.cell_reads.get()
    }

    /// How many of the server's own instances were read as spawns ([`Self::spawns`],
    /// [`Self::appearances`]): the work a listing does, counted.
    #[must_use]
    pub fn server_spawns_read(&self) -> u64 {
        self.server_reads.get()
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
                let list = self.retail_instances.entry(lb).or_default();
                let at = (lb, list.len());
                match sql_ids::number(&i.weenie) {
                    Some(c) => self.retail_by_class.classes.entry(c).or_default().push(at),
                    None => self.retail_by_class.others.push(at),
                }
                list.push(i);
            }
        }
        for (parent, child) in content.links {
            self.retail_links.entry(parent).or_default().push(child);
        }
        self.link_table = std::cell::OnceCell::new();
        self.retail_quests.extend(content.quests);
        self.retail_treasure.extend(content.treasure);
        self.retail_wielded.extend(content.wielded);
        self.retail_loot_uses.extend(content.loot_uses);
        // Learned again, a class's users are the database's as read last.
        self.retail_uses.extend(content.uses);
        self.retail_body_parts.extend(content.body_parts);
        self.retail_quest_uses.extend(content.quest_uses);
    }

    /// The typical row of body part `key` among the server's weenies, every column but its own
    /// id and its weenie's, as written: `None` before the server's database is learned, or for a
    /// part none of its weenies has.
    #[must_use]
    pub fn typical_body_part(&self, key: u32) -> Option<&[(String, String)]> {
        self.retail_body_parts.get(&key).map(Vec::as_slice)
    }

    /// Every weenie whose create list, generator or emotes name weenie `w`, and how: the packs'
    /// weenies as their files have them (a later pack's file of a weenie over an earlier's), and
    /// the server's own as its database has them, but for a server's weenie a pack writes again,
    /// whose pack's file stands for it.
    #[must_use]
    pub fn uses_of(&self, w: &Weenie) -> Vec<(Weenie, sql_ids::Use)> {
        let as_weenie = |pack: &str, v: &str| match sql_ids::number(v) {
            Some(c) => Some(Weenie::Class(c)),
            None => token_symbol(pack, v).map(Weenie::Symbol),
        };
        let mut out = Vec::new();
        let mut written: std::collections::HashSet<Weenie> = std::collections::HashSet::new();
        for p in self.packs.iter().rev() {
            for f in &p.sql {
                let uses = sql_ids::class_uses(&f.text);
                let owners: Vec<Weenie> = sql_ids::weenie_facts(&f.text)
                    .keys()
                    .filter_map(|k| as_weenie(&p.name, k))
                    .collect();
                if owners.iter().any(|o| written.contains(o)) {
                    continue;
                }
                for u in uses {
                    if as_weenie(&p.name, &u.names).as_ref() != Some(w) {
                        continue;
                    }
                    if let Some(by) = as_weenie(&p.name, &u.by) {
                        out.push((by, u.how));
                    }
                }
                written.extend(owners);
            }
        }
        if let Weenie::Class(c) = w {
            for (by, how) in self.retail_uses.get(c).into_iter().flatten() {
                if !written.contains(&Weenie::Class(*by)) {
                    out.push((Weenie::Class(*by), *how));
                }
            }
        }
        out
    }

    /// Everywhere weenie `w` is named but by itself: its spawns in the world, the weenies whose
    /// create lists, generators and emotes name it ([`Self::uses_of`]), and the wielded loot tables
    /// that hold it.
    #[must_use]
    pub fn named_anywhere(&self, w: &Weenie) -> Named {
        Named {
            spawns: self.appearances(w),
            by: self
                .uses_of(w)
                .into_iter()
                .filter(|(by, _)| by != w)
                .collect(),
            loot: self.loot_holding(w),
        }
    }

    /// The change that takes the pack being edited's own weenie `w` away: its file, as one step.
    ///
    /// # Errors
    /// One of the server's weenies (a pack's change of one is put back instead), another pack's,
    /// one whose file writes other weenies too, or one still named anywhere.
    pub fn weenie_removal(&self, w: &Weenie) -> Result<Op, Error> {
        let Weenie::Symbol(sym) = w else {
            return Err(Error::Refused(
                "the server's weenies are not deleted; a pack's change of one is put back".into(),
            ));
        };
        let (file, mine) = self
            .weenie_file(w)
            .ok_or_else(|| Error::Refused(format!("no file of the packs writes {sym}")))?;
        if !mine {
            return Err(Error::Refused(format!(
                "{sym} is another pack's: it is deleted in that pack"
            )));
        }
        let others = self.packs[self.target]
            .sql
            .iter()
            .find(|f| f.path == file)
            .map_or(0, |f| sql_ids::weenie_facts(&f.text).len());
        if others > 1 {
            return Err(Error::Refused(format!(
                "{} writes {others} weenies: it is changed by hand",
                file.display()
            )));
        }
        let named = self.named_anywhere(w);
        if !named.is_empty() {
            return Err(Error::Refused(format!(
                "{sym} is still used in {} place(s)",
                named.places()
            )));
        }
        Ok(Op::WriteSql { file, text: None })
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

    /// Every quest the server's database registers, by name.
    #[must_use]
    pub fn server_quests(&self) -> &BTreeMap<String, sql_ids::Quest> {
        &self.retail_quests
    }

    /// Who names each quest among the server's own weenies, by the quest as registered
    /// ([`crate::content_check::quest_key`]), as its database has them.
    #[must_use]
    pub fn server_quest_uses(&self) -> &BTreeMap<String, sql_ids::QuestNamers> {
        &self.retail_quest_uses
    }

    /// The death treasure of treasure type `id` as it stands: a pack's (a later pack's over an
    /// earlier's), else the server's.
    #[must_use]
    pub fn treasure(&self, id: u32) -> Option<sql_ids::Treasure> {
        self.pack_loot()
            .death
            .get(&id)
            .map(|(t, _)| t.clone())
            .or_else(|| self.retail_treasure.get(&id).cloned())
    }

    /// Every death treasure of the world as it stands, by its id: the packs' and the server's.
    #[must_use]
    pub fn treasures(&self) -> BTreeMap<u32, sql_ids::Treasure> {
        let mut out = self.retail_treasure.clone();
        out.extend(
            self.pack_loot()
                .death
                .iter()
                .map(|(id, (t, _))| (*id, t.clone())),
        );
        out
    }

    /// Every death treasure the server's database holds, by its id.
    #[must_use]
    pub fn server_treasures(&self) -> &BTreeMap<u32, sql_ids::Treasure> {
        &self.retail_treasure
    }

    /// Every wielded treasure the server's database holds, by its id, each one's rows in order.
    #[must_use]
    pub fn server_wielded(&self) -> &BTreeMap<u32, Vec<sql_ids::WieldedItem>> {
        &self.retail_wielded
    }

    /// Who names each loot table among the server's own weenies, by the table's id, as its
    /// database has them.
    #[must_use]
    pub fn server_loot_uses(&self) -> &BTreeMap<u32, Vec<(u32, sql_ids::LootUse)>> {
        &self.retail_loot_uses
    }

    /// The packs' loot tables, as their files stand now.
    pub(crate) fn pack_loot(&self) -> &crate::loot::PackLoot {
        self.pack_loot
            .get_or_init(|| crate::loot::PackLoot::read(&self.packs, &self.compiled.lock))
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
        self.pack_class_facts()
            .get(&class)
            .or_else(|| self.retail.get(&class))
            .cloned()
    }

    /// The creatures the spawns standing in `landblocks` are, and those their generators make:
    /// what the SQL says of each (its level and death treasure among it), once for each spawn or
    /// profile.
    pub fn creatures_in(&mut self, landblocks: &BTreeSet<u16>) -> Vec<sql_ids::WeenieFacts> {
        const CREATURE: u32 = 10;
        let pack_weenies = self.pack_weenies();
        let facts = |s: &Self, pack: usize, w: &str| -> Option<sql_ids::WeenieFacts> {
            if let Some(c) = sql_ids::number(w) {
                return s.class_facts(c);
            }
            let name = &s.packs.get(pack)?.name;
            pack_weenies.get(&token_symbol(name, w)?).cloned()
        };
        let mut out = Vec::new();
        let spawns: Vec<Spawn> = self
            .spawns()
            .into_iter()
            .filter(|s| landblocks.contains(&s.landblock()))
            .collect();
        for s in &spawns {
            out.extend(facts(self, s.pack, &s.weenie));
            if let Some(g) = self.generator(s) {
                for p in g.profiles.iter().filter(|p| !p.makes_treasure()) {
                    for _ in 0..p.max.clamp(1, 8) {
                        out.extend(facts(self, s.pack, &p.class));
                    }
                }
            }
        }
        out.retain(|f| f.kind == Some(CREATURE) && f.level.is_some());
        out
    }

    /// The quests a place checks: those the portals standing in `landblocks` (and those their
    /// generators make) let only their holders through, and those the emotes of what stands there
    /// test; and, for a dungeon, those of every portal that leads into landblock `into`. Each quest
    /// once, in the order found, with the name of the first thing that checks it.
    pub fn quests_checked_in(
        &mut self,
        landblocks: &BTreeSet<u16>,
        into: Option<u16>,
    ) -> Vec<(String, String)> {
        const PORTAL: u32 = 7;
        let pack_weenies = self.pack_weenies();
        let classes = self.pack_classes();
        // A pack's copy of a server class over the server's own, as `class_facts` takes it.
        let facts = |s: &Self, pack: usize, w: &str| -> Option<sql_ids::WeenieFacts> {
            if let Some(c) = sql_ids::number(w) {
                return classes.get(&c).or_else(|| s.retail.get(&c)).cloned();
            }
            let name = &s.packs.get(pack)?.name;
            pack_weenies.get(&token_symbol(name, w)?).cloned()
        };
        let mut found: Vec<sql_ids::WeenieFacts> = Vec::new();
        let spawns: Vec<Spawn> = self
            .spawns()
            .into_iter()
            .filter(|s| landblocks.contains(&s.landblock()))
            .collect();
        for s in &spawns {
            found.extend(facts(self, s.pack, &s.weenie));
            if let Some(g) = self.generator(s) {
                for p in g.profiles.iter().filter(|p| !p.makes_treasure()) {
                    found.extend(facts(self, s.pack, &p.class));
                }
            }
        }
        if let Some(lb) = into {
            let leads = |f: &sql_ids::WeenieFacts| {
                f.kind == Some(PORTAL) && f.destination.is_some_and(|d| d >> 16 == u32::from(lb))
            };
            found.extend(
                self.retail
                    .iter()
                    .filter(|(c, f)| !classes.contains_key(c) && leads(f))
                    .map(|(_, f)| f.clone()),
            );
            found.extend(classes.values().filter(|f| leads(f)).cloned());
            found.extend(pack_weenies.values().filter(|f| leads(f)).cloned());
        }
        let mut out: Vec<(String, String)> = Vec::new();
        for f in found {
            let who = f.name.clone().unwrap_or_default();
            for q in f.quest_restriction.iter().chain(&f.tests_quests) {
                let key = crate::content_check::quest_key(q);
                if !key.is_empty()
                    && !out
                        .iter()
                        .any(|(n, _)| crate::content_check::quest_key(n) == key)
                {
                    out.push((
                        q.split('@').next().unwrap_or(q).trim().to_owned(),
                        who.clone(),
                    ));
                }
            }
        }
        out
    }

    /// Every quest the world registers, by name: the packs' and the server's own, once each.
    #[must_use]
    pub fn quest_names(&self) -> Vec<String> {
        let mut out: Vec<String> = self.retail_quests.keys().cloned().collect();
        for p in &self.packs {
            for f in &p.sql {
                out.extend(sql_ids::quests_in(&f.text).into_keys());
            }
        }
        out.sort_by_key(|n| n.to_lowercase());
        out.dedup_by_key(|n| n.to_lowercase());
        out
    }

    /// The server classes the packs write again, and what their SQL says of each (a later
    /// pack's over an earlier's).
    #[must_use]
    pub fn pack_classes(&self) -> BTreeMap<u32, sql_ids::WeenieFacts> {
        self.pack_class_facts().clone()
    }

    /// [`Self::pack_classes`], read once for each change of the packs' SQL.
    fn pack_class_facts(&self) -> &BTreeMap<u32, sql_ids::WeenieFacts> {
        self.pack_class_facts
            .get_or_init(|| self.read_pack_classes())
    }

    fn read_pack_classes(&self) -> BTreeMap<u32, sql_ids::WeenieFacts> {
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

    /// Every portal standing in the world that leads into landblock `lb`: the packs' spawns of a
    /// portal whose destination is there, and the server's own (but those a pack writes again or
    /// takes away), each as its spawn's guid, landblock and place there.
    #[must_use]
    pub fn entrances(&self, lb: u16) -> Vec<Entrance> {
        const PORTAL: u32 = 7;
        let into = |f: &sql_ids::WeenieFacts| {
            f.kind == Some(PORTAL) && f.destination.is_some_and(|d| d >> 16 == u32::from(lb))
        };
        let to = |f: &sql_ids::WeenieFacts| {
            let [x, y, z] = f.destination_at.unwrap_or_default();
            f.destination.map(|c| (c, Vec3::new(x, y, z)))
        };
        // The packs' portals, by the weenie as their spawns write it, with where each sends.
        let mut leads: BTreeMap<String, Option<(u32, Vec3)>> = BTreeMap::new();
        for p in &self.packs {
            for f in &p.sql {
                for (w, x) in sql_ids::weenie_facts(&f.text) {
                    if into(&x) {
                        leads.insert(token_symbol(&p.name, &w).unwrap_or(w), to(&x));
                    }
                }
            }
        }
        let classes = self.pack_classes();
        // Where the portal weenie `w` (as pack `pack`'s spawn writes it) sends who uses it, when
        // it leads in.
        let leads_in = |pack: usize, w: &str| -> Option<Option<(u32, Vec3)>> {
            if let Some(sym) = token_symbol(&self.packs[pack].name, w) {
                return leads.get(&sym).copied();
            }
            let c = sql_ids::number(w)?;
            classes
                .get(&c)
                .or_else(|| self.retail.get(&c))
                .filter(|f| into(f))
                .map(to)
        };
        let mut out: Vec<Entrance> = self
            .spawns()
            .into_iter()
            .filter_map(|s| {
                let to = leads_in(s.pack, &s.weenie)?;
                Some(Entrance {
                    guid: s.guid,
                    landblock: (s.cell >> 16) as u16,
                    at: s.frame.origin,
                    cell: s.cell,
                    to,
                })
            })
            .collect();
        // The server's own beyond the landblocks shown.
        let seen: BTreeSet<String> = out.iter().map(|e| e.guid.clone()).collect();
        let taken = self.server_taken();
        // Only the instances of a class that leads in, in the order the server lists them.
        let by_class = &self.retail_by_class;
        let leading: BTreeSet<u32> = self
            .retail
            .iter()
            .filter(|(c, f)| !classes.contains_key(c) && into(f))
            .chain(classes.iter().filter(|(_, f)| into(f)))
            .map(|(c, _)| *c)
            .collect();
        let mut places: Vec<(u16, usize)> = leading
            .iter()
            .filter_map(|c| by_class.classes.get(c))
            .flatten()
            .chain(&by_class.others)
            .copied()
            .collect();
        places.sort_unstable();
        for (at, i) in places.iter().filter_map(|(at, k)| {
            self.retail_instances
                .get(at)
                .and_then(|l| l.get(*k))
                .map(|i| (at, i))
        }) {
            let Some(g) = sql_ids::number(&i.guid).filter(|g| !taken.contains(g)) else {
                continue;
            };
            let guid = guid_text(g);
            if seen.contains(&guid) {
                continue;
            }
            let Some(to) = leads_in(self.target, &i.weenie) else {
                continue;
            };
            out.push(Entrance {
                guid,
                landblock: *at,
                at: Vec3::new(i.origin[0], i.origin[1], i.origin[2]),
                cell: i.cell,
                to,
            });
        }
        out
    }

    /// Where who goes into dungeon `lb` arrives: the destination of the portals that lead in,
    /// those that stand outdoors first, then the one most of them share. `None` when no portal
    /// leads in.
    #[must_use]
    pub fn drop_point(&self, lb: u16) -> Option<(u32, Vec3)> {
        drop_point(&self.entrances(lb))
    }

    /// The SQL file that writes weenie `w`'s rows, and whether it is the pack being edited's:
    /// the pack's file of its symbol, or of a server class the pack writes again.
    #[must_use]
    pub fn weenie_file(&self, w: &Weenie) -> Option<(PathBuf, bool)> {
        self.weenie_files
            .get_or_init(|| self.read_weenie_files())
            .get(w)
            .cloned()
    }

    /// The file that writes each weenie: the last pack's first.
    fn read_weenie_files(&self) -> std::collections::HashMap<Weenie, (PathBuf, bool)> {
        let mut out = std::collections::HashMap::new();
        for (pi, p) in self.packs.iter().enumerate().rev() {
            for f in p
                .sql
                .iter()
                .filter(|f| f.text.contains("INSERT INTO `weenie` ("))
            {
                for k in sql_ids::weenie_facts(&f.text).into_keys() {
                    let w = match sql_ids::number(&k) {
                        Some(c) => Weenie::Class(c),
                        None => match token_symbol(&p.name, &k) {
                            Some(s) => Weenie::Symbol(s),
                            None => continue,
                        },
                    };
                    out.entry(w)
                        .or_insert_with(|| (f.path.clone(), pi == self.target));
                }
            }
        }
        out
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

    /// The SQL text of `path` as it was last read from the disk or saved: `None` for a file the
    /// disk did not have.
    pub fn saved_sql_text(&mut self, path: &Path) -> Option<String> {
        self.sources.stored(&DiskStore, path)
    }

    /// Whether the session's text of `path` differs from the disk's.
    #[must_use]
    pub fn sql_unsaved(&self, path: &Path) -> bool {
        self.sources.differs(path)
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
        self.pack_weenie_facts
            .get_or_init(|| self.read_pack_weenies())
            .clone()
    }

    fn read_pack_weenies(&self) -> BTreeMap<String, sql_ids::WeenieFacts> {
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
                    e.icon = x.icon.or(e.icon);
                    e.motion_table = x.motion_table.or(e.motion_table);
                    e.sound_table = x.sound_table.or(e.sound_table);
                    e.palette_base = x.palette_base.or(e.palette_base);
                    e.kind = x.kind.or(e.kind);
                    e.stuck = x.stuck.or(e.stuck);
                    e.attackable = x.attackable.or(e.attackable);
                    e.generator_radius = x.generator_radius.or(e.generator_radius);
                    e.generates |= x.generates;
                    e.destination = x.destination.or(e.destination);
                    e.destination_at = x.destination_at.or(e.destination_at);
                    e.level = x.level.or(e.level);
                    e.death_treasure = x.death_treasure.or(e.death_treasure);
                    e.wielded_treasure = x.wielded_treasure.or(e.wielded_treasure);
                    e.burden = x.burden.or(e.burden);
                    e.unit_burden = x.unit_burden.or(e.unit_burden);
                    e.max_stack = x.max_stack.or(e.max_stack);
                    e.holds = x.holds.or(e.holds);
                    e.locations = x.locations.or(e.locations);
                    if x.wield.iter().any(|v| *v != 0) {
                        e.wield = x.wield;
                    }
                    e.augmentation_cost = x.augmentation_cost.or(e.augmentation_cost);
                    e.quest_restriction = x.quest_restriction.or(e.quest_restriction.take());
                    for q in x.tests_quests {
                        if !e.tests_quests.contains(&q) {
                            e.tests_quests.push(q);
                        }
                    }
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
        self.base
            .decode::<CellLandblock>(DataId(id), &self.cell_record(id)?)
            .ok()
    }

    /// Landblock `lb`'s statics record as the compiled world holds it.
    #[must_use]
    pub fn statics(&self, lb: u16) -> Option<LandblockInfo> {
        let id = (u32::from(lb) << 16) | 0xFFFE;
        self.base
            .decode::<LandblockInfo>(DataId(id), &self.cell_record(id)?)
            .ok()
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
            match self
                .base
                .decode::<dereth_assets::world::EnvCell>(DataId(id), &bytes)
            {
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

    /// The free doorways of the target pack's cells in landblock `lb`: each portal its rows leave
    /// shut, of its new cells and of the landblock's own it reshaped, with the link a new piece's
    /// portal there is given. With the new cells `lifting` (their symbols, bare or in full) as
    /// though taken away: their own portals left out, and those joined to them free.
    #[must_use]
    pub fn free_doorways(&self, lb: u16, lifting: &[String]) -> Vec<crate::grid::Doorway> {
        use crate::source::Link;
        let pack = &self.packs[self.target].name;
        let bare = |full: &str| {
            full.strip_prefix(&format!("{pack}:"))
                .unwrap_or(full)
                .to_owned()
        };
        let lifted: BTreeSet<String> = lifting.iter().map(|n| bare(n)).collect();
        let free = |l: &Link| match l {
            Link::Shut => true,
            Link::Cell { cell, .. } => lifted.contains(&bare(cell)),
            _ => false,
        };
        let mut envs: BTreeMap<DataId, Option<dereth_assets::Environment>> = BTreeMap::new();
        let mut env = |id: DataId| {
            envs.entry(id)
                .or_insert_with(|| self.environment(id))
                .clone()
        };
        let mut out = Vec::new();
        for e in self.packs[self.target]
            .edits
            .iter()
            .filter(|e| e.landblock == lb)
        {
            for r in &e.replaces {
                let r = &r.item;
                let frame = Frame::new(
                    Vec3::new(r.at[0], r.at[1], r.at[2]),
                    compile::quat_of(r.rotation),
                );
                let Some(en) = env(r.environment) else {
                    continue;
                };
                for (slot, l) in r.portals.iter().enumerate() {
                    if !free(l) {
                        continue;
                    }
                    if let Some(o) = crate::pieces::opening(&en, r.piece, slot, &frame) {
                        out.push(crate::grid::Doorway {
                            link: Link::Base {
                                cell: r.cell,
                                portal: slot,
                            },
                            opening: o,
                        });
                    }
                }
            }
            for a in &e.cells {
                let a = &a.item;
                if lifted.contains(&bare(&a.name)) {
                    continue;
                }
                let frame = Frame::new(
                    Vec3::new(a.at[0], a.at[1], a.at[2]),
                    compile::quat_of(a.rotation),
                );
                let Some(en) = env(a.environment) else {
                    continue;
                };
                for (slot, l) in a.portals.iter().enumerate() {
                    if !free(l) {
                        continue;
                    }
                    if let Some(o) = crate::pieces::opening(&en, a.piece, slot, &frame) {
                        out.push(crate::grid::Doorway {
                            link: Link::Cell {
                                cell: a.name.clone(),
                                portal: slot,
                            },
                            opening: o,
                        });
                    }
                }
            }
        }
        out
    }

    /// The environment `id`, with the compiled packs over the base files.
    #[must_use]
    pub fn environment(&self, id: DataId) -> Option<dereth_assets::Environment> {
        let bytes = read_compiled(&self.compiled, &self.base, ModernDat::Portal, id.raw())?;
        self.base
            .decode::<dereth_assets::Environment>(id, &bytes)
            .ok()
    }

    /// The packs' original dungeon pieces, with their default surfaces, including unplaced pieces.
    #[must_use]
    pub fn new_pieces(&self) -> Vec<crate::pieces::Piece> {
        use crate::source::{NewRecord, RecordRef, Target};
        use crate::symbol::Kind;
        let mut out = Vec::new();
        for record in self.packs.iter().flat_map(|p| &p.records) {
            let NewRecord::Geometry { target, record } = &record.item else {
                continue;
            };
            let crate::model_import::Record::Environment { surfaces, .. } = record.as_ref() else {
                continue;
            };
            let id = match target {
                Target::New(name) => self.compiled.lock.get(Kind::Environment, name).map(DataId),
                Target::Replace(id) => Some(*id),
            };
            let Some(env) = id.and_then(|id| self.environment(id)) else {
                continue;
            };
            let surfaces: Option<Vec<_>> = surfaces
                .iter()
                .map(|r| match r {
                    RecordRef::Id(id) => Some(*id),
                    RecordRef::Symbol(name) => {
                        self.compiled.lock.get(Kind::Surface, name).map(DataId)
                    }
                })
                .collect();
            if let Some(surfaces) = surfaces {
                out.extend(crate::pieces::original(&env, &surfaces));
            }
        }
        out.sort_by_key(|p| (p.environment, p.piece));
        out.dedup_by_key(|p| (p.environment, p.piece));
        out
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

    /// [`Self::cell_hit`], meeting a wall or floor only from its own room's inside: the cell,
    /// the point, and the way the wall or floor faces into its room.
    #[must_use]
    pub fn cell_hit_facing(&self, lb: u16, from: Vec3, dir: Vec3) -> Option<(u16, Vec3, Vec3)> {
        crate::walk::cell_hit_facing(&self.cells(lb), from, dir, &|id| self.environment(id))
    }

    /// The indoor cell of landblock `lb` point `p` (landblock meters) stands in, and the floor's
    /// height under it.
    #[must_use]
    pub fn cell_at(&self, lb: u16, p: Vec3) -> Option<(u16, f32)> {
        crate::walk::cell_at(&self.cells(lb), p, &|id| self.environment(id))
    }

    /// The indoor cell of landblock `lb` a body let go at point `p` (landblock meters) comes to
    /// stand in, and the height of the floor it stands on: down through any opening in a floor
    /// to the floor under it ([`crate::walk::landing`]).
    #[must_use]
    pub fn landing(&self, lb: u16, p: Vec3) -> Option<(u16, f32)> {
        crate::walk::landing(&self.cells(lb), p, &|id| self.environment(id))
    }

    /// The world's own dungeons: every landblock of the files whose first cell is a dungeon's,
    /// with its number of cells ([`crate::dungeons::retail`]). A read of every landblock record,
    /// to be kept.
    #[must_use]
    pub fn retail_dungeons(&self) -> BTreeMap<u16, u32> {
        crate::dungeons::retail(&|id| self.base.cell.read(id).ok(), self.base.era)
    }

    /// What the packs say of landblock `lb`'s dungeon: its name and its look, a later pack's over
    /// an earlier's.
    #[must_use]
    pub fn dungeon_info(&self, lb: u16) -> source::DungeonInfo {
        let mut out = source::DungeonInfo::default();
        for p in &self.packs {
            for e in p.edits.iter().filter(|e| e.landblock == lb) {
                if let Some(d) = &e.dungeon {
                    if d.item.name.is_some() {
                        out.name.clone_from(&d.item.name);
                    }
                    if d.item.look.is_some() {
                        out.look = d.item.look;
                    }
                }
            }
        }
        out
    }

    /// The dungeons: the world's own (`retail`, as [`Self::retail_dungeons`] found them) and every
    /// landblock a pack places cells in or names, each named as a pack names it, else by the
    /// portals the server knows that lead into it, in landblock order; not one whose every room a
    /// pack takes away.
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
        // A dungeon a pack names: by its name, even before its first room.
        let named: BTreeSet<u16> = self
            .packs
            .iter()
            .flat_map(|p| p.edits.iter())
            .filter(|e| e.dungeon.is_some())
            .map(|e| e.landblock)
            .collect();
        for lb in named {
            let info = self.dungeon_info(lb);
            let d = out.entry(lb).or_insert_with(|| crate::dungeons::Dungeon {
                landblock: lb,
                name: names.get(&lb).cloned(),
                cells: 0,
                pack: true,
            });
            d.pack = true;
            if info.name.is_some() {
                d.name = info.name;
            }
        }
        // A dungeon whose every room a pack takes away is gone.
        let taking: BTreeSet<u16> = self
            .packs
            .iter()
            .flat_map(|p| p.edits.iter())
            .filter(|e| !e.taken.is_empty())
            .map(|e| e.landblock)
            .collect();
        for lb in taking {
            let open = self
                .cells(lb)
                .values()
                .filter(|c| c.frame.origin.z > crate::cells::CLOSED_DEPTH + 1.0)
                .count();
            if open == 0 {
                out.remove(&lb);
            }
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
    /// the spawns that go with it ([`Self::building_spawns`]) carried along: one step. A dropped
    /// building's own move carries what goes with it, and stands it on level ground whatever the
    /// height asked.
    #[must_use]
    pub fn move_building(
        &self,
        lb: u16,
        index: usize,
        at: [f32; 3],
        rotation: Rotation,
    ) -> Option<Op> {
        let from = self.statics(lb)?.buildings.get(index)?.frame;
        // A dropped one: its row, which takes what goes with it along ([`crate::drops`]).
        if self.dropped_building(lb, index).is_some() {
            return Some(Op::MoveBuilding {
                landblock: lb,
                index,
                at,
                rotation,
            });
        }
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

    /// The packs' spawns standing in the pack's new cell `name` (its symbol, in full) of
    /// landblock `lb`.
    #[must_use]
    pub fn spawns_in_cell(&self, lb: u16, name: &str) -> Vec<Spawn> {
        let Some(index) = self
            .cell_names(lb)
            .into_iter()
            .find(|(_, n)| n == name)
            .map(|(c, _)| c)
        else {
            return Vec::new();
        };
        let id = (u32::from(lb) << 16) | u32::from(index);
        self.spawns()
            .into_iter()
            .filter(|s| s.cell == id && !s.server && s.pack == self.target)
            .collect()
    }

    /// Every spawn the packs' SQL places, and the server's own in the landblocks shown, with the
    /// setup each is drawn with when the session knows it.
    #[must_use]
    pub fn spawns(&self) -> Vec<Spawn> {
        let classes = self.pack_classes();
        let mut out = self.pack_spawns(&classes);
        // The server's own in the landblocks shown, but those a pack writes again or takes away.
        let taken = self.server_taken();
        for lb in &self.shown {
            for i in self.retail_instances.get(lb).into_iter().flatten() {
                out.extend(self.server_spawn(*lb, i, &taken, &classes));
            }
        }
        out
    }

    /// Every spawn of weenie `w` in the world: the packs' (as [`Self::spawns`] lists them) and,
    /// for a server class, the server's own in every landblock, shown or not, but those a pack
    /// writes again or takes away; by landblock. Only the server's instances of the class are
    /// read, through the index kept as they are learned.
    #[must_use]
    pub fn appearances(&self, w: &Weenie) -> Vec<Spawn> {
        let classes = self.pack_classes();
        let mut out: Vec<Spawn> = self
            .pack_spawns(&classes)
            .into_iter()
            .filter(|s| self.spawn_weenie(s).as_ref() == Some(w))
            .collect();
        if let Weenie::Class(c) = w {
            let taken = self.server_taken();
            for (lb, k) in self.retail_by_class.classes.get(c).into_iter().flatten() {
                if let Some(i) = self.retail_instances.get(lb).and_then(|l| l.get(*k)) {
                    out.extend(self.server_spawn(*lb, i, &taken, &classes));
                }
            }
        }
        out.sort_by_key(Spawn::landblock);
        out
    }

    /// Spawn `guid` of `file`, wherever it stands: a pack's, or one of the server's own in any
    /// landblock, shown or not (`file` is then its landblock's override file).
    #[must_use]
    pub fn spawn(&self, file: &Path, guid: &str) -> Option<Spawn> {
        let classes = self.pack_classes();
        if let Some(s) = self
            .pack_spawns(&classes)
            .into_iter()
            .find(|s| s.file == file && s.guid == guid)
        {
            return Some(s);
        }
        let lb = self.override_landblock(file)?;
        let g = sql_ids::number(guid)?;
        let i = self
            .retail_instances
            .get(&lb)?
            .iter()
            .find(|i| sql_ids::number(&i.guid) == Some(g))?;
        self.server_spawn(lb, i, &self.server_taken(), &classes)
    }

    /// Every spawn the packs' SQL places, the packs' classes `classes` ([`Self::pack_classes`])
    /// giving a server class's setup where a pack writes it again.
    fn pack_spawns(&self, classes: &BTreeMap<u32, sql_ids::WeenieFacts>) -> Vec<Spawn> {
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
        let mut out = Vec::new();
        for (pi, p) in self.packs.iter().enumerate() {
            for f in &p.sql {
                for i in sql_ids::instances(&f.text) {
                    let setup = match token_symbol(&p.name, &i.weenie) {
                        Some(sym) => setups.get(&sym).copied(),
                        None => {
                            sql_ids::number(&i.weenie).and_then(|c| self.class_setup(classes, c))
                        }
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
        out
    }

    /// The setup server class `c` is drawn with: a pack's copy of it (among `classes`), else
    /// the server's own.
    fn class_setup(&self, classes: &BTreeMap<u32, sql_ids::WeenieFacts>, c: u32) -> Option<u32> {
        classes
            .get(&c)
            .or_else(|| self.retail.get(&c))
            .and_then(|f| f.setup)
    }

    /// The guids of the server's own spawns the packs write again or take away.
    fn server_taken(&self) -> BTreeSet<u32> {
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
        taken
    }

    /// The server's own instance `i` of landblock `lb` as a spawn, unless a pack writes it again
    /// or takes it away (its guid among `taken`): counted as read either way.
    fn server_spawn(
        &self,
        lb: u16,
        i: &sql_ids::Instance,
        taken: &BTreeSet<u32>,
        classes: &BTreeMap<u32, sql_ids::WeenieFacts>,
    ) -> Option<Spawn> {
        self.server_reads.set(self.server_reads.get() + 1);
        let g = sql_ids::number(&i.guid).filter(|g| !taken.contains(g))?;
        let [w, x, y, z] = i.angles;
        Some(Spawn {
            pack: self.target,
            file: self.override_file(lb),
            guid: guid_text(g),
            weenie: i.weenie.clone(),
            cell: i.cell,
            frame: Frame::new(
                Vec3::new(i.origin[0], i.origin[1], i.origin[2]),
                Quat::new(w, x, y, z),
            ),
            setup: sql_ids::number(&i.weenie)
                .and_then(|c| self.class_setup(classes, c))
                .map(DataId),
            line: 0,
            server: true,
        })
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
            g = generator_of(&crate::content::SqlDoc::parse(&text));
        } else if let Weenie::Class(c) = &w {
            g.radius = self.retail.get(c).and_then(|f| f.generator_radius);
        }
        // A door's, a switch's or a pressure plate's links are what sets it off, not what it owns.
        if let Some(guid) = sql_ids::number(&s.guid).filter(|_| !self.can_be_set_off(s)) {
            let mut kids: Vec<u32> = self.retail_links.get(&guid).cloned().unwrap_or_default();
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
        self.override_entry(lb, &mut o, guid)?;
        Ok(o.render(lb, &self.today, &|w| self.weenie_name(w)))
    }

    /// The server's own spawn `guid` of landblock `lb` as override file `o` writes it: put in
    /// whole as the server has it (the instances it links too), unless the file has it.
    fn override_entry<'a>(
        &self,
        lb: u16,
        o: &'a mut Overrides,
        guid: u32,
    ) -> Result<&'a mut Override, Error> {
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
            e.insert(Override::Placed(i.clone(), self.retail_children(guid)));
        }
        o.entries
            .get_mut(&guid)
            .ok_or_else(|| Error::Refused(format!("no spawn {}", guid_text(guid))))
    }

    /// The server's own spawn `guid` of landblock `lb` as its world places it, with its links;
    /// `None` for a spawn the server does not place there.
    #[must_use]
    pub fn server_own_spawn(&self, lb: u16, guid: u32) -> Option<ServerSpawn> {
        let instance = self
            .retail_instances
            .get(&lb)?
            .iter()
            .find(|i| sql_ids::number(&i.guid) == Some(guid))?
            .clone();
        let mut children = self.retail_links.get(&guid).cloned().unwrap_or_default();
        let mut parents: Vec<u32> = self
            .retail_links
            .iter()
            .filter(|(_, kids)| kids.contains(&guid))
            .map(|(p, _)| *p)
            .collect();
        // A database learned twice lists each link twice.
        children.sort_unstable();
        children.dedup();
        parents.sort_unstable();
        Some(ServerSpawn {
            instance,
            children,
            parents,
        })
    }

    /// The instances the server's own spawn `guid` links, as an override file writes them.
    fn retail_children(&self, guid: u32) -> Vec<String> {
        self.retail_links
            .get(&guid)
            .into_iter()
            .flatten()
            .map(|c| guid_text(*c))
            .collect()
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
        self.sources.text(&DiskStore, path)
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
        self.weenie_files = std::cell::OnceCell::new();
        let sql_dir = self.pack_dir().join("server").join("sql");
        self.pack_loot = std::cell::OnceCell::new();
        self.link_table = std::cell::OnceCell::new();
        self.pack_class_facts = std::cell::OnceCell::new();
        self.pack_weenie_facts = std::cell::OnceCell::new();
        for (path, text) in texts {
            self.sources.put(&DiskStore, path, text.clone());
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
            let cells_before = (self.compiled.cells.clone(), self.compiled.rooms.clone());
            compile::recompile_landblocks(
                &self.packs,
                &self.base,
                &mut self.compiled,
                &touched.landblocks,
            )?;
            // Cells come or gone: the SQL that names them is filled again.
            if touched.sql
                || (&self.compiled.cells, &self.compiled.rooms)
                    != (&cells_before.0, &cells_before.1)
            {
                compile::refresh_sql(
                    &self.world,
                    &self.packs,
                    &self.base,
                    &self.opened_lock,
                    &mut self.compiled,
                )?;
            }
            // A house's cells name its spawn by the id the SQL gave it.
            let stale = crate::drops::stale_houses(&self.compiled);
            if !stale.is_empty() {
                compile::recompile_landblocks(&self.packs, &self.base, &mut self.compiled, &stale)?;
                touched.landblocks.extend(stale);
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
        let start = self.history.len();
        let applied = self.apply_steps(op)?;
        self.history.merge_since(start);
        self.history.clear_redo();
        Ok(applied)
    }

    /// Apply `op` into the history's last step, so one undo takes both back: the steps of one
    /// stroke of a brush or one drag of a gizmo.
    ///
    /// # Errors
    /// As [`Self::apply`].
    pub fn apply_into_last(&mut self, op: &Op) -> Result<Applied, Error> {
        let start = self.history.len().saturating_sub(1);
        let applied = self.apply_steps(op)?;
        self.history.merge_since(start);
        self.history.clear_redo();
        Ok(applied)
    }

    fn apply_steps(&mut self, op: &Op) -> Result<Applied, Error> {
        match op {
            Op::MoveBuilding {
                landblock,
                index,
                at,
                rotation,
            } if self.dropped_building(*landblock, *index).is_some() => {
                return self.move_dropped(*landblock, *index, *at, *rotation);
            }
            Op::RemoveBuilding { landblock, index } => {
                return self.remove_dropped(*landblock, *index);
            }
            _ => {}
        }
        if let Op::Batch(ops) = op {
            let start = self.history.len();
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
                        while self.history.len() > start {
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
        self.history.push(Step {
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
            warnings: Vec::new(),
        })
    }

    /// Apply `ops` as one change of the files: each drafted over the text the ones before it
    /// left, then the landblocks they reach compiled once. They must not need the world compiled
    /// between them.
    pub(crate) fn apply_drafted(&mut self, ops: &[Op]) -> Result<Applied, Error> {
        let mut texts: BTreeMap<PathBuf, Option<String>> = BTreeMap::new();
        for op in ops {
            self.draft_into(op, &mut texts)?;
        }
        self.apply_texts(texts)
    }

    /// Change the files to `texts` as one step of the history.
    pub(crate) fn apply_texts(
        &mut self,
        texts: BTreeMap<PathBuf, Option<String>>,
    ) -> Result<Applied, Error> {
        if texts.is_empty() {
            return Ok(Applied::default());
        }
        let before: BTreeMap<PathBuf, Option<String>> =
            texts.keys().map(|p| (p.clone(), self.text(p))).collect();
        let touched = self.change(&texts)?;
        self.history.push(Step {
            before,
            after: texts,
        });
        Ok(Applied {
            touched,
            ..Applied::default()
        })
    }

    /// Take the history back to `len` steps, as a batch that failed partway is.
    pub(crate) fn roll_back_to(&mut self, len: usize) {
        let redo = self.history.counts().1;
        while self.history.len() > len {
            if self.undo_step().is_err() {
                break;
            }
        }
        self.history.truncate_redo(redo);
    }

    /// How many steps the history holds.
    pub(crate) fn steps(&self) -> usize {
        self.history.len()
    }

    fn undo_step(&mut self) -> Result<Touched, Error> {
        let Some(step) = self.history.take_undo() else {
            return Ok(Touched::default());
        };
        match self.change(&step.before) {
            Ok(t) => {
                self.history.undone(step);
                Ok(t)
            }
            Err(e) => {
                self.history.push(step);
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
        let Some(step) = self.history.take_redo() else {
            return Ok(Touched::default());
        };
        match self.change(&step.after) {
            Ok(t) => {
                self.history.push(step);
                Ok(t)
            }
            Err(e) => {
                self.history.undone(step);
                Err(e)
            }
        }
    }

    /// How many steps undo and redo can take.
    #[must_use]
    pub fn history(&self) -> (usize, usize) {
        self.history.counts()
    }

    /// Whether any file differs from the disk's.
    #[must_use]
    pub fn unsaved(&self) -> bool {
        self.sources.unsaved()
    }

    /// Write every file of the target pack the session changed whose text differs from the
    /// disk's (and remove one the session made and took away again), and the lockfile when its
    /// ids changed. Answers the files written or removed.
    ///
    /// # Errors
    /// A file that will not write.
    pub fn save(&mut self) -> Result<Vec<PathBuf>, Error> {
        let mut written = self
            .sources
            .save(&DiskStore)
            .map_err(|(path, e)| Error::io(&path, e))?;
        let lock_path = self.world.lock_path();
        if self.compiled.lock.save(&DiskStore, &lock_path)? {
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
        let (placed, spawned) = self.draft_into(op, &mut out)?;
        Ok((out, placed, spawned))
    }

    /// The file changes `op` makes over those `out` holds already, put into `out`; the static it
    /// places (landblock and symbol) and the spawn file it writes.
    #[allow(clippy::too_many_lines, clippy::type_complexity)]
    pub(crate) fn draft_into(
        &mut self,
        op: &Op,
        out: &mut BTreeMap<PathBuf, Option<String>>,
    ) -> Result<(Option<(u16, String)>, Option<PathBuf>), Error> {
        let mut placed = None;
        let mut spawned = None;
        match op {
            Op::PlaceStatic {
                landblock,
                model,
                at,
                rotation,
                name,
            } => {
                let path = self.edits_file(*landblock);
                let mut doc = self.doc(&path, out)?;
                let pack = self.packs[self.target].name.clone();
                // The names the file holds, as its text stands now.
                let used: BTreeSet<String> = tables(&mut doc, "static")
                    .iter()
                    .filter_map(|t| string(t, "name"))
                    .map(|n| format!("{pack}:{n}"))
                    .collect();
                let name = match name {
                    Some(n) if used.contains(&format!("{pack}:{n}")) => {
                        return Err(Error::Refused(format!(
                            "landblock {landblock:04X} has a static named {n} already"
                        )));
                    }
                    Some(n) => n.clone(),
                    None => (1..)
                        .map(|n| format!("static-{n}"))
                        .find(|n| !used.contains(&format!("{pack}:{n}")))
                        .unwrap_or_default(),
                };
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
                let mut doc = self.doc(&path, out)?;
                self.move_static(&mut doc, *landblock, which, at.as_ref(), *rotation)?;
                out.insert(path, Some(render(&doc)));
            }
            Op::RemoveStatic { landblock, which } => {
                let path = self.edits_file(*landblock);
                let mut doc = self.doc(&path, out)?;
                self.remove_static(&mut doc, *landblock, which)?;
                out.insert(path, Some(render(&doc)));
            }
            Op::SetVertices(sets) => {
                for s in sets {
                    self.set_vertex(out, *s)?;
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
                let mut doc = self.doc(&path, out)?;
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
            Op::AddCellStatic {
                landblock,
                cell,
                model,
                at,
                rotation,
            } => {
                let path = self.edits_file(*landblock);
                let mut doc = self.doc(&path, out)?;
                let key: Value = match self.cell_names(*landblock).get(cell) {
                    Some(s) => Value::from(s.clone()),
                    None => hex16(*cell),
                };
                let mut t = entry();
                t.insert("cell", Item::Value(key));
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
                push(&mut doc, "add-cell-static", t);
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
                let mut doc = self.doc(&path, out)?;
                self.cell_static(&mut doc, *landblock, *cell, *index, Some((*at, *rotation)))?;
                out.insert(path, Some(render(&doc)));
            }
            Op::RemoveCellStatic {
                landblock,
                cell,
                index,
            } => {
                let path = self.edits_file(*landblock);
                let mut doc = self.doc(&path, out)?;
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
                    None => match out.get(file) {
                        Some(Some(t)) => t.clone(),
                        _ => self.spawn_file(file)?,
                    },
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
                // What it sets off, and what sets it off, let go of it.
                let gone = self.spawn(file, guid);
                let now = self.current(file, out).unwrap_or_default();
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
                if let Some(s) = gone {
                    self.draft_unlinked(&s, out)?;
                }
            }
            Op::RemoveSpawn { file } => {
                let text = self.spawn_file(file)?;
                let placed = sql_ids::instances(&text);
                if placed.len() > 1 {
                    return Err(Error::Refused(format!(
                        "{} places {} spawns; it is taken away only whole, by hand",
                        file.display(),
                        placed.len()
                    )));
                }
                if let Some(s) = placed.first().and_then(|i| self.spawn(file, &i.guid)) {
                    self.draft_unlinked(&s, out)?;
                }
                out.insert(file.clone(), None);
            }
            Op::Link { file, guid, target } => {
                self.draft_link(file, guid, target.as_ref(), out)?;
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
                let mut doc = self.doc(&path, out)?;
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
                link_partners(&mut doc, &name, links, *landblock, &bare)?;
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
                let mut doc = self.doc(&path, out)?;
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
                unlink(&mut doc, &["cell"], &|one| one.starts_with(&here));
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
            Op::SetCellSurfaces {
                landblock,
                name,
                surfaces,
            } => {
                let path = self.edits_file(*landblock);
                let mut doc = self.doc(&path, out)?;
                let pack = self.packs[self.target].name.clone();
                let bare = name
                    .strip_prefix(&format!("{pack}:"))
                    .unwrap_or(name)
                    .to_owned();
                let Some(t) = tables(&mut doc, "cell")
                    .iter_mut()
                    .find(|t| string(t, "name") == Some(bare.as_str()))
                else {
                    return Err(Error::Refused(format!("this pack places no cell {bare}")));
                };
                let list = surfaces
                    .iter()
                    .map(|s| format!("0x{:08X}", s.raw()))
                    .collect::<Vec<_>>()
                    .join(", ");
                t.insert("surfaces", Item::Value(parse_value(&format!("[{list}]"))));
                out.insert(path, Some(render(&doc)));
            }
            Op::SetDungeon {
                landblock,
                name,
                look,
            } => {
                let path = self.edits_file(*landblock);
                let mut doc = self.doc(&path, out)?;
                let name = name.as_deref().map(str::trim).filter(|n| !n.is_empty());
                if name.is_none() && look.is_none() {
                    doc.remove("dungeon");
                } else {
                    let mut t = Table::new();
                    // Before the operations, at the head of the file.
                    t.set_position(Some(0));
                    if !doc.to_string().trim().is_empty() {
                        t.decor_mut().set_suffix("");
                    }
                    if let Some(n) = name {
                        t.insert("name", Item::Value(Value::from(n)));
                    }
                    if let Some(l) = look {
                        t.insert("look", Item::Value(hex16(*l)));
                    }
                    doc.insert("dungeon", Item::Table(t));
                }
                out.insert(path, Some(render(&doc)));
            }
            Op::RemoveCell { landblock, name } => {
                let path = self.edits_file(*landblock);
                let mut doc = self.doc(&path, out)?;
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
                unjoin(&mut doc, &bare);
                out.insert(path, Some(render(&doc)));
                // The spawns standing in it go with it.
                for s in self.spawns_in_cell(*landblock, name) {
                    let text = self.spawn_file(&s.file)?;
                    if sql_ids::instances(&text).len() > 1 {
                        return Err(Error::Refused(format!(
                            "{} places spawns in cell {bare} and elsewhere; it is taken away only whole, by hand",
                            s.file.display()
                        )));
                    }
                    out.insert(s.file.clone(), None);
                }
            }
            Op::PlacePieces {
                landblock,
                pieces,
                joins,
            } => {
                self.draft_pieces(*landblock, pieces, joins, out)?;
            }
            Op::TakeCells { landblock, cells } => {
                self.draft_take(*landblock, cells, out)?;
            }
            Op::DressCell {
                landblock,
                cell,
                surfaces,
            } => {
                self.draft_dress(*landblock, *cell, surfaces, out)?;
            }
            Op::MoveCell {
                landblock,
                name,
                at,
                rotation,
                links,
            } => {
                let path = self.edits_file(*landblock);
                let pack = self.packs[self.target].name.clone();
                let bare = |full: &str| {
                    full.strip_prefix(&format!("{pack}:"))
                        .unwrap_or(full)
                        .to_owned()
                };
                let me = bare(name);
                let old = self.packs[self.target]
                    .edits
                    .iter()
                    .filter(|e| e.landblock == *landblock)
                    .flat_map(|e| e.cells.iter())
                    .find(|c| c.item.name == *name || bare(&c.item.name) == me)
                    .map(|c| c.item.clone())
                    .ok_or_else(|| Error::Refused(format!("this pack places no cell {me}")))?;
                let from = Frame::new(
                    Vec3::new(old.at[0], old.at[1], old.at[2]),
                    compile::quat_of(old.rotation),
                );
                let to = Frame::new(Vec3::new(at[0], at[1], at[2]), compile::quat_of(*rotation));
                let carry = |p: Vec3| {
                    dereth_primitives::frame::localtoglobal(
                        &to,
                        dereth_primitives::frame::globaltolocal(&from, p),
                    )
                };
                let turn = heading_of(to.rotation) - heading_of(from.rotation);
                let mut doc = self.doc(&path, out)?;
                unjoin(&mut doc, &me);
                let Some(t) = tables(&mut doc, "cell")
                    .iter_mut()
                    .find(|t| string(t, "name") == Some(me.as_str()))
                else {
                    return Err(Error::Refused(format!("this pack places no cell {me}")));
                };
                t.insert(
                    "at",
                    Item::Value(parse_value(&format!(
                        "[{}, {}, {}]",
                        meters(at[0]),
                        meters(at[1]),
                        meters(at[2])
                    ))),
                );
                set_rotation(t, *rotation);
                let mut arr = toml_edit::Array::new();
                for l in links {
                    arr.push(link_text(l, &bare).as_str());
                }
                t.insert("portals", Item::Value(Value::Array(arr)));
                // Its statics, by the same move.
                if let Some(aot) = t.get_mut("static").and_then(Item::as_array_of_tables_mut) {
                    for (st, was) in aot.iter_mut().zip(&old.statics) {
                        let p = carry(Vec3::new(was.at[0], was.at[1], was.at[2]));
                        st.insert(
                            "at",
                            Item::Value(parse_value(&format!(
                                "[{}, {}, {}]",
                                meters(p.x),
                                meters(p.y),
                                meters(p.z)
                            ))),
                        );
                        let h = heading_of(compile::quat_of(was.rotation)) + turn;
                        set_rotation(st, Rotation::Heading(h.rem_euclid(360.0)));
                    }
                }
                link_partners(&mut doc, &me, links, *landblock, &bare)?;
                out.insert(path, Some(render(&doc)));
                // Its spawns, by the same move.
                for s in self.spawns_in_cell(*landblock, name) {
                    let text = match out.get(&s.file) {
                        Some(Some(t)) => t.clone(),
                        _ => self.spawn_file(&s.file)?,
                    };
                    let p = carry(s.frame.origin);
                    let h = (heading_of(s.frame.rotation) + turn).rem_euclid(360.0);
                    out.insert(
                        s.file.clone(),
                        Some(Self::moved_spawn(&s.file, &text, &s.guid, p, h)?),
                    );
                }
            }
            Op::MoveBuilding {
                landblock,
                index,
                at,
                rotation,
            } => {
                let path = self.edits_file(*landblock);
                let mut doc = self.doc(&path, out)?;
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
            Op::AddBuilding {
                landblock,
                building,
            } => {
                let path = self.edits_file(*landblock);
                let mut doc = self.doc(&path, out)?;
                let pack = self.packs[self.target].name.clone();
                let row = building_row(building, &pack);
                let bare = string(&row, "name").unwrap_or_default().to_owned();
                // A drop of that name written anew where it is, its comments kept.
                let found = tables(&mut doc, "add-building")
                    .iter()
                    .position(|t| string(t, "name") == Some(bare.as_str()));
                match found {
                    Some(k) => {
                        let t = tables(&mut doc, "add-building")
                            .get_mut(k)
                            .expect("found above");
                        let decor = t.decor().clone();
                        *t = row;
                        *t.decor_mut() = decor;
                    }
                    None => push(&mut doc, "add-building", row),
                }
                out.insert(path, Some(render(&doc)));
            }
            Op::AddDungeon { landblock, dungeon } => {
                let path = self.edits_file(*landblock);
                let mut doc = self.doc(&path, out)?;
                let pack = self.packs[self.target].name.clone();
                let row = dungeon_row(dungeon, &pack);
                let bare = string(&row, "name").unwrap_or_default().to_owned();
                let found = tables(&mut doc, "add-dungeon")
                    .iter()
                    .position(|t| string(t, "name") == Some(bare.as_str()));
                match found {
                    Some(k) => {
                        let t = tables(&mut doc, "add-dungeon")
                            .get_mut(k)
                            .expect("found above");
                        let decor = t.decor().clone();
                        *t = row;
                        *t.decor_mut() = decor;
                    }
                    None => push(&mut doc, "add-dungeon", row),
                }
                out.insert(path, Some(render(&doc)));
            }
            Op::UnsetVertices(list) => {
                for (gx, gy, field) in list {
                    self.unset_vertex(out, *gx, *gy, *field)?;
                }
            }
            Op::RemoveBuilding { .. } | Op::Batch(_) => {
                unreachable!("a batch, and the taking away of a building, are applied op by op")
            }
        }
        Ok((placed, spawned))
    }

    /// The cell rows `pieces` make in landblock `lb`'s file, in `out`: each part a new cell, named
    /// `cell-n` by the first numbers no cell of the landblock has held, its links as given and
    /// `joins` linking parts placed together; the cells and reshaped cells they join linked back.
    fn draft_pieces(
        &mut self,
        lb: u16,
        pieces: &[NewPiece],
        joins: &[(PartPortal, PartPortal)],
        out: &mut BTreeMap<PathBuf, Option<String>>,
    ) -> Result<(), Error> {
        let path = self.edits_file(lb);
        let mut doc = self.doc(&path, out)?;
        let pack = self.packs[self.target].name.clone();
        let bare = |full: &str| {
            full.strip_prefix(&format!("{pack}:"))
                .unwrap_or(full)
                .to_owned()
        };
        let mut used: BTreeSet<String> = tables(&mut doc, "cell")
            .iter()
            .filter_map(|t| string(t, "name").map(str::to_owned))
            .collect();
        let held = |n: &str| {
            self.compiled
                .lock
                .get(
                    symbol::Kind::Cell,
                    &compile::cell_symbol(&format!("{pack}:{n}"), lb),
                )
                .is_some()
        };
        let mut next = 1usize;
        let mut names: Vec<Vec<String>> = Vec::new();
        for p in pieces {
            let mut these = Vec::new();
            for _ in &p.parts {
                let name = loop {
                    let n = format!("cell-{next}");
                    next += 1;
                    if !used.contains(&n) && !held(&n) {
                        break n;
                    }
                };
                used.insert(name.clone());
                these.push(name);
            }
            names.push(these);
        }
        let mut links: Vec<Vec<Vec<source::Link>>> = pieces
            .iter()
            .map(|p| p.parts.iter().map(|(_, l)| l.clone()).collect())
            .collect();
        let has = |links: &Vec<Vec<Vec<source::Link>>>, (k, j, s): PartPortal| {
            links
                .get(k)
                .and_then(|v| v.get(j))
                .is_some_and(|v| s < v.len())
        };
        // A portal joined to several of the parts placed joins each through part of it.
        let join = |l: &mut source::Link, to: source::Link| {
            *l = match std::mem::replace(l, source::Link::Shut) {
                source::Link::Shut => to,
                source::Link::Many(mut all) => {
                    all.push(to);
                    source::Link::Many(all)
                }
                one => source::Link::Many(vec![one, to]),
            };
        };
        for (a, b) in joins {
            if !has(&links, *a) || !has(&links, *b) {
                return Err(Error::Refused(
                    "a join names a portal the pieces placed have not".into(),
                ));
            }
            join(
                &mut links[a.0][a.1][a.2],
                source::Link::Cell {
                    cell: format!("{pack}:{}", names[b.0][b.1]),
                    portal: b.2,
                },
            );
            join(
                &mut links[b.0][b.1][b.2],
                source::Link::Cell {
                    cell: format!("{pack}:{}", names[a.0][a.1]),
                    portal: a.2,
                },
            );
        }
        let new: BTreeSet<String> = names.iter().flatten().cloned().collect();
        for (k, p) in pieces.iter().enumerate() {
            for (j, (part, _)) in p.parts.iter().enumerate() {
                let mut t = entry();
                t.insert("name", Item::Value(Value::from(names[k][j].clone())));
                t.insert("environment", Item::Value(hex(p.environment.raw())));
                t.insert("piece", Item::Value(Value::from(i64::from(*part))));
                let list = p
                    .surfaces
                    .iter()
                    .map(|s| format!("0x{:08X}", s.raw()))
                    .collect::<Vec<_>>()
                    .join(", ");
                t.insert("surfaces", Item::Value(parse_value(&format!("[{list}]"))));
                t.insert(
                    "at",
                    Item::Value(parse_value(&format!(
                        "[{}, {}, {}]",
                        meters(p.at[0]),
                        meters(p.at[1]),
                        meters(p.at[2])
                    ))),
                );
                set_rotation(&mut t, p.rotation);
                let mut arr = toml_edit::Array::new();
                for l in &links[k][j] {
                    arr.push(link_text(l, &bare).as_str());
                }
                t.insert("portals", Item::Value(Value::Array(arr)));
                push(&mut doc, "cell", t);
            }
        }
        // The cells and reshaped cells already there that the new parts join, linked back.
        for (k, p) in pieces.iter().enumerate() {
            for j in 0..p.parts.len() {
                let placed_here = |l: &source::Link| matches!(l, source::Link::Cell { cell, .. } if new.contains(&bare(cell)));
                let outside: Vec<source::Link> = links[k][j]
                    .iter()
                    .map(|l| match l {
                        l if placed_here(l) => source::Link::Shut,
                        source::Link::Many(all) => {
                            let rest: Vec<source::Link> =
                                all.iter().filter(|l| !placed_here(l)).cloned().collect();
                            match rest.len() {
                                0 => source::Link::Shut,
                                1 => rest[0].clone(),
                                _ => source::Link::Many(rest),
                            }
                        }
                        other => other.clone(),
                    })
                    .collect();
                link_partners(&mut doc, &names[k][j], &outside, lb, &bare)?;
            }
        }
        out.insert(path, Some(render(&doc)));
        Ok(())
    }

    /// The links a row reshaping one of the world's own cells, `cell` as compiled now, writes to
    /// keep its ways: `keep` where a portal of its piece leads somewhere but into `closing`, shut
    /// elsewhere.
    fn keeping(
        &self,
        cell: &dereth_assets::world::EnvCell,
        closing: &BTreeSet<u16>,
    ) -> Vec<String> {
        let Some(env) = self.environment(cell.environment) else {
            return Vec::new();
        };
        let Some(cs) = env.cells.get(usize::from(cell.cell_struct)) else {
            return Vec::new();
        };
        cs.portals
            .iter()
            .map(|poly| {
                let way = cell.portals.iter().find(|p| p.polygon_id == *poly);
                match way {
                    Some(p)
                        if p.flags & 4 != 0
                            || !u16::try_from(p.other_cell_id)
                                .is_ok_and(|o| closing.contains(&o)) =>
                    {
                        "keep".to_owned()
                    }
                    _ => String::new(),
                }
            })
            .collect()
    }

    /// A `[[replace-cell]]` row reshaping one of the world's own cells, `cell` (numbered `n`), as
    /// it stands: its piece and its place, drawn with `surfaces`, its portals linked `portals`.
    fn standing_row(
        n: u16,
        cell: &dereth_assets::world::EnvCell,
        surfaces: &[DataId],
        portals: &[String],
    ) -> Table {
        let mut t = entry();
        t.insert("cell", Item::Value(hex16(n)));
        t.insert("environment", Item::Value(hex(cell.environment.raw())));
        t.insert(
            "piece",
            Item::Value(Value::from(i64::from(cell.cell_struct))),
        );
        let list = surfaces
            .iter()
            .map(|s| format!("0x{:08X}", s.raw()))
            .collect::<Vec<_>>()
            .join(", ");
        t.insert("surfaces", Item::Value(parse_value(&format!("[{list}]"))));
        let o = cell.frame.origin;
        t.insert(
            "at",
            Item::Value(parse_value(&format!(
                "[{}, {}, {}]",
                meters(o.x),
                meters(o.y),
                meters(o.z)
            ))),
        );
        let q = cell.frame.rotation;
        set_rotation(&mut t, Rotation::Quat([q.w, q.x, q.y, q.z]));
        let mut arr = toml_edit::Array::new();
        for p in portals {
            arr.push(p.as_str());
        }
        t.insert("portals", Item::Value(Value::Array(arr)));
        t
    }

    /// The changes taking landblock `lb`'s own cells `cells` away makes, in `out`.
    #[allow(clippy::too_many_lines)]
    fn draft_take(
        &mut self,
        lb: u16,
        cells: &[u16],
        out: &mut BTreeMap<PathBuf, Option<String>>,
    ) -> Result<(), Error> {
        let taken: BTreeSet<u16> = cells.iter().copied().collect();
        let mine: BTreeSet<u16> = self.cell_names(lb).keys().copied().collect();
        if let Some(n) = taken.iter().find(|n| mine.contains(n)) {
            return Err(Error::Refused(format!(
                "cell {lb:04X}{n:04X} is the pack's own: it is taken away by its row"
            )));
        }
        let now = self.cells(lb);
        if let Some(n) = taken.iter().find(|n| !now.contains_key(n)) {
            return Err(Error::Refused(format!(
                "landblock {lb:04X} has no cell {n:04X}"
            )));
        }
        let path = self.edits_file(lb);
        let mut doc = self.doc(&path, out)?;
        let num = |t: &Table| -> Option<u16> {
            t.get("cell")
                .and_then(Item::as_integer)
                .and_then(|n| u16::try_from(n).ok())
        };
        // What the pack did to them goes: their reshaping, and the statics it moved, took away
        // or added in them.
        for key in [
            "replace-cell",
            "move-cell-static",
            "remove-cell-static",
            "add-cell-static",
        ] {
            if !doc.contains_key(key) {
                continue;
            }
            loop {
                let k = tables(&mut doc, key)
                    .iter()
                    .position(|t| num(t).is_some_and(|n| taken.contains(&n)));
                match k {
                    Some(k) => remove_table(&mut doc, key, k),
                    None => break,
                }
            }
        }
        // The pack's own cells joined to them, shut there.
        let heads: Vec<String> = taken.iter().map(|n| format!("0x{n:04X}:")).collect();
        unlink(&mut doc, &["cell"], &|one| {
            heads.iter().any(|h| one.starts_with(h.as_str()))
        });
        // The world's own cells around them: reshaped as they stand, their ways into them free.
        for (n, cell) in &now {
            if taken.contains(n) || mine.contains(n) {
                continue;
            }
            let into: Vec<u16> = cell
                .portals
                .iter()
                .filter(|p| {
                    p.flags & 4 == 0
                        && u16::try_from(p.other_cell_id).is_ok_and(|o| taken.contains(&o))
                })
                .map(|p| p.polygon_id)
                .collect();
            if into.is_empty() {
                continue;
            }
            let ways = self.keeping(cell, &taken);
            let at = doc
                .contains_key("replace-cell")
                .then(|| {
                    tables(&mut doc, "replace-cell")
                        .iter()
                        .position(|t| num(t) == Some(*n))
                })
                .flatten();
            match at {
                Some(k) => {
                    // Reshaped already: the portals that kept those ways shut.
                    let slots: Vec<usize> = self
                        .environment(cell.environment)
                        .and_then(|e| e.cells.get(usize::from(cell.cell_struct)).cloned())
                        .map(|cs| {
                            into.iter()
                                .filter_map(|poly| cs.portals.iter().position(|q| q == poly))
                                .collect()
                        })
                        .unwrap_or_default();
                    let row = tables(&mut doc, "replace-cell")
                        .get_mut(k)
                        .expect("found above");
                    for s in slots {
                        if let Some(v) = port(row, s) {
                            *v = Value::from("");
                        }
                    }
                }
                None => {
                    let row = Self::standing_row(*n, cell, &cell.surfaces, &ways);
                    push(&mut doc, "replace-cell", row);
                }
            }
        }
        for n in &taken {
            let exists = doc.contains_key("remove-cell")
                && tables(&mut doc, "remove-cell")
                    .iter()
                    .any(|t| num(t) == Some(*n));
            if !exists {
                let mut t = entry();
                t.insert("cell", Item::Value(hex16(*n)));
                push(&mut doc, "remove-cell", t);
            }
        }
        out.insert(path, Some(render(&doc)));
        // The spawns standing in them: the packs' taken away with their files, the server's own
        // taken away in the landblock's override file.
        let ids: BTreeSet<u32> = taken
            .iter()
            .map(|n| (u32::from(lb) << 16) | u32::from(*n))
            .collect();
        for s in self.spawns() {
            if s.server || s.pack != self.target || !ids.contains(&s.cell) {
                continue;
            }
            let text = match out.get(&s.file) {
                Some(Some(t)) => t.clone(),
                Some(None) => continue,
                None => self.spawn_file(&s.file)?,
            };
            if sql_ids::instances(&text).len() > 1 {
                return Err(Error::Refused(format!(
                    "{} places spawns in cell {lb:04X}{:04X} and elsewhere; it is taken away \
                     only whole, by hand",
                    s.file.display(),
                    s.cell & 0xFFFF
                )));
            }
            out.insert(s.file.clone(), None);
        }
        let guids: Vec<(u32, bool)> = self
            .retail_instances
            .get(&lb)
            .into_iter()
            .flatten()
            .filter(|i| ids.contains(&i.cell))
            .filter_map(|i| sql_ids::number(&i.guid).map(|g| (g, i.link_child)))
            .collect();
        if !guids.is_empty() {
            let file = self.override_file(lb);
            let now = match out.get(&file) {
                Some(t) => t.clone().unwrap_or_default(),
                None => self.text(&file).unwrap_or_default(),
            };
            let mut o = Overrides::read(&now);
            for (g, link_child) in guids {
                o.entries.insert(g, Override::Removed { link_child });
            }
            out.insert(
                file,
                Some(o.render(lb, &self.today, &|w| self.weenie_name(w))),
            );
        }
        Ok(())
    }

    /// The change drawing cell `cell` of landblock `lb` with `surfaces` makes, in `out`.
    fn draft_dress(
        &mut self,
        lb: u16,
        cell: u16,
        surfaces: &[DataId],
        out: &mut BTreeMap<PathBuf, Option<String>>,
    ) -> Result<(), Error> {
        let path = self.edits_file(lb);
        let mut doc = self.doc(&path, out)?;
        let pack = self.packs[self.target].name.clone();
        let list = surfaces
            .iter()
            .map(|s| format!("0x{:08X}", s.raw()))
            .collect::<Vec<_>>()
            .join(", ");
        let value = parse_value(&format!("[{list}]"));
        let num = |t: &Table| -> Option<u16> {
            t.get("cell")
                .and_then(Item::as_integer)
                .and_then(|n| u16::try_from(n).ok())
        };
        if let Some(name) = self.cell_names(lb).get(&cell).cloned() {
            let bare = name
                .strip_prefix(&format!("{pack}:"))
                .unwrap_or(&name)
                .to_owned();
            let Some(t) = tables(&mut doc, "cell")
                .iter_mut()
                .find(|t| string(t, "name") == Some(bare.as_str()))
            else {
                return Err(Error::Refused(format!("this pack places no cell {bare}")));
            };
            t.insert("surfaces", Item::Value(value));
        } else {
            let row = doc
                .contains_key("replace-cell")
                .then(|| {
                    tables(&mut doc, "replace-cell")
                        .iter()
                        .position(|t| num(t) == Some(cell))
                })
                .flatten();
            match row {
                Some(k) => {
                    tables(&mut doc, "replace-cell")
                        .get_mut(k)
                        .expect("found above")
                        .insert("surfaces", Item::Value(value));
                }
                None => {
                    let now = self.cells(lb);
                    let c = now.get(&cell).ok_or_else(|| {
                        Error::Refused(format!("landblock {lb:04X} has no cell {cell:04X}"))
                    })?;
                    let ways = self.keeping(c, &BTreeSet::new());
                    let row = Self::standing_row(cell, c, surfaces, &ways);
                    push(&mut doc, "replace-cell", row);
                }
            }
        }
        out.insert(path, Some(render(&doc)));
        Ok(())
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
        // One a pack adds to the cell: its own row, changed or taken out.
        if origin & compile::ADDED != 0 {
            let id = (u32::from(lb) << 16) | u32::from(cell);
            let (p, row) = self
                .compiled
                .cell_static_adds
                .get(&id)
                .and_then(|v| v.get(origin & !compile::ADDED))
                .copied()
                .ok_or_else(|| Error::Refused(format!("cell {cell:04X} has no static {index}")))?;
            if p != self.target {
                return Err(Error::Refused(format!(
                    "static {index} of cell {cell:04X} is {}'s; it is changed there",
                    self.packs[p].name
                )));
            }
            match to {
                Some((at, rotation)) => {
                    let Some(t) = tables(doc, "add-cell-static").get_mut(row) else {
                        return Err(Error::Refused(format!(
                            "cell {cell:04X} has no static {index}"
                        )));
                    };
                    t.insert("at", Item::Value(at_value(at)));
                    if let Some(r) = rotation {
                        set_rotation(t, r);
                    }
                }
                None => remove_table(doc, "add-cell-static", row),
            }
            return Ok(());
        }
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
            .and_then(|b| self.base.decode::<LandblockInfo>(DataId(id), &b).ok())
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
        let info = self
            .base
            .decode::<LandblockInfo>(
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
        let key = compile::field_name(s.field);
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

    /// Take the target pack's setting of `field` of world vertex `(gx, gy)` away, from every row
    /// of its files that sets that one vertex; a row left setting nothing goes.
    fn unset_vertex(
        &mut self,
        out: &mut BTreeMap<PathBuf, Option<String>>,
        gx: u32,
        gy: u32,
        field: Field,
    ) -> Result<(), Error> {
        let key = compile::field_name(field);
        let files: BTreeSet<PathBuf> = self.packs[self.target]
            .edits
            .iter()
            .map(|e| e.file.clone())
            .chain(
                out.keys()
                    .filter(|p| source::landblock_of_file(p).is_ok())
                    .cloned(),
            )
            .collect();
        for path in files {
            let lb = source::landblock_of_file(&path)?;
            let (bx, by) = (u32::from(lb >> 8) * 8, u32::from(lb & 0xFF) * 8);
            let mut doc = self.doc(&path, out)?;
            let Some(rows) = doc.get_mut("vertex").and_then(Item::as_array_of_tables_mut) else {
                continue;
            };
            let at = |t: &Table| -> Option<(u32, u32)> {
                let a = t.get("at").and_then(Item::as_array)?;
                let x = u32::try_from(a.get(0)?.as_integer()?).ok()?;
                let y = u32::try_from(a.get(1)?.as_integer()?).ok()?;
                Some((bx + x, by + y))
            };
            let mut changed = false;
            let mut emptied = Vec::new();
            for (k, t) in rows.iter_mut().enumerate() {
                if t.contains_key("to") || at(t) != Some((gx, gy)) || !t.contains_key(key) {
                    continue;
                }
                t.remove(key);
                changed = true;
                if Field::TABLE.iter().all(|f| !t.contains_key(f.name)) {
                    emptied.push(k);
                }
            }
            if !changed {
                continue;
            }
            for k in emptied.into_iter().rev() {
                remove_table(&mut doc, "vertex", k);
            }
            out.insert(path, Some(render(&doc)));
        }
        Ok(())
    }

    /// The rows of the target pack's file of landblock `lb` that add a static to one of `rooms`
    /// or move one there, carried by the move of their building from `from` to `to`.
    pub(crate) fn carry_room_statics(
        &mut self,
        out: &mut BTreeMap<PathBuf, Option<String>>,
        lb: u16,
        rooms: &BTreeSet<u16>,
        from: &Frame,
        to: &Frame,
    ) -> Result<(), Error> {
        if rooms.is_empty() {
            return Ok(());
        }
        let path = self.edits_file(lb);
        let mut doc = self.doc(&path, out)?;
        let mut changed = false;
        for key in ["add-cell-static", "move-cell-static"] {
            let Some(rows) = doc.get_mut(key).and_then(Item::as_array_of_tables_mut) else {
                continue;
            };
            for t in rows.iter_mut() {
                let in_room = int(t, "cell")
                    .and_then(|c| u16::try_from(c).ok())
                    .is_some_and(|c| rooms.contains(&c));
                let at = t.get("at").and_then(Item::as_array).and_then(|a| {
                    let n = |i: usize| {
                        a.get(i)
                            .and_then(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64)))
                    };
                    Some([n(0)?, n(1)?, n(2)?])
                });
                let (true, Some([x, y, z])) = (in_room, at) else {
                    continue;
                };
                #[allow(clippy::cast_possible_truncation)]
                let was = Frame::new(
                    Vec3::new(x as f32, y as f32, z as f32),
                    compile::quat_of(row_rotation(t)),
                );
                let now = crate::cells::carried(&was, from, to);
                t.insert(
                    "at",
                    Item::Value(parse_value(&format!(
                        "[{}, {}, {}]",
                        meters(now.origin.x),
                        meters(now.origin.y),
                        meters(now.origin.z)
                    ))),
                );
                set_rotation(t, upright(now.rotation));
                changed = true;
            }
        }
        if changed {
            out.insert(path, Some(render(&doc)));
        }
        Ok(())
    }

    /// Dropped building `bare`'s row taken out of the target pack's file of landblock `lb`, with
    /// the rows that add, move or take away statics in its rooms `rooms`.
    pub(crate) fn unwrite_drop(
        &mut self,
        out: &mut BTreeMap<PathBuf, Option<String>>,
        lb: u16,
        bare: &str,
        rooms: &BTreeSet<u16>,
    ) -> Result<(), Error> {
        let path = self.edits_file(lb);
        let mut doc = self.doc(&path, out)?;
        let k = tables(&mut doc, "add-building")
            .iter()
            .position(|t| string(t, "name") == Some(bare))
            .ok_or_else(|| {
                Error::Refused(format!(
                    "landblock {lb:04X}'s file drops no building {bare}"
                ))
            })?;
        remove_table(&mut doc, "add-building", k);
        unwrite_cell_statics(&mut doc, rooms);
        if doc
            .get("add-building")
            .and_then(Item::as_array_of_tables)
            .is_some_and(ArrayOfTables::is_empty)
        {
            doc.remove("add-building");
        }
        out.insert(path, Some(render(&doc)));
        Ok(())
    }

    /// Copied dungeon `bare`'s row taken out of the target pack's file of landblock `lb`, with
    /// the rows that add, move or take away statics in its cells `cells`.
    pub(crate) fn unwrite_dungeon(
        &mut self,
        out: &mut BTreeMap<PathBuf, Option<String>>,
        lb: u16,
        bare: &str,
        cells: &BTreeSet<u16>,
    ) -> Result<(), Error> {
        let path = self.edits_file(lb);
        let mut doc = self.doc(&path, out)?;
        let k = tables(&mut doc, "add-dungeon")
            .iter()
            .position(|t| string(t, "name") == Some(bare))
            .ok_or_else(|| {
                Error::Refused(format!(
                    "landblock {lb:04X}'s file copies no dungeon {bare}"
                ))
            })?;
        remove_table(&mut doc, "add-dungeon", k);
        unwrite_cell_statics(&mut doc, cells);
        if doc
            .get("add-dungeon")
            .and_then(Item::as_array_of_tables)
            .is_some_and(ArrayOfTables::is_empty)
        {
            doc.remove("add-dungeon");
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
        let indoor = named || cell & 0xFFFF >= 0x100;
        if !indoor && (!(0.0..192.0).contains(&origin.x) || !(0.0..192.0).contains(&origin.y)) {
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
                    && !self.sources.holds(&dir.join(format!("{lb:04X} {n}.sql")))
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
            let Ok(mut cell) = self
                .base
                .decode::<dereth_assets::world::EnvCell>(DataId(id), &bytes)
            else {
                continue;
            };
            cell.static_objects.extend(extra);
            cell.flags |= 2;
            let Ok(bytes) = cell.encode_for(self.base.era) else {
                continue;
            };
            records
                .entry(ModernDat::Cell)
                .or_default()
                .insert(id, Staged { bytes, version });
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
            let Ok(bytes) = info.encode_for(self.base.era) else {
                continue;
            };
            let cells = records.entry(ModernDat::Cell).or_default();
            cells.insert(lbi, Staged { bytes, version });
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
                    if let Ok(bytes) = t.encode_for(self.base.era) {
                        cells.insert(id, Staged { bytes, version });
                    }
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
        report
            .warnings
            .extend(self.compiled.warnings.values().flatten().cloned());
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
            loot_tables: learned.then(|| {
                let packs = self.pack_loot();
                self.retail_treasure
                    .keys()
                    .chain(self.retail_wielded.keys())
                    .chain(packs.death.keys())
                    .chain(packs.wielded.keys())
                    .copied()
                    .collect()
            }),
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
        // The buildings the packs moved, and those they dropped.
        let mut moved: BTreeMap<u16, BTreeSet<usize>> = BTreeMap::new();
        for p in &self.packs {
            for e in &p.edits {
                for m in &e.buildings {
                    moved.entry(e.landblock).or_default().insert(m.item.index);
                }
            }
        }
        for (lb, list) in &self.compiled.dropped {
            moved
                .entry(*lb)
                .or_default()
                .extend(list.iter().map(|(_, i)| *i));
        }
        let (problems, warnings) = crate::drops::checks(self, self.buildings.as_ref());
        report.problems.extend(problems);
        report.warnings.extend(warnings);
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
