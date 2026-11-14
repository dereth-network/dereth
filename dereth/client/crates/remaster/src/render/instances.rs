//! An art pack's placed models chosen and listed on the device (`shaders/instances.slang`): for
//! each, every frame, its level of detail and its fade into the next, whether it is seen, the sun's
//! cascades it casts into, and the commands that draw it. The processor writes a model's row once,
//! as its block comes, and each frame only the frame's own: the eye, the view, the groups' levels'
//! distances and which blocks the walk drew.
//!
//! **Which parts.** A block's part is chosen on the device when every level of it is a pack mesh
//! (or draws nothing), none of them turns toward the viewer, it is no tree drawn far off as an
//! impostor, and each of its items is drawn in the G-buffer's culled buckets. Everything else,
//! retail's own parts above all, is chosen on the processor as before ([`super::lod`]); the
//! processor's walks of a block take only its other parts ([`PackInstances::cpu_parts`]).
//!
//! **The same choice.** A model's level, fade and turn are the processor's for such a part: its
//! hand-overs at its own scale from its group's errors and the pack's distances, faded over the
//! same band, never turned, and its first level alone while its group's errors are not measured
//! or levels are not chosen. A model is seen where its block is drawn and its sphere stands in the
//! view, or in the mirrored view while the water's mirror draws.
//!
//! **The lists.** Each model's commands follow the processor's in the G-buffer's two culled
//! buckets, and its first level's go into each cascade it reaches as it is drawn again; ground
//! cover casts into the nearest cascade alone. Every list is in the models' order, frame after
//! frame.
//!
//! **Where the device takes no draw count from a buffer**, every part stays on the processor.

use std::collections::{BTreeMap, HashMap};
use std::ops::Range;

use dereth_gfx::{
    Buffer, BufferDesc, BufferUsage, ComputeDesc, Device, Draw, Location, Pipeline, Recorder,
    Touch, Use,
};
use glam::{Mat4, Vec3, Vec4};

use super::lod::{pixels_at_one_metre, Lod};
use super::pool::Pool;
use super::scene::{Bucket, GpuScene, PartItems, NONE};
use super::{push_words, shader, PerFrame};
use crate::assets::pack::of_pack_mesh;
use crate::assets::AssetTables;
use crate::scene::{BlockId, FrameDelta, SceneDelta};
use crate::shaders::Entry;

/// The most levels a model chosen on the device has.
pub const MAX_LEVELS: usize = 8;
/// The bytes of one model's row, of one group's, and of the frame's constants.
const ENTRY_BYTES: usize = 64;
const GROUP_BYTES: usize = 80;
const CONSTANT_ROWS: usize = 27;
/// The blocks' slots the drawn blocks' bits cover.
const SLOTS: u32 = 1 << 12;
/// A model's flags, as the shader reads them: ground cover; held.
const M_COVER: u32 = 1;
const M_LIVE: u32 = 2;
/// A model with no group draws its first level alone.
const NO_GROUP: u32 = u32::MAX;
/// The models a workgroup takes.
const GROUP: u32 = 64;
/// The models there is room for at first.
const FIRST_CAPACITY: u32 = 1 << 12;

/// A buffer and its index in the table.
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
        usage: usage | BufferUsage::STORAGE | BufferUsage::TRANSFER_SRC,
        location: Location::Gpu,
    })?;
    Ok(Bound {
        buffer,
        index: d.bind_buffer(buffer)?,
    })
}

/// One block's models: their rows' run, the parts left to the processor, and the parts the rows
/// stand for, in the rows' order.
#[derive(Debug)]
struct Held {
    range: Range<u32>,
    cpu_parts: Vec<u32>,
    parts: Vec<u32>,
    /// The items of the models' first levels, together: what they add to the cascades' lists.
    first_items: u64,
}

/// The cascades as the models are cast into them this frame.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PackCascades {
    /// From the frame's space to the sun's.
    pub light_view: Mat4,
    /// Each cascade's box across the sun, and along it.
    pub across: [[f32; 4]; 4],
    pub along: [[f32; 2]; 4],
    /// A bit for each cascade drawn again this frame.
    pub redraw: u32,
    /// The casters are only those in the view.
    pub in_view: bool,
}

/// What the device's command lists the models go into are, this frame.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PackTargets {
    /// Each block's offset by its slot, the instances and the draw items, by buffer and index.
    pub offsets: (Buffer, u32),
    pub instances: (Buffer, u32),
    pub items: (Buffer, u32),
    /// The G-buffer's commands and their counts, `capacity` commands a bucket, and how many of
    /// each of its two culled buckets the processor's own take.
    pub commands: (Buffer, u32),
    pub counts: (Buffer, u32),
    pub capacity: u32,
    pub bases: [u32; 2],
}

/// What one frame asks of the models.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PackAsk<'a> {
    pub frame: &'a FrameDelta,
    /// The frame drawn this many pixels tall, as the levels are chosen for it.
    pub height: u32,
    /// The unjittered view and projection, and the mirrored view's while the water's mirror draws.
    pub clip: Mat4,
    pub mirror: Option<Mat4>,
    pub cascades: Option<PackCascades>,
    pub targets: PackTargets,
}

/// The models' frame on the device.
#[derive(Debug, Clone)]
pub struct PackPlan {
    pipelines: [Pipeline; 3],
    push: [u32; 17],
    count: u32,
    reads: Vec<Buffer>,
    writes: [Buffer; 3],
    instances: Buffer,
    commands: Buffer,
    counts: Buffer,
    casters: Buffer,
    caster_counts: Buffer,
    caster_capacity: u32,
    command_bytes: u32,
    redraw: u32,
}

/// One model's choice as the device made it, for the checks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PackState {
    /// The model's block and its place among the block's parts.
    pub block: BlockId,
    pub part: u32,
    /// The level it draws, the level it fades into (its own when not fading) and how far.
    pub level: u8,
    pub next: u8,
    pub fade: f32,
    /// Whether it is seen, and a bit for each cascade it casts into.
    pub seen: bool,
    pub casts: u8,
}

/// The models chosen on the device.
#[derive(Debug)]
pub struct PackInstances {
    /// The passes; `None` where the device takes no draw count from a buffer.
    pipelines: Option<[Pipeline; 3]>,
    /// Whether the models are chosen on the device; off, every part is the processor's.
    on: bool,
    /// Every block's rows to be written again at the next change.
    rebuild: bool,
    pool: Pool,
    /// The rows, as on the device.
    rows: Vec<u8>,
    table: Option<Bound>,
    tallies: Option<Bound>,
    sums: Option<Bound>,
    states: Option<Bound>,
    casters: Option<Bound>,
    caster_counts: Option<Bound>,
    caster_capacity: u32,
    caster_items: u64,
    // ORDER-OK: looked up by block, and walked in block order.
    blocks: BTreeMap<BlockId, Held>,
    /// The groups the models choose their levels by, by their place in the frame's table.
    groups: Vec<u32>,
    // ORDER-OK: only looked up.
    group_slot: HashMap<u32, u32>,
    constants: PerFrame,
    group_rows: PerFrame,
    drawn: PerFrame,
    /// The rows written so far: none past it is held.
    high: u32,
    /// Counts the changes to which parts are the processor's ([`Self::cpu_parts`]).
    generation: u64,
}

impl PackInstances {
    /// The models' passes on `d`, made where it takes a draw count from a buffer.
    ///
    /// # Errors
    /// What the device refuses.
    pub fn new(d: &mut Device) -> Result<Self, dereth_gfx::Error> {
        let pipelines = if d.caps().indirect_count {
            let mut make = |label, e| {
                d.create_compute_pipeline(&ComputeDesc {
                    label,
                    shader: shader(e),
                })
            };
            Some([
                make("remaster pack choose", Entry::InstancesCsChoose)?,
                make("remaster pack scan", Entry::InstancesCsScan)?,
                make("remaster pack write", Entry::InstancesCsWrite)?,
            ])
        } else {
            None
        };
        Ok(Self {
            pipelines,
            on: true,
            rebuild: false,
            pool: Pool::new(0),
            rows: Vec::new(),
            table: None,
            tallies: None,
            sums: None,
            states: None,
            casters: None,
            caster_counts: None,
            caster_capacity: 0,
            caster_items: 0,
            blocks: BTreeMap::new(),
            groups: Vec::new(),
            group_slot: HashMap::new(),
            constants: PerFrame::new(
                d,
                "remaster pack constants",
                (CONSTANT_ROWS * 16) as u64,
                BufferUsage::default(),
            )?,
            group_rows: PerFrame::new(d, "remaster pack groups", 1 << 12, BufferUsage::default())?,
            drawn: PerFrame::new(
                d,
                "remaster pack drawn blocks",
                u64::from(SLOTS / 8),
                BufferUsage::default(),
            )?,
            high: 0,
            generation: 0,
        })
    }

    /// Whether the models are chosen on the device, from the next change of the scene on.
    pub(crate) fn set_on(&mut self, on: bool) {
        if on != self.on {
            self.on = on;
            self.rebuild = true;
        }
    }

    /// Whether the models are chosen on the device: asked for, and the device can.
    #[must_use]
    pub fn on(&self) -> bool {
        self.on && self.pipelines.is_some()
    }

    /// The models chosen on the device.
    #[must_use]
    pub fn count(&self) -> usize {
        self.blocks.values().map(|h| h.parts.len()).sum()
    }

    /// Block `block`'s parts the processor chooses and lists, by their place among its parts;
    /// `None` when it chooses them all.
    #[must_use]
    pub fn cpu_parts(&self, block: BlockId) -> Option<&[u32]> {
        self.blocks.get(&block).map(|h| h.cpu_parts.as_slice())
    }

    /// A number that changes whenever which parts are the processor's ([`Self::cpu_parts`]) may
    /// have.
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Take the scene's changes: each block that came, or changed, has its models' rows written,
    /// and each that went has its rows let go.
    ///
    /// # Errors
    /// What the device refuses.
    pub(crate) fn take(
        &mut self,
        d: &mut Device,
        scene: &GpuScene,
        lod: &mut Lod,
        tables: &AssetTables,
        deltas: &[SceneDelta],
    ) -> Result<(), dereth_gfx::Error> {
        if std::mem::take(&mut self.rebuild) {
            let ids: Vec<BlockId> = scene.blocks.keys().copied().collect();
            for id in ids {
                self.block_in(d, scene, lod, tables, id)?;
            }
        }
        for delta in deltas {
            match delta {
                SceneDelta::BlockIn(b) | SceneDelta::BlockChanged(b) => {
                    self.block_in(d, scene, lod, tables, b.block)?;
                }
                SceneDelta::BlockOut(id) => self.block_out(d, *id)?,
                _ => {}
            }
        }
        Ok(())
    }

    /// Let block `id`'s rows go, written over as held by none.
    fn block_out(&mut self, d: &mut Device, id: BlockId) -> Result<(), dereth_gfx::Error> {
        let Some(h) = self.blocks.remove(&id) else {
            return Ok(());
        };
        self.generation += 1;
        let (a, b) = (h.range.start as usize, h.range.end as usize);
        self.rows[a * ENTRY_BYTES..b * ENTRY_BYTES].fill(0);
        if let Some(t) = self.table {
            d.write_buffer(
                t.buffer,
                (a * ENTRY_BYTES) as u64,
                &self.rows[a * ENTRY_BYTES..b * ENTRY_BYTES],
            )?;
        }
        self.caster_items -= h.first_items;
        self.pool.give(h.range);
        self.high = self.blocks.values().map(|h| h.range.end).max().unwrap_or(0);
        Ok(())
    }

    /// Write block `id`'s models' rows, as the scene now holds it.
    fn block_in(
        &mut self,
        d: &mut Device,
        scene: &GpuScene,
        lod: &mut Lod,
        tables: &AssetTables,
        id: BlockId,
    ) -> Result<(), dereth_gfx::Error> {
        self.block_out(d, id)?;
        lod.set_cpu_records(id, None);
        let Some(b) = scene.blocks.get(&id) else {
            return Ok(());
        };
        if !self.on() || b.slot == NONE || b.slot >= SLOTS {
            return Ok(());
        }
        let mut rows: Vec<u8> = Vec::new();
        let mut parts = Vec::new();
        let mut cpu_parts = Vec::new();
        let mut first_items = 0u64;
        for (k, p) in b.parts.iter().enumerate() {
            let k = u32::try_from(k).unwrap_or(u32::MAX);
            match self.row(scene, lod, tables, id, b.slot, p) {
                Some((row, first)) => {
                    rows.extend_from_slice(&row);
                    parts.push(k);
                    first_items += u64::from(first);
                }
                None => cpu_parts.push(k),
            }
        }
        if parts.is_empty() {
            return Ok(());
        }
        let n = u32::try_from(parts.len()).unwrap_or(u32::MAX);
        let range = self.room(d, n)?;
        let at = range.start as usize * ENTRY_BYTES;
        self.rows[at..at + rows.len()].copy_from_slice(&rows);
        if let Some(t) = self.table {
            d.write_buffer(t.buffer, at as u64, &rows)?;
        }
        // The records the device chooses for are never chosen on the processor.
        let gpu_records: std::collections::BTreeSet<u32> = parts
            .iter()
            .filter_map(|k| b.parts.get(*k as usize).and_then(|p| p.degrade))
            .collect();
        let records = u32::try_from(lod.records(id).len()).unwrap_or(0);
        lod.set_cpu_records(
            id,
            Some((0..records).filter(|r| !gpu_records.contains(r)).collect()),
        );
        self.caster_items += first_items;
        self.high = self.high.max(range.end);
        self.blocks.insert(
            id,
            Held {
                range,
                cpu_parts,
                parts,
                first_items,
            },
        );
        self.generation += 1;
        self.casters_fit(d)?;
        Ok(())
    }

    /// Room for `n` rows, the buffers grown when there is none.
    fn room(&mut self, d: &mut Device, n: u32) -> Result<Range<u32>, dereth_gfx::Error> {
        if let (Some(r), Some(_)) = (self.pool.take(n), self.table) {
            return Ok(r);
        }
        let old = self.pool.capacity();
        let capacity = (old * 2)
            .max(old + n)
            .max(FIRST_CAPACITY)
            .next_power_of_two();
        for b in [self.table, self.tallies, self.sums, self.states]
            .into_iter()
            .flatten()
        {
            d.destroy_buffer(b.buffer);
        }
        let entries = u64::from(capacity);
        self.table = Some(bound(
            d,
            "remaster pack models",
            entries * ENTRY_BYTES as u64,
            BufferUsage::default(),
        )?);
        self.tallies = Some(bound(
            d,
            "remaster pack tallies",
            entries * 16,
            BufferUsage::default(),
        )?);
        self.sums = Some(bound(
            d,
            "remaster pack sums",
            entries.div_ceil(u64::from(GROUP)) * 32,
            BufferUsage::default(),
        )?);
        self.states = Some(bound(
            d,
            "remaster pack states",
            entries * 16,
            BufferUsage::default(),
        )?);
        self.rows.resize(capacity as usize * ENTRY_BYTES, 0);
        if let Some(t) = self.table {
            d.write_buffer(t.buffer, 0, &self.rows)?;
        }
        self.pool.grow(capacity);
        self.pool
            .take(n)
            .ok_or_else(|| dereth_gfx::Error::Invalid("a pool that grew still has no room".into()))
    }

    /// Room in each cascade's list for every model's first level.
    fn casters_fit(&mut self, d: &mut Device) -> Result<(), dereth_gfx::Error> {
        if self.caster_counts.is_none() {
            self.caster_counts = Some(bound(
                d,
                "remaster pack caster counts",
                16,
                BufferUsage::INDIRECT,
            )?);
        }
        if self.caster_items <= u64::from(self.caster_capacity) && self.casters.is_some() {
            return Ok(());
        }
        let capacity =
            u32::try_from(self.caster_items.max(1024).next_power_of_two()).unwrap_or(u32::MAX);
        if let Some(c) = self.casters.take() {
            d.destroy_buffer(c.buffer);
        }
        self.casters = Some(bound(
            d,
            "remaster pack casters",
            4 * u64::from(capacity) * u64::from(super::command_bytes(d)),
            BufferUsage::INDIRECT,
        )?);
        self.caster_capacity = capacity;
        Ok(())
    }

    /// Part `p`'s row, and its first level's items, when the device chooses it.
    fn row(
        &mut self,
        scene: &GpuScene,
        lod: &mut Lod,
        tables: &AssetTables,
        block: BlockId,
        slot: u32,
        p: &PartItems,
    ) -> Option<([u8; ENTRY_BYTES], u32)> {
        if p.turned.is_some() || p.levels.is_empty() || p.levels.len() > MAX_LEVELS {
            return None;
        }
        let pack = |m: u32| of_pack_mesh(m).is_some();
        let (group, centre, scale) = match p.degrade {
            Some(r) => {
                let g = lod.group_of_record(block, r)?;
                let (levels, turns, _) = lod.group_info(g)?;
                let ours = !turns
                    && levels.len() == p.levels.len()
                    && levels.first().is_some_and(|m| pack(*m))
                    && levels.iter().all(|m| *m == 0 || pack(*m));
                if !ours || lod.group_tree(g, tables) != Some(false) {
                    return None;
                }
                let (centre, scale_z) = lod
                    .records(block)
                    .get(r as usize)
                    .map(|rec| (rec.centre, rec.scale_z))?;
                lod.want(g);
                let scale = if scale_z == 0.0 { 1.0 } else { scale_z.abs() };
                (self.group_slot_of(g), centre, scale)
            }
            None if pack(p.mesh) && p.levels.len() == 1 => (NO_GROUP, Vec3::ZERO, 1.0),
            None => return None,
        };
        // Each level's items, one run after another, each in the G-buffer's culled buckets.
        let mut counts = [0u8; MAX_LEVELS];
        for (k, r) in p.levels.iter().enumerate() {
            if k > 0 && p.levels[k - 1].end != r.start {
                return None;
            }
            counts[k] = u8::try_from(r.end - r.start).ok()?;
            for i in r.clone() {
                let culled = matches!(
                    scene.buckets.get(i as usize),
                    Some(Bucket::Opaque | Bucket::OpaqueTwoSided)
                );
                if !culled || scene.item_draws.get(i as usize).is_none_or(|d| d.edge) {
                    return None;
                }
            }
        }
        // Its sphere, block-local: wide enough to hold it however far it is drawn lifted.
        let t = p.transform;
        let reach = t
            .x_axis
            .truncate()
            .length()
            .max(t.y_axis.truncate().length())
            .max(t.z_axis.truncate().length());
        let sphere = t.transform_point3(Vec3::from(p.sphere.0));
        let radius = p.sphere.1 * reach + p.lifts[0].abs().max(p.lifts[1].abs());
        let mut out = [0u8; ENTRY_BYTES];
        let words = [
            p.instance,
            slot,
            group,
            p.levels[0].start,
            u32::from_le_bytes([counts[0], counts[1], counts[2], counts[3]]),
            u32::from_le_bytes([counts[4], counts[5], counts[6], counts[7]]),
            M_LIVE | if p.cover { M_COVER } else { 0 },
            u32::try_from(p.levels.len()).unwrap_or(0),
        ];
        let floats = [
            centre.x, centre.y, centre.z, scale, sphere.x, sphere.y, sphere.z, radius,
        ];
        for (k, w) in words.iter().enumerate() {
            out[k * 4..k * 4 + 4].copy_from_slice(&w.to_le_bytes());
        }
        for (k, f) in floats.iter().enumerate() {
            out[32 + k * 4..36 + k * 4].copy_from_slice(&f.to_le_bytes());
        }
        Some((out, u32::from(counts[0])))
    }

    /// Group `g`'s place in the frame's table of groups, given one when it has none.
    fn group_slot_of(&mut self, g: u32) -> u32 {
        if let Some(s) = self.group_slot.get(&g) {
            return *s;
        }
        let s = u32::try_from(self.groups.len()).unwrap_or(u32::MAX);
        self.groups.push(g);
        self.group_slot.insert(g, s);
        s
    }

    /// Write the frame's own state for the models: the groups' rows, the constants and the drawn
    /// blocks' bits. `None` when no model is held.
    ///
    /// # Errors
    /// What the device refuses.
    pub(crate) fn prepare(
        &mut self,
        d: &mut Device,
        scene: &GpuScene,
        lod: &Lod,
        distance_of: &dyn Fn(u32) -> Option<f32>,
        ask: &PackAsk<'_>,
    ) -> Result<Option<PackPlan>, dereth_gfx::Error> {
        let (Some(pipelines), Some(table), Some(tallies), Some(sums), Some(states)) = (
            self.pipelines,
            self.table,
            self.tallies,
            self.sums,
            self.states,
        ) else {
            return Ok(None);
        };
        let (Some(casters), Some(caster_counts)) = (self.casters, self.caster_counts) else {
            return Ok(None);
        };
        if self.high == 0 || !self.on() {
            return Ok(None);
        }
        let frame = ask.frame;
        let settings = lod.settings();
        // The groups: whether measured, then each level's error and the distance it hands over at.
        let mut groups = Vec::with_capacity(self.groups.len() * GROUP_BYTES);
        for g in &self.groups {
            let (levels, _, errors) = lod.group_info(*g).unwrap_or((&[], false, None));
            let n = levels.len().min(MAX_LEVELS);
            let mut row = [0u32; 20];
            row[0] = u32::try_from(n).unwrap_or(0);
            row[1] = u32::from(errors.is_some());
            for k in 0..MAX_LEVELS {
                let e = errors.and_then(|e| e.get(k)).copied().unwrap_or(0.0);
                row[4 + k] = e.to_bits();
                let s = if k + 1 < n {
                    distance_of(levels[k]).unwrap_or(f32::INFINITY)
                } else {
                    f32::INFINITY
                };
                row[12 + k] = s.to_bits();
            }
            groups.extend(push_words(&row));
        }
        self.group_rows.write(d, &groups)?;

        // The constants, row by row as the shader's `C_*` lay them out.
        let mut c: Vec<u8> = Vec::with_capacity(CONSTANT_ROWS * 16);
        let f = |c: &mut Vec<u8>, v: [f32; 4]| c.extend(v.iter().flat_map(|x| x.to_le_bytes()));
        let eye = frame.camera.eye;
        let k = pixels_at_one_metre(ask.height, frame.camera.fov_y);
        f(&mut c, [eye.x, eye.y, eye.z, k]);
        let flags = u32::from(ask.mirror.is_some())
            | if ask.cascades.is_some_and(|s| s.in_view) {
                2
            } else {
                0
            };
        f(
            &mut c,
            [
                settings.error_px,
                settings.fade,
                if settings.on { 1.0 } else { 0.0 },
                f32::from_bits(flags),
            ],
        );
        let redraw = ask.cascades.map_or(0, |s| s.redraw);
        c.extend(push_words(&[
            ask.targets.bases[0].min(ask.targets.capacity),
            ask.targets.bases[1].min(ask.targets.capacity),
            redraw,
            u32::try_from(self.groups.len()).unwrap_or(0),
        ]));
        for p in super::shadow::frustum(ask.clip) {
            f(&mut c, p.to_array());
        }
        for p in super::shadow::frustum(ask.mirror.unwrap_or(ask.clip)) {
            f(&mut c, p.to_array());
        }
        let sun = ask.cascades.map_or(Mat4::IDENTITY, |s| s.light_view);
        for col in sun.to_cols_array_2d() {
            f(&mut c, col);
        }
        for i in 0..4 {
            let (across, along) = ask
                .cascades
                .map_or(([0.0; 4], [0.0; 2]), |s| (s.across[i], s.along[i]));
            f(&mut c, across);
            f(&mut c, [along[0], along[1], 0.0, 0.0]);
        }
        self.constants.write(d, &c)?;

        // The blocks the walk drew, a bit a slot.
        let mut bits = vec![0u32; (SLOTS / 32) as usize];
        if frame.landscape {
            for id in &frame.drawn_blocks {
                if let Some(b) = scene.blocks.get(id).filter(|b| b.slot < SLOTS) {
                    bits[(b.slot / 32) as usize] |= 1 << (b.slot % 32);
                }
            }
        }
        self.drawn.write(d, &push_words(&bits))?;

        let t = ask.targets;
        let push = [
            table.index,
            self.high,
            self.group_rows.index,
            self.constants.index,
            t.offsets.1,
            self.drawn.index,
            t.instances.1,
            t.items.1,
            t.commands.1,
            t.counts.1,
            t.capacity,
            tallies.index,
            sums.index,
            states.index,
            casters.index,
            caster_counts.index,
            self.caster_capacity,
        ];
        Ok(Some(PackPlan {
            pipelines,
            push,
            count: self.high,
            reads: vec![
                table.buffer,
                self.group_rows.buffer,
                self.constants.buffer,
                t.offsets.0,
                self.drawn.buffer,
                t.items.0,
            ],
            writes: [tallies.buffer, sums.buffer, states.buffer],
            instances: t.instances.0,
            commands: t.commands.0,
            counts: t.counts.0,
            casters: casters.buffer,
            caster_counts: caster_counts.buffer,
            caster_capacity: self.caster_capacity,
            command_bytes: super::command_bytes(d),
            redraw,
        }))
    }

    /// The device's last choice for every model, after it has finished: for the checks.
    ///
    /// # Errors
    /// What the device refuses.
    pub fn read_states(&self, d: &mut Device) -> Result<Vec<PackState>, dereth_gfx::Error> {
        let Some(states) = self.states else {
            return Ok(Vec::new());
        };
        let bytes = read_back(d, states.buffer, 0, u64::from(self.high) * 16)?;
        let words: Vec<u32> = bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|w| u32::from_le_bytes(*w))
            .collect();
        let mut out = Vec::new();
        for (block, h) in &self.blocks {
            for (k, part) in h.parts.iter().enumerate() {
                let at = (h.range.start as usize + k) * 4;
                let Some(s) = words.get(at..at + 4) else {
                    continue;
                };
                let flags = s[0] >> 16;
                out.push(PackState {
                    block: *block,
                    part: *part,
                    level: (s[0] & 0xFF) as u8,
                    next: ((s[0] >> 8) & 0xFF) as u8,
                    fade: f32::from_bits(s[1]),
                    seen: flags & 1 != 0,
                    casts: ((flags >> 2) & 0xF) as u8,
                });
            }
        }
        Ok(out)
    }

    /// The items each cascade's list held when last drawn, after the device has finished: for
    /// the checks.
    ///
    /// # Errors
    /// What the device refuses.
    pub fn read_casters(&self, d: &mut Device) -> Result<[Vec<u32>; 4], dereth_gfx::Error> {
        let (Some(casters), Some(counts)) = (self.casters, self.caster_counts) else {
            return Ok(Default::default());
        };
        let words = super::command_bytes(d) / 4;
        let n = read_back(d, counts.buffer, 0, 16)?;
        let all = read_back(
            d,
            casters.buffer,
            0,
            4 * u64::from(self.caster_capacity) * u64::from(words) * 4,
        )?;
        let all: Vec<u32> = all
            .as_chunks::<4>()
            .0
            .iter()
            .map(|w| u32::from_le_bytes(*w))
            .collect();
        let mut out: [Vec<u32>; 4] = Default::default();
        for (c, list) in out.iter_mut().enumerate() {
            let count = u32::from_le_bytes([n[c * 4], n[c * 4 + 1], n[c * 4 + 2], n[c * 4 + 3]]);
            for s in 0..count.min(self.caster_capacity) as usize {
                let at = (c * self.caster_capacity as usize + s) * words as usize;
                // The item is each layout's last word.
                if let Some(item) = all.get(at + words as usize - 1) {
                    list.push(*item);
                }
            }
        }
        Ok(out)
    }
}

/// `len` bytes of `b` from `offset`, after the device has finished with it.
pub(crate) fn read_back(
    d: &mut Device,
    b: Buffer,
    offset: u64,
    len: u64,
) -> Result<Vec<u8>, dereth_gfx::Error> {
    let len = len.max(4);
    d.wait_idle()?;
    let back = d.create_buffer(&BufferDesc {
        label: "remaster pack read-back",
        size: len,
        usage: BufferUsage::TRANSFER_DST,
        location: Location::Readback,
    })?;
    let mut r = d.begin()?;
    r.copy_buffer(b, offset, back, 0, len)?;
    r.touch(&[Touch::Buffer(back, Use::HostRead)])?;
    let ticket = r.submit()?;
    d.wait(ticket)?;
    let bytes = d.read_buffer(back, 0, len)?;
    d.destroy_buffer(back);
    Ok(bytes)
}

impl PackPlan {
    /// Record the three passes: the models chosen, the lists' places counted, the commands
    /// written. Recorded after the processor's own items' cull, whose commands come first.
    ///
    /// # Errors
    /// What the layer refuses.
    pub(crate) fn record(&self, r: &mut Recorder<'_>) -> Result<(), dereth_gfx::Error> {
        let groups = self.count.div_ceil(GROUP).max(1);
        let push = push_words(&self.push);
        let [tallies, sums, states] = self.writes;
        let mut touches: Vec<Touch> = self
            .reads
            .iter()
            .map(|b| Touch::Buffer(*b, Use::StorageRead))
            .collect();
        touches.extend([
            Touch::Buffer(self.instances, Use::StorageWrite),
            Touch::Buffer(tallies, Use::StorageWrite),
            Touch::Buffer(sums, Use::StorageWrite),
            Touch::Buffer(states, Use::StorageWrite),
        ]);
        r.timestamp("pack choose");
        r.compute(&touches, |c| {
            c.set_pipeline(self.pipelines[0])?;
            c.push(0, &push)?;
            c.dispatch(groups, 1, 1);
            Ok(())
        })?;
        let constants = self.reads[2];
        r.compute(
            &[
                Touch::Buffer(constants, Use::StorageRead),
                Touch::Buffer(sums, Use::StorageWrite),
                Touch::Buffer(self.counts, Use::StorageWrite),
                Touch::Buffer(self.caster_counts, Use::StorageWrite),
            ],
            |c| {
                c.set_pipeline(self.pipelines[1])?;
                c.push(0, &push)?;
                c.dispatch(1, 1, 1);
                Ok(())
            },
        )?;
        let mut touches: Vec<Touch> = self
            .reads
            .iter()
            .map(|b| Touch::Buffer(*b, Use::StorageRead))
            .collect();
        touches.extend([
            Touch::Buffer(tallies, Use::StorageRead),
            Touch::Buffer(sums, Use::StorageRead),
            Touch::Buffer(states, Use::StorageRead),
            Touch::Buffer(self.commands, Use::StorageWrite),
            Touch::Buffer(self.casters, Use::StorageWrite),
        ]);
        r.compute(&touches, |c| {
            c.set_pipeline(self.pipelines[2])?;
            c.push(0, &push)?;
            c.dispatch(groups, 1, 1);
            Ok(())
        })?;
        // The fades written into the instances' rows, for every pass that draws them.
        r.touch(&[Touch::Buffer(self.instances, Use::StorageRead)])
    }

    /// Whether cascade `c`'s cache takes the models' casters this frame: it is drawn again.
    #[must_use]
    pub(crate) fn casts(&self, c: usize) -> bool {
        (self.redraw >> c) & 1 != 0
    }

    /// What cascade `c`'s caster draw reads.
    #[must_use]
    pub(crate) fn caster_touches(&self) -> [Touch; 2] {
        [
            Touch::Buffer(self.casters, Use::Indirect),
            Touch::Buffer(self.caster_counts, Use::Indirect),
        ]
    }

    /// Draw the models' casters into cascade `c`'s cache, the casters' pipeline, push constants
    /// and index buffer bound.
    ///
    /// # Errors
    /// What the layer refuses.
    pub(crate) fn draw_casters(&self, d: &mut Draw<'_>, c: usize) -> Result<(), dereth_gfx::Error> {
        #[allow(clippy::cast_possible_truncation)] // four cascades
        let c = c as u32;
        d.draw_indexed_indirect_count(
            self.casters,
            u64::from(c) * u64::from(self.caster_capacity) * u64::from(self.command_bytes),
            self.caster_counts,
            u64::from(c) * 4,
            self.caster_capacity,
            self.command_bytes,
        )
    }
}

/// The view and projection's planes as `shadow::frustum` makes them, kept for the checks.
#[must_use]
pub fn seen(clip: Mat4, centre: Vec3, radius: f32) -> bool {
    super::shadow::frustum(clip)
        .iter()
        .all(|p: &Vec4| p.truncate().dot(centre) + p.w >= -radius)
}
