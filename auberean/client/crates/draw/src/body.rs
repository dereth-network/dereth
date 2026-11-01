//! A posed body as mesh batches: each part group's meshes skinned to its posed skeleton on the CPU,
//! as the client blends them, or (a mesh with no bone weights) carried whole by its bone. Each part
//! is drawn as its appearance changes it: the meshes swapped in, the materials replaced and
//! modified, the opacity.

use std::collections::HashMap;
use std::sync::Arc;

use auberean_animation::body::{Body, PartPose};
use auberean_animation::look::PartLook;
use auberean_animation::skin::{influences, Influences};
use auberean_assets::mesh::Mesh;
use auberean_primitives::Vec3;
use auberean_world::color::Color;
use auberean_world::math::Mat4;
use auberean_world::records::WorldRecords;

use crate::material::{
    draw_layers, instance_material_with, material_kind, DrawLayer, DEVICE_FLAGS,
};
use crate::mesh::{level_at, vertices, MeshVertex};
use crate::scene::ObjectBatch;

struct Loaded {
    mesh: Arc<Mesh>,
    vertices: Vec<MeshVertex>,
    influences: Option<Vec<Influences>>,
}

/// A material as an appearance leaves it: the mesh's material, the template that replaces its
/// own, the modifiers added after its own.
type MaterialKey = (u32, Option<u32>, Vec<u32>);

/// Turns posed bodies into batches, keeping what it decoded.
#[derive(Default)]
pub struct BodyDrawer {
    meshes: HashMap<u32, Option<Arc<Loaded>>>,
    materials: HashMap<MaterialKey, Vec<DrawLayer>>,
    kinds: HashMap<u32, u32>,
    present: HashMap<u32, bool>,
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
    fn mesh(&mut self, records: &WorldRecords, id: u32) -> Option<Arc<Loaded>> {
        self.meshes
            .entry(id)
            .or_insert_with(|| {
                let mesh = records.get::<Mesh>(id)?;
                let vertices = vertices(&mesh.vertices)?;
                let influences = influences(&mesh.vertices);
                Some(Arc::new(Loaded {
                    mesh: Arc::new(mesh),
                    vertices,
                    influences,
                }))
            })
            .clone()
    }

    fn material(&mut self, records: &WorldRecords, key: MaterialKey) -> Vec<DrawLayer> {
        self.materials
            .entry(key)
            .or_insert_with_key(|(id, template, extra)| {
                instance_material_with(records, *id, *template, extra)
                    .map(|m| draw_layers(&m, DEVICE_FLAGS))
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
    ) -> Vec<DrawLayer> {
        if look.materials.is_empty() {
            return self.material(records, (id, None, Vec::new()));
        }
        let kind = self.material_kind(records, id);
        let (template, extra) = look.material(mesh_kind, kind, &mut |a| {
            (a.modifier.is_none() || self.present(records, a.modifier.0))
                && (a.template.is_none() || self.present(records, a.template.0))
        });
        let extra = extra.into_iter().map(|d| d.0).collect();
        self.material(records, (id, template.map(|t| t.0), extra))
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
    /// in `hidden` (as the player's own head is left out with the camera at its eyes).
    // D2-09879, D2-06604, D2-06577, D2-06589
    pub fn batches_without(
        &mut self,
        records: &WorldRecords,
        body: &Body,
        poses: &[PartPose],
        place: &Mat4,
        hidden: &[u32],
    ) -> Vec<ObjectBatch> {
        let mut out = Vec::new();
        for (part, pose) in body.parts.iter().zip(poses) {
            let tint = match part.look.opacity {
                Some(o) => Color {
                    a: o,
                    ..Color::WHITE
                },
                None => Color::WHITE,
            };
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
                let Some(level) = level_at(&m.mesh, 1.0, 2) else {
                    continue;
                };
                let verts: Vec<MeshVertex> = match &m.influences {
                    Some(inf) => {
                        let pos: Vec<Vec3> = m.vertices.iter().map(|v| v3(v.position)).collect();
                        let nrm: Vec<Vec3> = m.vertices.iter().map(|v| v3(v.normal)).collect();
                        let (p, n) = auberean_animation::skin::skin(&pos, &nrm, inf, &pose.palette);
                        m.vertices
                            .iter()
                            .zip(p.iter().zip(&n))
                            .map(|(v, (p, n))| {
                                let p = pose.world.point(*p);
                                let n = normalize(pose.world.direction(*n));
                                MeshVertex {
                                    position: [p.x, p.y, p.z],
                                    normal: [n.x, n.y, n.z],
                                    ..*v
                                }
                            })
                            .collect()
                    }
                    None => {
                        let bone = pose
                            .bones
                            .get(i)
                            .copied()
                            .unwrap_or(Mat4::IDENTITY)
                            .then(&pose.world);
                        m.vertices
                            .iter()
                            .map(|v| {
                                let p = bone.point(v3(v.position));
                                let n = normalize(bone.direction(v3(v.normal)));
                                MeshVertex {
                                    position: [p.x, p.y, p.z],
                                    normal: [n.x, n.y, n.z],
                                    ..*v
                                }
                            })
                            .collect()
                    }
                };
                let verts = Arc::new(verts);
                for frag in &m.mesh.levels[level].fragments {
                    let Some(mat) = usize::try_from(frag.material)
                        .ok()
                        .and_then(|k| m.mesh.materials.get(k))
                    else {
                        continue;
                    };
                    let indices = Arc::new(frag.indices.clone());
                    for layer in self.part_material(records, &part.look, m.mesh.kind, mat.0) {
                        out.push(ObjectBatch {
                            vertices: verts.clone(),
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
                        });
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
        Vec3::new(v.x / n, v.y / n, v.z / n)
    } else {
        v
    }
}
