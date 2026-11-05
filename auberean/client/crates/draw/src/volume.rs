//! The dynamic objects' shadows at the recommended quality: each mesh of an object that casts one
//! throws a volume of shadow away from the light that falls on it, and whatever the volumes enclose
//! is darkened.
//!
//! A mesh's volume is built from its triangles facing the light: each such triangle once pushed a
//! hair's breadth along the light (the near cap), once pushed sixty meters along it (the far cap,
//! turned to face out), and along each edge it shares with no other triangle facing the light, a
//! side joining the two. Every frame takes the volumes of the meshes as posed, near objects first,
//! until the frame's room for them is spent; a mesh standing as it did in the frame before, under
//! the same light, keeps the volume it had. An object farther than forty meters from the camera
//! casts none. Drawn after the opaque objects that cast none, the volumes count
//! where the camera's sight to the nearest surface passes through them, and those places are
//! darkened by the shadow's strength; the objects casting them are drawn after that, so their own
//! shadows never fall on them.

use std::collections::HashMap;
use std::sync::Arc;

use auberean_assets::mesh::Mesh;
use auberean_primitives::Vec3;
use auberean_world::color::Color;
use auberean_world::math::{cross, dot, length, normalize, scale, sub, Mat4};
use auberean_world::records::WorldRecords;

use crate::scene::ObjectBatch;

/// The longest a volume reaches along the light, in meters (and how far it reaches when it is
/// given no length).
pub const VOLUME_LENGTH: f32 = 60.0;

/// How far along the light a volume's near cap is pushed off its triangles: a hair's breadth when
/// the volumes are made for the frame, more when a mesh makes its own as it is drawn.
pub const NEAR_OFFSET: f32 = 0.001;
pub const DRAWN_NEAR_OFFSET: f32 = 0.05;

/// The farthest an object stands from the camera and still casts a volume, in meters.
pub const VOLUME_REACH: f32 = 40.0;

/// A triangle whose normal's share along the light is less than this faces the light (one met
/// edge on does); and nearer than this on every axis, two vertices are one.
const FACING: f32 = 0.0002;

/// How the dynamic objects' shadows are drawn: the graphics preference's choices.
pub mod setting {
    pub const DISABLED: u32 = 0;
    /// A dark disc on the ground under each object.
    pub const SIMPLE: u32 = 1;
    /// A volume of shadow from each object.
    pub const VOLUMETRIC: u32 = 2;
    /// Volumes the objects also cast on themselves.
    pub const SELF_SHADOWING: u32 = 3;
}

/// The room a frame's volumes have: how many volumes, and how many vertices and indices between
/// them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VolumeRoom {
    pub volumes: usize,
    pub vertices: usize,
    pub indices: usize,
}

impl VolumeRoom {
    /// A frame's room: a hundred volumes, forty thousand vertices and sixty thousand indices.
    pub const FRAME: Self = Self {
        volumes: 100,
        vertices: 40_000,
        indices: 60_000,
    };
}

/// One triangle of a mesh's level as its volumes see it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Triangle {
    /// Its corners, as places in the level's list of distinct vertices.
    pub corners: [u16; 3],
    /// Its edges (first to second corner, second to third, third to first), as places in the
    /// level's list of edges, and whether each met an edge an earlier triangle made.
    pub edges: [u32; 3],
    pub shared: [bool; 3],
}

/// One edge between two distinct vertices, the triangle that made it and the one that met it
/// running the other way, when one did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Edge {
    pub from: u16,
    pub to: u16,
    pub first: u32,
    pub second: Option<u32>,
}

/// A mesh level's triangles joined at their edges: its distinct vertices (vertices standing within
/// two ten-thousandths of a meter of each other on every axis are one), its triangles, its edges.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Connectivity {
    /// Each distinct vertex as the first of the mesh's vertices found there.
    pub local: Vec<u16>,
    pub triangles: Vec<Triangle>,
    pub edges: Vec<Edge>,
}

impl Connectivity {
    /// The connectivity of a level whose fragments' triangles (three indices each) are `fragments`
    /// over vertices standing at `positions`: the distinct vertices in the order the triangles
    /// first meet them, then the triangles in order, each edge met again running the other way by
    /// a later triangle joined to the first edge still unmet. `None` when the level has no
    /// triangle or names a vertex it does not have.
    // D2-05495, D2-05493, D2-05494
    #[must_use]
    pub fn of<'a>(
        positions: &[Vec3],
        fragments: impl IntoIterator<Item = &'a [u16]> + Clone,
    ) -> Option<Self> {
        let near = |a: Vec3, b: Vec3| {
            (a.x - b.x).abs() < FACING && (a.y - b.y).abs() < FACING && (a.z - b.z).abs() < FACING
        };
        let mut local: Vec<u16> = Vec::new();
        let mut map = vec![0u16; positions.len()];
        for f in fragments.clone() {
            for &i in f {
                let p = *positions.get(usize::from(i))?;
                let at = local
                    .iter()
                    .position(|&j| j == i || near(p, positions[usize::from(j)]));
                map[usize::from(i)] = match at {
                    Some(k) => u16::try_from(k).ok()?,
                    None => {
                        local.push(i);
                        u16::try_from(local.len() - 1).ok()?
                    }
                };
            }
        }
        if local.is_empty() {
            return None;
        }
        let mut triangles = Vec::new();
        let mut edges: Vec<Edge> = Vec::new();
        // the edges still unmet, by their ends, in the order they were made
        let mut open: HashMap<(u16, u16), Vec<u32>> = HashMap::new();
        for f in fragments {
            for t in f.as_chunks::<3>().0 {
                let tri = u32::try_from(triangles.len()).ok()?;
                let c = [
                    map[usize::from(t[0])],
                    map[usize::from(t[1])],
                    map[usize::from(t[2])],
                ];
                let mut e = [0u32; 3];
                let mut shared = [false; 3];
                for k in 0..3 {
                    let (a, b) = (c[k], c[(k + 1) % 3]);
                    let met = open
                        .get_mut(&(b, a))
                        .and_then(|v| (!v.is_empty()).then(|| v.remove(0)));
                    if let Some(m) = met {
                        edges[m as usize].second = Some(tri);
                        e[k] = m;
                        shared[k] = true;
                    } else {
                        let m = u32::try_from(edges.len()).ok()?;
                        edges.push(Edge {
                            from: a,
                            to: b,
                            first: tri,
                            second: None,
                        });
                        open.entry((a, b)).or_default().push(m);
                        e[k] = m;
                    }
                }
                triangles.push(Triangle {
                    corners: c,
                    edges: e,
                    shared,
                });
            }
        }
        (!triangles.is_empty()).then_some(Self {
            local,
            triangles,
            edges,
        })
    }
}

/// One volume as its mesh makes it, in the mesh's frame.
#[derive(Clone, Debug, PartialEq)]
pub struct VolumeMesh {
    pub vertices: Vec<Vec3>,
    pub indices: Vec<u16>,
}

/// The volume a mesh level with connectivity `c` casts, its vertices standing at `positions`, the
/// light travelling along unit direction `toward` (in the mesh's frame): its near cap pushed
/// `offset` along the light, its far cap `length` (sixty meters when nearly none, and never more),
/// the vertices and indices no more than `vertex_room` and `index_room`. `None` when it has three
/// triangles or fewer, none faces the light, or the volume does not fit. A triangle the light
/// meets edge on (or nearly) faces it.
// D2-05499
#[must_use]
pub fn shadow_volume(
    c: &Connectivity,
    positions: &[Vec3],
    toward: Vec3,
    length: f32,
    offset: f32,
    vertex_room: usize,
    index_room: usize,
) -> Option<VolumeMesh> {
    if c.triangles.len() < 4 {
        return None;
    }
    let length = if length.abs() < FACING {
        VOLUME_LENGTH
    } else {
        length.min(VOLUME_LENGTH)
    };
    let at = |corner: u16| -> Option<Vec3> {
        let v = *c.local.get(usize::from(corner))?;
        positions.get(usize::from(v)).copied()
    };
    let mut vertices = Vec::new();
    let mut indices: Vec<u16> = Vec::new();
    // each triangle facing the light: where its six vertices start
    let mut lit: Vec<Option<u16>> = vec![None; c.triangles.len()];
    for (k, t) in c.triangles.iter().enumerate() {
        let [p0, p1, p2] = [at(t.corners[0])?, at(t.corners[1])?, at(t.corners[2])?];
        let n = unit(cross(sub(p1, p0), sub(p2, p0)));
        // a triangle the light meets, or meets edge on (a degenerate one too)
        if dot(n, toward) >= FACING || dot(n, toward).is_nan() {
            continue;
        }
        let base = vertices.len();
        if base + 6 > vertex_room {
            return None;
        }
        let first = u16::try_from(base).ok()?;
        lit[k] = Some(first);
        for p in [p0, p1, p2] {
            vertices.push(add(p, scale(toward, offset)));
            vertices.push(add(p, scale(toward, length)));
        }
        if indices.len() + 3 > index_room {
            return None;
        }
        indices.extend([first, first + 2, first + 4]);
    }
    if lit.iter().all(Option::is_none) {
        return None;
    }
    for (k, t) in c.triangles.iter().enumerate() {
        let Some(first) = lit[k] else {
            continue;
        };
        if indices.len() + 3 > index_room {
            return None;
        }
        indices.extend([first + 5, first + 3, first + 1]);
        let tri = u32::try_from(k).ok()?;
        for (side, &e) in t.edges.iter().enumerate() {
            let edge = c.edges.get(e as usize)?;
            let other = if edge.first == tri {
                edge.second
            } else {
                Some(edge.first)
            };
            if other.is_some_and(|o| lit.get(o as usize).copied().flatten().is_some()) {
                continue;
            }
            if indices.len() + 6 > index_room {
                return None;
            }
            #[allow(clippy::cast_possible_truncation)] // a corner, 0 to 2
            let (a, b) = (first + 2 * side as u16, first + 2 * ((side as u16 + 1) % 3));
            indices.extend([a, a + 1, b, a + 1, b + 1, b]);
        }
    }
    Some(VolumeMesh { vertices, indices })
}

fn add(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(a.x + b.x, a.y + b.y, a.z + b.z)
}

/// `a` made a unit, or none when it has no length (as the original's normalizing leaves it).
fn unit(a: Vec3) -> Vec3 {
    let l = length(a);
    if l > 0.0 {
        scale(a, 1.0 / l)
    } else {
        Vec3::ZERO
    }
}

/// The level of `mesh` whose triangles its volume is made from, drawn at detail `detail` (1 full
/// to 0 least) with model detail `model_detail`: of the levels marked for that model detail (every
/// level when none is), the count less one, or less two at the highest model detail when the
/// object is drawn at half detail or more, taken as a level of the mesh itself.
// D2-05510
#[must_use]
pub fn volume_level(mesh: &Mesh, detail: f32, model_detail: u32) -> Option<usize> {
    let bit = 1u32.checked_shl(model_detail).unwrap_or(0);
    let marked = mesh.levels.iter().filter(|l| l.detail & bit != 0).count();
    let (count, model_detail) = if marked == 0 {
        (mesh.levels.len(), None)
    } else {
        (marked, Some(model_detail))
    };
    let level = if model_detail == Some(2) && detail >= 0.5 && count >= 2 {
        count - 2
    } else {
        count.checked_sub(1)?
    };
    (level < mesh.levels.len()).then_some(level)
}

/// One mesh's volume of shadow, in the frame of the draw it shadows, and where that frame stands.
#[derive(Clone, Debug, PartialEq)]
pub struct Volume {
    pub vertices: Arc<Vec<[f32; 3]>>,
    pub indices: Arc<Vec<u16>>,
    pub transform: Mat4,
}

/// The darkening the dynamic objects' volumes lay over a frame: the volumes, near objects' first,
/// and how much their shadow takes away, per channel.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Volumes {
    pub volumes: Vec<Volume>,
    pub strength: Color,
}

impl Volumes {
    /// How much a volume's shadow takes away: the shadow color of the light the frame is drawn
    /// under, held to 0..1, alpha one. Inside, it is a mid gray (0.45).
    // D2-10596, D2-11007
    #[must_use]
    pub fn strength_of(shadow_color: Color, inside: bool) -> Color {
        let c = if inside {
            Color::new(0.45, 0.45, 0.45, 1.0)
        } else {
            shadow_color
        };
        Color::new(
            c.r.clamp(0.0, 1.0),
            c.g.clamp(0.0, 1.0),
            c.b.clamp(0.0, 1.0),
            1.0,
        )
    }
}

/// The inverse of `m`'s first three rows: the direction whose image under `m` is a given one.
fn direction_preimage(m: &Mat4, d: Vec3) -> Option<Vec3> {
    let a = &m.0;
    let det = a[0][0] * (a[1][1] * a[2][2] - a[1][2] * a[2][1])
        - a[0][1] * (a[1][0] * a[2][2] - a[1][2] * a[2][0])
        + a[0][2] * (a[1][0] * a[2][1] - a[1][1] * a[2][0]);
    if det.abs() < 1e-12 {
        return None;
    }
    // d = x · A (row vector), so x = d · A⁻¹, A⁻¹ = adj(A) / det
    let inv = [
        [
            a[1][1] * a[2][2] - a[1][2] * a[2][1],
            a[0][2] * a[2][1] - a[0][1] * a[2][2],
            a[0][1] * a[1][2] - a[0][2] * a[1][1],
        ],
        [
            a[1][2] * a[2][0] - a[1][0] * a[2][2],
            a[0][0] * a[2][2] - a[0][2] * a[2][0],
            a[0][2] * a[1][0] - a[0][0] * a[1][2],
        ],
        [
            a[1][0] * a[2][1] - a[1][1] * a[2][0],
            a[0][1] * a[2][0] - a[0][0] * a[2][1],
            a[0][0] * a[1][1] - a[0][1] * a[1][0],
        ],
    ];
    Some(Vec3::new(
        (d.x * inv[0][0] + d.y * inv[1][0] + d.z * inv[2][0]) / det,
        (d.x * inv[0][1] + d.y * inv[1][1] + d.z * inv[2][1]) / det,
        (d.x * inv[0][2] + d.y * inv[1][2] + d.z * inv[2][2]) / det,
    ))
}

/// What a volume is made from: the mesh level, the list its vertices stand in, the light along its
/// own frame and how far its near cap is pushed.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Asked {
    mesh: u32,
    level: usize,
    places: usize,
    toward: [u32; 3],
    near: u32,
}

/// A volume's vertices and indices.
type Lists = (Arc<Vec<[f32; 3]>>, Arc<Vec<u16>>);

/// A volume made, or that the mesh throws none, and the lists it was made from, held so that their
/// addresses stay theirs while it is kept.
struct Made {
    volume: Option<Lists>,
    _posed: Option<Arc<crate::mesh::PosedList>>,
    _vertices: Arc<Vec<crate::mesh::MeshVertex>>,
    used: u64,
}

/// Makes the frame's volumes, keeping each mesh level's connectivity once it is first built, and
/// the volumes the frame before took.
#[derive(Default)]
pub struct VolumeMaker {
    meshes: HashMap<u32, Option<Arc<Mesh>>>,
    connectivity: HashMap<(u32, usize), Option<Arc<Connectivity>>>,
    made: HashMap<Asked, Made>,
    frame: u64,
}

impl std::fmt::Debug for VolumeMaker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VolumeMaker")
            .field("meshes", &self.connectivity.len())
            .finish_non_exhaustive()
    }
}

impl VolumeMaker {
    /// A frame starts: the volumes the one before it did not take are let go.
    pub fn begin_frame(&mut self) {
        let last = self.frame;
        self.made.retain(|_, m| m.used == last);
        self.frame += 1;
    }

    fn mesh(&mut self, records: &WorldRecords, id: u32) -> Option<Arc<Mesh>> {
        self.meshes
            .entry(id)
            .or_insert_with(|| records.get::<Mesh>(id).map(Arc::new))
            .clone()
    }

    /// Adds to `out` the volumes one object casts, its draws `batches` (each mesh it draws once,
    /// as posed for the frame, found by its mesh and vertices), its shadow falling along unit
    /// direction `toward` in the world, drawn at `detail` with `model_detail`, while `room` lasts:
    /// a mesh whose volume does not fit is left out, and nothing more is made once a hundred are.
    // D2-10547, D2-05510
    #[allow(clippy::too_many_arguments)]
    pub fn cast(
        &mut self,
        records: &WorldRecords,
        batches: &[ObjectBatch],
        toward: Vec3,
        detail: f32,
        model_detail: u32,
        room: &mut VolumeRoom,
        out: &mut Vec<Volume>,
    ) {
        let level = |mesh: &Mesh| volume_level(mesh, detail, model_detail);
        self.make(records, batches, toward, &level, NEAR_OFFSET, room, out);
    }

    /// As [`VolumeMaker::cast`], for an object that makes its volumes as it is drawn (the self
    /// shadowing setting): each mesh's from the level it is drawn at (`detail`, `model_detail`),
    /// its near cap pushed five centimeters along the light, so a surface facing the light is not
    /// darkened by its own cap while the shadow its other parts throw still falls on it.
    // D2-05510, D2-10547
    #[allow(clippy::too_many_arguments)]
    pub fn cast_as_drawn(
        &mut self,
        records: &WorldRecords,
        batches: &[ObjectBatch],
        toward: Vec3,
        detail: f32,
        model_detail: u32,
        room: &mut VolumeRoom,
        out: &mut Vec<Volume>,
    ) {
        let level = |mesh: &Mesh| crate::mesh::level_at(mesh, detail, model_detail);
        self.make(
            records,
            batches,
            toward,
            &level,
            DRAWN_NEAR_OFFSET,
            room,
            out,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn make(
        &mut self,
        records: &WorldRecords,
        batches: &[ObjectBatch],
        toward: Vec3,
        level_of: &dyn Fn(&Mesh) -> Option<usize>,
        near: f32,
        room: &mut VolumeRoom,
        out: &mut Vec<Volume>,
    ) {
        let mut seen: Vec<(u32, usize)> = Vec::new();
        for b in batches {
            if b.mesh == 0 || room.volumes == 0 {
                continue;
            }
            let key = (b.mesh, b.places());
            if seen.contains(&key) {
                continue;
            }
            seen.push(key);
            let Some(mesh) = self.mesh(records, b.mesh) else {
                continue;
            };
            let Some(level) = level_of(&mesh) else {
                continue;
            };
            // the light along the mesh's own frame, its place's movement left out
            let Some(local) = direction_preimage(&b.transform, toward) else {
                continue;
            };
            let local = normalize(local);
            let asked = Asked {
                mesh: b.mesh,
                level,
                places: b.places(),
                toward: [local.x.to_bits(), local.y.to_bits(), local.z.to_bits()],
                near: near.to_bits(),
            };
            let frame = self.frame;
            if let Some(m) = self.made.get_mut(&asked) {
                m.used = frame;
            } else {
                let c = self
                    .connectivity
                    .entry((b.mesh, level))
                    .or_insert_with(|| {
                        let stored: Vec<Vec3> = crate::mesh::vertices(&mesh.vertices)?
                            .iter()
                            .map(|v| Vec3::new(v.position[0], v.position[1], v.position[2]))
                            .collect();
                        Connectivity::of(
                            &stored,
                            mesh.levels[level]
                                .fragments
                                .iter()
                                .map(|f| f.indices.as_slice()),
                        )
                        .map(Arc::new)
                    })
                    .clone();
                let Some(c) = c else {
                    continue;
                };
                let walked: Vec<Vec3>;
                let positions: &[Vec3] = if let Some(list) = &b.posed {
                    list.positions()
                } else {
                    walked = {
                        let mut all = Vec::with_capacity(b.vertices.len());
                        b.each_position(|[x, y, z]| all.push(Vec3::new(x, y, z)));
                        all
                    };
                    &walked
                };
                // Whether it fits is the frame's to say, taking it.
                let volume = shadow_volume(&c, positions, local, 0.0, near, usize::MAX, usize::MAX)
                    .map(|v| {
                        (
                            Arc::new(v.vertices.iter().map(|p| [p.x, p.y, p.z]).collect()),
                            Arc::new(v.indices),
                        )
                    });
                self.made.insert(
                    asked,
                    Made {
                        volume,
                        _posed: b.posed.clone(),
                        _vertices: Arc::clone(&b.vertices),
                        used: frame,
                    },
                );
            }
            let Some((vertices, indices)) = self.made.get(&asked).and_then(|m| m.volume.as_ref())
            else {
                continue;
            };
            if vertices.len() > room.vertices || indices.len() > room.indices {
                continue;
            }
            room.volumes -= 1;
            room.vertices -= vertices.len();
            room.indices -= indices.len();
            out.push(Volume {
                vertices: Arc::clone(vertices),
                indices: Arc::clone(indices),
                transform: b.transform,
            });
        }
    }
}

/// The way an object's shadow falls (a unit direction in the world) from the lights it keeps,
/// strongest first, as the first decides: when it is a directional light, its direction; else the
/// lights on the object's side (inside or out) summed, each the way it throws the shadow times its
/// weight there, made a unit (straight down when they sum to nothing). With no light the sun's
/// direction `sun`.
///
/// `lights` gives each kept light: its kind, its place, the cell it stands in, the direction it
/// shines along (a directional light) and its weight at the object; `at` and `cell` are the
/// object's place and cell, `up` its own up.
// D2-10630
#[must_use]
pub fn shadow_direction(
    lights: &[ShadowLight],
    at: Vec3,
    inside: bool,
    up: Vec3,
    sun: Vec3,
) -> Vec3 {
    let Some(first) = lights.first() else {
        return sun;
    };
    if first.kind == ShadowLightKind::Directional {
        return first.direction;
    }
    let mut sum = Vec3::ZERO;
    for l in lights.iter().filter(|l| l.inside == inside) {
        let v = point_light_shadow(l, at, up, sun);
        sum = add(sum, scale(v, l.weight));
    }
    let len = length(sum);
    if len < FACING {
        Vec3::new(0.0, 0.0, -1.0)
    } else {
        scale(sum, 1.0 / len)
    }
}

/// The kinds of light that throw an object's shadow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShadowLightKind {
    Point,
    Directional,
    /// A light that shines with the day's light outside: it throws the shadow the sun does.
    Sky,
}

/// One light an object keeps, as its shadow sees it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowLight {
    pub kind: ShadowLightKind,
    /// Its place in the object's frame of the world (its block's offset taken in).
    pub origin: Vec3,
    /// Whether it stands inside.
    pub inside: bool,
    pub direction: Vec3,
    /// How strongly it lights the object, 0 to 1.
    pub weight: f32,
}

/// The way light `l` throws the shadow of an object at `at` whose own up is `up`: a sky light the
/// sun's way `sun`; any other from itself to the object, leaning a fifth of the way down the
/// object's own up, made a unit.
// D2-10594
#[must_use]
pub fn point_light_shadow(l: &ShadowLight, at: Vec3, up: Vec3, sun: Vec3) -> Vec3 {
    if l.kind == ShadowLightKind::Sky {
        return sun;
    }
    let away = unit(sub(at, l.origin));
    unit(sub(away, scale(up, 0.2)))
}

/// One object casting volumes in a frame: how far it stands from the camera, its draws (a run of
/// the frame's objects), the way its shadow falls, and the detail it is drawn at.
#[derive(Clone, Debug, PartialEq)]
pub struct Caster {
    pub distance: f32,
    pub draws: std::ops::Range<usize>,
    pub toward: Vec3,
    pub detail: f32,
}

/// The objects the world places that cast a volume of shadow in `frame` and stand within `reach`
/// of its camera (forty meters, [`VOLUME_REACH`], when the frame makes the volumes; any distance
/// when each object makes its own as it is drawn), as they stand among its draws: each placed
/// object's run of draws, how far it stands, the way its shadow falls (its own lights' way, else
/// `sun`) and the detail its first part is drawn at there (full when its setup gives no
/// distances).
// D2-06220, D2-06658
#[must_use]
pub fn placed_casters(frame: &crate::scene::Frame, sun: Vec3, reach: f32) -> Vec<Caster> {
    let eye = frame.camera.position;
    let mut out = Vec::new();
    let mut k = 0;
    while k < frame.objects.len() {
        let Some(c) = frame.objects[k].caster else {
            k += 1;
            continue;
        };
        let start = k;
        while frame
            .objects
            .get(k)
            .is_some_and(|b| b.caster.is_some_and(|o| o.object == c.object))
        {
            k += 1;
        }
        let distance = length(sub(c.at, eye));
        if distance > reach {
            continue;
        }
        let detail = c
            .detail
            .map_or(1.0, |d| crate::mesh::detail_at(&d, distance).unwrap_or(0.0));
        out.push(Caster {
            distance,
            draws: start..k,
            toward: c.toward.unwrap_or(sun),
            detail,
        });
    }
    out
}
