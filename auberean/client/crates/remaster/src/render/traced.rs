//! The sun's shadows and the ambient occlusion by rays traced through the frame's acceleration
//! structures, where the device traces rays: the shared passes (`dereth_gfx_passes::traced`) over
//! structures made here from the frame's meshes. They stand in for the cascades' look-up and the
//! screen-space occlusion and write the same pictures; where the device cannot trace rays, or the
//! frame has nothing to trace, the cascades and the screen-space occlusion draw them as ever.
//!
//! **What the structures hold.** The sun's casters (`Frame::shadow`: every object of the detailed
//! blocks at its full detail, as the cascades take them) and the cells the frame draws, each draw
//! an instance of a bottom-level structure of its mesh's triangles. The land is in none of them: as
//! with the cascades, its own vertices already carry where its hills keep the sun off it.
//!
//! - A mesh's structure is made once and kept while its lists are kept and frames use it.
//! - A moved body's is made again each frame from its pose, and let go of the next.
//! - The top-level structure is built every frame from the instances, at the head of the passes
//!   that trace it.
//!
//! What the shaders read of each instance is laid out in `shaders/rt.slang`: its two rows in the
//! instances' table, which find its vertices, its indices, its constants among the frame's draws,
//! its first stage's picture and how far the wind bends it. A draw tested through its alpha (or
//! swaying in the wind) is handed to the queries to test; every other is taken as it is.

use std::collections::HashMap;
use std::sync::Arc;

use auberean_draw::constants::{bound_stages, DrawUniforms};
use auberean_draw::mesh::PosedVertex;
use auberean_draw::picture::Image;
use auberean_draw::scene::ObjectBatch;
use dereth_gfx::{
    Blas, BlasGeometry, Buffer, BufferUsage, IndexType, Recorder, Tlas, TlasInstance,
};
use dereth_gfx_passes::frame::PerFrame;
use dereth_gfx_passes::traced::{RtAoPass, RtShadowPass, TracedScene};

use super::{Remaster, RemasterError, KEEP_FRAMES, SWAY_LEAVES};
use crate::shaders::{shader, Entry};

/// The one mask every instance takes, as `rt.slang` names it.
const MASK_OBJECT: u8 = 8;
/// An instance's flags, as `rt.slang` names them.
const ALPHA_TESTED: u32 = 1;
const SWAYS: u32 = 8;
/// The bytes of a mesh vertex and of a posed vertex on the device.
const VERTEX_BYTES: u64 = std::mem::size_of::<auberean_draw::constants::MeshGpuVertex>() as u64;
const POSED_BYTES: u64 = std::mem::size_of::<PosedVertex>() as u64;

/// A mesh's structure: its vertex list's key and its index list's (each its place in memory and
/// length, as the kept buffers are keyed), and whether every hit on it is taken.
type BlasKey = ((usize, usize), (usize, usize), bool);

/// The traced passes, and the structures they trace.
#[derive(Debug)]
pub(super) struct Traced {
    pub(super) shadow: RtShadowPass,
    pub(super) ao: RtAoPass,
    /// Each mesh's structure, with the last frame that used it.
    blas: HashMap<BlasKey, (Blas, u64)>,
    /// The moved bodies' structures of the last frame, let go of at the next.
    posed: Vec<Blas>,
    tlas: Option<Tlas>,
    /// The instances' table.
    rows: PerFrame,
}

impl Traced {
    /// The passes on `gfx`, off: their pipelines made where the device traces rays.
    ///
    /// # Errors
    /// What the device refuses.
    pub(super) fn new(gfx: &mut dereth_gfx::Device) -> Result<Self, dereth_gfx::Error> {
        Ok(Self {
            shadow: RtShadowPass::new(gfx, shader(Entry::RtShadowFsMain))?,
            ao: RtAoPass::new(gfx, shader(Entry::RtAoFsRays), shader(Entry::RtAoFsFilter))?,
            blas: HashMap::new(),
            posed: Vec::new(),
            tlas: None,
            rows: PerFrame::new(
                gfx,
                "remaster traced instances",
                1 << 16,
                BufferUsage::default(),
            )?,
        })
    }
}

/// What an instance places.
#[derive(Debug, Clone, Copy)]
enum Shape {
    /// A mesh's kept structure.
    Kept(Blas),
    /// A moved body's: its pose's first vertex among the frame's, its vertex count, its indices
    /// and their count, and whether every hit on it is taken.
    Posed {
        base: u32,
        vertices: u32,
        indices: Buffer,
        index_count: u32,
        opaque: bool,
    },
}

/// One instance of the frame's structure, as gathered before the frame's poses are written.
#[derive(Debug, Clone, Copy)]
struct Pending {
    transform: [f32; 12],
    shape: Shape,
    rows: [u32; 8],
}

/// The frame's instances, gathered with its draws.
#[derive(Debug)]
pub(super) struct TracedPlan {
    pending: Vec<Pending>,
}

/// The frame's structure, ready to build.
#[derive(Debug)]
pub(super) struct TracedFrame {
    tlas: Tlas,
    instances: Vec<TlasInstance>,
    /// The instances' table and the frame's draws, by their index in the table.
    tables: [u32; 2],
}

impl TracedFrame {
    /// Build the frame's top-level structure, now: the traced passes read it after. Returns the
    /// scene as they take it.
    ///
    /// # Errors
    /// What the layer refuses.
    pub(super) fn record(&self, r: &mut Recorder<'_>) -> Result<TracedScene, dereth_gfx::Error> {
        r.timestamp("acceleration");
        r.build_tlas(self.tlas, &self.instances)?;
        Ok(TracedScene {
            accel: r.bind_tlas(self.tlas)?,
            tables: self.tables,
            pools: [0; 4],
        })
    }

    /// How many instances the structure holds.
    pub(super) fn instances(&self) -> usize {
        self.instances.len()
    }
}

/// `m`, a draw's transform, as a top-level structure's instance takes it: three rows of four.
fn rows_of(m: &auberean_world::math::Mat4) -> [f32; 12] {
    // The stored matrix is the transposed column-vector one: its rows are the columns.
    std::array::from_fn(|i| m.0[i % 4][i / 4])
}

impl Remaster {
    /// Whether the traced shadows trace rays from the next frame on.
    pub fn set_rt_shadows(&mut self, settings: dereth_gfx_passes::traced::RtShadowSettings) {
        self.traced.shadow.set_settings(settings);
    }

    /// How the traced shadows are drawn.
    #[must_use]
    pub fn rt_shadows(&self) -> dereth_gfx_passes::traced::RtShadowSettings {
        self.traced.shadow.settings()
    }

    /// Whether the traced occlusion traces rays from the next frame on.
    pub fn set_rt_ao(&mut self, settings: dereth_gfx_passes::traced::RtAoSettings) {
        self.traced.ao.set_settings(settings);
    }

    /// How the traced occlusion is drawn.
    #[must_use]
    pub fn rt_ao(&self) -> dereth_gfx_passes::traced::RtAoSettings {
        self.traced.ao.settings()
    }

    /// Frames the shadows and the occlusion were traced in, and how many instances the last
    /// frame's structure held.
    #[must_use]
    pub fn traced_frames(&self) -> (u64, u64, usize) {
        (
            self.traced.shadow.frames(),
            self.traced.ao.frames(),
            self.last_traced,
        )
    }

    /// Whether the device traces rays.
    #[must_use]
    pub fn traces_rays(&self) -> bool {
        self.gfx.caps().ray_queries
    }

    /// The frame's instances, when a traced pass wants them (`shadows` and `ao` say whether the
    /// shadows and the occlusion are drawn this frame): every caster of `casters` and every cell of
    /// `cells`, each with its constants (into `records`, lit by `sun_color`) and a moved body's
    /// pose (into `posed`). `None` when no traced pass runs or there is nothing to trace.
    ///
    /// # Errors
    /// What the device refuses.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn plan_traced(
        &mut self,
        casters: &[ObjectBatch],
        cells: &[&ObjectBatch],
        (shadows, ao): (bool, bool),
        sun_color: [f32; 4],
        records: &mut Vec<DrawUniforms>,
        posed: &mut Vec<PosedVertex>,
        load: &mut dyn FnMut(u32) -> Option<Image>,
    ) -> Result<Option<TracedPlan>, RemasterError> {
        let wanted =
            (self.traced.shadow.wants_rays() && shadows) || (self.traced.ao.wants_rays() && ao);
        if !wanted {
            return Ok(None);
        }
        let mut pending = Vec::new();
        for b in casters.iter().chain(cells.iter().copied()) {
            if b.indices.len() < 3 {
                continue;
            }
            let c = self.caster(b, sun_color, records, posed, load)?;
            let textured = bound_stages(&b.layer)[0].is_some();
            let alpha = b.layer.alpha_test && textured;
            // The cut-out leaves sway as the frame draws them.
            let sway = (self.sway && b.posed.is_none() && b.layer.alpha_test && !b.points.inside)
                .then_some(SWAY_LEAVES);
            let opaque = !alpha && sway.is_none();
            let indices = if opaque {
                u32::MAX
            } else {
                self.index_binding(&b.indices)?
            };
            let shape = match c.posed {
                Some(base) => Shape::Posed {
                    base,
                    vertices: u32::try_from(b.vertices.len()).unwrap_or(0),
                    indices: c.indices,
                    index_count: c.index_count,
                    opaque,
                },
                None => Shape::Kept(self.blas_for(b, c.indices, opaque)?),
            };
            let flags =
                if alpha { ALPHA_TESTED } else { 0 } | if sway.is_some() { SWAYS } else { 0 };
            pending.push(Pending {
                transform: rows_of(&b.transform),
                shape,
                rows: [
                    c.vertices,
                    indices,
                    c.record,
                    flags,
                    c.texture,
                    c.sampler,
                    sway.map_or(0, f32::to_bits),
                    0,
                ],
            });
        }
        Ok((!pending.is_empty()).then_some(TracedPlan { pending }))
    }

    /// The structure of `b`'s mesh (its vertices and `indices`, the buffer of its index list),
    /// made the first time it is asked for.
    ///
    /// # Errors
    /// What the device refuses.
    fn blas_for(
        &mut self,
        b: &ObjectBatch,
        indices: Buffer,
        opaque: bool,
    ) -> Result<Blas, RemasterError> {
        let vkey = (Arc::as_ptr(&b.vertices) as usize, b.vertices.len());
        let key = (
            vkey,
            (Arc::as_ptr(&b.indices) as usize, b.indices.len()),
            opaque,
        );
        if let Some((blas, used)) = self.traced.blas.get_mut(&key) {
            *used = self.frame;
            return Ok(*blas);
        }
        let vertices = self.vertices.get(&vkey).map(|k| k.buffer).ok_or_else(|| {
            RemasterError::Device(dereth_gfx::Error::Invalid(
                "a mesh's vertices are not on the device".into(),
            ))
        })?;
        let blas = self.gfx.create_blas(
            "remaster mesh",
            &[BlasGeometry {
                vertices,
                vertex_offset: 0,
                vertex_stride: VERTEX_BYTES,
                vertex_count: u32::try_from(b.vertices.len()).unwrap_or(0),
                indices,
                index_offset: 0,
                index_type: IndexType::U16,
                triangles: u32::try_from(b.indices.len() / 3).unwrap_or(0),
                opaque,
            }],
        )?;
        self.traced.blas.insert(key, (blas, self.frame));
        Ok(blas)
    }

    /// The frame's structure from its instances, once its poses are on the device (`self.posed`
    /// written): the moved bodies' structures made over them, the instances' table written, and
    /// the structures no frame has used for a while, or whose lists are gone, let go of.
    ///
    /// # Errors
    /// What the device refuses.
    pub(super) fn finish_traced(
        &mut self,
        plan: &TracedPlan,
    ) -> Result<TracedFrame, RemasterError> {
        for b in std::mem::take(&mut self.traced.posed) {
            self.gfx.destroy_blas(b);
        }
        let now = self.frame;
        let (vertices, indices) = (&self.vertices, &self.indices);
        let gone: Vec<BlasKey> = self
            .traced
            .blas
            .iter()
            .filter(|((v, i, _), (_, used))| {
                now.saturating_sub(*used) > KEEP_FRAMES
                    || !vertices.contains_key(v)
                    || !indices.contains_key(i)
            })
            .map(|(k, _)| *k)
            .collect();
        for k in gone {
            if let Some((b, _)) = self.traced.blas.remove(&k) {
                self.gfx.destroy_blas(b);
            }
        }
        let mut instances = Vec::with_capacity(plan.pending.len());
        let mut rows = Vec::with_capacity(plan.pending.len() * 8);
        for (n, p) in plan.pending.iter().enumerate() {
            let blas = match p.shape {
                Shape::Kept(b) => b,
                Shape::Posed {
                    base,
                    vertices,
                    indices,
                    index_count,
                    opaque,
                } => {
                    let b = self.gfx.create_blas(
                        "remaster posed body",
                        &[BlasGeometry {
                            vertices: self.posed.buffer,
                            vertex_offset: u64::from(base) * POSED_BYTES,
                            vertex_stride: POSED_BYTES,
                            vertex_count: vertices,
                            indices,
                            index_offset: 0,
                            index_type: IndexType::U16,
                            triangles: index_count / 3,
                            opaque,
                        }],
                    )?;
                    self.traced.posed.push(b);
                    b
                }
            };
            instances.push(TlasInstance {
                blas,
                transform: p.transform,
                custom: u32::try_from(n).unwrap_or(0),
                mask: MASK_OBJECT,
            });
            rows.extend_from_slice(&p.rows);
        }
        self.traced
            .rows
            .write(&mut self.gfx, bytemuck::cast_slice(&rows))?;
        let tlas = match self.traced.tlas {
            Some(t) => t,
            None => {
                let t = self.gfx.create_tlas(
                    "remaster frame",
                    u32::try_from(instances.len()).unwrap_or(1),
                )?;
                self.traced.tlas = Some(t);
                t
            }
        };
        Ok(TracedFrame {
            tlas,
            instances,
            tables: [self.traced.rows.index, self.draws.index],
        })
    }
}
