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

/// One record of the overlay: its bytes and its version.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Staged {
    pub bytes: Vec<u8>,
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
    pub tombstones: BTreeMap<ModernDat, BTreeSet<(u32, u32)>>,
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
            return Some((s.bytes.clone(), s.version));
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
                records.insert(id.raw(), Staged { bytes, version });
            }
            c.imported.insert(t, records.clone());
        }
    }
    Ok(())
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
/// - A new cell's id is `0xXXYYnnnn` in its landblock, the next after the base's cells there and
///   every cell number the lockfile holds there: a number is never handed to another cell.
///
/// # Errors
/// A span with no id left.
pub fn number(
    lock: &mut Lock,
    wanted: &BTreeSet<(Kind, String)>,
    base: &DatSet,
    staged: &Records,
    wcid_start: u32,
) -> Result<Vec<(Kind, String, u32)>, Error> {
    let mut out = Vec::new();
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
                let staged_in_span = staged
                    .values()
                    .flat_map(BTreeMap::keys)
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
                        || base
                            .file(ModernDat::Cell)
                            .is_some_and(|f| f.base_entry(DataId(first | n)).is_some())
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
            c.records
                .entry(ModernDat::Portal)
                .or_default()
                .insert(id.raw(), Staged { bytes, version });
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
        c.records
            .entry(ModernDat::Portal)
            .or_default()
            .insert(id.raw(), Staged { bytes, version });
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
            max: 31,
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
pub fn gather_vertices(packs: &[Pack], overrides: &[Override]) -> (VertexSets, Settled) {
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
                        let g = (bx * 8 + u32::from(x), by * 8 + u32::from(y));
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
        let (owner, x, y) = crate::edit::vertex_owner(gx, gy);
        let mut item = Item::new(format!("vertex {gx},{gy} {}", field_name(field))).at(
            owner,
            f32::from(x) * 24.0,
            f32::from(y) * 24.0,
        );
        item.landblocks
            .extend(copies(gx, gy).into_iter().map(|(lb, _, _)| lb));
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

/// The landblocks holding a copy of world vertex `(gx, gy)`, with its place in each: one inside a
/// landblock, two on an edge, four at a corner.
#[must_use]
pub fn copies(gx: u32, gy: u32) -> Vec<(u16, usize, usize)> {
    crate::brush::WORLD.copies(gx, gy)
}

fn set_field(lb: &mut CellLandblock, x: usize, y: usize, field: Field, value: u16) {
    let i = x * 9 + y;
    let t = &mut lb.terrain[i];
    match field {
        Field::Height => lb.height[i] = u8::try_from(value).unwrap_or(u8::MAX),
        Field::Road => *t = (*t & !0x0003) | (value & 0x3),
        Field::Terrain => *t = (*t & !0x007C) | ((value & 0x1F) << 2),
        Field::Scene => *t = (*t & !0xF800) | ((value & 0x1F) << 11),
    }
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
/// as the client and the server split the cell.
pub fn ground(lb: &CellLandblock, region: &Region, landblock: u16, x: f32, y: f32) -> f32 {
    use dereth_terrain::land::mesh::{generate_landblock, plane_set_height, Direction};
    let (bx, by) = (i32::from(landblock >> 8), i32::from(landblock & 0xFF));
    let mesh = generate_landblock(lb, region, bx, by, 1, Direction::InViewerBlock);
    let cell = |v: f32| dereth_primitives::num::to_i32((v / 24.0).floor()).clamp(0, 7);
    let (i, j) = (cell(x), cell(y));
    let (iu, ju) = (
        usize::try_from(i).unwrap_or(0),
        usize::try_from(j).unwrap_or(0),
    );
    #[allow(clippy::cast_precision_loss)]
    let (dx, dy) = (x - i as f32 * 24.0, y - j as f32 * 24.0);
    let cut = mesh.sw_to_ne_cut[iu * 8 + ju];
    let (a, b) = mesh.cell_polygons(iu, ju);
    // With the south-west to north-east cut the first triangle is the south-east half; with the
    // other cut it is the south-west half.
    let first = if cut { dx >= dy } else { dx + dy <= 24.0 };
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
            .map(|s| (s.bytes.clone(), s.version))
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
                bytes: written(id, base, cell.encode_for(base.era))?,
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
/// templates (read under the packs) with their rooms numbered: a room's number is the
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
        (
            rows.iter()
                .map(|r| crate::drops::prepare(r, &read, region, base.era))
                .collect::<Result<Vec<_>, _>>()?,
            dungeons
                .iter()
                .map(|r| crate::drops::prepare_dungeon(r, &read, base.era))
                .collect::<Result<Vec<_>, _>>()?,
        )
    };
    let wanted: BTreeSet<(Kind, String)> = prepared
        .iter()
        .flat_map(|p| p.symbols(lb))
        .chain(copied.iter().flat_map(|d| d.symbols(lb)))
        .collect();
    let mut lock = c.lock.clone();
    let numbered = number(&mut lock, &wanted, base, &c.imported, 0)?;
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
    let (vertices, settled) = gather_vertices(packs, &c.overrides);
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
        for (lb, x, y) in copies(*gx, *gy) {
            if !wanted(lb) {
                continue;
            }
            let (t, _) = match terrain.entry(lb) {
                std::collections::btree_map::Entry::Occupied(e) => e.into_mut(),
                std::collections::btree_map::Entry::Vacant(e) => e.insert(landblock_of(c, lb)?),
            };
            set_field(t, x, y, *field, *value);
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
                        bytes: written(id, base, cell.encode_for(base.era))?,
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
        c.records
            .entry(ModernDat::Cell)
            .or_default()
            .insert(lbi_id, Staged { bytes, version });
    }
    // The statics standing in indoor cells, moved and removed.
    for (lb, edits) in &blocks {
        if edits.files.iter().any(|(_, e)| e.touches_cell_statics()) {
            cell_statics(packs, *lb, edits, base, c)?;
        }
    }
    let vertex_blocks: BTreeSet<u16> = vertices
        .keys()
        .flat_map(|(gx, gy, _)| copies(*gx, *gy).into_iter().map(|(lb, _, _)| lb))
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
                bytes: written(id, base, t.encode_for(base.era))?,
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

/// A new cell's symbol: its name (in full) and its landblock.
#[must_use]
pub fn cell_symbol(name: &str, landblock: u16) -> String {
    format!("{name}@{landblock:04X}")
}

/// Leave to the base every record (of the landblocks in `only`, or every record) that came out
/// as the base holds it.
fn leave_to_base(c: &mut Compiled, base: &DatSet, only: Option<&BTreeSet<u16>>) {
    for (t, records) in &mut c.records {
        let Some(f) = base.file(*t) else { continue };
        #[allow(clippy::cast_possible_truncation)] // a cell id's top half is its landblock
        records.retain(|id, s| {
            if only.is_some_and(|o| *t != ModernDat::Cell || !o.contains(&((id >> 16) as u16))) {
                return true;
            }
            f.base_entry(DataId(*id)).is_none()
                || f.read(DataId(*id)).ok().as_ref() != Some(&s.bytes)
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
            .map(|s| (s.bytes.clone(), s.version))
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
        c.records
            .entry(ModernDat::Cell)
            .or_default()
            .insert(id, Staged { bytes, version });
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
        world.id_start(Kind::Wcid),
    )?;
    c.lock = lock;
    build_records(packs, &defs, base, &mut c)?;
    c.settled.insert(Part::Records, defs.settled.clone());
    edit_landblocks(packs, base, &mut c, None)?;
    fill_sql(packs, &defs.defined, &tokens, &mut c)?;
    // A record that came out as the base holds it is left to the base.
    leave_to_base(&mut c, base, None);
    flag_statics(&mut c, base, None)?;
    Ok(c)
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
    let imported = c
        .imported
        .get(&ModernDat::Cell)
        .cloned()
        .unwrap_or_default();
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
    // they were first given, which the editor knows them by.
    for kind in [Kind::Cell, Kind::Treasure] {
        for (symbol, id) in c.lock.of(kind) {
            if lock.get(kind, symbol).is_none() {
                lock.insert(kind, symbol, id);
            }
        }
    }
    c.numbered = number(
        &mut lock,
        &wanted,
        base,
        &c.imported,
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
        let revision = decompose::revision_over(file);
        let path = dir.join(container_name(t));
        let mut w = OverlayWriter::open_or_create(&path, file, base.base_name(t), key, 0)?;
        for (id, mask) in gone.into_iter().flatten() {
            w.tombstone(DataId(*id), *mask, revision)?;
        }
        for (id, s) in records.into_iter().flatten() {
            w.put(DataId(*id), &s.bytes, s.version, revision, 0)?;
        }
        w.add_iteration(revision, 0)?;
        w.flush(0)?;
    }
    Ok(())
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
    let c = compile(&world, &packs, &base, &lock)?;
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
        write_overlay(&c, &base, &world.key, &staging.join("overlay"))?;
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
        return Some(s.bytes.clone());
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
