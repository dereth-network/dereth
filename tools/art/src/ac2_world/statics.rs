//! AC2's placed statics in AC1's formats: the fixed things AC2's landblocks place (walls, gates,
//! towers, bridges, forts, ruins, houses' shells, fences, props), each made once as an AC1 model
//! and placed as an AC1 landblock static at AC2's own place.
//!
//! **What is taken:** every entity a landblock places that AC2 draws (its drawn kinds), whose parts
//! resolve to meshes as AC2 places them (its posed bones, its part groups' connection points, its
//! scale). Spawned things (doors, creatures, characters) are not placed by landblocks and are not
//! here. Placements on AC2's placeholder land, left out as sea, are left out with it.
//!
//! **What moves** ([`stance`]): AC2 draws a mesh weighted to bones as stored when nothing plays it,
//! and so it is made. An object whose look has a behavior table plays through a body, posed as AC2
//! poses it: a tree or plant (the swaying table) is made at rest, every bone at its default pose,
//! as AC1's own trees stand still; any other's idle is sampled 30 times a second, and when nothing
//! in it moves it is made in the pose it holds. One that moves is made an AC1 setup whose default
//! animation is that idle, cut where it comes back to its start ([`make_mover`]): a part for each mesh carried whole
//! by a bone, and for each bone of a mesh wholly weighted to single bones; a part that moves has no
//! collision. One whose meshes bend as it moves (a flag, a curtain, a creature) is left out.
//!
//! **Models:** each part's most detailed level, its vertices moved into the entity's frame (its
//! pose, its attachment, its scale) and every part laid into one AC1 model (more where one would
//! pass AC1's vertex limit). Collision is AC2's own: each mesh's physics polygons, a part without
//! them colliding with nothing.
//!
//! **Materials:** each as AC2 makes it ready to draw: its template's own parameters, then its
//! instance's modifiers, then the changes the part's look makes to it (its appearance's), the last
//! diffuse picture and color named winning. Pictures are AC2's own blocks
//! ([`crate::ac2_world::art::Pictures`]), the pictures made for one language from the English file; a
//! picture stored only larger than the limit is taken at its smallest level, and one mostly clear
//! is drawn two-sided, blended by its alpha where AC2 blends rather than tests it, else cut out.
//! Where the diffuse stage shows no picture, or one the install does not hold, AC2 draws white
//! times the material's color, and so the static is drawn in that
//! color; a material whose color is clear is not drawn (an object drawn clear keeps its collision).
//!
//! **Placement:** AC2's origin is AC1's and a meter is a meter, so a static stands at AC2's world
//! position in whichever AC1 block holds it, turned as AC2 turns it, and raised or lowered by as
//! much as the converted ground differs from AC2's where AC2 sets it into the ground
//! ([`crate::ac2_world::buried`]), or at its origin when it is set into none, so it stands on the ground
//! it stood on.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use crate::ac2_look::source::Ac2;
use auberean_animation::body::PartPose;
use auberean_animation::skin::{influences, skin_vertex, Influences};
use auberean_animation::{Body, CachedRecords};
use auberean_assets::landblock::LandblockInfo as Ac2Info;
use auberean_assets::mesh::Mesh;
use auberean_assets::setup::BoneTransform;
use auberean_dat::locate::DatName;
use auberean_world::look::PartLook;
use auberean_world::math::Mat4;
use auberean_world::place::{parts_of, Part, DRAWN_KINDS};
use auberean_world::records::WorldRecords;
use dereth_assets::geometry::{Placement, SetupOrder};
use dereth_assets::world::ObjectEntry;
use dereth_assets::{GfxObj, Polygon, Setup, Sphere, SwVertex, VertexArray};
use dereth_primitives::frame::V3;
use dereth_primitives::{DataId, EraId, Frame, Quat, Vec3};

use crate::ac2_world::buried::{self, Cover, Point};
use crate::ac2_world::effects;

use crate::ac2_world::art::{Ids, Pictures, Records};
use dereth_ac2_convert::surface::{surface_of, Shown};

/// The most levels of detail a static's models can be given (`--levels`). By default they are
/// given one, the most detailed: AC2's coarser levels add about 68 MB to the overlay and drew
/// no faster where measured.
pub const MAX_LEVELS: usize = 3;
/// AC1 models keep at most this many vertices.
const MAX_VERTICES: usize = 30_000;
/// AC2's block side, meters; AC1's.
const AC2_BLOCK: f64 = 160.0;
const AC1_BLOCK: f64 = 192.0;

/// A placed static's model: AC2's entity record (or visual description) and the scale it is made
/// at, in thousandths so the key is exact.
type ModelKey = (u32, [i32; 3]);

/// What the statics' conversion made: per AC1 block, the statics to add; and a line per model.
pub struct Statics {
    pub by_block: BTreeMap<(usize, usize), Vec<ObjectEntry>>,
    pub tsv: String,
    pub placed: usize,
    pub models: usize,
    /// Placements passed over, by why.
    pub left_out: BTreeMap<&'static str, usize>,
    /// Each placed static's box on the ground, world meters: west, south, east, north.
    pub footprints: Vec<(f64, f64, f64, f64)>,
    /// Each placed static's points to cover, and what of it shows on wet squares or past the cap.
    pub covers: Vec<Cover>,
    /// A line per placed static left showing more than the tolerance (past the cap, or on a
    /// square with a water corner).
    pub showing_tsv: String,
    /// Placements whose height was taken from where they are set into the ground, and of those,
    /// how many stand more than half a meter from where their origin alone put them.
    pub set_in: usize,
    pub moved: usize,
    /// How the placed models' materials are drawn: per way, the models with a material drawn
    /// that way and their placements.
    pub surfaces: BTreeMap<&'static str, (usize, usize)>,
    /// A line per placed model's material not drawn with its own picture as AC2 makes it ready.
    pub surfaces_tsv: String,
    /// How the placed models were made: per way, the models and their placements.
    pub made: BTreeMap<&'static str, (usize, usize)>,
    /// A line per placed model that has weights or a behavior table, and how it was made.
    pub made_tsv: String,
    /// A line per object whose placements are left out for what it is (it draws nothing, it
    /// bends as it moves...), and how many.
    pub left_tsv: String,
    /// The placements of objects that draw nothing but an effect, made AC1 objects that play
    /// its particles ([`effects`]), and of drawn objects that play effects too; and a line per
    /// such object and scale, and what it became.
    pub effects: usize,
    pub mesh_effects: usize,
    pub effects_tsv: String,
    /// The statics asked for as buildings' shells ([`crate::ac2_world::rooms`]), made but not placed.
    pub shells: Vec<Shell>,
    /// Every placement's collision as it stands ([`crate::ac2_world::ledges`]), and each model's collision
    /// at a scale of 1 by its AC2 object.
    pub solids: Vec<Solid>,
    pub collisions: BTreeMap<u32, Collision>,
}

/// Where a placed static's collision stands: its model, scale and frame, its AC1 block and its
/// place in the world, whether it is a building's shell, and AC2's block and the entity's index
/// there.
#[derive(Clone, Debug)]
pub struct Solid {
    pub object: u32,
    pub scale: Vec3,
    pub block: (usize, usize),
    /// Its frame in its block, and its origin in world meters.
    pub frame: Frame,
    pub world: (f64, f64, f64),
    pub shell: bool,
    pub ac2: (usize, usize, usize),
}

impl Solid {
    /// Its collision polygons in world meters, `collision` being its model's at a scale of 1.
    #[must_use]
    pub fn polygons(&self, collision: &Collision) -> Vec<Vec<[f64; 3]>> {
        let at = Frame::new(Vec3::ZERO, self.frame.rotation);
        let k = self.scale;
        collision
            .rings
            .iter()
            .map(|r| {
                r.iter()
                    .filter_map(|&i| collision.points.get(usize::from(i)))
                    .map(|p| {
                        let v = dereth_primitives::frame::localtoglobal(
                            &at,
                            Vec3::new(p.x * k.x, p.y * k.y, p.z * k.z),
                        );
                        [
                            self.world.0 + f64::from(v.x),
                            self.world.1 + f64::from(v.y),
                            self.world.2 + f64::from(v.z),
                        ]
                    })
                    .collect()
            })
            .collect()
    }
}

/// A static made to stand as a building's shell: where AC2 places it, and where and how it stands
/// in AC1's world.
#[derive(Clone, Debug)]
pub struct Shell {
    /// AC2's block and the index of the entity among the block's.
    pub ac2: (usize, usize, usize),
    /// The AC1 block it stands in, and its frame there.
    pub block: (usize, usize),
    pub frame: Frame,
    /// Its place in AC2's world, meters: x, y, and its height.
    pub ac2_at: (f64, f64, f64),
    /// Its scale in thousandths, its drawn models and its collision.
    pub scale: [i32; 3],
    pub parts: Vec<DataId>,
    pub collision: Option<Collision>,
}

/// A static's models, made once at its own size: their ids, their box ([`box_of`]), and their
/// counts.
pub(crate) struct Model {
    parts: Vec<DataId>,
    lo: Vec3,
    hi: Vec3,
    /// The farthest any vertex stands from the upright through the origin.
    xy: f32,
    /// Every vertex of its most detailed level, in the entity's frame at scale 1.
    verts: Vec<Vec3>,
    /// Its collision, in the entity's frame at scale 1, and the model made of it at that scale.
    collision: Option<Collision>,
    collision_part: Option<DataId>,
    triangles: usize,
    physics: usize,
    /// Each material its meshes draw with: the AC2 material, the picture list it shows (0 for
    /// none), and how it is drawn ([`Shown`]'s ways).
    shown: std::collections::BTreeSet<(u32, u32, &'static str)>,
    /// How it was made ([`MADE_STILL`] and the others).
    made: &'static str,
    /// How far the pose it was made in stands from the idle's first frame, meters (a tree at
    /// rest; 0 for the others).
    off: f32,
    /// A moving static's frames at 30 a second, each part's frame in the object's; and whether
    /// its idle comes back to its start in them (else they end where it comes nearest).
    frames: Option<Vec<Vec<Frame>>>,
    looped: bool,
}

impl Model {
    /// Whether it plays its idle (its parts move).
    pub(crate) const fn moves(&self) -> bool {
        self.frames.is_some()
    }

    /// Whether it has a collision of its own.
    pub(crate) const fn collides(&self) -> bool {
        self.collision.is_some()
    }

    /// Its drawn models.
    pub(crate) fn parts(&self) -> &[DataId] {
        &self.parts
    }
}

/// The AC2 install's world records, for the placements and their parts.
pub(crate) fn world_records(dir: &Path) -> Result<WorldRecords, String> {
    let open = |n: DatName| {
        let p = n.in_dir(dir);
        auberean_dat::DatFile::open(&p).map_err(|e| format!("{}: {e}", p.display()))
    };
    let portal = open(DatName::Portal)?;
    let cell = open(DatName::Cell(1))?;
    let ctx = auberean_assets::Context::from_portal(&portal)
        .map_err(|e| format!("AC2 property list: {e:?}"))?;
    Ok(WorldRecords::new(portal, cell, None, ctx))
}

/// The AC1 block and block-local point of world point `(x, y)` meters, if it lies in AC1's world.
#[must_use]
pub fn ac1_place(x: f64, y: f64) -> Option<((usize, usize), (f64, f64))> {
    let (bx, by) = ((x / AC1_BLOCK).floor(), (y / AC1_BLOCK).floor());
    if !(0.0..255.0).contains(&bx) || !(0.0..255.0).contains(&by) {
        return None;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let block = (bx as usize, by as usize);
    Some((block, (x - bx * AC1_BLOCK, y - by * AC1_BLOCK)))
}

/// A scale in thousandths, the model key's.
fn milli(s: auberean_primitives::Vec3) -> [i32; 3] {
    #[allow(clippy::cast_possible_truncation)]
    // LINT-OK: a scale in thousandths, a few thousand at most.
    let m = |v: f32| (f64::from(v) * 1000.0).round() as i32;
    [m(s.x), m(s.y), m(s.z)]
}

/// The ground under a world point, as [`place_statics`] asks for it: AC2's (AC2 meters), the
/// converted world's (its meters, rounded heights), whether the converted square under it has a
/// water corner, and that square (the index of its south-west corner).
pub type Under = (f64, f64, bool, (usize, usize));

/// Every static AC2's landblocks place, made and placed. `skip_block` says which AC2 blocks were
/// left out as placeholder land; `under` the ground under a world point (none off AC2's land).
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub fn place_statics(
    ac2_dir: &Path,
    ac2: &Ac2,
    skip_block: &dyn Fn(usize, usize) -> bool,
    under: &dyn Fn(f64, f64) -> Option<Under>,
    shells: &std::collections::BTreeSet<(usize, usize, usize)>,
    pictures: &mut Pictures,
    ids: &mut Ids,
    out: &mut Records,
    max_levels: usize,
) -> Result<Statics, String> {
    let wr = world_records(ac2_dir)?;
    let mut st = Statics {
        by_block: BTreeMap::new(),
        tsv: String::new(),
        placed: 0,
        models: 0,
        left_out: BTreeMap::new(),
        footprints: Vec::new(),
        covers: Vec::new(),
        showing_tsv: String::from("ac2_object\tx\ty\tac1_block\tshows_past_cap\tshows_on_water\n"),
        set_in: 0,
        moved: 0,
        surfaces: BTreeMap::new(),
        surfaces_tsv: String::from("ac2_object\tac2_material\tpicture_list\tdrawn\n"),
        made: BTreeMap::new(),
        made_tsv: String::from(
            "ac2_object\tmade\tparts\tframes\tlooped\trest_from_first_frame_m\tplacements\n",
        ),
        left_tsv: String::from("ac2_object\twhy\tplacements\n"),
        effects: 0,
        mesh_effects: 0,
        effects_tsv: String::from(
            "ac2_object\tscale_milli\tac1_setup\tdraws\tsystems\temitters\tlights\tlooks\tnot_carried\tplacements\n",
        ),
        shells: Vec::new(),
        solids: Vec::new(),
        collisions: BTreeMap::new(),
    };
    let mut left_of: BTreeMap<(u32, &'static str), usize> = BTreeMap::new();
    let mut models: BTreeMap<u32, Result<Model, &'static str>> = BTreeMap::new();
    let mut setups: BTreeMap<ModelKey, (DataId, usize)> = BTreeMap::new();
    // The objects that draw nothing but an effect, by object and scale, and their placements.
    let mut effect_setups: BTreeMap<ModelKey, Result<effects::Made, &'static str>> =
        BTreeMap::new();
    let mut effect_placed: BTreeMap<ModelKey, usize> = BTreeMap::new();
    // The drawn objects that play effects too, by object and scale.
    let mut mesh_effects: BTreeMap<ModelKey, effects::Played> = BTreeMap::new();
    let mut shared = effects::Shared::default();
    let skip = |st: &mut Statics, why: &'static str| *st.left_out.entry(why).or_insert(0) += 1;
    for bx in 0..255usize {
        for by in 0..255usize {
            #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
            let Some(info): Option<Ac2Info> = wr.landblock_info(bx as i32, by as i32) else {
                continue;
            };
            for (index, e) in info.entities.entities.iter().enumerate() {
                if !e.kind.is_some_and(|k| DRAWN_KINDS.contains(&k)) {
                    skip(&mut st, "not a drawn kind");
                    continue;
                }
                let (Some(object), Some(offset)) = (e.object, e.offset.as_ref()) else {
                    skip(&mut st, "no object or no place");
                    continue;
                };
                if skip_block(bx, by) {
                    skip(&mut st, "on placeholder land left out");
                    continue;
                }
                let scale = e
                    .scale
                    .unwrap_or(auberean_primitives::Vec3::new(1.0, 1.0, 1.0));
                if let std::collections::btree_map::Entry::Vacant(slot) = models.entry(object) {
                    slot.insert(make_model(
                        &wr, ac2, object, false, pictures, ids, out, max_levels,
                    )?);
                }
                let model = match models.get(&object) {
                    Some(Ok(model)) => model,
                    Some(Err(why)) if *why == DRAWS_NOTHING || *why == NO_MESH => {
                        // It may still play an effect: its particles as AC1 emitters.
                        let key = (object, milli(scale));
                        if let std::collections::btree_map::Entry::Vacant(slot) =
                            effect_setups.entry(key)
                        {
                            slot.insert(effects::effect_setup(
                                &wr,
                                ac2,
                                object,
                                scale,
                                &mut shared,
                                ids,
                                out,
                            )?);
                        }
                        let made = match effect_setups.get(&key) {
                            Some(Ok(made)) => made,
                            Some(Err(not)) => {
                                skip(&mut st, not);
                                *left_of.entry((object, not)).or_default() += 1;
                                continue;
                            }
                            None => unreachable!("made above"),
                        };
                        let o = offset.frame.origin;
                        let (wx, wy) = (
                            bx as f64 * AC2_BLOCK + f64::from(o.x),
                            by as f64 * AC2_BLOCK + f64::from(o.y),
                        );
                        let Some((block, (lx, ly))) = ac1_place(wx, wy) else {
                            skip(&mut st, "outside AC1's world");
                            continue;
                        };
                        // Where its origin stands above AC2's ground, above the converted ground.
                        let z = under(wx, wy).map_or_else(
                            || crate::ac2_world::table::lower(f64::from(o.z)),
                            |u| f64::from(o.z) + u.1 - u.0,
                        );
                        let r = offset.frame.rotation;
                        #[allow(clippy::cast_possible_truncation)]
                        st.by_block.entry(block).or_default().push(ObjectEntry {
                            id: made.setup,
                            frame: Frame::new(
                                Vec3::new(lx as f32, ly as f32, z as f32),
                                Quat {
                                    w: r.w,
                                    x: r.x,
                                    y: r.y,
                                    z: r.z,
                                },
                            ),
                        });
                        *effect_placed.entry(key).or_default() += 1;
                        st.placed += 1;
                        st.effects += 1;
                        continue;
                    }
                    Some(Err(why)) => {
                        skip(&mut st, why);
                        *left_of.entry((object, why)).or_default() += 1;
                        continue;
                    }
                    None => unreachable!("made above"),
                };
                // A moving static turns its parts whole only at a scale the same along each axis.
                let even = (scale.x - scale.y).abs() < 1e-3 && (scale.x - scale.z).abs() < 1e-3;
                if model.frames.is_some() && !(even && scale.x > 0.0) {
                    skip(&mut st, MOVES_UNEVEN);
                    *left_of.entry((object, MOVES_UNEVEN)).or_default() += 1;
                    continue;
                }
                let footprint = footprint_of(model, scale, offset.frame.rotation);
                let key = (object, milli(scale));
                if let std::collections::btree_map::Entry::Vacant(slot) = setups.entry(key) {
                    // What it plays as well as what it draws: a brazier's flame, a torch's.
                    let played =
                        effects::effects_of(&wr, ac2, object, scale, &mut shared, ids, out)?.ok();
                    let setup = setup_playing(model, key.1, 0.0, played.as_ref(), ids, out)?;
                    if let Some(p) = played {
                        mesh_effects.insert(key, p);
                    }
                    slot.insert((setup, 0));
                }
                let m = setups.get_mut(&key).expect("made above");
                let o = offset.frame.origin;
                let (wx, wy) = (
                    bx as f64 * AC2_BLOCK + f64::from(o.x),
                    by as f64 * AC2_BLOCK + f64::from(o.y),
                );
                let Some((block, (lx, ly))) = ac1_place(wx, wy) else {
                    skip(&mut st, "outside AC1's world");
                    continue;
                };
                let r = offset.frame.rotation;
                let turn = Mat4::from_parts(
                    auberean_primitives::Vec3::new(0.0, 0.0, 0.0),
                    r,
                    auberean_primitives::Vec3::new(1.0, 1.0, 1.0),
                );
                let points: Vec<(Point, (usize, usize))> = model
                    .verts
                    .iter()
                    .filter_map(|v| {
                        let p = turn.point(auberean_primitives::Vec3::new(
                            v.x * scale.x,
                            v.y * scale.y,
                            v.z * scale.z,
                        ));
                        let (x, y) = (wx + f64::from(p.x), wy + f64::from(p.y));
                        let (ac2_ground, converted, wet, square) = under(x, y)?;
                        let z = f64::from(o.z) + f64::from(p.z);
                        Some((
                            Point {
                                z,
                                ac2: ac2_ground,
                                converted,
                                wet,
                            },
                            square,
                        ))
                    })
                    .collect();
                let at_origin = under(wx, wy).map(|u| u.1 - u.0);
                let plain: Vec<Point> = points.iter().map(|p| p.0).collect();
                let shift = match (buried::shift(&plain), at_origin) {
                    (Some(s), origin) => {
                        st.set_in += 1;
                        st.moved += usize::from(origin.is_some_and(|o| (o - s).abs() > 0.5));
                        Some(s)
                    }
                    (None, origin) => origin,
                };
                let z = shift.map_or_else(
                    || crate::ac2_world::table::lower(f64::from(o.z)),
                    |s| f64::from(o.z) + s,
                );
                if let Some(s) = shift {
                    let c = buried::cover(&points, s);
                    if c.past_cap > 0.0 || c.wet > 0.0 {
                        let _ = writeln!(
                            st.showing_tsv,
                            "{object:08X}\t{wx:.1}\t{wy:.1}\t{:02X}{:02X}\t{:.2}\t{:.2}",
                            block.0, block.1, c.past_cap, c.wet
                        );
                    }
                    st.covers.push(c);
                }
                #[allow(clippy::cast_possible_truncation)]
                let entry = ObjectEntry {
                    id: m.0,
                    frame: Frame::new(
                        Vec3::new(lx as f32, ly as f32, z as f32),
                        Quat {
                            w: r.w,
                            x: r.x,
                            y: r.y,
                            z: r.z,
                        },
                    ),
                };
                if let Some(c) = &model.collision {
                    st.collisions.entry(object).or_insert_with(|| c.clone());
                    #[allow(clippy::cast_precision_loss)]
                    st.solids.push(Solid {
                        object,
                        scale: Vec3::new(
                            key.1[0] as f32 / 1000.0,
                            key.1[1] as f32 / 1000.0,
                            key.1[2] as f32 / 1000.0,
                        ),
                        block,
                        frame: entry.frame,
                        world: (wx, wy, f64::from(entry.frame.origin.z)),
                        shell: shells.contains(&(bx, by, index)),
                        ac2: (bx, by, index),
                    });
                }
                if shells.contains(&(bx, by, index)) {
                    st.shells.push(Shell {
                        ac2: (bx, by, index),
                        block,
                        frame: entry.frame,
                        ac2_at: (wx, wy, f64::from(o.z)),
                        scale: key.1,
                        parts: model.parts.clone(),
                        collision: model.collision.clone(),
                    });
                } else {
                    st.by_block.entry(block).or_default().push(entry);
                }
                st.footprints.push((
                    wx + footprint.0,
                    wy + footprint.1,
                    wx + footprint.2,
                    wy + footprint.3,
                ));
                m.1 += 1;
                st.placed += 1;
            }
        }
    }
    let _ = writeln!(
        st.tsv,
        "ac2_object\tscale_milli\tac1_setup\tparts\ttriangles\tphysics_polygons\tplacements"
    );
    st.models = models.values().filter(|m| m.is_ok()).count();
    for ((object, s), n) in &effect_placed {
        let Some(Ok(m)) = effect_setups.get(&(*object, *s)) else {
            continue;
        };
        let _ = writeln!(
            st.effects_tsv,
            "{object:08X}\t{},{},{}\t{:08X}\tnothing\t{}\t{}\t{}\t{}\t{}\t{n}",
            s[0],
            s[1],
            s[2],
            m.setup.raw(),
            m.systems,
            m.emitters,
            m.lights,
            m.look,
            m.lost,
        );
    }
    for (key, p) in &mesh_effects {
        let Some((setup, n)) = setups.get(key) else {
            continue;
        };
        st.mesh_effects += n;
        let s = key.1;
        let _ = writeln!(
            st.effects_tsv,
            "{:08X}\t{},{},{}\t{:08X}\tdrawn\t{}\t{}\t{}\t{}\t{}\t{n}",
            key.0,
            s[0],
            s[1],
            s[2],
            setup.raw(),
            p.systems,
            p.emitters,
            p.lights.len(),
            p.look,
            p.lost,
        );
    }
    for ((object, why), n) in &left_of {
        let _ = writeln!(st.left_tsv, "{object:08X}\t{why}\t{n}");
    }
    let mut placed_of: BTreeMap<u32, usize> = BTreeMap::new();
    for ((object, _), (_, placements)) in &setups {
        *placed_of.entry(*object).or_default() += placements;
    }
    for (object, placements) in &placed_of {
        let Some(Ok(m)) = models.get(object) else {
            continue;
        };
        let e = st.made.entry(m.made).or_default();
        e.0 += 1;
        e.1 += placements;
        if m.made != MADE_STILL {
            let _ = writeln!(
                st.made_tsv,
                "{object:08X}\t{}\t{}\t{}\t{}\t{:.3}\t{placements}",
                m.made,
                m.parts.len(),
                m.frames.as_ref().map_or(0, Vec::len),
                m.looped,
                m.off,
            );
        }
        let ways: std::collections::BTreeSet<&'static str> = m.shown.iter().map(|s| s.2).collect();
        for way in ways {
            let e = st.surfaces.entry(way).or_default();
            e.0 += 1;
            e.1 += placements;
        }
        for (mi, list, way) in &m.shown {
            if *way != Shown::OWN_PICTURE {
                let _ = writeln!(st.surfaces_tsv, "{object:08X}\t{mi:08X}\t{list:08X}\t{way}");
            }
        }
    }
    for ((object, s), (setup, placements)) in &setups {
        let Some(Ok(m)) = models.get(object) else {
            continue;
        };
        let _ = writeln!(
            st.tsv,
            "{object:08X}\t{},{},{}\t{:08X}\t{}\t{}\t{}\t{placements}",
            s[0],
            s[1],
            s[2],
            setup.raw(),
            m.parts.len(),
            m.triangles,
            m.physics,
        );
    }
    Ok(st)
}

/// A model's box on the ground about its origin, at `scale` and turned by `rotation`: west, south,
/// east, north, meters. A model with no corners (a mover none of whose frames place a part) covers
/// its origin alone.
fn footprint_of(
    m: &Model,
    scale: auberean_primitives::Vec3,
    rotation: auberean_primitives::Quat,
) -> (f64, f64, f64, f64) {
    let turn = Mat4::from_parts(
        auberean_primitives::Vec3::new(0.0, 0.0, 0.0),
        rotation,
        auberean_primitives::Vec3::new(1.0, 1.0, 1.0),
    );
    // What only collides covers its origin alone, however far its collision reaches.
    if m.parts.is_empty() {
        return (0.0, 0.0, 0.0, 0.0);
    }
    let mut b = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for (x, y) in [
        (m.lo.x, m.lo.y),
        (m.hi.x, m.lo.y),
        (m.lo.x, m.hi.y),
        (m.hi.x, m.hi.y),
    ] {
        let p = turn.point(auberean_primitives::Vec3::new(
            x * scale.x,
            y * scale.y,
            0.0,
        ));
        let (px, py) = (f64::from(p.x), f64::from(p.y));
        b = (b.0.min(px), b.1.min(py), b.2.max(px), b.3.max(py));
    }
    b
}

/// A piece's triangles joined into AC1's larger flat polygons, the shape unchanged
/// ([`crate::ac2_world::polys`]): its drawn polygons by surface and sides, its physics polygons over its
/// vertices welded by place (physics reads only where the corners are).
fn join_piece(piece: &mut Piece) {
    let pos: Vec<[f64; 3]> = piece
        .vertices
        .iter()
        .map(|v| {
            [
                f64::from(v.position.x),
                f64::from(v.position.y),
                f64::from(v.position.z),
            ]
        })
        .collect();
    // A leaf AC2 gives its own back is drawn from each side once, and a copy of a face is left
    // out ([`crate::ac2_world::polys::unfought`]).
    let sides = crate::ac2_world::polys::unfought(
        &piece
            .polygons
            .iter()
            .map(|p| (p.pos_surface, p.sides_type, p.vertex_ids.as_slice()))
            .collect::<Vec<_>>(),
        &pos,
    );
    let drawn: Vec<((u16, i32), Vec<u16>)> = piece
        .polygons
        .iter()
        .zip(sides)
        .filter_map(|(p, s)| s.map(|s| ((p.pos_surface, s), p.vertex_ids.clone())))
        .collect();
    // Joined, and once more for faces only the joins make alike (a leaf's back cut into other
    // triangles than its front).
    let joined = crate::ac2_world::polys::join_flat(&drawn, &pos);
    let sides = crate::ac2_world::polys::unfought(
        &joined
            .iter()
            .map(|((surface, sides), ring)| (*surface, *sides, ring.as_slice()))
            .collect::<Vec<_>>(),
        &pos,
    );
    piece.polygons = joined
        .into_iter()
        .zip(sides)
        .filter_map(|(((surface, _), ring), s)| s.map(|s| ((surface, s), ring)))
        .enumerate()
        .map(|(k, ((surface, sides), ring))| {
            let n = ring.len();
            Polygon {
                poly_id: i16::try_from(k).unwrap_or(i16::MAX),
                num_pts: u8::try_from(n).unwrap_or(u8::MAX),
                stippling: 1,
                sides_type: sides,
                pos_surface: surface,
                neg_surface: surface,
                vertex_ids: ring,
                pos_uv_indices: Some(vec![0; n]),
                neg_uv_indices: (sides == 1).then(|| vec![0; n]),
            }
        })
        .collect();
    let mut first: BTreeMap<[u32; 3], u16> = BTreeMap::new();
    let weld: Vec<u16> = pos
        .iter()
        .enumerate()
        .map(|(i, p)| {
            #[allow(clippy::cast_possible_truncation)]
            let key = p.map(|v| (v as f32).to_bits());
            *first
                .entry(key)
                .or_insert(u16::try_from(i).unwrap_or(u16::MAX))
        })
        .collect();
    let physics: Vec<(u8, Vec<u16>)> = piece
        .physics
        .iter()
        .map(|p| {
            (
                0,
                p.vertex_ids.iter().map(|&i| weld[usize::from(i)]).collect(),
            )
        })
        .collect();
    piece.physics = crate::ac2_world::polys::join_flat(&physics, &pos)
        .into_iter()
        .enumerate()
        .map(|(k, (_, ring))| Polygon {
            poly_id: i16::try_from(k).unwrap_or(i16::MAX),
            num_pts: u8::try_from(ring.len()).unwrap_or(u8::MAX),
            // No texture coordinates on a physics polygon.
            stippling: 4,
            sides_type: 0,
            pos_surface: 0,
            neg_surface: 0,
            vertex_ids: ring,
            pos_uv_indices: None,
            neg_uv_indices: None,
        })
        .collect();
    // Only the vertices a polygon names are kept: a mesh's array holds its coarser levels' too.
    let mut used = vec![false; piece.vertices.len()];
    for p in piece.polygons.iter().chain(&piece.physics) {
        for &i in &p.vertex_ids {
            used[usize::from(i)] = true;
        }
    }
    let mut new_id = vec![0u16; used.len()];
    let mut kept = Vec::with_capacity(used.len());
    for (i, v) in piece.vertices.iter().enumerate() {
        if used[i] {
            new_id[i] = u16::try_from(kept.len()).unwrap_or(u16::MAX);
            let mut v = v.clone();
            v.id = i16::try_from(kept.len()).unwrap_or(i16::MAX);
            kept.push(v);
        }
    }
    piece.vertices = kept;
    for p in piece.polygons.iter_mut().chain(piece.physics.iter_mut()) {
        for i in &mut p.vertex_ids {
            *i = new_id[usize::from(*i)];
        }
    }
}

/// One model's geometry being gathered: vertices, drawn polygons and physics polygons.
#[derive(Default)]
struct Piece {
    vertices: Vec<SwVertex>,
    polygons: Vec<Polygon>,
    physics: Vec<Polygon>,
    /// Surfaces by the converter's id, in the order first used.
    surfaces: Vec<DataId>,
}

impl Piece {
    fn surface(&mut self, id: DataId) -> u16 {
        let k = self
            .surfaces
            .iter()
            .position(|s| *s == id)
            .unwrap_or_else(|| {
                self.surfaces.push(id);
                self.surfaces.len() - 1
            });
        u16::try_from(k).unwrap_or(0)
    }
}

/// Where a model hands over to a coarser level: where any of its meshes does, nearest first (all
/// but alike taken as one), for a model of at most `levels` levels in all (one: none).
fn hand_overs(from: impl Iterator<Item = f64>, levels: usize) -> Vec<f64> {
    let mut steps: Vec<f64> = from.collect();
    steps.sort_by(f64::total_cmp);
    steps.dedup_by(|b, a| *b - *a <= 0.02 * *b);
    steps.truncate(levels.clamp(1, MAX_LEVELS) - 1);
    steps
}

/// Why a placement's object is not made: no mesh of it draws (none is in the data, or none
/// has a polygon drawn, or every one is past the vertex limit).
pub const DRAWS_NOTHING: &str = "draws nothing: no mesh of it draws";
/// Its look resolves to no mesh (an effect, or a look not read).
pub const NO_MESH: &str = "draws nothing: its look resolves to no mesh";
/// It moves, and its meshes bend as they move (weights blended over bones that move apart: a
/// flag, a curtain, a creature), which AC1's models cannot.
pub const MOVES_BENDING: &str = "moves, bending (blended weights)";
/// It moves, and a moving part grows, shrinks or shears as it moves, which AC1's frames cannot.
pub const MOVES_SCALING: &str = "moves, scaling its parts";
/// It moves, and is placed at a scale not the same along each axis, under which its parts would
/// no longer turn whole.
pub const MOVES_UNEVEN: &str = "moves, placed at an uneven scale";

/// How a made model stands: still as AC2 draws it, or moving.
pub const MADE_STILL: &str = "still: no weights, no behavior";
/// Weighted to bones but with no behavior to play: AC2 draws its meshes as stored.
pub const MADE_STORED: &str = "still: weighted, drawn as stored";
/// A behavior table whose idle moves nothing: in the pose it holds.
pub const MADE_HELD: &str = "still: its idle moves nothing";
/// A tree or plant that sways in AC2, standing at rest, as AC1's own trees do.
pub const MADE_AT_REST: &str = "still: a tree or plant at rest";
/// Rigid parts its idle moves, each an AC1 part playing the idle as the setup's animation.
pub const MADE_MOVING: &str = "moving: rigid parts, its idle as an animation";

/// The behavior table AC2's trees and plants sway by.
const SWAYING: u32 = 0x4400_0002;
/// The frames a second AC1 plays a static's animation at.
const FPS: f32 = 30.0;
/// The longest idle looked at for a loop, frames (32 s).
const MAX_FRAMES: usize = 32 * 30;
/// The shortest loop taken, frames.
const MIN_LOOP: usize = 30;
/// A transform whose every entry is within this of another's is the same (meters for its
/// translation).
const SAME: f32 = 1e-3;
/// A loop closes where both its first two frames come back within this.
const CLOSES: f32 = 2e-3;
/// A vertex wholly on one bone carries at least this share of its weight on it (the orrery's rings
/// carry a few vertices 0.998 on one bone; what is left moves them a few centimeters).
const WHOLLY: f32 = 0.99;

/// Where a part's mesh stands: its vertices carried whole by a transform, or skinned by a
/// palette and then carried.
#[derive(Clone, Debug, PartialEq)]
enum Carry {
    Whole(Mat4),
    Skinned(Vec<Mat4>, Mat4),
}

impl Carry {
    /// A vertex's position and normal (not normalized) as the part stands.
    fn vertex(
        &self,
        inf: Option<&Influences>,
        p: auberean_primitives::Vec3,
        n: auberean_primitives::Vec3,
    ) -> (auberean_primitives::Vec3, auberean_primitives::Vec3) {
        match self {
            Self::Whole(m) => (m.point(p), m.direction(n)),
            Self::Skinned(palette, m) => {
                let none = Influences::default();
                let (p, n) = skin_vertex(p, n, inf.unwrap_or(&none), palette);
                (m.point(p), m.direction(n))
            }
        }
    }

    /// The greatest difference between any entry of its transforms and `other`'s (infinite
    /// when they are not alike).
    fn differs(&self, other: &Self) -> f32 {
        let mats = |a: &Mat4, b: &Mat4| {
            let mut d = 0.0f32;
            for i in 0..4 {
                for j in 0..4 {
                    d = d.max((a.0[i][j] - b.0[i][j]).abs());
                }
            }
            d
        };
        match (self, other) {
            (Self::Whole(a), Self::Whole(b)) => mats(a, b),
            (Self::Skinned(pa, a), Self::Skinned(pb, b)) if pa.len() == pb.len() => pa
                .iter()
                .zip(pb)
                .map(|(x, y)| mats(x, y))
                .fold(mats(a, b), f32::max),
            _ => f32::INFINITY,
        }
    }
}

/// The greatest difference between two frames of parts' carries.
fn frames_differ(a: &[Carry], b: &[Carry]) -> f32 {
    a.iter()
        .zip(b)
        .map(|(x, y)| x.differs(y))
        .fold(0.0, f32::max)
}

/// How a placed object stands, as its parts would be made.
enum Stance {
    /// Still: each part's carry, how it was found still, and how far the pose taken stands from
    /// the idle's first frame (meters; a tree at rest, 0 for the others).
    Still(Vec<Carry>, &'static str, f32),
    /// Moving: each frame at 30 a second, each part's carry; and whether the idle comes back to
    /// its start where the frames end (else they end where it comes nearest).
    Moving(Vec<Vec<Carry>>, bool),
}

/// Each part group's pose with every bone at its default pose, as a body stands when nothing
/// animates it: a bone's transform is its own then its parent's, its skin the same with scales,
/// against the skeleton's rest pose; a group hangs from its parent's bone by its connection.
fn rest_poses(body: &Body) -> Vec<PartPose> {
    let mut out: Vec<PartPose> = Vec::with_capacity(body.parts.len());
    for part in &body.parts {
        let n = part.setup.bones.len();
        let parents: Vec<Option<usize>> = part
            .skeleton
            .as_ref()
            .map_or_else(|| vec![None; n], |s| s.parents.clone());
        let fill = |t: Option<&BoneTransform>, with_scale: bool| {
            t.map_or(Mat4::IDENTITY, |t| {
                let s = if with_scale {
                    t.scale
                } else {
                    auberean_primitives::Vec3::new(1.0, 1.0, 1.0)
                };
                Mat4::from_parts(t.offset, t.rotation, s)
            })
        };
        let mut bones: Vec<Mat4> = Vec::with_capacity(n);
        let mut deform: Vec<Mat4> = Vec::with_capacity(n);
        for b in 0..n {
            let t = part.default_pose.get(b);
            let (plain, scaled) = (fill(t, false), fill(t, true));
            match parents.get(b).copied().flatten() {
                Some(p) if p < b => {
                    bones.push(plain.then(&bones[p]));
                    deform.push(scaled.then(&deform[p]));
                }
                _ => {
                    bones.push(plain);
                    deform.push(scaled);
                }
            }
        }
        let palette = part
            .skeleton
            .as_ref()
            .map_or_else(|| deform.clone(), |s| s.palette(&deform));
        let world = match part.parent {
            Some((pi, bone, cp)) => out.get(pi).map_or(Mat4::IDENTITY, |parent| {
                let pb = parent.bones.get(bone).copied().unwrap_or(Mat4::IDENTITY);
                cp.then(&pb).then(&parent.world)
            }),
            None => Mat4::IDENTITY,
        };
        out.push(PartPose {
            world,
            bones,
            palette,
        });
    }
    out
}

/// Each part's carry in a body's poses, as AC2 poses a placed object's draws: a weighted mesh
/// skinned by its group's palette, any other carried by the bone of its place in its group's
/// setup; then the group's frame and the object's own. A part the body has no group for stands
/// as stored.
fn carries_in(parts: &[Part], weighted: &[bool], body: &Body, poses: &[PartPose]) -> Vec<Carry> {
    parts
        .iter()
        .zip(weighted)
        .map(|(p, &weighted)| {
            let Some(by) = p.played else {
                return Carry::Whole(p.transform);
            };
            let Some(pose) = body
                .parts
                .iter()
                .position(|q| q.key == by.group)
                .and_then(|at| poses.get(at))
            else {
                return Carry::Whole(p.transform);
            };
            let carry = pose.world.then(&by.object);
            if weighted {
                Carry::Skinned(pose.palette.clone(), carry)
            } else {
                let bone = pose.bones.get(by.mesh).copied().unwrap_or(Mat4::IDENTITY);
                Carry::Whole(bone.then(&carry))
            }
        })
        .collect()
}

/// How far apart two carries put a part's mesh: the greatest distance between a vertex's two
/// places, meters.
fn apart(m: &Mesh, inf: Option<&[Influences]>, a: &Carry, b: &Carry) -> f32 {
    let Some(vs) = m.vertices.attributes() else {
        return 0.0;
    };
    let zero = auberean_primitives::Vec3::new(0.0, 0.0, 0.0);
    vs.iter()
        .enumerate()
        .map(|(k, v)| {
            let i = inf.and_then(|i| i.get(k));
            let (p, _) = a.vertex(i, v.position, zero);
            let (q, _) = b.vertex(i, v.position, zero);
            ((p.x - q.x).powi(2) + (p.y - q.y).powi(2) + (p.z - q.z).powi(2)).sqrt()
        })
        .fold(0.0, f32::max)
}

/// How the object whose parts are `parts` (with their meshes) stands. With no behavior to play,
/// as stored, as AC2 draws it (a weighted mesh too); else in its body's poses: a tree or plant at
/// rest; otherwise its idle sampled 30 times a second for up to 32 s, still when nothing in it
/// moves, else moving, cut where it comes back to its start (or nearest to it).
fn stance(wr: &WorldRecords, parts: &[Part], meshes: &[Option<Mesh>]) -> Stance {
    let stored: Vec<Carry> = parts.iter().map(|p| Carry::Whole(p.transform)).collect();
    let weighted: Vec<bool> = meshes
        .iter()
        .map(|m| {
            m.as_ref()
                .is_some_and(|m| influences(&m.vertices).is_some())
        })
        .collect();
    let infs: Vec<Option<Vec<Influences>>> = meshes
        .iter()
        .map(|m| m.as_ref().and_then(|m| influences(&m.vertices)))
        .collect();
    // How far one stance stands from another, over every part.
    let distance = |a: &[Carry], b: &[Carry]| {
        meshes
            .iter()
            .zip(&infs)
            .zip(a.iter().zip(b))
            .filter_map(|((m, i), (x, y))| Some(apart(m.as_ref()?, i.as_deref(), x, y)))
            .fold(0.0, f32::max)
    };
    let Some(played) = parts.iter().find_map(|p| p.played) else {
        let how = if weighted.iter().any(|&w| w) {
            MADE_STORED
        } else {
            MADE_STILL
        };
        return Stance::Still(stored, how, 0.0);
    };
    let records = CachedRecords::new(wr);
    let Some(mut body) = Body::assemble(&records, played.look) else {
        // With no body to play it through, AC2 draws it as stored.
        return Stance::Still(stored, MADE_STORED, 0.0);
    };
    let sways = auberean_world::look::Look::of_record(&records, played.look)
        .is_ok_and(|l| l.behavior_table.0 == SWAYING);
    let first = carries_in(parts, &weighted, &body, &body.pose(&records));
    if sways {
        let rest = carries_in(parts, &weighted, &body, &rest_poses(&body));
        let off = distance(&rest, &first);
        return Stance::Still(rest, MADE_AT_REST, off);
    }
    let mut frames = vec![first];
    for _ in 1..=MAX_FRAMES {
        body.advance(&records, 1.0 / FPS);
        frames.push(carries_in(parts, &weighted, &body, &body.pose(&records)));
    }
    if frames.iter().all(|f| frames_differ(f, &frames[0]) <= SAME) {
        let held = frames.swap_remove(0);
        return Stance::Still(held, MADE_HELD, 0.0);
    }
    let closes = |p: usize| {
        frames_differ(&frames[p], &frames[0]) + frames_differ(&frames[p + 1], &frames[1])
    };
    // The first frame its first two come back at; else the frame they come nearest back at (an
    // idle that jumps where it starts over, as AC2 plays it).
    let (period, looped) = match (MIN_LOOP..MAX_FRAMES - 1).find(|&p| closes(p) <= CLOSES) {
        Some(p) => (p, true),
        None => (
            (MIN_LOOP..MAX_FRAMES - 1)
                .min_by(|&a, &b| closes(a).total_cmp(&closes(b)))
                .unwrap_or(MAX_FRAMES),
            false,
        ),
    };
    frames.truncate(period);
    Stance::Moving(frames, looped)
}

/// A transform split into a scale along each of its own axes, a rotation and a translation
/// (`diag(scale) · R(rotation) · T(origin)`), when its first three rows are at right angles.
fn split_rigid(
    m: &Mat4,
) -> Option<(
    auberean_primitives::Vec3,
    auberean_primitives::Quat,
    auberean_primitives::Vec3,
)> {
    let r = &m.0;
    let len = |k: usize| (r[k][0] * r[k][0] + r[k][1] * r[k][1] + r[k][2] * r[k][2]).sqrt();
    let s = [len(0), len(1), len(2)];
    if s.iter().any(|&v| !v.is_finite() || v <= 1e-6) {
        return None;
    }
    let u: [[f32; 3]; 3] = std::array::from_fn(|k| std::array::from_fn(|j| r[k][j] / s[k]));
    let dot = |a: usize, b: usize| (0..3).map(|j| u[a][j] * u[b][j]).sum::<f32>();
    if dot(0, 1).abs() > 1e-3 || dot(0, 2).abs() > 1e-3 || dot(1, 2).abs() > 1e-3 {
        return None;
    }
    // A mirror is no rotation.
    let det = u[0][0] * (u[1][1] * u[2][2] - u[1][2] * u[2][1])
        - u[0][1] * (u[1][0] * u[2][2] - u[1][2] * u[2][0])
        + u[0][2] * (u[1][0] * u[2][1] - u[1][1] * u[2][0]);
    if det < 0.0 {
        return None;
    }
    Some((
        auberean_primitives::Vec3::new(s[0], s[1], s[2]),
        rotation_of(&u),
        auberean_primitives::Vec3::new(r[3][0], r[3][1], r[3][2]),
    ))
}

/// The unit quaternion of the rotation whose rows are `u` (a point `p` turns to `p · u`).
fn rotation_of(u: &[[f32; 3]; 3]) -> auberean_primitives::Quat {
    let trace = u[0][0] + u[1][1] + u[2][2];
    let (w, x, y, z) = if trace > 0.0 {
        let s = (trace + 1.0).sqrt() * 2.0;
        (
            0.25 * s,
            (u[1][2] - u[2][1]) / s,
            (u[2][0] - u[0][2]) / s,
            (u[0][1] - u[1][0]) / s,
        )
    } else if u[0][0] > u[1][1] && u[0][0] > u[2][2] {
        let s = (1.0 + u[0][0] - u[1][1] - u[2][2]).sqrt() * 2.0;
        (
            (u[1][2] - u[2][1]) / s,
            0.25 * s,
            (u[1][0] + u[0][1]) / s,
            (u[2][0] + u[0][2]) / s,
        )
    } else if u[1][1] > u[2][2] {
        let s = (1.0 + u[1][1] - u[0][0] - u[2][2]).sqrt() * 2.0;
        (
            (u[2][0] - u[0][2]) / s,
            (u[1][0] + u[0][1]) / s,
            0.25 * s,
            (u[2][1] + u[1][2]) / s,
        )
    } else {
        let s = (1.0 + u[2][2] - u[0][0] - u[1][1]).sqrt() * 2.0;
        (
            (u[0][1] - u[1][0]) / s,
            (u[2][0] + u[0][2]) / s,
            (u[2][1] + u[1][2]) / s,
            0.25 * s,
        )
    };
    let n = (w * w + x * x + y * y + z * z).sqrt();
    auberean_primitives::Quat {
        w: w / n,
        x: x / n,
        y: y / n,
        z: z / n,
    }
}

/// AC2's `object` at its own size as AC1 models, or why it is not made. `as_built` makes it as its
/// records store it, as AC2 draws a frill, whatever its look plays.
#[allow(clippy::too_many_arguments)]
pub(crate) fn make_model(
    wr: &WorldRecords,
    ac2: &Ac2,
    object: u32,
    as_built: bool,
    pictures: &mut Pictures,
    ids: &mut Ids,
    out: &mut Records,
    max_levels: usize,
) -> Result<Result<Model, &'static str>, String> {
    let parts = parts_of(
        wr,
        auberean_primitives::DataId(object),
        &Mat4::IDENTITY,
        auberean_primitives::Vec3::new(1.0, 1.0, 1.0),
    );
    if parts.is_empty() {
        return Ok(Err(NO_MESH));
    }
    let meshes: Vec<Option<Mesh>> = parts.iter().map(|p| wr.get::<Mesh>(p.mesh.0)).collect();
    let stance = if as_built {
        let how = if meshes
            .iter()
            .flatten()
            .any(|m| influences(&m.vertices).is_some())
        {
            MADE_STORED
        } else {
            MADE_STILL
        };
        Stance::Still(
            parts.iter().map(|p| Carry::Whole(p.transform)).collect(),
            how,
            0.0,
        )
    } else {
        stance(wr, &parts, &meshes)
    };
    match stance {
        Stance::Still(carries, made, off) => {
            let inputs = parts
                .iter()
                .zip(meshes)
                .zip(carries)
                .filter_map(|((p, m), carry)| {
                    Some(Input {
                        m: m?,
                        carry,
                        only: None,
                        detail: p.detail,
                        look: p.look.clone(),
                        physics: true,
                    })
                })
                .collect();
            let Some(b) = build(ac2, inputs, pictures, ids, out, max_levels)? else {
                return Ok(Err(DRAWS_NOTHING));
            };
            let (lo, hi) = box_of(&b.corners, &b.collision.points);
            let mut verts = b.corners;
            verts.extend(b.collision.points.iter().copied());
            let collision = (!b.collision.rings.is_empty()).then_some(b.collision);
            let collision_part = collision
                .as_ref()
                .map(|c| collision_model(c, Vec3::new(1.0, 1.0, 1.0), ids, out))
                .transpose()?;
            Ok(Ok(Model {
                parts: b.parts,
                lo,
                hi,
                xy: reach(&verts),
                verts,
                collision,
                collision_part,
                triangles: b.triangles,
                physics: b.physics,
                shown: b.shown,
                made,
                off,
                frames: None,
                looped: false,
            }))
        }
        Stance::Moving(frames, looped) => make_mover(
            ac2, &parts, meshes, &frames, looped, pictures, ids, out, max_levels,
        ),
    }
}

/// A moving object as AC1 parts, one for each mesh carried whole by a bone and one for each bone
/// of a mesh whose every vertex is wholly on one bone, each placed by its own frame at every
/// frame of the idle; a part that does not move keeps its collision, one that moves has none.
#[allow(clippy::too_many_arguments)]
fn make_mover(
    ac2: &Ac2,
    parts: &[Part],
    meshes: Vec<Option<Mesh>>,
    frames: &[Vec<Carry>],
    looped: bool,
    pictures: &mut Pictures,
    ids: &mut Ids,
    out: &mut Records,
    max_levels: usize,
) -> Result<Result<Model, &'static str>, String> {
    // Each piece: its part, the vertices it takes (none: all), and its transform at each frame.
    let mut pieces: Vec<(usize, Option<Vec<bool>>, Vec<Mat4>)> = Vec::new();
    for (i, m) in meshes.iter().enumerate() {
        let Some(m) = m else {
            continue;
        };
        match &frames[0][i] {
            Carry::Whole(_) => {
                let at = frames
                    .iter()
                    .map(|f| match &f[i] {
                        Carry::Whole(t) => *t,
                        Carry::Skinned(_, t) => *t,
                    })
                    .collect();
                pieces.push((i, None, at));
            }
            Carry::Skinned(..) => {
                let Some(inf) = influences(&m.vertices) else {
                    continue;
                };
                let mut on: Vec<u8> = Vec::with_capacity(inf.len());
                for v in &inf {
                    match wholly_on(v) {
                        Some(bone) => on.push(bone),
                        None => return Ok(Err(MOVES_BENDING)),
                    }
                }
                let mut bones = on.clone();
                bones.sort_unstable();
                bones.dedup();
                for bone in bones {
                    let at = frames
                        .iter()
                        .map(|f| match &f[i] {
                            Carry::Skinned(palette, t) => palette
                                .get(usize::from(bone))
                                .copied()
                                .unwrap_or(Mat4::IDENTITY)
                                .then(t),
                            Carry::Whole(t) => *t,
                        })
                        .collect();
                    pieces.push((i, Some(on.iter().map(|&b| b == bone).collect()), at));
                }
            }
        }
    }
    let mut model_parts = Vec::new();
    let mut anim: Vec<Vec<Frame>> = vec![Vec::new(); frames.len()];
    let mut verts = Vec::new();
    let mut corners = Vec::new();
    let (mut triangles, mut physics) = (0, 0);
    // The collision of the parts that do not move, where they stand.
    let mut collision = Collision::default();
    let mut shown = std::collections::BTreeSet::new();
    for (i, only, at) in pieces {
        let Some((scale, _, _)) = split_rigid(&at[0]) else {
            return Ok(Err(MOVES_SCALING));
        };
        let mut placed = Vec::with_capacity(at.len());
        for t in &at {
            let Some((s, q, o)) = split_rigid(t) else {
                return Ok(Err(MOVES_SCALING));
            };
            let off = (s.x - scale.x)
                .abs()
                .max((s.y - scale.y).abs())
                .max((s.z - scale.z).abs());
            if off > SAME * scale.x.max(scale.y).max(scale.z) {
                return Ok(Err(MOVES_SCALING));
            }
            placed.push(Frame::new(
                Vec3::new(o.x, o.y, o.z),
                Quat {
                    w: q.w,
                    x: q.x,
                    y: q.y,
                    z: q.z,
                },
            ));
        }
        let still = at
            .iter()
            .all(|t| Carry::Whole(*t).differs(&Carry::Whole(at[0])) <= SAME);
        let Some(m) = meshes[i].clone() else {
            continue;
        };
        let input = Input {
            m,
            carry: Carry::Whole(Mat4::from_parts(
                auberean_primitives::Vec3::new(0.0, 0.0, 0.0),
                auberean_primitives::Quat::IDENTITY,
                scale,
            )),
            only,
            detail: parts[i].detail,
            look: parts[i].look.clone(),
            physics: still,
        };
        let Some(b) = build(ac2, vec![input], pictures, ids, out, max_levels)? else {
            continue;
        };
        let first = Mat4::from_parts(
            auberean_primitives::Vec3::new(
                placed[0].origin.x,
                placed[0].origin.y,
                placed[0].origin.z,
            ),
            auberean_primitives::Quat {
                w: placed[0].rotation.w,
                x: placed[0].rotation.x,
                y: placed[0].rotation.y,
                z: placed[0].rotation.z,
            },
            auberean_primitives::Vec3::new(1.0, 1.0, 1.0),
        );
        let (plo, phi) = box_of(&b.corners, &b.collision.points);
        for f in &placed {
            let t = Mat4::from_parts(
                auberean_primitives::Vec3::new(f.origin.x, f.origin.y, f.origin.z),
                auberean_primitives::Quat {
                    w: f.rotation.w,
                    x: f.rotation.x,
                    y: f.rotation.y,
                    z: f.rotation.z,
                },
                auberean_primitives::Vec3::new(1.0, 1.0, 1.0),
            );
            for k in 0..8u32 {
                let c = auberean_primitives::Vec3::new(
                    if k & 1 == 0 { plo.x } else { phi.x },
                    if k & 2 == 0 { plo.y } else { phi.y },
                    if k & 4 == 0 { plo.z } else { phi.z },
                );
                let p = t.point(c);
                corners.push(Vec3::new(p.x, p.y, p.z));
            }
        }
        verts.extend(b.corners.iter().map(|c| {
            let p = first.point(auberean_primitives::Vec3::new(c.x, c.y, c.z));
            Vec3::new(p.x, p.y, p.z)
        }));
        for id in &b.parts {
            model_parts.push(*id);
            for (k, f) in placed.iter().enumerate() {
                anim[k].push(*f);
            }
        }
        collision.add_carried(&b.collision, &first);
        triangles += b.triangles;
        physics += b.physics;
        shown.extend(b.shown);
    }
    if model_parts.is_empty() {
        return Ok(Err(DRAWS_NOTHING));
    }
    let (lo, hi) = bounds(corners.iter().copied());
    verts.extend(collision.points.iter().copied());
    let collision = (!collision.rings.is_empty()).then_some(collision);
    let collision_part = collision
        .as_ref()
        .map(|c| collision_model(c, Vec3::new(1.0, 1.0, 1.0), ids, out))
        .transpose()?;
    Ok(Ok(Model {
        parts: model_parts,
        lo,
        hi,
        xy: reach(&corners),
        verts,
        collision,
        collision_part,
        triangles,
        physics,
        shown,
        made: MADE_MOVING,
        off: 0.0,
        frames: Some(anim),
        looped,
    }))
}

/// The bone a vertex is wholly on, its influences on that bone together (one bone may be named
/// more than once), when it is.
fn wholly_on(v: &Influences) -> Option<u8> {
    let n = usize::from(v.count).min(v.bones.len());
    let total: f32 = v.weights[..n].iter().sum();
    let on = |bone: u8| -> f32 {
        (0..n)
            .filter(|&k| v.bones[k] == bone)
            .map(|k| v.weights[k])
            .sum()
    };
    let best = (0..n)
        .map(|k| v.bones[k])
        .max_by(|&a, &b| on(a).total_cmp(&on(b)))?;
    (total > 0.0 && on(best) >= WHOLLY * total).then_some(best)
}

/// The box holding every point.
fn bounds(points: impl Iterator<Item = Vec3>) -> (Vec3, Vec3) {
    points.fold(
        (
            Vec3::new(f32::MAX, f32::MAX, f32::MAX),
            Vec3::new(f32::MIN, f32::MIN, f32::MIN),
        ),
        |(lo, hi), p| {
            (
                Vec3::new(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z)),
                Vec3::new(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z)),
            )
        },
    )
}

/// The box of what is `drawn`, or of what is `collided` with where nothing is drawn.
fn box_of(drawn: &[Vec3], collided: &[Vec3]) -> (Vec3, Vec3) {
    let held = if drawn.is_empty() { collided } else { drawn };
    bounds(held.iter().copied())
}

/// The farthest any point stands from the upright through the origin.
fn reach(points: &[Vec3]) -> f32 {
    points
        .iter()
        .map(|p| (p.x * p.x + p.y * p.y).sqrt())
        .fold(0.0, f32::max)
}

/// One mesh to lay into a model: where its vertices stand, the vertices whose polygons it takes
/// (none: all), what its part's look does to it, and whether its collision is kept.
struct Input {
    m: Mesh,
    carry: Carry,
    only: Option<Vec<bool>>,
    detail: Option<[auberean_assets::setup::DetailDistances; 3]>,
    look: Option<std::sync::Arc<PartLook>>,
    physics: bool,
}

/// What laying meshes into AC1 models made.
struct Built {
    parts: Vec<DataId>,
    /// Every vertex of the most detailed level, where the meshes stand.
    corners: Vec<Vec3>,
    /// The meshes' collision, where they stand (none when they have none).
    collision: Collision,
    triangles: usize,
    physics: usize,
    shown: std::collections::BTreeSet<(u32, u32, &'static str)>,
}

/// Meshes laid into AC1 models (more where one would pass AC1's vertex limit), each with its
/// levels of detail; `None` when they draw nothing (and collide with nothing they are drawn clear
/// for).
#[allow(clippy::too_many_lines)]
fn build(
    ac2: &Ac2,
    inputs: Vec<Input>,
    pictures: &mut Pictures,
    ids: &mut Ids,
    out: &mut Records,
    max_levels: usize,
) -> Result<Option<Built>, String> {
    // Each mesh: its vertices in the object's frame, the levels it is drawn at (its most
    // detailed, then the coarser ones AC2 marks for its highest model detail, each with the
    // distance AC2 hands over to it at), and the piece it goes in.
    struct Drawn {
        m: Mesh,
        vs: Vec<SwVertex>,
        levels: Vec<usize>,
        from: Vec<f64>,
        piece: usize,
        /// What its part's look does to its materials.
        look: Option<std::sync::Arc<PartLook>>,
        only: Option<Vec<bool>>,
        physics: bool,
    }
    let mut drawn: Vec<Drawn> = Vec::new();
    let mut sizes: Vec<usize> = vec![0];
    for Input {
        m,
        carry,
        only,
        detail,
        look,
        physics: keeps_physics,
    } in inputs
    {
        let Some(vs) = m.vertices.attributes() else {
            continue;
        };
        let inf = influences(&m.vertices);
        let total = |l: &auberean_assets::mesh::DetailLevel| {
            l.fragments.iter().map(|f| f.indices.len()).sum::<usize>()
        };
        let best = (0..m.levels.len()).max_by_key(|&k| (total(&m.levels[k]), usize::MAX - k));
        let has_drawn = best.is_some_and(|k| total(&m.levels[k]) > 0);
        if (!has_drawn && m.physics_polygons.is_empty()) || vs.len() > MAX_VERTICES {
            continue;
        }
        let bit = 1u32 << 2;
        let mut marked: Vec<usize> = (0..m.levels.len())
            .filter(|&k| m.levels[k].detail & bit != 0)
            .collect();
        if marked.is_empty() {
            marked = (0..m.levels.len()).collect();
        }
        let at_best = best
            .and_then(|b| marked.iter().position(|&k| k == b))
            .unwrap_or(0);
        let distances = detail.map_or_else(Vec::new, |d| {
            crate::ac2_look::source::level_distances(&d[2], marked.len())
        });
        let mut levels: Vec<usize> = best.into_iter().collect();
        let mut from = Vec::new();
        for (i, &k) in marked.iter().enumerate().skip(at_best + 1) {
            if let Some(&d) = distances.get(i - 1) {
                if total(&m.levels[k]) > 0 && d.is_finite() && d > 0.0 {
                    levels.push(k);
                    from.push(d);
                }
            }
        }
        if *sizes.last().unwrap_or(&0) + vs.len() > MAX_VERTICES && *sizes.last().unwrap_or(&0) > 0
        {
            sizes.push(0);
        }
        let piece = sizes.len() - 1;
        *sizes.last_mut().expect("a piece") += vs.len();
        let up = auberean_primitives::Vec3::new(0.0, 0.0, 1.0);
        let vs = vs
            .iter()
            .enumerate()
            .map(|(k, v)| {
                let (p, n) = carry.vertex(
                    inf.as_ref().and_then(|i| i.get(k)),
                    v.position,
                    v.normal.unwrap_or(up),
                );
                let n = auberean_world::math::normalize(n);
                let n = if n.x.is_finite() && (n.x != 0.0 || n.y != 0.0 || n.z != 0.0) {
                    n
                } else {
                    up
                };
                let uv = v.uv.unwrap_or([0.0, 0.0]);
                SwVertex {
                    id: i16::try_from(k).unwrap_or(0),
                    position: Vec3::new(p.x, p.y, p.z),
                    normal: Vec3::new(n.x, n.y, n.z),
                    uvs: vec![(uv[0], uv[1])],
                }
            })
            .collect();
        drawn.push(Drawn {
            m,
            vs,
            levels,
            from,
            piece,
            look,
            only,
            physics: keeps_physics,
        });
    }
    // The model's levels: at each, every mesh at the coarsest of its levels handed over to by then.
    let steps = hand_overs(
        drawn.iter().flat_map(|d| d.from.iter().copied()),
        max_levels,
    );
    let mut surfaces = BTreeMap::new();
    let mut shown = std::collections::BTreeSet::new();
    let (mut triangles, mut physics) = (0, 0);
    // Per piece, per level: its geometry.
    let mut levels: Vec<Vec<Piece>> = Vec::new();
    for level in 0..=steps.len() {
        let mut pieces: Vec<Piece> = (0..sizes.len()).map(|_| Piece::default()).collect();
        for d in &drawn {
            let piece = &mut pieces[d.piece];
            let base = piece.vertices.len();
            piece.vertices.extend(d.vs.iter().enumerate().map(|(k, v)| {
                let mut v = v.clone();
                v.id = i16::try_from(base + k).unwrap_or(0);
                v
            }));
            let id_of = |i: u16| u16::try_from(base + usize::from(i)).ok();
            // A polygon is taken when every vertex it names is one taken.
            let taken = |ring: &[u16]| {
                d.only.as_ref().is_none_or(|o| {
                    ring.iter()
                        .all(|&i| o.get(usize::from(i)).copied().unwrap_or(false))
                })
            };
            let at = if level == 0 {
                0
            } else {
                d.from
                    .iter()
                    .filter(|&&f| f <= steps[level - 1] * 1.02)
                    .count()
            };
            if let Some(&k) = d.levels.get(at) {
                for f in &d.m.levels[k].fragments {
                    let Some(&mi) = d.m.materials.get(f.material as usize) else {
                        continue;
                    };
                    if !f.indices.as_chunks::<3>().0.iter().any(|t| taken(t)) {
                        continue;
                    }
                    let (made, list, way) = surface_of(
                        ac2,
                        mi.0,
                        d.m.kind,
                        d.look.as_deref(),
                        pictures,
                        ids,
                        out,
                        &mut surfaces,
                    )?;
                    if level == 0 {
                        shown.insert((mi.0, list, way));
                    }
                    // A material AC2 draws clear draws nothing here either.
                    let Some((surface, both)) = made else {
                        continue;
                    };
                    let s = piece.surface(surface);
                    for tri in f.indices.as_chunks::<3>().0 {
                        if !taken(tri) {
                            continue;
                        }
                        let Some(v) = tri.iter().map(|&i| id_of(i)).collect::<Option<Vec<u16>>>()
                        else {
                            continue;
                        };
                        if usize::from(*v.iter().max().unwrap_or(&0)) >= piece.vertices.len() {
                            continue;
                        }
                        piece.polygons.push(Polygon {
                            poly_id: i16::try_from(piece.polygons.len()).unwrap_or(i16::MAX),
                            num_pts: 3,
                            stippling: 1,
                            sides_type: i32::from(both),
                            pos_surface: s,
                            neg_surface: s,
                            vertex_ids: v,
                            pos_uv_indices: Some(vec![0; 3]),
                            neg_uv_indices: both.then(|| vec![0; 3]),
                        });
                        triangles += usize::from(level == 0);
                    }
                }
            }
            // Collision is the most detailed level's alone.
            if level > 0 || !d.physics {
                continue;
            }
            for p in &d.m.physics_polygons {
                if !taken(&p.vertices) {
                    continue;
                }
                let Some(v) = p
                    .vertices
                    .iter()
                    .map(|&i| id_of(i))
                    .collect::<Option<Vec<u16>>>()
                else {
                    continue;
                };
                if v.len() < 3 || usize::from(*v.iter().max().unwrap_or(&0)) >= piece.vertices.len()
                {
                    continue;
                }
                piece.physics.push(Polygon {
                    poly_id: i16::try_from(piece.physics.len()).unwrap_or(i16::MAX),
                    num_pts: u8::try_from(v.len()).unwrap_or(u8::MAX),
                    // No texture coordinates on a physics polygon.
                    stippling: 4,
                    sides_type: 0,
                    pos_surface: 0,
                    neg_surface: 0,
                    vertex_ids: v,
                    pos_uv_indices: None,
                    neg_uv_indices: None,
                });
                physics += 1;
            }
        }
        levels.push(pieces);
    }
    let keep: Vec<usize> = (0..sizes.len())
        .filter(|&p| !levels[0][p].polygons.is_empty() || !levels[0][p].physics.is_empty())
        .collect();
    // A model that draws nothing is left out, unless AC2 draws it clear and it collides: its
    // collision stays, unseen, as AC2's does.
    let clear = shown.iter().any(|s| s.2 == Shown::CLEAR);
    if keep.iter().all(|&p| levels[0][p].polygons.is_empty()) && !(clear && physics > 0) {
        return Ok(None);
    }
    // Each piece as an AC1 model, its coarser levels as the models its detail-level record names.
    let mut part_ids = Vec::with_capacity(keep.len());
    let mut corners: Vec<Vec3> = Vec::new();
    // Each piece's collision, gathered into a model of its own below.
    let mut collision = Collision::default();
    for &p in &keep {
        collision.take(&mut levels[0][p]);
        if levels[0][p].polygons.is_empty() {
            continue;
        }
        let degrade = (!steps.is_empty()).then(|| ids.next(0x11));
        let mut level_ids = Vec::with_capacity(levels.len());
        for (level, pieces) in levels.iter_mut().enumerate() {
            let mut piece = std::mem::take(&mut pieces[p]);
            join_piece(&mut piece);
            let va = VertexArray {
                vertex_type: 1,
                vertices: piece.vertices,
            };
            if level == 0 {
                corners.extend(va.vertices.iter().map(|v| v.position));
            }
            let drawing = dereth_builder::bsp::drawing(&va, &piece.polygons);
            let sort_center = drawing
                .nodes
                .first()
                .and_then(|n| n.sphere)
                .map_or(Vec3::ZERO, |s| s.center);
            let id = ids.next(0x01);
            let named = if level == 0 { degrade } else { None };
            let g = GfxObj {
                id,
                flags: 2 | if named.is_some() { 8 } else { 0 },
                surfaces: piece.surfaces,
                vertex_array: va,
                physics_polygons: Vec::new(),
                physics_bsp: None,
                sort_center,
                polygons: piece.polygons,
                drawing_bsp: Some(drawing),
                did_degrade: named,
            };
            out.add(
                id,
                g.encode_for(EraId::Eor)
                    .map_err(|e| format!("{id:?}: {e}"))?,
            );
            level_ids.push(id);
        }
        if let Some(degrade) = degrade {
            // Each level drawn out to where the next takes over; the last out to any distance.
            #[allow(clippy::cast_possible_truncation)]
            let degrades = level_ids
                .iter()
                .enumerate()
                .map(|(k, &gfxobj_id)| {
                    let d = steps.get(k).map_or(f32::MAX, |&d| d as f32);
                    dereth_assets::motion::GfxObjInfo {
                        gfxobj_id,
                        degrade_mode: 1,
                        min_dist: d,
                        ideal_dist: d,
                        max_dist: d,
                    }
                })
                .collect();
            let info = dereth_assets::motion::GfxObjDegradeInfo {
                id: degrade,
                degrades,
            };
            out.add(
                degrade,
                info.encode_for(EraId::Eor)
                    .map_err(|e| format!("{degrade:?}: {e}"))?,
            );
        }
        part_ids.push(level_ids[0]);
    }
    Ok(Some(Built {
        parts: part_ids,
        corners,
        collision,
        triangles,
        physics,
        shown,
    }))
}

/// A static's collision: the corners its physics polygons name, in the entity's frame at scale 1,
/// and each polygon's ring of them.
#[derive(Clone, Debug, Default)]
pub(crate) struct Collision {
    pub(crate) points: Vec<Vec3>,
    pub(crate) rings: Vec<Vec<u16>>,
}

impl Collision {
    /// Take `piece`'s physics polygons, with the corners they name.
    fn take(&mut self, piece: &mut Piece) {
        let mut at: BTreeMap<u16, u16> = BTreeMap::new();
        for poly in std::mem::take(&mut piece.physics) {
            let mut ring = Vec::with_capacity(poly.vertex_ids.len());
            for &i in &poly.vertex_ids {
                let Some(v) = piece.vertices.get(usize::from(i)) else {
                    continue;
                };
                let k = *at.entry(i).or_insert_with(|| {
                    self.points.push(v.position);
                    u16::try_from(self.points.len() - 1).unwrap_or(u16::MAX)
                });
                ring.push(k);
            }
            if ring.len() >= 3 {
                self.rings.push(ring);
            }
        }
    }

    /// Add `other`'s polygons, each corner carried by `by`.
    fn add_carried(&mut self, other: &Self, by: &Mat4) {
        let base = self.points.len();
        self.points.extend(other.points.iter().map(|c| {
            let p = by.point(auberean_primitives::Vec3::new(c.x, c.y, c.z));
            Vec3::new(p.x, p.y, p.z)
        }));
        self.rings.extend(other.rings.iter().map(|r| {
            r.iter()
                .map(|&i| u16::try_from(base + usize::from(i)).unwrap_or(u16::MAX))
                .collect()
        }));
    }
}

/// A model of `c` alone at scale `k`: physics polygons and their tree, nothing drawn.
fn collision_model(
    c: &Collision,
    k: Vec3,
    ids: &mut Ids,
    out: &mut Records,
) -> Result<DataId, String> {
    let pos: Vec<[f64; 3]> = c
        .points
        .iter()
        .map(|p| {
            [
                f64::from(p.x * k.x),
                f64::from(p.y * k.y),
                f64::from(p.z * k.z),
            ]
        })
        .collect();
    let mut first: BTreeMap<[u32; 3], u16> = BTreeMap::new();
    let weld: Vec<u16> = pos
        .iter()
        .enumerate()
        .map(|(i, p)| {
            #[allow(clippy::cast_possible_truncation)]
            let key = p.map(|v| (v as f32).to_bits());
            *first
                .entry(key)
                .or_insert(u16::try_from(i).unwrap_or(u16::MAX))
        })
        .collect();
    let rings: Vec<(u8, Vec<u16>)> = c
        .rings
        .iter()
        .map(|r| (0, r.iter().map(|&i| weld[usize::from(i)]).collect()))
        .collect();
    let joined = crate::ac2_world::polys::join_flat(&rings, &pos);
    // Only the corners a polygon names, numbered afresh.
    let mut new_id: BTreeMap<u16, u16> = BTreeMap::new();
    let mut vertices: Vec<SwVertex> = Vec::new();
    let mut polygons = Vec::with_capacity(joined.len());
    for (n, (_, ring)) in joined.into_iter().enumerate() {
        let mut ids_of = Vec::with_capacity(ring.len());
        for &i in &ring {
            let id = *new_id.entry(i).or_insert_with(|| {
                let p = pos[usize::from(i)];
                let at = vertices.len();
                #[allow(clippy::cast_possible_truncation)]
                vertices.push(SwVertex {
                    id: i16::try_from(at).unwrap_or(i16::MAX),
                    position: Vec3::new(p[0] as f32, p[1] as f32, p[2] as f32),
                    normal: Vec3::new(0.0, 0.0, 1.0),
                    uvs: Vec::new(),
                });
                u16::try_from(at).unwrap_or(u16::MAX)
            });
            ids_of.push(id);
        }
        polygons.push(Polygon {
            poly_id: i16::try_from(n).unwrap_or(i16::MAX),
            num_pts: u8::try_from(ids_of.len()).unwrap_or(u8::MAX),
            // No texture coordinates on a physics polygon.
            stippling: 4,
            sides_type: 0,
            pos_surface: 0,
            neg_surface: 0,
            vertex_ids: ids_of,
            pos_uv_indices: None,
            neg_uv_indices: None,
        });
    }
    let va = VertexArray {
        vertex_type: 1,
        vertices,
    };
    let physics_bsp = dereth_builder::bsp::physics(&va, &polygons, true);
    let id = ids.next(0x01);
    let g = GfxObj {
        id,
        flags: 1,
        surfaces: Vec::new(),
        vertex_array: va,
        physics_polygons: polygons,
        physics_bsp: Some(physics_bsp),
        sort_center: Vec3::ZERO,
        polygons: Vec::new(),
        drawing_bsp: None,
        did_degrade: None,
    };
    out.add(
        id,
        g.encode_for(EraId::Eor)
            .map_err(|e| format!("{id:?}: {e}"))?,
    );
    Ok(id)
}

/// Write `setup`.
///
/// # Errors
/// A height, radius or sphere of it that is not finite, or a record that will not encode.
pub(crate) fn add_setup(setup: &Setup, out: &mut Records) -> Result<(), String> {
    let id = setup.id;
    let (sorting, selection) = (setup.sorting_sphere, setup.selection_sphere);
    let finite = [sorting, selection].iter().all(|s| {
        let c = s.center;
        [c.x, c.y, c.z, s.radius].iter().all(|v| v.is_finite())
    }) && setup.height.is_finite()
        && setup.radius.is_finite();
    if !finite {
        return Err(format!(
            "{id:?}: a bound that is not finite (height {}, radius {}, {sorting:?}, {selection:?})",
            setup.height, setup.radius
        ));
    }
    out.add(
        id,
        setup
            .encode_for(EraId::Eor)
            .map_err(|e| format!("{id:?}: {e}"))?,
    );
    Ok(())
}

/// A setup that only collides: `c`'s polygons (corners about its origin, unturned), drawing
/// nothing ([`crate::ac2_world::ledges`]' ramps).
pub(crate) fn collision_setup(
    c: &Collision,
    ids: &mut Ids,
    out: &mut Records,
) -> Result<DataId, String> {
    let part = collision_model(c, Vec3::new(1.0, 1.0, 1.0), ids, out)?;
    let (lo, hi) = bounds(c.points.iter().copied());
    let center = lo.add(hi).mul(0.5);
    let sphere = Sphere {
        center,
        radius: hi.sub(lo).dot(hi.sub(lo)).sqrt() * 0.5,
    };
    let setup_id = ids.next(0x02);
    let setup = Setup {
        id: setup_id,
        flags: 4 | 8,
        parent_index: None,
        default_scale: None,
        allow_free_heading: true,
        has_physics_bsp: true,
        holding_locations: BTreeMap::new(),
        connection_points: BTreeMap::new(),
        placement_frames: BTreeMap::from([(
            0,
            Placement {
                frames: vec![Frame::new(Vec3::ZERO, Quat::IDENTITY)],
                hooks: Vec::new(),
            },
        )]),
        parts: vec![part],
        cylspheres: Vec::new(),
        spheres: Vec::new(),
        height: hi.z.max(0.0),
        radius: reach(&c.points),
        step_up_height: 0.0,
        step_down_height: 0.0,
        sorting_sphere: sphere,
        selection_sphere: sphere,
        lights: BTreeMap::new(),
        default_anim_id: DataId(0),
        default_script_id: DataId(0),
        default_mtable_id: DataId(0),
        default_stable_id: DataId(0),
        default_phstable_id: DataId(0),
        written_order: SetupOrder::default(),
    };
    add_setup(&setup, out)?;
    Ok(setup_id)
}

/// A static's AC1 setup at `scale`: its models, each scaled by the setup when the scale is not 1,
/// and raised `lift` meters (at the setup's own size) above its origin. A moving static's scale is
/// the same along each axis; its parts' frames at each frame of its idle are its animation, their
/// places scaled with it, and the first its resting placement.
pub(crate) fn setup_for(
    m: &Model,
    scale: [i32; 3],
    lift: f32,
    ids: &mut Ids,
    out: &mut Records,
) -> Result<DataId, String> {
    setup_playing(m, scale, lift, None, ids, out)
}

/// [`setup_for`], its object playing `played` as well: its default script starting the effects'
/// emitters beside its drawn parts, and the effects' lights its own.
pub(crate) fn setup_playing(
    m: &Model,
    scale: [i32; 3],
    lift: f32,
    played: Option<&effects::Played>,
    ids: &mut Ids,
    out: &mut Records,
) -> Result<DataId, String> {
    #[allow(clippy::cast_precision_loss)]
    let k = Vec3::new(
        scale[0] as f32 / 1000.0,
        scale[1] as f32 / 1000.0,
        scale[2] as f32 / 1000.0,
    );
    let unit = scale == [1000, 1000, 1000];
    let (lo, hi) = (
        Vec3::new(m.lo.x * k.x, m.lo.y * k.y, m.lo.z * k.z + lift),
        Vec3::new(m.hi.x * k.x, m.hi.y * k.y, m.hi.z * k.z + lift),
    );
    let center = lo.add(hi).mul(0.5);
    let sphere = Sphere {
        center,
        radius: hi.sub(lo).dot(hi.sub(lo)).sqrt() * 0.5,
    };
    // Its collision is a part of its own, made at the static's size: AC1 files a static into the
    // cells its parts' boxes reach as they are at a scale of 1, and scales its collision by the
    // scale's height alone, so a collision scaled by the setup would be met only near its middle,
    // and wrongly where the scale differs between axes.
    let collision = match (&m.collision, unit) {
        (None, _) => None,
        (Some(_), true) => m.collision_part,
        (Some(c), false) => Some(collision_model(c, k, ids, out)?),
    };
    let mut parts = m.parts.clone();
    parts.extend(collision);
    // A moving static's frames, the collision part standing where it was made at each.
    let frames: Option<Vec<Vec<Frame>>> = m.frames.as_ref().map(|frames| {
        frames
            .iter()
            .map(|f| {
                f.iter()
                    .map(|p| {
                        Frame::new(
                            Vec3::new(p.origin.x * k.x, p.origin.y * k.y, p.origin.z * k.z + lift),
                            p.rotation,
                        )
                    })
                    .chain(collision.map(|_| Frame::new(Vec3::new(0.0, 0.0, lift), Quat::IDENTITY)))
                    .collect()
            })
            .collect()
    });
    let anim_id = match &frames {
        Some(frames) => {
            let id = ids.next(0x03);
            let anim = dereth_assets::Animation {
                id,
                flags: 0,
                num_parts: u32::try_from(parts.len()).unwrap_or(0),
                num_frames: u32::try_from(frames.len()).unwrap_or(0),
                has_hooks: false,
                pos_frames: None,
                part_frames: frames
                    .iter()
                    .map(|f| dereth_assets::geometry::AnimFrame {
                        frames: f.clone(),
                        hooks: Vec::new(),
                    })
                    .collect(),
            };
            out.add(
                id,
                anim.encode_for(EraId::Eor)
                    .map_err(|e| format!("{id:?}: {e}"))?,
            );
            id
        }
        None => DataId(0),
    };
    let resting = frames
        .as_ref()
        .and_then(|f| f.first().cloned())
        .unwrap_or_else(|| {
            vec![Frame::new(Vec3::new(0.0, 0.0, lift), Quat::IDENTITY); parts.len()]
        });
    let setup_id = ids.next(0x02);
    let setup = Setup {
        id: setup_id,
        flags: 4 | if unit { 0 } else { 2 } | if collision.is_some() { 8 } else { 0 },
        parent_index: None,
        default_scale: (!unit).then(|| {
            let mut s = vec![k; m.parts.len()];
            s.extend(collision.map(|_| Vec3::new(1.0, 1.0, 1.0)));
            s
        }),
        allow_free_heading: true,
        has_physics_bsp: collision.is_some(),
        holding_locations: BTreeMap::new(),
        connection_points: BTreeMap::new(),
        placement_frames: BTreeMap::from([(
            0,
            Placement {
                frames: resting,
                hooks: Vec::new(),
            },
        )]),
        parts,
        cylspheres: Vec::new(),
        spheres: Vec::new(),
        height: hi.z.max(0.0),
        radius: m.xy * k.x.abs().max(k.y.abs()),
        step_up_height: 0.0,
        step_down_height: 0.0,
        sorting_sphere: sphere,
        selection_sphere: sphere,
        lights: played.map(|p| p.lights.clone()).unwrap_or_default(),
        default_anim_id: anim_id,
        default_script_id: played.and_then(|p| p.script).unwrap_or(DataId(0)),
        default_mtable_id: DataId(0),
        default_stable_id: DataId(0),
        default_phstable_id: DataId(0),
        written_order: SetupOrder::default(),
    };
    add_setup(&setup, out)?;
    Ok(setup_id)
}

#[cfg(test)]
mod tests {
    //! Behavior: none (tooling: the AC2 world's statics)
    use super::*;

    #[test]
    fn a_world_point_falls_in_the_ac1_block_holding_it() {
        assert_eq!(ac1_place(0.0, 0.0), Some(((0, 0), (0.0, 0.0))));
        assert_eq!(ac1_place(200.0, 400.0), Some(((1, 2), (8.0, 16.0))));
        assert_eq!(ac1_place(-1.0, 5.0), None);
        assert_eq!(ac1_place(255.0 * 192.0, 5.0), None);
    }

    #[test]
    fn a_model_hands_over_to_no_coarser_level_by_default_and_up_to_two_when_asked() {
        let from = [40.0, 12.0, 40.5, 90.0, 200.0];
        assert!(hand_overs(from.into_iter(), 1).is_empty());
        assert_eq!(hand_overs(from.into_iter(), MAX_LEVELS), vec![12.0, 40.0]);
        assert_eq!(hand_overs(from.into_iter(), 2), vec![12.0]);
    }

    #[test]
    fn a_moving_statics_animation_carries_its_collision_part_still_and_its_places_scaled() {
        let (mut ids, mut out) = (Ids::default(), Records::default());
        let c = wall();
        let unit_part = collision_model(&c, Vec3::new(1.0, 1.0, 1.0), &mut ids, &mut out).unwrap();
        let turned = Quat {
            w: 0.0,
            x: 0.0,
            y: 0.0,
            z: 1.0,
        };
        let m = Model {
            parts: vec![DataId(0x0100_0001)],
            lo: Vec3::new(-1.0, -1.0, 0.0),
            hi: Vec3::new(1.0, 1.0, 2.0),
            xy: 1.5,
            verts: Vec::new(),
            collision: Some(c),
            collision_part: Some(unit_part),
            triangles: 2,
            physics: 2,
            shown: std::collections::BTreeSet::new(),
            made: MADE_MOVING,
            off: 0.0,
            frames: Some(vec![
                vec![Frame::new(Vec3::new(0.0, 0.0, 3.0), Quat::IDENTITY)],
                vec![Frame::new(Vec3::new(0.0, 0.0, 3.0), turned)],
            ]),
            looped: true,
        };
        use dereth_assets::Decode;
        let id = setup_for(&m, [2000, 2000, 2000], 0.0, &mut ids, &mut out).unwrap();
        let s = Setup::decode_payload(id, &out.0[&id]).expect("a setup");
        assert_eq!(s.parts.len(), 2, "the drawn part and its collision");
        assert_eq!(
            s.placement_frames[&0].frames[0].origin,
            Vec3::new(0.0, 0.0, 6.0)
        );
        let a = s.default_anim_id;
        let anim = dereth_assets::Animation::decode_payload(a, &out.0[&a]).expect("an animation");
        assert_eq!((anim.num_parts, anim.part_frames.len()), (2, 2));
        let f = &anim.part_frames[1].frames;
        assert_eq!(
            f[0].origin,
            Vec3::new(0.0, 0.0, 6.0),
            "its place scaled with it"
        );
        assert_eq!(f[0].rotation, turned);
        assert_eq!(
            f[1],
            Frame::new(Vec3::ZERO, Quat::IDENTITY),
            "its collision stays"
        );
    }

    /// A wall 2 m wide and 2 m high, standing on the origin, as a static's collision.
    fn wall() -> Collision {
        Collision {
            points: vec![
                Vec3::new(-1.0, 0.0, 0.0),
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(1.0, 0.0, 2.0),
                Vec3::new(-1.0, 0.0, 2.0),
            ],
            rings: vec![vec![0, 1, 2], vec![0, 2, 3]],
        }
    }

    fn decoded(out: &Records, id: DataId) -> GfxObj {
        use dereth_assets::Decode;
        GfxObj::decode_payload(id, &out.0[&id]).expect("a model")
    }

    #[test]
    fn a_statics_collision_is_a_model_of_its_own_made_at_the_statics_size() {
        let (mut ids, mut out) = (Ids::default(), Records::default());
        let id = collision_model(&wall(), Vec3::new(3.7, 3.7, 3.7), &mut ids, &mut out).unwrap();
        let g = decoded(&out, id);
        assert_eq!(g.flags, 1, "collision and nothing drawn");
        assert!(g.polygons.is_empty() && g.physics_bsp.is_some());
        assert_eq!(
            g.physics_polygons.len(),
            1,
            "the wall's two triangles joined"
        );
        let xs: Vec<f32> = g
            .vertex_array
            .vertices
            .iter()
            .map(|v| v.position.x)
            .collect();
        let zs: Vec<f32> = g
            .vertex_array
            .vertices
            .iter()
            .map(|v| v.position.z)
            .collect();
        let span = |v: &[f32]| {
            v.iter().copied().fold(f32::MIN, f32::max) - v.iter().copied().fold(f32::MAX, f32::min)
        };
        assert!((span(&xs) - 7.4).abs() < 1e-4, "{xs:?}");
        assert!((span(&zs) - 7.4).abs() < 1e-4, "{zs:?}");
        // Its box, which AC1 files a static into cells by, is the box at that size.
        let b = g
            .physics_bsp
            .as_ref()
            .and_then(|t| t.nodes.first())
            .and_then(|n| n.sphere);
        assert!(b.is_some_and(|s| s.radius > 4.0), "{b:?}");
    }

    #[test]
    fn a_scaled_statics_setup_scales_its_drawn_parts_and_takes_its_collision_at_its_size() {
        let (mut ids, mut out) = (Ids::default(), Records::default());
        let c = wall();
        let unit_part = collision_model(&c, Vec3::new(1.0, 1.0, 1.0), &mut ids, &mut out).unwrap();
        let m = Model {
            parts: vec![DataId(0x0100_0001)],
            lo: Vec3::new(-1.0, -1.0, 0.0),
            hi: Vec3::new(1.0, 1.0, 2.0),
            xy: 1.5,
            verts: Vec::new(),
            collision: Some(c),
            collision_part: Some(unit_part),
            triangles: 2,
            physics: 2,
            shown: std::collections::BTreeSet::new(),
            made: MADE_STILL,
            off: 0.0,
            frames: None,
            looped: false,
        };
        let unit = |id: DataId, out: &Records| {
            use dereth_assets::Decode;
            Setup::decode_payload(id, &out.0[&id]).expect("a setup")
        };
        let at_one = setup_for(&m, [1000, 1000, 1000], 0.0, &mut ids, &mut out).unwrap();
        let s = unit(at_one, &out);
        assert_eq!(s.parts, vec![DataId(0x0100_0001), unit_part]);
        assert!(s.default_scale.is_none() && s.has_physics_bsp);
        let at_more = setup_for(&m, [3700, 3700, 2000], 0.0, &mut ids, &mut out).unwrap();
        let s = unit(at_more, &out);
        assert_eq!(s.parts.len(), 2);
        assert_ne!(s.parts[1], unit_part, "a collision of its own size");
        let scales = s.default_scale.expect("scaled");
        assert_eq!(
            scales[0],
            Vec3::new(3.7, 3.7, 2.0),
            "the drawn part by the setup"
        );
        assert_eq!(
            scales[1],
            Vec3::new(1.0, 1.0, 1.0),
            "the collision at its size already"
        );
        let g = decoded(&out, s.parts[1]);
        let top = g
            .vertex_array
            .vertices
            .iter()
            .map(|v| v.position.z)
            .fold(f32::MIN, f32::max);
        assert!((top - 4.0).abs() < 1e-4, "its own height scale: {top}");
    }

    /// A still model one part wide, with `lo` and `hi` its box.
    fn still(lo: Vec3, hi: Vec3) -> Model {
        Model {
            parts: vec![DataId(0x0100_0001)],
            lo,
            hi,
            xy: 1.0,
            verts: Vec::new(),
            collision: None,
            collision_part: None,
            triangles: 2,
            physics: 0,
            shown: std::collections::BTreeSet::new(),
            made: MADE_STILL,
            off: 0.0,
            frames: None,
            looped: false,
        }
    }

    #[test]
    fn a_lifted_setup_stands_its_parts_and_its_bounds_that_far_over_its_origin() {
        use dereth_assets::Decode;
        let (mut ids, mut out) = (Ids::default(), Records::default());
        let m = still(Vec3::new(-1.0, -1.0, 0.0), Vec3::new(1.0, 1.0, 2.0));
        let id = setup_for(&m, [1000, 1000, 1000], -0.25, &mut ids, &mut out).unwrap();
        let s = Setup::decode_payload(id, &out.0[&id]).expect("a setup");
        assert_eq!(
            s.placement_frames[&0].frames[0].origin,
            Vec3::new(0.0, 0.0, -0.25)
        );
        assert!((s.sorting_sphere.center.z - 0.75).abs() < 1e-6);
        assert!((s.height - 1.75).abs() < 1e-6);
    }

    /// A drawn static that plays an effect too keeps its parts and starts the effect's script
    /// beside them, with the effect's lights as its own; one that plays none has neither.
    #[test]
    fn a_drawn_static_playing_an_effect_starts_its_script_beside_its_parts() {
        use dereth_assets::Decode;
        let (mut ids, mut out) = (Ids::default(), Records::default());
        let m = still(Vec3::new(-1.0, -1.0, 0.0), Vec3::new(1.0, 1.0, 2.0));
        let light = dereth_primitives::records::LightInfo {
            frame: Frame::new(Vec3::new(0.0, 0.0, 1.5), Quat::IDENTITY),
            color_argb: 0xFFFF_8000,
            intensity: 1.0,
            falloff: 6.0,
            cone_angle: 0.0,
        };
        let played = effects::Played {
            script: Some(DataId(0x3300_C000)),
            lights: BTreeMap::from([(0, light)]),
            reach: 1.0,
            systems: 1,
            emitters: 1,
            look: "orange glow".into(),
            lost: "-".into(),
        };
        let id = setup_playing(
            &m,
            [1000, 1000, 1000],
            0.0,
            Some(&played),
            &mut ids,
            &mut out,
        )
        .unwrap();
        let s = Setup::decode_payload(id, &out.0[&id]).expect("a setup");
        assert_eq!(s.parts, m.parts, "its drawn parts");
        assert_eq!(s.default_script_id, DataId(0x3300_C000));
        assert_eq!(s.lights.get(&0), Some(&light));
        let id = setup_for(&m, [1000, 1000, 1000], 0.0, &mut ids, &mut out).unwrap();
        let s = Setup::decode_payload(id, &out.0[&id]).expect("a setup");
        assert_eq!(s.default_script_id, DataId(0));
        assert!(s.lights.is_empty());
    }

    /// A still model that draws nothing and collides as [`wall`] does.
    fn only_collides() -> Model {
        let c = wall();
        let (lo, hi) = box_of(&[], &c.points);
        Model {
            parts: Vec::new(),
            xy: reach(&c.points),
            verts: c.points.clone(),
            collision: Some(c),
            physics: 2,
            ..still(lo, hi)
        }
    }

    #[test]
    fn a_static_that_only_collides_stands_by_its_collisions_box_at_any_scale() {
        use dereth_assets::Decode;
        for (scale, lift) in [(2000, 0.0), (500, 0.0), (1000, 0.0), (3700, -0.25)] {
            let (mut ids, mut out) = (Ids::default(), Records::default());
            let mut m = only_collides();
            let c = wall();
            m.collision_part =
                Some(collision_model(&c, Vec3::new(1.0, 1.0, 1.0), &mut ids, &mut out).unwrap());
            let id = setup_for(&m, [scale; 3], lift, &mut ids, &mut out).unwrap();
            let s = Setup::decode_payload(id, &out.0[&id]).expect("a setup");
            let k = scale as f32 / 1000.0;
            for sphere in [s.sorting_sphere, s.selection_sphere] {
                let (at, r) = (sphere.center, sphere.radius);
                assert!(
                    [at.x, at.y, at.z, r].iter().all(|v| v.is_finite()),
                    "at {scale}: {sphere:?}"
                );
                for p in &c.points {
                    let d = Vec3::new(p.x * k, p.y * k, p.z * k + lift).sub(at);
                    assert!(
                        d.dot(d).sqrt() <= r + 1e-4,
                        "at {scale}: {sphere:?} leaves out {p:?}"
                    );
                }
                assert!(
                    (r - k * 2.0_f32.sqrt()).abs() < 1e-4,
                    "at {scale}: {sphere:?}"
                );
            }
            assert!(
                (s.height - (2.0 * k + lift)).abs() < 1e-4,
                "at {scale}: {}",
                s.height
            );
        }
    }

    #[test]
    fn a_setup_whose_height_radius_or_sphere_is_not_finite_is_refused_and_not_written() {
        let one = Vec3::new(1.0, 1.0, 1.0);
        // A box of no points, a box too big for its sphere, a height and a radius of no number.
        let mut empty = still(
            Vec3::new(f32::MAX, f32::MAX, f32::MAX),
            Vec3::new(f32::MIN, f32::MIN, f32::MIN),
        );
        empty.parts.clear();
        let huge = still(Vec3::new(-1.0e30, 0.0, 0.0), one);
        let tall = still(Vec3::ZERO, Vec3::new(1.0, 1.0, f32::INFINITY));
        let mut wide = still(Vec3::ZERO, one);
        wide.xy = f32::NAN;
        for (case, m) in [
            ("empty", empty),
            ("huge", huge),
            ("tall", tall),
            ("wide", wide),
        ] {
            for scale in [500, 1000, 2000] {
                let (mut ids, mut out) = (Ids::default(), Records::default());
                let made = setup_for(&m, [scale; 3], 0.0, &mut ids, &mut out);
                assert!(made.is_err(), "{case} at {scale}");
                assert!(out.0.is_empty(), "{case} at {scale}");
            }
        }
        let (mut ids, mut out) = (Ids::default(), Records::default());
        let far = Collision {
            points: vec![
                Vec3::ZERO,
                Vec3::new(1.0e20, 0.0, 0.0),
                Vec3::new(0.0, 1.0e20, 0.0),
            ],
            rings: vec![vec![0, 1, 2]],
        };
        assert!(collision_setup(&far, &mut ids, &mut out).is_err());
        assert!(out.0.keys().all(|id| id.raw() >> 24 != 0x02));
    }

    #[test]
    fn a_model_with_no_corners_covers_its_origin_alone() {
        let empty = only_collides();
        let scale = auberean_primitives::Vec3::new(2.0, 2.0, 2.0);
        let q = auberean_primitives::Quat {
            w: 1.0,
            x: 0.0,
            y: 0.0,
            z: 0.0,
        };
        assert_eq!(footprint_of(&empty, scale, q), (0.0, 0.0, 0.0, 0.0));
        let some = still(Vec3::new(-1.0, -2.0, 0.0), Vec3::new(1.0, 2.0, 1.0));
        assert_eq!(footprint_of(&some, scale, q), (-2.0, -4.0, 2.0, 4.0));
    }

    #[test]
    fn a_scale_keys_by_thousandths() {
        assert_eq!(
            milli(auberean_primitives::Vec3::new(1.0, 0.5, 1.2345)),
            [1000, 500, 1235]
        );
    }

    fn v(x: f32, y: f32, z: f32) -> auberean_primitives::Vec3 {
        auberean_primitives::Vec3::new(x, y, z)
    }

    /// A rotation by `angle` radians about the unit axis `(x, y, z)`.
    fn turn(angle: f32, x: f32, y: f32, z: f32) -> auberean_primitives::Quat {
        let (s, c) = dereth_primitives::num::math::sin_cosf(0.5 * angle);
        auberean_primitives::Quat {
            w: c,
            x: x * s,
            y: y * s,
            z: z * s,
        }
    }

    #[test]
    fn a_turned_scaled_and_moved_transform_splits_back_into_its_parts() {
        for (q, s, o) in [
            (turn(0.3, 0.0, 0.0, 1.0), v(1.0, 1.0, 1.0), v(1.0, 2.0, 3.0)),
            (
                turn(2.9, 0.6, 0.0, 0.8),
                v(2.0, 0.5, 1.5),
                v(-4.0, 0.0, 9.0),
            ),
            (turn(3.1, 1.0, 0.0, 0.0), v(1.0, 1.0, 1.0), v(0.0, 0.0, 0.0)),
            (turn(3.1, 0.0, 1.0, 0.0), v(3.0, 3.0, 3.0), v(0.5, 0.5, 0.5)),
            (
                turn(-3.1, 0.0, 0.0, 1.0),
                v(1.0, 1.0, 1.0),
                v(0.0, 7.0, 0.0),
            ),
        ] {
            let m = Mat4::from_parts(o, q, s);
            let (s2, q2, o2) = split_rigid(&m).expect("a rigid transform");
            let back = Mat4::from_parts(o2, q2, s2);
            for i in 0..4 {
                for j in 0..4 {
                    assert!(
                        (back.0[i][j] - m.0[i][j]).abs() < 1e-5,
                        "{:?} against {:?}",
                        back,
                        m
                    );
                }
            }
            let same = (q.w * q2.w + q.x * q2.x + q.y * q2.y + q.z * q2.z).abs();
            assert!((same - 1.0).abs() < 1e-5, "{q:?} split as {q2:?}");
        }
    }

    #[test]
    fn a_sheared_or_mirrored_transform_is_no_rigid_one() {
        let mut sheared = Mat4::IDENTITY;
        sheared.0[1][0] = 0.5;
        assert!(split_rigid(&sheared).is_none());
        let mirrored = Mat4::from_parts(
            v(0.0, 0.0, 0.0),
            turn(0.0, 0.0, 0.0, 1.0),
            v(-1.0, 1.0, 1.0),
        );
        assert!(split_rigid(&mirrored).is_none());
        let flat = Mat4::from_parts(v(0.0, 0.0, 0.0), turn(0.0, 0.0, 0.0, 1.0), v(1.0, 0.0, 1.0));
        assert!(split_rigid(&flat).is_none());
    }

    #[test]
    fn a_vertex_is_wholly_on_a_bone_its_weights_name_alone_however_often() {
        let on = |bones: [u8; 4], weights: [f32; 4], count: u8| {
            wholly_on(&Influences {
                count,
                bones,
                weights,
            })
        };
        assert_eq!(on([3, 0, 0, 0], [1.0, 0.0, 0.0, 0.0], 1), Some(3));
        assert_eq!(on([3, 3, 0, 0], [0.5, 0.5, 0.0, 0.0], 2), Some(3));
        assert_eq!(on([2, 5, 0, 0], [0.0, 1.0, 0.0, 0.0], 2), Some(5));
        assert_eq!(on([2, 5, 0, 0], [0.4, 0.6, 0.0, 0.0], 2), None, "blended");
        assert_eq!(on([2, 5, 0, 0], [0.0, 0.0, 0.0, 0.0], 2), None, "no weight");
    }

    #[test]
    fn a_carry_skins_a_vertex_by_its_bone_then_carries_it() {
        let palette = vec![Mat4::IDENTITY, Mat4::translation(v(0.0, 0.0, 2.0))];
        let on_one = Influences {
            count: 1,
            bones: [1, 0, 0, 0],
            weights: [1.0, 0.0, 0.0, 0.0],
        };
        let carry = Carry::Skinned(palette, Mat4::translation(v(10.0, 0.0, 0.0)));
        let (p, _) = carry.vertex(Some(&on_one), v(1.0, 1.0, 1.0), v(0.0, 0.0, 1.0));
        assert_eq!((p.x, p.y, p.z), (11.0, 1.0, 3.0));
        let (p, _) = carry.vertex(None, v(1.0, 1.0, 1.0), v(0.0, 0.0, 1.0));
        assert_eq!(
            (p.x, p.y, p.z),
            (11.0, 1.0, 1.0),
            "with no weights, as stored"
        );
        let whole = Carry::Whole(Mat4::translation(v(0.0, 5.0, 0.0)));
        assert!(whole.differs(&carry).is_infinite());
        assert!(whole.differs(&whole) == 0.0);
    }
}
