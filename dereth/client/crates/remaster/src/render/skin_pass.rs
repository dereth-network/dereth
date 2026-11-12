//! The skinned bodies posed on the device ([`crate::assets::skin`]): a compute pass that moves
//! each body's vertices by its joints and writes them over its mesh's vertices in the vertex pool,
//! before anything that draws the world reads them.
//!
//! **Once a body:** its vertices in their bind pose and its influences (four joints and weights,
//! and its share of dual-quaternion blending) go to the device the first time an object wears it.
//!
//! **Each frame:** only each object's joints (its matrices, and the same as dual quaternions), a
//! few hundred bytes, worked out on the processor as the posing on the processor works them out
//! ([`SkinModel::joints`]).
//!
//! **The motion pass** finds each vertex's last place where the shader kept it, beside the mesh,
//! before writing over it ([`super::scene::GpuScene::previous`]).

use std::collections::BTreeMap;
use std::sync::Arc;

use dereth_gfx::{Buffer, BufferDesc, BufferUsage, ComputeDesc, Device, Location, Pipeline};
use dereth_gfx::{Recorder, Touch, Use};
use glam::{Mat3, Mat4, Quat};

use super::scene::GpuScene;
use super::{push_words, shader};
use crate::assets::skin::SkinModel;
use crate::assets::MeshKey;
use crate::shaders::Entry;

/// One body's data on the device, made once.
#[derive(Debug)]
struct Statics {
    bind: (Buffer, u32),
    influences: (Buffer, u32),
    count: u32,
}

/// One object's body posed this frame.
#[derive(Debug, Clone, Copy)]
struct Job {
    count: u32,
    bind: u32,
    influences: u32,
    joints: u32,
    first: u32,
    last: u32,
}

/// The pass: its pipeline, each body's data, this frame's joints and the frame's work.
#[derive(Debug)]
pub struct SkinPass {
    pipeline: Pipeline,
    bodies: BTreeMap<u32, Statics>,
    frame: Option<(Buffer, u32, u64)>,
    jobs: Vec<Job>,
    /// Buffers outgrown, destroyed once the next recording is begun.
    pub retired: Vec<Buffer>,
}

/// What a frame records of the pass.
#[derive(Debug, Clone)]
pub struct SkinPlan {
    pipeline: Pipeline,
    frame: (Buffer, u32),
    pool: (Buffer, u32),
    reads: Vec<Buffer>,
    jobs: Vec<Job>,
}

fn floats(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|f| f.to_le_bytes()).collect()
}

fn made(
    d: &mut Device,
    label: &'static str,
    bytes: &[u8],
) -> Result<(Buffer, u32), dereth_gfx::Error> {
    let b = d.create_buffer(&BufferDesc {
        label,
        size: (bytes.len() as u64).max(16),
        usage: BufferUsage::STORAGE,
        location: Location::Gpu,
    })?;
    if !bytes.is_empty() {
        d.write_buffer(b, 0, bytes)?;
    }
    Ok((b, d.bind_buffer(b)?))
}

/// A body's vertices in their bind pose, as the pool holds vertices, and its influences, two rows
/// a vertex.
fn statics_of(model: &SkinModel) -> (Vec<u8>, Vec<u8>) {
    let n = model.mesh.vertices.len();
    let mut bind = Vec::with_capacity(n * 64);
    for (k, v) in model.mesh.vertices.iter().enumerate() {
        let t = model
            .mesh
            .tangents
            .get(k)
            .copied()
            .unwrap_or([1.0, 0.0, 0.0, 1.0]);
        let c = model.mesh.colors.get(k).copied().unwrap_or([1.0; 4]);
        bind.extend(floats(&[
            v.position[0],
            v.position[1],
            v.position[2],
            v.normal[0],
            v.normal[1],
            v.normal[2],
            v.uv[0],
            v.uv[1],
        ]));
        bind.extend(floats(&t));
        bind.extend(floats(&c));
    }
    let slot = |j: u16| -> u32 { u32::try_from(model.slot(j)).unwrap_or(0) };
    let mut influences = Vec::with_capacity(n * 32);
    for (k, (ps, ws)) in model.parts.iter().zip(&model.weights).enumerate() {
        let share = model.rig.dqs.get(k).copied().unwrap_or(0.0);
        influences.extend(push_words(&[
            slot(ps[0]) | slot(ps[1]) << 16,
            slot(ps[2]) | slot(ps[3]) << 16,
            share.to_bits(),
            0,
        ]));
        influences.extend(floats(ws));
    }
    (bind, influences)
}

/// A skinning matrix's turn as a unit quaternion, its scale taken out.
fn turn_of(m: Mat4) -> Quat {
    let r = Mat3::from_mat4(m);
    let s = dereth_primitives::num::math::powf(r.determinant().abs(), 1.0 / 3.0).max(1e-12);
    Quat::from_mat3(&(r * (1.0 / s))).normalize()
}

impl SkinPass {
    /// The pass, its pipeline made now.
    ///
    /// # Errors
    /// What the device refuses.
    pub fn new(gfx: &mut Device) -> Result<Self, dereth_gfx::Error> {
        let pipeline = gfx.create_compute_pipeline(&ComputeDesc {
            label: "remaster skinning",
            shader: shader(Entry::SkinCsMain),
        })?;
        Ok(Self {
            pipeline,
            bodies: BTreeMap::new(),
            frame: None,
            jobs: Vec::new(),
            retired: Vec::new(),
        })
    }

    /// This frame's work: each `(model, mesh id, joints)` posed, its body's data made on the
    /// device the first time, its joints written, its last places kept in `scene` when `motion`
    /// asks.
    ///
    /// # Errors
    /// What the device refuses.
    pub fn receive(
        &mut self,
        d: &mut Device,
        scene: &mut GpuScene,
        posed: &[(Arc<SkinModel>, u32, Vec<Mat4>)],
        motion: bool,
    ) -> Result<(), dereth_gfx::Error> {
        self.jobs.clear();
        let mut frame: Vec<u8> = Vec::new();
        for (model, id, joints) in posed {
            let key = MeshKey::GfxObj(*id);
            let Some(first) = scene.meshes.get(&key).map(|m| m.vertices.start) else {
                continue;
            };
            let s = match self.bodies.entry(model.skin) {
                std::collections::btree_map::Entry::Occupied(e) => e.into_mut(),
                std::collections::btree_map::Entry::Vacant(e) => {
                    let (bind, influences) = statics_of(model);
                    e.insert(Statics {
                        bind: made(d, "remaster skin bind", &bind)?,
                        influences: made(d, "remaster skin influences", &influences)?,
                        count: u32::try_from(model.mesh.vertices.len()).unwrap_or(0),
                    })
                }
            };
            let last = if motion {
                scene.previous_region(d, key)?.map_or(0, |r| r + 1)
            } else {
                0
            };
            let joints_row = u32::try_from(frame.len() / 16).unwrap_or(0);
            for m in joints {
                frame.extend(floats(&m.to_cols_array()));
                let q = turn_of(*m);
                let t = m.w_axis.truncate();
                let dq = Quat::from_xyzw(t.x, t.y, t.z, 0.0) * q;
                frame.extend(floats(&[q.x, q.y, q.z, q.w]));
                frame.extend(floats(&[dq.x * 0.5, dq.y * 0.5, dq.z * 0.5, dq.w * 0.5]));
            }
            self.jobs.push(Job {
                count: s.count,
                bind: s.bind.1,
                influences: s.influences.1,
                joints: joints_row,
                first,
                last,
            });
        }
        if self.jobs.is_empty() {
            return Ok(());
        }
        let need = (frame.len() as u64).max(16);
        if self.frame.is_none_or(|f| f.2 < need) {
            let size = need.next_power_of_two();
            let b = d.create_buffer(&BufferDesc {
                label: "remaster skin joints",
                size,
                usage: BufferUsage::STORAGE,
                location: Location::Gpu,
            })?;
            let index = d.bind_buffer(b)?;
            if let Some(old) = self.frame.replace((b, index, size)) {
                self.retired.push(old.0);
            }
        }
        if let Some(f) = self.frame {
            d.write_buffer(f.0, 0, &frame)?;
        }
        Ok(())
    }

    /// What the frame records, over the pool `pool` (its buffer and index); `None` with nothing
    /// to pose.
    #[must_use]
    pub fn plan(&self, pool: (Buffer, u32)) -> Option<SkinPlan> {
        let frame = self.frame?;
        if self.jobs.is_empty() {
            return None;
        }
        let mut reads = vec![frame.0];
        for s in self.bodies.values() {
            reads.extend([s.bind.0, s.influences.0]);
        }
        Some(SkinPlan {
            pipeline: self.pipeline,
            frame: (frame.0, frame.1),
            pool,
            reads,
            jobs: self.jobs.clone(),
        })
    }
}

impl SkinPlan {
    /// The bodies posed into the pool, then the pool made ready for every pass that reads it.
    ///
    /// # Errors
    /// What the layer refuses.
    pub fn record(&self, r: &mut Recorder<'_>) -> Result<(), dereth_gfx::Error> {
        r.timestamp("skinning");
        let mut touches = vec![Touch::Buffer(self.pool.0, Use::StorageWrite)];
        touches.extend(
            self.reads
                .iter()
                .map(|b| Touch::Buffer(*b, Use::StorageRead)),
        );
        r.compute(&touches, |c| {
            c.set_pipeline(self.pipeline)?;
            for j in &self.jobs {
                c.push(
                    0,
                    &push_words(&[
                        j.count,
                        j.bind,
                        j.influences,
                        self.frame.1,
                        j.joints,
                        self.pool.1,
                        j.first,
                        j.last,
                    ]),
                )?;
                c.dispatch(j.count.div_ceil(64).max(1), 1, 1);
            }
            Ok(())
        })?;
        // Every pass after reads the posed vertices: their writes are waited for.
        r.compute(&[Touch::Buffer(self.pool.0, Use::StorageRead)], |_| Ok(()))
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (a layout check of the data the skinning shader reads)
    use super::*;
    use crate::assets::skin::{Bone, Rig, BONE};
    use crate::assets::{Mesh, Subset, SurfaceRef, Vertex};

    fn word(b: &[u8], i: usize) -> u32 {
        u32::from_le_bytes(b[i * 4..i * 4 + 4].try_into().expect("4"))
    }

    fn float(b: &[u8], i: usize) -> f32 {
        f32::from_bits(word(b, i))
    }

    #[test]
    fn a_bodys_data_names_its_bones_after_its_parts_with_each_vertexs_blending_share() {
        let v = |z: f32| Vertex {
            position: [0.0, 0.0, z],
            normal: [0.0, 1.0, 0.0],
            uv: [0.5, 0.25],
        };
        let mesh = Mesh {
            vertices: vec![v(0.0), v(1.0), v(0.5)],
            indices: vec![0, 1, 2],
            subsets: vec![Subset {
                first_index: 0,
                index_count: 3,
                surface: SurfaceRef::Surface(0x0800_0001),
                two_sided: false,
                tiled: true,
            }],
            ..Mesh::default()
        };
        let model = SkinModel::new(
            0,
            mesh,
            vec![[1, 0, 0, 0], [2, 0, 0, 0], [1, BONE, 2, 0]],
            vec![
                [1.0, 0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0, 0.0],
                [0.25, 0.5, 0.25, 0.0],
            ],
            &[(1, Mat4::IDENTITY), (2, Mat4::IDENTITY)],
        )
        .expect("a body")
        .with_rig(Rig {
            bones: vec![Bone::new(Mat4::IDENTITY)],
            dqs: vec![0.0, 0.0, 1.0],
            ..Rig::default()
        });
        let (bind, influences) = statics_of(&model);
        assert_eq!(bind.len(), 3 * 64);
        assert_eq!([float(&bind, 16 + 2), float(&bind, 16 + 6)], [1.0, 0.5]);
        assert_eq!(influences.len(), 3 * 32);
        // Three parts' places (0..2), so the body's own bone is the fourth joint.
        assert_eq!(word(&influences, 16), 1 | 3 << 16);
        assert_eq!(word(&influences, 17), 2);
        assert_eq!(float(&influences, 16 + 4 + 1), 0.5);
        assert_eq!(float(&influences, 16 + 2), 1.0, "its dual-quaternion share");
        assert_eq!(float(&influences, 2), 0.0);
    }
}
