//! Decals: a picture laid over the ground for a moment, such as the dark disc under an object at
//! the simple dynamic object shadows, or the pool of a carried light on the land.
//!
//! A decal is a box standing on the ground at a point: its width along a direction, its depth
//! across it and its height up. The ground's triangles about the point (outdoors the landscape's
//! squares round it, inside the cells' triangles near it) are clipped to the box, each piece fanned into triangles, lifted fifteen millimeters
//! and given picture coordinates across the box. Decals are kept by their material, in groups; each
//! frame a group's decals are drawn together with its material, their vertices colored by the
//! decal's color with its fade in its alpha. A decal drawn once is gone after its first draw; any
//! other lives out its lifespan, fading in over its first tenth (a quarter second at most) and out
//! over its last fifth (a second at most).

use std::collections::HashMap;
use std::sync::Arc;

use auberean_primitives::Vec3;
use auberean_world::color::Color;
use auberean_world::math::{cross, dot, scale, sub, Mat4};
use auberean_world::records::WorldRecords;

use crate::material::{draw_layers, instance_material_with, DrawLayer, DEVICE_FLAGS};
use crate::mesh::MeshVertex;
use crate::scene::{Cull, ObjectBatch, PointLights};

/// What a decal is made from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DecalDesc {
    /// Its material instance (0: none).
    pub material: u32,
    /// How long it lives, in seconds.
    pub lifespan: f32,
    /// Its box: along its direction, across it, up.
    pub size: Vec3,
    /// The way its width runs along the ground.
    pub direction: Vec3,
    pub color: Color,
    /// Drawn once and gone.
    pub draw_once: bool,
}

impl Default for DecalDesc {
    /// A white decal of no material, a meter on each side running east, living a second.
    // D2-06879
    fn default() -> Self {
        Self {
            material: 0,
            lifespan: 1.0,
            size: Vec3::new(1.0, 1.0, 1.0),
            direction: Vec3::new(1.0, 0.0, 0.0),
            color: Color::WHITE,
            draw_once: false,
        }
    }
}

/// A point of the ground a decal is laid on: where it is and its normal.
pub type Corner = (Vec3, Vec3);

/// The polygon `points` cut by the plane `normal · p + d = 0`, keeping the side where it is not
/// negative: each point kept in order, a point where an edge crosses the plane put between a kept
/// point and a dropped one (place and normal alike taken along the edge). Nothing when every point
/// is dropped.
// D2-10336
#[must_use]
pub fn clip_polygon(normal: Vec3, d: f32, points: &[Corner]) -> Vec<Corner> {
    let side = |p: Vec3| dot(normal, p) + d;
    let out: Vec<bool> = points.iter().map(|(p, _)| side(*p) < 0.0).collect();
    if out.iter().all(|&o| o) {
        return Vec::new();
    }
    let along = |a: Corner, b: Corner| -> Corner {
        // the share of the way from `a` toward `b` where the plane is met
        let t = side(a.0) / dot(normal, sub(a.0, b.0));
        let mix = |x: Vec3, y: Vec3| {
            Vec3::new(
                (1.0 - t) * x.x + t * y.x,
                (1.0 - t) * x.y + t * y.y,
                (1.0 - t) * x.z + t * y.z,
            )
        };
        (mix(a.0, b.0), mix(a.1, b.1))
    };
    let mut kept = Vec::with_capacity(points.len() + 1);
    let n = points.len();
    for i in 0..n {
        let prev = (i + n - 1) % n;
        if out[i] {
            if !out[prev] {
                kept.push(along(points[prev], points[i]));
            }
        } else {
            if out[prev] {
                kept.push(along(points[i], points[prev]));
            }
            kept.push(points[i]);
        }
    }
    kept
}

/// One corner of a decal's triangle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DecalVertex {
    pub position: Vec3,
    pub normal: Vec3,
    pub uv: [f32; 2],
}

/// How far a decal is lifted off the ground it is laid on, in meters.
pub const DECAL_LIFT: f32 = 0.015;

/// A decal's triangles: the ground's `triangles` cut to the box about `center` whose width
/// (`size.x`) runs along `direction`, its depth (`size.y`) along `up × direction`, its height
/// (`size.z`) along `up`; each piece fanned from its first corner, its picture coordinates its
/// place across the box (0 to 1, the middle at one half), then lifted.
// D2-10343
#[must_use]
pub fn construct_decal(
    center: Vec3,
    up: Vec3,
    direction: Vec3,
    size: Vec3,
    triangles: &[[Corner; 3]],
) -> Vec<[DecalVertex; 3]> {
    let across = cross(up, direction);
    let planes = [
        (direction, 0.5 * size.x - dot(center, direction)),
        (
            scale(direction, -1.0),
            0.5 * size.x + dot(center, direction),
        ),
        (across, 0.5 * size.y - dot(center, across)),
        (scale(across, -1.0), 0.5 * size.y + dot(center, across)),
        (up, 0.5 * size.z - dot(center, up)),
        (scale(up, -1.0), 0.5 * size.z + dot(center, up)),
    ];
    let mut out = Vec::new();
    for t in triangles {
        let mut poly = t.to_vec();
        for (n, d) in planes {
            poly = clip_polygon(n, d, &poly);
            if poly.is_empty() {
                break;
            }
        }
        if poly.len() < 3 {
            continue;
        }
        let corner = |c: Corner| {
            let r = sub(c.0, center);
            DecalVertex {
                position: Vec3::new(c.0.x, c.0.y, c.0.z + DECAL_LIFT),
                normal: c.1,
                uv: [
                    dot(r, direction) / size.x + 0.5,
                    dot(r, across) / size.y + 0.5,
                ],
            }
        };
        for k in 1..poly.len() - 1 {
            out.push([corner(poly[0]), corner(poly[k]), corner(poly[k + 1])]);
        }
    }
    out
}

/// A decal on the landscape at `center`: the ground of the nine squares about it (its own and
/// those a square's side away along each axis and diagonal, each square's two triangles as
/// `square_at` gives the square holding a point), cut to the decal's box standing up from the
/// ground along its direction.
// D2-10344
#[must_use]
pub fn landscape_decal(
    center: Vec3,
    desc: &DecalDesc,
    square_side: f32,
    square_at: &dyn Fn(Vec3) -> Option<[Corner; 6]>,
) -> Vec<[DecalVertex; 3]> {
    let s = square_side;
    let offsets = [
        (0.0, 0.0),
        (-s, 0.0),
        (s, 0.0),
        (0.0, -s),
        (0.0, s),
        (-s, s),
        (-s, -s),
        (s, s),
        (s, -s),
    ];
    let mut ground = Vec::new();
    for (dx, dy) in offsets {
        if let Some(c) = square_at(Vec3::new(center.x + dx, center.y + dy, center.z)) {
            ground.push([c[0], c[1], c[2]]);
            ground.push([c[3], c[4], c[5]]);
        }
    }
    construct_decal(
        center,
        Vec3::new(0.0, 0.0, 1.0),
        desc.direction,
        desc.size,
        &ground,
    )
}

/// Whether the box about `points` meets the box from `lo` to `hi` (touching counts).
fn meets(points: &[Vec3], lo: Vec3, hi: Vec3) -> bool {
    let Some(first) = points.first() else {
        return false;
    };
    let (mut a, mut b) = (*first, *first);
    for p in points {
        a = Vec3::new(a.x.min(p.x), a.y.min(p.y), a.z.min(p.z));
        b = Vec3::new(b.x.max(p.x), b.y.max(p.y), b.z.max(p.z));
    }
    a.x <= hi.x && b.x >= lo.x && a.y <= hi.y && b.y >= lo.y && a.z <= hi.z && b.z >= lo.z
}

/// The ground about a point inside: the triangles of interior cell `low` of block `block` (its
/// mesh's most detailed level) whose box meets the cube of half side `reach` about `at` (in the
/// block's frame), each corner with its normal, in the block's frame; and once, the same of each
/// interior cell through a doorway of this one whose shape meets the cube.
// D2-03579
#[must_use]
pub fn nearby_geometry(
    records: &WorldRecords,
    block: (i32, i32),
    low: u16,
    at: Vec3,
    reach: f32,
) -> Vec<[Corner; 3]> {
    let mut out = Vec::new();
    cell_geometry(records, block, low, at, reach, true, &mut out);
    out
}

fn cell_geometry(
    records: &WorldRecords,
    block: (i32, i32),
    low: u16,
    at: Vec3,
    reach: f32,
    through_doorways: bool,
    out: &mut Vec<[Corner; 3]>,
) {
    let Some(cell) = records.cell(block.0, block.1, low) else {
        return;
    };
    let Some(mesh) = records.get::<auberean_assets::mesh::CellMesh>(cell.mesh.0) else {
        return;
    };
    let Some(vertices) = crate::mesh::vertices(&mesh.mesh.vertices) else {
        return;
    };
    let frame = Mat4::from_parts(
        cell.frame.origin,
        cell.frame.rotation,
        Vec3::new(1.0, 1.0, 1.0),
    );
    let here = frame.rigid_preimage(at);
    let lo = Vec3::new(here.x - reach, here.y - reach, here.z - reach);
    let hi = Vec3::new(here.x + reach, here.y + reach, here.z + reach);
    let place = |k: u16| {
        vertices
            .get(usize::from(k))
            .map(|v| Vec3::new(v.position[0], v.position[1], v.position[2]))
    };
    if let Some(level) = mesh.mesh.levels.first() {
        for f in &level.fragments {
            for t in f.indices.as_chunks::<3>().0 {
                let (Some(a), Some(b), Some(c)) = (
                    vertices.get(usize::from(t[0])),
                    vertices.get(usize::from(t[1])),
                    vertices.get(usize::from(t[2])),
                ) else {
                    continue;
                };
                let corners = [a, b, c].map(|v| {
                    (
                        Vec3::new(v.position[0], v.position[1], v.position[2]),
                        Vec3::new(v.normal[0], v.normal[1], v.normal[2]),
                    )
                });
                if meets(&corners.map(|c| c.0), lo, hi) {
                    out.push(corners.map(|(p, n)| (frame.point(p), frame.direction(n))));
                }
            }
        }
    }
    if !through_doorways {
        return;
    }
    for p in &cell.portals {
        if p.other_cell == low || !(0x100..0xFFFF).contains(&p.other_cell) {
            continue;
        }
        let shape: Vec<Vec3> = mesh
            .portals
            .get(usize::from(p.portal))
            .and_then(|d| mesh.mesh.physics_polygons.get(usize::from(d.polygon)))
            .map(|poly| poly.vertices.iter().filter_map(|&k| place(k)).collect())
            .unwrap_or_default();
        if meets(&shape, lo, hi) {
            cell_geometry(records, block, p.other_cell, at, reach, false, out);
        }
    }
}

/// A decal inside, at `center` in interior cell `low` of block `block`: the cells' ground about
/// it as far as the box's largest side ([`nearby_geometry`]), cut to the box standing up from
/// it along its direction.
// D2-10345
#[must_use]
pub fn dungeon_decal(
    records: &WorldRecords,
    block: (i32, i32),
    low: u16,
    center: Vec3,
    desc: &DecalDesc,
) -> Vec<[DecalVertex; 3]> {
    let reach = desc.size.x.max(desc.size.y).max(desc.size.z);
    let ground = nearby_geometry(records, block, low, center, reach);
    construct_decal(
        center,
        Vec3::new(0.0, 0.0, 1.0),
        desc.direction,
        desc.size,
        &ground,
    )
}

/// One decal as it lives, its triangles let go of with it.
// D2-10342
#[derive(Clone, Debug, PartialEq)]
pub struct Decal {
    pub lifespan: f32,
    /// When it was made, seconds of the client's run.
    pub created: f64,
    pub draw_once: bool,
    pub color: Color,
    pub alive: bool,
    pub triangles: Vec<[DecalVertex; 3]>,
    /// How much of it shows, 0 to 1.
    pub opacity: f32,
}

impl Decal {
    /// A decal made at `now` from `desc` with `triangles`: alive when it was laid on any ground
    /// at all (a decal on no ground lives too, drawing nothing).
    // D2-10338, D2-10346
    #[must_use]
    pub fn new(desc: &DecalDesc, now: f64, triangles: Vec<[DecalVertex; 3]>) -> Self {
        Self {
            lifespan: desc.lifespan,
            created: now,
            draw_once: desc.draw_once,
            color: desc.color,
            alive: true,
            triangles,
            opacity: 1.0,
        }
    }

    /// Moves it on to `now`: dead once its lifespan is spent; else it shows as far as it has faded
    /// in (fully at once when drawn once) and not yet out.
    // D2-10339
    pub fn think(&mut self, now: f64) {
        if !self.alive {
            return;
        }
        #[allow(clippy::cast_possible_truncation)] // a few seconds
        let age = (now - self.created) as f32;
        if self.lifespan <= age {
            self.alive = false;
            self.triangles.clear();
            return;
        }
        let out = (self.lifespan * 0.2).min(1.0);
        let in_ = (self.lifespan * 0.1).min(0.25);
        let rise = if self.draw_once {
            1.0
        } else {
            (age / in_).min(1.0)
        };
        self.opacity = (rise / out * (self.lifespan - age)).clamp(0.0, 1.0);
    }

    /// Adds its corners to `out`, colored by its color with its opacity in the alpha (each
    /// channel in 8-bit steps); a decal drawn once then holds nothing more and dies.
    // D2-10340
    pub fn geometry(&mut self, out: &mut Vec<MeshVertex>) {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // 0..=255
        let byte = |x: f32| (x * 255.0) as u32 & 0xFF;
        let c = self.color;
        let diffuse = byte(c.a * self.opacity) << 24 | byte(c.r) << 16 | byte(c.g) << 8 | byte(c.b);
        for t in &self.triangles {
            for v in t {
                out.push(MeshVertex {
                    position: [v.position.x, v.position.y, v.position.z],
                    normal: [v.normal.x, v.normal.y, v.normal.z],
                    diffuse,
                    uv: [v.uv, [0.0; 2], [0.0; 2], [0.0; 2]],
                    ..MeshVertex::default()
                });
            }
        }
        if self.draw_once {
            self.alive = false;
            self.triangles.clear();
        }
    }
}

/// The most vertices a group of decals draws in a frame: past it, the group draws nothing.
pub const GROUP_VERTICES: usize = 0x2000;

/// The decals of one material, let go of with it.
// D2-10348
#[derive(Clone, Debug, PartialEq)]
pub struct DecalGroup {
    pub material: u32,
    pub effects: Vec<Decal>,
}

impl DecalGroup {
    /// Moves every decal on to `now`, letting go of the dead.
    // D2-10349
    pub fn think(&mut self, now: f64) {
        for e in &mut self.effects {
            e.think(now);
        }
        self.effects.retain(|e| e.alive);
    }

    /// The group's vertices for this frame, three a triangle: none when it has no triangle or more
    /// than its room.
    // D2-10350
    pub fn render(&mut self) -> Vec<MeshVertex> {
        let total: usize = self
            .effects
            .iter()
            .filter(|e| e.alive)
            .map(|e| e.triangles.len())
            .sum();
        let mut out = Vec::new();
        if total == 0 || total * 3 >= GROUP_VERTICES {
            return out;
        }
        for e in &mut self.effects {
            e.geometry(&mut out);
        }
        out
    }
}

/// Every decal, kept by material: none at first, every group let go of with it.
// D2-10353, D2-10354, D2-10356
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DecalManager {
    pub groups: Vec<DecalGroup>,
}

impl DecalManager {
    /// Lays decal `desc` at `now` with `triangles`, in the group of its material (made when there
    /// is none, when the material is found: `material_found`). False when it stands in no cell
    /// (`cell` 0) or its material is not found.
    // D2-10358, D2-10347, D2-10351
    pub fn create(
        &mut self,
        cell: u32,
        desc: &DecalDesc,
        now: f64,
        triangles: Vec<[DecalVertex; 3]>,
        material_found: bool,
    ) -> bool {
        if cell == 0 {
            return false;
        }
        let k = match self.groups.iter().position(|g| g.material == desc.material) {
            Some(k) => k,
            None => {
                if desc.material != 0 && !material_found {
                    return false;
                }
                self.groups.push(DecalGroup {
                    material: desc.material,
                    effects: Vec::new(),
                });
                self.groups.len() - 1
            }
        };
        self.groups[k]
            .effects
            .push(Decal::new(desc, now, triangles));
        true
    }

    /// Moves every group's decals on to `now`.
    // D2-10355
    pub fn think(&mut self, now: f64) {
        for g in &mut self.groups {
            g.think(now);
        }
    }

    /// Each group's material and its vertices for this frame (the groups with none left out).
    // D2-10357
    pub fn render(&mut self) -> Vec<(u32, Vec<MeshVertex>)> {
        self.groups
            .iter_mut()
            .map(|g| (g.material, g.render()))
            .filter(|(_, v)| !v.is_empty())
            .collect()
    }
}

/// The material the simple setting's discs are drawn with: the one the data maps to its value
/// (0 when none is).
#[must_use]
pub fn disc_material(records: &WorldRecords) -> u32 {
    records.data_id_by_enum(0x4000_000E, 0x4000_0002)
}

/// The disc the simple setting lays under an object whose physics box runs from `lo` to `hi`,
/// scaled by `scale`, facing `heading` (degrees clockwise from north), with shadow strength
/// `strength`: its width and depth 0.85 of the box's (none unless both pass a quarter meter), two
/// meters high, running the way it faces, black with the strength's three channels summed and
/// taken a third of in its alpha, living a second and drawn once.
// D2-11013
#[must_use]
pub fn disc(
    material: u32,
    lo: Vec3,
    hi: Vec3,
    scale: Vec3,
    heading: f32,
    strength: Color,
) -> Option<DecalDesc> {
    let w = (hi.x - lo.x) * scale.x * 0.85;
    let d = (hi.y - lo.y) * scale.y * 0.85;
    if w <= 0.25 || d <= 0.25 {
        return None;
    }
    let h = heading.to_radians();
    let alpha = ((strength.r + strength.g + strength.b) * 0.333).clamp(0.0, 1.0);
    Some(DecalDesc {
        material,
        lifespan: 1.0,
        size: Vec3::new(w, d, 2.0),
        direction: Vec3::new(
            dereth_primitives::num::math::sinf(h),
            dereth_primitives::num::math::cosf(h),
            0.0,
        ),
        color: Color::new(0.0, 0.0, 0.0, alpha),
        draw_once: true,
    })
}

/// The frame's decal draws: each group's vertices this frame, drawn with its material's layers
/// (kept in `layers`) where the device draws the frame from, with no transform of their own.
// D2-10357, D2-10350
pub fn decal_batches(
    records: &WorldRecords,
    decals: &mut DecalManager,
    layers: &mut HashMap<u32, Vec<DrawLayer>>,
) -> Vec<ObjectBatch> {
    let mut out = Vec::new();
    for (material, vertices) in decals.render() {
        let layers = layers
            .entry(material)
            .or_insert_with(|| {
                instance_material_with(records, material, None, &[])
                    .map(|m| draw_layers(&m, DEVICE_FLAGS))
                    .unwrap_or_default()
            })
            .clone();
        let n = u16::try_from(vertices.len()).unwrap_or(0);
        let vertices = Arc::new(vertices);
        let indices = Arc::new((0..n).collect::<Vec<u16>>());
        for layer in layers {
            out.push(ObjectBatch {
                vertices: vertices.clone(),
                posed: None,
                played: None,
                indices: indices.clone(),
                transform: Mat4::IDENTITY,
                layer,
                tint: Color::WHITE,
                points: PointLights::NONE,
                mesh_type: 0,
                shade: None,
                outside: false,
                cull: Cull::Never,
                mesh: 0,
                casts_volume: false,
                caster: None,
                seen_through: false,
                kept_by: None,
                anchor: None,
                step: None,
                visible: None,
                block: None,
                far: false,
                ground: None,
                lod: None,
            });
        }
    }
    out
}

/// The enumeration value naming the material a point light's pool on the land is drawn with at
/// time of day `time_of_day`: the night's before 0.2 and after 0.95, the twilight's before 0.3
/// and after 0.85, else the day's.
// D2-11035
#[must_use]
pub fn pool_material_value(time_of_day: f32) -> u32 {
    if time_of_day < 0.2 || time_of_day > 0.95 {
        0x4000_002C
    } else if time_of_day < 0.3 || time_of_day > 0.85 {
        0x4000_002B
    } else {
        0x4000_000D
    }
}

/// The material a point light's pool on the land is drawn with at time of day `time_of_day` (0
/// when the data maps none).
#[must_use]
pub fn pool_material(records: &WorldRecords, time_of_day: f32) -> u32 {
    records.data_id_by_enum(pool_material_value(time_of_day), 0x4000_0002)
}

/// The height of the ground of a square (its two triangles, as [`landscape_decal`] takes them)
/// under `(x, y)`: the plane of the triangle holding the point (or touching it) there.
#[must_use]
pub fn ground_height(square: &[Corner; 6], x: f32, y: f32) -> Option<f32> {
    for t in [&square[0..3], &square[3..6]] {
        let (a, b, c) = (t[0].0, t[1].0, t[2].0);
        let side = |p: Vec3, q: Vec3| (q.x - p.x) * (y - p.y) - (q.y - p.y) * (x - p.x);
        let (d1, d2, d3) = (side(a, b), side(b, c), side(c, a));
        let neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
        let pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
        if neg && pos {
            continue;
        }
        let n = cross(sub(b, a), sub(c, a));
        if n.z.abs() < 1e-6 {
            continue;
        }
        return Some(a.z - (n.x * (x - a.x) + n.y * (y - a.y)) / n.z);
    }
    None
}

/// The pool of light a point light at `origin` lays on the land whose height under it is
/// `ground`, its range and its intensity as they stand this moment, its diffuse color `color` and
/// opacity `opacity`: none unless it stands above the ground and less than its range above it;
/// else a box about the light twice its range on every side running north, its color the light's
/// times its intensity, opacity and how near it is to the ground (one less its height over its
/// range), held to 0..1 with alpha one, drawn once.
// D2-11035
#[must_use]
pub fn point_light_pool(
    material: u32,
    origin: Vec3,
    ground: f32,
    range: f32,
    intensity: f32,
    color: Color,
    opacity: f32,
) -> Option<DecalDesc> {
    if ground >= origin.z {
        return None;
    }
    let near = 1.0 - (origin.z - ground) / range;
    if near.is_nan() || near <= 0.0 {
        return None;
    }
    let k = intensity * opacity * near;
    let side = range + range;
    Some(DecalDesc {
        material,
        lifespan: 1.0,
        size: Vec3::new(side, side, side),
        direction: Vec3::new(0.0, 1.0, 0.0),
        color: Color::new(
            (color.r * k).clamp(0.0, 1.0),
            (color.g * k).clamp(0.0, 1.0),
            (color.b * k).clamp(0.0, 1.0),
            1.0,
        ),
        draw_once: true,
    })
}
