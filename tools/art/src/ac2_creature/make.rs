//! A creature's records, made from its table and the player's own Asheron's Call 2 files.
//!
//! - **Its parts** are the AC2 bones its skin hangs from, each carrying its parent's place in the
//!   bone tree; part 0 is the body's root.
//! - **Its models**, one a part: the triangles each part carries the most of, so a client that
//!   draws rigid parts still draws the whole body (it parts at the joints when it moves).
//! - **Its skinned body**: the whole skin bound to the parts, which bends at the joints.
//! - **Its animations**, one a motion: each AC2 clip played on AC2's skeleton and each part's
//!   frame taken thirty times a second, relative to the object, as Asheron's Call's animations
//!   hold them.
//! - **Its motion table**: the motions in the creature's two stances (out of combat and in
//!   unarmed combat), each with its own Ready where the table gives one, the links between them
//!   (eased from one pose to the next where they stand apart), and its **combat table**: the
//!   attacks the server may choose by height. Its death plays where it stands and ends on the
//!   ground; it turns at AC2's own rate.
//! - **Its collision**, from its skin: a radius that reaches its head and its blows, so a target
//!   it closes on stays outside it, and the one or two spheres that make it solid, in the shape
//!   Asheron's Call's creatures' spheres take.
//!
//! Everything is at AC2's own size, collision included: a world draws a creature larger or smaller
//! by its weenie's scale, as it does any creature's.

use std::collections::BTreeMap;

use dereth_animation::MotionCommand;
use dereth_assets::common::{Polygon, SwVertex, VertexArray};
use dereth_assets::motion::{MotionData, MotionTable};
use dereth_assets::tables::{CombatManeuver, CombatManeuverTable};
use dereth_assets::{
    skinned_body_id, AnimHook, Animation, GfxObj, HookData, Setup, SkinnedBody, SkinnedGroup,
    SkinnedVertex, Surface,
};
use dereth_primitives::records::{AnimData, AttackCone};
use dereth_primitives::shape::Sphere;
use dereth_primitives::{DataId, EraId, Frame, Quat, Vec3};
use glam::{DMat3, DMat4, DQuat, DVec3};

use auberean_assets::entity::Entity;
use auberean_assets::mesh::Mesh as Ac2Mesh;
use auberean_assets::setup::Setup as Ac2Setup;
use auberean_primitives::DataId as Ac2Id;
use auberean_world::look::{Look, PartLook};
use dereth_ac2_convert::records::{NewIds, Pictures, Records};
use dereth_ac2_convert::surface::{diffuse_stage, made_ready, SURFACE_CLIP_MAP, SURFACE_IMAGE};

use super::table::{Motion, Table};
use super::tint::{baked_texture, look_color, Bake, LookColor};
use crate::ac2_look::creature::{
    keyframes, native, parents_first, rot, worlds, Native, NativeClip,
};
use crate::ac2_look::source::Ac2;
use auberean_assets::material::{Material, MaterialModifier};

/// Keys a second an animation is taken at.
pub const RATE: f64 = 30.0;

/// The least weight a vertex puts on a bone for the bone to become a part.
const LEAST_WEIGHT: f64 = 1e-3;

/// Out of combat, and unarmed combat: the two stances a creature is written for.
pub const STYLES: [MotionCommand; 2] = [MotionCommand::NON_COMBAT, MotionCommand::HAND_COMBAT];

/// AC2's hit trigger in a clip's game hooks.
const AC2_HIT: u32 = 0x4000_0002;

/// The image category of a creature's pictures.
const IMAGE_CATEGORY: u32 = 4;

/// The pace Asheron's Call moves a walking creature at, meters a second, whatever its table says;
/// a walk's clip is played at the rate that sets its feet down at it.
pub const WALK_SPEED: f64 = 3.12;
/// The pace of a run at a run rate of one.
pub const RUN_SPEED: f64 = 4.0;

/// How much farther than AC2 each coarser level of detail starts, where a creature's table does
/// not say: AC2's distances switch to the coarser levels sooner than wanted here.
pub const DETAIL_DISTANCE_SCALE: f64 = 1.5;

/// A turn's rate where AC2 gives the body none, radians a second turning right: Asheron's
/// Call's own for a person.
pub const TURN_RATE: f64 = -1.5;

/// The turn from AC2's axes as the look's skeleton reader holds them (y up) to Asheron's Call's
/// (z up).
fn to_ac() -> DMat4 {
    DMat4::from_mat3(DMat3::from_cols(
        DVec3::new(1.0, 0.0, 0.0),
        DVec3::new(0.0, 0.0, 1.0),
        DVec3::new(0.0, -1.0, 0.0),
    ))
}

#[allow(clippy::cast_possible_truncation)] // LINT-OK: the files hold single precision.
fn v3(v: DVec3) -> Vec3 {
    Vec3::new(v.x as f32, v.y as f32, v.z as f32)
}

/// A world matrix in Asheron's Call's axes as a frame: its place, and its turn with any bone
/// scale taken out.
#[allow(clippy::cast_possible_truncation)] // LINT-OK: the files hold single precision.
fn frame_of(w: &DMat4) -> Frame {
    let q = DQuat::from_mat3(&rot(w)).normalize();
    Frame::new(
        v3(w.w_axis.truncate()),
        Quat::new(q.w as f32, q.x as f32, q.y as f32, q.z as f32),
    )
}

fn matrix_of(f: &Frame) -> DMat4 {
    let q = DQuat::from_xyzw(
        f64::from(f.rotation.x),
        f64::from(f.rotation.y),
        f64::from(f.rotation.z),
        f64::from(f.rotation.w),
    );
    DMat4::from_rotation_translation(
        q,
        DVec3::new(
            f64::from(f.origin.x),
            f64::from(f.origin.y),
            f64::from(f.origin.z),
        ),
    )
}

/// A vertex at the bind pose: its place and normal (Asheron's Call's axes), picture
/// coordinates, parts and weights.
type BoundVertex = (DVec3, DVec3, [f32; 2], [u16; 4], [f32; 4]);

/// The body read and bound: the skeleton, the parts chosen from it, and every vertex in the
/// object's space at the bind pose.
#[derive(Debug)]
pub struct Bound {
    native: Native,
    /// The bones that became parts, parents first; part `k` is bone `parts[k]`.
    pub parts: Vec<usize>,
    /// Each part's parent part.
    pub parent: Vec<Option<usize>>,
    /// Each part's frame at the bind pose, relative to the object.
    pub bind: Vec<Frame>,
    /// Per mesh, each vertex's place and normal at the bind pose, and its parts and
    /// weights.
    vertices: Vec<Vec<BoundVertex>>,
}

impl Bound {
    /// Each part's frame with `clip` at `t` seconds.
    fn frames_at(&self, clip: &NativeClip, t: f64) -> Vec<Frame> {
        let over = clip.at(t);
        let w = worlds(&self.native, Some(&over));
        let c = to_ac();
        let ci = c.inverse();
        self.parts
            .iter()
            .map(|&b| frame_of(&(c * w[b] * ci)))
            .collect()
    }
}

/// The table's body, bound in its bind clip's first keys.
///
/// # Errors
/// A record on the way will not read, or the bind clip moves another number of bones.
pub fn bind(ac2: &Ac2, t: &Table) -> Result<Bound, String> {
    let n = native(ac2, t.entity)?;
    let clip = NativeClip::of(&keyframes(ac2, t.bind)?, false);
    let first = worlds(&n, Some(&clip.first()));
    let palette: Vec<DMat4> = first
        .iter()
        .zip(&n.inverse_binds)
        .map(|(w, ib)| *w * *ib)
        .collect();
    // Every bone some drawn vertex hangs from, and the root.
    let mut weighed = vec![false; n.rest.len()];
    weighed[n.body[0]] = true;
    for p in &n.primitives {
        let vs = &n.meshes[p.mesh];
        for &i in &p.indices {
            let v = vs[i as usize];
            for k in 0..4 {
                if v.weights[k] >= LEAST_WEIGHT {
                    weighed[v.joints[k]] = true;
                }
            }
        }
    }
    let parts: Vec<usize> = parents_first(&n)
        .into_iter()
        .filter(|&b| weighed[b])
        .collect();
    let part_of: BTreeMap<usize, usize> = parts.iter().enumerate().map(|(k, b)| (*b, k)).collect();
    let parent = parts
        .iter()
        .map(|&b| {
            let mut up = n.parent[b];
            while let Some(p) = up {
                if let Some(k) = part_of.get(&p) {
                    return Some(*k);
                }
                up = n.parent[p];
            }
            None
        })
        .collect();
    let c = to_ac();
    let ci = c.inverse();
    let bind = parts
        .iter()
        .map(|&b| frame_of(&(c * first[b] * ci)))
        .collect();
    let vertices = n
        .meshes
        .iter()
        .map(|vs| {
            vs.iter()
                .map(|v| {
                    let mut m = DMat4::ZERO;
                    for k in 0..4 {
                        m += palette[v.joints[k]] * v.weights[k];
                    }
                    let p = c.transform_point3(m.transform_point3(v.position));
                    let nn = c.transform_vector3(DMat3::from_mat4(m) * v.normal);
                    let mut ps = [0u16; 4];
                    let mut ws = [0f32; 4];
                    let mut total = 0.0;
                    for k in 0..4 {
                        if v.weights[k] >= LEAST_WEIGHT {
                            if let Some(&part) = part_of.get(&v.joints[k]) {
                                ps[k] = u16::try_from(part).unwrap_or(0);
                                #[allow(clippy::cast_possible_truncation)]
                                // LINT-OK: the files hold single precision.
                                let w = v.weights[k] as f32;
                                ws[k] = w;
                                total += w;
                            }
                        }
                    }
                    if total > 0.0 {
                        for w in &mut ws {
                            *w /= total;
                        }
                    } else {
                        ws[0] = 1.0;
                    }
                    (p, nn.normalize_or_zero(), v.uv, ps, ws)
                })
                .collect()
        })
        .collect();
    Ok(Bound {
        native: n,
        parts,
        parent,
        bind,
        vertices,
    })
}

impl Bound {
    /// Every drawn vertex's place (meshes in order, each vertex once) with the parts at `frames`:
    /// the skin bent as the skinned body draws it.
    fn skin(&self, frames: &[Frame]) -> Vec<DVec3> {
        let to: Vec<DMat4> = frames
            .iter()
            .zip(&self.bind)
            .map(|(f, b)| matrix_of(f) * matrix_of(b).inverse())
            .collect();
        let mut drawn = std::collections::BTreeSet::new();
        for (mesh, t, _) in triangles(self) {
            for i in t {
                drawn.insert((mesh, i));
            }
        }
        drawn
            .into_iter()
            .map(|(mesh, i)| {
                let (p, _, _, parts, weights) = self.vertices[mesh][i as usize];
                (0..4)
                    .filter(|&k| weights[k] > 0.0)
                    .map(|k| to[usize::from(parts[k])].transform_point3(p) * f64::from(weights[k]))
                    .sum()
            })
            .collect()
    }

    /// The bounds of the drawn skin with the parts at `frames`: its least and greatest corner.
    fn bounds(&self, frames: &[Frame]) -> (DVec3, DVec3) {
        self.skin(frames).iter().fold(
            (DVec3::splat(f64::MAX), DVec3::splat(f64::MIN)),
            |(lo, hi), v| (lo.min(*v), hi.max(*v)),
        )
    }
}

/// How far apart two poses of the body are: the most any drawn vertex moves between them, meters
/// of AC2's size.
fn apart(b: &Bound, x: &[Frame], y: &[Frame]) -> f64 {
    b.skin(x)
        .iter()
        .zip(b.skin(y))
        .map(|(p, q)| p.distance(q))
        .fold(0.0, f64::max)
}

/// The new records' ids, handed out in order from one low number in every kind.
#[derive(Debug, Clone)]
pub struct Ids {
    next: BTreeMap<u32, u32>,
    first: u32,
}

impl Ids {
    /// Ids from `first` (the low sixteen bits) in every kind.
    #[must_use]
    pub fn from(first: u16) -> Self {
        Self {
            next: BTreeMap::new(),
            first: u32::from(first),
        }
    }

    /// The next id of the kind whose ids start with byte `kind`.
    pub fn next(&mut self, kind: u32) -> DataId {
        let n = self.next.entry(kind).or_insert((kind << 24) | self.first);
        let id = *n;
        *n += 1;
        DataId(id)
    }
}

impl NewIds for Ids {
    fn next_for(&mut self, kind: u32, _name: &str) -> DataId {
        self.next(kind)
    }
}

/// What a made creature is: the records' ids a weenie names, and how its motions were timed.
#[derive(Debug, Clone)]
pub struct Made {
    pub setup: DataId,
    pub motion_table: DataId,
    pub combat_table: DataId,
    pub skinned_body: DataId,
    pub parts: usize,
    pub vertices: usize,
    /// Its triangles at each level of detail, the fullest first.
    pub triangles: Vec<usize>,
    /// How far from the eye each level past the fullest is drawn from, meters.
    pub levels: Vec<f32>,
    /// Of the body's surfaces, how many and how many AC2 draws from both sides.
    pub surfaces: (usize, usize),
    /// Per motion: its command, its AC2 clip, the animation made, its frames, the rate it plays
    /// at, and where its blow lands (a share).
    pub motions: Vec<(String, u32, DataId, usize, f64, Option<f64>)>,
    /// The table's other looks of the body.
    pub looks: Vec<MadeLook>,
    /// Its size: AC2's radius and height for the body, and the collision made from its skin.
    pub size: Size,
    /// Its turn, radians a second (negative turning right).
    pub turn: f64,
    /// How far apart its two stances' Readies stand (linked where more than a few hundredths of
    /// its height), when it has both.
    pub stances: Option<f64>,
    /// Per stance and action played from its Ready: how far its first pose stands from the
    /// Ready's where it is eased into, and its last where it is eased back.
    pub eased: Vec<(String, String, Option<f64>, Option<f64>)>,
    /// Its death's sideways travel taken out, and how far its last pose was raised to the ground
    /// (lowered, where negative).
    pub death: Option<(f64, f64)>,
}

/// A made creature's size against AC2's.
#[derive(Debug, Clone)]
pub struct Size {
    /// AC2's radius and height for the body.
    pub ac2: (f64, f64),
    pub made: Collision,
}

/// Each material's surface and whether AC2 draws it from both sides; and its texture, where it
/// has a picture (the texture a look swaps for another).
type Surfaces = (Vec<(DataId, bool)>, Vec<Option<DataId>>);

/// A surface for each AC2 material the body draws with: its picture, cut out where the picture
/// is mostly clear; and whether AC2 draws the material from both sides. Also each material's
/// picture (the texture a look swaps for another), where it has one.
fn surfaces(
    ac2: &Ac2,
    b: &Bound,
    limit: u32,
    pictures: &mut Pictures,
    ids: &mut Ids,
    out: &mut Records,
) -> Result<Surfaces, String> {
    let mut made = Vec::with_capacity(b.native.materials.len());
    let mut textures = Vec::with_capacity(b.native.materials.len());
    for &(mi, list) in &b.native.materials {
        let mut texture = None;
        let surface = match list {
            Some(list) => {
                let (tex, _) = pictures.texture(ac2, list, limit, IMAGE_CATEGORY, ids, out)?;
                texture = Some(tex);
                let clear = ac2
                    .picture(list, 128)
                    .is_some_and(|p| p.clear_share() >= dereth_ac2_convert::surface::CLEAR_SHARE);
                Surface {
                    surface_type: if clear {
                        SURFACE_CLIP_MAP
                    } else {
                        SURFACE_IMAGE
                    },
                    orig_texture_id: Some(tex),
                    orig_palette_id: Some(DataId(0)),
                    color_value: None,
                    translucency: 0.0,
                    luminosity: 0.0,
                    diffuse: 1.0,
                }
            }
            None => dereth_ac2_convert::surface::plain([1.0; 4]),
        };
        let both = dereth_ac2_convert::surface::made_ready(ac2, mi, 0, None)
            .is_some_and(|(t, m)| dereth_ac2_convert::surface::two_sided(&t, &m));
        let id = ids.next_for(0x08, &format!("creature surface {mi:08X}"));
        out.add(id, surface.encode());
        made.push((id, both));
        textures.push(texture);
    }
    Ok((made, textures))
}

/// The picture a look draws one material with: its picture list, and the look's color baked into
/// it where the look changes the color.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LookPicture {
    pub list: u32,
    pub bake: Option<Bake>,
}

/// What another look of the body draws differently: an AC2 entity drawn with the same body, whose
/// appearance lays other pictures and colors on its materials.
#[derive(Debug, Clone, PartialEq)]
pub struct LookPictures {
    /// Per material of the body, in its order: the picture the look draws it with, where that is
    /// another than the body's own.
    pub pictures: Vec<Option<LookPicture>>,
    /// The least alpha AC2 draws a material of the look see-through by, where it does.
    pub alpha: Option<f32>,
    /// What the look changes that a classic surface cannot show.
    pub lost: Vec<&'static str>,
    /// The look's own size: its entity's scale times its description's.
    pub scale: f64,
    /// How many of the body's meshes the look swaps for others: a look of another shape, which
    /// takes a body of its own.
    pub swapped: usize,
}

/// The look `entity` draws the body whose materials (each a material instance and its picture
/// list, in the body's order) are `materials` with.
///
/// # Errors
/// The entity or its description will not read, or its description's chain will not put
/// together.
pub fn look_pictures(
    ac2: &Ac2,
    materials: &[(u32, Option<u32>)],
    entity: u32,
) -> Result<LookPictures, String> {
    let records = ac2.records();
    let e: Entity = ac2
        .record(entity)
        .ok_or_else(|| format!("entity {entity:08X} will not read"))?;
    let vd = e
        .object
        .ok_or_else(|| format!("entity {entity:08X} names no description"))?;
    let look = Look::of_record(records, Ac2Id(vd))
        .map_err(|err| format!("entity {entity:08X}: its description {vd:08X}: {err:?}"))?;
    let scale = f64::from(e.scale.map_or(1.0, |s| s.x)) * f64::from(look.scale.x);
    let picture = |ready: &Option<(Material, Vec<MaterialModifier>)>| {
        ready
            .as_ref()
            .map(|(t, m)| diffuse_stage(t, m).0)
            .filter(|l| *l != 0)
    };
    let mut pictures = vec![None; materials.len()];
    let mut alpha: Option<f32> = None;
    let mut lost: Vec<&'static str> = Vec::new();
    let mut swapped = 0;
    for g in look.tree() {
        let part = look.atoms(records, g.key).map(PartLook::from_atoms);
        let Some(setup) = ac2.record::<Ac2Setup>(g.setup.0) else {
            continue;
        };
        for &mesh in &setup.meshes {
            let Some(m) = ac2.record::<Ac2Mesh>(mesh.0) else {
                continue;
            };
            if let Some(p) = &part {
                if p.mesh(mesh, &mut |id| ac2.record::<Ac2Mesh>(id.0).map(|m| m.kind)) != mesh {
                    swapped += 1;
                }
            }
            for mi in &m.materials {
                let Some(k) = materials.iter().position(|(x, _)| *x == mi.0) else {
                    continue;
                };
                let own_ready = made_ready(ac2, mi.0, m.kind, None);
                let look_ready = made_ready(ac2, mi.0, m.kind, part.as_ref());
                let own = picture(&own_ready).or(materials[k].1);
                let theirs = picture(&look_ready);
                let color = match (&own_ready, &look_ready) {
                    (Some((t0, m0)), Some((t1, m1))) => look_color(ac2, (t0, m0), (t1, m1)),
                    _ => LookColor::default(),
                };
                if let Some(a) = color.alpha {
                    alpha = Some(alpha.map_or(a, |x| x.min(a)));
                }
                for l in &color.lost {
                    if !lost.contains(l) {
                        lost.push(l);
                    }
                }
                let list = theirs.or(own);
                if let (Some(list), true) = (
                    list,
                    (theirs.is_some() && theirs != own) || color.bake.is_some(),
                ) {
                    pictures[k] = Some(LookPicture {
                        list,
                        bake: color.bake,
                    });
                }
            }
        }
    }
    Ok(LookPictures {
        pictures,
        alpha,
        lost,
        scale,
        swapped,
    })
}

/// A look made: the textures its weenie swaps on the body (old for new), and its size.
#[derive(Debug, Clone)]
pub struct MadeLook {
    pub name: String,
    pub entity: u32,
    pub scale: f64,
    pub textures: Vec<(DataId, DataId)>,
    /// The swaps as its weenie lays them, part by part ([`part_swaps`]).
    pub parts: Vec<(u16, DataId, DataId)>,
    /// Meshes it swaps (left as the body's: another shape takes a creature table of its own).
    pub swapped: usize,
    /// Materials it changes that have no picture of their own to swap.
    pub unswapped: usize,
    /// How see-through AC2 draws it, as a weenie's translucency (one less its color's alpha).
    pub translucency: Option<f64>,
    /// What it changes that a classic surface cannot show.
    pub lost: Vec<&'static str>,
}

/// Each of the table's looks: its pictures written, and the swaps that draw it.
fn looks(
    ac2: &Ac2,
    t: &Table,
    b: &Bound,
    textures: &[Option<DataId>],
    pictures: &mut Pictures,
    ids: &mut Ids,
    out: &mut Records,
) -> Result<Vec<MadeLook>, String> {
    let mut made = Vec::new();
    let mut baked: BTreeMap<(u32, [u32; 6]), DataId> = BTreeMap::new();
    for l in &t.look {
        let lp = look_pictures(ac2, &b.native.materials, l.entity)?;
        let mut swaps = Vec::new();
        let mut unswapped = 0;
        for (k, p) in lp.pictures.iter().enumerate() {
            let Some(p) = p else { continue };
            let Some(old) = textures.get(k).copied().flatten() else {
                unswapped += 1;
                continue;
            };
            let new = match &p.bake {
                Some(bake) => match baked.get(&(p.list, bake.key())) {
                    Some(id) => *id,
                    None => {
                        let id =
                            baked_texture(ac2, p.list, t.picture, IMAGE_CATEGORY, bake, ids, out)?;
                        baked.insert((p.list, bake.key()), id);
                        id
                    }
                },
                None => {
                    pictures
                        .texture(ac2, p.list, t.picture, IMAGE_CATEGORY, ids, out)?
                        .0
                }
            };
            if new != old && !swaps.contains(&(old, new)) {
                swaps.push((old, new));
            }
        }
        made.push(MadeLook {
            name: l.name.clone(),
            entity: l.entity,
            scale: lp.scale,
            parts: Vec::new(),
            textures: swaps,
            swapped: lp.swapped,
            unswapped,
            translucency: lp.alpha.map(|a| 1.0 - f64::from(a)),
            lost: lp.lost,
        });
    }
    Ok(made)
}

/// Each drawn triangle: its mesh, its three vertices and its material.
fn triangles(b: &Bound) -> Vec<(usize, [u32; 3], usize)> {
    let mut out = Vec::new();
    for p in &b.native.primitives {
        for t in p.indices.as_chunks::<3>().0 {
            out.push((p.mesh, *t, p.material));
        }
    }
    out
}

/// Each part's triangles: each drawn triangle goes to the part that carries the most of it.
fn by_part(b: &Bound) -> Vec<Vec<(usize, [u32; 3], usize)>> {
    let mut by_part: Vec<Vec<(usize, [u32; 3], usize)>> = vec![Vec::new(); b.parts.len()];
    for (mesh, t, material) in triangles(b) {
        let mut weight = vec![0f32; b.parts.len()];
        for &i in &t {
            let (_, _, _, ps, ws) = b.vertices[mesh][i as usize];
            for k in 0..4 {
                weight[usize::from(ps[k])] += ws[k];
            }
        }
        let part = (0..weight.len())
            .max_by(|x, y| weight[*x].total_cmp(&weight[*y]))
            .unwrap_or(0);
        by_part[part].push((mesh, t, material));
    }
    by_part
}

/// The materials each part's model draws with, each once.
fn part_materials(b: &Bound) -> Vec<Vec<usize>> {
    by_part(b)
        .iter()
        .map(|tris| {
            let mut used: Vec<usize> = Vec::new();
            for &(_, _, m) in tris {
                if !used.contains(&m) {
                    used.push(m);
                }
            }
            used
        })
        .collect()
}

/// The most texture swaps one object's description carries: its count is a byte.
pub const MOST_SWAPS: usize = 255;

/// A look's swaps as its weenie lays them, part by part: every swap on part 0, which the skinned
/// body draws by, and on each other part the swaps of the textures its own model draws, so that a
/// client drawing the parts draws the look on all of them and not on the first alone.
fn part_swaps(
    swaps: &[(DataId, DataId)],
    textures: &[Option<DataId>],
    materials: &[Vec<usize>],
) -> Vec<(u16, DataId, DataId)> {
    let mut out: Vec<(u16, DataId, DataId)> =
        swaps.iter().map(|&(old, new)| (0, old, new)).collect();
    for (part, used) in materials.iter().enumerate().skip(1) {
        let Ok(part) = u16::try_from(part) else {
            break;
        };
        for &(old, new) in swaps {
            let drawn = used
                .iter()
                .any(|&m| textures.get(m).copied().flatten() == Some(old));
            if drawn {
                out.push((part, old, new));
            }
        }
    }
    out
}

/// One model a part: the triangles the part carries the most of, in its own space at the bind
/// pose. A part that carries none draws the shared empty model.
fn models(
    b: &Bound,
    surfaces: &[(DataId, bool)],
    ids: &mut Ids,
    out: &mut Records,
) -> Result<Vec<DataId>, String> {
    let by_part = by_part(b);
    let empty = ids.next(0x01);
    let g = GfxObj {
        id: empty,
        flags: 0,
        surfaces: Vec::new(),
        vertex_array: VertexArray {
            vertex_type: 1,
            vertices: Vec::new(),
        },
        physics_polygons: Vec::new(),
        physics_bsp: None,
        sort_center: Vec3::ZERO,
        polygons: Vec::new(),
        drawing_bsp: None,
        did_degrade: None,
    };
    out.add(empty, encode(&g, empty)?);
    let mut made = Vec::with_capacity(b.parts.len());
    for (part, tris) in by_part.iter().enumerate() {
        if tris.is_empty() {
            made.push(empty);
            continue;
        }
        let inverse = matrix_of(&b.bind[part]).inverse();
        let mut used_surfaces: Vec<usize> = Vec::new();
        let mut index_of: BTreeMap<(usize, u32), u16> = BTreeMap::new();
        let mut vertices: Vec<SwVertex> = Vec::new();
        let mut polygons: Vec<Polygon> = Vec::new();
        for &(mesh, t, material) in tris {
            let s = match used_surfaces.iter().position(|m| *m == material) {
                Some(k) => k,
                None => {
                    used_surfaces.push(material);
                    used_surfaces.len() - 1
                }
            };
            let mut corner = [0u16; 3];
            for (k, &i) in t.iter().enumerate() {
                let at = vertices.len();
                let id = *index_of.entry((mesh, i)).or_insert_with(|| {
                    let (p, n, uv, _, _) = b.vertices[mesh][i as usize];
                    vertices.push(SwVertex {
                        id: i16::try_from(at).unwrap_or(i16::MAX),
                        position: v3(inverse.transform_point3(p)),
                        normal: v3(inverse.transform_vector3(n)),
                        uvs: vec![(uv[0], uv[1])],
                    });
                    u16::try_from(at).unwrap_or(u16::MAX)
                });
                corner[k] = id;
            }
            let both = surfaces[material].1;
            polygons.push(Polygon {
                poly_id: i16::try_from(polygons.len()).unwrap_or(i16::MAX),
                num_pts: 3,
                stippling: 1,
                sides_type: i32::from(both),
                pos_surface: u16::try_from(s).unwrap_or(0),
                neg_surface: u16::try_from(s).unwrap_or(0),
                vertex_ids: corner.to_vec(),
                pos_uv_indices: Some(vec![0; 3]),
                neg_uv_indices: both.then(|| vec![0; 3]),
            });
        }
        if vertices.len() > usize::from(u16::MAX >> 1) {
            return Err(format!(
                "part {part} carries {} vertices, more than a model holds",
                vertices.len()
            ));
        }
        let va = VertexArray {
            vertex_type: 1,
            vertices,
        };
        let drawing = dereth_builder::bsp::drawing(&va, &polygons);
        let sort_center = drawing
            .nodes
            .first()
            .and_then(|n| n.sphere)
            .map_or(Vec3::ZERO, |s| s.center);
        let id = ids.next(0x01);
        let g = GfxObj {
            id,
            flags: 2,
            surfaces: used_surfaces.iter().map(|m| surfaces[*m].0).collect(),
            vertex_array: va,
            physics_polygons: Vec::new(),
            physics_bsp: None,
            sort_center,
            polygons,
            drawing_bsp: Some(drawing),
            did_degrade: None,
        };
        out.add(id, encode(&g, id)?);
        made.push(id);
    }
    Ok(made)
}

fn encode(g: &GfxObj, id: DataId) -> Result<Vec<u8>, String> {
    g.encode_for(EraId::Eor).map_err(|e| format!("{id:?}: {e}"))
}

/// The skinned body: every drawn triangle, by level of detail and surface, its vertices carried
/// by their parts. The coarser levels draw the fullest's vertices.
fn skinned(
    b: &Bound,
    setup: DataId,
    surfaces: &[(DataId, bool)],
    detail_scale: f64,
) -> SkinnedBody {
    let mut vertices = Vec::new();
    let mut index_of: BTreeMap<(usize, u32), u32> = BTreeMap::new();
    let mut groups: BTreeMap<(u8, usize), Vec<u32>> = BTreeMap::new();
    let lower = b.native.lower.iter().enumerate().flat_map(|(j, prims)| {
        let level = u8::try_from(j + 1).unwrap_or(u8::MAX);
        prims.iter().flat_map(move |p| {
            p.indices
                .as_chunks::<3>()
                .0
                .iter()
                .map(move |t| (level, p.mesh, *t, p.material))
        })
    });
    let all = triangles(b)
        .into_iter()
        .map(|(mesh, t, material)| (0u8, mesh, t, material))
        .chain(lower);
    for (level, mesh, t, material) in all {
        let g = groups.entry((level, material)).or_default();
        for &i in &t {
            let at = u32::try_from(vertices.len()).unwrap_or(u32::MAX);
            let k = *index_of.entry((mesh, i)).or_insert_with(|| {
                let (p, n, uv, parts, weights) = b.vertices[mesh][i as usize];
                vertices.push(SkinnedVertex {
                    position: v3(p),
                    normal: v3(n),
                    uv,
                    parts,
                    weights,
                });
                at
            });
            g.push(k);
        }
    }
    SkinnedBody {
        id: skinned_body_id(setup),
        setup,
        bind: b.bind.clone(),
        surfaces: surfaces.iter().map(|s| s.0).collect(),
        vertices,
        groups: groups
            .into_iter()
            .map(|((level, m), indices)| SkinnedGroup {
                surface: u32::try_from(m).unwrap_or(0),
                two_sided: surfaces.get(m).is_some_and(|s| s.1),
                indices,
                level,
            })
            .collect(),
        levels: detail_distances(&b.native.lower_from, detail_scale),
    }
}

/// The distance each coarser level starts at: AC2's, `scale` times as far.
fn detail_distances(ac2: &[f32], scale: f64) -> Vec<f32> {
    #[allow(clippy::cast_possible_truncation)] // a distance in meters
    ac2.iter().map(|&d| (f64::from(d) * scale) as f32).collect()
}

/// The most collision spheres a body is given: the clients and the server collide with no more
/// than a setup's first two.
pub const MOST_SPHERES: usize = 2;

/// The body's size as the server and the client measure it, from its skin: its radius (the
/// reach of its front and its sides from its middle), its height, and the spheres it collides
/// with. All at AC2's size; a weenie's scale scales them, as any creature's.
#[derive(Debug, Clone, PartialEq)]
pub struct Collision {
    pub radius: f64,
    pub height: f64,
    pub spheres: Vec<(DVec3, f64)>,
}

/// The collision of a body whose drawn skin at the bind pose spans `lo` to `hi`, whose shortest
/// blow lands `blow` meters in front of its middle, and whose girth (its collision sphere's
/// radius in AC2) is `girth`.
///
/// Melee reach is measured from the edge of the radius, so the radius is the farthest the body
/// reaches forward or to a side: as it stands, and at the moment its shortest blow lands, so a
/// target it closes on stays outside its head and front and is met by the blow. What trails
/// behind (a tail) doesn't count. Below the ground counts as the ground.
///
/// Its spheres take the shape Asheron's Call's creatures have: on the body's middle, standing on
/// the ground, one where the body is no taller than about two of them and two stacked to its top
/// where it is taller. A body whose head reaches more than a sphere's width ahead of its middle
/// lays its second sphere forward to its head instead, wide enough that the two touch, as the
/// long creatures' spheres lie. No sphere stands taller than the body.
#[must_use]
pub fn collision(lo: DVec3, hi: DVec3, blow: Option<f64>, girth: f64) -> Collision {
    let ground = lo.z.max(0.0);
    let side = lo.x.abs().max(hi.x.abs());
    let radius = hi.y.max(side).max(blow.unwrap_or(0.0)).max(0.05);
    let height = (hi.z - ground).max(0.05);
    let r = girth.min(height * 0.5).max(0.05);
    let ahead = hi.y;
    let spheres = if ahead > 2.0 * r {
        let width = (hi.x - lo.x).max(0.1);
        let r = (ahead / 3.0).max(r).min(height * 0.5).min(width * 0.5);
        vec![
            (DVec3::new(0.0, 0.0, ground + r), r),
            (DVec3::new(0.0, ahead - r, ground + r), r),
        ]
    } else if height > 2.5 * r {
        vec![
            (DVec3::new(0.0, 0.0, ground + r), r),
            (DVec3::new(0.0, 0.0, ground + height - r), r),
        ]
    } else {
        vec![(DVec3::new(0.0, 0.0, ground + r), r)]
    };
    Collision {
        radius,
        height,
        spheres,
    }
}

/// How far in front of the body's middle its shortest blow lands: each attack's clip at its hit.
fn shortest_blow(ac2: &Ac2, t: &Table, b: &Bound) -> Result<Option<f64>, String> {
    let mut least: Option<f64> = None;
    for m in t.motion.iter().filter(|m| m.attack.is_some()) {
        let clip = NativeClip::of(&keyframes(ac2, m.animation)?, true);
        let share = m.hit.or_else(|| ac2_hit(ac2, m.animation)).unwrap_or(0.5);
        let front = b.bounds(&b.frames_at(&clip, clip.period() * share)).1.y;
        least = Some(least.map_or(front, |l| l.min(front)));
    }
    Ok(least)
}

/// The setup: the parts, their parents, the bind pose as the default placement, and the
/// body's collision.
#[allow(clippy::cast_possible_truncation)] // LINT-OK: the files hold single precision.
fn setup(
    ac2: &Ac2,
    b: &Bound,
    id: DataId,
    models: Vec<DataId>,
    (entity, c): (u32, &Collision),
) -> Result<Setup, String> {
    let ac2_setup = ac2_body_setup(ac2, entity)?;
    let (mut lo, mut hi) = (DVec3::splat(f64::MAX), DVec3::splat(f64::MIN));
    for vs in &b.vertices {
        for v in vs {
            lo = lo.min(v.0);
            hi = hi.max(v.0);
        }
    }
    let middle = (lo + hi) * 0.5;
    let sorting = Sphere {
        center: v3(middle),
        radius: ((hi - lo).length() * 0.5) as f32,
    };
    let spheres = c
        .spheres
        .iter()
        .map(|(center, radius)| Sphere {
            center: v3(*center),
            radius: *radius as f32,
        })
        .collect();
    let sel = ac2_setup.selection_sphere;
    let placement = dereth_assets::geometry::Placement {
        frames: b.bind.clone(),
        hooks: Vec::new(),
    };
    Ok(Setup {
        id,
        flags: 1 | 4,
        parts: models,
        parent_index: Some(
            b.parent
                .iter()
                .map(|p| p.map_or(u32::MAX, |k| u32::try_from(k).unwrap_or(u32::MAX)))
                .collect(),
        ),
        default_scale: None,
        allow_free_heading: true,
        has_physics_bsp: false,
        holding_locations: BTreeMap::new(),
        connection_points: BTreeMap::new(),
        placement_frames: BTreeMap::from([(0, placement.clone()), (0x65, placement)]),
        cylspheres: Vec::new(),
        spheres,
        height: c.height as f32,
        radius: c.radius as f32,
        step_up_height: ac2_setup.step_up,
        step_down_height: ac2_setup.step_down,
        sorting_sphere: sorting,
        selection_sphere: Sphere {
            center: Vec3::new(sel.center.x, sel.center.y, sel.center.z),
            radius: sel.radius,
        },
        lights: BTreeMap::new(),
        default_anim_id: DataId(0),
        default_script_id: DataId(0),
        default_mtable_id: DataId(0),
        default_stable_id: DataId(0),
        default_phstable_id: DataId(0),
        written_order: dereth_assets::SetupOrder::default(),
    })
}

/// AC2's collision for a body: its height, radius, girth (its first collision sphere's radius,
/// else its radius as Asheron's Call's creatures relate the two), steps and picking sphere.
struct Ac2Collision {
    height: f32,
    radius: f32,
    girth: f32,
    step_up: f32,
    step_down: f32,
    selection_sphere: Sphere,
}

fn ac2_body_setup(ac2: &Ac2, entity: u32) -> Result<Ac2Collision, String> {
    use auberean_assets::entity::Entity;
    use auberean_assets::visual::VisualDesc;
    let e: Entity = ac2
        .record(entity)
        .ok_or_else(|| format!("entity {entity:08X} will not read"))?;
    let vd: VisualDesc = ac2
        .record(e.object.unwrap_or(0))
        .ok_or_else(|| format!("entity {entity:08X}: its description"))?;
    let setup_id = vd
        .part_groups
        .unwrap_or_default()
        .into_iter()
        .find_map(|(_, g)| g.setup)
        .ok_or_else(|| format!("entity {entity:08X} has no body"))?;
    let s: auberean_assets::setup::Setup = ac2
        .record(setup_id.0)
        .ok_or_else(|| format!("setup {:08X} will not read", setup_id.0))?;
    let sphere = |c: auberean_primitives::Vec3, r: f32| Sphere {
        center: Vec3::new(c.x, c.y, c.z),
        radius: r,
    };
    Ok(Ac2Collision {
        height: s.height,
        radius: s.radius,
        girth: s
            .spheres
            .first()
            .map_or(s.radius * std::f32::consts::FRAC_1_SQRT_2, |sp| sp.radius),
        step_up: s.step_up_height,
        step_down: s.step_down_height,
        selection_sphere: sphere(s.selection_sphere.center, s.selection_sphere.radius),
    })
}

/// The share of `clip` at which its first AC2 hit lands.
fn ac2_hit(ac2: &Ac2, clip: u32) -> Option<f64> {
    let a = keyframes(ac2, clip).ok()?;
    let period = f64::from(a.period).max(1e-6);
    let hit = a
        .events
        .as_ref()?
        .hooks
        .iter()
        .find(|h| h.kind == AC2_HIT)?;
    let t = f64::from(hit.time);
    // A hit is kept as a share of the clip where it is no more than one, else in seconds.
    Some(if t <= 1.0 { t } else { t / period }.clamp(0.0, 1.0))
}

/// How far one round of `clip` carries the body, meters of AC2's size: how far forward and back
/// a foot (a part that comes within a tenth of the body's height of its lowest point) swings,
/// the most of any.
fn measured_stride(b: &Bound, clip: &NativeClip) -> f64 {
    let period = clip.period();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    // LINT-OK: a clip's length in steps is small and positive.
    let steps = ((period * RATE).ceil() as usize).max(2);
    let frames: Vec<Vec<Frame>> = (0..steps)
        .map(|k| b.frames_at(clip, period * k as f64 / steps as f64))
        .collect();
    let lowest = frames
        .iter()
        .flatten()
        .map(|f| f64::from(f.origin.z))
        .fold(f64::MAX, f64::min);
    let highest = frames
        .iter()
        .flatten()
        .map(|f| f64::from(f.origin.z))
        .fold(f64::MIN, f64::max);
    let near = lowest + (highest - lowest) * 0.1;
    let mut stride = 0.0f64;
    for part in 0..b.parts.len() {
        let zs = frames.iter().map(|f| f64::from(f[part].origin.z));
        if zs.fold(f64::MAX, f64::min) > near {
            continue;
        }
        let ys: Vec<f64> = frames.iter().map(|f| f64::from(f[part].origin.y)).collect();
        let span = ys.iter().copied().fold(f64::MIN, f64::max)
            - ys.iter().copied().fold(f64::MAX, f64::min);
        stride = stride.max(span);
    }
    stride
}

/// The share of a dead body's skin that may lie below the ground: a fin, a spike or a wing tip
/// sinks in rather than holding the body up off the ground.
const SUNK: f64 = 0.05;

/// A death played where the creature stands and ending on the ground: each frame's drawn skin
/// kept over the place its middle started from (the body's own sideways travel taken out, its
/// turns kept), and the body eased up or down over the clip until, at its last frame, it lies on
/// the ground: all but the lowest [`SUNK`] of its skin above it. Returns how far the travel taken
/// out had carried the middle, and how far the last frame was raised.
#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
// LINT-OK: single precision; a few frames.
fn settle(b: &Bound, frames: &mut [dereth_assets::geometry::AnimFrame]) -> (f64, f64) {
    let middle = |f: &[Frame]| {
        let v = b.skin(f);
        v.iter().sum::<DVec3>() / v.len().max(1) as f64
    };
    let Some(first) = frames.first().map(|f| middle(&f.frames)) else {
        return (0.0, 0.0);
    };
    let mut carried = 0.0f64;
    for f in frames.iter_mut() {
        let d = middle(&f.frames) - first;
        carried = carried.max(d.truncate().length());
        for p in &mut f.frames {
            p.origin.x -= d.x as f32;
            p.origin.y -= d.y as f32;
        }
    }
    let last = frames.len() - 1;
    let lift = -lowest_but(b, &frames[last].frames, SUNK);
    for (k, f) in frames.iter_mut().enumerate() {
        let s = if last == 0 {
            1.0
        } else {
            k as f64 / last as f64
        };
        let eased = s * s * (3.0 - 2.0 * s);
        for p in &mut f.frames {
            p.origin.z += (lift * eased) as f32;
        }
    }
    (carried, lift)
}

/// The height below which `share` of the drawn skin lies, with the parts at `frames`.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
// LINT-OK: a share of a vertex count.
fn lowest_but(b: &Bound, frames: &[Frame], share: f64) -> f64 {
    let mut zs: Vec<f64> = b.skin(frames).iter().map(|v| v.z).collect();
    zs.sort_by(f64::total_cmp);
    zs.get(((zs.len().saturating_sub(1)) as f64 * share) as usize)
        .copied()
        .unwrap_or(0.0)
}

/// AC2's own turn rate for the body standing, radians a second: its motion description's, on
/// the ground in combat, or else at peace.
fn ac2_turn_rate(ac2: &Ac2, entity: u32) -> Option<f64> {
    /// The ground; and AC2's peace and creature combat modes.
    const EARTH: u32 = 0x4000_0001;
    const PEACE: u32 = 0x4000_0001;
    const COMBAT: u32 = 0x4100_0012;
    let m = super::probe::motion_desc(ac2, entity)?;
    let set = |mode: u32| {
        m.set_of
            .iter()
            .find(|(k, _)| k.element == EARTH && k.mode == mode)
            .and_then(|(_, i)| m.sets.get(usize::try_from(*i).ok()?))
    };
    let s = set(COMBAT).or_else(|| set(PEACE))?;
    let rate = f64::from(s.default.still_turn_rate);
    (rate > 0.0).then(|| rate.to_radians())
}

/// The class bits of a command: what kind of motion it is.
const fn class(c: MotionCommand) -> u32 {
    c.0 >> 24
}

/// A motion's animation: each part's frame thirty times a second over the clip, and the attack
/// hook where its blow lands. A cycle (`looped`) leaves out its closing frame, which is its first
/// again; a walk or a run carries the body forward `travel` meters (AC2's size) a round, spread
/// evenly over its frames, as the object's own motion; a death (`dying`) is [`settle`]d, and
/// what that changed is returned with it.
fn animation(
    b: &Bound,
    clip: &NativeClip,
    id: DataId,
    hit: Option<(f64, &super::table::Attack)>,
    looped: bool,
    travel: Option<f64>,
    dying: bool,
) -> (Animation, Option<(f64, f64)>) {
    let period = clip.period();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    // LINT-OK: a clip's length in steps is small and positive.
    let steps = ((period * RATE).round() as usize).max(1);
    let count = if looped { steps } else { steps + 1 };
    let hit_frame = hit.map(|(share, _)| {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // LINT-OK: a share of a small frame count.
        let f = (share * steps as f64).round() as usize;
        f.min(count - 1)
    });
    let mut part_frames = (0..count)
        .map(|k| {
            let t = (period * k as f64 / steps as f64)
                .min(period - 1e-4)
                .max(0.0);
            let mut hooks = Vec::new();
            if let (Some(at), Some((_, a))) = (hit_frame, hit) {
                if at == k {
                    #[allow(clippy::cast_possible_truncation)] // LINT-OK: single precision.
                    let reach = a.reach as f32;
                    hooks.push(AnimHook {
                        hook_type: 3,
                        direction: 1,
                        data: HookData::Attack(AttackCone {
                            part_index: a.part,
                            left: (-reach * 0.5, reach),
                            right: (reach * 0.5, reach),
                            radius: reach,
                            height: reach,
                        }),
                    });
                }
            }
            dereth_assets::geometry::AnimFrame {
                frames: b.frames_at(clip, t),
                hooks,
            }
        })
        .collect::<Vec<_>>();
    let settled = dying.then(|| settle(b, &mut part_frames));
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
    // LINT-OK: single precision; a few frames.
    let pos_frames = travel.map(|d| {
        let step = (d / count as f64) as f32;
        vec![Frame::new(Vec3::new(0.0, step, 0.0), Quat::IDENTITY); count]
    });
    (
        Animation {
            id,
            flags: u32::from(pos_frames.is_some()),
            num_parts: u32::try_from(b.parts.len()).unwrap_or(0),
            num_frames: u32::try_from(count).unwrap_or(0),
            has_hooks: false,
            pos_frames,
            part_frames,
        },
        settled,
    )
}

/// How many frames a made link between two motions takes: a fifth of a second.
const LINK_FRAMES: usize = 6;

/// How many frames the link between the two stances takes: AC2 eases a creature between peace
/// and combat over a quarter of a second, and plays nothing of its own.
const STANCE_FRAMES: usize = 8;

/// A link from one pose to another over `count` frames: each part's frame eased from `from` to
/// `to` (its place in a straight line, its turn by the shorter way).
fn blend(b: &Bound, from: &[Frame], to: &[Frame], id: DataId, count: usize) -> Animation {
    let quat =
        |f: &Frame| glam::Quat::from_xyzw(f.rotation.x, f.rotation.y, f.rotation.z, f.rotation.w);
    let part_frames = (0..count)
        .map(|k| {
            #[allow(clippy::cast_precision_loss)] // LINT-OK: a few frames.
            let s = (k + 1) as f32 / count as f32;
            let frames = from
                .iter()
                .zip(to)
                .map(|(a, c)| {
                    let q = quat(a).slerp(quat(c), s).normalize();
                    Frame::new(
                        Vec3::new(
                            a.origin.x + (c.origin.x - a.origin.x) * s,
                            a.origin.y + (c.origin.y - a.origin.y) * s,
                            a.origin.z + (c.origin.z - a.origin.z) * s,
                        ),
                        Quat::new(q.w, q.x, q.y, q.z),
                    )
                })
                .collect();
            dereth_assets::geometry::AnimFrame {
                frames,
                hooks: Vec::new(),
            }
        })
        .collect();
    Animation {
        id,
        flags: 0,
        num_parts: u32::try_from(b.parts.len()).unwrap_or(0),
        num_frames: u32::try_from(count).unwrap_or(0),
        has_hooks: false,
        pos_frames: None,
        part_frames,
    }
}

#[allow(clippy::cast_possible_truncation)] // LINT-OK: the files hold single precision.
fn entry(key: u32, anims: Vec<AnimData>, omega: Option<f64>) -> MotionData {
    MotionData {
        key,
        bitfield: 0,
        flags: if omega.is_some() { 2 } else { 0 },
        anims,
        velocity: None,
        omega: omega.map(|w| Vec3::new(0.0, 0.0, w as f32)),
    }
}

/// The creature's records from its table, into `out`.
///
/// # Errors
/// A record on the way will not read or encode, or a motion the table names is not one of
/// Asheron's Call's.
#[allow(clippy::too_many_lines, clippy::cast_possible_truncation)]
pub fn make(ac2: &Ac2, t: &Table, ids: &mut Ids, out: &mut Records) -> Result<Made, String> {
    let b = bind(ac2, t)?;
    let mut pictures = Pictures::default();
    let (surfaces, textures) = surfaces(ac2, &b, t.picture, &mut pictures, ids, out)?;
    let mut looks = looks(ac2, t, &b, &textures, &mut pictures, ids, out)?;
    let materials = part_materials(&b);
    for l in &mut looks {
        l.parts = part_swaps(&l.textures, &textures, &materials);
        if l.parts.len() > MOST_SWAPS {
            return Err(format!(
                "look {} swaps {} textures over the parts, more than a description carries \
                 ({MOST_SWAPS})",
                l.name,
                l.parts.len()
            ));
        }
    }
    let models = models(&b, &surfaces, ids, out)?;
    let setup_id = ids.next(0x02);
    let (lo, hi) = b.bounds(&b.bind);
    let ac2_size = ac2_body_setup(ac2, t.entity)?;
    let size = collision(
        lo,
        hi,
        shortest_blow(ac2, t, &b)?,
        f64::from(ac2_size.girth),
    );
    let s = setup(ac2, &b, setup_id, models, (t.entity, &size))?;
    out.add(
        setup_id,
        s.encode_for(EraId::Eor)
            .map_err(|e| format!("{setup_id:?}: {e}"))?,
    );
    let detail_scale = t.detail_distance_scale.unwrap_or(DETAIL_DISTANCE_SCALE);
    let body = skinned(&b, setup_id, &surfaces, detail_scale);
    let mut triangles = vec![0; body.levels.len() + 1];
    for g in &body.groups {
        if let Some(t) = triangles.get_mut(usize::from(g.level)) {
            *t += g.indices.len() / 3;
        }
    }
    let levels = body.levels.clone();
    let vertices = body.vertices.len();
    out.add(body.id, body.encode());

    let mtable_id = ids.next(0x09);
    let ctable_id = ids.next(0x30);
    let mut cycles: Vec<MotionData> = Vec::new();
    let mut links: BTreeMap<u32, Vec<MotionData>> = BTreeMap::new();
    let mut maneuvers = Vec::new();
    let mut motions = Vec::new();
    // One animation for each clip, blow and hook: two attacks that share a clip but deal their
    // blow with another part, or reach another distance, are two animations.
    let mut made: BTreeMap<AnimKey, (DataId, usize)> = BTreeMap::new();
    // Each animation's first and last pose, for the links into and out of it.
    let mut poses: Poses = BTreeMap::new();
    // Each stance's cycles' animations, by command, for the links between them.
    let mut starts: BTreeMap<(u32, u32), DataId> = BTreeMap::new();
    // Each stance's actions, linked from its Ready once every Ready is known.
    let mut actions: Vec<(MotionCommand, MotionCommand, AnimData)> = Vec::new();
    let mut death = None;
    let omega = ac2_turn_rate(ac2, t.entity).map_or(TURN_RATE, |r| -r);
    let mut turn = None;
    for m in &t.motion {
        let command = MotionCommand::from_name(&m.command)
            .ok_or_else(|| format!("{} is not a motion", m.command))?;
        // A state the creature stays in while it lasts loops; everything else plays once.
        let looped = class(command) & 0x40 != 0 && command != MotionCommand::DEAD;
        let dying = command == MotionCommand::DEAD;
        let clip = NativeClip::of(&keyframes(ac2, m.animation)?, !looped);
        let forward =
            command == MotionCommand::WALK_FORWARD || command == MotionCommand::RUN_FORWARD;
        let travel = forward.then(|| m.stride.unwrap_or_else(|| measured_stride(&b, &clip)));
        let hit = m.attack.as_ref().map(|a| {
            (
                m.hit.or_else(|| ac2_hit(ac2, m.animation)).unwrap_or(0.5),
                a,
            )
        });
        let key = (
            m.animation,
            hit.map_or(u64::MAX, |h| h.0.to_bits()),
            hit.map_or(u32::MAX, |h| h.1.part),
            hit.map_or(u64::MAX, |h| h.1.reach.to_bits()),
            dying,
        );
        let (anim_id, frames) = if let Some(done) = made.get(&key) {
            *done
        } else {
            let id = ids.next(0x03);
            let (a, settled) = animation(&b, &clip, id, hit, looped, travel, dying);
            if settled.is_some() {
                death = settled;
            }
            let frames = a.part_frames.len();
            poses.insert(
                id,
                (
                    a.part_frames[0].frames.clone(),
                    a.part_frames[frames - 1].frames.clone(),
                ),
            );
            out.add(
                id,
                a.encode_for(EraId::Eor)
                    .map_err(|e| format!("{id:?}: {e}"))?,
            );
            made.insert(key, (id, frames));
            (id, frames)
        };
        let rate = rate_of(&b, &clip, m, command, t.paced_at.unwrap_or(1.0));
        let fps = (RATE * rate) as f32;
        let whole = AnimData {
            anim_id,
            low_frame: 0,
            high_frame: -1,
            framerate: fps,
        };
        motions.push((
            m.stance
                .as_ref()
                .map_or(m.command.clone(), |s| format!("{} ({s})", m.command)),
            m.animation,
            anim_id,
            frames,
            rate,
            hit.map(|h| h.0),
        ));
        let styles = styles_of(t, m);
        for &style in &styles {
            if dying {
                // Into death, the whole clip; dead, its last frame held.
                let last = i32::try_from(frames.saturating_sub(1)).unwrap_or(0);
                cycles.push(entry(
                    dereth_animation::MotionTable::key(style, command),
                    vec![AnimData {
                        anim_id,
                        low_frame: last,
                        high_frame: last,
                        framerate: fps,
                    }],
                    None,
                ));
                actions.push((style, command, whole));
            } else if looped {
                // A state the creature stays in while it lasts: a cycle.
                let omega =
                    (command == MotionCommand::TURN_RIGHT).then(|| m.omega.unwrap_or(omega));
                if omega.is_some() {
                    turn = omega;
                }
                cycles.push(entry(
                    dereth_animation::MotionTable::key(style, command),
                    vec![whole],
                    omega,
                ));
                starts.insert((style.0, command.0), anim_id);
            } else {
                // An action played once from Ready: a link.
                actions.push((style, command, whole));
            }
        }
        if let (Some(a), true) = (&m.attack, styles.contains(&MotionCommand::HAND_COMBAT)) {
            maneuvers.push(CombatManeuver {
                style: MotionCommand::HAND_COMBAT.0,
                attack_height: match a.height.as_str() {
                    "high" => 1,
                    "medium" => 2,
                    "low" => 3,
                    h => return Err(format!("{h} is not an attack height")),
                },
                attack_type: match a.kind.as_str() {
                    "punch" => 0x1,
                    "thrust" => 0x2,
                    "slash" => 0x4,
                    "kick" => 0x8,
                    k => return Err(format!("{k} is not an attack kind")),
                },
                min_skill_level: 0,
                motion: command.0,
            });
        }
    }
    // Poses nearer than this need no link.
    let close = close_for(size.height);
    let mut joins = Joins {
        b: &b,
        poses: &poses,
        made: BTreeMap::new(),
    };
    let ready = |style: MotionCommand| starts.get(&(style.0, MotionCommand::READY.0)).copied();
    // AC2 blends its motions where Asheron's Call plays a link between them: a short ease from
    // each stance's Ready into each of its other cycles and back.
    for style in STYLES {
        let Some(r) = ready(style) else { continue };
        for (&(_, other), &a) in starts
            .iter()
            .filter(|((s, c), _)| *s == style.0 && *c != MotionCommand::READY.0)
        {
            for (from, to, x, y) in [
                (MotionCommand::READY.0, other, (r, false), (a, false)),
                (other, MotionCommand::READY.0, (a, false), (r, false)),
            ] {
                let link = joins.join(x, y, LINK_FRAMES, ids, out)?;
                links
                    .entry(dereth_animation::MotionTable::key(
                        style,
                        MotionCommand(from),
                    ))
                    .or_default()
                    .push(entry(to, vec![link], None));
            }
        }
    }
    // Between the stances, from one Ready to the other, where they stand apart.
    let mut stances = None;
    if let (Some(peace), Some(combat)) = (
        ready(MotionCommand::NON_COMBAT),
        ready(MotionCommand::HAND_COMBAT),
    ) {
        let d = apart(&b, &poses[&peace].0, &poses[&combat].0);
        if d > close {
            for (from, to, x, y) in [
                (
                    MotionCommand::NON_COMBAT,
                    MotionCommand::HAND_COMBAT,
                    peace,
                    combat,
                ),
                (
                    MotionCommand::HAND_COMBAT,
                    MotionCommand::NON_COMBAT,
                    combat,
                    peace,
                ),
            ] {
                let link = joins.join((x, false), (y, false), STANCE_FRAMES, ids, out)?;
                links
                    .entry(dereth_animation::MotionTable::key(
                        from,
                        MotionCommand::READY,
                    ))
                    .or_default()
                    .push(entry(to.0, vec![link], None));
            }
        }
        stances = Some(d);
    }
    // Each action from its stance's Ready: eased into from the Ready's pose where it starts
    // elsewhere, and back to it where it ends elsewhere (a death stays where it ends).
    let mut eased = Vec::new();
    for (style, command, whole) in actions {
        let mut anims = vec![whole];
        let (mut into, mut back) = (None, None);
        if let Some(r) = ready(style) {
            let (first, last) = &poses[&whole.anim_id];
            let d = apart(&b, &poses[&r].0, first);
            if d > close {
                anims.insert(
                    0,
                    joins.join((r, false), (whole.anim_id, false), LINK_FRAMES, ids, out)?,
                );
                into = Some(d);
            }
            let d = apart(&b, last, &poses[&r].0);
            if command != MotionCommand::DEAD && d > close {
                anims.push(joins.join((whole.anim_id, true), (r, false), LINK_FRAMES, ids, out)?);
                back = Some(d);
            }
        }
        eased.push((style, command, into, back));
        links
            .entry(dereth_animation::MotionTable::key(
                style,
                MotionCommand::READY,
            ))
            .or_default()
            .push(entry(command.0, anims, None));
    }
    let table = MotionTable {
        id: mtable_id,
        default_style: MotionCommand::NON_COMBAT.0,
        style_defaults: STYLES
            .iter()
            .map(|s| (s.0, MotionCommand::READY.0))
            .collect(),
        cycles,
        modifiers: Vec::new(),
        links,
    };
    out.add(mtable_id, table.encode());
    let combat = CombatManeuverTable {
        id: ctable_id,
        maneuvers,
    };
    out.add(ctable_id, combat.encode());
    Ok(Made {
        setup: setup_id,
        motion_table: mtable_id,
        combat_table: ctable_id,
        skinned_body: body.id,
        parts: b.parts.len(),
        vertices,
        triangles,
        levels,
        surfaces: (surfaces.len(), surfaces.iter().filter(|s| s.1).count()),
        motions,
        looks,
        size: Size {
            ac2: (f64::from(ac2_size.radius), f64::from(ac2_size.height)),
            made: size,
        },
        turn: turn.unwrap_or(omega),
        stances,
        eased: eased
            .into_iter()
            .map(|(s, c, into, back)| {
                (
                    stance_name(s).to_owned(),
                    c.name().unwrap_or("?").to_owned(),
                    into,
                    back,
                )
            })
            .collect(),
        death,
    })
}

/// How far apart two poses of a body `height` meters tall may stand and need no link between
/// them: a few hundredths of its height, and at least a tenth of a meter.
fn close_for(height: f64) -> f64 {
    (height * 0.03).max(0.1)
}

/// A stance's name in a table: `peace` or `combat`.
fn stance_name(style: MotionCommand) -> &'static str {
    if style == MotionCommand::HAND_COMBAT {
        "combat"
    } else {
        "peace"
    }
}

/// The stances a row plays in: its own, or else each stance with no row of its own for the
/// command.
fn styles_of(t: &Table, m: &Motion) -> Vec<MotionCommand> {
    let style_of = |m: &Motion| {
        m.stance.as_deref().map(|s| {
            if s == "combat" {
                MotionCommand::HAND_COMBAT
            } else {
                MotionCommand::NON_COMBAT
            }
        })
    };
    match style_of(m) {
        Some(s) => vec![s],
        None => STYLES
            .into_iter()
            .filter(|&s| {
                !t.motion
                    .iter()
                    .any(|o| o.command == m.command && style_of(o) == Some(s))
            })
            .collect(),
    }
}

/// What one motion's animation is made from: its clip, its blow's share, part and reach, and
/// whether it is a death.
type AnimKey = (u32, u64, u32, u64, bool);

/// Each animation made: its first pose and its last.
type Poses = BTreeMap<DataId, (Vec<Frame>, Vec<Frame>)>;

/// The links made between two poses, each an animation's first or last (`true`), once each.
struct Joins<'a> {
    b: &'a Bound,
    poses: &'a Poses,
    made: BTreeMap<(DataId, bool, DataId, bool, usize), DataId>,
}

impl Joins<'_> {
    /// The link from `from`'s pose to `to`'s over `count` frames, made the first time it is
    /// asked for.
    fn join(
        &mut self,
        from: (DataId, bool),
        to: (DataId, bool),
        count: usize,
        ids: &mut Ids,
        out: &mut Records,
    ) -> Result<AnimData, String> {
        let pose = |(id, end): (DataId, bool)| {
            let (first, last) = &self.poses[&id];
            if end {
                last
            } else {
                first
            }
        };
        let key = (from.0, from.1, to.0, to.1, count);
        let anim_id = if let Some(id) = self.made.get(&key) {
            *id
        } else {
            let id = ids.next(0x03);
            out.add(
                id,
                blend(self.b, pose(from), pose(to), id, count)
                    .encode_for(EraId::Eor)
                    .map_err(|e| format!("{id:?}: {e}"))?,
            );
            self.made.insert(key, id);
            id
        };
        #[allow(clippy::cast_possible_truncation)] // LINT-OK: single precision.
        Ok(AnimData {
            anim_id,
            low_frame: 0,
            high_frame: -1,
            framerate: RATE as f32,
        })
    }
}

/// How much faster than AC2's own a motion's clip is played: a walk or a run at the pace that
/// sets its feet down at the speed Asheron's Call moves the creature at, drawn at `scale` of AC2's
/// size; anything else at its row's rate.
fn rate_of(b: &Bound, clip: &NativeClip, m: &Motion, command: MotionCommand, scale: f64) -> f64 {
    let speed = if command == MotionCommand::WALK_FORWARD {
        WALK_SPEED
    } else if command == MotionCommand::RUN_FORWARD {
        RUN_SPEED
    } else {
        return m.rate.unwrap_or(1.0);
    };
    let stride = m.stride.unwrap_or_else(|| measured_stride(b, clip)) * scale;
    let period = clip.period().max(1e-6);
    if stride <= 1e-6 {
        return 1.0;
    }
    speed / (stride / period)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: an AC2 creature made into Asheron's Call's records)
    use super::*;
    use dereth_assets::Decode;

    /// The kept table `name`, made from this machine's AC2 install.
    fn made(name: &str) -> (Made, Records) {
        let text = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join(format!("ac2-creatures/{name}.toml")),
        )
        .expect("the table");
        let t = Table::parse(&text).expect("the table reads");
        let ac2 = Ac2::open(&auberean_dat::testing::require_target_install()).expect("opens");
        let mut out = Records::default();
        let made = make(&ac2, &t, &mut Ids::from(0xE000), &mut out).expect("the creature is made");
        (made, out)
    }

    fn tyrant() -> (Made, Records) {
        made("tyrant")
    }

    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the AC2 install: --features retail-dats"
    )]
    fn each_coarser_level_starts_at_ac2s_distance_times_the_tables_scale() {
        let text = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("ac2-creatures/beetle.toml"),
        )
        .expect("the table");
        let ac2 = Ac2::open(&auberean_dat::testing::require_target_install()).expect("opens");
        let kept = Table::parse(&text).expect("the table reads");
        assert_eq!(kept.detail_distance_scale, None);
        let theirs = bind(&ac2, &kept).expect("binds").native.lower_from;
        assert!(!theirs.is_empty(), "the beetle has coarser levels");
        #[allow(clippy::cast_possible_truncation)]
        let scaled = |scale: f64| -> Vec<f32> {
            theirs
                .iter()
                .map(|&d| (f64::from(d) * scale) as f32)
                .collect()
        };
        // Left out, the default scale; set, the table's own.
        let set = Table::parse(&text.replace(
            "picture = 512\n",
            "picture = 512\ndetail_distance_scale = 2.0\n",
        ))
        .expect("the table reads");
        assert_eq!(set.detail_distance_scale, Some(2.0));
        for (t, scale) in [(&kept, DETAIL_DISTANCE_SCALE), (&set, 2.0)] {
            let mut out = Records::default();
            let made = make(&ac2, t, &mut Ids::from(0xE000), &mut out).expect("made");
            let body: SkinnedBody = read(&out, made.skinned_body);
            assert_eq!(body.levels, scaled(scale), "at {scale}");
            assert_eq!(made.levels, body.levels);
        }
    }

    /// The body part each of a table's attacks deals its blow with, by its motion.
    fn blow_parts(made: &Made, out: &Records) -> BTreeMap<u32, u32> {
        let table: MotionTable = read(out, made.motion_table);
        let from_ready = &table.links
            [&dereth_animation::MotionTable::key(MotionCommand::HAND_COMBAT, MotionCommand::READY)];
        from_ready
            .iter()
            .filter_map(|l| {
                l.anims.iter().find_map(|x| {
                    let a: Animation = read(out, x.anim_id);
                    a.part_frames
                        .iter()
                        .flat_map(|f| f.hooks.iter())
                        .find_map(|h| match &h.data {
                            HookData::Attack(c) => Some((l.key, c.part_index)),
                            _ => None,
                        })
                })
            })
            .collect()
    }

    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the AC2 install: --features retail-dats"
    )]
    fn attacks_that_share_a_clip_each_deal_their_blow_with_their_own_part() {
        // The crone plays one clip for all three heights: the high one bites, the others claw.
        let (m, out) = made("crone");
        let parts = blow_parts(&m, &out);
        assert_eq!(parts.get(&0x1000_0062), Some(&0), "{parts:X?}");
        assert_eq!(parts.get(&0x1000_0063), Some(&20), "{parts:X?}");
        assert_eq!(parts.get(&0x1000_0064), Some(&20), "{parts:X?}");
    }

    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the AC2 install: --features retail-dats"
    )]
    fn each_tyrant_look_swaps_the_bodys_textures_for_pictures_of_its_own() {
        let (made, out) = tyrant();
        let body: SkinnedBody = read(&out, made.skinned_body);
        assert!(made.looks.len() >= 6, "{} looks", made.looks.len());
        let mut seen = std::collections::BTreeSet::new();
        for l in &made.looks {
            assert_eq!((l.swapped, l.unswapped), (0, 0), "{}", l.name);
            assert!(!l.textures.is_empty(), "{} swaps nothing", l.name);
            for (old, new) in &l.textures {
                // Each old texture is one the body draws with, and each new one is written.
                assert!(
                    body.surfaces.iter().any(|s| {
                        let surface: Surface = read(&out, *s);
                        surface.orig_texture_id == Some(*old)
                    }),
                    "{}: {old:?}",
                    l.name
                );
                assert!(out.0.contains_key(new), "{}: {new:?}", l.name);
            }
            assert!(seen.insert(l.textures.clone()), "{} repeats a look", l.name);
        }
    }

    fn read<T: Decode>(out: &Records, id: DataId) -> T {
        T::decode_payload(id, &out.0[&id]).unwrap_or_else(|e| panic!("{id:?}: {e}"))
    }

    /// A client that draws a creature's parts, not its skinned body, draws a look on every part
    /// whose model draws a texture the look swaps: the swap is laid on each such part, and every
    /// swap on the first part, which the skinned body draws by.
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the AC2 install: --features retail-dats"
    )]
    fn each_look_swaps_its_textures_on_every_part_whose_model_draws_them() {
        let (made, out) = tyrant();
        let setup: Setup = read(&out, made.setup);
        let drawn: Vec<Vec<DataId>> = setup
            .parts
            .iter()
            .map(|g| {
                let model: GfxObj = read(&out, *g);
                model
                    .surfaces
                    .iter()
                    .filter_map(|s| read::<Surface>(&out, *s).orig_texture_id)
                    .collect()
            })
            .collect();
        for l in &made.looks {
            assert!(l.parts.len() <= MOST_SWAPS, "{}: {}", l.name, l.parts.len());
            for &(old, new) in &l.textures {
                assert!(l.parts.contains(&(0, old, new)), "{}: part 0", l.name);
                for (part, textures) in drawn.iter().enumerate().skip(1) {
                    let part = u16::try_from(part).unwrap();
                    assert_eq!(
                        l.parts.contains(&(part, old, new)),
                        textures.contains(&old),
                        "{}: part {part} {old:?}",
                        l.name
                    );
                }
            }
            let on = drawn.iter().skip(1).filter(|t| !t.is_empty()).count();
            assert!(on > 1, "the tyrant's look reaches more than its first part");
        }
    }

    /// A look's weenie gets its swaps written part by part under its own id, the rest of it as
    /// it was.
    #[test]
    fn a_look_weenies_swaps_are_written_part_by_part_under_its_own_id() {
        let sql = "/* x */\nINSERT INTO `weenie` VALUES (7);\n\n/* Its pictures. */\n\
                   INSERT INTO `weenie_properties_texture_map` (`object_Id`, `index`, `old_Id`, `new_Id`)\n\
                   VALUES ({{wcid:t2}}, 0, 0x0500E000, 0x0500E003);\n\nINSERT INTO `after` VALUES (1);\n";
        let parts = [
            (0, DataId(0x0500_E000), DataId(0x0500_E003)),
            (4, DataId(0x0500_E000), DataId(0x0500_E003)),
        ];
        let out = super::super::weenie_swaps(sql, &parts).expect("it has swaps");
        assert_eq!(
            out,
            "/* x */\nINSERT INTO `weenie` VALUES (7);\n\n/* Its pictures. */\n\
             INSERT INTO `weenie_properties_texture_map` (`object_Id`, `index`, `old_Id`, `new_Id`)\n\
             VALUES ({{wcid:t2}}, 0, 0x0500E000, 0x0500E003)\n     , ({{wcid:t2}}, 4, 0x0500E000, 0x0500E003);\n\n\
             INSERT INTO `after` VALUES (1);\n"
        );
        assert_eq!(
            super::super::weenie_swaps(&out, &parts).as_deref(),
            Some(out.as_str())
        );
        assert!(super::super::weenie_swaps("INSERT INTO `weenie` VALUES (7);", &parts).is_none());
    }

    /// Each level of the body draws, mesh by mesh, every triangle of AC2's own level for that
    /// mesh, and every mesh draws at every level: a mesh with fewer coarse levels than another
    /// draws its coarsest past them, so the body's meshes switch together and none drops out.
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the AC2 install: --features retail-dats"
    )]
    fn each_level_draws_all_of_ac2s_own_level_of_every_mesh() {
        let ac2 = Ac2::open(&auberean_dat::testing::require_target_install()).expect("opens");
        // The tyrant (one mesh) and the flayer (a body and two meshes with a level fewer).
        for name in ["tyrant", "flayer"] {
            let (made, _) = made(name);
            let text = std::fs::read_to_string(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join(format!("ac2-creatures/{name}.toml")),
            )
            .expect("the table");
            let t = Table::parse(&text).expect("the table reads");
            let vd = super::super::probe::description(&ac2, t.entity).expect("its description");
            let groups = vd.part_groups.unwrap_or_default();
            let (_, body) = groups
                .iter()
                .find(|(k, _)| *k == 0x4000_0001)
                .expect("a body group");
            let s: Ac2Setup = ac2.record(body.setup.expect("a setup").0).expect("reads");
            // AC2's own triangles a mesh, at its fullest and each coarser level it draws at
            // high model detail.
            let theirs: Vec<Vec<usize>> = s
                .meshes
                .iter()
                .filter_map(|m| ac2.record::<Ac2Mesh>(m.0))
                .filter(|m| !m.levels.is_empty() && m.vertices.count >= 4)
                .map(|m| {
                    let count = |l: &auberean_assets::mesh::DetailLevel| {
                        l.fragments
                            .iter()
                            .map(|f| f.indices.len() / 3)
                            .sum::<usize>()
                    };
                    let best = (0..m.levels.len())
                        .max_by_key(|&k| (count(&m.levels[k]), std::cmp::Reverse(k)))
                        .unwrap();
                    std::iter::once(count(&m.levels[best]))
                        .chain(
                            (best + 1..m.levels.len())
                                .filter(|&k| {
                                    m.levels[k].detail & crate::ac2_look::creature::HIGH_DETAIL != 0
                                })
                                .map(|k| count(&m.levels[k])),
                        )
                        .collect()
                })
                .collect();
            let levels = theirs.iter().map(Vec::len).max().unwrap();
            let want: Vec<usize> = (0..levels)
                .map(|j| {
                    theirs
                        .iter()
                        .map(|m| m.get(j).or(m.last()).copied().unwrap_or(0))
                        .sum()
                })
                .collect();
            assert_eq!(made.triangles, want, "{name}: {theirs:?}");
        }
    }

    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the AC2 install: --features retail-dats"
    )]
    fn the_tyrants_records_read_back_and_agree_with_each_other() {
        let (made, out) = tyrant();
        let setup: Setup = read(&out, made.setup);
        let body: SkinnedBody = read(&out, made.skinned_body);
        let table: MotionTable = read(&out, made.motion_table);
        let combat: CombatManeuverTable = read(&out, made.combat_table);
        // The body is bound to every part, and every weight names one.
        assert_eq!(body.setup, made.setup);
        assert_eq!(body.bind.len(), setup.parts.len());
        assert!(body
            .vertices
            .iter()
            .all(|v| v.parts.iter().all(|&p| usize::from(p) < setup.parts.len())));
        // Every animation the table plays moves every part.
        let anims = table
            .cycles
            .iter()
            .chain(table.links.values().flatten())
            .flat_map(|m| m.anims.iter());
        for a in anims {
            let anim: Animation = read(&out, a.anim_id);
            assert_eq!(anim.num_parts as usize, setup.parts.len());
        }
        // The walk carries the body forward; the attacks the server can choose land a blow.
        let walk = table
            .cycles
            .iter()
            .find(|m| {
                m.key
                    == dereth_animation::MotionTable::key(
                        MotionCommand::HAND_COMBAT,
                        MotionCommand::WALK_FORWARD,
                    )
            })
            .expect("a walk");
        let w: Animation = read(&out, walk.anims[0].anim_id);
        assert!(w
            .pos_frames
            .is_some_and(|f| f.iter().all(|p| p.origin.y > 0.0)));
        assert_eq!(combat.maneuvers.len(), 3);
        let from_ready = &table.links
            [&dereth_animation::MotionTable::key(MotionCommand::HAND_COMBAT, MotionCommand::READY)];
        for m in &combat.maneuvers {
            let attack = from_ready
                .iter()
                .find(|l| l.key == m.motion)
                .expect("each attack is played from Ready");
            let hooks: usize = attack
                .anims
                .iter()
                .map(|x| {
                    let a: Animation = read(&out, x.anim_id);
                    a.part_frames
                        .iter()
                        .flat_map(|f| f.hooks.iter())
                        .filter(|h| matches!(h.data, HookData::Attack(_)))
                        .count()
                })
                .sum();
            assert_eq!(hooks, 1, "{:08X}", m.motion);
        }
    }

    /// The motion table's cycle for `command` in `style`.
    fn cycle(table: &MotionTable, style: MotionCommand, command: MotionCommand) -> &MotionData {
        let key = dereth_animation::MotionTable::key(style, command);
        table
            .cycles
            .iter()
            .find(|c| c.key == key)
            .unwrap_or_else(|| panic!("no {command:?} in {style:?}"))
    }

    /// The link from `from` in `style` to `to`.
    fn link(
        table: &MotionTable,
        style: MotionCommand,
        from: MotionCommand,
        to: MotionCommand,
    ) -> Option<&MotionData> {
        table
            .links
            .get(&dereth_animation::MotionTable::key(style, from))?
            .iter()
            .find(|l| l.key == to.0)
    }

    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the AC2 install: --features retail-dats"
    )]
    fn a_combat_ready_idles_in_its_own_pose_and_the_stances_and_attacks_ease_into_it() {
        let (made, out) = made("mimbu");
        let table: MotionTable = read(&out, made.motion_table);
        let first = |id: DataId| {
            let a: Animation = read(&out, id);
            a.part_frames[0].frames.clone()
        };
        let last = |id: DataId| {
            let a: Animation = read(&out, id);
            a.part_frames[a.part_frames.len() - 1].frames.clone()
        };
        let b = {
            let text = std::fs::read_to_string(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("ac2-creatures/mimbu.toml"),
            )
            .expect("the table");
            let ac2 = Ac2::open(&auberean_dat::testing::require_target_install()).expect("opens");
            bind(&ac2, &Table::parse(&text).expect("reads")).expect("binds")
        };
        let (peace, combat) = (MotionCommand::NON_COMBAT, MotionCommand::HAND_COMBAT);
        let peace_ready = cycle(&table, peace, MotionCommand::READY).anims[0].anim_id;
        let combat_ready = cycle(&table, combat, MotionCommand::READY).anims[0].anim_id;
        assert_ne!(peace_ready, combat_ready, "the combat Ready is its own");
        // Each way between the stances is a link from one Ready's pose to the other's.
        for (from, to, x, y) in [
            (peace, combat, peace_ready, combat_ready),
            (combat, peace, combat_ready, peace_ready),
        ] {
            let l = link(&table, from, MotionCommand::READY, to).expect("a stance link");
            assert_eq!(l.anims.len(), 1);
            let ease = l.anims[0].anim_id;
            assert_eq!(
                read::<Animation>(&out, ease).num_frames as usize,
                STANCE_FRAMES
            );
            assert!(
                apart(&b, &last(ease), &first(y)) < 1e-3,
                "it ends in the other Ready's pose"
            );
            assert!(
                apart(&b, &first(ease), &first(x)) > 1e-3,
                "it moves from the first frame"
            );
        }
        // Every attack in combat starts from the combat Ready's pose and returns to it: the
        // link's first and last frames are within a few centimeters of it.
        let ready_pose = first(combat_ready);
        let close = close_for(made.size.made.height);
        for attack in [
            MotionCommand::ATTACK_HIGH1,
            MotionCommand::ATTACK_MED1,
            MotionCommand::ATTACK_LOW1,
        ] {
            let l = link(&table, combat, MotionCommand::READY, attack).expect("an attack");
            // Either the clip starts at the Ready's pose, or a link eases into it from there.
            let start = apart(&b, &first(l.anims[0].anim_id), &ready_pose);
            let clip_start = l
                .anims
                .iter()
                .find(|x| read::<Animation>(&out, x.anim_id).num_frames as usize != LINK_FRAMES)
                .map(|x| apart(&b, &first(x.anim_id), &ready_pose))
                .expect("the attack's own clip");
            assert!(
                start <= close || start < clip_start / 2.0,
                "{attack:?} starts {start:.2} m from the Ready (its clip {clip_start:.2} m)"
            );
            let end = last(l.anims[l.anims.len() - 1].anim_id);
            assert!(
                apart(&b, &end, &ready_pose) <= close,
                "{attack:?} ends apart"
            );
        }
        // It turns at AC2's own rate for the body, 200 degrees a second.
        let turn = cycle(&table, peace, MotionCommand::TURN_RIGHT);
        let omega = turn.omega.expect("a turn rate").z;
        assert!(
            (f64::from(omega) + 200f64.to_radians()).abs() < 1e-3,
            "{omega}"
        );
    }

    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the AC2 install: --features retail-dats"
    )]
    fn a_death_plays_where_the_creature_stands_and_ends_on_the_ground() {
        // The scraven falls back five meters in AC2; the mimbu's fin ends half a meter under the
        // ground.
        for name in ["scraven", "mimbu"] {
            let text = std::fs::read_to_string(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join(format!("ac2-creatures/{name}.toml")),
            )
            .expect("the table");
            let t = Table::parse(&text).expect("reads");
            let ac2 = Ac2::open(&auberean_dat::testing::require_target_install()).expect("opens");
            let b = bind(&ac2, &t).expect("binds");
            let mut out = Records::default();
            let made = make(&ac2, &t, &mut Ids::from(0xE000), &mut out).expect("made");
            let (carried, _) = made.death.expect("a death");
            assert!(
                carried > 1.0,
                "{name}: AC2's death carries it {carried:.2} m"
            );
            let table: MotionTable = read(&out, made.motion_table);
            let dead = cycle(&table, MotionCommand::NON_COMBAT, MotionCommand::DEAD).anims[0];
            let a: Animation = read(&out, dead.anim_id);
            let frame = |k: usize| a.part_frames[k].frames.clone();
            let middle = |f: &[Frame]| {
                let v = b.skin(f);
                v.iter().sum::<DVec3>() / v.len() as f64
            };
            let held = usize::try_from(dead.low_frame).expect("a frame");
            assert_eq!(held, a.part_frames.len() - 1, "the last frame is held");
            let moved = (middle(&frame(held)) - middle(&frame(0)))
                .truncate()
                .length();
            assert!(
                moved < 0.01,
                "{name}: the corpse lies {moved:.3} m from where it stood"
            );
            // It lies on the ground, all but a sliver of its skin (the mimbu's fin) above it.
            let ground = lowest_but(&b, &frame(held), SUNK);
            assert!(
                ground.abs() < 0.01,
                "{name}: its body lies at {ground:.3} m"
            );
            let lowest = b.bounds(&frame(held)).0.z;
            if name == "mimbu" {
                assert!(lowest < -0.3, "its fin holds it up ({lowest:.3} m)");
            }
        }
    }

    /// The mean red, green and blue (0 to 1) of the texture a look swaps in for its first.
    fn swapped_in_mean(made: &Made, out: &Records, look: &str) -> [f64; 3] {
        let l = made
            .looks
            .iter()
            .find(|l| l.name == look)
            .unwrap_or_else(|| panic!("no {look}"));
        let (_, new) = l.textures[0];
        let st: dereth_assets::SurfaceTexture = read(out, new);
        let rs_id = st.source_levels[0];
        let rs: dereth_assets::RenderSurface = read(out, rs_id);
        let px = rs.payload(&out.0[&rs_id]).expect("its pixels");
        assert_eq!(rs.format, dereth_ac2_convert::records::PFID_A8R8G8B8);
        let mut sum = [0.0f64; 3];
        for t in px.chunks(4) {
            // Blue, green, red, alpha.
            for (c, k) in [(0, 2), (1, 1), (2, 0)] {
                sum[c] += f64::from(t[k]) / 255.0;
            }
        }
        #[allow(clippy::cast_precision_loss)]
        sum.map(|s| s / (px.len() / 4) as f64)
    }

    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the AC2 install: --features retail-dats"
    )]
    fn a_looks_color_is_baked_into_its_picture_and_its_see_through_color_carried() {
        // The tyrant's fifth look is a see-through cyan ghost: its picture tinted cyan, and its
        // weenie drawn 60% see-through.
        let (made, out) = tyrant();
        let ghost = made
            .looks
            .iter()
            .find(|l| l.name == "tyrant-5")
            .expect("look 5");
        assert!(
            ghost.translucency.is_some_and(|t| (t - 0.6).abs() < 1e-3),
            "{:?}",
            ghost.translucency
        );
        let [r, g, b] = swapped_in_mean(&made, &out, "tyrant-5");
        assert!(b > r * 1.5 && g > r * 1.5, "{r:.2} {g:.2} {b:.2}");
        // Its other looks are opaque.
        assert!(made
            .looks
            .iter()
            .filter(|l| l.name != "tyrant-5")
            .all(|l| l.translucency.is_none()));
        // The beetle's sixth look is an unlit shadow: near black.
        let (made, out) = made_named("beetle");
        let shadow = swapped_in_mean(&made, &out, "beetle-6");
        assert!(shadow.iter().all(|c| *c < 0.15), "{shadow:.2?}");
    }

    fn made_named(name: &str) -> (Made, Records) {
        made(name)
    }

    #[test]
    fn a_bodys_radius_is_its_farthest_reach_forward_or_sideways() {
        // Four meters in front, five behind, a meter and a half each side, two and a half high.
        let (lo, hi) = (DVec3::new(-1.5, -5.0, -0.1), DVec3::new(1.5, 4.0, 2.5));
        let c = collision(lo, hi, None, 1.0);
        assert!((c.radius - 4.0).abs() < 1e-9, "{}", c.radius);
        assert!((c.height - 2.5).abs() < 1e-9, "{}", c.height);
        // A blow that lands farther out sets it; one that lands nearer does not.
        assert!((collision(lo, hi, Some(4.6), 1.0).radius - 4.6).abs() < 1e-9);
        assert!((collision(lo, hi, Some(1.0), 1.0).radius - 4.0).abs() < 1e-9);
        // A body wider than it is long reaches farthest to its side.
        let wide = collision(
            DVec3::new(-3.0, -0.5, 0.0),
            DVec3::new(2.0, 0.5, 1.0),
            None,
            1.0,
        );
        assert!((wide.radius - 3.0).abs() < 1e-9, "{}", wide.radius);
    }

    /// Each sphere of `c` as (x, y, z, r), rounded to millimeters.
    fn spheres_mm(c: &Collision) -> Vec<[i64; 4]> {
        #[allow(clippy::cast_possible_truncation)] // LINT-OK: millimeters of a few meters.
        let mm = |v: f64| (v * 1000.0).round() as i64;
        c.spheres
            .iter()
            .map(|(p, r)| [mm(p.x), mm(p.y), mm(p.z), mm(*r)])
            .collect()
    }

    #[test]
    fn a_bodys_spheres_stand_on_its_middle_one_for_a_squat_body_two_stacked_for_a_tall_one() {
        // A squat body, as tall as two of its spheres: one, on the ground at its middle.
        let squat = collision(
            DVec3::new(-1.0, -1.5, 0.0),
            DVec3::new(1.0, 1.0, 2.0),
            None,
            1.0,
        );
        assert_eq!(spheres_mm(&squat), vec![[0, 0, 1000, 1000]]);
        // A tall one: two, the lower on the ground and the upper to its top.
        let tall = collision(
            DVec3::new(-0.4, -0.3, -0.1),
            DVec3::new(0.4, 0.5, 2.0),
            None,
            0.5,
        );
        assert_eq!(spheres_mm(&tall), vec![[0, 0, 500, 500], [0, 0, 1500, 500]]);
        // A girth more than half the body's height is cut to it, so no sphere stands taller.
        let low = collision(
            DVec3::new(-1.0, -1.0, 0.0),
            DVec3::new(1.0, 1.0, 1.0),
            None,
            0.8,
        );
        assert_eq!(spheres_mm(&low), vec![[0, 0, 500, 500]]);
        // Never more than the clients collide with.
        for c in [&squat, &tall, &low] {
            assert!(c.spheres.len() <= MOST_SPHERES);
        }
    }

    #[test]
    fn a_long_bodys_second_sphere_lies_forward_to_its_head_touching_the_first() {
        // A head 3.6 m ahead of the middle, 2.6 m high, 3 m wide, girth 0.9.
        let c = collision(
            DVec3::new(-1.5, -4.5, 0.0),
            DVec3::new(1.5, 3.6, 2.6),
            None,
            0.9,
        );
        // Each 1.2 m (a third of the reach), one at the middle and one whose front is the head.
        assert_eq!(
            spheres_mm(&c),
            vec![[0, 0, 1200, 1200], [0, 2400, 1200, 1200]]
        );
        let ((a, ra), (b, rb)) = (c.spheres[0], c.spheres[1]);
        assert!(a.distance(b) <= ra + rb + 1e-9, "a gap between them");
    }

    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the AC2 install: --features retail-dats"
    )]
    fn a_long_bodys_radius_reaches_its_head_and_its_blow() {
        // The mucor stretches far in front of its middle: AC2's radius stops short of its head.
        let (made, _) = made("mucor");
        let s = &made.size;
        assert!(s.made.radius > 3.5, "radius {:.2}", s.made.radius);
        assert!(s.made.radius > s.ac2.0 * 2.0, "AC2's {:.2}", s.ac2.0);
    }

    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the AC2 install: --features retail-dats"
    )]
    fn a_made_setup_collides_with_spheres_sized_from_its_skin() {
        for name in ["tyrant", "mucor", "crone", "beetle"] {
            let (made, out) = made(name);
            let setup: Setup = read(&out, made.setup);
            let c = &made.size.made;
            // Spheres, as Asheron's Call's creatures have, and no cylinders.
            assert!(setup.cylspheres.is_empty(), "{name}");
            assert!(
                (1..=MOST_SPHERES).contains(&setup.spheres.len()),
                "{name}: {} spheres",
                setup.spheres.len()
            );
            assert_eq!(setup.spheres.len(), c.spheres.len(), "{name}");
            for (s, (center, r)) in setup.spheres.iter().zip(&c.spheres) {
                assert!((f64::from(s.radius) - r).abs() < 1e-4, "{name}");
                assert!((f64::from(s.center.z) - center.z).abs() < 1e-4, "{name}");
                // On the body's middle line, on the ground, no higher than the body.
                assert!(s.center.x.abs() < 1e-6, "{name}: {s:?}");
                assert!(s.center.z >= s.radius - 1e-4, "{name}: below the ground");
                assert!(
                    f64::from(s.center.z + s.radius) <= c.height + 1e-4,
                    "{name}: above its top"
                );
                // Well inside its melee reach.
                assert!(
                    f64::from(s.center.y + s.radius) <= c.radius + 1e-4,
                    "{name}"
                );
            }
            assert!((f64::from(setup.height) - c.height).abs() < 1e-4, "{name}");
            assert!((f64::from(setup.radius) - c.radius).abs() < 1e-4, "{name}");
        }
        // The tyrant is one sphere as wide as AC2's own and squat enough to fill its height.
        let (tyrant, _) = made("tyrant");
        let t = &tyrant.size.made;
        assert_eq!(t.spheres.len(), 1);
        assert!((t.spheres[0].1 - 5.49).abs() < 0.01, "{:?}", t.spheres);
        // The mucor's head reaches far ahead: its second sphere lies forward and fronts the head.
        let (mucor, _) = made("mucor");
        let m = &mucor.size.made;
        assert_eq!(m.spheres.len(), 2);
        let (front, r) = m.spheres[1];
        assert!(front.y > 2.0 && front.z < 1.5, "{:?}", m.spheres);
        assert!((front.y + r - 3.59).abs() < 0.01, "{:?}", m.spheres);
        // The crone stands taller than two of its spheres: two, stacked.
        let (crone, _) = made("crone");
        let c = &crone.size.made;
        assert_eq!(c.spheres.len(), 2);
        assert!(c.spheres[0].0.y.abs() < 1e-9 && c.spheres[1].0.y.abs() < 1e-9);
        assert!(c.spheres[1].0.z > c.spheres[0].0.z);
    }
}
