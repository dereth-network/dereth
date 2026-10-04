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
//! - **A world's overlay** (records a world adds, replaces or deletes over the locked files) keeps
//!   the world's own look for everything it touches: the verdicts are worked out over the locked
//!   files, and then any record the overlay holds or deletes, any graphics object whose surfaces,
//!   textures, pictures or palettes reach one, any setup with such a part, and any room the
//!   overlay changes (or whose environment or surfaces it changes) is no longer the same as the
//!   other era's ([`pin_to_world`](crate::object_identity::ObjectIdentity::pin_to_world)). The
//!   other era's look is Turbine's by definition, and a world's own change is the world's.
//!
//! [`load_or_build`](crate::object_identity::ObjectIdentity::load_or_build) keeps the answer in
//! a per-user cache file keyed by both files' hashes, so a pair of files is only worked out once.
//!
//! The work is long (seconds for a pair of files never seen before), so it is written to be done
//! a little at a time: [`IdentityBuild`](crate::object_identity::IdentityBuild) takes it in steps
//! of a few milliseconds, one per frame, from the moment the client starts, with the same results
//! as doing it at once. The cache is read and written through the host's file store
//! ([`crate::platform::files`]), so a host without a disk keeps it too.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt::Write as _;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::time::Duration;

use dereth_assets::geometry::Environment;
use dereth_assets::tables::ObjDesc;
use dereth_assets::world::EnvCell;
use dereth_assets::{
    CharGen, ClothingTable, Decode, GfxObj, PaletteSet, RenderSurface, Setup, Surface,
    SurfaceTexture,
};
use dereth_dat::{divine_type_in, ContainerEra, DatFile, DbType, RetailDatStore};
use dereth_primitives::frame::{localtoglobal, V3 as _};
use dereth_primitives::{DataId, Frame, Vec3};

use crate::platform::files as host_files;

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
async fn rooms(
    world_cell: &DatFile,
    other_cell: &DatFile,
    world_portal: &DatFile,
    other_portal: &DatFile,
    pace: &Pace,
) -> (HashSet<u32>, RoomCensus) {
    let mut out = HashSet::new();
    let mut c = RoomCensus::default();
    let (mut wenv, mut oenv) = (HashMap::new(), HashMap::new());
    // Environment rooms already compared: (environment, room) -> every vertex in the same place.
    let mut shapes: HashMap<(u32, u16), bool> = HashMap::new();
    let mut held: HashMap<u32, bool> = HashMap::new();
    for id in world_cell.iter_ids() {
        pace.tick().await;
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
    // The environments decoded on the way are many and large: letting them go is paced too.
    for (n, _) in wenv.drain().chain(oenv.drain()).enumerate() {
        if n % ENVIRONMENTS_PER_TICK == 0 {
            pace.tick().await;
        }
    }
    (out, c)
}

/// How many decoded environments one unit of work lets go of.
const ENVIRONMENTS_PER_TICK: usize = 256;

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
/// translation found stands (heritages in order, then the shared sets by id). A palette set
/// between each tick of `pace`.
async fn palette_translations(world: &DatFile, other: &DatFile, pace: &Pace) -> HashMap<u32, u32> {
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
    let theirs: HashSet<u32> = ids(other, DbType::PalSet, pace).await.into_iter().collect();
    pairs.extend(
        ids(world, DbType::PalSet, pace)
            .await
            .into_iter()
            .filter(|s| theirs.contains(s))
            .map(|s| (s, s)),
    );
    for (ws, ls) in pairs {
        pace.tick().await;
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
    /// Surface -> the graphics objects naming it (for the census).
    surfaces_named: BTreeMap<u32, BTreeSet<u32>>,
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

/// Every id of record type `t` in `f`, ascending.
async fn ids(f: &DatFile, t: DbType, pace: &Pace) -> Vec<u32> {
    let era = f.era();
    let mut out = Vec::new();
    for (n, id) in f.iter_ids().enumerate() {
        if n % IDS_PER_TICK == 0 {
            pace.tick().await;
        }
        if divine_type_in(era, id) == Some(t) {
            out.push(id.0);
        }
    }
    out
}

/// How many directory entries one unit of work sorts by type: sorting one is far cheaper than
/// decoding a record.
const IDS_PER_TICK: usize = 1024;

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

async fn side(f: &DatFile, pace: &Pace) -> Side {
    let mut s = Side::default();
    for id in ids(f, DbType::Setup, pace).await {
        pace.tick().await;
        if let Some(v) = get::<Setup>(f, id) {
            for p in &v.parts {
                s.gfx_refs.entry(p.0).or_default().insert(id);
            }
            s.setups.insert(id, v.parts.iter().map(|p| p.0).collect());
        }
    }
    let mut surfaces: BTreeMap<u32, (Option<u32>, Option<u32>)> = BTreeMap::new();
    for id in ids(f, DbType::Surface, pace).await {
        pace.tick().await;
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
    for id in ids(f, DbType::GfxObj, pace).await {
        pace.tick().await;
        let Some(v) = get::<GfxObj>(f, id) else {
            continue;
        };
        for (k, sid) in v.surfaces.iter().enumerate() {
            s.surfaces_named.entry(sid.0).or_default().insert(id);
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
    for id in ids(f, DbType::Clothing, pace).await {
        pace.tick().await;
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
    for id in ids(f, DbType::PalSet, pace).await {
        pace.tick().await;
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
    for id in ids(f, DbType::SurfaceTexture, pace).await {
        pace.tick().await;
        if let Some(p) = texture_palette(f, id) {
            s.pal_uses
                .entry(p)
                .or_default()
                .insert((USE_TEXTURE_DEFAULT, id, 0, 0));
        }
    }
    pace.tick().await;
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

/// Work out the verdicts for the world's portal `world` against the other era's portal `other`,
/// a unit of work between each tick of `pace` ([`ObjectIdentity::build`]).
async fn build(world: &DatFile, other: &DatFile, pace: &Pace) -> (ObjectIdentity, IdentityCensus) {
    let a = side(world, pace).await;
    let b = side(other, pace).await;
    let mut census = IdentityCensus::default();
    let put = |c: &mut IdentityCensus, t: DbType, id: u32, v: Verdict| {
        c.verdicts.insert((t, id), v);
    };

    // Graphics objects.
    let theirs: HashSet<u32> = ids(other, DbType::GfxObj, pace).await.into_iter().collect();
    for id in ids(world, DbType::GfxObj, pace).await {
        pace.tick().await;
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
        pace.tick().await;
        let Some(theirs) = b.setups.get(id) else {
            continue;
        };
        let v = if parts == theirs {
            Verdict::Identical
        } else if parts.iter().any(|p| theirs.contains(p)) {
            Verdict::SharedReferrer
        } else {
            let (Some(x), Some(y)) = (get::<Setup>(world, *id), get::<Setup>(other, *id)) else {
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
    let theirs: HashSet<u32> = ids(other, DbType::SurfaceTexture, pace)
        .await
        .into_iter()
        .collect();
    for id in ids(world, DbType::SurfaceTexture, pace).await {
        if theirs.contains(&id) {
            let v = uses_verdict(a.tex_uses.get(&id), b.tex_uses.get(&id));
            put(&mut census, DbType::SurfaceTexture, id, v);
        }
    }
    let theirs: HashSet<u32> = ids(other, DbType::Palette, pace)
        .await
        .into_iter()
        .collect();
    for id in ids(world, DbType::Palette, pace).await {
        if theirs.contains(&id) {
            let v = uses_verdict(a.pal_uses.get(&id), b.pal_uses.get(&id));
            put(&mut census, DbType::Palette, id, v);
        }
    }

    // Surfaces: counted only, to show they are numbered per era.
    let theirs: HashSet<u32> = ids(other, DbType::Surface, pace)
        .await
        .into_iter()
        .collect();
    let (na, nb) = (&a.surfaces_named, &b.surfaces_named);
    for id in ids(world, DbType::Surface, pace).await {
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
    pace.tick().await;
    let bare = bare_parts(world, other);
    census.bare_parts = bare.len();
    pace.tick().await;
    let hair = hair_styles(world, other);
    census.hair_styles = hair.values().map(Vec::len).sum();
    let palettes = palette_translations(world, other, pace).await;
    census.palettes = palettes.len();
    census.remodels_drawable = remodels.len();
    (
        ObjectIdentity {
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

impl ObjectIdentity {
    /// Work out the verdicts for the world's portal `world` against the other era's portal
    /// `other`, either way round, all at once.
    #[must_use]
    pub fn build(world: &DatFile, other: &DatFile) -> (Self, IdentityCensus) {
        run_to_end(build(world, other, &Pace::unlimited()))
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
        let (r, c) = run_to_end(rooms(
            world_cell,
            other_cell,
            world_portal,
            other_portal,
            &Pace::unlimited(),
        ));
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

    /// Keep the world's own look for everything the world's overlay over `portal` and `cell`
    /// touches: a record it holds or deletes, a graphics object whose surfaces reach one (through
    /// their textures, pictures and palettes), a setup with such a part, and a room it changes or
    /// whose environment or surfaces it changes. Answers how many verdicts were withdrawn.
    pub fn pin_to_world(&mut self, portal: &DatFile, cell: &DatFile) -> usize {
        let before = self.same.len() + self.rooms.len();
        let touched = |f: &DatFile| -> HashSet<u32> {
            let Some(layer) = f.layer() else {
                return HashSet::new();
            };
            let mut t: HashSet<u32> = layer.records().map(|(id, _)| id.0).collect();
            t.extend(
                f.base_entries()
                    .filter(|(id, _)| layer.hides(*id))
                    .map(|(id, _)| id.0),
            );
            t
        };
        let t = touched(portal);
        if !t.is_empty() {
            let mut surface_reaches: HashMap<u32, bool> = HashMap::new();
            let mut reaches_surface = |s: u32| -> bool {
                *surface_reaches.entry(s).or_insert_with(|| {
                    if t.contains(&s) {
                        return true;
                    }
                    let Some(surface) = get::<Surface>(portal, s) else {
                        return false;
                    };
                    let tex = surface.orig_texture_id.map(|d| d.0);
                    let pal = surface.orig_palette_id.map(|d| d.0);
                    if tex.is_some_and(|x| t.contains(&x)) || pal.is_some_and(|x| t.contains(&x)) {
                        return true;
                    }
                    tex.and_then(|x| get::<SurfaceTexture>(portal, x))
                        .is_some_and(|st| st.source_levels.iter().any(|l| t.contains(&l.0)))
                })
            };
            let gfx: Vec<u32> = self
                .same
                .iter()
                .copied()
                .filter(|id| id >> 24 == 0x01)
                .collect();
            let mut gone: HashSet<u32> = t.clone();
            for g in gfx {
                if gone.contains(&g) {
                    continue;
                }
                let reaches = get::<GfxObj>(portal, g)
                    .is_some_and(|o| o.surfaces.iter().any(|s| reaches_surface(s.0)));
                if reaches {
                    gone.insert(g);
                }
            }
            let setups: Vec<u32> = self
                .same
                .iter()
                .copied()
                .filter(|id| id >> 24 == 0x02)
                .collect();
            for su in setups {
                let reaches = get::<Setup>(portal, su)
                    .is_some_and(|s| s.parts.iter().any(|p| gone.contains(&p.0)));
                if reaches {
                    gone.insert(su);
                }
            }
            self.same.retain(|id| !gone.contains(id));
            self.geometry_same.retain(|id| !gone.contains(id));
            self.remodels.retain(|id, _| !gone.contains(id));
            self.palettes.retain(|id, _| !gone.contains(id));
            self.bare.retain(|(_, model), _| !gone.contains(model));
            self.hair.retain(|(_, model), _| !gone.contains(model));
            // A room whose environment or surfaces the overlay changes is the world's room.
            if gone
                .iter()
                .any(|id| matches!(id >> 24, 0x0D | 0x08 | 0x05 | 0x06 | 0x04))
            {
                let rooms: Vec<u32> = self.rooms.iter().copied().collect();
                for r in rooms {
                    let reaches = get::<EnvCell>(cell, r).is_some_and(|c| {
                        gone.contains(&c.environment.0)
                            || c.surfaces.iter().any(|s| reaches_surface(s.0))
                    });
                    if reaches {
                        self.rooms.remove(&r);
                    }
                }
            }
        }
        let c = touched(cell);
        self.rooms.retain(|r| !c.contains(r));
        before - (self.same.len() + self.rooms.len())
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

    /// The cache file under `dir`, named by the hashes of both files and, when compared,
    /// both eras' `cells`. Every file is hashed whole; `None` means a read failed.
    #[must_use]
    pub fn cache_file_with(
        dir: &Path,
        world: &DatFile,
        other: &DatFile,
        cells: Option<(&DatFile, &DatFile)>,
    ) -> Option<PathBuf> {
        let files = IdentityFiles {
            world,
            other,
            cells,
        };
        let (name, _) = run_to_end(cache_name(files, None, &Pace::unlimited()))?;
        Some(dir.join(name))
    }

    /// The verdicts for this pair of files: read from `cache` when it holds them, worked out and
    /// written there otherwise. Without a cache, or when it cannot be read or written, they are
    /// worked out in memory every time.
    #[must_use]
    pub fn load_or_build(world: &DatFile, other: &DatFile, cache: Option<&Path>) -> Self {
        Self::load_or_build_with(world, other, None, cache)
    }

    /// [`Self::load_or_build`], with the rooms of `cells` (the world's cell file, the other
    /// era's) compared too ([`Self::build_rooms`]) and kept in the same cache file. All at once:
    /// [`IdentityBuild`] does the same work a step at a time.
    #[must_use]
    pub fn load_or_build_with(
        world: &DatFile,
        other: &DatFile,
        cells: Option<(&DatFile, &DatFile)>,
        cache: Option<&Path>,
    ) -> Self {
        let files = IdentityFiles {
            world,
            other,
            cells,
        };
        run_to_end(load_or_build(files, cache, &Pace::unlimited())).0
    }

    /// The cache file's text: the magic line, then one id per line, `s` for the same object and
    /// `g` for the same geometry; then the translations, one per line: `b` bare parts, `h` hair
    /// styles, `r` remodels and `p` palettes; then `c` runs of consecutive rooms (the first cell id
    /// and how many).
    #[must_use]
    pub fn to_text(&self) -> String {
        run_to_end(self.to_text_paced(&Pace::unlimited()))
    }

    /// [`Self::to_text`], a few thousand lines between each tick of `pace`.
    async fn to_text_paced(&self, pace: &Pace) -> String {
        let mut t = format!("{CACHE_MAGIC}\n");
        let mut s: Vec<u32> = self.same.iter().copied().collect();
        s.sort_unstable();
        for id in s {
            let _ = writeln!(t, "s {id:08X}");
        }
        pace.tick().await;
        let mut g: Vec<u32> = self.geometry_same.iter().copied().collect();
        g.sort_unstable();
        for id in g {
            let _ = writeln!(t, "g {id:08X}");
        }
        pace.tick().await;
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
        // The rooms in ascending order, a landblock at a time (sorting them all at once is a
        // unit of work too long for one frame), as runs of consecutive cells.
        let mut blocks: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
        for (k, cell) in self.rooms.iter().enumerate() {
            if k % ROOMS_PER_TICK == 0 {
                pace.tick().await;
            }
            blocks.entry(cell >> 16).or_default().push(*cell);
        }
        let mut run: Option<(u32, u32)> = None;
        for (k, (_, mut cells)) in blocks.into_iter().enumerate() {
            if k % 64 == 0 {
                pace.tick().await;
            }
            cells.sort_unstable();
            for cell in cells {
                run = match run {
                    Some((first, n)) if first.checked_add(n) == Some(cell) => Some((first, n + 1)),
                    Some((first, n)) => {
                        let _ = writeln!(t, "c {first:08X} {n}");
                        Some((cell, 1))
                    }
                    None => Some((cell, 1)),
                };
            }
        }
        if let Some((first, n)) = run {
            let _ = writeln!(t, "c {first:08X} {n}");
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
        run_to_end(Self::parse_paced(text, &Pace::unlimited()))
    }

    /// [`Self::parse`], a line between each tick of `pace`.
    async fn parse_paced(text: &str, pace: &Pace) -> Option<Self> {
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
            pace.tick().await;
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

/// The files one set of verdicts is worked out from: the world's portal, the other era's, and
/// both eras' cell files (the world's, the other era's) when the rooms are compared too.
#[derive(Debug, Clone, Copy)]
struct IdentityFiles<'a> {
    world: &'a DatFile,
    other: &'a DatFile,
    cells: Option<(&'a DatFile, &'a DatFile)>,
}

impl<'a> IdentityFiles<'a> {
    /// Every file, in the order the cache file's name lists them.
    fn all(&self) -> Vec<&'a DatFile> {
        let mut out = vec![self.world, self.other];
        if let Some((w, o)) = self.cells {
            out.extend([w, o]);
        }
        out
    }
}

/// The verdicts for `files`: read from the cache folder `cache` when it holds them, worked out
/// and written there otherwise; in memory alone without one, or when it cannot be read or
/// written. A unit of work between each tick of `pace`.
async fn load_or_build(
    files: IdentityFiles<'_>,
    cache: Option<&Path>,
    pace: &Pace,
) -> (ObjectIdentity, IdentitySource) {
    let mut source = IdentitySource::default();
    let file = match cache {
        Some(dir) => cache_name(files, Some(dir), pace).await.map(|(n, hashed)| {
            source.files_hashed = hashed;
            dir.join(n)
        }),
        None => None,
    };
    if let Some(f) = &file {
        pace.tick().await;
        if let Ok(text) = host_files::read_to_string(f) {
            if let Some(found) = ObjectIdentity::parse_paced(&text, pace).await {
                source.from_cache = true;
                return (found, source);
            }
        }
    }
    let (mut built, _) = build(files.world, files.other, pace).await;
    if let Some((wc, oc)) = files.cells {
        let (r, _) = rooms(wc, oc, files.world, files.other, pace).await;
        built.rooms = r;
    }
    if let Some(f) = &file {
        let text = built.to_text_paced(pace).await;
        pace.tick().await;
        let written = f
            .parent()
            .map_or(Ok(()), host_files::make_dirs)
            .and_then(|()| host_files::write(f, text));
        if let Err(e) = written {
            tracing::warn!(
                "the object identity cache {} was not written: {e}",
                f.display()
            );
        }
    }
    (built, source)
}

/// Where an [`IdentityBuild`]'s verdicts came from.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct IdentitySource {
    /// Read back from the cache file, rather than worked out from the files.
    pub from_cache: bool,
    /// How many of the files were read whole to hash them; the others were known by their
    /// fingerprint.
    pub files_hashed: usize,
}

/// The cache file's name for `files`: the first 16 hex digits of each file's BLAKE3 hash,
/// `<world portal>-<other portal>[-<world cell>-<other cell>].txt`. `None` when a file cannot be
/// read whole. With `index`, a file whose fingerprint was seen before is not read again
/// ([`dat_hash`]); the count is of the files that were.
async fn cache_name(
    files: IdentityFiles<'_>,
    index: Option<&Path>,
    pace: &Pace,
) -> Option<(String, usize)> {
    let mut name = String::new();
    let mut hashed = 0;
    for f in files.all() {
        let (h, read) = dat_hash(f, index, pace).await?;
        hashed += usize::from(read);
        if !name.is_empty() {
            name.push('-');
        }
        name.push_str(&h[..16]);
    }
    Some((format!("{name}.txt"), hashed))
}

/// The first line of a file of the hash index ([`dat_hash`]).
const HASH_MAGIC: &str = "dereth dat hash 1";

/// What a file's fingerprint hashes first, so a change to what it covers changes every one.
const FINGERPRINT_MAGIC: &str = "dereth dat fingerprint 1";

/// How many bytes of a file one unit of hashing reads.
const HASH_CHUNK: usize = 1 << 18;

/// How many directory entries one unit of fingerprinting covers.
const ENTRIES_PER_TICK: usize = 4096;

/// How many rooms one unit of writing the cache file sorts into their landblocks.
const ROOMS_PER_TICK: usize = 65_536;

/// A file's BLAKE3 hash in hex. With `index`, a folder of earlier answers kept by the file's
/// fingerprint ([`fingerprint`]): the file is read whole only when its fingerprint is not there,
/// and the answer is kept there for the next time. `None` when the file cannot be read whole;
/// with the hash, whether the file was read whole for it.
async fn dat_hash(f: &DatFile, index: Option<&Path>, pace: &Pace) -> Option<(String, bool)> {
    let Some(dir) = index else {
        return Some((content_hash(f, pace).await?, true));
    };
    let fp = fingerprint(f, pace).await;
    let key = dir.join(format!("dat-{}.hash", &fp[..32]));
    pace.tick().await;
    let known = host_files::read_to_string(&key).ok().and_then(|t| {
        let mut lines = t.lines();
        (lines.next()? == HASH_MAGIC)
            .then(|| lines.next())
            .flatten()
            .filter(|h| h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit()))
            .map(str::to_owned)
    });
    if let Some(h) = known {
        return Some((h, false));
    }
    let h = content_hash(f, pace).await?;
    let written = host_files::make_dirs(dir)
        .and_then(|()| host_files::write(&key, format!("{HASH_MAGIC}\n{h}\n")));
    if let Err(e) = written {
        tracing::warn!("the dat hash {} was not kept: {e}", key.display());
    }
    Some((h, true))
}

/// What tells one state of a file from another without reading it whole: its header and every
/// entry of its directory (each record's id, place, size, date and iteration), hashed. A record
/// written into a file takes new blocks and moves its entry, and the header's free list moves
/// with it, so a changed file has a new fingerprint.
async fn fingerprint(f: &DatFile, pace: &Pace) -> String {
    let mut h = blake3::Hasher::new();
    h.update(FINGERPRINT_MAGIC.as_bytes());
    let hd = f.header();
    for w in [
        hd.magic,
        hd.block_size,
        hd.file_size,
        hd.data_set,
        hd.data_subset,
        hd.free_head,
        hd.free_tail,
        hd.free_count,
        hd.btree_root,
        hd.master_map_id,
        hd.version_minor,
        f.header_iteration().unwrap_or(0),
    ] {
        h.update(&w.to_le_bytes());
    }
    h.update(&hd.eng_pack_vnum.to_le_bytes());
    h.update(&hd.game_pack_vnum.to_le_bytes());
    h.update(&hd.version_major);
    for (n, (id, e)) in f.iter_entries().enumerate() {
        if n % ENTRIES_PER_TICK == 0 {
            pace.tick().await;
        }
        let mut b = [0u8; 24];
        for (k, w) in [id.0, e.bits, e.offset, e.size, e.date, e.iteration]
            .into_iter()
            .enumerate()
        {
            b[k * 4..k * 4 + 4].copy_from_slice(&w.to_le_bytes());
        }
        h.update(&b);
    }
    h.finalize().to_hex().to_string()
}

/// A file's BLAKE3 hash in hex, read whole (up to its header's file size) a quarter megabyte at a time.
/// `None` when it cannot be read.
async fn content_hash(f: &DatFile, pace: &Pace) -> Option<String> {
    let len = u64::from(f.header().file_size);
    let mut h = blake3::Hasher::new();
    let mut buf = vec![0u8; HASH_CHUNK];
    let mut at = 0u64;
    while at < len {
        pace.tick().await;
        let n = usize::try_from((len - at).min(HASH_CHUNK as u64)).ok()?;
        f.read_raw(at, &mut buf[..n]).ok()?;
        h.update(&buf[..n]);
        at += n as u64;
    }
    Some(h.finalize().to_hex().to_string())
}

/// How much of the work one step of an [`IdentityBuild`] may do. A step always does at least one
/// unit of work: one record or cell compared, a quarter megabyte hashed, one cache line read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Budget {
    /// Until this much time has passed.
    Time(Duration),
    /// This many units of work.
    Units(u32),
    /// All that is left.
    All,
}

/// Where paced work stops for the step: [`Pace::tick`] between two units of work ends the step
/// once its budget is spent, and the work carries on from there at the next.
#[derive(Debug, Default)]
struct Pace(Mutex<PaceState>);

#[derive(Debug, Default, Clone, Copy)]
struct PaceState {
    /// The step ends at this time.
    deadline: Option<web_time::Instant>,
    /// The step ends when this many more units are done.
    units: Option<u32>,
    /// Units of work begun, over every step.
    ticks: u64,
}

impl Pace {
    /// A pace that never stops the work.
    fn unlimited() -> Self {
        Self::default()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, PaceState> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// A new step, with `budget`.
    fn start(&self, budget: Budget) {
        let mut p = self.lock();
        p.deadline = None;
        p.units = None;
        match budget {
            Budget::Time(d) => p.deadline = Some(web_time::Instant::now() + d),
            Budget::Units(n) => p.units = Some(n.max(1)),
            Budget::All => {}
        }
    }

    /// A unit of work is about to begin: whether the step's budget is spent.
    fn spent(&self) -> bool {
        let mut p = self.lock();
        p.ticks += 1;
        if let Some(n) = p.units {
            if n == 0 {
                return true;
            }
            p.units = Some(n - 1);
        }
        p.deadline.is_some_and(|d| web_time::Instant::now() >= d)
    }

    /// Units of work begun so far.
    fn ticks(&self) -> u64 {
        self.lock().ticks
    }

    /// The point between two units of work where a step may end.
    fn tick(&self) -> Tick<'_> {
        Tick {
            pace: self,
            yielded: false,
        }
    }
}

/// [`Pace::tick`]: ready at once while the step's budget lasts; otherwise it ends the step and is
/// ready at the next.
struct Tick<'a> {
    pace: &'a Pace,
    yielded: bool,
}

impl Future for Tick<'_> {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<()> {
        if self.yielded || !self.pace.spent() {
            Poll::Ready(())
        } else {
            self.yielded = true;
            Poll::Pending
        }
    }
}

/// Paced work done all at once: under [`Pace::unlimited`] it never stops, so one poll finishes it.
fn run_to_end<T>(work: impl Future<Output = T>) -> T {
    let mut work = std::pin::pin!(work);
    let mut cx = Context::from_waker(Waker::noop());
    loop {
        if let Poll::Ready(v) = work.as_mut().poll(&mut cx) {
            return v;
        }
    }
}

/// The verdicts for the world's files against the other era's beside them, worked out a step at
/// a time.
///
/// The client starts one when it starts, whatever look is chosen, and gives it a few milliseconds
/// of each frame, so the other era's look is ready before it is asked for and no frame waits on
/// it. The steps are the work [`ObjectIdentity::load_or_build_with`] does, in the same order and
/// with the same results: the files' hashes (a file whose fingerprint was seen before is not read
/// again), then the cache file when it holds them, else the comparison, written to the cache. The
/// browser has no threads, so this is one path everywhere: the work stops between units and
/// carries on at the next step.
pub struct IdentityBuild {
    pace: Arc<Pace>,
    work: Option<IdentityWork>,
    done: Option<(Arc<ObjectIdentity>, IdentitySource)>,
    steps: u64,
}

/// The work of an [`IdentityBuild`], stopped between two units until its next step.
type IdentityWork = Pin<Box<dyn Future<Output = (ObjectIdentity, IdentitySource)> + Send>>;

impl std::fmt::Debug for IdentityBuild {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IdentityBuild")
            .field("steps", &self.steps)
            .field("units", &self.pace.ticks())
            .field("ready", &self.done.is_some())
            .finish_non_exhaustive()
    }
}

impl IdentityBuild {
    /// The build for `store`'s world against the other era's files beside it: its portal, and
    /// the rooms too when that era's cell file is there. `None` when no other era's files are
    /// beside the world. `cache` is the cache folder ([`default_cache_dir`]); `None` keeps
    /// nothing.
    #[must_use]
    pub fn for_store(store: &RetailDatStore, cache: Option<PathBuf>) -> Option<Self> {
        let era = match store.era() {
            ContainerEra::Tod => ContainerEra::PreTod,
            ContainerEra::PreTod => ContainerEra::Tod,
        };
        let look = store.object_files(era)?;
        let interiors = store.interior_files(era);
        Some(Self::new(store.clone(), look, interiors, cache))
    }

    /// The build for `world`'s portal against `look`'s, and `world`'s cell file against
    /// `interiors`' when given.
    #[must_use]
    pub fn new(
        world: RetailDatStore,
        look: RetailDatStore,
        interiors: Option<RetailDatStore>,
        cache: Option<PathBuf>,
    ) -> Self {
        let pace = Arc::new(Pace::default());
        let p = Arc::clone(&pace);
        let work = async move {
            // Worked out over the locked files, so a world's overlay never changes the cache; the
            // overlay's own changes are then pinned to the world's look.
            let (portal, cell) = (world.portal().base(), world.cell().base());
            let files = IdentityFiles {
                world: &portal,
                other: look.portal(),
                cells: interiors.as_ref().map(|i| (&cell, i.cell())),
            };
            let (mut identity, source) = load_or_build(files, cache.as_deref(), &p).await;
            let pinned = identity.pin_to_world(world.portal(), world.cell());
            if pinned > 0 {
                tracing::info!(
                    "object identity: {pinned} verdict(s) keep the world's own look, which its \
                     overlay changes"
                );
            }
            (identity, source)
        };
        Self {
            pace,
            work: Some(Box::pin(work)),
            done: None,
            steps: 0,
        }
    }

    /// Do up to `budget` of the work. The verdicts once they are ready, on this step and every
    /// one after.
    pub fn step(&mut self, budget: Budget) -> Option<Arc<ObjectIdentity>> {
        if let Some(work) = self.work.as_mut() {
            self.pace.start(budget);
            self.steps += 1;
            let mut cx = Context::from_waker(Waker::noop());
            if let Poll::Ready((id, source)) = work.as_mut().poll(&mut cx) {
                self.done = Some((Arc::new(id), source));
                self.work = None;
            }
        }
        self.ready().cloned()
    }

    /// The verdicts, once ready.
    #[must_use]
    pub fn ready(&self) -> Option<&Arc<ObjectIdentity>> {
        self.done.as_ref().map(|(id, _)| id)
    }

    /// Where the verdicts came from, once ready.
    #[must_use]
    pub fn source(&self) -> Option<IdentitySource> {
        self.done.as_ref().map(|(_, s)| *s)
    }

    /// How many steps have done work.
    #[must_use]
    pub fn steps(&self) -> u64 {
        self.steps
    }

    /// How many units of work have been begun.
    #[must_use]
    pub fn units(&self) -> u64 {
        self.pace.ticks()
    }
}

/// Where the client keeps the cache: `object-identity` in the host's cache folder
/// ([`crate::platform::files::cache_dir`]). On the desktop that is
/// `%LOCALAPPDATA%\Dereth\object-identity` on Windows, `~/Library/Caches/Dereth/object-identity`
/// on macOS, and `$XDG_CACHE_HOME/dereth/object-identity` (else
/// `~/.cache/dereth/object-identity`) elsewhere; in the browser, the page's own storage.
#[must_use]
pub fn default_cache_dir() -> Option<PathBuf> {
    Some(host_files::cache_dir()?.join("object-identity"))
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

    /// Behaviour: none (the pacing of long work over steps: a step ends between two units once
    /// its budget is spent, and the next carries on where it stopped).
    #[test]
    fn work_stopped_between_units_carries_on_where_it_stopped() {
        let mut id = ObjectIdentity::default();
        for k in 0..50u32 {
            id.same.insert(0x0100_0000 + k);
        }
        id.rooms.insert(0xA9B4_0100);
        let text = id.to_text();
        let mut cx = Context::from_waker(Waker::noop());
        for budget in [Budget::Units(1), Budget::Time(Duration::ZERO)] {
            let pace = Pace::default();
            let mut work = std::pin::pin!(ObjectIdentity::parse_paced(&text, &pace));
            let mut steps = 0;
            let back = loop {
                pace.start(budget);
                steps += 1;
                if let Poll::Ready(v) = work.as_mut().poll(&mut cx) {
                    break v;
                }
            };
            assert_eq!(back.as_ref(), Some(&id), "{budget:?}");
            // 51 lines after the first, at most two a step.
            assert!(steps >= 26, "{budget:?}: {steps} steps");
            assert_eq!(pace.ticks(), 51, "{budget:?}: one unit a line");
        }
        let pace = Pace::default();
        pace.start(Budget::All);
        let mut work = std::pin::pin!(ObjectIdentity::parse_paced(&text, &pace));
        assert_eq!(work.as_mut().poll(&mut cx), Poll::Ready(Some(id)));
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
