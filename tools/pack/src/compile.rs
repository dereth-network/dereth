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
//!    a landblock's edge is one vertex with a copy in each neighbor: every copy is written. Two
//!    files that set the same field of one vertex differently are a conflict.
//! 5. Each landblock's statics: the base's removed and moved, the packs' appended in load order.
//!    A static placed on the ground stands on the edited terrain. A landblock that had no record
//!    of its statics gets one, and its terrain record says so.
//! 6. A record whose bytes come out as the base's is left to the base.
//!
//! **The same bytes every time:** every record goes in in id order into fresh containers, every
//! date is 0, a replaced record keeps its base version and a new one is version 3 (as a patched
//! record is marked), and every record and the overlay's iteration list carry one revision, the
//! base file's count plus one.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use dereth_assets::world::{CellLandblock, LandblockInfo, ObjectEntry};
use dereth_assets::{Decode, Region, RenderSurface, Surface, SurfaceTexture};
use dereth_dat::overlay::{container_name, OverlayDir, OverlayWriter};
use dereth_dat::{decompose, ModernDat};
use dereth_primitives::{DataId, Frame, Quat, Vec3};

use crate::dats::DatSet;
use crate::error::{Error, Loc};
use crate::lock::Lock;
use crate::source::{
    self, Height, LandblockEdits, NewRecord, Pack, PixelFormat, Place, RecordRef, Rotation, Target,
    Which,
};
use crate::sql::{self, Token};
use crate::symbol::Kind;
use crate::world::World;

/// The version a new record is stamped with, as a patched record is marked.
pub const NEW_RECORD_VERSION: u16 = 3;

/// The region record, whose height table turns a vertex's index into meters.
const REGION: DataId = DataId(0x1300_0000);

/// One record of the overlay: its bytes and its version.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Staged {
    pub bytes: Vec<u8>,
    pub version: u16,
}

/// A world compiled in memory.
#[derive(Clone, Debug, Default)]
pub struct Compiled {
    /// The records of each file, by id.
    pub records: BTreeMap<ModernDat, BTreeMap<u32, Staged>>,
    /// Each file's deletions, `(id, mask)`, from the imports.
    pub tombstones: BTreeMap<ModernDat, BTreeSet<(u32, u32)>>,
    /// The SQL files, by their path under the output's `sql` folder, with the ids filled in.
    pub sql: BTreeMap<PathBuf, String>,
    /// The lockfile, with every new symbol numbered.
    pub lock: Lock,
    /// The symbols numbered in this build, in the order they were numbered.
    pub numbered: Vec<(Kind, String, u32)>,
    /// One line for each landblock edited.
    pub summary: Vec<String>,
}

impl Compiled {
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

/// The world as compiled so far: the overlay's records and deletions, then the base.
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

    /// The record `id` of the file `t` as the world reads it now, with its version.
    fn get(&self, t: ModernDat, id: u32) -> Option<(Vec<u8>, u16)> {
        if let Some(s) = self.c.records.get(&t).and_then(|r| r.get(&id)) {
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
/// - An instance's id is `0x7XXYYnnn` in its landblock, from `nnn = 0xFFF` down, that the lockfile
///   does not hold.
///
/// # Errors
/// A span with no id left.
pub fn number(
    lock: &mut Lock,
    wanted: &BTreeSet<(Kind, String)>,
    base: &DatSet,
    staged: &Compiled,
    wcid_start: u32,
) -> Result<Vec<(Kind, String, u32)>, Error> {
    let mut out = Vec::new();
    for (kind, symbol) in wanted {
        if lock.get(*kind, symbol).is_some() {
            continue;
        }
        let id = match kind {
            Kind::RenderSurface | Kind::SurfaceTexture | Kind::Surface => {
                let (lo, hi) = kind.span().unwrap_or((0, 0));
                let staged_in_span = staged
                    .records
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
        };
        lock.insert(*kind, symbol, id);
        out.push((*kind, symbol.clone(), id));
    }
    Ok(out)
}

/// Every record a pack defines, by kind and symbol, with where.
type Defined = BTreeMap<(Kind, String), Loc>;

fn defined_records(packs: &[Pack]) -> Result<(Defined, BTreeMap<u32, Loc>), Error> {
    let mut defined: Defined = BTreeMap::new();
    let mut replaced: BTreeMap<u32, Loc> = BTreeMap::new();
    for p in packs {
        for r in &p.records {
            match r.item.target() {
                Target::New(sym) => {
                    if let Some(first) = defined.insert((r.item.kind(), sym.clone()), r.at.clone())
                    {
                        return Err(Error::Conflict {
                            first,
                            second: r.at.clone(),
                            what: format!("{} {sym} is defined twice", r.item.kind()),
                        });
                    }
                }
                Target::Replace(id) => {
                    if let Some(first) = replaced.insert(id.raw(), r.at.clone()) {
                        return Err(Error::Conflict {
                            first,
                            second: r.at.clone(),
                            what: format!("{:#010X} is replaced twice", id.raw()),
                        });
                    }
                }
            }
        }
    }
    Ok((defined, replaced))
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
    defined: &Defined,
    view_base: &DatSet,
    c: &mut Compiled,
) -> Result<(), Error> {
    for p in packs {
        for r in &p.records {
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
            let bytes = match &r.item {
                NewRecord::RenderSurface {
                    picture,
                    format,
                    category,
                    ..
                } => {
                    let (width, height, rgba) = read_picture(at, picture)?;
                    let px = pixels(*format, &rgba);
                    RenderSurface {
                        id,
                        data_category: *category,
                        width,
                        height,
                        format: format.id(),
                        image_size: u32::try_from(px.len()).unwrap_or(u32::MAX),
                        data_offset: 0x18,
                        default_palette_id: None,
                    }
                    .encode(&px)
                }
                NewRecord::SurfaceTexture {
                    levels, category, ..
                } => SurfaceTexture {
                    id,
                    data_category: *category,
                    // The constant every image texture carries.
                    unknown_byte: 2,
                    source_levels: levels
                        .iter()
                        .map(|l| resolve(&c.lock, defined, at, Kind::RenderSurface, l))
                        .collect::<Result<_, _>>()?,
                }
                .encode(),
                NewRecord::Surface {
                    surface_type,
                    texture,
                    palette,
                    color,
                    translucency,
                    luminosity,
                    diffuse,
                    ..
                } => Surface {
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
                .encode(),
            };
            let view = View { base: view_base, c };
            let version = match r.item.target() {
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
            };
            c.records
                .entry(ModernDat::Portal)
                .or_default()
                .insert(id.raw(), Staged { bytes, version });
        }
    }
    Ok(())
}

/// One field a vertex edit sets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Field {
    Height,
    Terrain,
    Scene,
    Road,
}

/// Every vertex edit of every pack, by world vertex and field: the value and the file that set it.
type VertexSets = BTreeMap<(u32, u32, Field), (u16, Loc)>;

fn gather_vertices(packs: &[Pack]) -> Result<VertexSets, Error> {
    let mut sets: VertexSets = BTreeMap::new();
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
                            match sets.get(&(g.0, g.1, field)) {
                                // Another file set it otherwise: a conflict. The same file may
                                // set it again; its later edit wins.
                                Some((old, first)) if *old != value && first.file != v.at.file => {
                                    return Err(Error::Conflict {
                                        first: first.clone(),
                                        second: v.at.clone(),
                                        what: format!(
                                            "the {field:?} of the vertex at world ({}, {}) is set to {value}",
                                            g.0, g.1
                                        )
                                        .to_lowercase(),
                                    });
                                }
                                _ => {
                                    sets.insert((g.0, g.1, field), (value, v.at.clone()));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(sets)
}

/// The landblocks holding a copy of world vertex `(gx, gy)`, with its place in each: one inside a
/// landblock, two on an edge, four at a corner.
fn copies(gx: u32, gy: u32) -> Vec<(u16, usize, usize)> {
    let xs: Vec<u32> = if gx.is_multiple_of(8) && gx > 0 {
        vec![gx / 8 - 1, gx / 8]
    } else {
        vec![gx / 8]
    };
    let ys: Vec<u32> = if gy.is_multiple_of(8) && gy > 0 {
        vec![gy / 8 - 1, gy / 8]
    } else {
        vec![gy / 8]
    };
    let mut out = Vec::new();
    for bx in &xs {
        for by in &ys {
            if *bx > 0xFE || *by > 0xFE {
                continue;
            }
            let lb = u16::try_from((bx << 8) | by).unwrap_or(u16::MAX);
            out.push((lb, (gx - bx * 8) as usize, (gy - by * 8) as usize));
        }
    }
    out
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

fn quat_of(r: Rotation) -> Quat {
    match r {
        Rotation::Heading(h) => heading_quat(h),
        Rotation::Quat([w, x, y, z]) => Quat::new(w, x, y, z),
    }
}

/// The ground's height at landblock-local `(x, y)`: the plane of the terrain triangle under it,
/// as the client and the server split the cell.
fn ground(lb: &CellLandblock, region: &Region, landblock: u16, x: f32, y: f32) -> f32 {
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

/// One landblock's static edits from every pack, in load order.
struct BlockEdits<'a> {
    files: Vec<(&'a Pack, &'a LandblockEdits)>,
}

#[allow(clippy::too_many_lines)]
fn edit_landblocks(packs: &[Pack], base: &DatSet, c: &mut Compiled) -> Result<(), Error> {
    let vertices = gather_vertices(packs)?;
    let mut blocks: BTreeMap<u16, BlockEdits<'_>> = BTreeMap::new();
    for p in packs {
        for e in &p.edits {
            blocks
                .entry(e.landblock)
                .or_insert_with(|| BlockEdits { files: Vec::new() })
                .files
                .push((p, e));
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
        let decoded = CellLandblock::decode_payload(DataId(id), &bytes)
            .map_err(|e| Error::Dat(format!("landblock {lb:04X}: {e}")))?;
        Ok((decoded, version))
    };
    for ((gx, gy, field), (value, _)) in &vertices {
        for (lb, x, y) in copies(*gx, *gy) {
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
            .and_then(|(b, _)| Region::decode_payload(REGION, &b).ok())
    };
    for (lb, edits) in &blocks {
        let has_statics = edits
            .files
            .iter()
            .any(|(_, e)| !e.statics.is_empty() || !e.moves.is_empty() || !e.removes.is_empty());
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
                LandblockInfo::decode_payload(DataId(lbi_id), &bytes)
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
        // Base statics: removals and moves by index, checked against what the base holds.
        let mut removed: BTreeMap<usize, Loc> = BTreeMap::new();
        let mut moved: BTreeMap<usize, (Loc, Frame)> = BTreeMap::new();
        let mut added: Vec<(Option<String>, Loc, ObjectEntry)> = Vec::new();
        for (_, e) in &edits.files {
            for r in &e.removes {
                let Which::Index(i) = r.item.which else {
                    continue;
                };
                let Some(o) = info.objects.get(i) else {
                    return Err(Error::source(
                        &r.at,
                        format!("landblock {lb:04X} has {base_len} statics; there is no {i}"),
                    ));
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
                removed.entry(i).or_insert_with(|| r.at.clone());
            }
            for s in &e.statics {
                let frame = frame_at(s.item.at, quat_of(s.item.rotation), &ground_z);
                if let Some(name) = &s.item.name {
                    if let Some((_, first, _)) =
                        added.iter().find(|(n, _, _)| n.as_ref() == Some(name))
                    {
                        return Err(Error::Conflict {
                            first: first.clone(),
                            second: s.at.clone(),
                            what: format!("the static {name} is placed twice"),
                        });
                    }
                }
                added.push((
                    s.item.name.clone(),
                    s.at.clone(),
                    ObjectEntry {
                        id: s.item.model,
                        frame,
                    },
                ));
            }
        }
        for (_, e) in &edits.files {
            for m in &e.moves {
                match &m.item.which {
                    Which::Index(i) => {
                        let Some(o) = info.objects.get(*i) else {
                            return Err(Error::source(
                                &m.at,
                                format!(
                                    "landblock {lb:04X} has {base_len} statics; there is no {i}"
                                ),
                            ));
                        };
                        if removed.contains_key(i) {
                            return Err(Error::source(&m.at, format!("static {i} is removed")));
                        }
                        let place = m.item.at.unwrap_or(Place {
                            x: o.frame.origin.x,
                            y: o.frame.origin.y,
                            z: Height::At(o.frame.origin.z),
                        });
                        let rot = m.item.rotation.map_or(o.frame.rotation, quat_of);
                        let frame = frame_at(place, rot, &ground_z);
                        if let Some((first, f)) = moved.get(i) {
                            if *f != frame {
                                return Err(Error::Conflict {
                                    first: first.clone(),
                                    second: m.at.clone(),
                                    what: format!("static {i} of landblock {lb:04X} is moved"),
                                });
                            }
                        }
                        moved.insert(*i, (m.at.clone(), frame));
                    }
                    Which::Symbol(sym) => {
                        let Some((_, _, o)) =
                            added.iter_mut().find(|(n, _, _)| n.as_ref() == Some(sym))
                        else {
                            return Err(Error::source(
                                &m.at,
                                format!("no pack places a static {sym} in landblock {lb:04X}"),
                            ));
                        };
                        let place = m.item.at.unwrap_or(Place {
                            x: o.frame.origin.x,
                            y: o.frame.origin.y,
                            z: Height::At(o.frame.origin.z),
                        });
                        let rot = m.item.rotation.map_or(o.frame.rotation, quat_of);
                        o.frame = frame_at(place, rot, &ground_z);
                    }
                }
            }
            for r in &e.removes {
                if let Which::Symbol(sym) = &r.item.which {
                    let before = added.len();
                    added.retain(|(n, _, _)| n.as_ref() != Some(sym));
                    if added.len() == before {
                        return Err(Error::source(
                            &r.at,
                            format!("no pack places a static {sym} in landblock {lb:04X}"),
                        ));
                    }
                }
            }
        }
        let mut objects = Vec::with_capacity(base_len + added.len());
        for (i, o) in info.objects.iter().enumerate() {
            if removed.contains_key(&i) {
                continue;
            }
            let mut o = *o;
            if let Some((_, f)) = moved.get(&i) {
                o.frame = *f;
            }
            objects.push(o);
        }
        objects.extend(added.iter().map(|(_, _, o)| *o));
        let (n_removed, n_moved, n_added) = (removed.len(), moved.len(), added.len());
        info.objects = objects;
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
        let bytes = info.encode();
        c.records
            .entry(ModernDat::Cell)
            .or_default()
            .insert(lbi_id, Staged { bytes, version });
    }
    let vertex_blocks: BTreeSet<u16> = vertices
        .keys()
        .flat_map(|(gx, gy, _)| copies(*gx, *gy).into_iter().map(|(lb, _, _)| lb))
        .collect();
    for lb in &vertex_blocks {
        c.summary.push(format!("{lb:04X}: terrain edited"));
    }
    for (lb, (t, version)) in terrain {
        let id = (u32::from(lb) << 16) | 0xFFFF;
        c.records.entry(ModernDat::Cell).or_default().insert(
            id,
            Staged {
                bytes: t.encode(),
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
    for (pi, p) in packs.iter().enumerate() {
        for (fi, f) in p.sql.iter().enumerate() {
            let toks = &tokens[&(pi, fi)];
            for t in toks {
                if t.kind.is_record() && !defined.contains_key(&(t.kind, t.symbol.clone())) {
                    return Err(Error::source(
                        &t.at,
                        format!("no pack defines {} {}", t.kind, t.symbol),
                    ));
                }
            }
            let lock = &c.lock;
            let text = sql::fill(&f.text, toks, |t| lock.get(t.kind, &t.symbol).unwrap_or(0));
            c.sql.insert(Path::new(&p.name).join(&f.rel), text);
        }
    }
    Ok(())
}

/// Compile `packs` (the world's, in load order) over `base`, keeping the ids `lock` holds.
///
/// # Errors
/// A source that does not hold together (an unknown symbol, a conflict, a static that is not
/// there), and the data files' errors.
pub fn compile(
    world: &World,
    packs: &[Pack],
    base: &DatSet,
    lock: &Lock,
) -> Result<Compiled, Error> {
    let mut c = Compiled {
        lock: lock.clone(),
        ..Compiled::default()
    };
    read_imports(world, base, &mut c)?;
    let (defined, _) = defined_records(packs)?;
    // Every symbol the SQL names, read before anything is numbered.
    let mut tokens: BTreeMap<(usize, usize), Vec<Token>> = BTreeMap::new();
    let mut wanted: BTreeSet<(Kind, String)> = defined.keys().cloned().collect();
    for (pi, p) in packs.iter().enumerate() {
        for (fi, f) in p.sql.iter().enumerate() {
            let toks = sql::tokens(&f.path, &p.name, &f.text)?;
            for t in &toks {
                if !t.kind.is_record() {
                    wanted.insert((t.kind, t.symbol.clone()));
                }
            }
            tokens.insert((pi, fi), toks);
        }
    }
    let mut lock = c.lock.clone();
    c.numbered = number(&mut lock, &wanted, base, &c, world.wcid_start)?;
    c.lock = lock;
    build_records(packs, &defined, base, &mut c)?;
    edit_landblocks(packs, base, &mut c)?;
    fill_sql(packs, &defined, &tokens, &mut c)?;
    // A record that came out as the base holds it is left to the base.
    for (t, records) in &mut c.records {
        let Some(f) = base.file(*t) else { continue };
        records.retain(|id, s| {
            f.base_entry(DataId(*id)).is_none()
                || f.read(DataId(*id)).ok().as_ref() != Some(&s.bytes)
        });
    }
    c.records.retain(|_, r| !r.is_empty());
    Ok(c)
}

/// Write `c` as an overlay folder at `dir` (made fresh) over `base`, for the world `key`.
///
/// # Errors
/// The writer's errors.
pub fn write_overlay(c: &Compiled, base: &DatSet, key: &str, dir: &Path) -> Result<(), Error> {
    if dir.exists() {
        std::fs::remove_dir_all(dir).map_err(|e| Error::io(dir, e))?;
    }
    std::fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
    for t in ModernDat::ALL {
        let records = c.records.get(&t);
        let gone = c.tombstones.get(&t);
        if records.is_none_or(BTreeMap::is_empty) && gone.is_none_or(BTreeSet::is_empty) {
            continue;
        }
        let Some(file) = base.file(t) else {
            return Err(Error::Dat(format!(
                "the overlay changes {}, which the files do not have",
                t.file_name()
            )));
        };
        let revision = decompose::revision_over(file);
        let path = dir.join(container_name(t));
        let mut w = OverlayWriter::open_or_create(&path, file, t.file_name(), key, 0)?;
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
    /// Whether the build matched what was already there (with `check`, nothing is written).
    pub current: bool,
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
/// As [`compile`]; records that fail the checks ([`Error::Invalid`]), and the writer's errors.
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
    let base = DatSet::open(dats)?;
    let lock_path = world.lock_path();
    let lock = Lock::read(&lock_path)?;
    let c = compile(&world, &packs, &base, &lock)?;
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
            crate::validate::validate(&layered, &crate::validate::Scope::of(&c))
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
            current,
        })
    })();
    let _ = std::fs::remove_dir_all(&staging);
    result
}
