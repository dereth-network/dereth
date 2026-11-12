//! The scene's acceleration structures, where the device traces rays (pass 1b): a bottom-level
//! structure per mesh as the scene holds it and per block's ground, and each frame a top-level
//! structure of every resident static at its finest level, every room's walls, every block's
//! ground and every object, in the G-buffer's own space.
//!
//! What shaders read is laid out in `shaders/rt.slang`: the masks, the geometry tables and their
//! rows. The instances' records are written on the device from the instances' own rows
//! (`rt.slang`'s `cs_instances`), so a structure stands exactly where the G-buffer draws it, block
//! offsets and the drawn ground's lifts applied.
//!
//! **When it runs.** Only while something traces rays and the device can
//! ([`dereth_gfx::Caps::ray_queries`]); otherwise it holds nothing and records nothing.
//!
//! **What it holds.**
//! - A bottom-level structure per distinct mesh run: a mesh's first vertex and the subsets the
//!   G-buffer draws opaque or alpha-tested (blended and additive ones left out), one geometry
//!   each, opaque unless alpha-tested. A skinned body's mesh is posed every frame, so its
//!   structure is refitted to it every frame, in place ([`RtPass::set_posed`]): made once, as
//!   the body comes, and fitted again at the head of each frame's recording, before that frame's
//!   posing, so it holds the body as the last frame posed it: a frame behind the G-buffer.
//! - The statics' entries and rows, made again only when what is resident changes.
//! - The objects' entries and rows, every frame.

use std::collections::{BTreeMap, BTreeSet};

use dereth_gfx::{
    Blas, BlasGeometry, Buffer, BufferDesc, BufferUsage, ComputeDesc, Device, IndexType, Location,
    Pipeline, Recorder, Tlas, Touch, Use,
};

use super::scene::{Bucket, GpuScene, ItemDraw, PartItems};
use super::{push_words, shader, PerFrame};
use crate::scene::{BlockId, ObjectKey};
use crate::shaders::Entry;

/// The masks, as `rt.slang` names them.
pub mod mask {
    pub const STATIC: u8 = 1;
    pub const STATIC_ALPHA: u8 = 2;
    pub const GROUND: u8 = 4;
    pub const OBJECT: u8 = 8;
    pub const BODY: u8 = 16;
    /// An art pack's ground cover: the sun's traced shadows take it near the eye alone.
    pub const COVER: u8 = 32;
}

/// A geometry row's flags, as `rt.slang` names them.
pub mod row_flag {
    pub const ALPHA_TESTED: u32 = 1;
    pub const TWO_SIDED: u32 = 2;
    pub const GROUND: u32 = 4;
}

/// The bit of an instance's index of the user's that names the objects' table.
pub const OBJECT_ROWS: u32 = 0x80_0000;

/// The bytes of a mesh vertex and of a ground vertex, as the scene's pools hold them.
const MESH_VERTEX: u64 = super::scene::VERTEX_BYTES as u64;
const GROUND_VERTEX: u64 = 32;

/// How many frames a structure no static uses is kept after an object last did.
const KEEP_FRAMES: u64 = 300;

/// What a bottom-level structure holds: its mesh's first vertex, and each geometry's first index,
/// index count and whether it is opaque.
type BlasKey = (u32, Vec<(u32, u32, bool)>);

#[derive(Debug)]
struct Held {
    blas: Blas,
    address: u64,
    /// The last frame an object placed it.
    seen: u64,
    /// The frame it was last made or refitted.
    made_at: u64,
    /// Whether it is refitted as its mesh is posed, and the vertices' and indices' buffers it
    /// reads: a pool that grew has moved to a new buffer, and what was made over the old one is
    /// made again.
    refitted: bool,
    buffers: (Buffer, Buffer),
}

#[derive(Debug)]
struct GroundHeld {
    blas: Blas,
    address: u64,
    /// The block's first ground vertex and its cuts, which it was made for.
    vertices: u32,
    cuts: Vec<bool>,
    /// Its own indices, two triangles a cell in the cells' order.
    indices: Buffer,
}

/// What the pass counts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RtCounts {
    /// Bottom-level structures held: the meshes' and the grounds'.
    pub meshes: u32,
    pub grounds: u32,
    /// The last frame's instances: statics (walls and ground included) and objects.
    pub statics: u32,
    pub objects: u32,
    /// Their geometry rows.
    pub rows: u32,
    /// Bytes the device holds for the structures.
    pub bytes: u64,
    /// Bottom-level structures made since the pass was: it stops rising once the scene settles,
    /// and a skinned body's is made once, as it comes.
    pub made: u64,
    /// Refits of the skinned bodies' structures since the pass was: one a body a frame.
    pub refits: u64,
}

/// One entry of the instances' pass, and its geometry rows.
#[derive(Debug, Clone, Copy)]
struct Entry32 {
    words: [u32; 8],
}

/// One block's statics as the top-level structure places them, kept while the block is held as it
/// was: a block changing makes only its own again, not every block's.
#[derive(Debug)]
struct BlockStatics {
    /// The block as it was held: its build, its first instance and item, and its parts and rooms,
    /// and of its parts those drawn as ground cover.
    held: (u32, u32, u32, usize, usize, usize),
    /// Each placed part's entry, as its bytes, its index of the user's counted from the block's
    /// own first geometry row.
    entries: Vec<u8>,
    rows: Vec<u32>,
    /// The structures its parts place, each once.
    keys: BTreeSet<BlasKey>,
}

/// What a part places among the statics: its geometry rows, whether it is alpha-tested, and its
/// structure's address.
type Placed = (Vec<u32>, bool, u64);

/// What `block` holds, as [`BlockStatics::held`] keeps it.
fn held(block: &super::scene::BlockGpu) -> (u32, u32, u32, usize, usize, usize) {
    (
        block.generation,
        block.instances.start,
        block.items.start,
        block.parts.len(),
        block.rooms.len(),
        block.cover_parts,
    )
}

/// The scene's acceleration structures.
#[derive(Debug)]
pub struct RtPass {
    /// The instances' pass, and the check against the G-buffer, where the device traces rays.
    pipeline: Option<Pipeline>,
    check: Option<Pipeline>,
    /// The last frame's plan, for the check.
    last: Option<RtPlan>,
    tlas: Option<(Tlas, u32)>,
    blas: BTreeMap<BlasKey, Held>,
    grounds: BTreeMap<BlockId, GroundHeld>,
    /// The statics' entries and rows, and the scene's generation they were made at.
    generation: Option<u64>,
    /// Bottom-level structures made since the pass was, and refits of them.
    made: u64,
    refits: u64,
    /// The structures the statics place, each with how many held blocks place it.
    static_keys: BTreeMap<BlasKey, u32>,
    /// Each held block's statics, as they were last placed.
    block_statics: BTreeMap<BlockId, BlockStatics>,
    /// Each mesh's vertex count by its first vertex, and the meshes' generation it was made at.
    vertex_counts: (Option<u64>, BTreeMap<u32, u32>),
    /// The first vertex of each mesh posed this frame, whose structure is refitted.
    posed: BTreeSet<u32>,
    static_count: u32,
    static_rows: u32,
    statics: Option<(PerFrame, PerFrame)>,
    objects: Option<(PerFrame, PerFrame)>,
    /// The records the top-level build reads, and how many it holds.
    instances: Option<(Buffer, u32, u32)>,
    frame: u64,
    counts: RtCounts,
}

/// What a frame's pass records.
#[derive(Debug, Clone, Copy)]
pub struct RtPlan {
    pipeline: Pipeline,
    tlas: Tlas,
    /// The structure's index in the table, which the passes that trace read.
    pub index: u32,
    /// The geometry tables, by their index in the table: the statics' and the objects'.
    pub static_rows: u32,
    pub object_rows: u32,
    /// The entries, by their index in the table, and how many statics come first: an instance's
    /// place in the top-level structure finds its entry.
    pub static_entries: u32,
    pub object_entries: u32,
    pub statics: u32,
    objects: u32,
    instances: Buffer,
    instances_index: u32,
}

impl RtPass {
    /// The pass on `gfx`: nothing to make where the device does not trace rays.
    ///
    /// # Errors
    /// What the device refuses.
    pub fn new(gfx: &mut Device) -> Result<Self, dereth_gfx::Error> {
        let (pipeline, check) = if gfx.caps().ray_queries {
            (
                Some(gfx.create_compute_pipeline(&ComputeDesc {
                    label: "remaster acceleration instances",
                    shader: shader(Entry::RtCsInstances),
                })?),
                Some(gfx.create_compute_pipeline(&ComputeDesc {
                    label: "remaster acceleration check",
                    shader: shader(Entry::RtCheckCsCheck),
                })?),
            )
        } else {
            (None, None)
        };
        Ok(Self {
            pipeline,
            check,
            last: None,
            tlas: None,
            blas: BTreeMap::new(),
            grounds: BTreeMap::new(),
            generation: None,
            made: 0,
            refits: 0,
            static_keys: BTreeMap::new(),
            block_statics: BTreeMap::new(),
            vertex_counts: (None, BTreeMap::new()),
            posed: BTreeSet::new(),
            static_count: 0,
            static_rows: 0,
            statics: None,
            objects: None,
            instances: None,
            frame: 0,
            counts: RtCounts::default(),
        })
    }

    /// Whether the device traces rays.
    #[must_use]
    pub fn available(&self) -> bool {
        self.pipeline.is_some()
    }

    /// What the pass counts.
    #[must_use]
    pub fn counts(&self) -> RtCounts {
        self.counts
    }

    /// Let go of everything held: what nothing traces need not be kept.
    pub fn release(&mut self, gfx: &mut Device) {
        for (_, h) in std::mem::take(&mut self.blas) {
            gfx.destroy_blas(h.blas);
        }
        for (_, g) in std::mem::take(&mut self.grounds) {
            gfx.destroy_blas(g.blas);
            gfx.destroy_buffer(g.indices);
        }
        if let Some((t, _)) = self.tlas.take() {
            gfx.destroy_tlas(t);
        }
        if let Some((b, _, _)) = self.instances.take() {
            gfx.destroy_buffer(b);
        }
        for (a, b) in [self.statics.take(), self.objects.take()]
            .into_iter()
            .flatten()
        {
            gfx.destroy_buffer(a.buffer);
            gfx.destroy_buffer(b.buffer);
        }
        self.generation = None;
        self.static_keys.clear();
        self.block_statics.clear();
        self.vertex_counts = (None, BTreeMap::new());
        self.counts = RtCounts::default();
        self.last = None;
    }

    /// The meshes posed this frame, by their first vertex: their structures are refitted to them
    /// rather than kept as they stand.
    pub fn set_posed(&mut self, first_vertices: BTreeSet<u32>) {
        self.posed = first_vertices;
    }

    /// The frame's structures, when `wanted` and the device traces rays: the bottom-level ones
    /// the scene's resident meshes and grounds need made (built at the recording's head), the
    /// statics' entries made again when what is resident changed, the objects' every frame.
    ///
    /// # Errors
    /// What the device refuses.
    pub fn prepare(
        &mut self,
        gfx: &mut Device,
        scene: &GpuScene,
        wanted: bool,
    ) -> Result<Option<RtPlan>, dereth_gfx::Error> {
        let Some(pipeline) = self.pipeline else {
            return Ok(None);
        };
        if !wanted {
            if self.tlas.is_some() || !self.blas.is_empty() {
                self.release(gfx);
            }
            return Ok(None);
        }
        self.frame += 1;
        // Each mesh's vertex count by its first vertex, made again only when the meshes change.
        if self.vertex_counts.0 != Some(scene.mesh_generation) {
            self.vertex_counts = (
                Some(scene.mesh_generation),
                scene
                    .meshes
                    .values()
                    .map(|m| (m.vertices.start, m.vertices.end - m.vertices.start))
                    .collect(),
            );
        }
        let vertex_counts = std::mem::take(&mut self.vertex_counts.1);
        let made = self.make_frame(gfx, scene, &vertex_counts);
        self.vertex_counts.1 = vertex_counts;
        let (object_entries, object_rows) = made?;
        let objects = u32::try_from(object_entries.len()).unwrap_or(u32::MAX);
        let object_bytes = entries_bytes(&object_entries);
        let object_row_bytes = push_words(&object_rows);
        let (oe, or) = match self.objects.as_mut() {
            Some(o) => o,
            None => {
                let e = PerFrame::new(
                    gfx,
                    "remaster acceleration objects",
                    1 << 12,
                    BufferUsage::default(),
                )?;
                let r = PerFrame::new(
                    gfx,
                    "remaster acceleration object rows",
                    1 << 12,
                    BufferUsage::default(),
                )?;
                self.objects.insert((e, r))
            }
        };
        oe.write(gfx, &object_bytes)?;
        or.write(gfx, &object_row_bytes)?;
        let (object_entries_index, object_rows_index) = (oe.index, or.index);
        // Bottom-level structures nothing places any more go.
        let frame = self.frame;
        let keys = &self.static_keys;
        let stale: Vec<BlasKey> = self
            .blas
            .iter()
            .filter(|(k, h)| !keys.contains_key(*k) && frame.saturating_sub(h.seen) > KEEP_FRAMES)
            .map(|(k, _)| k.clone())
            .collect();
        for k in stale {
            if let Some(h) = self.blas.remove(&k) {
                gfx.destroy_blas(h.blas);
            }
        }
        let total = self.static_count + objects;
        // The top-level structure, made for every instance, and its records' buffer.
        let need = total.max(1);
        let grow = self.tlas.as_ref().is_none_or(|(_, cap)| *cap < need);
        if grow {
            if let Some((t, _)) = self.tlas.take() {
                gfx.destroy_tlas(t);
            }
            let cap = need.next_power_of_two();
            let t = gfx.create_tlas("remaster scene", cap)?;
            self.tlas = Some((t, cap));
        }
        let (tlas, _) = self.tlas.expect("made above");
        let index = gfx.bind_tlas(tlas)?;
        if self
            .instances
            .as_ref()
            .is_none_or(|(_, _, cap)| *cap < need)
        {
            if let Some((b, _, _)) = self.instances.take() {
                gfx.destroy_buffer(b);
            }
            let cap = need.next_power_of_two();
            let b = gfx.create_buffer(&BufferDesc {
                label: "remaster acceleration records",
                size: u64::from(cap) * dereth_gfx::rt::INSTANCE_BYTES,
                usage: BufferUsage::STORAGE | BufferUsage::ACCELERATION_INPUT,
                location: Location::Gpu,
            })?;
            let i = gfx.bind_buffer(b)?;
            self.instances = Some((b, i, cap));
        }
        let (instances, instances_index, _) = self.instances.expect("made above");
        let (se, sr) = self.statics.as_ref().expect("made with the statics");
        self.counts = RtCounts {
            meshes: u32::try_from(self.blas.len()).unwrap_or(u32::MAX),
            grounds: u32::try_from(self.grounds.len()).unwrap_or(u32::MAX),
            statics: self.static_count,
            objects,
            rows: self.static_rows + u32::try_from(object_rows.len() / 4).unwrap_or(u32::MAX),
            bytes: gfx.stats().acceleration_bytes,
            made: self.made,
            refits: self.refits,
        };
        let plan = RtPlan {
            pipeline,
            tlas,
            index,
            static_rows: sr.index,
            object_rows: object_rows_index,
            static_entries: se.index,
            object_entries: object_entries_index,
            statics: self.static_count,
            objects,
            instances,
            instances_index,
        };
        self.last = Some(plan);
        Ok(Some(plan))
    }

    /// The last frame's structures checked against what it drew: rays from the eye through every
    /// `step`th pixel a side of a `width` by `height` frame, with mask `mask`, each one's hit
    /// distance (negative for a miss) and its instance's index of the user's, row by row. `pools`
    /// are the frame's constants, the scene's indices, vertices and materials, and the sampler
    /// the alpha tests read with, by their index in the table. Submits and waits for a recording
    /// of its own; `None` when the last frame built none.
    ///
    /// # Errors
    /// What the device refuses.
    pub fn check(
        &self,
        gfx: &mut Device,
        pools: [u32; 5],
        (width, height): (u32, u32),
        step: u32,
        mask: u8,
    ) -> Result<Option<Vec<(f32, u32)>>, dereth_gfx::Error> {
        let (Some(p), Some(plan)) = (self.check, self.last) else {
            return Ok(None);
        };
        let [frame, indices, vertices, materials, sampler] = pools;
        let step = step.max(1);
        let (cols, rows) = (width.div_ceil(step), height.div_ceil(step));
        let bytes = u64::from(cols * rows) * 8;
        let out = gfx.create_buffer(&BufferDesc {
            label: "remaster acceleration check",
            size: bytes,
            usage: BufferUsage::STORAGE | BufferUsage::TRANSFER_SRC,
            location: Location::Gpu,
        })?;
        let back = gfx.create_buffer(&BufferDesc {
            label: "remaster acceleration check read back",
            size: bytes,
            usage: BufferUsage::TRANSFER_DST,
            location: Location::Readback,
        })?;
        let result = (|| {
            let out_index = gfx.bind_buffer(out)?;
            let mut r = gfx.begin()?;
            r.compute(&[Touch::Buffer(out, Use::StorageWrite)], |c| {
                c.set_pipeline(p)?;
                c.push(
                    0,
                    &push_words(&[
                        frame,
                        plan.index,
                        plan.static_rows,
                        plan.object_rows,
                        indices,
                        vertices,
                        materials,
                        sampler,
                        out_index,
                        width,
                        height,
                        step,
                        u32::from(mask),
                    ]),
                )?;
                c.dispatch(cols.div_ceil(8), rows.div_ceil(8), 1);
                Ok(())
            })?;
            r.copy_buffer(out, 0, back, 0, bytes)?;
            r.touch(&[Touch::Buffer(back, Use::HostRead)])?;
            let t = r.submit()?;
            gfx.wait(t)?;
            gfx.read_buffer(back, 0, bytes)
        })();
        gfx.destroy_buffer(out);
        gfx.destroy_buffer(back);
        let words = result?;
        Ok(Some(
            words
                .as_chunks::<8>()
                .0
                .iter()
                .map(|w| {
                    (
                        f32::from_le_bytes([w[0], w[1], w[2], w[3]]),
                        u32::from_le_bytes([w[4], w[5], w[6], w[7]]),
                    )
                })
                .collect(),
        ))
    }

    /// The bottom-level structure for `key`, made when it is not held; its address.
    fn blas_for(
        &mut self,
        gfx: &mut Device,
        scene: &GpuScene,
        key: &BlasKey,
        vertex_count: u32,
    ) -> Result<u64, dereth_gfx::Error> {
        let posed = self.posed.contains(&key.0);
        let buffers = (scene.vertices.buffer, scene.indices.buffer);
        if let Some(h) = self.blas.get_mut(key) {
            let stale = posed && h.made_at != self.frame;
            if stale && h.refitted && h.buffers != buffers {
                // Its pools moved: made again over their new buffers.
                h.refitted = false;
            } else if stale && h.refitted {
                gfx.refit_blas(h.blas)?;
                self.refits += 1;
                h.made_at = self.frame;
            }
            if !stale || h.refitted {
                h.seen = self.frame;
                return Ok(h.address);
            }
            // A mesh first held as it stands and now posed, or whose pools moved: made again, to
            // be refitted.
            if let Some(h) = self.blas.remove(key) {
                gfx.destroy_blas(h.blas);
            }
        }
        let geometries: Vec<BlasGeometry> = key
            .1
            .iter()
            .map(|(first, count, opaque)| BlasGeometry {
                vertices: scene.vertices.buffer,
                vertex_offset: u64::from(key.0) * MESH_VERTEX,
                vertex_stride: MESH_VERTEX,
                vertex_count,
                indices: scene.indices.buffer,
                index_offset: u64::from(*first) * 4,
                index_type: IndexType::U32,
                triangles: count / 3,
                opaque: *opaque,
            })
            .collect();
        let blas = if posed {
            gfx.create_blas_refitted("remaster posed mesh", &geometries)?
        } else {
            gfx.create_blas("remaster mesh", &geometries)?
        };
        self.made += 1;
        let address = gfx.blas_address(blas)?;
        self.blas.insert(
            key.clone(),
            Held {
                blas,
                address,
                seen: self.frame,
                made_at: self.frame,
                refitted: posed,
                buffers,
            },
        );
        Ok(address)
    }

    /// A part's finest level as a structure's key and its geometry rows, when it draws anything
    /// opaque or alpha-tested.
    fn part_key(scene: &GpuScene, part: &PartItems) -> Option<(BlasKey, Vec<u32>, bool)> {
        let items = part.levels.first()?;
        let mut geometries = Vec::new();
        let mut rows = Vec::new();
        let mut alpha = false;
        let mut base = None;
        for i in items.clone() {
            let (Some(d), Some(b)) = (
                scene.item_draws.get(i as usize),
                scene.buckets.get(i as usize),
            ) else {
                continue;
            };
            if !matches!(b, Bucket::Opaque | Bucket::OpaqueTwoSided) || d.index_count < 3 {
                continue;
            }
            base.get_or_insert(d.vertex_base);
            geometries.push((d.first_index, d.index_count, !d.alpha_test));
            alpha |= d.alpha_test;
            let flags = if d.alpha_test {
                row_flag::ALPHA_TESTED
            } else {
                0
            } | if d.two_sided { row_flag::TWO_SIDED } else { 0 };
            rows.extend([d.first_index, d.vertex_base, d.material, flags]);
        }
        Some(((base?, geometries), rows, alpha))
    }

    /// The statics' entries and rows made again when what is resident changed, and the objects'
    /// entries and rows for this frame.
    fn make_frame(
        &mut self,
        gfx: &mut Device,
        scene: &GpuScene,
        vertex_counts: &BTreeMap<u32, u32>,
    ) -> Result<(Vec<Entry32>, Vec<u32>), dereth_gfx::Error> {
        if self.generation != Some(scene.static_generation) {
            self.make_statics(gfx, scene, vertex_counts)?;
            self.generation = Some(scene.static_generation);
        }
        self.make_objects(gfx, scene, vertex_counts)
    }

    /// Every static's, wall's and ground's entry and rows, made again.
    fn make_statics(
        &mut self,
        gfx: &mut Device,
        scene: &GpuScene,
        vertex_counts: &BTreeMap<u32, u32>,
    ) -> Result<(), dereth_gfx::Error> {
        let mut entries: Vec<u8> = Vec::new();
        let mut static_count = 0u32;
        let mut rows: Vec<u32> = Vec::new();
        let gone: Vec<BlockId> = self
            .block_statics
            .keys()
            .filter(|id| !scene.blocks.contains_key(id))
            .copied()
            .collect();
        for id in gone {
            if let Some(own) = self.block_statics.remove(&id) {
                forget_keys(&mut self.static_keys, &own.keys);
            }
        }
        for (id, block) in &scene.blocks {
            let fresh = self
                .block_statics
                .get(id)
                .is_some_and(|b| b.held == held(block));
            if !fresh {
                if let Some(own) = self.block_statics.remove(id) {
                    forget_keys(&mut self.static_keys, &own.keys);
                }
                let mut own = BlockStatics {
                    held: held(block),
                    entries: Vec::new(),
                    rows: Vec::new(),
                    keys: BTreeSet::new(),
                };
                let parts = block
                    .parts
                    .iter()
                    .chain(block.rooms.iter().flat_map(|r| r.statics.iter()))
                    .chain(block.rooms.iter().map(|r| &r.walls));
                // The part before, and what it placed: a block's ground cover is thousands of
                // parts drawing one model alike, each placing what the one before it did.
                let mut last: Option<(&PartItems, Option<Placed>)> = None;
                for part in parts {
                    let placed = match last {
                        Some((before, ref placed))
                            if same_draws((&scene.item_draws, &scene.buckets), before, part) =>
                        {
                            placed.clone()
                        }
                        _ => match Self::part_key(scene, part) {
                            Some((key, part_rows, alpha)) => {
                                match vertex_counts.get(&key.0).copied() {
                                    Some(count) => {
                                        let address = self.blas_for(gfx, scene, &key, count)?;
                                        own.keys.insert(key);
                                        Some((part_rows, alpha, address))
                                    }
                                    None => None,
                                }
                            }
                            None => None,
                        },
                    };
                    last = Some((part, placed.clone()));
                    let Some((part_rows, alpha, address)) = placed else {
                        continue;
                    };
                    let custom = u32::try_from(own.rows.len() / 4).unwrap_or(0);
                    own.rows.extend(part_rows);
                    let m = if part.cover {
                        mask::COVER
                    } else if alpha {
                        mask::STATIC_ALPHA
                    } else {
                        mask::STATIC
                    };
                    for w in entry(part.instance, custom, m, 0, address).words {
                        own.entries.extend_from_slice(&w.to_le_bytes());
                    }
                }
                for k in &own.keys {
                    *self.static_keys.entry(k.clone()).or_insert(0) += 1;
                }
                self.block_statics.insert(*id, own);
            }
            if let Some(own) = self.block_statics.get(id) {
                let base = u32::try_from(rows.len() / 4).unwrap_or(0);
                rows.extend_from_slice(&own.rows);
                append_entries(&mut entries, &own.entries, base);
                static_count += u32::try_from(own.entries.len() / ENTRY_BYTES).unwrap_or(u32::MAX);
            }
            // The block's ground.
            if let (Some(g), true) = (scene.ground.get(id), block.slot != super::scene::NONE) {
                let address = self.ground_for(gfx, scene, *id, g)?;
                let custom = u32::try_from(rows.len() / 4).unwrap_or(0);
                rows.extend([
                    0,
                    g.vertices.start,
                    g.cells.start,
                    row_flag::GROUND | (g.side << 8),
                ]);
                for w in entry(block.slot, custom, mask::GROUND, 1, address).words {
                    entries.extend_from_slice(&w.to_le_bytes());
                }
                static_count += 1;
            }
        }
        // Grounds of blocks no longer held go.
        let gone: Vec<BlockId> = self
            .grounds
            .keys()
            .filter(|b| !scene.ground.contains_key(b))
            .copied()
            .collect();
        for b in gone {
            if let Some(g) = self.grounds.remove(&b) {
                gfx.destroy_blas(g.blas);
                gfx.destroy_buffer(g.indices);
            }
        }
        self.static_count = static_count;
        self.static_rows = u32::try_from(rows.len() / 4).unwrap_or(u32::MAX);
        let (e, r) = match self.statics.as_mut() {
            Some(s) => s,
            None => {
                let e = PerFrame::new(
                    gfx,
                    "remaster acceleration statics",
                    1 << 16,
                    BufferUsage::default(),
                )?;
                let r = PerFrame::new(
                    gfx,
                    "remaster acceleration static rows",
                    1 << 16,
                    BufferUsage::default(),
                )?;
                self.statics.insert((e, r))
            }
        };
        e.write(gfx, &entries)?;
        r.write(gfx, &push_words(&rows))?;
        Ok(())
    }

    /// The bottom-level structure of block `id`'s ground, made again when its vertices or cuts
    /// changed; its address.
    fn ground_for(
        &mut self,
        gfx: &mut Device,
        scene: &GpuScene,
        id: BlockId,
        g: &super::scene::GroundGpu,
    ) -> Result<u64, dereth_gfx::Error> {
        if let Some(h) = self.grounds.get(&id) {
            if h.vertices == g.vertices.start && h.cuts == g.cuts {
                return Ok(h.address);
            }
        }
        if let Some(h) = self.grounds.remove(&id) {
            gfx.destroy_blas(h.blas);
            gfx.destroy_buffer(h.indices);
        }
        let indices = ground_indices(g.side, &g.cuts);
        let bytes = push_words(&indices);
        let buffer = gfx.create_buffer(&BufferDesc {
            label: "remaster ground triangles",
            size: bytes.len() as u64,
            usage: BufferUsage::ACCELERATION_INPUT | BufferUsage::STORAGE,
            location: Location::Gpu,
        })?;
        gfx.write_buffer(buffer, 0, &bytes)?;
        self.made += 1;
        let blas = gfx.create_blas(
            "remaster ground",
            &[BlasGeometry {
                vertices: scene.ground_vertices.buffer,
                vertex_offset: u64::from(g.vertices.start) * GROUND_VERTEX,
                vertex_stride: GROUND_VERTEX,
                vertex_count: g.vertices.end - g.vertices.start,
                indices: buffer,
                index_offset: 0,
                index_type: IndexType::U32,
                triangles: u32::try_from(indices.len() / 3).unwrap_or(0),
                opaque: true,
            }],
        )?;
        let address = gfx.blas_address(blas)?;
        self.grounds.insert(
            id,
            GroundHeld {
                blas,
                address,
                vertices: g.vertices.start,
                cuts: g.cuts.clone(),
                indices: buffer,
            },
        );
        Ok(address)
    }

    /// Every object's parts' entries and rows, this frame.
    fn make_objects(
        &mut self,
        gfx: &mut Device,
        scene: &GpuScene,
        vertex_counts: &BTreeMap<u32, u32>,
    ) -> Result<(Vec<Entry32>, Vec<u32>), dereth_gfx::Error> {
        let mut entries = Vec::new();
        let mut rows = Vec::new();
        for (key, object) in &scene.objects {
            let m = if *key == ObjectKey::Body {
                mask::BODY
            } else {
                mask::OBJECT
            };
            for part in &object.parts {
                let Some((k, part_rows, _)) = Self::part_key(scene, part) else {
                    continue;
                };
                let Some(count) = vertex_counts.get(&k.0).copied() else {
                    continue;
                };
                let address = self.blas_for(gfx, scene, &k, count)?;
                let custom = u32::try_from(rows.len() / 4).unwrap_or(0) | OBJECT_ROWS;
                rows.extend(part_rows);
                entries.push(entry(part.instance, custom, m, 0, address));
            }
        }
        Ok((entries, rows))
    }
}

impl RtPlan {
    /// Write each instance's record from its rows, then build the top-level structure from them.
    ///
    /// # Errors
    /// What the device refuses.
    pub fn record(
        &self,
        r: &mut Recorder<'_>,
        frame: u32,
        instances: u32,
        blocks: u32,
    ) -> Result<(), dereth_gfx::Error> {
        r.timestamp("acceleration");
        let total = self.statics + self.objects;
        if total > 0 {
            r.compute(&[Touch::Buffer(self.instances, Use::StorageWrite)], |c| {
                c.set_pipeline(self.pipeline)?;
                for (entries, count, first) in [
                    (self.static_entries, self.statics, 0),
                    (self.object_entries, self.objects, self.statics),
                ] {
                    if count == 0 {
                        continue;
                    }
                    c.push(
                        0,
                        &push_words(&[
                            frame,
                            instances,
                            blocks,
                            entries,
                            count,
                            self.instances_index,
                            first,
                        ]),
                    )?;
                    c.dispatch(count.div_ceil(64), 1, 1);
                }
                Ok(())
            })?;
        }
        r.build_tlas_from(self.tlas, self.instances, 0, total)
    }
}

/// One entry: the instance (or block slot for a ground), its index of the user's, its mask and
/// kind, and its bottom-level structure's address.
fn entry(instance: u32, custom: u32, mask: u8, kind: u32, address: u64) -> Entry32 {
    #[allow(clippy::cast_possible_truncation)] // the address's two halves
    Entry32 {
        words: [
            instance,
            custom,
            u32::from(mask) | (kind << 8),
            0,
            address as u32,
            (address >> 32) as u32,
            0,
            0,
        ],
    }
}

/// The bytes of an entry.
const ENTRY_BYTES: usize = 32;

fn entries_bytes(entries: &[Entry32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(entries.len() * ENTRY_BYTES);
    for w in entries.iter().flat_map(|e| e.words) {
        out.extend_from_slice(&w.to_le_bytes());
    }
    out
}

/// A block's entries, as [`BlockStatics::entries`] keeps them, added to `out`: each one's index
/// of the user's moved from the block's own rows to the table's, which hold the block's from
/// `base` on.
fn append_entries(out: &mut Vec<u8>, own: &[u8], base: u32) {
    let at = out.len();
    out.extend_from_slice(own);
    for e in out[at..].as_chunks_mut::<ENTRY_BYTES>().0 {
        let custom = u32::from_le_bytes([e[4], e[5], e[6], e[7]]) + base;
        e[4..8].copy_from_slice(&custom.to_le_bytes());
    }
}

/// Whether parts `a` and `b` draw their finest levels alike: the same items, as the structures
/// and their rows take them, in the same buckets.
fn same_draws((draws, buckets): (&[ItemDraw], &[Bucket]), a: &PartItems, b: &PartItems) -> bool {
    match (a.levels.first(), b.levels.first()) {
        (Some(x), Some(y)) => {
            x.len() == y.len()
                && x.clone().zip(y.clone()).all(|(i, j)| {
                    let (i, j) = (i as usize, j as usize);
                    draws.get(i) == draws.get(j) && buckets.get(i) == buckets.get(j)
                })
        }
        (None, None) => true,
        _ => false,
    }
}

/// `keys` no longer placed by a block that placed them: each one's count down, and gone at
/// none.
fn forget_keys(counts: &mut BTreeMap<BlasKey, u32>, keys: &BTreeSet<BlasKey>) {
    for k in keys {
        if let Some(n) = counts.get_mut(k) {
            *n -= 1;
            if *n == 0 {
                counts.remove(k);
            }
        }
    }
}

/// A block's ground as triangles over its `(side + 1)²` vertices, two a cell in the cells' order,
/// each cell cut as `cuts` says: the corners the ground pass draws.
#[must_use]
pub fn ground_indices(side: u32, cuts: &[bool]) -> Vec<u32> {
    let svc = side + 1;
    let mut out = Vec::with_capacity((side * side * 6) as usize);
    for cell in 0..side * side {
        let (i, j) = (cell / side, cell % side);
        let sw = i * svc + j;
        let se = (i + 1) * svc + j;
        let ne = (i + 1) * svc + j + 1;
        let nw = i * svc + j + 1;
        if cuts.get(cell as usize).copied().unwrap_or(false) {
            out.extend([sw, se, ne, sw, ne, nw]);
        } else {
            out.extend([sw, se, nw, ne, nw, se]);
        }
    }
    out
}

impl super::Renderer {
    /// Keep the scene's acceleration structures, built every frame, whether or not a pass of the
    /// frame traces rays: for the checks, and for passes outside the frame. Nothing where the
    /// device does not trace rays.
    pub fn set_acceleration(&mut self, on: bool) {
        self.rt_kept = on;
    }

    /// Whether the device traces rays.
    #[must_use]
    pub fn traces_rays(&self) -> bool {
        self.rt.available()
    }

    /// What the acceleration structures count.
    #[must_use]
    pub fn acceleration_counts(&self) -> RtCounts {
        self.rt.counts()
    }

    /// The last frame's acceleration structures checked against its picture: see
    /// [`RtPass::check`]. `None` when it built none.
    ///
    /// # Errors
    /// What the device refuses.
    pub fn acceleration_check(
        &mut self,
        step: u32,
        mask: u8,
    ) -> Result<Option<Vec<(f32, u32)>>, dereth_gfx::Error> {
        let Some(t) = self.targets else {
            return Ok(None);
        };
        let pools = [
            self.constants.index,
            self.scene.indices.index,
            self.scene.vertices.index,
            self.scene.materials.index,
            self.sampler(0),
        ];
        self.gfx.wait_idle()?;
        self.rt
            .check(&mut self.gfx, pools, (t.width, t.height), step, mask)
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental)
    use super::*;

    /// A ground of two cells a side has eight triangles over its nine vertices, each cell's
    /// two in the cells' order, cut as asked: the corners the ground pass draws.
    #[test]
    fn a_grounds_triangles_are_two_a_cell_cut_as_the_cells_are() {
        let t = ground_indices(2, &[true, false, false, true]);
        assert_eq!(t.len(), 24);
        assert_eq!(&t[0..6], &[0, 3, 4, 0, 4, 1]);
        assert_eq!(&t[6..12], &[1, 4, 2, 5, 2, 4]);
        assert!(t.iter().all(|v| *v < 9));
    }

    /// A part drawing one level.
    fn one(level: std::ops::Range<u32>) -> PartItems {
        part(std::slice::from_ref(&level))
    }

    fn part(levels: &[std::ops::Range<u32>]) -> PartItems {
        PartItems {
            instance: 0,
            degrade: None,
            levels: levels.to_vec(),
            sphere: ([0.0; 3], 0.0),
            transform: glam::Mat4::IDENTITY,
            scale: glam::Vec3::ONE,
            turned: None,
            lifts: [0.0; 2],
            mesh: 0,
            cover: false,
        }
    }

    /// **Parts drawn alike are known alike, and only they**: two parts whose finest levels draw
    /// the same items in the same buckets (a ground cover's placements of one model) place the
    /// same structure and rows; one whose item differs in anything, or whose bucket does, or that
    /// has more items, does not.
    #[test]
    fn parts_drawn_alike_are_known_alike_and_only_they() {
        let draw = |first_index| ItemDraw {
            index_count: 6,
            first_index,
            vertex_base: 40,
            material: 3,
            ..ItemDraw::default()
        };
        let draws = [
            draw(0),
            draw(6),
            draw(0),
            draw(6),
            draw(12),
            draw(0),
            draw(6),
        ];
        let buckets = [
            Bucket::Opaque,
            Bucket::OpaqueTwoSided,
            Bucket::Opaque,
            Bucket::OpaqueTwoSided,
            Bucket::Opaque,
            Bucket::Opaque,
            Bucket::Opaque,
        ];
        let pools = (&draws[..], &buckets[..]);
        let first = part(&[0..2, 4..5]);
        assert!(
            same_draws(pools, &first, &one(2..4)),
            "alike, coarser levels aside"
        );
        assert!(!same_draws(pools, &first, &one(4..6)), "another item");
        assert!(!same_draws(pools, &first, &one(5..7)), "another bucket");
        assert!(!same_draws(pools, &first, &one(2..5)), "more items");
        assert!(!same_draws(pools, &first, &part(&[])), "nothing drawn");
        assert!(same_draws(pools, &part(&[]), &part(&[])));
    }

    /// **A structure stays placed while any block places it**: two blocks placing one structure,
    /// and one of them another; once the second lets go, the shared one is still placed and its
    /// own is not.
    #[test]
    fn a_structure_stays_placed_while_any_block_places_it() {
        let key = |v: u32| -> BlasKey { (v, vec![(0, 3, true)]) };
        let mut counts: BTreeMap<BlasKey, u32> = BTreeMap::new();
        let a: BTreeSet<BlasKey> = [key(1)].into();
        let b: BTreeSet<BlasKey> = [key(1), key(2)].into();
        for k in a.iter().chain(&b) {
            *counts.entry(k.clone()).or_insert(0) += 1;
        }
        forget_keys(&mut counts, &b);
        assert!(counts.contains_key(&key(1)));
        assert!(!counts.contains_key(&key(2)));
        forget_keys(&mut counts, &a);
        assert!(counts.is_empty());
    }

    /// **A block's entries join the table's with their rows'**: added after what is there, each
    /// one's index of the user's moves by where the block's rows begin, and nothing else of it
    /// changes.
    #[test]
    fn a_blocks_entries_join_the_tables_with_their_rows() {
        let own = entries_bytes(&[
            entry(5, 0, mask::COVER, 0, 0x1_0000_0040),
            entry(6, 2, mask::STATIC, 0, 0x80),
        ]);
        let mut out = entries_bytes(&[entry(1, 0, mask::GROUND, 1, 0x40)]);
        append_entries(&mut out, &own, 10);
        assert_eq!(
            out,
            entries_bytes(&[
                entry(1, 0, mask::GROUND, 1, 0x40),
                entry(5, 10, mask::COVER, 0, 0x1_0000_0040),
                entry(6, 12, mask::STATIC, 0, 0x80),
            ])
        );
    }

    /// An entry's words: the instance, its index of the user's, its mask and kind, and the
    /// address's halves.
    #[test]
    fn an_entry_carries_its_instance_index_mask_kind_and_address() {
        let e = entry(7, 9, mask::GROUND, 1, 0x1_2345_6789);
        assert_eq!(e.words, [7, 9, 4 | (1 << 8), 0, 0x2345_6789, 1, 0, 0]);
    }
}
