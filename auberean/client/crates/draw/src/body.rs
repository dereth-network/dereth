//! A posed body as mesh batches: each part group's meshes skinned to its posed skeleton on the CPU,
//! as the client blends them, or (a mesh with no bone weights) carried whole by its bone. Each part
//! is drawn as its appearance changes it: the meshes swapped in, the materials replaced and
//! modified, the opacity.
//!
//! Only where a vertex stands and which way it faces moves with a pose: those go in a list of their
//! own, and a draw takes the rest of the vertex from the mesh's list, kept from frame to frame. A
//! pose that stands still (a body steps its animation less often than a fast frame rate draws) has
//! the list it had.

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::Hasher;
use std::sync::Arc;

use auberean_animation::body::{Body, PartPose};
use auberean_animation::look::PartLook;
use auberean_animation::skin::{influences, skin_vertex, Influences};
use auberean_assets::mesh::Mesh;
use auberean_primitives::Vec3;
use auberean_world::color::Color;
use auberean_world::math::Mat4;
use auberean_world::records::WorldRecords;

use crate::material::{
    device_flags, draw_layers, instance_material_with, material_kind, DrawLayer,
};
use crate::mesh::{level_at, vertices, MeshVertex, PosedVertex};
use crate::scene::Lod;
use crate::scene::ObjectBatch;

struct Loaded {
    mesh: Arc<Mesh>,
    vertices: Arc<Vec<MeshVertex>>,
    /// Where each of `vertices` stands at rest and which way it faces.
    rest: Vec<(Vec3, Vec3)>,
    influences: Option<Vec<Influences>>,
}

impl Loaded {
    /// Where the vertices stand on a skinned mesh, in its pose.
    fn skinned(&self, inf: &[Influences], pose: &PartPose) -> Vec<PosedVertex> {
        let none = Influences::default();
        let whole = pose.world == Mat4::IDENTITY;
        self.rest
            .iter()
            .enumerate()
            .map(|(k, (p, n))| {
                let (p, n) = skin_vertex(*p, *n, inf.get(k).unwrap_or(&none), &pose.palette);
                let (p, n) = if whole {
                    (p, n)
                } else {
                    (pose.world.point(p), pose.world.direction(n))
                };
                let n = normalize(n);
                PosedVertex {
                    position: [p.x, p.y, p.z],
                    normal: [n.x, n.y, n.z],
                }
            })
            .collect()
    }

    /// Where the vertices stand on a mesh carried whole by `bone`.
    fn carried(&self, bone: &Mat4) -> Vec<PosedVertex> {
        self.rest
            .iter()
            .map(|(p, n)| {
                let p = bone.point(*p);
                let n = normalize(bone.direction(*n));
                PosedVertex {
                    position: [p.x, p.y, p.z],
                    normal: [n.x, n.y, n.z],
                }
            })
            .collect()
    }
}

/// A pose's matrices, for telling it from others by their exact bits.
fn hash_matrices<'m>(matrices: impl IntoIterator<Item = &'m Mat4>) -> u64 {
    let mut h = DefaultHasher::new();
    for m in matrices {
        h.write(bytemuck::bytes_of(&m.0));
    }
    h.finish()
}

/// A mesh by its record's id, and a pose by its matrices' hash.
type PosedKey = (u32, u64);

/// A material as an appearance leaves it: the mesh's material, the template that replaces its
/// own, the modifiers added after its own; and the material detail it is drawn at.
type MaterialKey = (u32, Option<u32>, Vec<u32>, u32);

/// Turns posed bodies into batches, keeping what it decoded.
pub struct BodyDrawer {
    meshes: HashMap<u32, Option<Arc<Loaded>>>,
    materials: HashMap<MaterialKey, Vec<DrawLayer>>,
    kinds: HashMap<u32, u32>,
    present: HashMap<u32, bool>,
    /// The lists of this frame's poses, and of the last frame's.
    posed: HashMap<PosedKey, Arc<Vec<PosedVertex>>>,
    posed_before: HashMap<PosedKey, Arc<Vec<PosedVertex>>>,
    /// The model detail preference, 0 (low) to 2 (high): which of each mesh's levels are drawn.
    pub model_detail: u32,
    /// The material detail preference, 0 (low) to 2 (high): which of each material's layers are
    /// drawn.
    pub material_detail: u32,
    /// Where the camera stands in the frame bodies are placed in, when they degrade with distance
    /// (not in a viewport of the interface).
    pub eye: Option<Vec3>,
    /// The frame has a water's reflection, which sees each body by its distances at 40%.
    pub reflecting: bool,
    /// What the distances of a body both mobile and dynamic are scaled by: 1, or
    /// [`adaptive_scale`] with the adaptive degrade on.
    pub moving_scale: f32,
    /// What the distances of any other body are scaled by: the scenery draw distance's
    /// ([`crate::mesh::scenery_scale`]).
    pub still_scale: f32,
    scale: f32,
}

/// How see-through the player's own body has become while the pointer is over it: with the option
/// on, fading toward 0.3 at ten times its own opacity a second while the pointer is on it, back
/// toward whole when it is not.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HoverFade {
    /// How opaque it is.
    pub opacity: f32,
    last: f64,
}

impl Default for HoverFade {
    fn default() -> Self {
        Self {
            opacity: 1.0,
            last: 0.0,
        }
    }
}

impl HoverFade {
    /// Its turn at `now`, seconds, with the pointer on the body or not.
    // D2-06220
    pub fn step(&mut self, now: f64, over: bool) {
        #[allow(clippy::cast_possible_truncation)]
        let change = (now - self.last) as f32 * self.opacity * 10.0;
        self.last = now;
        self.opacity = if over {
            (self.opacity - change).max(0.3)
        } else {
            (self.opacity + change).min(1.0)
        };
    }
}

/// What the distances of a body both mobile and dynamic are scaled by, with the adaptive degrade
/// on, at `fps` frames a second: 1 at 41 frames a second and up (at a `bias` of 0), falling to
/// 0.1 at none.
// D2-06658
#[must_use]
pub fn adaptive_scale(fps: f32, bias: f32) -> f32 {
    let f = (fps / (bias.clamp(-1.0, 1.0) * 40.0 + 41.0)) * 1.05;
    f.min(1.0) * 0.9 + 0.100_000_024
}

impl Default for BodyDrawer {
    /// Nothing decoded yet; the highest model and material detail.
    fn default() -> Self {
        Self {
            meshes: HashMap::new(),
            materials: HashMap::new(),
            kinds: HashMap::new(),
            present: HashMap::new(),
            posed: HashMap::new(),
            posed_before: HashMap::new(),
            model_detail: 2,
            material_detail: 2,
            eye: None,
            reflecting: false,
            moving_scale: 1.0,
            still_scale: 1.0,
            scale: 1.0,
        }
    }
}

impl std::fmt::Debug for BodyDrawer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BodyDrawer")
            .field("meshes", &self.meshes.len())
            .finish_non_exhaustive()
    }
}

fn v3(p: [f32; 3]) -> Vec3 {
    Vec3::new(p[0], p[1], p[2])
}

impl BodyDrawer {
    /// A frame starts: the lists of poses no body has held since the last one are let go.
    pub fn begin_frame(&mut self) {
        self.posed_before = std::mem::take(&mut self.posed);
    }

    /// The list of a mesh in a pose: the one it had, if it stood so last frame or a body stands so
    /// already.
    fn posed(
        &mut self,
        key: PosedKey,
        make: impl FnOnce() -> Vec<PosedVertex>,
    ) -> Arc<Vec<PosedVertex>> {
        if let Some(list) = self.posed.get(&key) {
            return list.clone();
        }
        let list = self
            .posed_before
            .remove(&key)
            .unwrap_or_else(|| Arc::new(make()));
        self.posed.insert(key, list.clone());
        list
    }

    /// The next body drawn is (or is not) both mobile and dynamic.
    pub fn next_body(&mut self, moving: bool) {
        self.scale = if moving {
            self.moving_scale
        } else {
            self.still_scale
        };
    }

    fn mesh(&mut self, records: &WorldRecords, id: u32) -> Option<Arc<Loaded>> {
        self.meshes
            .entry(id)
            .or_insert_with(|| {
                let mesh = records.get::<Mesh>(id)?;
                let vertices = vertices(&mesh.vertices)?;
                let influences = influences(&mesh.vertices);
                let rest = vertices
                    .iter()
                    .map(|v| (v3(v.position), v3(v.normal)))
                    .collect();
                Some(Arc::new(Loaded {
                    mesh: Arc::new(mesh),
                    vertices: Arc::new(vertices),
                    rest,
                    influences,
                }))
            })
            .clone()
    }

    fn material(&mut self, records: &WorldRecords, key: MaterialKey) -> Vec<DrawLayer> {
        self.materials
            .entry(key)
            .or_insert_with_key(|(id, template, extra, detail)| {
                instance_material_with(records, *id, *template, extra)
                    .map(|m| draw_layers(&m, device_flags(*detail)))
                    .unwrap_or_default()
            })
            .clone()
    }

    fn material_kind(&mut self, records: &WorldRecords, id: u32) -> u32 {
        *self
            .kinds
            .entry(id)
            .or_insert_with(|| material_kind(records, id))
    }

    /// Is a record an appearance names in the data?
    fn present(&mut self, records: &WorldRecords, id: u32) -> bool {
        *self
            .present
            .entry(id)
            .or_insert_with(|| records.bytes(id).is_some())
    }

    /// The material a fragment draws with, as the part's appearance leaves it.
    fn part_material(
        &mut self,
        records: &WorldRecords,
        look: &PartLook,
        mesh_kind: u32,
        id: u32,
        detail: u32,
    ) -> Vec<DrawLayer> {
        if look.materials.is_empty() {
            return self.material(records, (id, None, Vec::new(), detail));
        }
        let kind = self.material_kind(records, id);
        let (template, extra) = look.material(mesh_kind, kind, &mut |a| {
            (a.modifier.is_none() || self.present(records, a.modifier.0))
                && (a.template.is_none() || self.present(records, a.template.0))
        });
        let extra = extra.into_iter().map(|d| d.0).collect();
        self.material(records, (id, template.map(|t| t.0), extra, detail))
    }

    /// The batches of `body` in `poses`, placed by `place` (the body's frame in the drawn frame).
    pub fn batches(
        &mut self,
        records: &WorldRecords,
        body: &Body,
        poses: &[PartPose],
        place: &Mat4,
    ) -> Vec<ObjectBatch> {
        self.batches_without(records, body, poses, place, &[])
    }

    /// The batches of `body` in `poses`, placed by `place`, leaving out every mesh whose kind is
    /// in `hidden` (as the player's own head is left out with the camera at its eyes); each mesh
    /// at the level its distance gives for the model detail set, each material at the material
    /// detail set, lowered far off; the body fades out at the edge of its setup's visible distance.
    // D2-09879, D2-06604, D2-06577, D2-06589, D2-06658
    pub fn batches_without(
        &mut self,
        records: &WorldRecords,
        body: &Body,
        poses: &[PartPose],
        place: &Mat4,
        hidden: &[u32],
    ) -> Vec<ObjectBatch> {
        let mut out = Vec::new();
        let model = usize::try_from(self.model_detail).unwrap_or(2).min(2);
        let distance = self.eye.map(|eye| {
            auberean_world::math::length(auberean_world::math::sub(place.point(Vec3::ZERO), eye))
        });
        for (part, pose) in body.parts.iter().zip(poses) {
            let mut tint = match part.look.opacity {
                Some(o) => Color {
                    a: o,
                    ..Color::WHITE
                },
                None => Color::WHITE,
            };
            let degrade = match distance {
                Some(d) => {
                    let Some(g) = crate::mesh::degrade(
                        &part.setup.detail[model],
                        d,
                        self.scale,
                        self.material_detail,
                    ) else {
                        continue;
                    };
                    tint.a *= g.opacity;
                    Some(g)
                }
                None => None,
            };
            let (t, material_detail) = degrade.map_or((1.0, self.material_detail), |g| {
                (g.detail, g.material_detail)
            });
            // What the reflection sees of the part, when it is near enough to be seen there.
            let in_reflection = distance.filter(|_| self.reflecting).and_then(|d| {
                crate::mesh::degrade_in(
                    &part.setup.detail[model],
                    d,
                    self.scale,
                    crate::scene::REFLECTED_DISTANCE,
                    self.material_detail,
                )
            });
            // The hash of what moves the part's skinned meshes, the first time one asks.
            let mut skin_hash = None;
            for (i, setup_mesh) in part.setup.meshes.iter().enumerate() {
                let mesh_id = part.look.mesh(*setup_mesh, &mut |id| {
                    self.mesh(records, id.0).map(|m| m.mesh.kind)
                });
                let Some(m) = self.mesh(records, mesh_id.0) else {
                    continue;
                };
                if hidden.contains(&m.mesh.kind) {
                    continue;
                }
                let Some(level) = level_at(&m.mesh, t, self.model_detail) else {
                    continue;
                };
                let posed = match &m.influences {
                    Some(inf) => {
                        let pose_hash = *skin_hash.get_or_insert_with(|| {
                            hash_matrices(std::iter::once(&pose.world).chain(&pose.palette))
                        });
                        self.posed((mesh_id.0, pose_hash), || m.skinned(inf, pose))
                    }
                    None => {
                        let bone = pose
                            .bones
                            .get(i)
                            .copied()
                            .unwrap_or(Mat4::IDENTITY)
                            .then(&pose.world);
                        self.posed((mesh_id.0, hash_matrices([&bone])), || m.carried(&bone))
                    }
                };
                // The reflection takes the same level when its choice falls on it, else one of
                // its own beside it.
                let mirrored = in_reflection.and_then(|g| {
                    level_at(&m.mesh, g.detail, self.model_detail).map(|l| (l, g.material_detail))
                });
                let lod = self
                    .reflecting
                    .then(|| Lod::of_body(true, mirrored == Some((level, material_detail))));
                let mut choices = vec![(level, material_detail, lod)];
                if let Some(c) = mirrored.filter(|c| *c != (level, material_detail)) {
                    choices.push((c.0, c.1, Some(Lod::of_body(false, true))));
                }
                for (level, material_detail, lod) in choices {
                    for frag in &m.mesh.levels[level].fragments {
                        let Some(mat) = usize::try_from(frag.material)
                            .ok()
                            .and_then(|k| m.mesh.materials.get(k))
                        else {
                            continue;
                        };
                        let indices = frag.indices.clone();
                        for layer in self.part_material(
                            records,
                            &part.look,
                            m.mesh.kind,
                            mat.0,
                            material_detail,
                        ) {
                            out.push(ObjectBatch {
                                vertices: m.vertices.clone(),
                                posed: Some(posed.clone()),
                                indices: indices.clone(),
                                transform: *place,
                                layer,
                                tint,
                                points: crate::scene::PointLights::NONE,
                                mesh_type: m.mesh.kind,
                                shade: None,
                                outside: false,
                                cull: crate::scene::Cull::Never,
                                mesh: mesh_id.0,
                                casts_volume: false,
                                caster: None,
                                seen_through: false,
                                kept_by: None,
                                anchor: None,
                                step: None,
                                visible: degrade.map(|g| g.visible),
                                block: None,
                                far: distance.is_some_and(|d| d > 40.0),
                                ground: None,
                                lod,
                            });
                        }
                    }
                }
            }
        }
        out
    }
}

fn normalize(v: Vec3) -> Vec3 {
    let n = (v.x * v.x + v.y * v.y + v.z * v.z).sqrt();
    if n > 1e-12 {
        let (at, mut out) = ([v.x, v.y, v.z, 0.0], [0.0f32; 4]);
        for i in 0..4 {
            out[i] = at[i] / n;
        }
        Vec3::new(out[0], out[1], out[2])
    } else {
        v
    }
}
