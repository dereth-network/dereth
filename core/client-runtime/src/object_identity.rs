//! Which records two eras' portal files share by id are the **same object** in both — repainted,
//! re-encoded or remodelled — and which are an unrelated record that reuses the id.
//!
//! Drawing the world's objects with another era's look reads each object's records from the other
//! era's files, and that is only right where the other era's record of the id is the same thing.
//! The answer is worked out once per pair of files, from the files alone and never from pictures:
//!
//! - **Graphics objects** are the same when their geometry is, when a setup or clothing table of
//!   the same id names them in both eras, or when their bounding boxes lie within a quarter of
//!   their size of each other (a remodel in place). Further apart but still within their own size
//!   of each other they are a **remodel** of the same object; anything else is another object.
//! - **Setups** are the same when their part lists are, or share a part. A setup whose parts are
//!   all new is a **remodel** when its resting pose fills a box within its own size of the other
//!   era's; anything else is another object. A server object built on a remodelled setup keeps the
//!   world's part list, so it can only draw the look's parts where each sits where the world's
//!   part of that index rests ([`remodel`](crate::object_identity::ObjectIdentity::remodel)).
//! - **Pictures and palettes** keep their ids: across the Throne of Destiny change every shared
//!   picture id is the same picture, re-encoded and often re-indexed into the larger palette, and
//!   every shared palette id the same colours redone with it, even where the two eras use them in
//!   different places. Where they are used is recorded for the census and does not decide.
//! - **The bare body** is translated through each era's own character-creation tables: the
//!   model a body part wears with nothing on it is not the same id in both eras (the human arm
//!   the later files leave bare is the older files' armoured arm, the older bare arm another
//!   model), so a part wearing the world's bare model draws the look's bare model, with the
//!   look's own bare pictures ([`bare_part`](crate::object_identity::ObjectIdentity::bare_part)).
//! - **Interior cells** are compared room by room when both eras' cell files are at hand
//!   ([`build_rooms`](crate::object_identity::ObjectIdentity::build_rooms)): a cell draws from
//!   the other era's record of it where that record is the same room in the same place, with
//!   the other era's own surfaces.
//! - **Hair** is translated through the same tables, style by style: the head a body wears for a
//!   hair style of the world's table draws the look's head for the style of the same heritage, sex
//!   and place in the look's list, where the look has one and it is bald or not alike
//!   ([`hair`](crate::object_identity::ObjectIdentity::hair)). A style the look's list lacks keeps
//!   the world's head.
//! - **Palettes the look lacks** are translated through the palette sets both eras pick them from:
//!   a skin, hair or eye colour through the character-creation tables, anything else through a
//!   palette set both files hold, each at the same share of the way along its set
//!   ([`palette`](crate::object_identity::ObjectIdentity::palette)).
//! - **Surfaces** are not answered at all: the later files renumbered them, and of the surface
//!   ids the February 2005 and end-of-retail portals share, two are named by a graphics object of
//!   the same id in both. A surface is only ever read in the era of the graphics object that
//!   names it, and so is everything below it.
//!
//! [`load_or_build`](crate::object_identity::ObjectIdentity::load_or_build) keeps the answer in
//! a per-user cache file keyed by both files' hashes, so a pair of files is only worked out once.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use dereth_assets::geometry::Environment;
use dereth_assets::tables::ObjDesc;
use dereth_assets::world::EnvCell;
use dereth_assets::{
    CharGen, ClothingTable, Decode, GfxObj, PaletteSet, RenderSurface, Setup, Surface,
    SurfaceTexture,
};
use dereth_dat::{divine_type_in, ContainerEra, DatFile, DbType};
use dereth_primitives::frame::{localtoglobal, V3 as _};
use dereth_primitives::{DataId, Frame, Vec3};

/// How an id both files hold was judged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Verdict {
    /// The same geometry (graphics objects) or the same part list (setups).
    Identical,
    /// Named by the same setup or clothing table in both eras (graphics objects), or sharing a
    /// part (setups).
    SharedReferrer,
    /// A bounding box within a quarter of its size of the other's (graphics objects).
    CloseShape,
    /// Used in the same place in both eras: the same surface slot of the same graphics object,
    /// the same clothing or character-creation entry, the same palette set position.
    SharedUse,
    /// A picture or palette used in both eras, never in the same place. Still the same record.
    UsedElsewhere,
    /// A picture or palette used in one era or neither. Still the same record.
    NoEvidence,
    /// A graphics object of another shape no referrer names alike whose box is still within its
    /// own size of the other's, or a setup sharing no part whose resting pose fills such a box:
    /// the same object, remodelled.
    Remodel,
    /// A different shape with no referrer in common (graphics objects), or no part in common
    /// (setups): another record reusing the id.
    Unrelated,
}

impl Verdict {
    /// Whether the other era's record may stand in for the world's.
    #[must_use]
    pub fn is_same(self) -> bool {
        !matches!(self, Self::Unrelated)
    }
}

/// The verdict for every id both files hold, by record type, as [`ObjectIdentity::build`] worked it
/// out. Kept by the measuring tools; the client keeps only the [`ObjectIdentity`].
#[derive(Debug, Clone, Default)]
pub struct IdentityCensus {
    /// `(record type, id) -> verdict`, for graphics objects, setups, textures and palettes.
    pub verdicts: BTreeMap<(DbType, u32), Verdict>,
    /// Graphics objects both hold with every vertex in the same place.
    pub geometry_same: BTreeSet<u32>,
    /// Surface ids both files hold, and how many of them a graphics object of the same id names
    /// in both.
    pub surfaces_shared: usize,
    pub surfaces_named_alike: usize,
    /// Body parts whose bare model the two eras' character tables translate.
    pub bare_parts: usize,
    /// Remodelled setups a server object of the world's can draw with the look's parts.
    pub remodels_drawable: usize,
    /// Hair styles the two eras' character tables translate.
    pub hair_styles: usize,
    /// Palettes the look lacks that a palette set translates.
    pub palettes: usize,
}

/// The ids whose other-era record is the same object, for one pair of portal files.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ObjectIdentity {
    same: HashSet<u32>,
    geometry_same: HashSet<u32>,
    bare: HashMap<(u32, u32), BarePart>,
    /// The world's interior cells the other era's cell file holds as the same room
    /// ([`Self::build_rooms`]); empty when the cell files were not compared.
    rooms: HashSet<u32>,
    /// Remodelled setup -> the look's part list.
    remodels: HashMap<u32, Vec<DataId>>,
    /// `(part, the world's head)` -> each hair style of the world's that wears it, as the look
    /// draws it.
    hair: HashMap<(u32, u32), Vec<BarePart>>,
    /// A palette the look lacks -> the look's.
    palettes: HashMap<u32, u32>,
}

/// What a body part with nothing on it draws in the look's era ([`ObjectIdentity::bare_part`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BarePart {
    /// The look's bare model of the part.
    pub model: DataId,
    /// The look's bare texture changes on it, `(old picture, new picture)`.
    pub look_maps: Vec<(DataId, DataId)>,
    /// The world's bare texture changes on it: a part carrying other changes is dressed.
    pub world_maps: Vec<(DataId, DataId)>,
}

/// One body of a character-creation table: each part's bare model and bare texture changes.
type Body = Vec<(DataId, Vec<(DataId, DataId)>)>;

/// Every body of a file's character-creation table, by heritage and sex.
fn bodies(f: &DatFile) -> BTreeMap<(u32, u32), Body> {
    let mut out = BTreeMap::new();
    let Some(cg) = get::<CharGen>(f, 0x0E00_0002) else {
        return out;
    };
    for (h, hg) in &cg.heritage_groups {
        for (x, sex) in &hg.sexes {
            let Some(setup) = get::<Setup>(f, sex.setup.0) else {
                continue;
            };
            let mut body: Body = setup.parts.iter().map(|p| (*p, Vec::new())).collect();
            for (part, g) in &sex.base_objdesc.anim_part_changes {
                if let Some(b) = body.get_mut(usize::from(*part)) {
                    b.0 = *g;
                }
            }
            for (part, old, new) in &sex.base_objdesc.texture_changes {
                if let Some(b) = body.get_mut(usize::from(*part)) {
                    b.1.push((*old, *new));
                }
            }
            out.insert((*h, *x), body);
        }
    }
    out
}

/// Every body of both eras' character-creation tables, part by part: `(part, world's bare model)
/// -> the look's`. A pair two bodies would translate differently is left out, and so is a part
/// both eras leave as the same model with the same pictures.
fn bare_parts(world: &DatFile, other: &DatFile) -> HashMap<(u32, u32), BarePart> {
    let (a, b) = (bodies(world), bodies(other));
    let mut out: HashMap<(u32, u32), BarePart> = HashMap::new();
    let mut clash: HashSet<(u32, u32)> = HashSet::new();
    for (who, wb) in &a {
        let Some(lb) = b.get(who) else {
            continue;
        };
        for (i, ((wm, wmaps), (lm, lmaps))) in wb.iter().zip(lb).enumerate() {
            // LINT-OK: a body part index; bodies have a few dozen parts.
            #[allow(clippy::cast_possible_truncation)]
            let key = (i as u32, wm.0);
            let v = BarePart {
                model: *lm,
                look_maps: lmaps.clone(),
                world_maps: wmaps.clone(),
            };
            match out.get(&key) {
                Some(have) if *have != v => {
                    clash.insert(key);
                }
                Some(_) => {}
                None => {
                    out.insert(key, v);
                }
            }
        }
    }
    for k in clash {
        out.remove(&k);
    }
    out.retain(|(_, wm), v| v.model.0 != *wm || v.look_maps != v.world_maps);
    out
}

/// How the world's interior cells compare with the other era's cell file, room by room
/// ([`ObjectIdentity::build_rooms`]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RoomCensus {
    /// The world's interior cells.
    pub cells: usize,
    /// The other era's cell file has no cell of that id.
    pub absent: usize,
    /// A record of either era will not decode, or names an environment its portal lacks.
    pub undecodable: usize,
    /// Another environment, cell structure, frame or portal list: another room, or the same room
    /// moved or rejoined.
    pub other_place: usize,
    /// The same environment and place, but the environment's room has vertices elsewhere.
    pub other_shape: usize,
    /// The same room, naming a surface the other era's portal does not hold.
    pub surfaces_missing: usize,
    /// The same room in the same place: drawn from the other era.
    pub same: usize,
    /// Of those, rooms the other era furnished with other objects or the same objects elsewhere.
    /// The furniture drawn is the world's either way (it is what collides), each piece by the
    /// objects' verdicts.
    pub other_furniture: usize,
}

/// The vertex positions of one room of an environment, `None` when it is not there.
fn room_vertices(
    f: &DatFile,
    memo: &mut HashMap<u32, Option<Environment>>,
    env: DataId,
    room: u16,
) -> Option<Vec<[f32; 3]>> {
    let e = memo
        .entry(env.0)
        .or_insert_with(|| get::<Environment>(f, env.0))
        .as_ref()?;
    let s = e.cells.get(usize::from(room))?;
    Some(
        s.vertex_array
            .vertices
            .iter()
            .map(|v| [v.position.x, v.position.y, v.position.z])
            .collect(),
    )
}

/// The world's interior cells whose other-era record is the same room in the same place, and
/// the census of every cell.
fn rooms(
    world_cell: &DatFile,
    other_cell: &DatFile,
    world_portal: &DatFile,
    other_portal: &DatFile,
) -> (HashSet<u32>, RoomCensus) {
    let mut out = HashSet::new();
    let mut c = RoomCensus::default();
    let (mut wenv, mut oenv) = (HashMap::new(), HashMap::new());
    // Environment rooms already compared: (environment, room) -> every vertex in the same place.
    let mut shapes: HashMap<(u32, u16), bool> = HashMap::new();
    let mut held: HashMap<u32, bool> = HashMap::new();
    for id in world_cell.iter_ids() {
        let low = id.0 & 0xFFFF;
        if !(0x0100..0xFFFE).contains(&low) {
            continue;
        }
        c.cells += 1;
        if !other_cell.contains(id) {
            c.absent += 1;
            continue;
        }
        let (Some(a), Some(b)) = (
            get::<EnvCell>(world_cell, id.0),
            get::<EnvCell>(other_cell, id.0),
        ) else {
            c.undecodable += 1;
            continue;
        };
        if a.environment != b.environment
            || a.cell_struct != b.cell_struct
            || a.frame != b.frame
            || a.portals != b.portals
        {
            c.other_place += 1;
            continue;
        }
        let key = (a.environment.0, a.cell_struct);
        let shape = if let Some(s) = shapes.get(&key) {
            Some(*s)
        } else {
            match (
                room_vertices(world_portal, &mut wenv, a.environment, a.cell_struct),
                room_vertices(other_portal, &mut oenv, b.environment, b.cell_struct),
            ) {
                (Some(p), Some(q)) => {
                    let s = p == q;
                    shapes.insert(key, s);
                    Some(s)
                }
                _ => None,
            }
        };
        match shape {
            None => {
                c.undecodable += 1;
                continue;
            }
            Some(false) => {
                c.other_shape += 1;
                continue;
            }
            Some(true) => {}
        }
        let all_held = b
            .surfaces
            .iter()
            .all(|s| *held.entry(s.0).or_insert_with(|| other_portal.contains(*s)));
        if !all_held {
            c.surfaces_missing += 1;
            continue;
        }
        c.same += 1;
        if a.static_objects != b.static_objects {
            c.other_furniture += 1;
        }
        out.insert(id.0);
    }
    (out, c)
}

/// Picture changes, `(old, new)`.
type Changes = Vec<(DataId, DataId)>;

/// Every hair style of both eras' character-creation tables, style by style: `(part, the world's
/// head) -> the style as the look draws it`, where both lists hold a style at that place, each
/// changes one part, and both are bald or neither. `world_maps` are the world style's own picture
/// changes, which tell two styles wearing one head apart; a style two bodies translate
/// differently is left out.
fn hair_styles(world: &DatFile, other: &DatFile) -> HashMap<(u32, u32), Vec<BarePart>> {
    let (Some(a), Some(b)) = (
        get::<CharGen>(world, 0x0E00_0002),
        get::<CharGen>(other, 0x0E00_0002),
    ) else {
        return HashMap::new();
    };
    let maps = |d: &ObjDesc| -> Vec<(DataId, DataId)> {
        d.texture_changes.iter().map(|(_, o, n)| (*o, *n)).collect()
    };
    let mut out: HashMap<(u32, u32), Vec<BarePart>> = HashMap::new();
    let mut clash: HashSet<(u32, u32, Changes)> = HashSet::new();
    for (h, hg) in &a.heritage_groups {
        let Some(og) = b.heritage_groups.get(h) else {
            continue;
        };
        for (x, sex) in &hg.sexes {
            let Some(os) = og.sexes.get(x) else {
                continue;
            };
            for (ws, ls) in sex.hair_styles.iter().zip(&os.hair_styles) {
                let ([(wp, wm)], [(lp, lm)]) = (
                    ws.objdesc.anim_part_changes.as_slice(),
                    ls.objdesc.anim_part_changes.as_slice(),
                ) else {
                    continue;
                };
                if wp != lp || ws.bald != ls.bald {
                    continue;
                }
                let v = BarePart {
                    model: *lm,
                    look_maps: maps(&ls.objdesc),
                    world_maps: maps(&ws.objdesc),
                };
                let key = (u32::from(*wp), wm.0);
                let list = out.entry(key).or_default();
                match list.iter().find(|have| have.world_maps == v.world_maps) {
                    Some(have) if *have != v => {
                        clash.insert((key.0, key.1, v.world_maps.clone()));
                    }
                    Some(_) => {}
                    None => list.push(v),
                }
            }
        }
    }
    for (p, m, gone) in clash {
        if let Some(list) = out.get_mut(&(p, m)) {
            list.retain(|v| v.world_maps != gone);
        }
    }
    out.retain(|(_, wm), v| {
        v.retain(|s| s.model.0 != *wm || s.look_maps != s.world_maps);
        !v.is_empty()
    });
    out
}

/// The palettes of the world's the look lacks, translated: each palette set the world picks one
/// from (a skin or hair colour of a body of the character-creation tables, or any palette set
/// both files hold) gives the look's palette at the same share of the way along the look's set;
/// an eye colour, a palette itself, gives the look's eye colour of the same place. The first
/// translation found stands (heritages in order, then the shared sets by id).
fn palette_translations(world: &DatFile, other: &DatFile) -> HashMap<u32, u32> {
    let held = |id: u32| other.contains(DataId(id));
    let mut pairs: Vec<(u32, u32)> = Vec::new();
    let mut out: HashMap<u32, u32> = HashMap::new();
    if let (Some(a), Some(b)) = (
        get::<CharGen>(world, 0x0E00_0002),
        get::<CharGen>(other, 0x0E00_0002),
    ) {
        for (h, hg) in &a.heritage_groups {
            let Some(og) = b.heritage_groups.get(h) else {
                continue;
            };
            for (x, sex) in &hg.sexes {
                let Some(os) = og.sexes.get(x) else {
                    continue;
                };
                pairs.push((sex.skin_palset.0, os.skin_palset.0));
                pairs.extend(
                    sex.hair_colors
                        .iter()
                        .copied()
                        .zip(os.hair_colors.iter().copied()),
                );
                for (w, l) in sex.eye_colors.iter().zip(&os.eye_colors) {
                    if !held(*w) && held(*l) {
                        out.entry(*w).or_insert(*l);
                    }
                }
            }
        }
    }
    let theirs: HashSet<u32> = ids(other, DbType::PalSet).into_iter().collect();
    pairs.extend(
        ids(world, DbType::PalSet)
            .into_iter()
            .filter(|s| theirs.contains(s))
            .map(|s| (s, s)),
    );
    for (ws, ls) in pairs {
        let (Some(w), Some(l)) = (get::<PaletteSet>(world, ws), get::<PaletteSet>(other, ls))
        else {
            continue;
        };
        let (n, m) = (w.palette_ids.len(), l.palette_ids.len());
        if n == 0 || m == 0 {
            continue;
        }
        for (j, p) in w.palette_ids.iter().enumerate() {
            if held(p.0) {
                continue;
            }
            // The same share of the way along, rounded: the first to the first, the last to the
            // last.
            let k = if n == 1 {
                0
            } else {
                (j * (m - 1) + (n - 1) / 2) / (n - 1)
            };
            let q = l.palette_ids[k].0;
            if held(q) {
                out.entry(p.0).or_insert(q);
            }
        }
    }
    out
}

/// The frames a setup's parts rest at: the resting placement, else the default one, else the
/// identity.
fn rest_frames(s: &Setup) -> Vec<Frame> {
    let f = s
        .placement_frames
        .get(&0x65)
        .or_else(|| s.placement_frames.get(&0))
        .map(|p| p.frames.as_slice())
        .unwrap_or_default();
    (0..s.parts.len())
        .map(|i| f.get(i).copied().unwrap_or_default())
        .collect()
}

/// The box a setup's parts fill at rest.
fn setup_box(f: &DatFile, s: &Setup) -> Option<([f32; 3], [f32; 3])> {
    let mut acc: Option<([f32; 3], [f32; 3])> = None;
    for (p, at) in s.parts.iter().zip(rest_frames(s)) {
        let Some((lo, hi)) = get::<GfxObj>(f, p.0).as_ref().and_then(bounds) else {
            continue;
        };
        for c in 0..8 {
            let v = Vec3::new(
                if c & 1 == 0 { lo[0] } else { hi[0] },
                if c & 2 == 0 { lo[1] } else { hi[1] },
                if c & 4 == 0 { lo[2] } else { hi[2] },
            );
            let w = localtoglobal(&at, v);
            let w = [w.x, w.y, w.z];
            let b = acc.get_or_insert((w, w));
            for (k, x) in w.iter().enumerate() {
                b.0[k] = b.0[k].min(*x);
                b.1[k] = b.1[k].max(*x);
            }
        }
    }
    acc
}

/// How far a part of the look's remodel may rest from where the world's part of that index rests
/// and still be posed by the world's motion: 10 cm, and turned by at most 10 degrees (the two
/// rotations' dot product at least the cosine of half that).
const REMODEL_REST: (f32, f32) = (0.10, 0.996_194_7);

/// The look's part list for a remodelled setup a server object of the world's can draw with it:
/// the look's setup has no more parts than the world's, and each rests where the world's part of
/// the same index rests (the world's motion places parts by index). `None` otherwise.
fn remodel_parts(world: &Setup, look: &Setup) -> Option<Vec<DataId>> {
    if look.parts.is_empty() || look.parts.len() > world.parts.len() {
        return None;
    }
    for (p, q) in rest_frames(world).iter().zip(&rest_frames(look)) {
        let d = p.origin.sub(q.origin).magnitude();
        let (r, s) = (&p.rotation, &q.rotation);
        let dot = (r.w * s.w + r.x * s.x + r.y * s.y + r.z * s.z).abs();
        if d > REMODEL_REST.0 || dot < REMODEL_REST.1 {
            return None;
        }
    }
    Some(look.parts.clone())
}

/// How far apart two graphics objects' bounding boxes may lie, as a share of their size, and
/// still be the same object remodelled in place.
pub const CLOSE_SHAPE: f32 = 0.25;

/// How far apart a remodel's two boxes may lie, as a share of their size: one size.
pub const REMODEL_SHAPE: f32 = 1.0;

/// The cache file's first line; a file without it is built again.
const CACHE_MAGIC: &str = "dereth object identity 6";

/// One usage of a picture or palette: the kind of place and where.
type Use = (u8, u32, u32, u32);

/// What one era's portal says about the records the verdicts read.
#[derive(Default)]
struct Side {
    /// Graphics object -> the setups and clothing tables naming it.
    gfx_refs: BTreeMap<u32, BTreeSet<u32>>,
    /// Texture -> its uses.
    tex_uses: BTreeMap<u32, BTreeSet<Use>>,
    /// Palette -> its uses.
    pal_uses: BTreeMap<u32, BTreeSet<Use>>,
    /// Setup -> its part list.
    setups: BTreeMap<u32, Vec<u32>>,
}

const USE_SLOT: u8 = 0;
const USE_CLOTHING_NEW: u8 = 1;
const USE_CLOTHING_OLD: u8 = 2;
const USE_CHARGEN_NEW: u8 = 3;
const USE_CHARGEN_OLD: u8 = 4;
const USE_PALETTE_SET: u8 = 5;
const USE_TEXTURE_DEFAULT: u8 = 6;
const USE_CHARGEN_PALETTE: u8 = 7;
const USE_CLOTHING_PALETTE_SET: u8 = 8;

fn ids(f: &DatFile, t: DbType) -> Vec<u32> {
    let era = f.era();
    f.iter_ids()
        .filter(|id| divine_type_in(era, *id) == Some(t))
        .map(|id| id.0)
        .collect()
}

fn get<T: Decode>(f: &DatFile, id: u32) -> Option<T> {
    let b = f.read(DataId(id)).ok()?;
    T::decode_payload_in(f.era(), DataId(id), &b).ok()
}

/// A texture's default palette: the older files keep it on the texture, the later ones on the
/// image level the texture names.
fn texture_palette(f: &DatFile, id: u32) -> Option<u32> {
    let b = f.read(DataId(id)).ok()?;
    let rs = if f.era() == ContainerEra::PreTod {
        RenderSurface::from_pre_tod_texture(DataId(id), &b).ok()?
    } else {
        let st = SurfaceTexture::decode_payload(DataId(id), &b).ok()?;
        let lvl = *st.source_levels.last()?;
        let lb = f.read(lvl).ok()?;
        RenderSurface::decode_payload(lvl, &lb).ok()?
    };
    rs.default_palette_id.map(|p| p.0)
}

fn side(f: &DatFile) -> Side {
    let mut s = Side::default();
    for id in ids(f, DbType::Setup) {
        if let Some(v) = get::<Setup>(f, id) {
            for p in &v.parts {
                s.gfx_refs.entry(p.0).or_default().insert(id);
            }
            s.setups.insert(id, v.parts.iter().map(|p| p.0).collect());
        }
    }
    let mut surfaces: BTreeMap<u32, (Option<u32>, Option<u32>)> = BTreeMap::new();
    for id in ids(f, DbType::Surface) {
        if let Some(v) = get::<Surface>(f, id) {
            surfaces.insert(
                id,
                (
                    v.orig_texture_id.map(|d| d.0),
                    v.orig_palette_id.map(|d| d.0),
                ),
            );
        }
    }
    for id in ids(f, DbType::GfxObj) {
        let Some(v) = get::<GfxObj>(f, id) else {
            continue;
        };
        for (k, sid) in v.surfaces.iter().enumerate() {
            let Some(&(tex, pal)) = surfaces.get(&sid.0) else {
                continue;
            };
            // LINT-OK: a surface slot index; graphics objects list a few dozen at most.
            #[allow(clippy::cast_possible_truncation)]
            let k = k as u32;
            if let Some(t) = tex {
                s.tex_uses
                    .entry(t)
                    .or_default()
                    .insert((USE_SLOT, id, k, 0));
            }
            if let Some(p) = pal {
                s.pal_uses
                    .entry(p)
                    .or_default()
                    .insert((USE_SLOT, id, k, 0));
            }
        }
    }
    for id in ids(f, DbType::Clothing) {
        let Some(v) = get::<ClothingTable>(f, id) else {
            continue;
        };
        for (setup, effects) in &v.clothing_bases {
            for e in effects {
                s.gfx_refs.entry(e.object_id.0).or_default().insert(id);
                for t in &e.texture_effects {
                    let at = (id, setup.0, e.part_num);
                    s.tex_uses.entry(t.new_texture.0).or_default().insert((
                        USE_CLOTHING_NEW,
                        at.0,
                        at.1,
                        at.2,
                    ));
                    s.tex_uses.entry(t.old_texture.0).or_default().insert((
                        USE_CLOTHING_OLD,
                        at.0,
                        at.1,
                        at.2,
                    ));
                }
            }
        }
        for (k, tpl) in &v.palette_templates {
            for (i, e) in tpl.subpalette_effects.iter().enumerate() {
                // LINT-OK: an effect index within one template.
                #[allow(clippy::cast_possible_truncation)]
                let i = i as u32;
                s.pal_uses.entry(e.palette_set.0).or_default().insert((
                    USE_CLOTHING_PALETTE_SET,
                    id,
                    *k,
                    i,
                ));
            }
        }
    }
    for id in ids(f, DbType::PalSet) {
        if let Some(v) = get::<PaletteSet>(f, id) {
            for (i, p) in v.palette_ids.iter().enumerate() {
                // LINT-OK: a position within one palette set.
                #[allow(clippy::cast_possible_truncation)]
                let i = i as u32;
                s.pal_uses
                    .entry(p.0)
                    .or_default()
                    .insert((USE_PALETTE_SET, id, i, 0));
            }
        }
    }
    for id in ids(f, DbType::SurfaceTexture) {
        if let Some(p) = texture_palette(f, id) {
            s.pal_uses
                .entry(p)
                .or_default()
                .insert((USE_TEXTURE_DEFAULT, id, 0, 0));
        }
    }
    if let Some(cg) = get::<CharGen>(f, 0x0E00_0002) {
        for (h, hg) in &cg.heritage_groups {
            for (sx, sex) in &hg.sexes {
                let who = (h << 8) | sx;
                s.pal_uses.entry(sex.base_palette.0).or_default().insert((
                    USE_CHARGEN_PALETTE,
                    who,
                    0,
                    0,
                ));
                let mut od = |kind: u32, i: usize, d: &ObjDesc| {
                    // LINT-OK: a choice index within one character-creation list.
                    #[allow(clippy::cast_possible_truncation)]
                    let at = (kind << 16) | i as u32;
                    for (part, old, new) in &d.texture_changes {
                        let p = u32::from(*part);
                        s.tex_uses
                            .entry(new.0)
                            .or_default()
                            .insert((USE_CHARGEN_NEW, who, at, p));
                        s.tex_uses
                            .entry(old.0)
                            .or_default()
                            .insert((USE_CHARGEN_OLD, who, at, p));
                    }
                    for (_, g) in &d.anim_part_changes {
                        s.gfx_refs.entry(g.0).or_default().insert(0x0E00_0002);
                    }
                };
                od(0, 0, &sex.base_objdesc);
                for (i, x) in sex.hair_styles.iter().enumerate() {
                    od(1, i, &x.objdesc);
                }
                for (i, x) in sex.eye_strips.iter().enumerate() {
                    od(2, i, &x.objdesc);
                }
                for (i, (_, d)) in sex.nose_strips.iter().enumerate() {
                    od(3, i, d);
                }
                for (i, (_, d)) in sex.mouth_strips.iter().enumerate() {
                    od(4, i, d);
                }
            }
        }
    }
    s
}

/// The vertex bounding box of a graphics object.
fn bounds(g: &GfxObj) -> Option<([f32; 3], [f32; 3])> {
    let mut it = g.vertex_array.vertices.iter();
    let f = it.next()?;
    let mut lo = [f.position.x, f.position.y, f.position.z];
    let mut hi = lo;
    for v in it {
        let p = [v.position.x, v.position.y, v.position.z];
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    Some((lo, hi))
}

/// How far apart two boxes are, as a share of the larger one's extent on each axis (at least
/// 5 cm), the worst of the six faces.
#[must_use]
pub fn box_distance(a: ([f32; 3], [f32; 3]), b: ([f32; 3], [f32; 3])) -> f32 {
    let mut worst = 0f32;
    for k in 0..3 {
        let ext = (a.1[k] - a.0[k]).max(b.1[k] - b.0[k]).max(0.05);
        worst = worst
            .max((a.0[k] - b.0[k]).abs() / ext)
            .max((a.1[k] - b.1[k]).abs() / ext);
    }
    worst
}

fn uses_verdict(a: Option<&BTreeSet<Use>>, b: Option<&BTreeSet<Use>>) -> Verdict {
    match (a, b) {
        (Some(a), Some(b)) if !a.is_disjoint(b) => Verdict::SharedUse,
        (Some(a), Some(b)) if !a.is_empty() && !b.is_empty() => Verdict::UsedElsewhere,
        _ => Verdict::NoEvidence,
    }
}

impl ObjectIdentity {
    /// Work out the verdicts for the world's portal `world` against the other era's portal
    /// `other`, either way round.
    #[must_use]
    pub fn build(world: &DatFile, other: &DatFile) -> (Self, IdentityCensus) {
        let (a, b) = (side(world), side(other));
        let mut census = IdentityCensus::default();
        let put = |c: &mut IdentityCensus, t: DbType, id: u32, v: Verdict| {
            c.verdicts.insert((t, id), v);
        };

        // Graphics objects.
        let theirs: HashSet<u32> = ids(other, DbType::GfxObj).into_iter().collect();
        for id in ids(world, DbType::GfxObj) {
            if !theirs.contains(&id) {
                continue;
            }
            let (Some(x), Some(y)) = (get::<GfxObj>(world, id), get::<GfxObj>(other, id)) else {
                continue;
            };
            // The same shape: every vertex where the other era has it. A shell of that shape meets
            // the world's interiors at its doorways whatever its polygons and paint.
            let same_shape = x.vertex_array.vertices.len() == y.vertex_array.vertices.len()
                && x.vertex_array
                    .vertices
                    .iter()
                    .zip(&y.vertex_array.vertices)
                    .all(|(p, q)| p.position == q.position);
            if same_shape {
                census.geometry_same.insert(id);
            }
            let v = if x.vertex_array == y.vertex_array && x.polygons == y.polygons {
                Verdict::Identical
            } else {
                let (ra, rb) = (a.gfx_refs.get(&id), b.gfx_refs.get(&id));
                let close = matches!(
                    (bounds(&x), bounds(&y)),
                    (Some(p), Some(q)) if box_distance(p, q) < CLOSE_SHAPE
                );
                let near = matches!(
                    (bounds(&x), bounds(&y)),
                    (Some(p), Some(q)) if box_distance(p, q) < REMODEL_SHAPE
                );
                match (ra, rb) {
                    (Some(p), Some(q)) if !p.is_disjoint(q) => Verdict::SharedReferrer,
                    _ if close => Verdict::CloseShape,
                    _ if near => Verdict::Remodel,
                    _ => Verdict::Unrelated,
                }
            };
            put(&mut census, DbType::GfxObj, id, v);
        }

        // Setups: the same part list, or a part in common, or a remodel filling the same box.
        let mut remodels = HashMap::new();
        for (id, parts) in &a.setups {
            let Some(theirs) = b.setups.get(id) else {
                continue;
            };
            let v = if parts == theirs {
                Verdict::Identical
            } else if parts.iter().any(|p| theirs.contains(p)) {
                Verdict::SharedReferrer
            } else {
                let (Some(x), Some(y)) = (get::<Setup>(world, *id), get::<Setup>(other, *id))
                else {
                    continue;
                };
                match (setup_box(world, &x), setup_box(other, &y)) {
                    (Some(p), Some(q)) if box_distance(p, q) < REMODEL_SHAPE => {
                        if let Some(l) = remodel_parts(&x, &y) {
                            remodels.insert(*id, l);
                        }
                        Verdict::Remodel
                    }
                    _ => Verdict::Unrelated,
                }
            };
            put(&mut census, DbType::Setup, *id, v);
        }

        // Textures and palettes, by use.
        let theirs: HashSet<u32> = ids(other, DbType::SurfaceTexture).into_iter().collect();
        for id in ids(world, DbType::SurfaceTexture) {
            if theirs.contains(&id) {
                let v = uses_verdict(a.tex_uses.get(&id), b.tex_uses.get(&id));
                put(&mut census, DbType::SurfaceTexture, id, v);
            }
        }
        let theirs: HashSet<u32> = ids(other, DbType::Palette).into_iter().collect();
        for id in ids(world, DbType::Palette) {
            if theirs.contains(&id) {
                let v = uses_verdict(a.pal_uses.get(&id), b.pal_uses.get(&id));
                put(&mut census, DbType::Palette, id, v);
            }
        }

        // Surfaces: counted only, to show they are numbered per era.
        let theirs: HashSet<u32> = ids(other, DbType::Surface).into_iter().collect();
        let named = |f: &DatFile| -> BTreeMap<u32, BTreeSet<u32>> {
            let mut m: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
            for g in ids(f, DbType::GfxObj) {
                if let Some(v) = get::<GfxObj>(f, g) {
                    for s in &v.surfaces {
                        m.entry(s.0).or_default().insert(g);
                    }
                }
            }
            m
        };
        let (na, nb) = (named(world), named(other));
        for id in ids(world, DbType::Surface) {
            if theirs.contains(&id) {
                census.surfaces_shared += 1;
                if matches!((na.get(&id), nb.get(&id)), (Some(p), Some(q)) if !p.is_disjoint(q)) {
                    census.surfaces_named_alike += 1;
                }
            }
        }

        let same = census
            .verdicts
            .iter()
            .filter(|(_, v)| v.is_same())
            .map(|((_, id), _)| *id)
            .collect();
        let geometry_same = census.geometry_same.iter().copied().collect();
        let bare = bare_parts(world, other);
        census.bare_parts = bare.len();
        let hair = hair_styles(world, other);
        census.hair_styles = hair.values().map(Vec::len).sum();
        let palettes = palette_translations(world, other);
        census.palettes = palettes.len();
        census.remodels_drawable = remodels.len();
        (
            Self {
                same,
                geometry_same,
                bare,
                rooms: HashSet::new(),
                remodels,
                hair,
                palettes,
            },
            census,
        )
    }

    /// Whether the other era's record of `id` is the same object as the world's.
    #[must_use]
    pub fn same(&self, id: DataId) -> bool {
        self.same.contains(&id.0)
    }

    /// What body part `part` draws in the look's era when it wears `model`, the world's bare
    /// model of that part on some body of the world's character tables; `None` for any other
    /// model.
    #[must_use]
    pub fn bare_part(&self, part: u32, model: DataId) -> Option<&BarePart> {
        self.bare.get(&(part, model.0))
    }

    /// Compare the world's interior cells with the other era's, room by room: a cell may draw
    /// from the other era's record of it when that record is the same room (the same environment
    /// and room of it, every vertex of the room where the world's is) in the same place (frame
    /// and portals), naming only surfaces the other era's portal holds. The furniture is not
    /// compared: the world's is drawn in either era's room. Its surfaces are then read in the other era's files, never the world's.
    /// Replaces any rooms already held, and returns the census.
    pub fn build_rooms(
        &mut self,
        world_cell: &DatFile,
        other_cell: &DatFile,
        world_portal: &DatFile,
        other_portal: &DatFile,
    ) -> RoomCensus {
        let (r, c) = rooms(world_cell, other_cell, world_portal, other_portal);
        self.rooms = r;
        c
    }

    /// Whether the world's interior cell `cell` may draw from the other era's record of it.
    #[must_use]
    pub fn same_room(&self, cell: u32) -> bool {
        self.rooms.contains(&cell)
    }

    /// How many interior cells may.
    #[must_use]
    pub fn rooms_len(&self) -> usize {
        self.rooms.len()
    }

    /// The look's part list for the remodelled setup `setup`, where a server object built on the
    /// world's setup can draw it: the look's has no more parts, and each rests where the world's
    /// part of the same index rests. A part past the end of the list draws nothing.
    #[must_use]
    pub fn remodel(&self, setup: DataId) -> Option<&[DataId]> {
        self.remodels.get(&setup.0).map(Vec::as_slice)
    }

    /// What body part `part` draws in the look's era when it wears the world's head `model` with
    /// the picture changes `maps`: the look's head for the same hair style, with the look style's
    /// own changes in place of the world style's (`world_maps`, all of which `maps` carries).
    /// `None` for a head no translated style wears.
    #[must_use]
    pub fn hair(&self, part: u32, model: DataId, maps: &[(DataId, DataId)]) -> Option<&BarePart> {
        self.hair
            .get(&(part, model.0))?
            .iter()
            .find(|s| s.world_maps.iter().all(|m| maps.contains(m)))
    }

    /// The look's palette for the world's palette `id`, which the look lacks; `None` when no
    /// palette set translates it.
    #[must_use]
    pub fn palette(&self, id: DataId) -> Option<DataId> {
        self.palettes.get(&id.0).copied().map(DataId)
    }

    /// Whether the graphics object `id` has every vertex in the same place in both eras.
    #[must_use]
    pub fn same_geometry(&self, id: DataId) -> bool {
        self.geometry_same.contains(&id.0)
    }

    /// How many ids the other era may stand in for.
    #[must_use]
    pub fn len(&self) -> usize {
        self.same.len()
    }

    /// Whether no id may.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.same.is_empty()
    }

    /// The cache file for this pair of files under `dir`, named by both files' hashes. `None`
    /// when a file cannot be read whole (a store opened from memory).
    #[must_use]
    pub fn cache_file(dir: &Path, world: &DatFile, other: &DatFile) -> Option<PathBuf> {
        Self::cache_file_with(dir, world, other, None)
    }

    /// [`Self::cache_file`], with both eras' cell files joining the name when `cells` (the
    /// world's, the other era's) are compared too.
    #[must_use]
    pub fn cache_file_with(
        dir: &Path,
        world: &DatFile,
        other: &DatFile,
        cells: Option<(&DatFile, &DatFile)>,
    ) -> Option<PathBuf> {
        let mut name = format!(
            "{}-{}",
            &file_hash(world.path())?[..16],
            &file_hash(other.path())?[..16]
        );
        if let Some((wc, oc)) = cells {
            let _ = write!(
                name,
                "-{}-{}",
                &file_hash(wc.path())?[..16],
                &file_hash(oc.path())?[..16]
            );
        }
        Some(dir.join(format!("{name}.txt")))
    }

    /// The verdicts for this pair of files: read from `cache` when it holds them, worked out and
    /// written there otherwise. Without a cache, or when it cannot be read or written, they are
    /// worked out in memory every time.
    #[must_use]
    pub fn load_or_build(world: &DatFile, other: &DatFile, cache: Option<&Path>) -> Self {
        Self::load_or_build_with(world, other, None, cache)
    }

    /// [`Self::load_or_build`], with the rooms of `cells` (the world's cell file, the other
    /// era's) compared too ([`Self::build_rooms`]) and kept in the same cache file.
    #[must_use]
    pub fn load_or_build_with(
        world: &DatFile,
        other: &DatFile,
        cells: Option<(&DatFile, &DatFile)>,
        cache: Option<&Path>,
    ) -> Self {
        let file = cache.and_then(|d| Self::cache_file_with(d, world, other, cells));
        if let Some(f) = &file {
            if let Some(found) = std::fs::read_to_string(f)
                .ok()
                .and_then(|t| Self::parse(&t))
            {
                return found;
            }
        }
        let (mut built, _) = Self::build(world, other);
        if let Some((wc, oc)) = cells {
            let _ = built.build_rooms(wc, oc, world, other);
        }
        if let Some(f) = &file {
            let written = f
                .parent()
                .map_or(Ok(()), std::fs::create_dir_all)
                .and_then(|()| std::fs::write(f, built.to_text()));
            if let Err(e) = written {
                tracing::warn!(
                    "the object identity cache {} was not written: {e}",
                    f.display()
                );
            }
        }
        built
    }

    /// The cache file's text: the magic line, then one id per line, `s` for the same object and
    /// `g` for the same geometry; then the translations, one per line: `b` bare parts, `h` hair
    /// styles, `r` remodels and `p` palettes; then `c` runs of consecutive rooms (the first cell id
    /// and how many).
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut t = format!("{CACHE_MAGIC}\n");
        let mut s: Vec<u32> = self.same.iter().copied().collect();
        s.sort_unstable();
        for id in s {
            let _ = writeln!(t, "s {id:08X}");
        }
        let mut g: Vec<u32> = self.geometry_same.iter().copied().collect();
        g.sort_unstable();
        for id in g {
            let _ = writeln!(t, "g {id:08X}");
        }
        let mut b: Vec<(&(u32, u32), &BarePart)> = self.bare.iter().collect();
        b.sort_by_key(|(k, _)| **k);
        let maps = |m: &[(DataId, DataId)]| {
            m.iter()
                .map(|(o, n)| format!("{:08X}>{:08X}", o.0, n.0))
                .collect::<Vec<_>>()
                .join(",")
        };
        for ((part, wm), v) in b {
            let _ = writeln!(
                t,
                "b {part} {wm:08X} {:08X} {} {}",
                v.model.0,
                maps(&v.look_maps),
                maps(&v.world_maps)
            );
        }
        let mut r: Vec<u32> = self.rooms.iter().copied().collect();
        r.sort_unstable();
        let mut i = 0;
        while i < r.len() {
            let mut n = 1u32;
            while r.get(i + n as usize) == Some(&(r[i] + n)) {
                n += 1;
            }
            let _ = writeln!(t, "c {:08X} {n}", r[i]);
            i += n as usize;
        }
        let mut h: Vec<(&(u32, u32), &Vec<BarePart>)> = self.hair.iter().collect();
        h.sort_by_key(|(k, _)| **k);
        for ((part, wm), list) in h {
            for v in list {
                let _ = writeln!(
                    t,
                    "h {part} {wm:08X} {:08X} {} {}",
                    v.model.0,
                    maps(&v.look_maps),
                    maps(&v.world_maps)
                );
            }
        }
        let mut r: Vec<(&u32, &Vec<DataId>)> = self.remodels.iter().collect();
        r.sort_by_key(|(k, _)| **k);
        for (setup, parts) in r {
            let list: Vec<String> = parts.iter().map(|p| format!("{:08X}", p.0)).collect();
            let _ = writeln!(t, "r {setup:08X} {}", list.join(","));
        }
        let mut p: Vec<(&u32, &u32)> = self.palettes.iter().collect();
        p.sort_unstable();
        for (w, l) in p {
            let _ = writeln!(t, "p {w:08X} {l:08X}");
        }
        t
    }

    /// [`Self::to_text`] read back; `None` for anything else.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let mut lines = text.lines();
        if lines.next()? != CACHE_MAGIC {
            return None;
        }
        let mut out = Self::default();
        let hex = |x: &str| u32::from_str_radix(x, 16).ok().map(DataId);
        let maps = |x: &str| -> Option<Vec<(DataId, DataId)>> {
            x.split(',')
                .filter(|m| !m.is_empty())
                .map(|m| {
                    let (o, n) = m.split_once('>')?;
                    Some((hex(o)?, hex(n)?))
                })
                .collect()
        };
        for l in lines {
            let (kind, rest) = l.split_once(' ')?;
            match kind {
                "s" => {
                    out.same.insert(hex(rest)?.0);
                }
                "g" => {
                    out.geometry_same.insert(hex(rest)?.0);
                }
                "b" | "h" => {
                    // Five fields; the two change lists may be empty.
                    let f: Vec<&str> = rest.split(' ').collect();
                    if f.len() != 5 {
                        return None;
                    }
                    let key = (f[0].parse().ok()?, hex(f[1])?.0);
                    let v = BarePart {
                        model: hex(f[2])?,
                        look_maps: maps(f[3])?,
                        world_maps: maps(f[4])?,
                    };
                    if kind == "b" {
                        out.bare.insert(key, v);
                    } else {
                        out.hair.entry(key).or_default().push(v);
                    }
                }
                "r" => {
                    let (setup, list) = rest.split_once(' ')?;
                    let parts = list.split(',').map(hex).collect::<Option<Vec<_>>>()?;
                    out.remodels.insert(hex(setup)?.0, parts);
                }
                "p" => {
                    let (w, l) = rest.split_once(' ')?;
                    out.palettes.insert(hex(w)?.0, hex(l)?.0);
                }
                "c" => {
                    let (first, n) = rest.split_once(' ')?;
                    let (first, n) = (hex(first)?.0, n.parse::<u32>().ok()?);
                    for k in 0..n {
                        out.rooms.insert(first.checked_add(k)?);
                    }
                }
                _ => return None,
            }
        }
        Some(out)
    }
}

/// A file's BLAKE3 hash in hex, read whole from disk. `None` when it cannot be read.
fn file_hash(path: &Path) -> Option<String> {
    use std::io::Read as _;
    let mut f = std::fs::File::open(path).ok()?;
    let mut h = blake3::Hasher::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Some(h.finalize().to_hex().to_string())
}

/// Where the desktop client keeps the cache: `%LOCALAPPDATA%\Dereth\object-identity` on
/// Windows, `~/Library/Caches/Dereth/object-identity` on macOS, and
/// `$XDG_CACHE_HOME/dereth/object-identity` (else `~/.cache/dereth/object-identity`) elsewhere.
/// Worked out once per run.
#[must_use]
pub fn default_cache_dir() -> Option<&'static Path> {
    static DIR: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    DIR.get_or_init(cache_dir_from_env).as_deref()
}

fn cache_dir_from_env() -> Option<PathBuf> {
    let var = |n: &str| {
        std::env::var_os(n)
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
    };
    let base = if cfg!(windows) {
        var("LOCALAPPDATA")?.join("Dereth")
    } else if cfg!(target_os = "macos") {
        var("HOME")?.join("Library/Caches/Dereth")
    } else {
        var("XDG_CACHE_HOME")
            .or_else(|| var("HOME").map(|h| h.join(".cache")))?
            .join("dereth")
    };
    Some(base.join("object-identity"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Behaviour: none (the cache file's own text round trip; the verdicts are tested against
    /// the retail files in the dat tier).
    #[test]
    fn the_cache_text_reads_back_as_the_same_identity_and_refuses_anything_else() {
        let mut id = ObjectIdentity::default();
        id.same.insert(0x0100_122B);
        id.same.insert(0x0500_02BE);
        id.geometry_same.insert(0x0100_122B);
        id.bare.insert(
            (10, 0x0100_0055),
            BarePart {
                model: DataId(0x0100_0497),
                look_maps: vec![(DataId(0x0500_0001), DataId(0x0500_0002))],
                world_maps: Vec::new(),
            },
        );
        for cell in [0xA9B4_0100, 0xA9B4_0101, 0xA9B4_0102, 0xA9B4_0105] {
            id.rooms.insert(cell);
        }
        let back = ObjectIdentity::parse(&id.to_text()).expect("reads back");
        assert!(back.same_room(0xA9B4_0101) && back.same_room(0xA9B4_0105));
        assert!(!back.same_room(0xA9B4_0103));
        assert_eq!(back, id);
        assert!(back.same(DataId(0x0500_02BE)));
        assert!(!back.same_geometry(DataId(0x0500_02BE)));
        assert_eq!(
            back.bare_part(10, DataId(0x0100_0055)).map(|b| b.model),
            Some(DataId(0x0100_0497))
        );
        assert!(ObjectIdentity::parse("something else\ns 01000001\n").is_none());
        assert!(ObjectIdentity::parse(&format!("{CACHE_MAGIC}\nx 01000001\n")).is_none());
    }

    /// Behaviour: none (geometry arithmetic).
    #[test]
    fn a_box_moved_by_half_its_size_is_half_its_size_away() {
        let a = ([0.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
        assert!(box_distance(a, a) < f32::EPSILON);
        let b = ([1.0, 0.0, 0.0], [3.0, 2.0, 2.0]);
        assert!((box_distance(a, b) - 0.5).abs() < 1e-6);
    }
}
