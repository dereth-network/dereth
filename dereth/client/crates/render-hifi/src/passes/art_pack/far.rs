//! The far trees on the device: every tree of the set the scene hands over, culled on the device
//! each frame and drawn model by model as its far pictures, the count of each model's draw
//! written by the culling.

use glam::{Mat4, Vec3};

use std::ops::Range;
use std::sync::Arc;

use crate::snapshot::{HifiFarKnobs, HifiFarTree, HifiFarTrees};

/// The far trees' culling shader.
pub(super) const FAR_WGSL: &str = include_str!("far.wgsl");

/// The bytes of one tree: its foot and scale, its rotation, and its model, number and block.
const TREE_BYTES: usize = 48;
/// The bytes of one model's far pictures: its bounds and where its trees' list starts.
const MODEL_BYTES: usize = 32;
/// The bytes of one model's draw: five numbers.
const DRAW_BYTES: u64 = 20;
/// The words of the excluded blocks' bits: a bit a block.
const EXCLUDED_WORDS: usize = 65536 / 32;
/// The bytes of the culling's constants.
const CULL_BYTES: usize = 16 * 9;
/// The trees the culling looks at in a group.
const GROUP: u32 = 64;
/// How far a far tree is sunk into the ground, meters a meter of its distance from the eye:
/// the far ground's mesh is coarser the farther it lies, and a little below its lowest point a
/// tree never floats over it.
pub(super) const SINK: f32 = 0.0015;
/// How far a tree's fade stretches as the thinning passes it, a share of the thinning.
const FADE: f32 = 0.1;
/// The least height on screen a far tree is drawn at, pixels: smaller, it fades out, and the
/// forest cover takes its place.
const LEAST_PIXELS: f32 = 2.0;

/// The far trees' layouts and pipelines, made with the pack's.
#[derive(Debug)]
pub(super) struct FarPipelines {
    pub(super) cull_layout: wgpu::BindGroupLayout,
    pub(super) draw_layout: wgpu::BindGroupLayout,
    pub(super) cull: wgpu::ComputePipeline,
}

impl FarPipelines {
    pub(super) fn new(device: &wgpu::Device) -> Self {
        let storage = |binding: u32, read_only: bool, visibility: wgpu::ShaderStages| {
            wgpu::BindGroupLayoutEntry {
                binding,
                visibility,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }
        };
        let uniform = |binding: u32, visibility: wgpu::ShaderStages| wgpu::BindGroupLayoutEntry {
            binding,
            visibility,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let c = wgpu::ShaderStages::COMPUTE;
        let cull_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("far trees cull"),
            entries: &[
                uniform(0, c),
                storage(1, true, c),
                storage(2, true, c),
                storage(3, true, c),
                storage(4, false, c),
                storage(5, false, c),
            ],
        });
        let v = wgpu::ShaderStages::VERTEX;
        let draw_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("far trees draw"),
            entries: &[
                storage(0, true, v),
                storage(1, true, v),
                storage(2, true, v),
                uniform(3, v),
            ],
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("far trees cull"),
            source: wgpu::ShaderSource::Wgsl(FAR_WGSL.into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("far trees cull"),
            bind_group_layouts: &[Some(&cull_layout)],
            immediate_size: 0,
        });
        let cull = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("far trees cull"),
            layout: Some(&layout),
            module: &module,
            entry_point: Some("cs_cull"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });
        Self {
            cull_layout,
            draw_layout,
            cull,
        }
    }
}

/// One model's far pictures as the far trees draw them: the pack's model, the pictures level's
/// part, and its bounds.
#[derive(Debug, Clone, Copy)]
pub(super) struct FarModel {
    /// The pack's model.
    pub(super) model: usize,
    /// Its pictures level.
    pub(super) lod: usize,
    /// The centre of its bounds in its own space, and their radius.
    pub(super) bounds: (Vec3, f32),
}

/// One block's trees in the buffers: its landblock, the list they were written from, where they
/// lie, and whether a block holds the slot.
#[derive(Debug)]
struct Slot {
    block: u16,
    from: Arc<Vec<HifiFarTree>>,
    range: Range<u32>,
    live: bool,
}

/// What the far trees' buffers hold, block by block. A block that leaves the set has its trees
/// written over with nought, trees of the first slot (never a block's, always left out by the
/// culling), and its room and its slot are free; a block new to the set is written into the
/// least free room it fits, or after the rest. Walking on writes a ring of blocks, not every tree
/// within reach.
#[derive(Debug)]
pub(super) struct Slots {
    /// How many trees there is room for, and how far they are written: no tree past it.
    capacity: u32,
    count: u32,
    slots: Vec<Slot>,
    /// The slots no block holds.
    spare: Vec<usize>,
    /// The free room before `count`, each start to its end, none touching another.
    free: std::collections::BTreeMap<u32, u32>,
    /// Each model's trees held: the room its list of kept trees needs.
    per_model: Vec<u32>,
}

impl Slots {
    /// Room for `capacity` trees of `models` models.
    #[must_use]
    pub(super) fn new(capacity: u32, models: usize) -> Self {
        Self {
            capacity,
            count: 0,
            slots: vec![Slot {
                block: u16::MAX,
                from: Arc::new(Vec::new()),
                range: 0..0,
                live: false,
            }],
            spare: Vec::new(),
            free: std::collections::BTreeMap::new(),
            per_model: vec![0; models],
        }
    }

    /// Bring what is held to `set` (a tree's model by `of`, its retail object's place among the
    /// models): blocks it no longer holds, or holds as another list, are written over and freed,
    /// and its blocks new here are written. The writes, each the tree it starts at and its
    /// bytes, in order; `None` when a block fits nowhere: the buffers are then made again,
    /// holding only the set.
    pub(super) fn take(
        &mut self,
        set: &HifiFarTrees,
        of: &dyn Fn(u32) -> Option<usize>,
    ) -> Option<Vec<(u32, Vec<u8>)>> {
        let wanted: std::collections::HashMap<u16, &Arc<Vec<HifiFarTree>>> =
            set.blocks.iter().map(|(b, t)| (*b, t)).collect();
        let mut held = std::collections::HashSet::new();
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
            for m in slot.from.iter().filter_map(|t| of(t.id)) {
                self.per_model[m] -= 1;
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
            let models: Vec<usize> = list.iter().filter_map(|t| of(t.id)).collect();
            if models.is_empty() {
                continue;
            }
            let n = u32::try_from(models.len()).ok()?;
            let at = self.room(n)?;
            let index = match self.spare.pop() {
                Some(k) => k,
                None if self.slots.len() < SLOTS => {
                    self.slots.push(Slot {
                        block: u16::MAX,
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
                self.per_model[m] += 1;
                for f in [t.at.x, t.at.y, t.at.z, t.scale] {
                    bytes.extend_from_slice(&f.to_le_bytes());
                }
                for f in t.rotation.to_array() {
                    bytes.extend_from_slice(&f.to_le_bytes());
                }
                for word in [u32::try_from(m).unwrap_or(0), t.seed, slot_word, 0] {
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

    /// Where each model's list of kept trees starts, each with room for every tree of it held.
    #[must_use]
    pub(super) fn bases(&self) -> Vec<u32> {
        self.per_model
            .iter()
            .scan(0u32, |at, n| {
                let base = *at;
                *at += n;
                Some(base)
            })
            .collect()
    }

    /// The slots the culling leaves out, a bit each: no block's, or of a block in `resident`
    /// (whose own objects stand in the world); and the trees standing in the rest.
    #[must_use]
    pub(super) fn left_out(&self, resident: &std::collections::HashSet<u16>) -> (Vec<u32>, u32) {
        let mut bits = vec![0u32; EXCLUDED_WORDS];
        let mut standing = 0;
        for (k, slot) in self.slots.iter().enumerate() {
            if !slot.live || resident.contains(&slot.block) {
                bits[k / 32] |= 1 << (k % 32);
            } else {
                standing += slot.range.end - slot.range.start;
            }
        }
        (bits, standing)
    }
}

/// The far trees' buffers on the device, kept as [`Slots`] says. They are made again, holding
/// only the blocks within reach, when a block fits nowhere.
#[derive(Debug)]
pub(super) struct FarGpu {
    /// The set's serial the buffers were last brought to.
    pub(super) serial: u64,
    /// What they hold.
    held: Slots,
    /// The models drawn, by their place in `draws`.
    pub(super) models: Vec<FarModel>,
    trees: wgpu::Buffer,
    model_buffer: wgpu::Buffer,
    cull_uniform: wgpu::Buffer,
    excluded: wgpu::Buffer,
    pub(super) draws: wgpu::Buffer,
    far_uniform: wgpu::Buffer,
    pub(super) cull_bind: wgpu::BindGroup,
    pub(super) draw_bind: wgpu::BindGroup,
    /// The slots left out last, a bit each, so they are written only when they change.
    left_out: Vec<u32>,
    /// The trees standing: written to blocks within reach and not left out.
    standing: u32,
    /// The bytes the buffers take on the device.
    pub(super) bytes: u64,
}

fn floats(values: &[f32]) -> Vec<u8> {
    values.iter().flat_map(|f| f.to_le_bytes()).collect()
}

/// The most slots the buffers hold, a bit each in the culling's list of those left out.
const SLOTS: usize = EXCLUDED_WORDS * 32;

impl FarGpu {
    /// Buffers with room for `capacity` trees, drawn with `models`.
    pub(super) fn new(
        device: &wgpu::Device,
        pipelines: &FarPipelines,
        models: Vec<FarModel>,
        capacity: u32,
    ) -> Self {
        let buffer = |label: &str, usage: wgpu::BufferUsages, size: u64| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: size.max(16).next_multiple_of(16),
                usage: usage | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let storage = wgpu::BufferUsages::STORAGE;
        let trees = buffer(
            "far trees",
            storage,
            u64::from(capacity) * TREE_BYTES as u64,
        );
        let model_buffer = buffer(
            "far tree models",
            storage,
            (models.len() * MODEL_BYTES) as u64,
        );
        let visible = buffer("far trees kept", storage, u64::from(capacity) * 8);
        let excluded = buffer("far trees left out", storage, (EXCLUDED_WORDS * 4) as u64);
        let draws = buffer(
            "far tree draws",
            storage | wgpu::BufferUsages::INDIRECT,
            DRAW_BYTES * models.len() as u64,
        );
        let cull_uniform = buffer(
            "far trees cull",
            wgpu::BufferUsages::UNIFORM,
            CULL_BYTES as u64,
        );
        let far_uniform = buffer("far trees frame", wgpu::BufferUsages::UNIFORM, 32);
        fn entry(binding: u32, b: &wgpu::Buffer) -> wgpu::BindGroupEntry<'_> {
            wgpu::BindGroupEntry {
                binding,
                resource: b.as_entire_binding(),
            }
        }
        let cull_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("far trees cull"),
            layout: &pipelines.cull_layout,
            entries: &[
                entry(0, &cull_uniform),
                entry(1, &trees),
                entry(2, &model_buffer),
                entry(3, &excluded),
                entry(4, &visible),
                entry(5, &draws),
            ],
        });
        let draw_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("far trees draw"),
            layout: &pipelines.draw_layout,
            entries: &[
                entry(0, &trees),
                entry(1, &visible),
                entry(2, &model_buffer),
                entry(3, &far_uniform),
            ],
        });
        let bytes = u64::from(capacity) * (TREE_BYTES as u64 + 8)
            + (models.len() * MODEL_BYTES) as u64
            + (EXCLUDED_WORDS * 4) as u64
            + DRAW_BYTES * models.len() as u64;
        Self {
            serial: 0,
            held: Slots::new(capacity, models.len()),
            models,
            trees,
            model_buffer,
            cull_uniform,
            excluded,
            draws,
            far_uniform,
            cull_bind,
            draw_bind,
            // Nothing written yet: every bit is written on the first frame.
            left_out: vec![u32::MAX; EXCLUDED_WORDS],
            standing: 0,
            bytes,
        }
    }

    /// The trees `set` holds that have a model (by `of`, a retail object's place among the
    /// models): the room buffers for it want.
    pub(super) fn room_for(set: &HifiFarTrees, of: &dyn Fn(u32) -> Option<usize>) -> u32 {
        let n: usize = set
            .blocks
            .iter()
            .map(|(_, t)| t.iter().filter(|t| of(t.id).is_some()).count())
            .sum();
        u32::try_from(n).unwrap_or(u32::MAX)
    }

    /// Bring the buffers to `set` (see [`Slots::take`]); `false` when they are to be made again.
    pub(super) fn take(
        &mut self,
        queue: &wgpu::Queue,
        set: &HifiFarTrees,
        of: &dyn Fn(u32) -> Option<usize>,
    ) -> bool {
        let Some(writes) = self.held.take(set, of) else {
            return false;
        };
        for (first, bytes) in writes {
            queue.write_buffer(&self.trees, u64::from(first) * TREE_BYTES as u64, &bytes);
        }
        let mut model_bytes = Vec::with_capacity(self.models.len() * MODEL_BYTES);
        for (m, base) in self.models.iter().zip(self.held.bases()) {
            model_bytes.extend(floats(&[
                m.bounds.0.x,
                m.bounds.0.y,
                m.bounds.0.z,
                m.bounds.1,
            ]));
            for word in [base, 0, 0, 0] {
                model_bytes.extend(word.to_le_bytes());
            }
        }
        queue.write_buffer(&self.model_buffer, 0, &model_bytes);
        self.serial = set.serial;
        true
    }

    /// This frame's constants: the view `clip_from_render`, the eye, where render space lies
    /// in the world, the knobs, the pixels a meter at a meter's distance, and the blocks whose
    /// own objects stand in the world (sorted); the draws' counts set to nought for the culling.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn prepare(
        &mut self,
        queue: &wgpu::Queue,
        clip_from_render: Mat4,
        eye: Vec3,
        global: glam::Vec2,
        knobs: &HifiFarKnobs,
        pixels: f32,
        left_out: &std::collections::HashSet<u16>,
        index_count: &dyn Fn(&FarModel) -> u32,
    ) {
        let mut c: Vec<f32> = Vec::with_capacity(CULL_BYTES / 4);
        for p in super::frustum(clip_from_render) {
            c.extend(p.to_array());
        }
        #[allow(clippy::cast_precision_loss)] // a tree count, far inside f32's exact range
        c.extend([eye.x, eye.y, eye.z, self.held.count as f32]);
        let reach = knobs.reach.max(0.0);
        c.extend([global.x, global.y, reach, knobs.thin_from.min(reach)]);
        c.extend([knobs.widen, FADE, pixels, LEAST_PIXELS]);
        queue.write_buffer(&self.cull_uniform, 0, &floats(&c));
        queue.write_buffer(
            &self.far_uniform,
            0,
            &floats(&[
                global.x,
                global.y,
                SINK,
                0.0,
                eye.x,
                eye.y,
                eye.z,
                knobs.soften.max(0.0),
            ]),
        );
        let mut draws = Vec::with_capacity(self.models.len() * 20);
        for m in &self.models {
            for word in [index_count(m), 0, 0, 0, 0] {
                draws.extend(word.to_le_bytes());
            }
        }
        queue.write_buffer(&self.draws, 0, &draws);
        // The slots left out: gone, or a block whose own objects stand in the world.
        let (bits, standing) = self.held.left_out(left_out);
        self.standing = standing;
        if bits != self.left_out {
            let bytes: Vec<u8> = bits.iter().flat_map(|w| w.to_le_bytes()).collect();
            queue.write_buffer(&self.excluded, 0, &bytes);
            self.left_out = bits;
        }
    }

    /// The trees standing: written to blocks within reach and not left out.
    pub(super) fn standing(&self) -> u32 {
        self.standing
    }

    /// The culling, recorded into `pass`.
    pub(super) fn cull(&self, pass: &mut wgpu::ComputePass<'_>, pipelines: &FarPipelines) {
        if self.held.count == 0 {
            return;
        }
        pass.set_pipeline(&pipelines.cull);
        pass.set_bind_group(0, &self.cull_bind, &[]);
        pass.dispatch_workgroups(self.held.count.div_ceil(GROUP), 1, 1);
    }

    /// Where model `k`'s draw is in the draws' buffer.
    pub(super) fn draw_offset(k: usize) -> u64 {
        DRAW_BYTES * k as u64
    }
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
    fn trees(n: usize, id: u32) -> Arc<Vec<HifiFarTree>> {
        Arc::new(
            (0..n)
                .map(|k| HifiFarTree {
                    id,
                    seed: u32::try_from(k).unwrap_or(0),
                    scale: 1.0,
                    ..HifiFarTree::default()
                })
                .collect(),
        )
    }

    fn set(serial: u64, blocks: &[(u16, &Arc<Vec<HifiFarTree>>)]) -> HifiFarTrees {
        HifiFarTrees {
            serial,
            blocks: blocks.iter().map(|(b, t)| (*b, Arc::clone(t))).collect(),
        }
    }

    /// Retail object 1 is model 0, 2 is model 1, anything else has none.
    fn of(id: u32) -> Option<usize> {
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

    /// The bits of `slots` set, the rest clear.
    fn bits(slots: &[usize]) -> Vec<u32> {
        let mut b = vec![0u32; EXCLUDED_WORDS];
        for k in slots {
            b[k / 32] |= 1 << (k % 32);
        }
        b
    }

    #[test]
    fn a_block_that_stays_within_reach_is_written_once_and_one_coming_in_after_the_rest() {
        let (a, b, c) = (trees(3, 1), trees(2, 2), trees(4, 1));
        let mut held = Slots::new(100, 2);
        let writes = held
            .take(&set(1, &[(10, &a), (11, &b)]), &of)
            .expect("room");
        assert_eq!(written(&writes), [(0, vec![1, 1, 1]), (3, vec![2, 2])]);
        // The eye moves on: block 10 stays, 11 leaves, 12 comes in. 11's trees are written over
        // with the first slot's, and 12 is written in its slot where 11's started: the room 11
        // left, last, is the room after the rest.
        let writes = held
            .take(&set(2, &[(10, &a), (12, &c)]), &of)
            .expect("room");
        assert_eq!(
            written(&writes),
            [(3, vec![0, 0]), (3, vec![2, 2, 2, 2])],
            "only the new block"
        );
        // The first slot is left out by the culling; block 10 and 12 stand, but 10's are left
        // out too while its own objects stand in the world.
        let (bits, standing) = held.left_out(&std::collections::HashSet::new());
        assert_eq!((bits[0], standing), (0b001, 7));
        let (bits, standing) = held.left_out(&[10].into_iter().collect());
        assert_eq!((bits[0], standing), (0b011, 4));
        // Each model's list has room for every tree of it held.
        assert_eq!(held.per_model, [7, 0]);
        assert_eq!(held.bases(), [0, 7]);
    }

    #[test]
    fn a_block_worked_out_again_or_coming_back_is_written_again_over_its_old_trees() {
        let (a, a2) = (trees(3, 1), trees(3, 1));
        let mut held = Slots::new(100, 2);
        held.take(&set(1, &[(10, &a)]), &of).expect("room");
        // The same block, worked out again (another list): its trees are written again, over
        // the old ones.
        let writes = held.take(&set(2, &[(10, &a2)]), &of).expect("room");
        assert_eq!(written(&writes), [(0, vec![0, 0, 0]), (0, vec![1, 1, 1])]);
        // It leaves: nothing is written past the trees held, and its slot is left out.
        held.take(&set(3, &[]), &of).expect("room");
        assert_eq!(held.count, 0);
        assert_eq!(
            held.left_out(&std::collections::HashSet::new()),
            (bits(&[0, 1]), 0)
        );
        // It comes back as the same list: written once more.
        let writes = held.take(&set(4, &[(10, &a2)]), &of).expect("room");
        assert_eq!(written(&writes), [(0, vec![1, 1, 1])]);
        let (bits, standing) = held.left_out(&std::collections::HashSet::new());
        assert_eq!((bits[0], standing), (0b01, 3));
    }

    #[test]
    fn trees_without_a_model_are_not_written() {
        let mixed = Arc::new(vec![
            HifiFarTree {
                id: 1,
                ..HifiFarTree::default()
            },
            HifiFarTree {
                id: 9,
                ..HifiFarTree::default()
            },
        ]);
        let none = trees(4, 9);
        let mut held = Slots::new(100, 2);
        let writes = held
            .take(&set(1, &[(10, &mixed), (11, &none)]), &of)
            .expect("room");
        assert_eq!(written(&writes), [(0, vec![1])]);
    }

    #[test]
    fn a_block_coming_in_is_written_into_the_least_free_room_it_fits() {
        let (big, kept, small, also) = (trees(5, 2), trees(2, 1), trees(3, 1), trees(2, 1));
        let mut held = Slots::new(100, 2);
        held.take(
            &set(1, &[(10, &big), (11, &kept), (12, &small), (13, &also)]),
            &of,
        )
        .expect("room");
        assert_eq!(held.per_model, [7, 5]);
        // 10 and 12 leave, and 14 comes in: it fits both their rooms, and takes 12's, the least.
        let new = trees(3, 1);
        let writes = held
            .take(&set(2, &[(11, &kept), (13, &also), (14, &new)]), &of)
            .expect("room");
        assert_eq!(
            written(&writes),
            [(0, vec![0; 5]), (7, vec![0; 3]), (7, vec![3, 3, 3])]
        );
        // 15 comes in, two trees, into 10's room, the rest of it left free.
        let more = trees(2, 1);
        let writes = held
            .take(
                &set(3, &[(11, &kept), (13, &also), (14, &new), (15, &more)]),
                &of,
            )
            .expect("room");
        assert_eq!(written(&writes), [(0, vec![1, 1])]);
        assert_eq!(held.count, 12, "nothing written after the trees held");
        // The lists have room for the trees held: none of model 1 now.
        assert_eq!(held.per_model, [9, 0]);
        assert_eq!(
            held.left_out(&std::collections::HashSet::new()),
            (bits(&[0]), 9)
        );
        // Room freed after free room is one room: 11 leaves, and a block of five fits where 10
        // and 11 stood.
        let five = trees(5, 1);
        let writes = held
            .take(
                &set(4, &[(13, &also), (14, &new), (15, &more), (16, &five)]),
                &of,
            )
            .expect("room");
        assert_eq!(written(&writes), [(5, vec![0, 0]), (2, vec![2; 5])]);
        // And room freed before free room: 14 leaves, then 16, and a block of eight fits where
        // both stood.
        held.take(&set(5, &[(13, &also), (15, &more), (16, &five)]), &of)
            .expect("room");
        let eight = trees(8, 1);
        let writes = held
            .take(&set(6, &[(13, &also), (15, &more), (17, &eight)]), &of)
            .expect("room");
        assert_eq!(written(&writes), [(2, vec![0; 5]), (2, vec![2; 8])]);
    }

    #[test]
    fn the_buffers_are_made_again_only_when_a_block_fits_nowhere() {
        let (a, b) = (trees(60, 1), trees(60, 1));
        let mut full = Slots::new(100, 2);
        full.take(&set(1, &[(10, &a)]), &of).expect("room");
        assert!(full.take(&set(2, &[(10, &a), (11, &b)]), &of).is_none());
        // However much leaves, while what comes in fits.
        let (big, small) = (trees(5000, 1), trees(10, 1));
        let mut gone = Slots::new(20_000, 2);
        gone.take(&set(1, &[(10, &big)]), &of).expect("room");
        assert!(gone.take(&set(2, &[(11, &small)]), &of).is_some());
    }
}
