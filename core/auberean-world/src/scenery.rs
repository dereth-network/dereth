//! Scenery: the trees, rocks, ferns and grass the landscape scatters over itself, placed the same
//! way on every client from the block's own data.
//!
//! Each vertex of a landblock names a scene through its terrain kind and its scene field: the
//! terrain kind's scenery list maps the field to a scenery kind, and a position hash picks one of
//! that kind's scenes by frequency. A scene is a list of objects, nested scenes and scatter groups;
//! a group is placed at a hashed offset with a hashed turn and scale, kept only where its rules
//! allow (on the block and off the road, within its slopes and heights, clear of what is already
//! there), and then places its own members relative to itself. Every choice is a hash of the
//! vertex's global coordinates and a running seed, so placements never depend on load order.
//!
//! A scene's decoration group places frills: small objects drawn only near the viewer, which
//! never keep anything else away.
//!
//! The block's own placed objects keep scenery away from their spot, or from the boxes and upright
//! cylinders their clearance properties describe about them ([`KeptClear`]).

use dereth_primitives::num::math;
use std::sync::Arc;

use auberean_assets::property::{PropertyCollection, PropertyValue};
use auberean_assets::region::{SceneDesc, TerrainDesc};
use auberean_assets::visual::{ScatterGroup, Scene, SceneObject};
use auberean_primitives::{DataId, Quat, Vec3};

use crate::land::LandConstants;
use crate::math::{
    circle_over_polygon, cross, dot, normalize, over_polygon, quat_axis_angle, quat_mul,
    rotation_rows, sub, Mat4,
};
use crate::random::fraction;
use crate::terrain::TerrainBlock;

/// Where the scenes come from.
pub trait SceneSource {
    fn scene(&self, id: DataId) -> Option<Arc<Scene>>;
}

/// One object the landscape places.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SceneryObject {
    /// The record placed (a setup, a visual description or an entity).
    pub object: DataId,
    /// In the block's frame.
    pub origin: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
    /// A decoration drawn only near the viewer.
    pub frill: bool,
}

/// What placing one block's scenery reads.
#[derive(Clone, Copy, Debug)]
pub struct SceneryInput<'a> {
    pub land: &'a LandConstants,
    pub terrain: &'a TerrainDesc,
    pub scenes: &'a SceneDesc,
    /// The block's vertex words.
    pub vertices: &'a [u32],
    pub block: &'a TerrainBlock,
    /// The block's own placed objects: scenery keeps its clearance from where they stand, or from
    /// what they ask to be kept clear.
    pub obstacles: &'a [KeptClear],
    /// The scenery distance preference, 0 to 1: which of a scene's levels applies.
    pub degrade: f32,
    /// The height of the block's water surface, if it has water.
    pub water: Option<f32>,
}

/// The property group of an object's clearance properties.
pub const CLEARANCE_GROUP: u32 = 0x4000_0009;

/// The clearance properties: the four boxes' least corners, then their greatest corners; the two
/// cylinders' radii, base centers and heights.
const BOX_MIN: u32 = 0x4000_0028;
const BOX_MAX: u32 = 0x4000_002C;
const CYLINDER_RADIUS: u32 = 0x4000_0030;
const CYLINDER_CENTER: u32 = 0x4000_0032;
const CYLINDER_HEIGHT: u32 = 0x4000_0034;

/// An upright cylinder an object keeps clear, in its own frame.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct ClearCylinder {
    /// The center of its base.
    pub low: Vec3,
    pub height: f32,
    pub radius: f32,
}

/// A placed object as scenery keeps clear of it: its spot (its frame's origin), or the boxes and
/// upright cylinders about it, in its own frame, that its clearance properties set.
#[derive(Clone, Debug, PartialEq)]
pub struct KeptClear {
    pub origin: Vec3,
    pub rotation: Quat,
    /// Least and greatest corners.
    pub boxes: Vec<(Vec3, Vec3)>,
    pub cylinders: Vec<ClearCylinder>,
}

impl KeptClear {
    /// An object standing at `origin`, turned by `rotation`, keeping clear what the clearance group
    /// of `properties` (its own with its ancestors' beneath) sets: each box's corners and each
    /// cylinder's radius, base and height by their own property (the first to fourth box, the
    /// first and second cylinder). A box or cylinder any property names is there, with every one
    /// before it, its other measures zero until set; a value of another type sets nothing.
    // D2-03675, D2-03639, D2-12456, D2-12462, D2-12457, D2-12458, D2-12459, D2-12460, D2-12461
    #[must_use]
    pub fn of_entity(
        origin: Vec3,
        rotation: Quat,
        properties: Option<&PropertyCollection>,
    ) -> Self {
        let mut c = Self {
            origin,
            rotation,
            boxes: Vec::new(),
            cylinders: Vec::new(),
        };
        let Some(group) = properties.and_then(|p| p.group(CLEARANCE_GROUP)) else {
            return c;
        };
        for p in &group.properties {
            let vector = match p.value {
                PropertyValue::Vector(v) => Some(v),
                _ => None,
            };
            let float = match p.value {
                PropertyValue::Float(f) => Some(f),
                _ => None,
            };
            let at = |base: u32, n: u32| {
                p.id.checked_sub(base)
                    .filter(|k| *k < n)
                    .and_then(|k| usize::try_from(k).ok())
            };
            if let Some(k) = at(BOX_MIN, 4) {
                grow(&mut c.boxes, k);
                if let Some(v) = vector {
                    c.boxes[k].0 = v;
                }
            } else if let Some(k) = at(BOX_MAX, 4) {
                grow(&mut c.boxes, k);
                if let Some(v) = vector {
                    c.boxes[k].1 = v;
                }
            } else if let Some(k) = at(CYLINDER_RADIUS, 2) {
                grow(&mut c.cylinders, k);
                if let Some(f) = float {
                    c.cylinders[k].radius = f;
                }
            } else if let Some(k) = at(CYLINDER_CENTER, 2) {
                grow(&mut c.cylinders, k);
                if let Some(v) = vector {
                    c.cylinders[k].low = v;
                }
            } else if let Some(k) = at(CYLINDER_HEIGHT, 2) {
                grow(&mut c.cylinders, k);
                if let Some(f) = float {
                    c.cylinders[k].height = f;
                }
            }
        }
        c
    }

    /// A point of the block's frame in the object's own frame.
    fn local(&self, p: Vec3) -> Vec3 {
        Mat4::from_parts(self.origin, self.rotation, Vec3::new(1.0, 1.0, 1.0)).rigid_preimage(p)
    }

    /// Whether a circle (a sphere when `in_3d`) about `at` of radius `r` reaches a box: a box with
    /// no extent on any axis reaches nothing; else the gaps from the point to the box along each
    /// axis (the height only in 3D) make a distance within `r`.
    // D2-12454
    fn reaches_box(&self, b: (Vec3, Vec3), at: Vec3, r: f32, in_3d: bool) -> bool {
        let (lo, hi) = b;
        if (lo.x - hi.x).abs() < 0.0002
            && (lo.y - hi.y).abs() < 0.0002
            && (lo.z - hi.z).abs() < 0.0002
        {
            return false;
        }
        let p = self.local(at);
        let gap = |v: f32, lo: f32, hi: f32| {
            if v < lo {
                lo - v
            } else if hi < v {
                v - hi
            } else {
                0.0
            }
        };
        let (dx, dy) = (gap(p.x, lo.x, hi.x), gap(p.y, lo.y, hi.y));
        let dz = if in_3d { gap(p.z, lo.z, hi.z) } else { 0.0 };
        dx * dx + dy * dy + dz * dz <= r * r
    }

    /// Whether a circle (a sphere when `in_3d`) about `at` of radius `r` reaches a cylinder: one of
    /// no radius reaches nothing; across the ground, the point within the two radii of its axis;
    /// in 3D, the sphere's center nearer than the two radii to the nearest point of the axis
    /// between the cylinder's base and top.
    // D2-12448
    fn reaches_cylinder(&self, c: &ClearCylinder, at: Vec3, r: f32, in_3d: bool) -> bool {
        if c.radius <= 0.0 {
            return false;
        }
        let p = self.local(at);
        let (dx, dy) = (p.x - c.low.x, p.y - c.low.y);
        let reach = r + c.radius;
        if in_3d {
            let top = c.low.z + c.height;
            let z = if p.z > top {
                top
            } else if p.z < c.low.z {
                c.low.z
            } else {
                p.z
            };
            let dz = p.z - z;
            dx * dx + dy * dy + dz * dz < reach * reach
        } else {
            dx * dx + dy * dy <= reach * reach
        }
    }

    /// Whether anything it keeps clear is within `r` of `at`: with no boxes or cylinders its spot
    /// alone, across the ground; else any of its boxes, then any of its cylinders.
    fn reaches(&self, at: Vec3, r: f32, in_3d: bool) -> bool {
        if self.boxes.is_empty() && self.cylinders.is_empty() {
            let (dx, dy) = (at.x - self.origin.x, at.y - self.origin.y);
            return dx * dx + dy * dy <= r * r;
        }
        self.boxes
            .iter()
            .any(|b| self.reaches_box(*b, at, r, in_3d))
            || self
                .cylinders
                .iter()
                .any(|c| self.reaches_cylinder(c, at, r, in_3d))
    }
}

/// Grow `v` with zeroed entries until it holds an entry `k`.
fn grow<T: Default>(v: &mut Vec<T>, k: usize) {
    while v.len() <= k {
        v.push(T::default());
    }
}

/// The seed every vertex's scene starts from.
const START_SEED: u32 = 0x0009_23B1;
/// The step between a scene's or a group's successive members' seeds.
const MEMBER_STEP: u32 = 0x2D41_BF19;
/// What a scene's decoration group adds to the scene's seed.
const DECORATION_STEP: u32 = 0x0380_4309;

/// Where a group's members are measured from.
const FRAME_RELATIVE: u32 = 9;
const LAND_HEIGHT_RELATIVE: u32 = 0;
const WATER_HEIGHT_RELATIVE: u32 = 1;

/// A piece of the block's ground: a triangle and its plane, normal up.
#[derive(Clone, Copy, Debug)]
struct Facet {
    a: Vec3,
    b: Vec3,
    c: Vec3,
    normal: Vec3,
    d: f32,
}

impl Facet {
    fn new(a: Vec3, b: Vec3, c: Vec3) -> Self {
        let mut n = normalize(cross(sub(b, a), sub(c, a)));
        if n.z < 0.0 {
            n = Vec3::new(-n.x, -n.y, -n.z);
        }
        Self {
            a,
            b,
            c,
            normal: n,
            d: -dot(n, a),
        }
    }

    /// The plane's height over `(x, y)`, or `z` itself on a vertical plane.
    // D2-03602
    fn height(&self, x: f32, y: f32, z: f32) -> f32 {
        if self.normal.z.abs() > 0.0002 {
            -(x * self.normal.x + y * self.normal.y + self.d) / self.normal.z
        } else {
            z
        }
    }

    /// The corners, in order.
    fn corners(&self) -> [[f32; 3]; 3] {
        [self.a, self.b, self.c].map(|v| [v.x, v.y, v.z])
    }
}

/// The heading of a normal's horizontal direction, degrees clockwise from north; 0 when it has
/// none.
fn normal_heading(n: Vec3) -> f32 {
    let l = (n.x * n.x + n.y * n.y).sqrt();
    if l < 0.0002 {
        return 0.0;
    }
    let deg = (450.0 - math::atan2f(n.y / l, n.x / l).to_degrees()).rem_euclid(360.0);
    if deg >= 360.0 {
        0.0
    } else {
        deg
    }
}

/// The rotation that turns local north (+y) to `heading` degrees clockwise from north.
fn heading_rotation(heading: f32) -> Quat {
    quat_axis_angle(Vec3::new(0.0, 0.0, 1.0), -heading.to_radians())
}

/// The rotation whose local axes are `forward × up`, `forward` and `up`.
fn rotation_from_axes(up: Vec3, forward: Vec3) -> Quat {
    let x = cross(forward, up);
    let m = [
        [x.x, x.y, x.z],
        [forward.x, forward.y, forward.z],
        [up.x, up.y, up.z],
    ];
    // rows are the images of the axes (`rotation_rows`); invert that construction
    let trace = m[0][0] + m[1][1] + m[2][2];
    let q = if trace > 0.0 {
        let s = (trace + 1.0).sqrt() * 2.0;
        Quat {
            w: 0.25 * s,
            x: (m[1][2] - m[2][1]) / s,
            y: (m[2][0] - m[0][2]) / s,
            z: (m[0][1] - m[1][0]) / s,
        }
    } else if m[0][0] > m[1][1] && m[0][0] > m[2][2] {
        let s = (1.0 + m[0][0] - m[1][1] - m[2][2]).sqrt() * 2.0;
        Quat {
            w: (m[1][2] - m[2][1]) / s,
            x: 0.25 * s,
            y: (m[1][0] + m[0][1]) / s,
            z: (m[2][0] + m[0][2]) / s,
        }
    } else if m[1][1] > m[2][2] {
        let s = (1.0 + m[1][1] - m[0][0] - m[2][2]).sqrt() * 2.0;
        Quat {
            w: (m[2][0] - m[0][2]) / s,
            x: (m[1][0] + m[0][1]) / s,
            y: 0.25 * s,
            z: (m[2][1] + m[1][2]) / s,
        }
    } else {
        let s = (1.0 + m[2][2] - m[0][0] - m[1][1]).sqrt() * 2.0;
        Quat {
            w: (m[0][1] - m[1][0]) / s,
            x: (m[2][0] + m[0][2]) / s,
            y: (m[2][1] + m[1][2]) / s,
            z: 0.25 * s,
        }
    };
    let l = (q.w * q.w + q.x * q.x + q.y * q.y + q.z * q.z).sqrt();
    Quat {
        w: q.w / l,
        x: q.x / l,
        y: q.y / l,
        z: q.z / l,
    }
}

/// A frame tilted onto a plane: its up the plane's normal, its north as close to north as the
/// plane allows.
// D2-13138
fn orient_to_plane(n: Vec3) -> Quat {
    if n.x.abs() >= 0.0002 || n.y.abs() >= 0.0002 {
        if n.z.abs() > 0.0002 {
            rotation_from_axes(n, normalize(Vec3::new(0.0, n.z, -n.y)))
        } else {
            rotation_from_axes(
                normalize(Vec3::new(n.x, n.y, 0.0)),
                Vec3::new(0.0, 0.0, 1.0),
            )
        }
    } else {
        rotation_from_axes(Vec3::new(0.0, 0.0, 1.0), Vec3::new(0.0, 1.0, 0.0))
    }
}

/// Where something stands: an origin and a turn, in the block's frame.
#[derive(Clone, Copy, Debug)]
struct Place {
    origin: Vec3,
    rotation: Quat,
}

/// Something placement keeps clear of: a placed object, a group while and after it places its
/// members, or a scene's own vertex.
// D2-12446, D2-12447, D2-12449, D2-12450, D2-12451, D2-12452, D2-12453, D2-03875
#[derive(Clone, Copy, Debug)]
struct Clearance {
    origin: Vec3,
    radius: f32,
    radius_in_3d: bool,
    group: bool,
    done: bool,
    /// The block's placed object it is, by its place among the obstacles.
    object: Option<usize>,
}

/// The vertex's scene hash: the sixteen-bit fraction that picks a scene among its kind's.
// D2-03732
fn scene_fraction(x: u32, y: u32) -> f32 {
    let v = y
        .wrapping_mul(x)
        .wrapping_mul(x)
        .wrapping_mul(0x019A_266F)
        .wrapping_sub(0x4A65_06DB)
        .wrapping_mul(y)
        .wrapping_sub(x.wrapping_mul(0x6B3C_5F11))
        .wrapping_add(0xC649_DA05);
    #[allow(clippy::cast_precision_loss)] // sixteen bits
    let f = (v >> 16) as f32;
    f * (1.0 / 65_536.0)
}

/// The position hash of the scenery: [`fraction`] of global vertex `(x, y)` and `seed`.
#[allow(clippy::cast_possible_wrap)] // the hash's arithmetic wraps
fn rand(x: u32, y: u32, seed: u32) -> f32 {
    // LINT-OK: the vertex and seed words' bits as signed words, not a float conversion
    fraction(x as i32, y as i32, seed as i32)
}

/// The scene a vertex grows: its terrain kind's scenery for its scene field (or the kind's
/// default), and of that scenery kind's scenes the one the hash picks by frequency.
// D2-07448, D2-12395, D2-07433, D2-12500, D2-07432, D2-12492
#[must_use]
pub fn vertex_scene(
    terrain: &TerrainDesc,
    scenes: &SceneDesc,
    terrain_kind: u32,
    scene_field: u32,
    pick: f32,
) -> Option<DataId> {
    let kind = terrain
        .terrains
        .iter()
        .find(|t| t.index == terrain_kind)
        .or(terrain.default_terrain.as_ref())?;
    let scenery = kind
        .scenery
        .iter()
        .find(|s| s.index == scene_field)
        .or(kind.default_scenery.as_ref())?;
    let st = scenes
        .scene_types
        .iter()
        .find(|s| s.index == scenery.kind)?;
    let last = st.scenes.last()?;
    let total: f32 = st.scenes.iter().map(|s| s.frequency).sum();
    let mut acc = 0.0;
    for s in &st.scenes[..st.scenes.len() - 1] {
        acc += s.frequency;
        if pick * total < acc {
            return (!s.scene.is_none()).then_some(s.scene);
        }
    }
    (!last.scene.is_none()).then_some(last.scene)
}

/// The lowest scene field under which `terrain_kind` grows nothing whatever the hash picks: its
/// scenery kind holds no scene, or only scenes with nothing in them. `None` when it grows something
/// under every field a vertex word can hold.
#[must_use]
pub fn barren_field(
    terrain: &TerrainDesc,
    scenes: &SceneDesc,
    source: &dyn SceneSource,
    terrain_kind: u32,
) -> Option<u32> {
    let kind = terrain
        .terrains
        .iter()
        .find(|t| t.index == terrain_kind)
        .or(terrain.default_terrain.as_ref())?;
    let empty = |id: DataId| {
        id.is_none()
            || source.scene(id).is_some_and(|s| {
                s.decoration.is_none() && s.levels.iter().all(|l| l.objects.is_empty())
            })
    };
    (0..SCENE_FIELDS).find(|field| {
        let scenery = kind
            .scenery
            .iter()
            .find(|s| s.index == *field)
            .or(kind.default_scenery.as_ref());
        let Some(scenery) = scenery else {
            return true;
        };
        scenes
            .scene_types
            .iter()
            .find(|s| s.index == scenery.kind)
            .is_none_or(|st| st.scenes.iter().all(|s| empty(s.scene)))
    })
}

/// How many values a vertex word's scene field can hold, at the data's five bits.
const SCENE_FIELDS: u32 = 32;

/// The scene each vertex of block `(bx, by)` grows, in the vertices' order (x-major), from the
/// block's vertex words: what the client keeps of a block it may stand near, and asks the data
/// cache for. `None` for a vertex that grows none.
// D2-03732
#[must_use]
pub fn vertex_scenes(
    land: &LandConstants,
    terrain: &TerrainDesc,
    scenes: &SceneDesc,
    (bx, by): (u32, u32),
    vertices: &[u32],
) -> Vec<Option<DataId>> {
    let per_block = u32::try_from(land.grid().squares_per_side()).unwrap_or(0);
    let side = land.grid().vertices_per_side();
    (0..side * side)
        .map(|k| {
            let word = *vertices.get(k)?;
            let (i, j) = (
                u32::try_from(k / side).unwrap_or(0),
                u32::try_from(k % side).unwrap_or(0),
            );
            let (x, y) = (per_block * bx + i, per_block * by + j);
            vertex_scene(
                terrain,
                scenes,
                land.word.terrain.of(word),
                land.word.scene.of(word),
                scene_fraction(x, y),
            )
        })
        .collect()
}

struct Placer<'a> {
    input: &'a SceneryInput<'a>,
    source: &'a dyn SceneSource,
    facets: Vec<Facet>,
    clear: Vec<Clearance>,
    out: Vec<SceneryObject>,
}

impl Placer<'_> {
    fn squares(&self) -> usize {
        self.input.block.geometry.squares_per_side
    }

    fn block_length(&self) -> f32 {
        self.input.land.grid().block_length()
    }

    /// The ground under `(x, y)`: of the triangles of the land cell holding it, its squares taken
    /// x-major and each square's two in order, the first it lies over (within the band).
    // D2-03823
    fn facet_at(&self, x: f32, y: f32) -> Option<&Facet> {
        let n = self.squares();
        let cell = self.input.land.grid().cell_length;
        let per = n / self.input.land.grid().cells_per_side;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let (cx, cy) = ((x / cell).floor() as isize, (y / cell).floor() as isize);
        let (cx, cy) = (usize::try_from(cx).ok()?, usize::try_from(cy).ok()?);
        if (cx + 1) * per > n || (cy + 1) * per > n {
            return None;
        }
        let p = [x, y, 0.0];
        for sx in cx * per..(cx + 1) * per {
            for sy in cy * per..(cy + 1) * per {
                let k = 2 * (sx * n + sy);
                if let Some(f) = self.facets[k..k + 2]
                    .iter()
                    .find(|f| over_polygon(&f.corners(), p, true))
                {
                    return Some(f);
                }
            }
        }
        None
    }

    /// Whether `(x, y)` is on the road.
    fn on_road(&self, x: f32, y: f32) -> bool {
        on_road(
            self.input.land,
            self.input.block.geometry.vertices_per_side,
            self.input.vertices,
            x,
            y,
        )
    }

    /// Whether every piece of ground a circle of radius `r` about the place reaches is within the
    /// group's slopes in radius (and, for an aligned group, turned within its heading limits);
    /// a circle reaching off the block fails.
    // D2-03644, D2-03831, D2-12424, D2-12428, D2-03611
    fn check_radius(&self, g: &ScatterGroup, place: &Place, scale: Vec3) -> bool {
        let r = if g.scale_check_radius {
            scale.x.max(scale.y) * g.check_slope_radius
        } else {
            g.check_slope_radius
        };
        let (mut lo, mut hi) = (0.0f32, 360.0f32);
        if g.align && g.align_max_heading_offset < 180.0 {
            let h = rotation_heading(place.rotation);
            lo = h - g.align_max_heading_offset;
            if lo < 0.0 {
                lo += 360.0;
            }
            hi = h + g.align_max_heading_offset;
        }
        let o = place.origin;
        let bl = self.block_length();
        let (x0, y0, x1, y1) = (o.x - r, o.y - r, o.x + r, o.y + r);
        if x0 < 0.0 || y0 < 0.0 || x0 >= bl || y0 >= bl || x1 >= bl || y1 >= bl {
            return false;
        }
        let n = self.squares();
        let cell = self.input.land.grid().cell_length;
        let per = n / self.input.land.grid().cells_per_side;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let c = |v: f32| (v / cell).floor() as usize;
        for cx in c(x0)..=c(x1) {
            for cy in c(y0)..=c(y1) {
                for sx in cx * per..(cx + 1) * per {
                    for sy in cy * per..(cy + 1) * per {
                        let k = 2 * (sx * n + sy);
                        for f in &self.facets[k..k + 2] {
                            let nz = f.normal.z;
                            let in_range = g.min_slope_in_radius + 0.0002 > nz
                                && nz > g.max_slope_in_radius - 0.0002;
                            if in_range && !g.align {
                                continue;
                            }
                            if !circle_over_polygon(
                                &f.corners(),
                                f.normal.z,
                                [o.x, o.y, o.z],
                                true,
                                r,
                            ) {
                                continue;
                            }
                            if !in_range {
                                return false;
                            }
                            let h = normal_heading(f.normal);
                            if hi <= lo {
                                if h < lo && hi < h {
                                    return false;
                                }
                            } else if h < lo || hi < h {
                                return false;
                            }
                        }
                    }
                }
            }
        }
        true
    }

    /// Whether a circle about `at` of radius `r` reaches anything already kept clear.
    // D2-03642, D2-12455, D2-12450
    fn collides(&self, at: Vec3, r: f32, in_3d: bool) -> bool {
        self.clear.iter().any(|c| {
            if c.group && !c.done {
                return false;
            }
            if let Some(o) = c.object.and_then(|k| self.input.obstacles.get(k)) {
                return o.reaches(at, r, in_3d);
            }
            let (dx, dy) = (at.x - c.origin.x, at.y - c.origin.y);
            let dz = if in_3d && c.radius_in_3d {
                at.z - c.origin.z
            } else {
                0.0
            };
            let reach = r + c.radius;
            dx * dx + dy * dy + dz * dz <= reach * reach
        })
    }

    /// Start keeping a group (or a scene's vertex, with no group) clear; frills keep nothing
    /// clear.
    // D2-03676, D2-12447, D2-12451
    fn start_group(
        &mut self,
        place: &Place,
        g: Option<&ScatterGroup>,
        scale: Vec3,
        frill: bool,
    ) -> Option<usize> {
        if frill {
            return None;
        }
        self.clear.push(Clearance {
            origin: place.origin,
            radius: g.map_or(0.0, |g| clearance_radius(g, scale)),
            radius_in_3d: g.is_some_and(|g| g.radius_in_3d),
            group: true,
            done: false,
            object: None,
        });
        Some(self.clear.len() - 1)
    }

    // D2-12449
    fn end_group(&mut self, k: Option<usize>) {
        if let Some(c) = k.and_then(|k| self.clear.get_mut(k)) {
            c.done = true;
        }
    }

    /// Place one object: a frill is only drawn; anything else is also kept clear.
    // D2-03677, D2-03643, D2-12446, D2-03891
    fn entity(&mut self, id: DataId, place: &Place, scale: Vec3, frill: bool) {
        if !frill {
            self.clear.push(Clearance {
                origin: place.origin,
                radius: 0.0,
                radius_in_3d: false,
                group: false,
                done: true,
                object: None,
            });
        }
        self.out.push(SceneryObject {
            object: id,
            origin: place.origin,
            rotation: place.rotation,
            scale,
            frill,
        });
    }

    /// A scene at `place`: its level's members in order, each with its own seed, then its
    /// decoration group as frills.
    // D2-03688
    #[allow(clippy::too_many_arguments)]
    fn scene(
        &mut self,
        x: u32,
        y: u32,
        id: DataId,
        place: &Place,
        scale: Vec3,
        count: &mut u32,
        seed: u32,
        frill: bool,
        depth: u32,
    ) -> bool {
        let seed = seed.wrapping_mul(START_SEED);
        let Some(scene) = self.source.scene(id) else {
            return false;
        };
        if depth > 16 {
            return true;
        }
        let degrade = self.input.degrade;
        // D2-12408
        let level = scene
            .levels
            .iter()
            .find(|l| degrade_distance(l, degrade) > 0.0);
        if let Some(level) = level {
            let k = self.start_group(place, None, scale, frill);
            let mut placed = 0;
            let mut s = seed;
            for obj in &level.objects {
                // D2-12442, D2-12443
                match obj {
                    SceneObject::Group(g) => {
                        if !self.group(x, y, g, place, scale, s, frill, depth + 1) {
                            return false;
                        }
                    }
                    SceneObject::Record { kind: 3, id } => {
                        if !self.scene(x, y, *id, place, scale, &mut placed, s, frill, depth + 1) {
                            return false;
                        }
                    }
                    SceneObject::Record { id, .. } => {
                        self.entity(*id, place, scale, frill);
                        *count += 1;
                    }
                }
                s = s.wrapping_add(MEMBER_STEP);
            }
            self.end_group(k);
        }
        if let Some(deco) = &scene.decoration {
            self.group(
                x,
                y,
                deco,
                place,
                scale,
                seed.wrapping_add(DECORATION_STEP),
                true,
                depth + 1,
            );
        }
        true
    }

    /// A group: if its frequency lets it and it can be placed, its members relative to it, until
    /// it has placed its most objects.
    // D2-03689, D2-12420, D2-12421, D2-12422
    #[allow(clippy::too_many_arguments)]
    fn group(
        &mut self,
        x: u32,
        y: u32,
        g: &ScatterGroup,
        parent: &Place,
        parent_scale: Vec3,
        seed: u32,
        frill: bool,
        depth: u32,
    ) -> bool {
        if depth > 16 {
            return true;
        }
        let mut place = *parent;
        if rand(x, y, seed.wrapping_add(0x0119_1E67)) >= g.frequency {
            return true;
        }
        let Some(scale) = group_scale(g, x, y, parent_scale, seed) else {
            return false;
        };
        let mut scale = scale;
        if !self.place_group(x, y, g, &mut place, parent_scale, &mut scale, seed) {
            return true;
        }
        let k = self.start_group(&place, Some(g), scale, frill);
        let mut placed = 0u32;
        let mut s = seed;
        for obj in &g.objects {
            match obj {
                SceneObject::Group(sub) => {
                    if !self.group(x, y, sub, &place, scale, s, frill, depth + 1) {
                        return false;
                    }
                }
                SceneObject::Record { kind: 3, id } => {
                    if !self.scene(x, y, *id, &place, scale, &mut placed, s, frill, depth + 1) {
                        return false;
                    }
                }
                SceneObject::Record { id, .. } => {
                    self.entity(*id, &place, scale, frill);
                    placed += 1;
                }
            }
            if g.max_objects != 0 && g.max_objects <= placed {
                break;
            }
            s = s.wrapping_add(MEMBER_STEP);
        }
        self.end_group(k);
        true
    }

    /// Move a group to its spot and check it: its offset, the block's edge and the road, the
    /// ground's slope, its turn and height, its scale by slope, its final height, the slopes
    /// around it and what it would crowd.
    // D2-03664, D2-12432, D2-12433, D2-12423, D2-12437, D2-12425, D2-12435, D2-12427
    #[allow(clippy::too_many_arguments)]
    fn place_group(
        &self,
        x: u32,
        y: u32,
        g: &ScatterGroup,
        place: &mut Place,
        parent_scale: Vec3,
        scale: &mut Vec3,
        seed: u32,
    ) -> bool {
        let ra = rand(x, y, seed.wrapping_add(0x28D0_642D));
        // the offset, in the parent's frame or not
        let ls = if g.scale_subobject_locations {
            parent_scale
        } else {
            Vec3::new(1.0, 1.0, 1.0)
        };
        let rb = rand(x, y, seed.wrapping_add(0x062A_DE4F));
        // (the seed steps that would read as program addresses are written in decimal)
        let rc = rand(x, y, seed.wrapping_add(7_459_693));
        let mut off = Vec3::new(
            ((g.max_location.x - g.min_location.x) * ra + g.min_location.x) * ls.x,
            ((g.max_location.y - g.min_location.y) * rb + g.min_location.y) * ls.y,
            0.0,
        );
        if g.height_relative == FRAME_RELATIVE {
            off.z = ((g.max_location.z - g.min_location.z) * rc + g.min_location.z) * ls.z;
        }
        place.origin = if g.rotate_subobjects_for_location {
            let r = rotation_rows(place.rotation);
            Vec3::new(
                place.origin.x + off.x * r[0][0] + off.y * r[1][0] + off.z * r[2][0],
                place.origin.y + off.x * r[0][1] + off.y * r[1][1] + off.z * r[2][1],
                place.origin.z + off.x * r[0][2] + off.y * r[1][2] + off.z * r[2][2],
            )
        } else {
            Vec3::new(
                place.origin.x + off.x,
                place.origin.y + off.y,
                place.origin.z + off.z,
            )
        };
        let snapshot = place.origin;
        let radius = clearance_radius(g, *scale);
        let bl = self.block_length();
        let o = place.origin;
        if !(radius <= o.x && radius <= o.y && o.x < bl - radius && o.y < bl - radius) {
            return false;
        }
        if !g.skip_position_check && self.on_road(o.x, o.y) {
            return false;
        }
        let Some(ground) = self.facet_at(o.x, o.y).copied() else {
            return false;
        };
        // the ground's slope: its normal's height between the group's bounds
        if !(ground.normal.z <= g.min_slope && g.max_slope <= ground.normal.z) {
            return false;
        }
        // the turn
        let mut heading = (g.max_rotation - g.min_rotation) * ra + g.min_rotation;
        if !g.orient && !g.align {
            let turn = heading_rotation(heading);
            place.rotation = if g.rotate_subobjects_for_orientation {
                quat_mul(place.rotation, turn)
            } else {
                turn
            };
        } else {
            if g.orient {
                heading += normal_heading(ground.normal) + 180.0;
            }
            let turn = heading_rotation(heading);
            place.rotation = if g.align {
                quat_mul(orient_to_plane(ground.normal), turn)
            } else {
                turn
            };
        }
        // the height, from the ground or the water
        let water = self.input.water;
        let before = place.origin;
        if g.height_relative != FRAME_RELATIVE {
            let mut z = place.origin.z;
            match g.height_relative {
                LAND_HEIGHT_RELATIVE => z = ground.height(place.origin.x, place.origin.y, z),
                WATER_HEIGHT_RELATIVE => match water {
                    Some(w) => z = w,
                    None => return false,
                },
                2..=8 => z = 0.0,
                _ => {}
            }
            let zs = if g.scale_subobject_locations {
                parent_scale.z
            } else {
                1.0
            };
            z += ((g.max_location.z - g.min_location.z) * ra + g.min_location.z) * zs;
            place.origin.z = z;
        }
        match g.scale_type {
            // scaled by the water's depth there
            // D2-12434
            5..=7 => {
                let Some(w) = water else {
                    return false;
                };
                let depth = w - ground.height(before.x, before.y, before.z);
                if depth <= 0.0002 {
                    return false;
                }
                let f = depth / g.scale_depth;
                if g.scale_type == 7 {
                    *scale = Vec3::new(f, f, f);
                } else {
                    scale.z = f;
                }
            }
            8 | 9 => {
                let (ax, ay) = (ground.normal.x.abs(), ground.normal.y.abs());
                if !(ax < 0.9998 && ay < 0.9998) {
                    return false;
                }
                let sx = 1.0 / (1.0 - ax * ax).sqrt();
                let sy = 1.0 / (1.0 - ay * ay).sqrt();
                scale.x = sx;
                scale.y = sy;
                if g.scale_type == 9 {
                    scale.z = (sx + sy) * 0.5;
                }
            }
            _ => {}
        }
        if !final_height_allowed(g, place, &ground, water) {
            return false;
        }
        // D2-03615
        if g.check_slope_radius > 0.0 && !self.check_radius(g, place, *scale) {
            return false;
        }
        g.skip_position_check || !self.collides(snapshot, radius, g.radius_in_3d)
    }
}

/// The heading a rotation turns local north to.
fn rotation_heading(q: Quat) -> f32 {
    let r = rotation_rows(q);
    normal_heading(Vec3::new(r[1][0], r[1][1], 0.0))
}

/// A group's clearance radius, scaled by its scale when it says so.
// D2-12426
fn clearance_radius(g: &ScatterGroup, s: Vec3) -> f32 {
    if !g.scale_clearance {
        return g.clearance_radius;
    }
    let m = if g.radius_in_3d {
        s.x.max(s.y).max(s.z)
    } else {
        s.x.max(s.y)
    };
    m * g.clearance_radius
}

/// A level's distance at degrade value `v`: from its least to its ideal distance below one half,
/// from its ideal to its greatest above.
// D2-12413
fn degrade_distance(l: &auberean_assets::visual::SceneLevel, v: f32) -> f32 {
    if v < 0.5 {
        2.0 * v * (l.ideal_distance - l.min_distance) + l.min_distance
    } else {
        2.0 * (v - 0.5) * (l.max_distance - l.ideal_distance) + l.ideal_distance
    }
}

/// A group's scale: its parent's (when it scales its members' sizes) times a hashed scale
/// between its least and greatest, each axis or together as its scale kind says; `None` for a
/// kind that cannot be placed this way.
fn group_scale(g: &ScatterGroup, x: u32, y: u32, parent: Vec3, seed: u32) -> Option<Vec3> {
    let mut s = if g.scale_subobject_sizes {
        parent
    } else {
        Vec3::new(1.0, 1.0, 1.0)
    };
    let (lo, hi) = (g.min_scale, g.max_scale);
    let same = (lo.x - hi.x).abs() < 0.0002
        && (lo.y - hi.y).abs() < 0.0002
        && (lo.z - hi.z).abs() < 0.0002;
    let lerp = |a: f32, b: f32, t: f32| (b - a) * t + a;
    let r1 = || rand(x, y, seed.wrapping_add(5_332_593));
    let r2 = || rand(x, y, seed.wrapping_add(0x0DE8_76C7));
    let r3 = || rand(x, y, seed.wrapping_add(0x17E0_784F));
    if same || g.scale_type == 7 || g.scale_type == 8 {
        return Some(Vec3::new(s.x * lo.x, s.y * lo.y, s.z * lo.z));
    }
    match g.scale_type {
        0 => {
            s.x *= lerp(lo.x, hi.x, r1());
            s.y *= lerp(lo.y, hi.y, r2());
            s.z *= lerp(lo.z, hi.z, r3());
        }
        1 => {
            let a = r1();
            s.x *= lerp(lo.x, hi.x, a);
            s.y *= lerp(lo.y, hi.y, a);
            s.z *= lerp(lo.z, hi.z, r2());
        }
        2 => {
            let a = r1();
            s.x *= lerp(lo.x, hi.x, a);
            s.y *= lerp(lo.y, hi.y, r2());
            s.z *= lerp(lo.z, hi.z, a);
        }
        3 => {
            let b = r2();
            s.x *= lerp(lo.x, hi.x, r1());
            s.y *= lerp(lo.y, hi.y, b);
            s.z *= lerp(lo.z, hi.z, b);
        }
        4 => {
            let a = r1();
            s.x *= lerp(lo.x, hi.x, a);
            s.y *= lerp(lo.y, hi.y, a);
            s.z *= lerp(lo.z, hi.z, a);
        }
        5 => {
            s.x *= lerp(lo.x, hi.x, r1());
            s.y *= lerp(lo.y, hi.y, r2());
        }
        6 => {
            let a = r1();
            s.x *= lerp(lo.x, hi.x, a);
            s.y *= lerp(lo.y, hi.y, a);
        }
        _ => return None,
    }
    Some(s)
}

/// Whether a group's final height is one its rules allow: over water deeper than nothing, the
/// water's depth and the height above or below the surface; elsewhere the height above dry land
/// or below ground.
// D2-12435
fn final_height_allowed(
    g: &ScatterGroup,
    place: &Place,
    ground: &Facet,
    water: Option<f32>,
) -> bool {
    let o = place.origin;
    let floor = ground.height(o.x, o.y, o.z);
    let allowed = |kind: u32, min: f32, max: f32, d: f32| match kind {
        0 => false,
        1 => min <= d,
        2 => d <= max && min <= d,
        _ => true,
    };
    if let Some(w) = water {
        let depth = w - floor;
        if depth > 0.0 {
            let deep_enough = match g.water_depth_type {
                0 => false,
                1 => g.min_water_depth <= depth,
                2 => depth <= g.max_water_depth && g.min_water_depth <= depth,
                _ => true,
            };
            if !deep_enough {
                return false;
            }
            let above = o.z - w;
            return if above >= 0.0 {
                allowed(
                    g.above_water_type,
                    g.min_above_water,
                    g.max_above_water,
                    above,
                )
            } else {
                allowed(
                    g.below_water_type,
                    g.min_below_water,
                    g.max_below_water,
                    -above,
                )
            };
        }
    }
    let h = o.z - floor;
    if h < 0.0 {
        allowed(
            g.below_ground_type,
            g.min_below_ground,
            g.max_below_ground,
            -h,
        )
    } else {
        allowed(
            g.above_dry_land_type,
            g.min_above_dry_land,
            g.max_above_dry_land,
            h,
        )
    }
}

/// Every scenery object of one landblock, in placement order.
// D2-03693, D2-03695
#[must_use]
pub fn block_scenery(input: &SceneryInput<'_>, source: &dyn SceneSource) -> Vec<SceneryObject> {
    let geo = &input.block.geometry;
    let to = |v: &dereth_primitives::Vec3| Vec3::new(v.x, v.y, v.z);
    let n = geo.squares_per_side;
    let side = geo.vertices_per_side;
    let mut facets = Vec::with_capacity(2 * n * n);
    for sx in 0..n {
        for sy in 0..n {
            let v = |i: usize, j: usize| to(&geo.vertices[i * side + j]);
            let (sw, se, nw, ne) = (v(sx, sy), v(sx + 1, sy), v(sx, sy + 1), v(sx + 1, sy + 1));
            if geo.sw_to_ne_cut[sx * n + sy] {
                facets.push(Facet::new(sw, se, ne));
                facets.push(Facet::new(sw, ne, nw));
            } else {
                facets.push(Facet::new(sw, se, nw));
                facets.push(Facet::new(ne, nw, se));
            }
        }
    }
    let mut p = Placer {
        input,
        source,
        facets,
        clear: input
            .obstacles
            .iter()
            .enumerate()
            .map(|(k, o)| Clearance {
                origin: o.origin,
                radius: 0.0,
                radius_in_3d: false,
                group: false,
                done: true,
                object: Some(k),
            })
            .collect(),
        out: Vec::new(),
    };
    let grid = input.land.grid();
    #[allow(clippy::cast_precision_loss)]
    let step = grid.cell_length / grid.vertices_per_cell as f32 - 0.0002;
    #[allow(clippy::cast_sign_loss)]
    let (bx, by) = (input.block.x as u32, input.block.y as u32);
    #[allow(clippy::cast_possible_truncation)]
    let per_block = n as u32;
    for i in 0..side {
        for j in 0..side {
            let word = input.vertices[i * side + j];
            #[allow(clippy::cast_possible_truncation)]
            let (x, y) = (per_block * bx + i as u32, per_block * by + j as u32);
            let kind = input.land.word.terrain.of(word);
            let field = input.land.word.scene.of(word);
            let Some(scene) = vertex_scene(
                input.terrain,
                input.scenes,
                kind,
                field,
                scene_fraction(x, y),
            ) else {
                continue;
            };
            #[allow(clippy::cast_precision_loss)]
            let origin = Vec3::new(
                i as f32 * step,
                j as f32 * step,
                geo.vertices[i * side + j].z,
            );
            let place = Place {
                origin,
                rotation: Quat::IDENTITY,
            };
            let mut count = 0;
            p.scene(
                x,
                y,
                scene,
                &place,
                Vec3::new(1.0, 1.0, 1.0),
                &mut count,
                START_SEED,
                false,
                0,
            );
        }
    }
    p.out
}

/// Whether point `(x, y)` of a block is on the road: its `side` by `side` landscape `words` say
/// which vertices are, and a road reaches out from them by its width.
// D2-03618
#[must_use]
pub fn on_road(land: &LandConstants, side: usize, words: &[u32], x: f32, y: f32) -> bool {
    let step = land.grid().vertex_spacing();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (sx, sy) = (
        (x * (1.0 / step)).floor() as i64,
        (y * (1.0 / step)).floor() as i64,
    );
    let road = |i: i64, j: i64| {
        let (Ok(i), Ok(j)) = (usize::try_from(i), usize::try_from(j)) else {
            return false;
        };
        if i >= side || j >= side {
            return false;
        }
        land.word.road.of(words[i * side + j]) != 0
    };
    #[allow(clippy::cast_precision_loss)]
    let (dx, dy) = (x - sx as f32 * step, y - sy as f32 * step);
    dereth_terrain::road::on_road_cell_with(
        road(sx, sy),
        road(sx, sy + 1),
        road(sx + 1, sy),
        road(sx + 1, sy + 1),
        dx,
        dy,
        step,
        land.road_width,
    )
}
