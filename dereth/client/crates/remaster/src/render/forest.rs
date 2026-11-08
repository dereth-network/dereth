//! The far forest of an art pack: every tree it has far pictures for, out to the reach, culled on
//! the device each frame and drawn into the G-buffer after the impostors (`shaders/forest.slang`);
//! and the forest cover the far ground takes past them.
//!
//! The trees are written block by block as [`Slots`] says, so walking on writes a ring of blocks,
//! not every tree within reach. A model's pictures come to the device a few a frame, and its trees
//! wait for them.

use dereth_gfx::{
    Buffer, BufferDesc, BufferUsage, ColorAttachment, ColorTarget, Compare, ComputeDesc, Cull,
    DepthAttachment, DepthState, Device, Format, GraphicsDesc, Load, Location, Pipeline,
    RasterPass, Recorder, Rect, Texture, TextureDesc, TextureUsage, Touch, Use,
};
use glam::{Mat4, Vec2, Vec3};

use std::collections::{BTreeMap, HashMap, HashSet};
use std::ops::Range;
use std::sync::Arc;

use super::scene::{gpu_format, GpuScene};
use super::{push_words, shader, Targets, ALBEDO, DEPTH, LIGHT, MATERIAL, NORMAL};
use crate::scene::{BlockId, FarKnobs, FarPack, FarTree, FarTreeSet, ForestCover, FrameDelta};
use crate::shaders::Entry;

/// The bytes of one tree: its foot and scale, its rotation, and its model, number and slot.
const TREE_BYTES: usize = 48;
/// The bytes of one model: its bounds, its quad and cut, its color, its pictures.
const MODEL_BYTES: usize = 64;
/// The bytes of one tree kept: its place, how much of its size it keeps, how much wider it stands.
const KEPT_BYTES: u64 = 12;
/// The words of the excluded blocks' bits: a bit a block.
const EXCLUDED_WORDS: usize = 65536 / 32;
/// The most slots the buffers hold, a bit each in the culling's list of those left out.
const SLOTS: usize = EXCLUDED_WORDS * 32;
/// The bytes of the constants, as the shader's `C_*` lay them out.
const CONSTANT_BYTES: usize = 16 * 10;
/// The trees the culling looks at in a group.
const GROUP: u32 = 64;
/// How far a far tree is sunk into the ground, meters a meter of its distance from the eye: the far
/// ground's mesh is coarser the farther it lies, and a little below its lowest point a tree never
/// floats over it.
const SINK: f32 = 0.0015;
/// How far a tree's fade stretches as the thinning passes it, a share of the thinning.
const FADE: f32 = 0.1;
/// The least height on screen a far tree is drawn at, pixels: smaller, it fades out, and the forest
/// cover takes its place.
const LEAST_PIXELS: f32 = 2.0;
/// A model whose pictures are not on the device yet.
const NO_PICTURES: u32 = u32::MAX;
/// The models whose pictures a frame sends to the device.
const PICTURES_A_FRAME: usize = 2;
/// The forest cover's canopy height a step of its texture holds, meters.
const CANOPY_STEP: f32 = 0.25;
/// Where the forest cover comes in while the far trees are not drawn, meters from the eye.
const COVER_FROM: f32 = 400.0;
const COVER_TO: f32 = 800.0;
/// How much of the light the crowns of the forest cover keep off the ground under them, past the
/// reach of the sun's shadows: there their trees cast none.
const FLOOR_SHADE: f32 = 0.45;

/// One block's trees in the buffers: its landblock, the list they were written from, where they
/// lie, and whether a block holds the slot.
#[derive(Debug)]
struct Slot {
    block: BlockId,
    from: Arc<Vec<FarTree>>,
    range: Range<u32>,
    live: bool,
}

/// What the far trees' buffers hold, block by block. A block that leaves the set has its trees
/// written over with nought, trees of the first slot (never a block's, always left out by the
/// culling), and its room and its slot are free; a block new to the set is written into the least
/// free room it fits, or after the rest.
#[derive(Debug)]
struct Slots {
    /// How many trees there is room for, and how far they are written: no tree past it.
    capacity: u32,
    count: u32,
    slots: Vec<Slot>,
    /// The slots no block holds.
    spare: Vec<usize>,
    /// The free room before `count`, each start to its end, none touching another.
    free: BTreeMap<u32, u32>,
}

impl Slots {
    /// Room for `capacity` trees.
    fn new(capacity: u32) -> Self {
        Self {
            capacity,
            count: 0,
            slots: vec![Slot {
                block: BlockId::MAX,
                from: Arc::new(Vec::new()),
                range: 0..0,
                live: false,
            }],
            spare: Vec::new(),
            free: BTreeMap::new(),
        }
    }

    /// Bring what is held to `set` (a tree's model by `of`, its retail object's place among the
    /// models): blocks it no longer holds, or holds as another list, are written over and freed,
    /// and its blocks new here are written. The writes, each the tree it starts at and its bytes,
    /// in order; `None` when a block fits nowhere: the buffers are then made again, holding only
    /// the set.
    fn take(
        &mut self,
        set: &FarTreeSet,
        of: &dyn Fn(u32) -> Option<u32>,
    ) -> Option<Vec<(u32, Vec<u8>)>> {
        // ORDER-OK: looked up by block.
        let wanted: HashMap<BlockId, &Arc<Vec<FarTree>>> =
            set.blocks.iter().map(|(b, t)| (*b, t)).collect();
        // ORDER-OK: looked up by block.
        let mut held = HashSet::new();
        let mut writes = Vec::new();
        for k in 0..self.slots.len() {
            let slot = &self.slots[k];
            if !slot.live {
                continue;
            }
            if wanted
                .get(&slot.block)
                .is_some_and(|t| Arc::ptr_eq(t, &slot.from))
            {
                held.insert(slot.block);
                continue;
            }
            let range = slot.range.clone();
            self.slots[k].live = false;
            self.spare.push(k);
            if !range.is_empty() {
                writes.push((
                    range.start,
                    vec![0; (range.end - range.start) as usize * TREE_BYTES],
                ));
                self.free_room(range);
            }
        }
        for (block, list) in &set.blocks {
            if held.contains(block) {
                continue;
            }
            let models: Vec<u32> = list.iter().filter_map(|t| of(t.id)).collect();
            if models.is_empty() {
                continue;
            }
            let n = u32::try_from(models.len()).ok()?;
            let at = self.room(n)?;
            let index = match self.spare.pop() {
                Some(k) => k,
                None if self.slots.len() < SLOTS => {
                    self.slots.push(Slot {
                        block: BlockId::MAX,
                        from: Arc::new(Vec::new()),
                        range: 0..0,
                        live: false,
                    });
                    self.slots.len() - 1
                }
                None => return None,
            };
            let mut bytes = Vec::with_capacity(models.len() * TREE_BYTES);
            let slot_word = u32::try_from(index).ok()?;
            for (t, &m) in list.iter().filter(|t| of(t.id).is_some()).zip(&models) {
                for f in [t.at.x, t.at.y, t.at.z, t.scale] {
                    bytes.extend_from_slice(&f.to_le_bytes());
                }
                for f in t.rotation.to_array() {
                    bytes.extend_from_slice(&f.to_le_bytes());
                }
                for word in [m, t.seed, slot_word, 0] {
                    bytes.extend_from_slice(&word.to_le_bytes());
                }
            }
            self.slots[index] = Slot {
                block: *block,
                from: Arc::clone(list),
                range: at..at + n,
                live: true,
            };
            writes.push((at, bytes));
        }
        Some(writes)
    }

    /// Where `n` trees are written: the start of the least free room they fit, or after the rest;
    /// `None` when there is no room for them.
    fn room(&mut self, n: u32) -> Option<u32> {
        let fits = self
            .free
            .iter()
            .filter(|(start, end)| *end - *start >= n)
            .min_by_key(|(start, end)| *end - *start)
            .map(|(start, end)| (*start, *end));
        if let Some((start, end)) = fits {
            self.free.remove(&start);
            if start + n < end {
                self.free.insert(start + n, end);
            }
            return Some(start);
        }
        if self.count + n > self.capacity {
            return None;
        }
        self.count += n;
        Some(self.count - n)
    }

    /// Free `range`, joined to the free room on either side, and to the room after the rest.
    fn free_room(&mut self, range: Range<u32>) {
        let (mut start, mut end) = (range.start, range.end);
        let before = self
            .free
            .range(..start)
            .next_back()
            .filter(|(_, before_end)| **before_end == start)
            .map(|(before, _)| *before);
        if let Some(before) = before {
            self.free.remove(&before);
            start = before;
        }
        if let Some(after_end) = self.free.remove(&end) {
            end = after_end;
        }
        if end == self.count {
            self.count = start;
        } else {
            self.free.insert(start, end);
        }
    }

    /// The slots the culling leaves out, a bit each: no block's, or of a block `resident` says has
    /// its own objects standing in the world; and the trees standing in the rest.
    fn left_out(&self, resident: &dyn Fn(BlockId) -> bool) -> (Vec<u32>, u32) {
        let mut bits = vec![0u32; EXCLUDED_WORDS];
        let mut standing = 0;
        for (k, slot) in self.slots.iter().enumerate() {
            if !slot.live || resident(slot.block) {
                bits[k / 32] |= 1 << (k % 32);
            } else {
                standing += slot.range.end - slot.range.start;
            }
        }
        (bits, standing)
    }
}

/// A buffer on the device, and its index in the table.
#[derive(Debug, Clone, Copy)]
struct Bound {
    buffer: Buffer,
    index: u32,
}

fn bound(
    d: &mut Device,
    label: &'static str,
    size: u64,
    usage: BufferUsage,
) -> Result<Bound, dereth_gfx::Error> {
    let buffer = d.create_buffer(&BufferDesc {
        label,
        size: size.max(16),
        usage,
        location: Location::Gpu,
    })?;
    Ok(Bound {
        buffer,
        index: d.bind_buffer(buffer)?,
    })
}

/// The trees' buffers on the device, kept as [`Slots`] says. They are made again, holding only the
/// blocks within reach, when a block fits nowhere.
#[derive(Debug)]
struct Trees {
    /// The set's serial the buffers were last brought to.
    serial: u64,
    held: Slots,
    trees: Bound,
    kept: Bound,
    excluded: Bound,
    /// The slots left out last, a bit each, so they are written only when they change.
    left_out: Vec<u32>,
    /// The bytes the buffers take on the device.
    bytes: u64,
}

impl Trees {
    fn new(d: &mut Device, capacity: u32) -> Result<Self, dereth_gfx::Error> {
        let trees_size = u64::from(capacity) * TREE_BYTES as u64;
        let kept_size = u64::from(capacity) * KEPT_BYTES;
        Ok(Self {
            serial: 0,
            held: Slots::new(capacity),
            trees: bound(d, "remaster far trees", trees_size, BufferUsage::STORAGE)?,
            kept: bound(
                d,
                "remaster far trees kept",
                kept_size,
                BufferUsage::STORAGE,
            )?,
            excluded: bound(
                d,
                "remaster far trees left out",
                (EXCLUDED_WORDS * 4) as u64,
                BufferUsage::STORAGE,
            )?,
            left_out: vec![u32::MAX; EXCLUDED_WORDS],
            bytes: trees_size + kept_size + (EXCLUDED_WORDS * 4) as u64,
        })
    }

    fn destroy(self, d: &mut Device) {
        for b in [self.trees, self.kept, self.excluded] {
            d.destroy_buffer(b.buffer);
        }
    }

    /// Bring the buffers to `set`; `false` when they are to be made again.
    fn take(
        &mut self,
        d: &mut Device,
        set: &FarTreeSet,
        of: &dyn Fn(u32) -> Option<u32>,
    ) -> Result<bool, dereth_gfx::Error> {
        let Some(writes) = self.held.take(set, of) else {
            return Ok(false);
        };
        for (first, bytes) in writes {
            d.write_buffer(
                self.trees.buffer,
                u64::from(first) * TREE_BYTES as u64,
                &bytes,
            )?;
        }
        self.serial = set.serial;
        Ok(true)
    }
}

/// A model's pictures on the device: its color and its normals, each with its index in the table.
#[derive(Debug, Clone, Copy)]
struct Pictures {
    base: (Texture, u32),
    normal: (Texture, u32),
}

/// The forest cover on the device.
#[derive(Debug, Clone, Copy)]
struct Cover {
    serial: u64,
    color: (Texture, u32),
    canopy: (Texture, u32),
}

/// What the far ground takes of the forest cover, as the ground's constants hold it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CoverRows {
    /// How strongly it tints the far ground (nought for none), where it comes in, and how much its
    /// crowns shade the ground past the sun's shadows.
    pub tint: [f32; 4],
    /// How many meters its whole canopy raises the far ground (nought for none), and the
    /// distances it rises in over.
    pub canopy: [f32; 4],
    /// The cover's and the canopy's pictures.
    pub pictures: [u32; 2],
}

impl Default for CoverRows {
    fn default() -> Self {
        Self {
            tint: [0.0; 4],
            canopy: [0.0; 4],
            pictures: [NO_PICTURES; 2],
        }
    }
}

/// Where the forest cover comes in, meters from the eye: from where the far trees thin out to
/// their reach while they are drawn, and over a stretch past the blocks round the viewer while
/// they are not.
fn cover_band(k: &FarKnobs) -> (f32, f32) {
    if k.trees && k.reach > 0.0 {
        let to = k.reach.max(1.0);
        (k.thin_from.min(to - 1.0), to)
    } else {
        (COVER_FROM, COVER_TO)
    }
}

/// What the far forest did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ForestStats {
    /// Models with far pictures, and those whose pictures are on the device.
    pub models: u32,
    pub pictures: u32,
    /// Trees standing: written, and in no block whose own objects stand in the world.
    pub standing: u32,
    /// Whether the forest cover is on the device.
    pub cover: bool,
    /// The bytes the trees' buffers and the cover take on the device; the pictures' bytes.
    pub bytes: u64,
    pub picture_bytes: u64,
}

/// What a frame's far trees record.
#[derive(Debug, Clone)]
pub struct ForestPlan {
    cull: Pipeline,
    draw: Pipeline,
    constants: Bound,
    models: Bound,
    trees: Bound,
    kept: Bound,
    excluded: Bound,
    args: Bound,
    /// The bytes of the draw's command. Its instance count lies 12 bytes before its end.
    stride: u32,
    count: u32,
    frame: u32,
    clamp: u32,
    /// The weather's constants, `NONE` without them, and the overhead map they read.
    weather: (u32, Option<Texture>),
}

/// What the pass needs of a frame.
#[derive(Debug)]
pub struct ForestInputs<'a> {
    pub frame: &'a FrameDelta,
    pub scene: &'a GpuScene,
    /// The view, the eye (both the frame's space), and pixels a meter at a meter's distance.
    pub clip: Mat4,
    pub eye: Vec3,
    pub pixels: f32,
    /// The frame's constants, and the sampler that holds at the edges.
    pub constants: u32,
    pub clamp: u32,
    /// The weather's constants, which wet the trees out in the rain as the meshes are (`NONE`
    /// without them), and the overhead map they read.
    pub weather: (u32, Option<Texture>),
}

/// The far forest's pass: its pipelines, its buffers and its pictures.
#[derive(Debug)]
pub struct ForestPass {
    cull: Pipeline,
    draw: Pipeline,
    constants: Bound,
    args: Bound,
    /// The pack on the device, and each of its models' pictures.
    pack: Option<Arc<FarPack>>,
    models: Option<Bound>,
    // ORDER-OK: looked up by retail object.
    place: HashMap<u32, u32>,
    pictures: Vec<Option<Pictures>>,
    /// The models whose pictures have been asked for, and those still to come.
    asked: Vec<bool>,
    queue: Vec<u32>,
    trees: Option<Trees>,
    cover: Option<Cover>,
    stats: ForestStats,
}

/// The bytes of a model's row with `pictures` on the device, or none.
fn model_row(m: &crate::scene::FarPackModel, pictures: Option<&Pictures>) -> Vec<u8> {
    let mut out = Vec::with_capacity(MODEL_BYTES);
    let (centre, radius) = m.bounds;
    for f in [
        centre.x, centre.y, centre.z, radius, m.quad[0], m.quad[1], m.quad[2], m.cut, m.color[0],
        m.color[1], m.color[2], m.color[3],
    ] {
        out.extend_from_slice(&f.to_le_bytes());
    }
    let (base, normal) = pictures.map_or((NO_PICTURES, NO_PICTURES), |p| (p.base.1, p.normal.1));
    for w in [base, normal, m.views[0], m.views[1]] {
        out.extend_from_slice(&w.to_le_bytes());
    }
    out
}

impl ForestPass {
    /// # Errors
    /// What the layer refuses.
    pub fn new(gfx: &mut Device) -> Result<Self, dereth_gfx::Error> {
        let targets = [ALBEDO, NORMAL, LIGHT, MATERIAL].map(|format| ColorTarget {
            format,
            blend: None,
            write_alpha: true,
        });
        let draw = gfx.create_graphics_pipeline(&GraphicsDesc {
            label: "remaster far trees",
            vertex: shader(Entry::ForestVsFar),
            fragment: Some(shader(Entry::ForestFsFar)),
            color: &targets,
            depth: Some(DepthState {
                format: DEPTH,
                compare: Some(Compare::LessEqual),
                write: true,
            }),
            cull: Cull::None,
        })?;
        let cull = gfx.create_compute_pipeline(&ComputeDesc {
            label: "remaster far trees cull",
            shader: shader(Entry::ForestCsCull),
        })?;
        let constants = bound(
            gfx,
            "remaster far trees constants",
            CONSTANT_BYTES as u64,
            BufferUsage::STORAGE,
        )?;
        let args = bound(
            gfx,
            "remaster far trees draw",
            20,
            BufferUsage::STORAGE | BufferUsage::INDIRECT,
        )?;
        Ok(Self {
            cull,
            draw,
            constants,
            args,
            pack: None,
            models: None,
            place: HashMap::new(),
            pictures: Vec::new(),
            asked: Vec::new(),
            queue: Vec::new(),
            trees: None,
            cover: None,
            stats: ForestStats::default(),
        })
    }

    /// What the far forest did.
    #[must_use]
    pub fn stats(&self) -> ForestStats {
        self.stats
    }

    /// The forest cover for the far ground this frame: on the device once, when it is made.
    ///
    /// # Errors
    /// What the layer refuses.
    pub fn cover_rows(
        &mut self,
        gfx: &mut Device,
        frame: &FrameDelta,
    ) -> Result<CoverRows, dereth_gfx::Error> {
        self.stats.cover = false;
        let Some((forest, cover)) = frame
            .forest
            .as_ref()
            .and_then(|f| f.cover.as_ref().map(|c| (f, c)))
        else {
            return Ok(CoverRows::default());
        };
        if self.cover.is_none_or(|c| c.serial != cover.serial) {
            self.hold_cover(gfx, cover)?;
        }
        let Some(held) = self.cover else {
            return Ok(CoverRows::default());
        };
        self.stats.cover = true;
        let k = &forest.knobs;
        let (from, to) = cover_band(k);
        let meters = if k.cover > 0.0 {
            k.canopy * 255.0 * CANOPY_STEP
        } else {
            0.0
        };
        Ok(CoverRows {
            tint: [k.cover, from, to, FLOOR_SHADE * k.cover.min(1.0)],
            canopy: [meters, from, to, 0.0],
            pictures: [held.color.1, held.canopy.1],
        })
    }

    fn hold_cover(
        &mut self,
        gfx: &mut Device,
        cover: &ForestCover,
    ) -> Result<(), dereth_gfx::Error> {
        let mut picture =
            |label, format, bytes: &[u8]| -> Result<(Texture, u32), dereth_gfx::Error> {
                let t = gfx.create_texture(&TextureDesc {
                    label,
                    width: cover.side,
                    height: cover.side,
                    mips: 1,
                    format,
                    usage: TextureUsage::SAMPLED,
                })?;
                gfx.upload_texture(t, 0, bytes)?;
                Ok((t, gfx.bind_texture(t)?))
            };
        let color = picture("remaster forest cover", Format::Rgba8Unorm, &cover.color)?;
        let canopy = picture("remaster forest canopy", Format::R8Unorm, &cover.canopy)?;
        if let Some(old) = self.cover.replace(Cover {
            serial: cover.serial,
            color,
            canopy,
        }) {
            gfx.destroy_texture(old.color.0);
            gfx.destroy_texture(old.canopy.0);
        }
        Ok(())
    }

    /// Take `pack` as the pack drawn: the buffer its models are read from, and their pictures,
    /// which come a few a frame.
    fn hold_pack(
        &mut self,
        gfx: &mut Device,
        pack: &Arc<FarPack>,
    ) -> Result<(), dereth_gfx::Error> {
        for p in self.pictures.drain(..).flatten() {
            gfx.destroy_texture(p.base.0);
            gfx.destroy_texture(p.normal.0);
        }
        if let Some(b) = self.models.take() {
            gfx.destroy_buffer(b.buffer);
        }
        if let Some(t) = self.trees.take() {
            t.destroy(gfx);
        }
        let mut bytes = Vec::with_capacity(pack.models.len() * MODEL_BYTES);
        for m in &pack.models {
            bytes.extend(model_row(m, None));
        }
        let models = bound(
            gfx,
            "remaster far tree models",
            bytes.len() as u64,
            BufferUsage::STORAGE,
        )?;
        if !bytes.is_empty() {
            gfx.write_buffer(models.buffer, 0, &bytes)?;
        }
        self.models = Some(models);
        self.place = (0u32..).zip(&pack.models).map(|(k, m)| (m.id, k)).collect();
        self.pictures = vec![None; pack.models.len()];
        self.asked = vec![false; pack.models.len()];
        self.queue.clear();
        self.pack = Some(Arc::clone(pack));
        self.stats.pictures = 0;
        self.stats.picture_bytes = 0;
        Ok(())
    }

    /// The next models' pictures to the device, those of the models whose trees came first.
    fn send_pictures(&mut self, gfx: &mut Device) -> Result<(), dereth_gfx::Error> {
        let (Some(pack), Some(models)) = (self.pack.clone(), self.models) else {
            return Ok(());
        };
        let mut sent = 0;
        let take = self.queue.len().min(PICTURES_A_FRAME);
        for m in self.queue.drain(..take).collect::<Vec<u32>>() {
            let Some([base, normal]) = pack.pictures.pictures(m as usize) else {
                continue;
            };
            let mut picture =
                |t: &crate::assets::Texture| -> Result<(Texture, u32), dereth_gfx::Error> {
                    let tex = gfx.create_texture(&TextureDesc {
                        label: "remaster far tree picture",
                        width: t.width,
                        height: t.height,
                        mips: u32::try_from(t.levels.len()).unwrap_or(1).max(1),
                        format: gpu_format(t.format),
                        usage: TextureUsage::SAMPLED,
                    })?;
                    for (k, level) in t.levels.iter().enumerate() {
                        gfx.upload_texture(tex, u32::try_from(k).unwrap_or(0), level)?;
                    }
                    Ok((tex, gfx.bind_texture(tex)?))
                };
            let on = Pictures {
                base: picture(&base)?,
                normal: picture(&normal)?,
            };
            for t in [&base, &normal] {
                self.stats.picture_bytes += t.levels.iter().map(|l| l.len() as u64).sum::<u64>();
            }
            let row = model_row(&pack.models[m as usize], Some(&on));
            gfx.write_buffer(models.buffer, u64::from(m) * MODEL_BYTES as u64, &row)?;
            self.pictures[m as usize] = Some(on);
            sent += 1;
        }
        self.stats.pictures += sent;
        Ok(())
    }

    /// The far trees for this frame: the set on the device, and its constants; nothing while the
    /// frame carries none.
    ///
    /// # Errors
    /// What the layer refuses.
    pub fn prepare(
        &mut self,
        gfx: &mut Device,
        i: &ForestInputs<'_>,
    ) -> Result<Option<ForestPlan>, dereth_gfx::Error> {
        let drawn_trees = i
            .frame
            .forest
            .as_ref()
            .filter(|f| f.knobs.trees && f.knobs.reach > 0.0);
        let (Some(forest), Some(ground)) = (drawn_trees, i.frame.drawn.as_ref()) else {
            if let Some(t) = self.trees.take() {
                t.destroy(gfx);
            }
            self.stats.standing = 0;
            self.stats.bytes = 0;
            return Ok(None);
        };
        if self.pack.as_ref().is_none_or(|p| **p != *forest.pack) {
            self.hold_pack(gfx, &forest.pack)?;
        }
        self.stats.models = u32::try_from(forest.pack.models.len()).unwrap_or(u32::MAX);
        self.send_pictures(gfx)?;
        let set = &forest.trees;
        if self.trees.as_ref().is_none_or(|t| t.serial != set.serial) {
            let place = &self.place;
            // The models of the trees written are the models whose pictures are wanted.
            let asked = std::cell::RefCell::new((&mut self.asked, &mut self.queue));
            let of = |id: u32| {
                let m = place.get(&id).copied()?;
                let (asked, queue) = &mut *asked.borrow_mut();
                if !std::mem::replace(&mut asked[m as usize], true) {
                    queue.push(m);
                }
                Some(m)
            };
            let taken = match self.trees.as_mut() {
                Some(t) => t.take(gfx, set, &of)?,
                None => false,
            };
            if !taken {
                if let Some(t) = self.trees.take() {
                    t.destroy(gfx);
                }
                // Room for half as many again as the set, so the blocks coming within reach fit
                // however the room the blocks leaving free is cut up, and a thicker forest ahead
                // fits for a good while before the buffers are made again.
                let n: usize = set
                    .blocks
                    .iter()
                    .map(|(_, t)| t.iter().filter(|t| of(t.id).is_some()).count())
                    .sum();
                let room = u32::try_from(n).unwrap_or(u32::MAX);
                let room = room.saturating_add(room / 2).max(4096);
                let mut made = Trees::new(gfx, room)?;
                made.take(gfx, set, &of)?;
                self.trees = Some(made);
            }
        }
        let (Some(trees), Some(models)) = (self.trees.as_mut(), self.models) else {
            return Ok(None);
        };
        // The blocks whose own objects stand in the world draw their trees themselves.
        let scene = i.scene;
        let resident = |b: BlockId| scene.blocks.get(&b).is_some_and(|g| !g.parts.is_empty());
        let (bits, standing) = trees.held.left_out(&resident);
        if bits != trees.left_out {
            let bytes: Vec<u8> = bits.iter().flat_map(|w| w.to_le_bytes()).collect();
            gfx.write_buffer(trees.excluded.buffer, 0, &bytes)?;
            trees.left_out = bits;
        }
        self.stats.standing = standing;
        self.stats.bytes = trees.bytes
            + if self.cover.is_some() {
                u64::from(forest.cover.as_ref().map_or(0, |c| c.side * c.side * 5))
            } else {
                0
            };
        let k = &forest.knobs;
        gfx.write_buffer(
            self.constants.buffer,
            0,
            &constants(i.clip, i.eye, ground.global, k, i.pixels, trees.held.count),
        )?;
        // The draw's command, its instance count counted by the culling.
        let words = gfx.backend().command_words();
        let command = gfx.backend().command(6, 0, 0, 0);
        gfx.write_buffer(self.args.buffer, 0, &push_words(&command))?;
        Ok(Some(ForestPlan {
            cull: self.cull,
            draw: self.draw,
            constants: self.constants,
            models,
            trees: trees.trees,
            kept: trees.kept,
            excluded: trees.excluded,
            args: self.args,
            stride: words * 4,
            count: trees.held.count,
            frame: i.constants,
            clamp: i.clamp,
            weather: i.weather,
        }))
    }

    /// The culling, recorded before the world is drawn.
    ///
    /// # Errors
    /// What the layer refuses.
    pub fn record_cull(plan: &ForestPlan, r: &mut Recorder<'_>) -> Result<(), dereth_gfx::Error> {
        r.timestamp("far trees cull");
        r.compute(
            &[
                Touch::Buffer(plan.kept.buffer, Use::StorageWrite),
                Touch::Buffer(plan.args.buffer, Use::StorageWrite),
            ],
            |c| {
                c.set_pipeline(plan.cull)?;
                c.push(
                    0,
                    &push_words(&[
                        plan.constants.index,
                        plan.trees.index,
                        plan.models.index,
                        plan.excluded.index,
                        plan.kept.index,
                        plan.args.index,
                        plan.stride - 12,
                    ]),
                )?;
                c.dispatch(plan.count.div_ceil(GROUP).max(1), 1, 1);
                Ok(())
            },
        )
    }

    /// The trees drawn into the G-buffer.
    ///
    /// # Errors
    /// What the layer refuses.
    pub fn record_draw(
        plan: &ForestPlan,
        r: &mut Recorder<'_>,
        t: &Targets,
    ) -> Result<(), dereth_gfx::Error> {
        r.timestamp("far trees");
        let keep = [t.albedo, t.normal, t.light, t.material].map(|texture| ColorAttachment {
            texture,
            load: Load::Keep,
        });
        let mut touches = vec![
            Touch::Buffer(plan.kept.buffer, Use::StorageRead),
            Touch::Buffer(plan.args.buffer, Use::Indirect),
        ];
        if let Some(cover) = plan.weather.1 {
            touches.push(Touch::Texture(cover, Use::Sampled));
        }
        r.raster(
            &RasterPass {
                name: "far trees",
                color: &keep,
                depth: Some(DepthAttachment {
                    texture: t.depth,
                    clear: None,
                    write: true,
                }),
                touches: &touches,
                area: Some(Rect {
                    x: 0,
                    y: 0,
                    width: t.width,
                    height: t.height,
                }),
            },
            |d| {
                d.set_pipeline(plan.draw)?;
                d.push(
                    0,
                    &push_words(&[
                        plan.frame,
                        plan.constants.index,
                        plan.trees.index,
                        plan.kept.index,
                        plan.models.index,
                        plan.clamp,
                        plan.weather.0,
                    ]),
                )?;
                d.draw_indirect(plan.args.buffer, 0, 1, plan.stride)?;
                Ok(())
            },
        )
    }
}

/// The constants' bytes, as the shader's `C_*` lay them out.
fn constants(
    clip: Mat4,
    eye: Vec3,
    global: Vec2,
    k: &FarKnobs,
    pixels: f32,
    count: u32,
) -> Vec<u8> {
    let mut c: Vec<f32> = Vec::with_capacity(CONSTANT_BYTES / 4);
    for p in super::shadow::frustum(clip) {
        c.extend(p.to_array());
    }
    #[allow(clippy::cast_precision_loss)] // a tree count, far inside f32's exact range
    c.extend([eye.x, eye.y, eye.z, count as f32]);
    let reach = k.reach.max(0.0);
    c.extend([global.x, global.y, reach, k.thin_from.min(reach)]);
    c.extend([k.widen, FADE, pixels, LEAST_PIXELS]);
    c.extend([SINK, k.soften.max(0.0), 0.0, 0.0]);
    c.iter().flat_map(|f| f.to_le_bytes()).collect()
}

/// The pixels a meter of height covers at a meter's distance for the projection `projection` on
/// a viewport `height` pixels tall.
#[must_use]
pub fn pixels_a_meter(projection: Mat4, height: u32) -> f32 {
    #[allow(clippy::cast_precision_loss)] // a viewport's height
    let h = height as f32;
    projection.y_axis.y.abs() * h * 0.5
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental hi-fi art packs)
    use super::*;

    /// `n` trees of the retail object `id`.
    fn trees(n: usize, id: u32) -> Arc<Vec<FarTree>> {
        Arc::new(
            (0..n)
                .map(|k| FarTree {
                    id,
                    seed: u32::try_from(k).unwrap_or(0),
                    scale: 1.0,
                    ..FarTree::default()
                })
                .collect(),
        )
    }

    fn set(serial: u64, blocks: &[(u16, &Arc<Vec<FarTree>>)]) -> FarTreeSet {
        FarTreeSet {
            serial,
            blocks: blocks.iter().map(|(b, t)| (*b, Arc::clone(t))).collect(),
        }
    }

    /// Retail object 1 is model 0, 2 is model 1, anything else has none.
    fn of(id: u32) -> Option<u32> {
        match id {
            1 => Some(0),
            2 => Some(1),
            _ => None,
        }
    }

    /// Each write's first tree and the slot each tree it writes names.
    fn written(writes: &[(u32, Vec<u8>)]) -> Vec<(u32, Vec<u32>)> {
        writes
            .iter()
            .map(|(first, bytes)| {
                let slots = bytes
                    .chunks(TREE_BYTES)
                    .map(|t| u32::from_le_bytes([t[40], t[41], t[42], t[43]]))
                    .collect();
                (*first, slots)
            })
            .collect()
    }

    #[test]
    fn a_block_that_stays_within_reach_is_written_once_and_one_coming_in_where_one_left() {
        let (a, b, c) = (trees(3, 1), trees(2, 2), trees(4, 1));
        let mut held = Slots::new(100);
        let writes = held
            .take(&set(1, &[(10, &a), (11, &b)]), &of)
            .expect("room");
        assert_eq!(written(&writes), [(0, vec![1, 1, 1]), (3, vec![2, 2])]);
        let writes = held
            .take(&set(2, &[(10, &a), (12, &c)]), &of)
            .expect("room");
        assert_eq!(
            written(&writes),
            [(3, vec![0, 0]), (3, vec![2, 2, 2, 2])],
            "only the new block"
        );
        let (bits, standing) = held.left_out(&|_| false);
        assert_eq!((bits[0], standing), (0b001, 7));
        let (bits, standing) = held.left_out(&|b| b == 10);
        assert_eq!((bits[0], standing), (0b011, 4));
    }

    #[test]
    fn trees_without_a_model_are_not_written() {
        let none = trees(4, 9);
        let one = trees(2, 1);
        let mut held = Slots::new(100);
        let writes = held
            .take(&set(1, &[(10, &one), (11, &none)]), &of)
            .expect("room");
        assert_eq!(written(&writes), [(0, vec![1, 1])]);
    }

    #[test]
    fn the_buffers_are_made_again_only_when_a_block_fits_nowhere() {
        let (a, b) = (trees(60, 1), trees(60, 1));
        let mut full = Slots::new(100);
        full.take(&set(1, &[(10, &a)]), &of).expect("room");
        assert!(full.take(&set(2, &[(10, &a), (11, &b)]), &of).is_none());
        let (big, small) = (trees(5000, 1), trees(10, 1));
        let mut gone = Slots::new(20_000);
        gone.take(&set(1, &[(10, &big)]), &of).expect("room");
        assert!(gone.take(&set(2, &[(11, &small)]), &of).is_some());
    }

    #[test]
    fn the_forest_cover_comes_in_over_the_thinning_while_trees_are_drawn() {
        let on = FarKnobs {
            trees: true,
            reach: 6000.0,
            thin_from: 3000.0,
            ..FarKnobs::default()
        };
        assert_eq!(cover_band(&on), (3000.0, 6000.0));
        let off = FarKnobs::default();
        assert_eq!(cover_band(&off), (COVER_FROM, COVER_TO));
    }
}
