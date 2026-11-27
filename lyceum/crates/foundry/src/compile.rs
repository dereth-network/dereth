//! The compiler: a world's packs, over the data files on this machine, into the world's one
//! overlay and its server SQL.
//!
//! [`compile`] works in memory and writes nothing; [`build`] runs it, writes the overlay into a
//! fresh folder, checks the result ([`crate::validate`]) and only then puts the overlay, the SQL
//! and the lockfile in place. In order:
//!
//! 1. The imported overlays are laid down first, in order, as if they were part of the base.
//! 2. Every symbol a pack defines or its SQL names is numbered ([`number`]), keeping the lockfile's.
//! 3. New records are encoded; a picture is read from its PNG.
//! 4. The vertex edits of every pack are gathered over the whole world first, because a vertex on
//!    a landblock's edge is one vertex with a copy in each neighbor: every copy is written.
//! 5. Each landblock's statics: the base's removed and moved, the packs' appended in load order.
//!    A static placed on the ground stands on the edited terrain. A landblock that had no record
//!    of its statics gets one, and its terrain record says so.
//! 6. A record whose bytes come out as the base's is left to the base.
//!
//! **Conflicts** ([`crate::conflict`]): where sources set one thing differently (a record two
//! packs replace, one field of one vertex, a static or building, a weenie class, a spawn, a
//! quest), the source the world's overrides name stands. With none, the conflict is kept in
//! [`Compiled::settled`] and the last source stands, so an editor can still show the world; a
//! build refuses it, with every conflict in one report.
//!
//! **The same bytes every time:** every record goes in in id order into fresh containers, every
//! date is 0, a replaced record keeps its base version and a new one is version 3 (as a patched
//! record is marked), and every record and the overlay's iteration list carry one revision, the
//! base file's count plus one.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use dereth_assets::world::{CellLandblock, LandblockInfo, ObjectEntry};
use dereth_assets::{Region, Surface, SurfaceTexture};
use dereth_dat::overlay::{container_name, OverlayDir, OverlayWriter};
use dereth_dat::{decompose, ModernDat};
use dereth_primitives::land_grid::LandGrid;
use dereth_primitives::{DataId, Frame, Quat, Vec3};

use crate::conflict::{self, Claim, Item, Override, Settled, SqlKey, SqlSet};
use crate::dats::{files_of, DatSet};
use crate::error::{Error, Loc};
use crate::lock::Lock;
use crate::source::{
    self, CellRef, Height, LandblockEdits, Located, MoveBuilding, MoveStatic, NewRecord, Pack,
    PixelFormat, Place, RecordRef, Rotation, Target, Which,
};
use crate::sql::{self, Token};
use crate::symbol::Kind;
use crate::world::World;

/// The version a new record is stamped with, as a patched record is marked.
pub const NEW_RECORD_VERSION: u16 = 3;

/// The region record, whose height table turns a vertex's index into meters.
pub const REGION: DataId = DataId(0x1300_0000);

/// The region of the player's own files `base`, under no import: the ground a template of theirs
/// stands on is made with it (a converted land's region lays its land on another grid).
#[must_use]
pub fn files_region(base: &DatSet) -> Option<Region> {
    let bytes = base.file(ModernDat::Portal)?.read(REGION).ok()?;
    base.decode::<Region>(REGION, &bytes).ok()
}

/// One record of the overlay: its bytes and its version.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Staged {
    pub bytes: std::sync::Arc<[u8]>,
    pub version: u16,
}

/// Where one static of a compiled statics record came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StaticOrigin {
    /// The base's own, by its place in the base record (moved or not).
    Base(usize),
    /// A pack's `[[static]]`: the pack, by its place in the world's list, its symbol when it has
    /// one, and its place among the landblock file's statics.
    Pack {
        pack: usize,
        name: Option<String>,
        entry: usize,
    },
}

/// The records of each file, by id.
pub type Records = BTreeMap<ModernDat, BTreeMap<u32, Staged>>;

/// Each file's deletions, `(id, mask)`: an id alone when the mask is 0, else every id whose bits
/// under the mask are the id's.
pub type Deletions = BTreeMap<ModernDat, BTreeSet<(u32, u32)>>;

/// Whether one of file `t`'s deletions in `gone` covers an id, its ids and families gathered once.
fn covered(gone: &Deletions, t: ModernDat) -> impl Fn(u32) -> bool {
    let mut single = std::collections::HashSet::new();
    let mut families: BTreeMap<u32, std::collections::HashSet<u32>> = BTreeMap::new();
    for &(id, mask) in gone.get(&t).into_iter().flatten() {
        if mask == 0 {
            single.insert(id);
        } else {
            families.entry(mask).or_default().insert(id & mask);
        }
    }
    move |id| {
        single.contains(&id)
            || families
                .iter()
                .any(|(mask, ids)| ids.contains(&(id & mask)))
    }
}

/// The mark of a cell's static a pack adds, in [`Compiled::cell_statics`].
pub const ADDED: usize = 1 << 30;

/// A world compiled in memory.
#[derive(Clone, Debug, Default)]
pub struct Compiled {
    /// The records of each file, by id.
    pub records: Records,
    /// The records the imports hold, by file and id: the layer the packs are compiled over, and
    /// part of [`Self::records`] where no pack changes them.
    pub imported: Records,
    /// For each landblock whose statics the packs change, where each static of its compiled
    /// statics record came from, in the record's order.
    pub statics: BTreeMap<u16, Vec<StaticOrigin>>,
    /// Each file's deletions, `(id, mask)`, from the imports.
    pub tombstones: Deletions,
    /// The SQL files, by their path under the output's `sql` folder, with the ids filled in.
    pub sql: BTreeMap<PathBuf, String>,
    /// For each landblock with new cells, the cell each new cell's symbol was given.
    pub cells: BTreeMap<u16, BTreeMap<String, u16>>,
    /// For each landblock with dropped buildings, the cell each of their rooms was given, by the
    /// room's name ([`crate::drops::room_name`]).
    pub rooms: BTreeMap<u16, BTreeMap<String, u16>>,
    /// For each landblock with dropped buildings, each one's name and its place in the compiled
    /// statics record's building list (a set's buildings each).
    pub dropped: BTreeMap<u16, Vec<(String, usize)>>,
    /// For each landblock whose drops are houses, the id its cells and its restriction table
    /// were given for each house's spawn, by the spawn's symbol.
    pub houses: BTreeMap<u16, BTreeMap<String, u32>>,
    /// The lockfile, with every new symbol numbered.
    pub lock: Lock,
    /// The symbols numbered in this build, in the order they were numbered.
    pub numbered: Vec<(Kind, String, u32)>,
    /// One line for each landblock edited.
    pub summary: Vec<String>,
    /// Nonblocking changes that need review.
    pub warnings: BTreeMap<u16, Vec<String>>,
    /// For each indoor cell whose statics the packs move, remove or add, where each static of
    /// its compiled record came from: its place among the cell's statics before those edits, or
    /// for one a pack adds, [`ADDED`] and its place among the cell's added statics.
    pub cell_statics: BTreeMap<u32, Vec<usize>>,
    /// For each such cell, the statics the packs add to it, in order: each by its pack and its
    /// place among its file's `add-cell-static` rows.
    pub cell_static_adds: BTreeMap<u32, Vec<(usize, usize)>>,
    /// Which pack wins where two disagree, as the world says.
    pub overrides: Vec<Override>,
    /// The conflicts found and the overrides taken, by the part of the world they are in.
    pub settled: BTreeMap<Part, Settled>,
    /// The world's land grid: its files', or the finer one the world sets ([`crate::land`]).
    pub grid: LandGrid,
    /// Each portal record of the game's tables the packs' rows wrote, as it was before them
    /// (`None`: none was compiled), and the lines they said: what the tables are compiled over
    /// again when their rows change ([`recompile_tables`]).
    pub tables_below: BTreeMap<u32, Option<Staged>>,
    pub tables_said: Vec<String>,
    /// The pieces of the AC2 dungeon kits the packs ask for: the land's own, or made from the
    /// player's install.
    pub ac2_pieces: Vec<crate::ac2_kit::Ac2Piece>,
    /// Those kits' lights, made from the same install.
    pub ac2_lights: Vec<crate::ac2_kit::Ac2Light>,
    /// Whether the world's land holds AC2's kit pieces, a land converted from AC2: the kits' pieces
    /// are then its own ([`crate::ac2_kit::from_land`]), each in the one look the land drew it in.
    pub ac2_from_land: bool,
}

/// A part of a world a compile settles conflicts in, so that compiling a part again replaces
/// its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Part {
    /// The new and replaced records.
    Records,
    /// The terrain vertices, over the whole world.
    Vertices,
    /// One landblock's statics, buildings and reshaped cells.
    Statics(u16),
    /// The statics standing in one landblock's indoor cells.
    CellStatics(u16),
    /// The server SQL.
    Sql,
    /// The game's tables the packs change field by field ([`crate::tables`]).
    Tables,
}

impl Compiled {
    /// Every conflict nothing settles, in the order of the parts.
    #[must_use]
    pub fn conflicts(&self) -> Vec<&conflict::Conflict> {
        self.settled.values().flat_map(|s| &s.conflicts).collect()
    }

    /// Every conflict the world's overrides settled.
    #[must_use]
    pub fn overridden(&self) -> Vec<&conflict::Overridden> {
        self.settled.values().flat_map(|s| &s.overridden).collect()
    }

    /// Whether the overlay holds nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.records.values().all(BTreeMap::is_empty)
            && self.tombstones.values().all(BTreeSet::is_empty)
    }

    /// The ids each file's overlay holds or deletes.
    #[must_use]
    pub fn touched(&self) -> BTreeMap<ModernDat, BTreeSet<u32>> {
        let mut out: BTreeMap<ModernDat, BTreeSet<u32>> = BTreeMap::new();
        for (t, r) in &self.records {
            out.entry(*t).or_default().extend(r.keys().copied());
        }
        for (t, d) in &self.tombstones {
            out.entry(*t)
                .or_default()
                .extend(d.iter().map(|(id, _)| *id));
        }
        out.retain(|_, ids| !ids.is_empty());
        out
    }
}

/// The world under the packs: the imports' records and deletions, then the base.
struct View<'a> {
    base: &'a DatSet,
    c: &'a Compiled,
}

impl View<'_> {
    fn deleted(&self, t: ModernDat, id: u32) -> bool {
        self.c.tombstones.get(&t).is_some_and(|d| {
            d.iter().any(|(x, mask)| {
                if *mask == 0 {
                    *x == id
                } else {
                    id & mask == x & mask
                }
            })
        })
    }

    /// The record `id` of the file `t` under the packs, with its version.
    fn get(&self, t: ModernDat, id: u32) -> Option<(Vec<u8>, u16)> {
        if let Some(s) = self.c.imported.get(&t).and_then(|r| r.get(&id)) {
            return Some((s.bytes.to_vec(), s.version));
        }
        if self.deleted(t, id) {
            return None;
        }
        let f = self.base.file(t)?;
        let e = f.base_entry(DataId(id))?;
        let version = u16::try_from(e.bits >> 16).unwrap_or(1).max(1);
        Some((f.read(DataId(id)).ok()?, version))
    }

    /// A portal-file record from either of the portal's two files.
    fn get_portal(&self, id: u32) -> Option<(Vec<u8>, u16)> {
        self.get(ModernDat::Portal, id)
            .or_else(|| self.get(ModernDat::HighRes, id))
    }
}

impl crate::tables::Below for View<'_> {
    /// A record the packs compiled, else the one under them.
    fn portal(&self, id: u32) -> Option<(Vec<u8>, u16)> {
        for t in [ModernDat::Portal, ModernDat::HighRes] {
            if let Some(s) = self.c.records.get(&t).and_then(|r| r.get(&id)) {
                return Some((s.bytes.to_vec(), s.version));
            }
        }
        self.get_portal(id)
    }

    /// The ground of the terrain as the packs edited it, else as the files below hold it.
    fn ground(&self, cell: u32, x: f32, y: f32) -> Option<f32> {
        if cell & 0xFFFF >= 0x0100 {
            return None;
        }
        let id = (cell & 0xFFFF_0000) | 0xFFFF;
        let bytes = match self
            .c
            .records
            .get(&ModernDat::Cell)
            .and_then(|r| r.get(&id))
        {
            Some(s) => s.bytes.to_vec(),
            None => self.get(ModernDat::Cell, id)?.0,
        };
        let land: CellLandblock = self.base.decode(DataId(id), &bytes).ok()?;
        let (region, _) = self.get_portal(0x1300_0000)?;
        let region: Region = self.base.decode(DataId(0x1300_0000), &region).ok()?;
        Some(ground(
            &land,
            &region,
            u16::try_from(cell >> 16).ok()?,
            x,
            y,
        ))
    }
}

fn read_imports(world: &World, base: &DatSet, c: &mut Compiled) -> Result<(), Error> {
    for dir in &world.imports {
        let od = OverlayDir::new(dir)?;
        for (t, file) in base.files() {
            let Some(layer) = od.layer_over(t, file, None)? else {
                continue;
            };
            let records = c.records.entry(t).or_default();
            let gone = c.tombstones.entry(t).or_default();
            // A later import's deletions take an earlier one's records with them.
            for x in layer.tombstones() {
                records.retain(|id, _| {
                    if x.mask == 0 {
                        *id != x.id
                    } else {
                        id & x.mask != x.id & x.mask
                    }
                });
                gone.insert((x.id, x.mask));
            }
            for (id, e) in layer.records() {
                let bytes = layer.read(id)?;
                let version = u16::try_from(e.bits >> 16).unwrap_or(1).max(1);
                records.insert(
                    id.raw(),
                    Staged {
                        bytes: bytes.into(),
                        version,
                    },
                );
            }
            c.imported.insert(t, records.clone());
        }
    }
    Ok(())
}

/// New records' ids from the lockfile, for what a maker names them: each kind's new record by its
/// name as a symbol of pack `pack`, numbered as [`number`] numbers it the first time it is met.
struct LockIds<'a> {
    lock: &'a mut Lock,
    base: &'a DatSet,
    staged: &'a Records,
    pack: &'a str,
    numbered: Vec<(Kind, String, u32)>,
    failed: Option<Error>,
}

impl dereth_ac2_convert::records::NewIds for LockIds<'_> {
    fn next_for(&mut self, kind: u32, name: &str) -> DataId {
        let kind = match kind {
            0x0D => Kind::Environment,
            0x08 => Kind::Surface,
            0x05 => Kind::SurfaceTexture,
            0x06 => Kind::RenderSurface,
            0x01 => Kind::GfxObj,
            0x02 => Kind::Setup,
            _ => {
                self.failed
                    .get_or_insert_with(|| Error::Refused(format!("no record kind {kind:#04X}")));
                return DataId(0);
            }
        };
        let symbol = format!("{}:{}", self.pack, crate::ac2_kit::symbol(name));
        if let Some(id) = self.lock.get(kind, &symbol) {
            return DataId(id);
        }
        let wanted = BTreeSet::from([(kind, symbol)]);
        match number(
            self.lock,
            &wanted,
            self.base,
            self.staged,
            &Deletions::new(),
            0,
        ) {
            Ok(mut n) => match n.pop() {
                Some(row) => {
                    let id = row.2;
                    self.numbered.push(row);
                    DataId(id)
                }
                None => DataId(0),
            },
            Err(e) => {
                self.failed.get_or_insert(e);
                DataId(0)
            }
        }
    }
}

/// The AC2 dungeon kits the packs ask for, into `c`: on a land converted from AC2 (an import with
/// [`crate::ac2_kit::LAND_TABLE`]), its own environments, no record made for them; else made from
/// `ac2`, their records numbered in the lockfile under the first pack that asks for one. Their
/// lights are made from `ac2` either way, when it is given.
pub(crate) fn ac2_kits(
    packs: &[Pack],
    imports: &[PathBuf],
    base: &DatSet,
    ac2: Option<&dereth_ac2_convert::source::Ac2>,
    c: &mut Compiled,
) -> Result<(), Error> {
    let kits: BTreeSet<crate::ac2_kit::Kit> = packs
        .iter()
        .flat_map(|p| p.ac2_kits.iter().map(|k| k.item))
        .collect();
    let land = crate::ac2_kit::LandEnvironments::of_imports(imports).map_err(Error::Refused)?;
    c.ac2_from_land = land.is_some();
    let Some(first) = packs.iter().find(|p| !p.ac2_kits.is_empty()) else {
        return Ok(());
    };
    let land_pieces = land.as_ref().map(|l| crate::ac2_kit::from_land(l, &kits));
    if land_pieces.is_none() && ac2.is_none() {
        return Err(Error::source(
            &first.ac2_kits[0].at,
            "this world uses AC2's dungeon kit: give the folder of your Asheron's Call 2 install \
             (--ac2-dats)",
        ));
    }
    let mut lock = c.lock.clone();
    let mut out = dereth_ac2_convert::records::Records::default();
    let (pieces, lights, numbered) = {
        let mut ids = LockIds {
            lock: &mut lock,
            base,
            staged: &c.imported,
            pack: &first.name,
            numbered: Vec::new(),
            failed: None,
        };
        let pieces = match (land_pieces, ac2) {
            (Some((pieces, _)), _) => pieces,
            (None, Some(ac2)) => {
                crate::ac2_kit::make(ac2, &kits, &mut ids, &mut out).map_err(Error::Refused)?
            }
            (None, None) => Vec::new(),
        };
        let lights = match ac2 {
            Some(ac2) => crate::ac2_kit::make_lights(ac2, &kits, &mut ids, &mut out)
                .map_err(Error::Refused)?,
            None => Vec::new(),
        };
        if let Some(e) = ids.failed.take() {
            return Err(e);
        }
        (pieces, lights, ids.numbered)
    };
    c.ac2_lights = lights;
    c.lock = lock;
    c.numbered.extend(numbered);
    let records = c.records.entry(ModernDat::Portal).or_default();
    for (id, bytes) in out.0 {
        records.insert(
            id.raw(),
            Staged {
                bytes: bytes.into(),
                version: NEW_RECORD_VERSION,
            },
        );
    }
    c.ac2_pieces = pieces;
    Ok(())
}

/// The world's land grid, from the region record under the packs; and, for a world that sets a
/// finer land than its files', the refined land laid down as the first layer over the imports
/// ([`crate::land`]): the region record saying the grid, and every landblock on it.
///
/// # Errors
/// A world whose files have no region record, or whose files already cut their land into steps the
/// world's spacing disagrees with.
fn land_layer(world: &World, base: &DatSet, c: &mut Compiled) -> Result<(), Error> {
    let (bytes, version, holder) = {
        let view = View { base, c };
        let held = view
            .get(ModernDat::Portal, REGION.raw())
            .map(|(b, v)| (b, v, ModernDat::Portal))
            .or_else(|| {
                view.get(ModernDat::HighRes, REGION.raw())
                    .map(|(b, v)| (b, v, ModernDat::HighRes))
            });
        match held {
            Some(h) => h,
            None if world.land_steps.is_none() => return Ok(()),
            None => return Err(Error::Dat("the files have no region record".to_owned())),
        }
    };
    let region = base
        .decode::<Region>(REGION, &bytes)
        .map_err(|e| Error::Dat(format!("the region record: {e}")))?;
    let files = region.land_defs.land_grid();
    c.grid = files;
    let Some(steps) = world.land_steps.filter(|s| *s != files.vertices_per_cell) else {
        return Ok(());
    };
    if !files.is_default() {
        return Err(Error::source(
            &Loc::new(&world.path, 0),
            format!(
                "the world's files lay their land {} meters apart; its land spacing is theirs",
                files.vertex_spacing()
            ),
        ));
    }
    let fine = crate::land::grid_of_steps(steps);
    let region_bytes = crate::land::refined_region(&bytes, steps)
        .filter(|b| {
            base.decode::<Region>(REGION, b)
                .is_ok_and(|r| r.land_defs.land_grid() == fine)
        })
        .ok_or_else(|| Error::Dat("the region record cannot say a finer land".to_owned()))?;
    let region_staged = Staged {
        bytes: region_bytes.into(),
        version,
    };
    for layer in [&mut c.records, &mut c.imported] {
        layer
            .entry(holder)
            .or_default()
            .insert(REGION.raw(), region_staged.clone());
    }
    let table = dereth_terrain::land::mesh::height_table(&region);
    let mut ids: BTreeSet<u32> = c
        .records
        .get(&ModernDat::Cell)
        .into_iter()
        .flat_map(BTreeMap::keys)
        .copied()
        .filter(|id| id & 0xFFFF == 0xFFFF)
        .collect();
    if let Some(f) = base.file(ModernDat::Cell) {
        ids.extend(
            f.base_entries()
                .map(|(id, _)| id.raw())
                .filter(|id| id & 0xFFFF == 0xFFFF),
        );
    }
    let mut refined = BTreeMap::new();
    for id in ids {
        let Some((bytes, version)) = (View { base, c }).get(ModernDat::Cell, id) else {
            continue;
        };
        let lb = base
            .decode::<CellLandblock>(DataId(id), &bytes)
            .map_err(|e| Error::Dat(format!("{id:#010X}: {e}")))?;
        #[allow(clippy::cast_possible_truncation)] // a cell id's top half is its landblock
        let land = crate::land::refine(&lb, &table, (id >> 16) as u16, steps);
        let bytes = written(id, base, land.encode_for(base.era))?;
        refined.insert(
            id,
            Staged {
                bytes: bytes.into(),
                version,
            },
        );
    }
    for layer in [&mut c.records, &mut c.imported] {
        layer
            .entry(ModernDat::Cell)
            .or_default()
            .extend(refined.iter().map(|(k, v)| (*k, v.clone())));
    }
    c.grid = fine;
    Ok(())
}

/// Refuse a pack's vertex that lies off its landblock on the world's land grid `grid`.
///
/// # Errors
/// The first such vertex, where its file sets it.
fn check_vertices(packs: &[Pack], grid: &LandGrid) -> Result<(), Error> {
    let last = grid.squares_per_side();
    for p in packs {
        for e in &p.edits {
            for v in &e.vertices {
                if usize::from(v.item.to.0.max(v.item.from.0)) > last
                    || usize::from(v.item.to.1.max(v.item.from.1)) > last
                {
                    return Err(Error::source(
                        &v.at,
                        format!("a vertex of this world is [x, y], each 0 to {last}"),
                    ));
                }
            }
        }
    }
    Ok(())
}

/// Whether an import of `world` says its world places its own things (`own_placements` in its
/// `world.toml`), so a server running the world places nothing of the world database's.
pub(crate) fn imports_own_placements(world: &World) -> bool {
    world.imports.iter().any(|i| {
        std::fs::read_to_string(i.join("world.toml"))
            .ok()
            .and_then(|t| t.parse::<toml::Table>().ok())
            .and_then(|t| t.get("own_placements").and_then(toml::Value::as_bool))
            .unwrap_or(false)
    })
}

/// The world's own placements file in overlay folder `dir`, with the packs' spawns (`sql`, the
/// build's filled SQL) written into it when an import's world places its own things: a server
/// running such a world places nothing of the world database's, so the packs' spawns are among
/// the world's own. An instance the import places too is the pack's as written, and one a pack
/// takes away is gone. Answers whether it wrote the file.
///
/// # Errors
/// A placements file that will not read or write.
pub fn write_own_spawns(
    world: &World,
    sql: &BTreeMap<PathBuf, String>,
    dir: &Path,
) -> Result<bool, Error> {
    use crate::sql_ids;
    if !imports_own_placements(world) {
        return Ok(false);
    }
    // Each spawn's line by its guid, a later file's over an earlier's.
    let mut lines: BTreeMap<u32, (sql_ids::Instance, Vec<u32>)> = BTreeMap::new();
    let mut links: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    let mut gone: BTreeSet<u32> = BTreeSet::new();
    for (rel, text) in sql {
        // A world repository's layer, mirrored in the pack being edited, is never the world's.
        if rel
            .components()
            .any(|c| c.as_os_str() == crate::mirror::MIRROR)
        {
            continue;
        }
        for g in sql_ids::deleted_instances(text) {
            if let Some(g) = sql_ids::number(&g) {
                gone.insert(g);
                lines.remove(&g);
            }
        }
        for i in sql_ids::instances(text) {
            if let Some(g) = sql_ids::number(&i.guid) {
                gone.remove(&g);
                lines.insert(g, (i, Vec::new()));
            }
        }
        sql_ids::rows(text, |row| {
            if row.table == "landblock_instance_link" {
                let n = |c: &str| row.get(c).and_then(sql_ids::number);
                if let (Some(p), Some(c)) = (n("parent_GUID"), n("child_GUID")) {
                    links.entry(p).or_default().push(c);
                }
            }
        });
    }
    if lines.is_empty() && gone.is_empty() {
        return Ok(false);
    }
    for (p, kids) in links {
        if let Some((_, l)) = lines.get_mut(&p) {
            l.extend(kids);
        }
    }
    let path = dir.join(WORLD_FILES[2]);
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(Error::io(&path, e)),
    };
    let mut out = String::new();
    for line in text.lines() {
        let guid = (!line.starts_with('#'))
            .then(|| line.split('\t').next().and_then(sql_ids::number))
            .flatten();
        if guid.is_some_and(|g| gone.contains(&g) || lines.contains_key(&g)) {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    if out.trim().is_empty() {
        out = "guid\twcid\tcell\tx\ty\tz\tqw\tqx\tqy\tqz\tlinks\n".to_owned();
    }
    for (g, (i, kids)) in &lines {
        let Some(w) = sql_ids::number(&i.weenie) else {
            continue;
        };
        let kids: Vec<String> = kids.iter().map(|c| format!("0x{c:08X}")).collect();
        let [x, y, z] = i.origin;
        let [qw, qx, qy, qz] = i.angles;
        out.push_str(&format!(
            "0x{g:08X}\t{w}\t0x{:08X}\t{x}\t{y}\t{z}\t{qw}\t{qx}\t{qy}\t{qz}\t{}\n",
            i.cell,
            kids.join(",")
        ));
    }
    std::fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
    std::fs::write(&path, out).map_err(|e| Error::io(&path, e))?;
    Ok(true)
}

/// The world's own files beside overlay folder `dir`, as a build writes them: those an import of
/// `world` holds ([`carry_world_files`]), with the packs' spawns (`sql`, the filled SQL) among the
/// things it places itself when its world places its own ([`write_own_spawns`]).
///
/// # Errors
/// A file that will not copy, read or write.
pub fn write_world_files(
    world: &World,
    sql: &BTreeMap<PathBuf, String>,
    dir: &Path,
) -> Result<(), Error> {
    carry_world_files(world, dir)?;
    write_own_spawns(world, sql, dir)?;
    Ok(())
}

/// The files beside an overlay that a server reads as the world's own: its rules, the places it
/// names and the things it places itself.
pub const WORLD_FILES: [&str; 3] = ["world.toml", "places.tsv", "placements.tsv"];

/// Copy each of [`WORLD_FILES`] an import of `world` holds into overlay folder `dir`, the last
/// import holding it winning: a world built on an imported world keeps that world's rules, place
/// names and placements. Answers the files copied.
///
/// # Errors
/// A file that will not copy.
pub fn carry_world_files(world: &World, dir: &Path) -> Result<Vec<PathBuf>, Error> {
    let mut out = Vec::new();
    for name in WORLD_FILES {
        let Some(from) = world
            .imports
            .iter()
            .rev()
            .map(|i| i.join(name))
            .find(|p| p.is_file())
        else {
            continue;
        };
        let to = dir.join(name);
        std::fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
        std::fs::copy(&from, &to).map_err(|e| Error::io(&from, e))?;
        out.push(to);
    }
    Ok(out)
}

/// The highest id the files hold in `[lo, hi]`, or `lo - 1` when they hold none.
fn highest_in(base: &DatSet, lo: u32, hi: u32) -> u32 {
    [Some(&base.portal), base.highres.as_ref()]
        .into_iter()
        .flatten()
        .flat_map(|f| f.base_entries().map(|(id, _)| id.raw()))
        .filter(|id| (lo..=hi).contains(id))
        .max()
        .unwrap_or(lo.saturating_sub(1))
}

/// Number every symbol in `wanted` the lockfile does not hold yet, in sorted order (kind, then
/// symbol), and answer the ones numbered.
///
/// - A record's id is the lowest above the highest the base files hold in the kind's span that
///   neither the files, the overlay so far nor the lockfile holds.
/// - A weenie class id is the lowest from `wcid_start` the lockfile does not hold.
/// - A loot table's id is the lowest from [`crate::loot::NEW_ID`] the lockfile does not hold.
/// - An instance's id is `0x7XXYYnnn` in its landblock, from `nnn = 0xFFF` down, that the lockfile
///   does not hold.
/// - A new cell's id is `0xXXYYnnnn` in its landblock, the next after the base's cells there (but
///   those an import deletes, `gone`) and every cell number the lockfile holds there: a number is
///   never handed to another cell.
///
/// # Errors
/// A span with no id left.
pub fn number(
    lock: &mut Lock,
    wanted: &BTreeSet<(Kind, String)>,
    base: &DatSet,
    staged: &Records,
    gone: &Deletions,
    wcid_start: u32,
) -> Result<Vec<(Kind, String, u32)>, Error> {
    let mut out = Vec::new();
    let deleted = covered(gone, ModernDat::Cell);
    for (kind, symbol) in wanted {
        if lock.get(*kind, symbol).is_some() {
            continue;
        }
        let id = match kind {
            // The files before Throne of Destiny reach a model's detail record by the model's own
            // id: the record takes the id its nearest level's implies.
            Kind::Degrade if base.layout() == dereth_dat::ContainerEra::Classic => {
                let level = crate::model_import::nearest_level(symbol);
                let model = lock.get(Kind::GfxObj, &level).ok_or_else(|| {
                    Error::Refused(format!("{symbol} has no model {level} to be found by"))
                })?;
                let id = dereth_assets::geometry::implicit_degrade_id(DataId(model)).raw();
                if base.has_portal(DataId(id)) || lock.holds_record_id(id) {
                    return Err(Error::Refused(format!(
                        "{symbol}: {id:#010X}, the id {level} reaches its detail by, is taken"
                    )));
                }
                id
            }
            Kind::GfxObj
            | Kind::Setup
            | Kind::Environment
            | Kind::Degrade
            | Kind::RenderSurface
            | Kind::SurfaceTexture
            | Kind::Surface => {
                let (lo, hi) = kind.span().unwrap_or((0, 0));
                // The portal files' records only: a cell file's (a landblock's land, `0xXXYYFFFF`)
                // shares these spans' numbers, and a finer or imported land stages one for every
                // landblock.
                let staged_in_span = staged
                    .iter()
                    .filter(|(t, _)| matches!(t, ModernDat::Portal | ModernDat::HighRes))
                    .flat_map(|(_, r)| r.keys())
                    .copied()
                    .filter(|id| (lo..=hi).contains(id))
                    .max();
                let mut id = highest_in(base, lo, hi)
                    .max(staged_in_span.unwrap_or(0))
                    .saturating_add(1);
                while id <= hi && (base.has_portal(DataId(id)) || lock.holds_record_id(id)) {
                    id += 1;
                }
                if id > hi {
                    return Err(Error::Refused(format!("no {kind} id is left for {symbol}")));
                }
                id
            }
            Kind::Wcid => {
                let held: BTreeSet<u32> = lock.of(Kind::Wcid).map(|(_, id)| id).collect();
                let mut id = wcid_start;
                while held.contains(&id) {
                    id = id
                        .checked_add(1)
                        .ok_or_else(|| Error::Refused("no weenie class id is left".into()))?;
                }
                id
            }
            Kind::Treasure => {
                let held: BTreeSet<u32> = lock.of(Kind::Treasure).map(|(_, id)| id).collect();
                let mut id = crate::loot::NEW_ID;
                while held.contains(&id) {
                    id = id
                        .checked_add(1)
                        .ok_or_else(|| Error::Refused("no loot table id is left".into()))?;
                }
                id
            }
            Kind::Guid => {
                let lb = crate::symbol::instance_landblock(symbol)
                    .ok()
                    .flatten()
                    .ok_or_else(|| Error::Refused(format!("{symbol} names no landblock")))?;
                let prefix = 0x7000_0000 | (u32::from(lb) << 12);
                let held: BTreeSet<u32> = lock.of(Kind::Guid).map(|(_, id)| id).collect();
                let mut low = 0xFFFu32;
                while held.contains(&(prefix | low)) {
                    low = low.checked_sub(1).ok_or_else(|| {
                        Error::Refused(format!("no instance id is left in landblock {lb:04X}"))
                    })?;
                }
                prefix | low
            }
            Kind::Cell => {
                let lb = crate::symbol::instance_landblock(symbol)
                    .ok()
                    .flatten()
                    .ok_or_else(|| Error::Refused(format!("{symbol} names no landblock")))?;
                let first = u32::from(lb) << 16;
                // After the cells the files hold there, which can be more than the statics
                // record counts.
                let held_by_files = |n: u32| {
                    staged
                        .get(&ModernDat::Cell)
                        .is_some_and(|r| r.contains_key(&(first | n)))
                        || (base
                            .file(ModernDat::Cell)
                            .is_some_and(|f| f.base_entry(DataId(first | n)).is_some())
                            && !deleted(first | n))
                };
                let mut after_base = u32::from(crate::cells::FIRST_CELL);
                while after_base < 0xFFFE && held_by_files(after_base) {
                    after_base += 1;
                }
                let after_held = lock
                    .of(Kind::Cell)
                    .filter(|(_, id)| id >> 16 == u32::from(lb))
                    .map(|(_, id)| (id & 0xFFFF) + 1)
                    .max()
                    .unwrap_or(0);
                let n = after_base.max(after_held);
                if n >= 0xFFFE {
                    return Err(Error::Refused(format!(
                        "no cell number is left in landblock {lb:04X}"
                    )));
                }
                first | n
            }
        };
        lock.insert(*kind, symbol, id);
        out.push((*kind, symbol.clone(), id));
    }
    Ok(out)
}

/// Every record a pack defines, by kind and symbol, with where.
type Defined = BTreeMap<(Kind, String), Loc>;

/// The packs' records: those defined, the sources a conflict's winner stands over (by where they
/// are), and what was settled.
struct Defs {
    defined: Defined,
    skip: BTreeSet<Loc>,
    settled: Settled,
}

/// A record's source, with the kind and symbol it defines when it is new.
type RecordClaim = (Claim, Option<(Kind, String)>);

fn defined_records(packs: &[Pack], overrides: &[Override]) -> Defs {
    // Each record's sources, by item: a new record's kind and symbol, a replaced record's id.
    let mut items: BTreeMap<String, Vec<RecordClaim>> = BTreeMap::new();
    for p in packs {
        for r in &p.records {
            let kind = r.item.kind();
            let value = format!("{:?}", r.item);
            let (item, claim, new) = match r.item.target() {
                Target::New(sym) => (
                    format!("{kind} {sym}"),
                    Claim::new(&p.name, &r.at, "defines it", value),
                    Some((kind, sym.clone())),
                ),
                Target::Replace(id) => (
                    conflict::record_item(id.raw()),
                    Claim::new(&p.name, &r.at, format!("replaces it with a {kind}"), value),
                    None,
                ),
            };
            items.entry(item).or_default().push((claim, new));
        }
    }
    let mut out = Defs {
        defined: Defined::new(),
        skip: BTreeSet::new(),
        settled: Settled::default(),
    };
    for (name, list) in items {
        let claims: Vec<Claim> = list.iter().map(|(c, _)| c.clone()).collect();
        let w = conflict::settle(overrides, &Item::new(name), &claims, &mut out.settled);
        for (i, (c, new)) in list.into_iter().enumerate() {
            if i == w {
                if let Some(key) = new {
                    out.defined.insert(key, c.at);
                }
            } else {
                out.skip.insert(c.at);
            }
        }
    }
    out
}

/// A picture, read: width, height, and four bytes a pixel, red, green, blue, alpha.
fn read_picture(at: &Loc, path: &Path) -> Result<(u32, u32, Vec<u8>), Error> {
    let bytes = std::fs::read(path).map_err(|e| Error::io(path, e))?;
    let bad = |why: String| Error::source(at, format!("{}: {why}", path.display()));
    let mut decoder = png::Decoder::new(bytes.as_slice());
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().map_err(|e| bad(e.to_string()))?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut buf)
        .map_err(|e| bad(e.to_string()))?;
    let (w, h) = (info.width, info.height);
    if !w.is_power_of_two() || !h.is_power_of_two() || w > 2048 || h > 2048 {
        return Err(bad(format!(
            "{w} x {h}: a texture's sides are powers of two, up to 2048"
        )));
    }
    let channels = match info.color_type {
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Indexed => return Err(bad("an indexed picture was not expanded".into())),
    };
    let mut rgba = Vec::with_capacity(w as usize * h as usize * 4);
    for row in buf[..info.buffer_size()].chunks_exact(info.line_size) {
        for px in row[..w as usize * channels].chunks_exact(channels) {
            rgba.extend_from_slice(&match channels {
                1 => [px[0], px[0], px[0], 255],
                2 => [px[0], px[0], px[0], px[1]],
                3 => [px[0], px[1], px[2], 255],
                _ => [px[0], px[1], px[2], px[3]],
            });
        }
    }
    Ok((w, h, rgba))
}

/// A picture's pixels in a record's format, top row first.
fn pixels(format: PixelFormat, rgba: &[u8]) -> Vec<u8> {
    let quads = rgba.as_chunks::<4>().0;
    match format {
        PixelFormat::A8R8G8B8 => quads
            .iter()
            .flat_map(|p| [p[2], p[1], p[0], p[3]])
            .collect(),
        PixelFormat::R8G8B8 => quads.iter().flat_map(|p| [p[2], p[1], p[0]]).collect(),
    }
}

fn resolve(
    lock: &Lock,
    defined: &Defined,
    at: &Loc,
    kind: Kind,
    r: &RecordRef,
) -> Result<DataId, Error> {
    match r {
        RecordRef::Id(id) => Ok(*id),
        RecordRef::Symbol(sym) => {
            if !defined.contains_key(&(kind, sym.clone())) {
                return Err(Error::source(at, format!("no pack defines {kind} {sym}")));
            }
            lock.get(kind, sym)
                .map(DataId)
                .ok_or_else(|| Error::source(at, format!("{kind} {sym} was not numbered")))
        }
    }
}

fn build_records(
    packs: &[Pack],
    defs: &Defs,
    view_base: &DatSet,
    c: &mut Compiled,
) -> Result<(), Error> {
    let era = view_base.era;
    let defined = &defs.defined;
    // The pictures this build makes, by id: in files whose image textures hold their pixels, a
    // texture is made from its first image's.
    let mut pictures: BTreeMap<u32, crate::pictures::Rgba> = BTreeMap::new();
    // Those textures, made once every picture is: each with its source, its first image and its
    // version.
    let mut holding: Vec<(DataId, Loc, DataId, u16)> = Vec::new();
    for p in packs {
        for r in &p.records {
            if defs.skip.contains(&r.at) {
                continue;
            }
            let at = &r.at;
            let kind = r.item.kind();
            let id = match r.item.target() {
                Target::New(sym) => {
                    resolve(&c.lock, defined, at, kind, &RecordRef::Symbol(sym.clone()))?
                }
                Target::Replace(id) => {
                    if id.raw() >> 24 != kind.span().map_or(0, |(lo, _)| lo >> 24) {
                        return Err(Error::source(
                            at,
                            format!("{:#010X} is not a {kind}", id.raw()),
                        ));
                    }
                    *id
                }
            };
            let version = {
                let view = View { base: view_base, c };
                match r.item.target() {
                    Target::Replace(_) => {
                        let Some((_, v)) = view.get_portal(id.raw()) else {
                            return Err(Error::source(
                                at,
                                format!(
                                    "{:#010X} is not in the files; a new record has a name",
                                    id.raw()
                                ),
                            ));
                        };
                        v
                    }
                    Target::New(_) => NEW_RECORD_VERSION,
                }
            };
            let held = |r: Result<Vec<u8>, dereth_assets::AssetError>| {
                r.map_err(|e| {
                    Error::source(
                        at,
                        format!(
                            "{:#010X} cannot be written in {}: {e}",
                            id.raw(),
                            files_of(era)
                        ),
                    )
                })
            };
            let bytes = match &r.item {
                NewRecord::Geometry { record, .. } => {
                    if let Some(picture) = record.picture() {
                        pictures.insert(id.raw(), picture);
                    }
                    record.encode(era, id, |kind, reference| {
                        resolve(&c.lock, defined, at, kind, reference)
                    })?
                }
                NewRecord::RenderSurface {
                    picture,
                    format,
                    category,
                    ..
                } => {
                    let (width, height, rgba) = read_picture(at, picture)?;
                    let picture = crate::pictures::Rgba {
                        width,
                        height,
                        pixels: rgba,
                    };
                    let bytes = held(crate::pictures::image_record(era, id, &picture, || {
                        (format.id(), *category, pixels(*format, &picture.pixels))
                    }))?;
                    pictures.insert(id.raw(), picture);
                    bytes
                }
                NewRecord::SurfaceTexture {
                    levels, category, ..
                } => {
                    let levels: Vec<DataId> = levels
                        .iter()
                        .map(|l| resolve(&c.lock, defined, at, Kind::RenderSurface, l))
                        .collect::<Result<_, _>>()?;
                    match era.container_era() {
                        dereth_dat::ContainerEra::Classic => {
                            let Some(first) = levels.first() else {
                                return Err(Error::source(at, "an image texture has an image"));
                            };
                            holding.push((id, at.clone(), *first, version));
                            continue;
                        }
                        dereth_dat::ContainerEra::Modern => held(
                            SurfaceTexture {
                                id,
                                data_category: *category,
                                // The constant every image texture carries.
                                unknown_byte: 2,
                                source_levels: levels,
                            }
                            .encode_for(era),
                        )?,
                    }
                }
                NewRecord::Surface {
                    surface_type,
                    texture,
                    palette,
                    color,
                    translucency,
                    luminosity,
                    diffuse,
                    ..
                } => held(
                    Surface {
                        surface_type: *surface_type,
                        orig_texture_id: texture
                            .as_ref()
                            .map(|t| resolve(&c.lock, defined, at, Kind::SurfaceTexture, t))
                            .transpose()?,
                        orig_palette_id: if texture.is_some() {
                            Some(match palette {
                                Some(RecordRef::Id(id)) => *id,
                                Some(RecordRef::Symbol(s)) => {
                                    return Err(Error::source(
                                        at,
                                        format!("{s}: a palette is named by its id"),
                                    ))
                                }
                                None => DataId(0),
                            })
                        } else {
                            None
                        },
                        color_value: *color,
                        translucency: *translucency,
                        luminosity: *luminosity,
                        diffuse: *diffuse,
                    }
                    .encode_for(era, id),
                )?,
            };
            c.records.entry(ModernDat::Portal).or_default().insert(
                id.raw(),
                Staged {
                    bytes: bytes.into(),
                    version,
                },
            );
        }
    }
    for (id, at, first, version) in holding {
        let picture = match pictures.get(&first.raw()) {
            Some(p) => p.clone(),
            None => {
                let view = View { base: view_base, c };
                let Some((bytes, _)) = view.get_portal(first.raw()) else {
                    return Err(Error::source(
                        &at,
                        format!("image {:#010X} is not in the files", first.raw()),
                    ));
                };
                crate::pictures::picture_of_image(era, first, &bytes)
                    .map_err(|e| Error::source(&at, format!("{:#010X}: {e}", first.raw())))?
            }
        };
        let bytes = crate::pictures::texture_holding(era, id, &picture)
            .map_err(|e| Error::source(&at, format!("{:#010X}: {e}", id.raw())))?;
        c.records.entry(ModernDat::Portal).or_default().insert(
            id.raw(),
            Staged {
                bytes: bytes.into(),
                version,
            },
        );
    }
    Ok(())
}

/// A record's bytes in the layout of `base`'s era, or why its files cannot hold it.
fn written(
    id: u32,
    base: &DatSet,
    r: Result<Vec<u8>, dereth_assets::AssetError>,
) -> Result<Vec<u8>, Error> {
    r.map_err(|e| {
        Error::Dat(format!(
            "{id:#010X} cannot be written in {}: {e}",
            files_of(base.era)
        ))
    })
}

/// One field a vertex edit sets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Field {
    Height,
    Terrain,
    Scene,
    Road,
}

impl Field {
    /// Every field, in the order of [`Field::TABLE`].
    pub const ALL: [Self; 4] = [Self::Height, Self::Terrain, Self::Scene, Self::Road];

    /// The fields as the shared table writes them: each one's name, and its highest value.
    pub const TABLE: [dereth_landscape::VertexField; 4] = [
        dereth_landscape::VertexField {
            name: "height",
            max: 255,
        },
        dereth_landscape::VertexField {
            name: "terrain",
            max: crate::land::GROUND_MAX,
        },
        dereth_landscape::VertexField {
            name: "scene",
            max: 31,
        },
        dereth_landscape::VertexField {
            name: "road",
            max: 3,
        },
    ];

    /// The field called `name`.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        dereth_landscape::VertexField::find(&Self::TABLE, name).map(|(i, _)| Self::ALL[i])
    }
}

/// Every vertex edit of every pack, by world vertex and field: the value and the file that set it.
pub type VertexSets = BTreeMap<(u32, u32, Field), (u16, Loc)>;

/// Every vertex edit of every pack over the whole world, the source of each field that stands,
/// and what was settled: conflicts and overrides one per run of vertices, by field and sources.
/// A file may set a field again; its later edit stands.
#[must_use]
pub fn gather_vertices(
    packs: &[Pack],
    overrides: &[Override],
    grid: &LandGrid,
) -> (VertexSets, Settled) {
    #[allow(clippy::cast_possible_truncation)] // 8 to 32 squares a block side
    let squares = grid.squares_per_side() as u32;
    let mut claims: BTreeMap<(u32, u32, Field), Vec<(u16, Claim)>> = BTreeMap::new();
    for p in packs {
        for e in &p.edits {
            let (bx, by) = (u32::from(e.landblock >> 8), u32::from(e.landblock & 0xFF));
            for v in &e.vertices {
                let fields = [
                    (Field::Height, v.item.height.map(u16::from)),
                    (Field::Terrain, v.item.terrain),
                    (Field::Scene, v.item.scene),
                    (Field::Road, v.item.road),
                ];
                for x in v.item.from.0..=v.item.to.0 {
                    for y in v.item.from.1..=v.item.to.1 {
                        let g = (bx * squares + u32::from(x), by * squares + u32::from(y));
                        for (field, value) in fields {
                            let Some(value) = value else { continue };
                            let claim = Claim::new(
                                &p.name,
                                &v.at,
                                format!("sets it to {value}"),
                                value.to_string(),
                            );
                            let list = claims.entry((g.0, g.1, field)).or_default();
                            match list.last_mut() {
                                Some(last) if last.1.at.file == v.at.file => *last = (value, claim),
                                _ => list.push((value, claim)),
                            }
                        }
                    }
                }
            }
        }
    }
    let mut sets = VertexSets::new();
    // The vertices of each run, by field, by whether it is a conflict, and by its sources.
    type Runs = BTreeMap<(Field, bool, String), (Vec<(u32, u32)>, Settled)>;
    let mut runs = Runs::new();
    for ((gx, gy, field), list) in claims {
        let (owner, x, y) = crate::edit::vertex_owner(grid, gx, gy);
        let spacing = grid.vertex_spacing();
        let mut item = Item::new(format!("vertex {gx},{gy} {}", field_name(field))).at(
            owner,
            f32::from(x) * spacing,
            f32::from(y) * spacing,
        );
        item.landblocks
            .extend(copies(grid, gx, gy).into_iter().map(|(lb, _, _)| lb));
        let only: Vec<Claim> = list.iter().map(|(_, c)| c.clone()).collect();
        let mut one = Settled::default();
        let w = conflict::settle(overrides, &item, &only, &mut one);
        let (value, claim) = &list[w];
        sets.insert((gx, gy, field), (*value, claim.at.clone()));
        let key = if let Some(c) = one.conflicts.first() {
            (field, true, format!("{:?}", c.claims))
        } else if let Some(o) = one.overridden.first() {
            (field, false, format!("{:?} {:?}", o.winner, o.over))
        } else {
            continue;
        };
        let run = runs.entry(key).or_default();
        run.0.push((gx, gy));
        if run.0.len() == 1 {
            run.1 = one;
        } else {
            for c in &mut run.1.conflicts {
                c.landblocks.extend(&item.landblocks);
            }
            for o in &mut run.1.overridden {
                o.landblocks.extend(&item.landblocks);
            }
        }
    }
    let mut settled = Settled::default();
    for ((field, _, _), (vertices, mut run)) in runs {
        if vertices.len() > 1 {
            let xs = vertices.iter().map(|v| v.0);
            let ys = vertices.iter().map(|v| v.1);
            let what = format!(
                "the {} of {} vertices from {},{} to {},{}",
                field_name(field),
                vertices.len(),
                xs.clone().min().unwrap_or(0),
                ys.clone().min().unwrap_or(0),
                xs.max().unwrap_or(0),
                ys.max().unwrap_or(0),
            );
            for c in &mut run.conflicts {
                what.clone_into(&mut c.what);
            }
            for o in &mut run.overridden {
                what.clone_into(&mut o.what);
            }
        }
        settled.extend(run);
    }
    (sets, settled)
}

/// A field's name, as an item names it.
#[must_use]
pub const fn field_name(field: Field) -> &'static str {
    Field::TABLE[field as usize].name
}

/// The landblocks holding a copy of world vertex `(gx, gy)` of land grid `grid`, with its place in
/// each: one inside a landblock, two on an edge, four at a corner.
#[must_use]
pub fn copies(grid: &LandGrid, gx: u32, gy: u32) -> Vec<(u16, usize, usize)> {
    crate::land::world_grid(*grid).copies(gx, gy)
}

/// The quaternion of a heading: degrees clockwise from north, a turn about up. North is +y,
/// which a model faces unturned.
#[must_use]
pub fn heading_quat(degrees: f32) -> Quat {
    let half = -degrees.to_radians() / 2.0;
    // Adding zero writes no turn as +0.0 rather than -0.0.
    let (cos, sin) = (
        dereth_primitives::num::math::cosf(half),
        dereth_primitives::num::math::sinf(half),
    );
    Quat::new(cos + 0.0, 0.0, 0.0, sin + 0.0)
}

/// What a pack does to one of a landblock's own cells: reshapes it, or takes it away.
#[derive(Clone, Copy, Debug)]
enum CellChange<'a> {
    Reshape(&'a Located<source::ReplaceCell>),
    Take(&'a Located<source::RemoveCell>),
}

/// The quaternion a rotation names.
#[must_use]
pub fn quat_of(r: Rotation) -> Quat {
    match r {
        Rotation::Heading(h) => heading_quat(h),
        Rotation::Quat([w, x, y, z]) => Quat::new(w, x, y, z),
    }
}

/// The ground's height at landblock-local `(x, y)`: the plane of the terrain triangle under it,
/// as the client and the server split the square, on the region's land grid.
pub fn ground(lb: &CellLandblock, region: &Region, landblock: u16, x: f32, y: f32) -> f32 {
    use dereth_terrain::land::mesh::{generate_landblock, plane_set_height, Direction};
    let (bx, by) = (i32::from(landblock >> 8), i32::from(landblock & 0xFF));
    let mesh = generate_landblock(lb, region, bx, by, 1, Direction::InViewerBlock);
    let squares = mesh.squares_per_side();
    let span = LandGrid::DEFAULT.block_length() / f32::from(u16::try_from(squares).unwrap_or(8));
    let last = i32::try_from(squares).unwrap_or(8) - 1;
    let square = |v: f32| dereth_primitives::num::to_i32((v / span).floor()).clamp(0, last);
    let (i, j) = (square(x), square(y));
    let (iu, ju) = (
        usize::try_from(i).unwrap_or(0),
        usize::try_from(j).unwrap_or(0),
    );
    #[allow(clippy::cast_precision_loss)]
    let (dx, dy) = (x - i as f32 * span, y - j as f32 * span);
    let cut = mesh.sw_to_ne_cut[iu * squares + ju];
    let (a, b) = mesh.cell_polygons(iu, ju);
    // With the south-west to north-east cut the first triangle is the south-east half; with the
    // other cut it is the south-west half.
    let first = if cut { dx >= dy } else { dx + dy <= span };
    let p = if first { a } else { b };
    plane_set_height(&p.plane, x, y).unwrap_or(0.0)
}

fn frame_at(place: Place, rotation: Quat, ground_z: &dyn Fn(f32, f32) -> f32) -> Frame {
    let z = match place.z {
        Height::At(z) => z,
        Height::Ground => ground_z(place.x, place.y),
    };
    Frame::new(Vec3::new(place.x, place.y, z), rotation)
}

/// One landblock's static edits from every pack, in load order, with each pack's place.
struct BlockEdits<'a> {
    files: Vec<(usize, &'a LandblockEdits)>,
}

/// One pack static placed in a landblock: its symbol, where it is written, its pack and place
/// among the file's statics, and what it places.
struct Added {
    name: Option<String>,
    pack: usize,
    entry: usize,
    object: ObjectEntry,
}

/// The statics standing in landblock `lb`'s indoor cells, as `edits` move and remove them: each
/// by its place among its cell's statics as the cell's source holds them. A later file's move of
/// one stands over an earlier's.
fn cell_statics(
    packs: &[Pack],
    lb: u16,
    edits: &BlockEdits<'_>,
    base: &DatSet,
    c: &mut Compiled,
) -> Result<(), Error> {
    type Moves = BTreeMap<usize, (Loc, Option<[f32; 3]>, Option<Rotation>)>;
    type Removes = BTreeMap<usize, (Loc, Option<DataId>)>;
    // Each static's sources, by its cell and place: a move or a removal.
    type Change = (
        Claim,
        Loc,
        Option<(Option<[f32; 3]>, Option<Rotation>)>,
        Option<DataId>,
    );
    let number = |r: &CellRef, at: &Loc| -> Result<u16, Error> {
        match r {
            CellRef::Number(n) => Ok(*n),
            CellRef::Symbol(s) => c
                .lock
                .get(Kind::Cell, &cell_symbol(s, lb))
                .and_then(|id| u16::try_from(id & 0xFFFF).ok())
                .ok_or_else(|| {
                    Error::source(at, format!("no pack places cell {s} in landblock {lb:04X}"))
                }),
        }
    };
    let mut changes: BTreeMap<(u16, usize), Vec<Change>> = BTreeMap::new();
    for (pi, e) in &edits.files {
        let pack = packs[*pi].name.as_str();
        for m in &e.cell_moves {
            let n = number(&m.item.cell, &m.at)?;
            let mut says = Vec::new();
            if let Some(a) = m.item.at {
                says.push(format!("moves it to ({}, {}, {})", a[0], a[1], a[2]));
            }
            if let Some(r) = m.item.rotation {
                says.push(format!("turns it {}", turn_text(r)));
            }
            changes.entry((n, m.item.index)).or_default().push((
                Claim::new(
                    pack,
                    &m.at,
                    says.join(" and "),
                    format!("{:?} {:?}", m.item.at, m.item.rotation),
                ),
                m.at.clone(),
                Some((m.item.at, m.item.rotation)),
                None,
            ));
        }
        for r in &e.cell_removes {
            let n = number(&r.item.cell, &r.at)?;
            changes.entry((n, r.item.index)).or_default().push((
                Claim::new(pack, &r.at, "removes it", "removed"),
                r.at.clone(),
                None,
                r.item.model,
            ));
        }
    }
    // The statics the packs add to cells not their own, in pack and file order.
    type Adds = Vec<(Loc, usize, usize, ObjectEntry)>;
    let mut adds: BTreeMap<u16, Adds> = BTreeMap::new();
    for (pi, e) in &edits.files {
        for (row, a) in e.cell_adds.iter().enumerate() {
            let n = number(&a.item.cell, &a.at)?;
            adds.entry(n).or_default().push((
                a.at.clone(),
                *pi,
                row,
                ObjectEntry {
                    id: a.item.model,
                    frame: Frame::new(
                        Vec3::new(a.item.at[0], a.item.at[1], a.item.at[2]),
                        quat_of(a.item.rotation),
                    ),
                },
            ));
        }
    }
    let mut settled = Settled::default();
    let mut by_cell: BTreeMap<u16, (Moves, Removes)> = BTreeMap::new();
    for ((n, i), list) in changes {
        let claims: Vec<Claim> = list.iter().map(|x| x.0.clone()).collect();
        let item = Item::new(format!("cell-static {lb:04X} {n:04X} {i}")).in_landblock(lb);
        let w = conflict::settle(&c.overrides, &item, &claims, &mut settled);
        let (_, at, moved, model) = list[w].clone();
        let entry = by_cell.entry(n).or_default();
        match moved {
            Some((to, turn)) => {
                entry.0.insert(i, (at, to, turn));
            }
            None => {
                entry.1.insert(i, (at, model));
            }
        }
    }
    if settled != Settled::default() {
        c.settled.insert(Part::CellStatics(lb), settled);
    }
    let changed: BTreeSet<u16> = by_cell.keys().chain(adds.keys()).copied().collect();
    for n in changed {
        let (moves, removes) = by_cell.remove(&n).unwrap_or_default();
        let added = adds.remove(&n).unwrap_or_default();
        let id = (u32::from(lb) << 16) | u32::from(n);
        let first = moves
            .values()
            .map(|m| m.0.clone())
            .chain(removes.values().map(|r| r.0.clone()))
            .chain(added.iter().map(|a| a.0.clone()))
            .next()
            .unwrap_or_else(|| Loc::new(Path::new(""), 0));
        let held = c
            .records
            .get(&ModernDat::Cell)
            .and_then(|r| r.get(&id))
            .map(|s| (s.bytes.to_vec(), s.version))
            .or_else(|| View { base, c }.get(ModernDat::Cell, id));
        let Some((bytes, version)) = held else {
            return Err(Error::source(
                &first,
                format!("landblock {lb:04X} has no cell {n:04X}"),
            ));
        };
        let mut cell = base
            .decode::<dereth_assets::world::EnvCell>(DataId(id), &bytes)
            .map_err(|e| Error::Dat(format!("{id:#010X}: {e}")))?;
        let len = cell.static_objects.len();
        let no_static = |at: &Loc, i: usize| {
            Error::source(
                at,
                format!("cell {n:04X} of landblock {lb:04X} has {len} statics; there is no {i}"),
            )
        };
        for (i, (at, ..)) in &moves {
            if *i >= len {
                return Err(no_static(at, *i));
            }
        }
        for (i, (at, model)) in &removes {
            let Some(o) = cell.static_objects.get(*i) else {
                return Err(no_static(at, *i));
            };
            if let Some(m) = model {
                if *m != o.id {
                    return Err(Error::source(
                        at,
                        format!(
                            "static {i} of cell {n:04X} of landblock {lb:04X} is {:#010X}, not {:#010X}",
                            o.id.raw(),
                            m.raw()
                        ),
                    ));
                }
            }
        }
        let mut objects = Vec::with_capacity(len);
        let mut origins = Vec::with_capacity(len);
        for (i, o) in cell.static_objects.iter().enumerate() {
            if removes.contains_key(&i) {
                continue;
            }
            let mut o = *o;
            if let Some((_, at, rotation)) = moves.get(&i) {
                let origin = at.map_or(o.frame.origin, |p| Vec3::new(p[0], p[1], p[2]));
                let turn = rotation.map_or(o.frame.rotation, quat_of);
                o.frame = Frame::new(origin, turn);
            }
            objects.push(o);
            origins.push(i);
        }
        let mut placed = Vec::with_capacity(added.len());
        for (k, (_, pack, row, o)) in added.into_iter().enumerate() {
            objects.push(o);
            origins.push(ADDED | k);
            placed.push((pack, row));
        }
        if !placed.is_empty() {
            c.cell_static_adds.insert(id, placed);
        }
        cell.static_objects = objects;
        if cell.static_objects.is_empty() {
            cell.flags &= !2;
        } else {
            cell.flags |= 2;
        }
        c.records.entry(ModernDat::Cell).or_default().insert(
            id,
            Staged {
                bytes: written(id, base, cell.encode_for(base.era))?.into(),
                version,
            },
        );
        c.cell_statics.insert(id, origins);
        c.summary.push(format!(
            "{lb:04X}: cell {n:04X}: {} static(s) moved, {} removed",
            moves.len(),
            removes.len()
        ));
    }
    Ok(())
}

/// A static's frame after `m` moves or turns it from `from`.
fn moved(from: Frame, m: &MoveStatic, ground_z: &dyn Fn(f32, f32) -> f32) -> Frame {
    let place = m.at.unwrap_or(Place {
        x: from.origin.x,
        y: from.origin.y,
        z: Height::At(from.origin.z),
    });
    let rot = m.rotation.map_or(from.rotation, quat_of);
    frame_at(place, rot, ground_z)
}

/// A place as the report says it.
fn place_text(p: Place) -> String {
    match p.z {
        Height::At(z) => format!("({}, {}, {z})", p.x, p.y),
        Height::Ground => format!("({}, {}, on the ground)", p.x, p.y),
    }
}

/// A turn as the report says it.
fn turn_text(r: Rotation) -> String {
    match r {
        Rotation::Heading(h) => format!("to heading {h}"),
        Rotation::Quat(q) => format!("to rotation {q:?}"),
    }
}

/// What a static's move sets, as the report says it.
fn move_says(m: &MoveStatic) -> String {
    match (m.at, m.rotation) {
        (Some(p), Some(r)) => format!(
            "moves it to {} and turns it {}",
            place_text(p),
            turn_text(r)
        ),
        (Some(p), None) => format!("moves it to {}", place_text(p)),
        (None, Some(r)) => format!("turns it {}", turn_text(r)),
        (None, None) => "leaves it where it is".into(),
    }
}

/// What a building's move sets, as the report says it.
fn building_says(m: &MoveBuilding) -> String {
    let to =
        m.at.map(|a| format!("moves it to ({}, {}, {})", a[0], a[1], a[2]));
    let turn = m.rotation.map(|r| format!("turns it {}", turn_text(r)));
    match (to, turn) {
        (Some(a), Some(b)) => format!("{a} and {b}"),
        (Some(a), None) | (None, Some(a)) => a,
        (None, None) => "leaves it where it is".into(),
    }
}

/// The buildings landblock `lb`'s files drop and the dungeons they copy, copied from their
/// templates (read under the packs, or from the player's own files for a row that says so) with
/// their rooms numbered: a room's number is the
/// lockfile's, given here the first time, and so is a house's spawn's when no SQL names it.
fn drop_buildings(
    edits: &BlockEdits<'_>,
    lb: u16,
    base: &DatSet,
    c: &mut Compiled,
    region: Option<&Region>,
    ground_z: &dyn Fn(f32, f32) -> f32,
) -> Result<Vec<crate::drops::Dropped>, Error> {
    let rows: Vec<&Located<source::AddBuilding>> = edits
        .files
        .iter()
        .flat_map(|(_, e)| e.drops.iter())
        .collect();
    let dungeons: Vec<&Located<source::AddDungeon>> = edits
        .files
        .iter()
        .flat_map(|(_, e)| e.dungeon_copies.iter())
        .collect();
    if rows.is_empty() && dungeons.is_empty() {
        return Ok(Vec::new());
    }
    let (prepared, copied) = {
        let view = View { base, c };
        let read = |id: u32| view.get(ModernDat::Cell, id).map(|(b, _)| b);
        // A row whose templates are the player's own files' reads them there, under no import,
        // their ground made with their own region.
        let files = |id: u32| base.file(ModernDat::Cell)?.read(DataId(id)).ok();
        let own = rows
            .iter()
            .any(|r| r.item.from_files)
            .then(|| files_region(base))
            .flatten();
        (
            rows.iter()
                .map(|r| {
                    let (from, region): (&dyn Fn(u32) -> Option<Vec<u8>>, _) = if r.item.from_files
                    {
                        (&files, own.as_ref())
                    } else {
                        (&read, region)
                    };
                    crate::drops::prepare(r, from, region, base.era)
                })
                .collect::<Result<Vec<_>, _>>()?,
            dungeons
                .iter()
                .map(|r| {
                    let from: &dyn Fn(u32) -> Option<Vec<u8>> =
                        if r.item.from_files { &files } else { &read };
                    crate::drops::prepare_dungeon(r, from, base.era)
                })
                .collect::<Result<Vec<_>, _>>()?,
        )
    };
    let wanted: BTreeSet<(Kind, String)> = prepared
        .iter()
        .flat_map(|p| p.symbols(lb))
        .chain(copied.iter().flat_map(|d| d.symbols(lb)))
        .collect();
    let mut lock = c.lock.clone();
    let numbered = number(&mut lock, &wanted, base, &c.imported, &c.tombstones, 0)?;
    c.lock = lock;
    c.numbered.extend(numbered);
    let out: Vec<crate::drops::Dropped> = prepared
        .iter()
        .map(|p| p.copy(lb, &c.lock, ground_z))
        .chain(copied.iter().map(|d| d.copy(lb, &c.lock)))
        .collect::<Result<_, _>>()?;
    let houses: BTreeMap<String, u32> = out.iter().flat_map(|d| d.houses.clone()).collect();
    if !houses.is_empty() {
        c.houses.insert(lb, houses);
    }
    Ok(out)
}

/// Apply every pack's operations to the landblocks they touch (all of them, or those in `only`),
/// over the imports and the base, and put the records that come out into `c`.
#[allow(clippy::too_many_lines)]
fn edit_landblocks(
    packs: &[Pack],
    base: &DatSet,
    c: &mut Compiled,
    only: Option<&BTreeSet<u16>>,
) -> Result<(), Error> {
    let wanted = |lb: u16| only.is_none_or(|o| o.contains(&lb));
    let grid = c.grid;
    let (vertices, settled) = gather_vertices(packs, &c.overrides, &grid);
    c.settled.insert(Part::Vertices, settled);
    let mut blocks: BTreeMap<u16, BlockEdits<'_>> = BTreeMap::new();
    for (pi, p) in packs.iter().enumerate() {
        for e in &p.edits {
            if !wanted(e.landblock) {
                continue;
            }
            blocks
                .entry(e.landblock)
                .or_insert_with(|| BlockEdits { files: Vec::new() })
                .files
                .push((pi, e));
        }
    }
    // The vertices first: every landblock holding a copy of an edited vertex.
    let mut terrain: BTreeMap<u16, (CellLandblock, u16)> = BTreeMap::new();
    let landblock_of = |c: &Compiled, lb: u16| -> Result<(CellLandblock, u16), Error> {
        let id = (u32::from(lb) << 16) | 0xFFFF;
        let view = View { base, c };
        let (bytes, version) = view
            .get(ModernDat::Cell, id)
            .ok_or_else(|| Error::Dat(format!("the files have no landblock {lb:04X}")))?;
        let decoded = base
            .decode::<CellLandblock>(DataId(id), &bytes)
            .map_err(|e| Error::Dat(format!("landblock {lb:04X}: {e}")))?;
        Ok((decoded, version))
    };
    for ((gx, gy, field), (value, _)) in &vertices {
        for (lb, x, y) in copies(&grid, *gx, *gy) {
            if !wanted(lb) {
                continue;
            }
            let (t, _) = match terrain.entry(lb) {
                std::collections::btree_map::Entry::Occupied(e) => e.into_mut(),
                std::collections::btree_map::Entry::Vacant(e) => e.insert(landblock_of(c, lb)?),
            };
            crate::land::set(t, x, y, *field, *value);
        }
    }
    let region = {
        let view = View { base, c };
        view.get_portal(REGION.raw())
            .and_then(|(b, _)| base.decode::<Region>(REGION, &b).ok())
    };
    for (lb, edits) in &blocks {
        c.cells.remove(lb);
        c.rooms.remove(lb);
        c.dropped.remove(lb);
        c.houses.remove(lb);
        let has_statics = edits.files.iter().any(|(_, e)| e.touches_statics());
        if !has_statics {
            continue;
        }
        if !terrain.contains_key(lb) {
            terrain.insert(*lb, landblock_of(c, *lb)?);
        }
        let lbi_id = (u32::from(*lb) << 16) | 0xFFFE;
        let existing = {
            let view = View { base, c };
            view.get(ModernDat::Cell, lbi_id)
        };
        let made = existing.is_none();
        let (mut info, version) = match existing {
            Some((bytes, v)) => (
                base.decode::<LandblockInfo>(DataId(lbi_id), &bytes)
                    .map_err(|e| Error::Dat(format!("{lbi_id:#010X}: {e}")))?,
                v,
            ),
            None => (
                LandblockInfo {
                    id: DataId(lbi_id),
                    num_cells: 0,
                    objects: Vec::new(),
                    pack_mask: 0,
                    buildings: Vec::new(),
                    restrictions: None,
                },
                NEW_RECORD_VERSION,
            ),
        };
        let (t, _) = &terrain[lb];
        let t = t.clone();
        let ground_z =
            |x: f32, y: f32| -> f32 { region.as_ref().map_or(0.0, |r| ground(&t, r, *lb, x, y)) };
        let base_len = info.objects.len();
        let overrides = c.overrides.clone();
        let mut settled = Settled::default();
        let name_of = |pi: usize| packs[pi].name.as_str();
        let no_static = |at: &Loc, i: usize| {
            Error::source(
                at,
                format!("landblock {lb:04X} has {base_len} statics; there is no {i}"),
            )
        };
        // Base statics: each source's removal or move of one, by its index, checked against what
        // the base holds.
        let mut changes: BTreeMap<usize, Vec<(Claim, Option<Frame>)>> = BTreeMap::new();
        for (pi, e) in &edits.files {
            for r in &e.removes {
                let Which::Index(i) = r.item.which else {
                    continue;
                };
                let Some(o) = info.objects.get(i) else {
                    return Err(no_static(&r.at, i));
                };
                if let Some(m) = r.item.model {
                    if m != o.id {
                        return Err(Error::source(
                            &r.at,
                            format!(
                                "static {i} of landblock {lb:04X} is {:#010X}, not {:#010X}",
                                o.id.raw(),
                                m.raw()
                            ),
                        ));
                    }
                }
                changes.entry(i).or_default().push((
                    Claim::new(name_of(*pi), &r.at, "removes it", "removed"),
                    None,
                ));
            }
            for m in &e.moves {
                let Which::Index(i) = m.item.which else {
                    continue;
                };
                let Some(o) = info.objects.get(i) else {
                    return Err(no_static(&m.at, i));
                };
                let frame = moved(o.frame, &m.item, &ground_z);
                changes.entry(i).or_default().push((
                    Claim::new(
                        name_of(*pi),
                        &m.at,
                        move_says(&m.item),
                        format!("{frame:?}"),
                    ),
                    Some(frame),
                ));
            }
        }
        let mut removed: BTreeSet<usize> = BTreeSet::new();
        let mut moved_to: BTreeMap<usize, Frame> = BTreeMap::new();
        for (i, list) in &changes {
            let claims: Vec<Claim> = list.iter().map(|(c, _)| c.clone()).collect();
            let o = info.objects[*i].frame.origin;
            let item = Item::new(format!("static {lb:04X} {i}")).at(*lb, o.x, o.y);
            match list[conflict::settle(&overrides, &item, &claims, &mut settled)].1 {
                None => {
                    removed.insert(*i);
                }
                Some(f) => {
                    moved_to.insert(*i, f);
                }
            }
        }
        // The packs' statics, placed in load order; a named one is one item, whoever places it.
        let mut added: Vec<Added> = Vec::new();
        let mut placed_by: BTreeMap<String, Vec<(Claim, usize)>> = BTreeMap::new();
        for (pi, e) in &edits.files {
            for (entry, s) in e.statics.iter().enumerate() {
                let model = match &s.item.model {
                    RecordRef::Id(id) => *id,
                    RecordRef::Symbol(sym) => {
                        DataId(c.lock.get(Kind::Setup, sym).ok_or_else(|| {
                            Error::source(&s.at, format!("no setup {sym} was numbered"))
                        })?)
                    }
                };
                let frame = frame_at(s.item.at, quat_of(s.item.rotation), &ground_z);
                if let Some(name) = &s.item.name {
                    placed_by.entry(name.clone()).or_default().push((
                        Claim::new(
                            name_of(*pi),
                            &s.at,
                            format!("places {:#010X} at {}", model.raw(), place_text(s.item.at)),
                            format!("{:?} {frame:?}", s.item.model),
                        ),
                        added.len(),
                    ));
                }
                added.push(Added {
                    name: s.item.name.clone(),
                    pack: *pi,
                    entry,
                    object: ObjectEntry { id: model, frame },
                });
            }
        }
        let mut dropped: BTreeSet<usize> = BTreeSet::new();
        for (name, list) in &placed_by {
            if list.len() < 2 {
                continue;
            }
            let claims: Vec<Claim> = list.iter().map(|(c, _)| c.clone()).collect();
            let o = added[list[0].1].object.frame.origin;
            let item = Item::new(format!("static {lb:04X} {name}")).at(*lb, o.x, o.y);
            let w = conflict::settle(&overrides, &item, &claims, &mut settled);
            dropped.extend(
                list.iter()
                    .enumerate()
                    .filter(|(j, _)| *j != w)
                    .map(|(_, (_, k))| *k),
            );
        }
        let mut k = 0;
        added.retain(|_| {
            k += 1;
            !dropped.contains(&(k - 1))
        });
        // The packs' statics moved or removed, by symbol.
        let mut named: BTreeMap<&String, Vec<(Claim, Option<Frame>)>> = BTreeMap::new();
        for (pi, e) in &edits.files {
            for r in &e.removes {
                if let Which::Symbol(sym) = &r.item.which {
                    named.entry(sym).or_default().push((
                        Claim::new(name_of(*pi), &r.at, "removes it", "removed"),
                        None,
                    ));
                }
            }
            for m in &e.moves {
                if let Which::Symbol(sym) = &m.item.which {
                    let Some(a) = added.iter().find(|a| a.name.as_ref() == Some(sym)) else {
                        return Err(Error::source(
                            &m.at,
                            format!("no pack places a static {sym} in landblock {lb:04X}"),
                        ));
                    };
                    let frame = moved(a.object.frame, &m.item, &ground_z);
                    named.entry(sym).or_default().push((
                        Claim::new(
                            name_of(*pi),
                            &m.at,
                            move_says(&m.item),
                            format!("{frame:?}"),
                        ),
                        Some(frame),
                    ));
                }
            }
        }
        for (sym, list) in &named {
            let Some(a) = added.iter().position(|a| a.name.as_ref() == Some(*sym)) else {
                return Err(Error::source(
                    &list[0].0.at,
                    format!("no pack places a static {sym} in landblock {lb:04X}"),
                ));
            };
            let claims: Vec<Claim> = list.iter().map(|(c, _)| c.clone()).collect();
            let o = added[a].object.frame.origin;
            let item = Item::new(format!("static {lb:04X} {sym}")).at(*lb, o.x, o.y);
            match list[conflict::settle(&overrides, &item, &claims, &mut settled)].1 {
                None => {
                    added.remove(a);
                }
                Some(f) => added[a].object.frame = f,
            }
        }
        let mut objects = Vec::with_capacity(base_len + added.len());
        let mut origins = Vec::with_capacity(base_len + added.len());
        for (i, o) in info.objects.iter().enumerate() {
            if removed.contains(&i) {
                continue;
            }
            let mut o = *o;
            if let Some(f) = moved_to.get(&i) {
                o.frame = *f;
            }
            objects.push(o);
            origins.push(StaticOrigin::Base(i));
        }
        for a in &added {
            objects.push(a.object);
            origins.push(StaticOrigin::Pack {
                pack: a.pack,
                name: a.name.clone(),
                entry: a.entry,
            });
        }
        let (n_removed, n_moved, n_added) = (removed.len(), moved_to.len(), added.len());
        info.objects = objects;
        // The cells: new ones placed, buildings moved with theirs.
        let adds: Vec<_> = edits
            .files
            .iter()
            .flat_map(|(_, e)| e.cells.iter())
            .collect();
        // A building moved by two sources is one item.
        let mut building_claims: BTreeMap<usize, Vec<(Claim, &Located<MoveBuilding>)>> =
            BTreeMap::new();
        for (pi, e) in &edits.files {
            for m in &e.buildings {
                building_claims.entry(m.item.index).or_default().push((
                    Claim::new(
                        name_of(*pi),
                        &m.at,
                        building_says(&m.item),
                        format!("{:?} {:?}", m.item.at, m.item.rotation),
                    ),
                    m,
                ));
            }
        }
        let mut building_moves: Vec<&Located<MoveBuilding>> = Vec::new();
        for (i, list) in &building_claims {
            let claims: Vec<Claim> = list.iter().map(|(c, _)| c.clone()).collect();
            let item = Item::new(format!("building {lb:04X} {i}"));
            let item = match info.buildings.get(*i) {
                Some(b) => item.at(*lb, b.frame.origin.x, b.frame.origin.y),
                None => item,
            };
            building_moves.push(list[conflict::settle(&overrides, &item, &claims, &mut settled)].1);
        }
        // A cell of the landblock's own reshaped or taken away by two sources is one item.
        let mut replace_claims: BTreeMap<u16, Vec<(Claim, CellChange<'_>)>> = BTreeMap::new();
        for (pi, e) in &edits.files {
            for r in &e.replaces {
                let item = &r.item;
                replace_claims.entry(item.cell).or_default().push((
                    Claim::new(
                        name_of(*pi),
                        &r.at,
                        format!(
                            "reshapes it as {:04X}/{} at ({}, {}, {})",
                            item.environment.raw() & 0xFFFF,
                            item.piece,
                            item.at[0],
                            item.at[1],
                            item.at[2]
                        ),
                        format!("{item:?}"),
                    ),
                    CellChange::Reshape(r),
                ));
            }
            for t in &e.taken {
                replace_claims.entry(t.item.cell).or_default().push((
                    Claim::new(name_of(*pi), &t.at, "takes it away", "taken away"),
                    CellChange::Take(t),
                ));
            }
        }
        let mut replaces: Vec<&Located<source::ReplaceCell>> = Vec::new();
        let mut taken: Vec<u16> = Vec::new();
        for (n, list) in &replace_claims {
            let claims: Vec<Claim> = list.iter().map(|(c, _)| c.clone()).collect();
            let item = Item::new(format!("cell {lb:04X} {n:04X}"));
            let item = match list[0].1 {
                CellChange::Reshape(r) => item.at(*lb, r.item.at[0], r.item.at[1]),
                CellChange::Take(_) => item,
            };
            match list[conflict::settle(&overrides, &item, &claims, &mut settled)].1 {
                CellChange::Reshape(r) => replaces.push(r),
                CellChange::Take(t) => taken.push(t.item.cell),
            }
        }
        if settled != Settled::default() {
            c.settled.insert(Part::Statics(*lb), settled);
        }
        let dropped = drop_buildings(edits, *lb, base, c, region.as_ref(), &ground_z)?;
        if !adds.is_empty()
            || !building_moves.is_empty()
            || !replaces.is_empty()
            || !taken.is_empty()
            || !dropped.is_empty()
        {
            let view = View { base, c };
            let mut base_cells = BTreeMap::new();
            let mut versions = BTreeMap::new();
            let mut n = crate::cells::FIRST_CELL;
            while let Some((bytes, v)) =
                view.get(ModernDat::Cell, (u32::from(*lb) << 16) | u32::from(n))
            {
                let id = DataId((u32::from(*lb) << 16) | u32::from(n));
                let cell = base
                    .decode::<dereth_assets::world::EnvCell>(id, &bytes)
                    .map_err(|e| Error::Dat(format!("{:#010X}: {e}", id.raw())))?;
                base_cells.insert(n, cell);
                versions.insert(n, v);
                n += 1;
            }
            let env = |id: DataId| {
                read_compiled(c, base, ModernDat::Portal, id.raw())
                    .and_then(|b| base.decode::<dereth_assets::Environment>(id, &b).ok())
            };
            let numbers = adds
                .iter()
                .map(|a| {
                    c.lock
                        .get(Kind::Cell, &cell_symbol(&a.item.name, *lb))
                        .and_then(|id| u16::try_from(id & 0xFFFF).ok())
                        .ok_or_else(|| {
                            Error::source(&a.at, format!("cell {} was not numbered", a.item.name))
                        })
                })
                .collect::<Result<Vec<u16>, Error>>()?;
            let placed = crate::cells::place(
                *lb,
                &mut info,
                &base_cells,
                &crate::cells::Changes {
                    adds: &adds,
                    numbers: &numbers,
                    moves: &building_moves,
                    replaces: &replaces,
                    taken: &taken,
                    drops: &dropped,
                },
                &env,
            )?;
            c.summary.extend(placed.summary);
            if !placed.warnings.is_empty() {
                c.warnings.insert(*lb, placed.warnings);
            }
            if !placed.names.is_empty() {
                c.cells.insert(*lb, placed.names.clone());
            }
            if !placed.rooms.is_empty() {
                c.rooms.insert(*lb, placed.rooms.clone());
            }
            if !placed.dropped.is_empty() {
                c.dropped.insert(*lb, placed.dropped.clone());
            }
            for (n, cell) in placed.cells {
                let id = (u32::from(*lb) << 16) | u32::from(n);
                let version = versions.get(&n).copied().unwrap_or(NEW_RECORD_VERSION);
                c.records.entry(ModernDat::Cell).or_default().insert(
                    id,
                    Staged {
                        bytes: written(id, base, cell.encode_for(base.era))?.into(),
                        version,
                    },
                );
            }
        }
        c.statics.insert(*lb, origins);
        if made {
            if let Some((t, _)) = terrain.get_mut(lb) {
                t.lbi_exists = 1;
            }
        }
        c.summary.push(format!(
            "{lb:04X}: {n_added} static(s) placed, {n_moved} moved, {n_removed} removed{}",
            if made {
                ", its statics record made"
            } else {
                ""
            }
        ));
        let bytes = written(lbi_id, base, info.encode_for(base.era))?;
        c.records.entry(ModernDat::Cell).or_default().insert(
            lbi_id,
            Staged {
                bytes: bytes.into(),
                version,
            },
        );
    }
    // The statics standing in indoor cells, moved and removed.
    for (lb, edits) in &blocks {
        if edits.files.iter().any(|(_, e)| e.touches_cell_statics()) {
            cell_statics(packs, *lb, edits, base, c)?;
        }
    }
    let vertex_blocks: BTreeSet<u16> = vertices
        .keys()
        .flat_map(|(gx, gy, _)| copies(&grid, *gx, *gy).into_iter().map(|(lb, _, _)| lb))
        .filter(|lb| wanted(*lb))
        .collect();
    for lb in &vertex_blocks {
        c.summary.push(format!("{lb:04X}: terrain edited"));
    }
    for (lb, (t, version)) in terrain {
        let id = (u32::from(lb) << 16) | 0xFFFF;
        c.records.entry(ModernDat::Cell).or_default().insert(
            id,
            Staged {
                bytes: written(id, base, t.encode_for(base.era))?.into(),
                version,
            },
        );
    }
    c.summary.sort();
    Ok(())
}

fn fill_sql(
    packs: &[Pack],
    defined: &Defined,
    tokens: &BTreeMap<(usize, usize), Vec<Token>>,
    c: &mut Compiled,
) -> Result<(), Error> {
    let mut cells = cell_symbols(packs, None);
    for (lb, rooms) in &c.rooms {
        cells.extend(rooms.keys().map(|r| (Kind::Cell, cell_symbol(r, *lb))));
    }
    // Each file filled: its pack, its place in the pack, its path under the output's folder.
    let mut filled: Vec<(usize, usize, PathBuf, String)> = Vec::new();
    for (pi, p) in packs.iter().enumerate() {
        for (fi, f) in p.sql.iter().enumerate() {
            let toks = &tokens[&(pi, fi)];
            for t in toks {
                if t.kind == Kind::Cell && !cells.contains(&(t.kind, t.symbol.clone())) {
                    return Err(Error::source(
                        &t.at,
                        format!(
                            "no pack places cell {}, which a spawn here stands in",
                            t.symbol
                        ),
                    ));
                }
                if t.kind.is_record() && !defined.contains_key(&(t.kind, t.symbol.clone())) {
                    return Err(Error::source(
                        &t.at,
                        format!("no pack defines {} {}", t.kind, t.symbol),
                    ));
                }
            }
            let lock = &c.lock;
            let text = sql::fill(&f.text, toks, |t| lock.get(t.kind, &t.symbol).unwrap_or(0));
            filled.push((pi, fi, Path::new(&p.name).join(&f.rel), text));
        }
    }
    // Every weenie class, spawn and quest two files set, settled; the statements of a source
    // that does not stand are left out of its file.
    let mut items: BTreeMap<SqlKey, Vec<(usize, SqlSet)>> = BTreeMap::new();
    for (k, (_, _, _, text)) in filled.iter().enumerate() {
        for set in conflict::sql_sets(text) {
            items.entry(set.key.clone()).or_default().push((k, set));
        }
    }
    let mut settled = Settled::default();
    let mut lost: BTreeMap<usize, BTreeSet<SqlKey>> = BTreeMap::new();
    for (key, sets) in &items {
        if sets.len() < 2 {
            continue;
        }
        let named: Vec<(String, Loc, &SqlSet)> = sets
            .iter()
            .map(|(k, set)| {
                let (pi, fi, _, _) = &filled[*k];
                (
                    packs[*pi].name.clone(),
                    Loc::new(&packs[*pi].sql[*fi].path, set.line),
                    set,
                )
            })
            .collect();
        let claims = conflict::sql_claims(&named);
        let item = Item {
            name: key.name(),
            landblocks: sets
                .iter()
                .flat_map(|(_, s)| s.landblocks.iter().copied())
                .collect(),
            place: sets.iter().find_map(|(_, s)| s.place),
        };
        let w = conflict::settle(&c.overrides, &item, &claims, &mut settled);
        for (j, (k, _)) in sets.iter().enumerate() {
            if claims[j].value() != claims[w].value() {
                lost.entry(*k).or_default().insert(key.clone());
            }
        }
    }
    for (k, (_, _, rel, text)) in filled.into_iter().enumerate() {
        let text = match lost.get(&k) {
            None => Some(text),
            Some(keys) => conflict::without(&text, keys),
        };
        if let Some(text) = text {
            c.sql.insert(rel, text);
        }
    }
    c.settled.insert(Part::Sql, settled);
    Ok(())
}

/// Every token of the packs' SQL, by pack and file, and every symbol a build numbers: the records
/// the packs define and the weenies and instances their SQL names.
type SqlSymbols = (
    BTreeMap<(usize, usize), Vec<Token>>,
    BTreeSet<(Kind, String)>,
);

fn sql_symbols(packs: &[Pack], defined: &Defined) -> Result<SqlSymbols, Error> {
    let mut tokens: BTreeMap<(usize, usize), Vec<Token>> = BTreeMap::new();
    let mut wanted: BTreeSet<(Kind, String)> = defined.keys().cloned().collect();
    for (pi, p) in packs.iter().enumerate() {
        for (fi, f) in p.sql.iter().enumerate() {
            let toks = sql::tokens(&f.path, &p.name, &f.text)?;
            // A cell is numbered where it is placed: a pack's cell with the others, a dropped
            // building's room with its building's, in their order.
            for t in &toks {
                if !t.kind.is_record() && t.kind != Kind::Cell {
                    wanted.insert((t.kind, t.symbol.clone()));
                }
            }
            tokens.insert((pi, fi), toks);
        }
    }
    Ok((tokens, wanted))
}

/// The symbol of every new cell the packs place (in the landblocks in `only`, or everywhere):
/// its name and its landblock, as the lockfile keeps it.
fn cell_symbols(packs: &[Pack], only: Option<&BTreeSet<u16>>) -> BTreeSet<(Kind, String)> {
    packs
        .iter()
        .flat_map(|p| &p.edits)
        .filter(|e| only.is_none_or(|o| o.contains(&e.landblock)))
        .flat_map(|e| {
            e.cells
                .iter()
                .map(move |c| (Kind::Cell, cell_symbol(&c.item.name, e.landblock)))
        })
        .collect()
}

/// The instance symbols the packs' dropped buildings and copied dungeons number: their houses'
/// and other spawns'.
fn drop_spawn_symbols(packs: &[Pack]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for e in packs.iter().flat_map(|p| &p.edits) {
        let lb = e.landblock;
        for d in &e.drops {
            for h in d.item.house.iter().chain(&d.item.spawns) {
                out.insert(crate::drops::instance_symbol(h, lb));
            }
        }
        for d in &e.dungeon_copies {
            for h in d.item.house.iter().chain(&d.item.spawns) {
                out.insert(crate::drops::instance_symbol(h, lb));
            }
        }
    }
    out
}

/// A new cell's symbol: its name (in full) and its landblock.
#[must_use]
pub fn cell_symbol(name: &str, landblock: u16) -> String {
    format!("{name}@{landblock:04X}")
}

/// Leave to the base every record (of the landblocks in `only`, or every record) that came out
/// as the base holds it, but one an import deletes: the base's copy is hidden, so the world holds
/// it only as its own.
fn leave_to_base(c: &mut Compiled, base: &DatSet, only: Option<&BTreeSet<u16>>) {
    for (t, records) in &mut c.records {
        let Some(f) = base.file(*t) else { continue };
        let deleted = covered(&c.tombstones, *t);
        #[allow(clippy::cast_possible_truncation)] // a cell id's top half is its landblock
        records.retain(|id, s| {
            if only.is_some_and(|o| *t != ModernDat::Cell || !o.contains(&((id >> 16) as u16))) {
                return true;
            }
            f.base_entry(DataId(*id)).is_none()
                || deleted(*id)
                || f.read(DataId(*id)).ok().as_deref() != Some(&*s.bytes)
        });
    }
    c.records.retain(|_, r| !r.is_empty());
}

/// Make the terrain of every landblock whose statics record the world writes (of the landblocks
/// in `only`, or every one) say it has one: the client loads a landblock's statics only when its
/// terrain says so. Every terrain of the end of retail's files with a statics record says so
/// already; a few of the February 2005 files' do not, and their statics are not drawn until a
/// world changes them.
fn flag_statics(
    c: &mut Compiled,
    base: &DatSet,
    only: Option<&BTreeSet<u16>>,
) -> Result<(), Error> {
    #[allow(clippy::cast_possible_truncation)] // a cell id's top half is its landblock
    let lbis: Vec<u32> = c
        .records
        .get(&ModernDat::Cell)
        .into_iter()
        .flat_map(BTreeMap::keys)
        .copied()
        .filter(|id| id & 0xFFFF == 0xFFFE && only.is_none_or(|o| o.contains(&((id >> 16) as u16))))
        .collect();
    for lbi in lbis {
        let id = lbi | 0xFFFF;
        let held = c
            .records
            .get(&ModernDat::Cell)
            .and_then(|r| r.get(&id))
            .map(|s| (s.bytes.to_vec(), s.version))
            .or_else(|| View { base, c }.get(ModernDat::Cell, id));
        let Some((bytes, version)) = held else {
            continue;
        };
        let mut t = base
            .decode::<CellLandblock>(DataId(id), &bytes)
            .map_err(|e| Error::Dat(format!("{id:#010X}: {e}")))?;
        if t.lbi_exists != 0 {
            continue;
        }
        t.lbi_exists = 1;
        let bytes = written(id, base, t.encode_for(base.era))?;
        c.records.entry(ModernDat::Cell).or_default().insert(
            id,
            Staged {
                bytes: bytes.into(),
                version,
            },
        );
    }
    Ok(())
}

/// Compile `packs` (the world's, in load order) over `base`, keeping the ids `lock` holds.
///
/// # Errors
/// A source that does not hold together (an unknown symbol, a static that is not there, an
/// override that names no pack of the world), and the data files' errors. Conflicts are no
/// error here: they are kept in [`Compiled::settled`].
pub fn compile(
    world: &World,
    packs: &[Pack],
    base: &DatSet,
    lock: &Lock,
) -> Result<Compiled, Error> {
    compile_with(world, packs, base, lock, None)
}

/// [`compile`], with the player's Asheron's Call 2 install `ac2` the AC2 dungeon kits the packs
/// ask for are made from ([`crate::ac2_kit`]).
///
/// # Errors
/// As [`compile`], and a pack that asks for an AC2 kit when there is no install to make it from.
pub fn compile_with(
    world: &World,
    packs: &[Pack],
    base: &DatSet,
    lock: &Lock,
    ac2: Option<&dereth_ac2_convert::source::Ac2>,
) -> Result<Compiled, Error> {
    for o in &world.overrides {
        for name in [&o.pack, &o.over] {
            if !packs.iter().any(|p| p.name == *name) {
                return Err(Error::source(
                    &o.at,
                    format!("the world has no pack named {name}"),
                ));
            }
        }
    }
    let mut c = Compiled {
        lock: lock.clone(),
        overrides: world.overrides.clone(),
        ..Compiled::default()
    };
    read_imports(world, base, &mut c)?;
    land_layer(world, base, &mut c)?;
    check_vertices(packs, &c.grid)?;
    let defs = defined_records(packs, &c.overrides);
    // Every symbol the SQL names, and every new cell, read before anything is numbered.
    let (tokens, mut wanted) = sql_symbols(packs, &defs.defined)?;
    wanted.extend(cell_symbols(packs, None));
    let mut lock = c.lock.clone();
    c.numbered = number(
        &mut lock,
        &wanted,
        base,
        &c.imported,
        &c.tombstones,
        world.id_start(Kind::Wcid),
    )?;
    c.lock = lock;
    build_records(packs, &defs, base, &mut c)?;
    c.settled.insert(Part::Records, defs.settled.clone());
    ac2_kits(packs, &world.imports, base, ac2, &mut c)?;
    edit_landblocks(packs, base, &mut c, None)?;
    game_tables(packs, base, &mut c)?;
    fill_sql(packs, &defs.defined, &tokens, &mut c)?;
    // A record that came out as the base holds it is left to the base.
    leave_to_base(&mut c, base, None);
    flag_statics(&mut c, base, None)?;
    Ok(c)
}

/// The game's tables the packs change, over those below, on the terrain as edited.
fn game_tables(packs: &[Pack], base: &DatSet, c: &mut Compiled) -> Result<(), Error> {
    let mut settled = Settled::default();
    let written = {
        let view = View { base, c };
        crate::tables::compile(packs, &c.overrides, base.era, &view, &mut settled)?
    };
    for w in written {
        let was = c.records.entry(ModernDat::Portal).or_default().insert(
            w.id,
            Staged {
                bytes: w.bytes.into(),
                version: w.version,
            },
        );
        c.tables_below.entry(w.id).or_insert(was);
        c.tables_said.push(w.summary.clone());
        c.summary.push(w.summary);
    }
    c.settled.insert(Part::Tables, settled);
    Ok(())
}

/// Compile the game's tables again, after the packs' rows of them changed: each table record
/// they wrote is put back as it was below them, then the tables are compiled over it as
/// [`compile`] compiles them. A table that comes out as the base holds it is left to the base.
///
/// # Errors
/// As [`compile`]: a row that doesn't apply, or a conflict.
pub fn recompile_tables(packs: &[Pack], base: &DatSet, c: &mut Compiled) -> Result<(), Error> {
    let portal = c.records.entry(ModernDat::Portal).or_default();
    for (id, was) in std::mem::take(&mut c.tables_below) {
        match was {
            Some(s) => {
                portal.insert(id, s);
            }
            None => {
                portal.remove(&id);
            }
        }
    }
    let said = std::mem::take(&mut c.tables_said);
    c.summary.retain(|l| !said.contains(l));
    game_tables(packs, base, c)?;
    if let (Some(f), Some(portal)) = (
        base.file(ModernDat::Portal),
        c.records.get_mut(&ModernDat::Portal),
    ) {
        let ids: Vec<u32> = c.tables_below.keys().copied().collect();
        for id in ids {
            if portal.get(&id).is_some_and(|s| {
                f.base_entry(DataId(id)).is_some()
                    && f.read(DataId(id)).ok().as_deref() == Some(&*s.bytes)
            }) {
                portal.remove(&id);
            }
        }
    }
    c.records.retain(|_, r| !r.is_empty());
    Ok(())
}

/// Compile again the landblocks in `touched`, after their operations changed: their terrain and
/// statics records are made again from the imports and the base, and replace what `c` held for
/// them. Every other record of `c` stands. The result is what [`compile`] makes of the same
/// packs, as long as `touched` holds every landblock the change reaches: those whose files
/// changed, and every landblock that holds a copy of a vertex whose value changed.
///
/// # Errors
/// As [`compile`].
pub fn recompile_landblocks(
    packs: &[Pack],
    base: &DatSet,
    c: &mut Compiled,
    touched: &BTreeSet<u16>,
) -> Result<(), Error> {
    let none = BTreeMap::new();
    let imported = c.imported.get(&ModernDat::Cell).unwrap_or(&none);
    let cells = c.records.entry(ModernDat::Cell).or_default();
    for lb in touched {
        // The landblock's terrain, statics and indoor cells, as the imports left them.
        let ids: Vec<u32> = cells
            .range((u32::from(*lb) << 16)..=((u32::from(*lb) << 16) | 0xFFFF))
            .map(|(id, _)| *id)
            .filter(|id| matches!(id & 0xFFFF, 0xFFFE | 0xFFFF) || id & 0xFFFF >= 0x0100)
            .collect();
        for id in ids {
            cells.remove(&id);
        }
        for low in [0xFFFF, 0xFFFE] {
            let id = (u32::from(*lb) << 16) | low;
            if let Some(s) = imported.get(&id) {
                cells.insert(id, s.clone());
            }
        }
        for (id, s) in
            imported.range((u32::from(*lb) << 16) | 0x0100..((u32::from(*lb) << 16) | 0xFFFE))
        {
            cells.insert(*id, s.clone());
        }
        c.statics.remove(lb);
        c.warnings.remove(lb);
        c.cells.remove(lb);
        c.rooms.remove(lb);
        c.dropped.remove(lb);
        c.houses.remove(lb);
        let first = u32::from(*lb) << 16;
        c.cell_statics
            .retain(|id, _| !(first..=first | 0xFFFF).contains(id));
        c.cell_static_adds
            .retain(|id, _| !(first..=first | 0xFFFF).contains(id));
        c.settled.remove(&Part::Statics(*lb));
        c.settled.remove(&Part::CellStatics(*lb));
    }
    let names: BTreeSet<String> = touched.iter().map(|lb| format!("{lb:04X}:")).collect();
    c.summary
        .retain(|l| !names.iter().any(|n| l.starts_with(n.as_str())));
    // A new cell is numbered as a build numbers it, and keeps its number after.
    let mut lock = c.lock.clone();
    let numbered = number(
        &mut lock,
        &cell_symbols(packs, Some(touched)),
        base,
        &c.imported,
        &c.tombstones,
        0,
    )?;
    c.lock = lock;
    c.numbered.extend(numbered);
    edit_landblocks(packs, base, c, Some(touched))?;
    leave_to_base(c, base, Some(touched));
    flag_statics(c, base, Some(touched))?;
    Ok(())
}

/// Number the symbols and fill the SQL again, after the packs' SQL changed: every symbol is
/// numbered from `lock`, the lockfile the world was opened with, as [`compile`] numbers them, and
/// every file is filled anew. The records stand.
///
/// # Errors
/// A token that does not read, a record symbol no pack defines, a span with no id left.
pub fn refresh_sql(
    world: &World,
    packs: &[Pack],
    base: &DatSet,
    lock: &Lock,
    c: &mut Compiled,
) -> Result<(), Error> {
    let defined = defined_records(packs, &c.overrides).defined;
    let (tokens, mut wanted) = sql_symbols(packs, &defined)?;
    wanted.extend(cell_symbols(packs, None));
    let mut lock = lock.clone();
    // The cells keep the numbers their records were made with, and the loot tables the numbers
    // they were first given, which the editor knows them by; so do the spawns a dropped
    // building's or copied dungeon's row names, which a world repository's placements carry.
    for kind in [Kind::Cell, Kind::Treasure] {
        for (symbol, id) in c.lock.of(kind) {
            if lock.get(kind, symbol).is_none() {
                lock.insert(kind, symbol, id);
            }
        }
    }
    let dropped = drop_spawn_symbols(packs);
    for (symbol, id) in c.lock.of(Kind::Guid) {
        if dropped.contains(symbol) && lock.get(Kind::Guid, symbol).is_none() {
            lock.insert(Kind::Guid, symbol, id);
        }
    }
    c.numbered = number(
        &mut lock,
        &wanted,
        base,
        &c.imported,
        &c.tombstones,
        world.id_start(Kind::Wcid),
    )?;
    c.lock = lock;
    c.sql.clear();
    fill_sql(packs, &defined, &tokens, c)
}

/// Write `c` as an overlay folder at `dir` (made fresh) over `base`, for the world `key`.
///
/// # Errors
/// The writer's errors.
pub fn write_overlay(c: &Compiled, base: &DatSet, key: &str, dir: &Path) -> Result<(), Error> {
    write_records(&c.records, &c.tombstones, base, key, dir)
}

/// [`write_overlay`] for `world`, numbered by its content in the world's ledger as
/// [`build_numbered`] numbers it (following `last`), the ledger's new line written with it.
/// Answers its generation.
///
/// # Errors
/// The writer's errors, and a ledger that doesn't read or write.
pub fn write_overlay_numbered(
    c: &Compiled,
    base: &DatSet,
    world: &World,
    dir: &Path,
    last: Option<&Path>,
) -> Result<u32, Error> {
    let n = crate::publish::Numbered::of(&world.path, &c.records, &c.tombstones)?;
    write_records_after(
        &c.records,
        &c.tombstones,
        base,
        &world.key,
        dir,
        n.generation,
        last,
    )?;
    n.record()?;
    Ok(n.generation)
}

/// Write `records` and the deletions `tombstones` as an overlay folder at `dir` (made fresh)
/// over `base`, for the world `key`, as [`write_overlay`] writes a compiled world.
///
/// # Errors
/// The writer's errors.
pub fn write_records(
    records: &Records,
    tombstones: &BTreeMap<ModernDat, BTreeSet<(u32, u32)>>,
    base: &DatSet,
    key: &str,
    dir: &Path,
) -> Result<(), Error> {
    write_records_at(records, tombstones, base, key, dir, 1)
}

/// [`write_records`] at generation `generation` of the overlay's content
/// ([`dereth_dat::generations`]): its iterations run on from the files' own, `generation` of them,
/// its records in the last. Generation 1 is [`write_records`]'s one revision.
///
/// # Errors
/// The writer's errors.
pub fn write_records_at(
    records: &Records,
    tombstones: &BTreeMap<ModernDat, BTreeSet<(u32, u32)>>,
    base: &DatSet,
    key: &str,
    dir: &Path,
    generation: u32,
) -> Result<(), Error> {
    if dir.exists() {
        std::fs::remove_dir_all(dir).map_err(|e| Error::io(dir, e))?;
    }
    std::fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
    for t in ModernDat::ALL {
        let records = records.get(&t);
        let gone = tombstones.get(&t);
        if records.is_none_or(BTreeMap::is_empty) && gone.is_none_or(BTreeSet::is_empty) {
            continue;
        }
        let Some(file) = base.file(t) else {
            return Err(Error::Dat(format!(
                "the overlay changes {}, which the files do not have",
                base.base_name(t)
            )));
        };
        let iterations =
            dereth_dat::generations::iterations(decompose::revision_over(file), generation);
        let revision = *iterations.last().expect("one iteration at least");
        let path = dir.join(container_name(t));
        let mut w = OverlayWriter::open_or_create(&path, file, base.base_name(t), key, 0)?;
        for (id, mask) in gone.into_iter().flatten() {
            w.tombstone(DataId(*id), *mask, revision)?;
        }
        for (id, s) in records.into_iter().flatten() {
            w.put(DataId(*id), &s.bytes, s.version, revision, 0)?;
        }
        for i in iterations {
            w.add_iteration(i, 0)?;
        }
        w.flush(0)?;
    }
    Ok(())
}

/// What [`write_records_after`] did: how many records it kept in the iterations they had, how many
/// went into the new generation's (changed or new) and how many it deleted there; and why the
/// last overlay was not followed, when it was not.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Followed {
    /// Whether a last overlay was followed.
    pub followed: bool,
    pub kept: usize,
    pub changed: usize,
    pub deleted: usize,
    pub restarted: Option<String>,
}

/// [`write_records_at`], following `last`, the overlay the world was last served with (it may be
/// `dir` itself): a record that reads as it did there keeps its iteration, and only what changed
/// goes into the generation's last ([`dereth_dat::revise::revise_folder_at`]), so a client is sent
/// what changed alone. Without a last overlay, or with one this world's can't follow (another
/// world's, over other files, or numbered past `generation`), everything is the generation's, as
/// [`write_records_at`] writes it, and why is answered. `dir` is replaced once the whole overlay is
/// written.
///
/// # Errors
/// The writer's errors, and a last overlay that won't read.
pub fn write_records_after(
    records: &Records,
    tombstones: &BTreeMap<ModernDat, BTreeSet<(u32, u32)>>,
    base: &DatSet,
    key: &str,
    dir: &Path,
    generation: u32,
    last: Option<&Path>,
) -> Result<Followed, Error> {
    let everything = || Followed {
        changed: records.values().map(BTreeMap::len).sum(),
        deleted: tombstones.values().map(BTreeSet::len).sum(),
        ..Followed::default()
    };
    let Some(last) = last.filter(|l| l.is_dir()) else {
        write_records_at(records, tombstones, base, key, dir, generation)?;
        return Ok(everything());
    };
    let name = dir
        .file_name()
        .map_or_else(|| "overlay".into(), |n| n.to_string_lossy().into_owned());
    let fresh = dir.with_file_name(format!(".{name}-fresh"));
    let next = dir.with_file_name(format!(".{name}-next"));
    let result = (|| -> Result<Followed, Error> {
        write_records_at(records, tombstones, base, key, &fresh, generation)?;
        if next.exists() {
            std::fs::remove_dir_all(&next).map_err(|e| Error::io(&next, e))?;
        }
        std::fs::create_dir_all(&next).map_err(|e| Error::io(&next, e))?;
        let bases: Vec<(ModernDat, dereth_dat::DatFile)> =
            base.files().map(|(t, f)| (t, f.clone())).collect();
        let followed = dereth_dat::revise::revise_folder_at(
            &bases,
            Some(&OverlayDir::new(last)?),
            &OverlayDir::new(&fresh)?,
            &OverlayDir::new(&next)?,
            key,
            Some(generation),
        );
        let (from, done) = match followed {
            Ok(files) => (
                &next,
                Followed {
                    followed: true,
                    kept: files.iter().map(|(_, r)| r.kept).sum(),
                    changed: files.iter().map(|(_, r)| r.changed).sum(),
                    deleted: files.iter().map(|(_, r)| r.deleted).sum(),
                    restarted: None,
                },
            ),
            Err(
                e @ (dereth_dat::revise::ReviseError::Numbering { .. }
                | dereth_dat::revise::ReviseError::OwnIterations(_)
                | dereth_dat::revise::ReviseError::Overlay(
                    dereth_dat::overlay::OverlayError::OtherWorld { .. }
                    | dereth_dat::overlay::OverlayError::BaseMismatch { .. },
                )),
            ) => (
                &fresh,
                Followed {
                    restarted: Some(e.to_string()),
                    ..everything()
                },
            ),
            Err(e) => return Err(Error::Dat(e.to_string())),
        };
        if dir.exists() {
            std::fs::remove_dir_all(dir).map_err(|e| Error::io(dir, e))?;
        }
        std::fs::rename(from, dir).map_err(|e| Error::io(dir, e))?;
        Ok(done)
    })();
    for d in [&fresh, &next] {
        let _ = std::fs::remove_dir_all(d);
    }
    result
}

/// What [`build`] did.
#[derive(Clone, Debug)]
pub struct Built {
    /// The output folder.
    pub out: PathBuf,
    /// Each overlay container written, with how many records it holds and deletes.
    pub containers: Vec<(PathBuf, usize, usize)>,
    /// How many SQL files were written.
    pub sql_files: usize,
    /// The symbols numbered in this build.
    pub numbered: Vec<(Kind, String, u32)>,
    /// One line for each landblock edited.
    pub summary: Vec<String>,
    /// What the checks warn of, which stops nothing.
    pub warnings: Vec<String>,
    /// Whether the build matched what was already there (with `check`, nothing is written).
    pub current: bool,
    /// Where two packs disagreed and the world's overrides said which wins.
    pub overridden: Vec<conflict::Overridden>,
    /// The overlay's generation ([`build_numbered`]); 1 when not numbered.
    pub generation: u32,
}

/// Every data id the compiled SQL names, placed at its pack's source file and line (filling a
/// token in never moves a line).
#[must_use]
pub fn sql_named(packs: &[Pack], c: &Compiled) -> Vec<crate::sql_ids::Named> {
    let mut out = Vec::new();
    for p in packs {
        for f in &p.sql {
            if let Some(text) = c.sql.get(&Path::new(&p.name).join(&f.rel)) {
                out.extend(crate::sql_ids::named_ids(&f.path, text));
            }
        }
    }
    out
}

fn read_tree(dir: &Path) -> Result<BTreeMap<PathBuf, Vec<u8>>, Error> {
    let mut out = BTreeMap::new();
    if !dir.is_dir() {
        return Ok(out);
    }
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).map_err(|e| Error::io(&d, e))? {
            let p = e.map_err(|e| Error::io(&d, e))?.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                let rel = p.strip_prefix(dir).unwrap_or(&p).to_path_buf();
                out.insert(rel, std::fs::read(&p).map_err(|e| Error::io(&p, e))?);
            }
        }
    }
    Ok(out)
}

fn write_sql(c: &Compiled, dir: &Path) -> Result<(), Error> {
    if dir.exists() {
        std::fs::remove_dir_all(dir).map_err(|e| Error::io(dir, e))?;
    }
    for (rel, text) in &c.sql {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        }
        std::fs::write(&path, text).map_err(|e| Error::io(&path, e))?;
    }
    Ok(())
}

fn replace_dir(from: &Path, to: &Path) -> Result<(), Error> {
    if to.exists() {
        std::fs::remove_dir_all(to).map_err(|e| Error::io(to, e))?;
    }
    if from.exists() {
        std::fs::rename(from, to).map_err(|e| Error::io(to, e))?;
    }
    Ok(())
}

/// Build the world at `world_path` over the files in `dats`, into `out` (the world's own output
/// folder when `None`): the overlay in `out/overlay`, the SQL in `out/sql`, and the lockfile beside
/// the world file. With `check`, build into a scratch folder instead, compare, and write nothing.
///
/// # Errors
/// As [`compile`]; every conflict the world's overrides do not settle ([`Error::Conflicts`]),
/// records that fail the checks ([`Error::Invalid`]), and the writer's errors.
pub fn build(
    world_path: &Path,
    dats: &Path,
    out: Option<&Path>,
    check: bool,
) -> Result<Built, Error> {
    build_with(world_path, dats, out, check, None)
}

/// [`build`], with the player's Asheron's Call 2 install `ac2` the AC2 dungeon kits the packs ask
/// for are made from ([`crate::ac2_kit`]).
///
/// # Errors
/// As [`build`], and a pack that asks for an AC2 kit with no install given.
pub fn build_with(
    world_path: &Path,
    dats: &Path,
    out: Option<&Path>,
    check: bool,
    ac2: Option<&dereth_ac2_convert::source::Ac2>,
) -> Result<Built, Error> {
    build_inner(world_path, dats, out, check, ac2, None)
}

/// [`build`], its overlay numbered by its content in the world's ledger
/// ([`crate::publish::Numbered`]), as an export serves it: a changed overlay gets the next
/// generation, so a client that holds the last one patches again, and an unchanged one keeps its
/// own. The ledger's new line is written with the build. Following `last`, the overlay the world
/// was last served with, only what changed takes the new generation ([`write_records_after`]).
///
/// # Errors
/// As [`build`], and a ledger that doesn't read or write.
pub fn build_numbered(
    world_path: &Path,
    dats: &Path,
    out: Option<&Path>,
    last: Option<&Path>,
) -> Result<Built, Error> {
    build_inner(world_path, dats, out, false, None, Some(last))
}

fn build_inner(
    world_path: &Path,
    dats: &Path,
    out: Option<&Path>,
    check: bool,
    ac2: Option<&dereth_ac2_convert::source::Ac2>,
    numbered: Option<Option<&Path>>,
) -> Result<Built, Error> {
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
    let base = DatSet::for_world(dats, world.era)?;
    let lock_path = world.lock_path();
    let lock = Lock::read(&lock_path)?;
    let c = compile_with(&world, &packs, &base, &lock, ac2)?;
    let conflicts = c.conflicts();
    if !conflicts.is_empty() {
        return Err(Error::Conflicts(conflicts.into_iter().cloned().collect()));
    }
    let out = out.map_or_else(|| world.out.clone(), Path::to_path_buf);
    std::fs::create_dir_all(&out).map_err(|e| Error::io(&out, e))?;
    let staging = out.join(".building");
    if staging.exists() {
        std::fs::remove_dir_all(&staging).map_err(|e| Error::io(&staging, e))?;
    }
    let result = (|| {
        let numbered = if let Some(last) = numbered {
            let n = crate::publish::Numbered::of(&world.path, &c.records, &c.tombstones)?;
            write_records_after(
                &c.records,
                &c.tombstones,
                &base,
                &world.key,
                &staging.join("overlay"),
                n.generation,
                last,
            )?;
            Some(n)
        } else {
            write_overlay(&c, &base, &world.key, &staging.join("overlay"))?;
            None
        };
        write_world_files(&world, &c.sql, &staging.join("overlay"))?;
        write_sql(&c, &staging.join("sql"))?;
        // The checks, over the overlay as it was written; its files are closed again before it
        // is moved into place.
        let report = {
            let layered = base.with_overlay(&staging.join("overlay"), Some(&world.key))?;
            let mut report = crate::validate::validate(&layered, &crate::validate::Scope::of(&c));
            report
                .warnings
                .extend(c.warnings.values().flatten().cloned());
            report
                .problems
                .extend(crate::sql_ids::check(&layered, &sql_named(&packs, &c)));
            for x in crate::content_check::check_packs(
                &packs,
                &c,
                &base,
                &crate::content_check::Known::default(),
            ) {
                let line = format!("{}: {}", x.at, x.text);
                if x.warning {
                    report.warnings.push(line);
                } else {
                    report.problems.push(line);
                }
            }
            // The packs' spawns in rooms, each on its room's floor: warnings here, as a build
            // knows no weenie but the packs' own and so can't tell what hangs on a wall (a house's
            // hook) from what stands; the editor, which knows the server's world, finds them
            // faults.
            report.warnings.extend({
                let (classes, symbols) = crate::floor::pack_kinds(&packs);
                crate::floor::check_packs(&packs, &c.lock, &layered, &|p, w| {
                    crate::sql_ids::number(w)
                        .and_then(|n| classes.get(&n))
                        .or_else(|| symbols.get(&crate::edit::token_symbol(&p.name, w)?))
                        .copied()
                })
            });
            report
        };
        if !report.problems.is_empty() {
            return Err(Error::Invalid(report.problems));
        }
        let mut containers = Vec::new();
        for t in ModernDat::ALL {
            let path = staging.join("overlay").join(container_name(t));
            if path.is_file() {
                containers.push((
                    out.join("overlay").join(container_name(t)),
                    c.records.get(&t).map_or(0, BTreeMap::len),
                    c.tombstones.get(&t).map_or(0, BTreeSet::len),
                ));
            }
        }
        let lock_text = c.lock.render();
        let current = read_tree(&staging.join("overlay"))? == read_tree(&out.join("overlay"))?
            && read_tree(&staging.join("sql"))? == read_tree(&out.join("sql"))?
            && std::fs::read_to_string(&lock_path).ok().as_deref() == Some(lock_text.as_str());
        if !check {
            replace_dir(&staging.join("overlay"), &out.join("overlay"))?;
            replace_dir(&staging.join("sql"), &out.join("sql"))?;
            if std::fs::read_to_string(&lock_path).ok().as_deref() != Some(lock_text.as_str()) {
                std::fs::write(&lock_path, &lock_text).map_err(|e| Error::io(&lock_path, e))?;
            }
            if let Some(n) = &numbered {
                n.record()?;
            }
        }
        Ok(Built {
            out: out.clone(),
            containers,
            sql_files: c.sql.len(),
            numbered: c.numbered.clone(),
            summary: c.summary.clone(),
            warnings: report.warnings,
            current,
            overridden: c.overridden().into_iter().cloned().collect(),
            generation: numbered.map_or(1, |n| n.generation),
        })
    })();
    let _ = std::fs::remove_dir_all(&staging);
    result
}

/// The record `id` of the file `t` under the packs: the imports', else the base's unless an
/// import deletes it.
#[must_use]
pub fn read_under(c: &Compiled, base: &DatSet, t: ModernDat, id: u32) -> Option<Vec<u8>> {
    View { base, c }.get(t, id).map(|(b, _)| b)
}

/// The record `id` of the file `t` as the compiled world reads it: the overlay's, else the
/// base's unless the overlay deletes it.
#[must_use]
pub fn read_compiled(c: &Compiled, base: &DatSet, t: ModernDat, id: u32) -> Option<Vec<u8>> {
    if let Some(s) = c.records.get(&t).and_then(|r| r.get(&id)) {
        return Some(s.bytes.to_vec());
    }
    let view = View { base, c };
    if view.deleted(t, id) {
        return None;
    }
    let f = match t {
        ModernDat::Portal | ModernDat::HighRes => base.file(base.portal_holder(DataId(id))?)?,
        other => base.file(other)?,
    };
    f.read(DataId(id)).ok()
}
