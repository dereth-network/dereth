//! The blocks' parts chosen and listed on the device (`shaders/instances.slang`): for each, every
//! frame, its level of detail and its fade into the next, how much of it a tree's impostor draws,
//! whether it is seen, the sun's cascades it casts into, the commands that draw it and its
//! impostor's card. The processor writes a part's row, and its degrade record's, once, as its block
//! comes, and retail's level for a record when the walk chooses another; each frame only the
//! frame's own: the eye, the view, each group's hand-overs and which blocks the walk drew. It does
//! nothing a part a frame.
//!
//! **Which parts.** A block's part is chosen on the device when it has at most [`MAX_LEVELS`]
//! levels, none of them turns toward the viewer, and each of its items is drawn in the G-buffer's
//! culled buckets. Everything else (a part with a blended surface or a cut-out's edges, which the
//! forward and edge passes draw far to near; one that turns; a room's statics) is chosen on the
//! processor as before ([`super::lod`]); the processor's walks of a block take only its other parts
//! ([`DeviceParts::cpu_parts`]).
//!
//! **The same choice.** A part's level, fade and impostor share are the processor's for it
//! ([`super::lod::BlockLod::part`] standing outdoors): its hand-overs from its group's errors and
//! retail's bands or the pack's distances, at its own scale; a tree's impostor after its last level,
//! wholly in before the far forest could stand it; faded over the same band; kept from levels
//! coarser than retail's own; and retail's level alone while its group's errors are not measured
//! or levels are not chosen, which past the blocks retail stands objects on they always are. Each
//! frame the processor writes each group's row, its levels' errors and the distances retail or the
//! pack hands them over at; the device works each part's hand-overs out from it at the part's
//! scale, and its distance and pick. A part is seen where its block is drawn and its sphere stands
//! in the view, or in the mirrored view while the water's mirror draws; its card only in the view.
//!
//! **The lists.** Each part's commands follow the processor's in the G-buffer's two culled
//! buckets, and its first level's go into each cascade it reaches as it is drawn again; ground
//! cover casts into the nearest cascade alone. The cards are drawn after the processor's own. Every
//! list is in the parts' order, frame after frame.
//!
//! **Where the device takes no draw count from a buffer**, every part stays on the processor.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::ops::Range;

use dereth_gfx::{
    Buffer, BufferDesc, BufferUsage, ComputeDesc, Device, Draw, Location, Pipeline, Recorder,
    Touch, Use,
};
use glam::{Mat4, Vec3, Vec4};

use super::impostor::ImpostorPass;
use super::lod::{impostor_cap, pixels_at_one_metre, Lod, LodSettings};
use super::pool::Pool;
use super::scene::{Bucket, GpuScene, PartItems};
use super::{push_words, shader, PerFrame};
use crate::assets::AssetTables;
use crate::scene::{BlockId, BlockLevels, DegradeBias, FrameDelta, SceneDelta};
use crate::shaders::Entry;

/// The most levels a part chosen on the device has.
pub const MAX_LEVELS: usize = 8;
/// The most hand-overs a group has: one a level but the last, and its impostor's.
const MAX_HANDS: usize = 8;
/// The bytes of one part's row, of one record's, of one group's, and the frame's constant rows.
const ENTRY_BYTES: usize = 64;
const RECORD_BYTES: usize = 32;
const GROUP_WORDS: usize = 32;
const CONSTANT_ROWS: usize = 28;
/// The bytes of one card.
const CARD_BYTES: u64 = 80;
/// The blocks' slots the drawn blocks' bits cover.
const SLOTS: u32 = 1 << 12;
/// A part's flags, as the shader reads them: ground cover; held.
const M_COVER: u32 = 1;
const M_LIVE: u32 = 2;
/// A group's flags: its errors are measured; its own hand-over distances grow with the part's
/// scale; its last level is a tree's impostor.
const G_MEASURED: u32 = 1;
const G_SCALED: u32 = 2;
const G_IMPOSTOR: u32 = 4;
/// No record, group or tree.
const NONE: u32 = u32::MAX;
/// The parts a workgroup takes.
const GROUP: u32 = 64;
/// The parts there is room for at first.
const FIRST_CAPACITY: u32 = 1 << 12;
/// The flags a part's state carries: seen; its block drawn.
const S_SEEN: u32 = 1;
const S_DRAWN: u32 = 64;

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

/// One block's parts on the device: their rows' run, their records' run, the parts left to the
/// processor, and the parts the rows stand for, in the rows' order.
#[derive(Debug)]
struct Held {
    range: Range<u32>,
    records: Range<u32>,
    cpu_parts: Vec<u32>,
    parts: Vec<u32>,
    /// The items of the parts' first levels, together: what they add to the cascades' lists.
    first_items: u64,
    /// The parts that are trees, each of which may draw a card.
    trees: u64,
}

/// A group of records the device picks levels for alike: the records of one group of levels
/// ([`Lod`]'s, which share their levels' errors) with the same bands, and the row in the atlas
/// of its tree's pictures when it is a tree's.
#[derive(Debug, Clone)]
struct DeviceGroup {
    lod: u32,
    /// A record of retail's bands, each level's nearest, ideal and farthest distance, at a scale
    /// of one; `None` for records whose levels hand over where their art pack says.
    retail: Option<crate::scene::Degrade>,
    tree: Option<u32>,
}

/// What a frame works the groups' rows out in, kept from frame to frame.
#[derive(Debug, Default)]
struct RowScratch {
    distances: Vec<f32>,
    switches: Vec<f32>,
    errors: Vec<f32>,
}

/// What groups the device groups: the group of levels and the bands, by their bits.
type GroupKey = (u32, Vec<[u32; 3]>);

/// The cascades as the parts are cast into them this frame.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PartsCascades {
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

/// What the device's command lists the parts go into are, this frame.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PartsTargets {
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
    /// The trees' pictures' rows in the atlas, and the frame's constants as the G-buffer reads
    /// them.
    pub trees: (Buffer, u32),
    pub frame: u32,
}

/// What one frame asks of the parts.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PartsAsk<'a> {
    pub frame: &'a FrameDelta,
    /// The frame drawn this many pixels tall, as the levels are chosen for it.
    pub height: u32,
    /// The unjittered view and projection, and the mirrored view's while the water's mirror draws.
    pub clip: Mat4,
    pub mirror: Option<Mat4>,
    pub cascades: Option<PartsCascades>,
    pub targets: PartsTargets,
}

/// The parts' frame on the device.
#[derive(Debug, Clone)]
pub struct PartsPlan {
    pipelines: [Pipeline; 3],
    push: [u32; 25],
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
    cards: Buffer,
    cards_index: u32,
    card_parts: Buffer,
    card_draw: Buffer,
}

/// One part's choice as the device made it, for the checks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PartState {
    /// The part's block and its place among the block's parts.
    pub block: BlockId,
    pub part: u32,
    /// The level it draws, the level it fades into (its own when not fading) and how far.
    pub level: u8,
    pub next: u8,
    pub fade: f32,
    /// How much of it its impostor draws: 0 none, 1 all of it.
    pub share: f32,
    /// Whether it is seen, whether its block is drawn, and a bit for each cascade it casts into.
    pub seen: bool,
    pub drawn: bool,
    pub casts: u8,
}

/// One card the device drew, for the checks: its tree's block and place among the block's parts,
/// and the card's five rows as the cards' pass reads them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DeviceCard {
    pub block: BlockId,
    pub part: u32,
    pub rows: [[f32; 4]; 5],
}

/// The parts chosen on the device.
#[derive(Debug)]
pub struct DeviceParts {
    /// The passes; `None` where the device takes no draw count from a buffer.
    pipelines: Option<[Pipeline; 3]>,
    /// Whether the parts are chosen on the device; off, every part is the processor's.
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
    /// The records' rows and retail's level of each, as on the device.
    record_pool: Pool,
    record_rows: Vec<u8>,
    retail_words: Vec<u32>,
    records: Option<Bound>,
    retail: Option<Bound>,
    casters: Option<Bound>,
    caster_counts: Option<Bound>,
    caster_capacity: u32,
    caster_items: u64,
    /// The cards, the part each stands for, and their indirect draw.
    cards: Option<Bound>,
    card_parts: Option<Bound>,
    card_draw: Option<Bound>,
    card_capacity: u32,
    trees: u64,
    // ORDER-OK: looked up by block, and walked in block order.
    blocks: BTreeMap<BlockId, Held>,
    /// The groups, by their place in the frame's table of groups, and what their rows are worked
    /// out in.
    groups: Vec<DeviceGroup>,
    scratch: RowScratch,
    // ORDER-OK: only looked up.
    group_slot: HashMap<GroupKey, u32>,
    constants: PerFrame,
    group_rows: PerFrame,
    drawn: PerFrame,
    /// The rows written so far: none past it is held.
    high: u32,
    /// The last frame's origin, its blocks' offsets measured from it.
    origin: [i32; 2],
    /// Counts the changes to which parts are the processor's ([`Self::cpu_parts`]).
    generation: u64,
}

impl DeviceParts {
    /// The parts' passes on `d`, made where it takes a draw count from a buffer.
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
                make("remaster parts choose", Entry::InstancesCsChoose)?,
                make("remaster parts scan", Entry::InstancesCsScan)?,
                make("remaster parts write", Entry::InstancesCsWrite)?,
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
            record_pool: Pool::new(0),
            record_rows: Vec::new(),
            retail_words: Vec::new(),
            records: None,
            retail: None,
            casters: None,
            caster_counts: None,
            caster_capacity: 0,
            caster_items: 0,
            cards: None,
            card_parts: None,
            card_draw: None,
            card_capacity: 0,
            trees: 0,
            blocks: BTreeMap::new(),
            groups: Vec::new(),
            scratch: RowScratch::default(),
            group_slot: HashMap::new(),
            constants: PerFrame::new(
                d,
                "remaster parts constants",
                (CONSTANT_ROWS * 16) as u64,
                BufferUsage::default(),
            )?,
            group_rows: PerFrame::new(d, "remaster parts groups", 1 << 12, BufferUsage::default())?,
            drawn: PerFrame::new(
                d,
                "remaster parts drawn blocks",
                u64::from(SLOTS / 8),
                BufferUsage::default(),
            )?,
            high: 0,
            origin: [0; 2],
            generation: 0,
        })
    }

    /// The block whose south-west corner was the last frame's origin.
    #[must_use]
    pub fn origin(&self) -> [i32; 2] {
        self.origin
    }

    /// Whether the parts are chosen on the device, from the next change of the scene on.
    pub(crate) fn set_on(&mut self, on: bool) {
        if on != self.on {
            self.on = on;
            self.rebuild = true;
        }
    }

    /// Whether the parts are chosen on the device: asked for, and the device can.
    #[must_use]
    pub fn on(&self) -> bool {
        self.on && self.pipelines.is_some()
    }

    /// The parts chosen on the device.
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

    /// Take the scene's changes: each block that came, or changed, has its parts' rows written,
    /// and each that went has its rows let go.
    ///
    /// # Errors
    /// What the device refuses.
    pub(crate) fn take(
        &mut self,
        d: &mut Device,
        held: (&GpuScene, &AssetTables),
        lod: &mut Lod,
        impostors: &mut ImpostorPass,
        deltas: &[SceneDelta],
    ) -> Result<(), dereth_gfx::Error> {
        let (scene, _) = held;
        if std::mem::take(&mut self.rebuild) {
            let ids: Vec<BlockId> = scene.blocks.keys().copied().collect();
            for id in ids {
                self.block_in(d, held, lod, impostors, id)?;
            }
        }
        for delta in deltas {
            match delta {
                SceneDelta::BlockIn(b) | SceneDelta::BlockChanged(b) => {
                    self.block_in(d, held, lod, impostors, b.block)?;
                }
                SceneDelta::BlockOut(id) => self.block_out(d, *id)?,
                _ => {}
            }
        }
        Ok(())
    }

    /// Retail's levels of block `l.block`'s records as the walk now chooses them.
    ///
    /// # Errors
    /// What the device refuses.
    pub(crate) fn levels(
        &mut self,
        d: &mut Device,
        scene: &GpuScene,
        lod: &Lod,
        l: &BlockLevels,
    ) -> Result<(), dereth_gfx::Error> {
        self.write_retail(d, scene, lod, l.block)
    }

    /// Write retail's level of each of block `id`'s records, and whether the walk chooses it, as
    /// the processor reads them ([`super::lod::BlockLod::part`]).
    fn write_retail(
        &mut self,
        d: &mut Device,
        scene: &GpuScene,
        lod: &Lod,
        id: BlockId,
    ) -> Result<(), dereth_gfx::Error> {
        let (Some(h), Some(b)) = (self.blocks.get(&id), scene.blocks.get(&id)) else {
            return Ok(());
        };
        let walked = lod.walked(id);
        let (a, z) = (h.records.start as usize, h.records.end as usize);
        for (k, w) in self.retail_words[a..z].iter_mut().enumerate() {
            let level = b.levels.get(k).copied().unwrap_or(0);
            *w = u32::from(level) | if k < walked { 0x100 } else { 0 };
        }
        if let Some(r) = self.retail {
            d.write_buffer(
                r.buffer,
                (a * 4) as u64,
                &push_words(&self.retail_words[a..z]),
            )?;
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
        self.trees -= h.trees;
        self.pool.give(h.range);
        if !h.records.is_empty() {
            self.record_pool.give(h.records);
        }
        self.high = self.blocks.values().map(|h| h.range.end).max().unwrap_or(0);
        Ok(())
    }

    /// Write block `id`'s parts' rows, as the scene now holds it.
    fn block_in(
        &mut self,
        d: &mut Device,
        (scene, tables): (&GpuScene, &AssetTables),
        lod: &mut Lod,
        impostors: &mut ImpostorPass,
        id: BlockId,
    ) -> Result<(), dereth_gfx::Error> {
        self.block_out(d, id)?;
        lod.set_cpu_records(id, None);
        let Some(b) = scene.blocks.get(&id) else {
            return Ok(());
        };
        if !self.on() || b.slot == super::scene::NONE || b.slot >= SLOTS {
            return Ok(());
        }
        let records = u32::try_from(lod.records(id).len()).unwrap_or(0);
        // The records' buffers are made with the first block, whether or not it has records.
        let record_range = if records == 0 && self.records.is_some() {
            0..0
        } else {
            self.record_room(d, records)?
        };
        let mut rows: Vec<u8> = Vec::new();
        let mut parts = Vec::new();
        let mut cpu_parts = Vec::new();
        let (mut first_items, mut trees) = (0u64, 0u64);
        for (k, p) in b.parts.iter().enumerate() {
            let k = u32::try_from(k).unwrap_or(u32::MAX);
            match self.row(
                d,
                (scene, tables),
                lod,
                impostors,
                (id, b.slot),
                &record_range,
                p,
            )? {
                Some((row, first, tree)) => {
                    rows.extend_from_slice(&row);
                    parts.push(k);
                    first_items += u64::from(first);
                    trees += u64::from(tree);
                }
                None => cpu_parts.push(k),
            }
        }
        // The processor chooses only the records its own parts and the rooms' statics degrade by:
        // those of the device's parts, and those no part degrades by (a placement an art pack
        // drew its own model for), it never asks of.
        let cpu_records: BTreeSet<u32> = cpu_parts
            .iter()
            .filter_map(|k| b.parts.get(*k as usize).and_then(|p| p.degrade))
            .chain(
                b.rooms
                    .iter()
                    .flat_map(|r| r.statics.iter().filter_map(|p| p.degrade)),
            )
            .collect();
        lod.set_cpu_records(id, Some(cpu_records.into_iter().collect()));
        if parts.is_empty() {
            if !record_range.is_empty() {
                self.record_pool.give(record_range);
            }
            return Ok(());
        }
        // The records' rows: where each stands, its scale, and its group.
        let mut record_bytes = Vec::with_capacity(records as usize * RECORD_BYTES);
        for (r, rec) in lod.records(id).iter().enumerate() {
            let group = lod
                .group_of_record(id, u32::try_from(r).unwrap_or(u32::MAX))
                .and_then(|g| self.group_slot.get(&group_key(g, rec)).copied())
                .unwrap_or(NONE);
            let scale = if rec.scale_z == 0.0 {
                1.0
            } else {
                rec.scale_z.abs()
            };
            for f in [rec.centre.x, rec.centre.y, rec.centre.z, scale] {
                record_bytes.extend_from_slice(&f.to_le_bytes());
            }
            record_bytes.extend(push_words(&[group, 0, 0, 0]));
        }
        if !record_range.is_empty() {
            let at = record_range.start as usize * RECORD_BYTES;
            self.record_rows[at..at + record_bytes.len()].copy_from_slice(&record_bytes);
            if let Some(r) = self.records {
                d.write_buffer(r.buffer, at as u64, &record_bytes)?;
            }
        }
        let n = u32::try_from(parts.len()).unwrap_or(u32::MAX);
        let range = self.room(d, n)?;
        let at = range.start as usize * ENTRY_BYTES;
        self.rows[at..at + rows.len()].copy_from_slice(&rows);
        if let Some(t) = self.table {
            d.write_buffer(t.buffer, at as u64, &rows)?;
        }
        self.caster_items += first_items;
        self.trees += trees;
        self.high = self.high.max(range.end);
        self.blocks.insert(
            id,
            Held {
                range,
                records: record_range,
                cpu_parts,
                parts,
                first_items,
                trees,
            },
        );
        self.write_retail(d, scene, lod, id)?;
        self.generation += 1;
        self.casters_fit(d)?;
        self.cards_fit(d)?;
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
            "remaster parts",
            entries * ENTRY_BYTES as u64,
            BufferUsage::default(),
        )?);
        self.tallies = Some(bound(
            d,
            "remaster parts tallies",
            entries * 16,
            BufferUsage::default(),
        )?);
        self.sums = Some(bound(
            d,
            "remaster parts sums",
            entries.div_ceil(u64::from(GROUP)) * 32,
            BufferUsage::default(),
        )?);
        self.states = Some(bound(
            d,
            "remaster parts states",
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

    /// Room for `n` records, their buffers grown when there is none.
    fn record_room(&mut self, d: &mut Device, n: u32) -> Result<Range<u32>, dereth_gfx::Error> {
        if let (Some(r), Some(_)) = (self.record_pool.take(n), self.records) {
            return Ok(r);
        }
        let old = self.record_pool.capacity();
        let capacity = (old * 2)
            .max(old + n)
            .max(FIRST_CAPACITY)
            .next_power_of_two();
        for b in [self.records, self.retail].into_iter().flatten() {
            d.destroy_buffer(b.buffer);
        }
        let entries = u64::from(capacity);
        let records = bound(
            d,
            "remaster parts records",
            entries * RECORD_BYTES as u64,
            BufferUsage::default(),
        )?;
        let retail = bound(
            d,
            "remaster parts retail levels",
            entries * 4,
            BufferUsage::default(),
        )?;
        self.record_rows.resize(capacity as usize * RECORD_BYTES, 0);
        self.retail_words.resize(capacity as usize, 0);
        d.write_buffer(records.buffer, 0, &self.record_rows)?;
        d.write_buffer(retail.buffer, 0, &push_words(&self.retail_words))?;
        self.records = Some(records);
        self.retail = Some(retail);
        self.record_pool.grow(capacity);
        self.record_pool
            .take(n)
            .ok_or_else(|| dereth_gfx::Error::Invalid("a pool that grew still has no room".into()))
    }

    /// Room in each cascade's list for every part's first level.
    fn casters_fit(&mut self, d: &mut Device) -> Result<(), dereth_gfx::Error> {
        if self.caster_counts.is_none() {
            self.caster_counts = Some(bound(
                d,
                "remaster parts caster counts",
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
            "remaster parts casters",
            4 * u64::from(capacity) * u64::from(super::command_bytes(d)),
            BufferUsage::INDIRECT,
        )?);
        self.caster_capacity = capacity;
        Ok(())
    }

    /// Room for a card for every tree, and the cards' draw.
    fn cards_fit(&mut self, d: &mut Device) -> Result<(), dereth_gfx::Error> {
        if self.card_draw.is_none() {
            self.card_draw = Some(bound(
                d,
                "remaster parts cards' draw",
                32,
                BufferUsage::INDIRECT,
            )?);
        }
        if self.trees <= u64::from(self.card_capacity) && self.cards.is_some() {
            return Ok(());
        }
        let capacity = u32::try_from(self.trees.max(256).next_power_of_two()).unwrap_or(u32::MAX);
        for c in [self.cards.take(), self.card_parts.take()]
            .into_iter()
            .flatten()
        {
            d.destroy_buffer(c.buffer);
        }
        self.cards = Some(bound(
            d,
            "remaster parts cards",
            u64::from(capacity) * CARD_BYTES,
            BufferUsage::default(),
        )?);
        self.card_parts = Some(bound(
            d,
            "remaster parts cards' parts",
            u64::from(capacity) * 4,
            BufferUsage::default(),
        )?);
        self.card_capacity = capacity;
        Ok(())
    }

    /// Part `p`'s row, its first level's items, and whether it is a tree, when the device chooses
    /// it: block `block` standing at slot `slot`, its records' rows from `records`.
    #[allow(clippy::too_many_arguments)]
    fn row(
        &mut self,
        d: &mut Device,
        (scene, tables): (&GpuScene, &AssetTables),
        lod: &mut Lod,
        impostors: &mut ImpostorPass,
        (block, slot): (BlockId, u32),
        records: &Range<u32>,
        p: &PartItems,
    ) -> Result<Option<([u8; ENTRY_BYTES], u32, bool)>, dereth_gfx::Error> {
        if p.turned.is_some() || p.levels.is_empty() || p.levels.len() > MAX_LEVELS {
            return Ok(None);
        }
        // Each level's items, one run after another, each in the G-buffer's culled buckets.
        let mut counts = [0u8; MAX_LEVELS];
        for (k, r) in p.levels.iter().enumerate() {
            if k > 0 && p.levels[k - 1].end != r.start {
                return Ok(None);
            }
            let Ok(n) = u8::try_from(r.end - r.start) else {
                return Ok(None);
            };
            counts[k] = n;
            for i in r.clone() {
                let culled = matches!(
                    scene.buckets.get(i as usize),
                    Some(Bucket::Opaque | Bucket::OpaqueTwoSided)
                );
                if !culled || scene.item_draws.get(i as usize).is_none_or(|d| d.edge) {
                    return Ok(None);
                }
            }
        }
        let (record, tree, card) = match p.degrade {
            Some(r) => {
                let Some(g) = lod.group_of_record(block, r) else {
                    return Ok(None);
                };
                let Some((levels, turns, _)) = lod.group_info(g) else {
                    return Ok(None);
                };
                if turns || levels.len() != p.levels.len() {
                    return Ok(None);
                }
                // Whether it is a tree is known once its first level is held.
                let Some(tree) = lod.group_tree(g, tables) else {
                    return Ok(None);
                };
                let Some(rec) = lod.records(block).get(r as usize).cloned() else {
                    return Ok(None);
                };
                if r >= records.end - records.start {
                    return Ok(None);
                }
                let key = group_key(g, &rec);
                if !self.group_slot.contains_key(&key) {
                    let tree_slot = match (tree, lod.first_level(block, r)) {
                        (true, Some(mesh)) => impostors.tree_slot(d, scene, mesh, p)?,
                        _ => None,
                    };
                    let s = u32::try_from(self.groups.len()).unwrap_or(u32::MAX);
                    self.groups.push(DeviceGroup {
                        lod: g,
                        retail: (rec.bands.len() >= 2).then(|| crate::scene::Degrade {
                            bands: rec.bands.clone(),
                            scale_z: 1.0,
                            ..crate::scene::Degrade::default()
                        }),
                        tree: tree_slot,
                    });
                    self.group_slot.insert(key, s);
                }
                lod.want(g);
                // Its card's sphere, block-local, worked out as the processor works out its own
                // cards' ([`super::impostor`]), so the device's stand where the processor's would.
                let card = lod
                    .first_level(block, r)
                    .filter(|_| tree)
                    .and_then(|mesh| impostors.tree_sphere(mesh))
                    .map(|(centre, radius)| {
                        let t = p.transform;
                        let scale = t
                            .x_axis
                            .truncate()
                            .length()
                            .max(t.y_axis.truncate().length())
                            .max(t.z_axis.truncate().length());
                        (t.transform_point3(centre), radius * scale)
                    });
                (records.start + r, tree, card)
            }
            None if p.levels.len() == 1 => (NONE, false, None),
            None => return Ok(None),
        };
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
            record,
            p.levels[0].start,
            u32::from_le_bytes([counts[0], counts[1], counts[2], counts[3]]),
            u32::from_le_bytes([counts[4], counts[5], counts[6], counts[7]]),
            M_LIVE | if p.cover { M_COVER } else { 0 },
            u32::try_from(p.levels.len()).unwrap_or(0),
        ];
        for (k, w) in words.iter().enumerate() {
            out[k * 4..k * 4 + 4].copy_from_slice(&w.to_le_bytes());
        }
        let (centre, card_radius) = card.unwrap_or((Vec3::ZERO, 0.0));
        let floats = [
            sphere.x,
            sphere.y,
            sphere.z,
            radius,
            centre.x,
            centre.y,
            centre.z,
            card_radius,
        ];
        for (k, f) in floats.iter().enumerate() {
            out[32 + k * 4..36 + k * 4].copy_from_slice(&f.to_le_bytes());
        }
        Ok(Some((out, u32::from(counts[0]), tree)))
    }

    /// Write the frame's own state for the parts: the groups' rows, the constants and the drawn
    /// blocks' bits. `None` when no part is held.
    ///
    /// # Errors
    /// What the device refuses.
    pub(crate) fn prepare(
        &mut self,
        d: &mut Device,
        scene: &GpuScene,
        lod: &Lod,
        distance_of: &dyn Fn(u32) -> Option<f32>,
        ask: &PartsAsk<'_>,
    ) -> Result<Option<PartsPlan>, dereth_gfx::Error> {
        self.origin = ask.frame.origin;
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
        let (Some(records), Some(retail)) = (self.records, self.retail) else {
            return Ok(None);
        };
        let (Some(cards), Some(card_parts), Some(card_draw)) =
            (self.cards, self.card_parts, self.card_draw)
        else {
            return Ok(None);
        };
        if self.high == 0 || !self.on() {
            return Ok(None);
        }
        let frame = ask.frame;
        let settings = lod.settings();
        // Past the blocks retail stands objects on, levels are chosen as on chooses them whatever
        // the settings say, as the processor chooses them.
        let chosen = if settings.on {
            settings
        } else {
            LodSettings::ON
        };
        // Retail's bias follows its frame rate; its hand-overs are taken at the farthest it
        // slides them, as the processor takes them.
        let farthest = DegradeBias {
            bias: frame.degrade.bias.max(1.0),
            ..frame.degrade
        };
        let mut groups = Vec::with_capacity(self.groups.len() * GROUP_WORDS * 4);
        for g in &self.groups {
            let row = group_row(
                g,
                lod,
                (chosen.impostors, &farthest),
                distance_of,
                &mut self.scratch,
            );
            groups.extend(row.iter().flat_map(|w| w.to_le_bytes()));
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
                chosen.error_px,
                chosen.fade,
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
        let cap = impostor_cap(frame.scenery_reach, chosen.fade, &frame.degrade);
        f(&mut c, [cap, 0.0, 0.0, 0.0]);
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
            records.index,
            retail.index,
            t.trees.1,
            cards.index,
            card_parts.index,
            self.card_capacity,
            card_draw.index,
            t.frame,
        ];
        Ok(Some(PartsPlan {
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
                records.buffer,
                retail.buffer,
                t.trees.0,
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
            cards: cards.buffer,
            cards_index: cards.index,
            card_parts: card_parts.buffer,
            card_draw: card_draw.buffer,
        }))
    }

    /// The device's last choice for every part, after it has finished: for the checks.
    ///
    /// # Errors
    /// What the device refuses.
    pub fn read_states(&self, d: &mut Device) -> Result<Vec<PartState>, dereth_gfx::Error> {
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
                out.push(PartState {
                    block: *block,
                    part: *part,
                    level: (s[0] & 0xFF) as u8,
                    next: ((s[0] >> 8) & 0xFF) as u8,
                    fade: f32::from_bits(s[1]),
                    share: f32::from_bits(s[2]),
                    seen: flags & S_SEEN != 0,
                    drawn: flags & S_DRAWN != 0,
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

    /// The cards the device last drew, after it has finished: for the checks.
    ///
    /// # Errors
    /// What the device refuses.
    pub fn read_cards(&self, d: &mut Device) -> Result<Vec<DeviceCard>, dereth_gfx::Error> {
        let (Some(cards), Some(parts), Some(draw)) = (self.cards, self.card_parts, self.card_draw)
        else {
            return Ok(Vec::new());
        };
        let head = read_back(d, draw.buffer, 0, 32)?;
        // The instances' count: the second word of the draw, after the item on Direct3D 12.
        let at = if d.backend() == dereth_gfx::Backend::D3d12 {
            8
        } else {
            4
        };
        let count = head
            .get(at..at + 4)
            .map_or(0, |w| u32::from_le_bytes([w[0], w[1], w[2], w[3]]))
            .min(self.card_capacity);
        if count == 0 {
            return Ok(Vec::new());
        }
        let rows = read_back(d, cards.buffer, 0, u64::from(count) * CARD_BYTES)?;
        let owners = read_back(d, parts.buffer, 0, u64::from(count) * 4)?;
        let floats: Vec<f32> = rows
            .as_chunks::<4>()
            .0
            .iter()
            .map(|w| f32::from_le_bytes(*w))
            .collect();
        let mut out = Vec::new();
        for (k, w) in owners.as_chunks::<4>().0.iter().enumerate() {
            let row = u32::from_le_bytes(*w);
            let Some((block, part)) = self.part_of_row(row) else {
                continue;
            };
            let mut card = [[0.0f32; 4]; 5];
            for (r, c) in card.iter_mut().enumerate() {
                for (j, x) in c.iter_mut().enumerate() {
                    *x = floats.get(k * 20 + r * 4 + j).copied().unwrap_or(0.0);
                }
            }
            out.push(DeviceCard {
                block,
                part,
                rows: card,
            });
        }
        Ok(out)
    }

    /// The block and place among its parts of the part at row `row`.
    fn part_of_row(&self, row: u32) -> Option<(BlockId, u32)> {
        self.blocks.iter().find_map(|(id, h)| {
            h.range
                .contains(&row)
                .then(|| {
                    h.parts
                        .get((row - h.range.start) as usize)
                        .map(|p| (*id, *p))
                })
                .flatten()
        })
    }
}

/// What groups record `rec` of levels' group `g` on the device.
fn group_key(g: u32, rec: &crate::scene::Degrade) -> GroupKey {
    let bands = if rec.bands.len() >= 2 {
        rec.bands
            .iter()
            .map(|b| b.map(f32::to_bits))
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    (g, bands)
}

/// Group `g`'s row this frame: its hand-overs' count, its flags, its impostor's level and its
/// tree's row in the atlas; its levels' errors (its impostor's in place of a level that draws
/// nothing, or after its last); its hand-overs' own distances, as the processor works them out
/// for a record of it ([`Lod::prepare`]), retail's to be scaled by the part's.
fn group_row(
    g: &DeviceGroup,
    lod: &Lod,
    (impostors, farthest): (bool, &DegradeBias),
    distance_of: &dyn Fn(u32) -> Option<f32>,
    scratch: &mut RowScratch,
) -> [u32; GROUP_WORDS] {
    let mut row = [0u32; GROUP_WORDS];
    row[2] = NONE;
    row[3] = g.tree.unwrap_or(NONE);
    let Some((levels, _, Some(errors))) = lod.group_info(g.lod) else {
        return row;
    };
    let mut flags = G_MEASURED;
    let RowScratch {
        distances,
        switches,
        errors: own,
    } = scratch;
    switches.clear();
    match &g.retail {
        Some(record) => {
            super::lod::retail_switches_into(record, farthest, switches);
            flags |= G_SCALED;
        }
        None => {
            distances.clear();
            distances.extend(
                levels[..levels.len().saturating_sub(1)]
                    .iter()
                    .map(|m| distance_of(*m).unwrap_or(f32::INFINITY)),
            );
            super::lod::pack_switches_into(distances, farthest, switches);
        }
    }
    own.clear();
    own.extend_from_slice(errors);
    // A tree's impostor takes the place of its level that draws nothing, or comes after its last.
    if let (true, Some(at)) = (impostors, lod.group_impostor_error(g.lod)) {
        if levels.len() > 1 && levels.last() == Some(&0) {
            let i = levels.len() - 1;
            own[i] = at;
            row[2] = u32::try_from(i).unwrap_or(NONE);
        } else {
            own.push(at);
            switches.push(0.0);
            row[2] = u32::try_from(levels.len()).unwrap_or(NONE);
        }
        flags |= G_IMPOSTOR;
    }
    row[0] = u32::try_from(switches.len().min(MAX_HANDS)).unwrap_or(0);
    row[1] = flags;
    for (k, e) in own.iter().take(12).enumerate() {
        row[4 + k] = e.to_bits();
    }
    for (k, s) in switches.iter().take(MAX_HANDS).enumerate() {
        row[16 + k] = s.to_bits();
    }
    row
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
        label: "remaster parts read-back",
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

impl PartsPlan {
    /// Record the three passes: the parts chosen, the lists' places counted, the commands and
    /// cards written. Recorded after the processor's own items' cull, whose commands come first.
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
        r.timestamp("parts choose");
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
                Touch::Buffer(self.card_draw, Use::StorageWrite),
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
            Touch::Buffer(self.instances, Use::StorageRead),
            Touch::Buffer(tallies, Use::StorageRead),
            Touch::Buffer(sums, Use::StorageRead),
            Touch::Buffer(states, Use::StorageRead),
            Touch::Buffer(self.commands, Use::StorageWrite),
            Touch::Buffer(self.casters, Use::StorageWrite),
            Touch::Buffer(self.cards, Use::StorageWrite),
            Touch::Buffer(self.card_parts, Use::StorageWrite),
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

    /// Whether cascade `c`'s cache takes the parts' casters this frame: it is drawn again.
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

    /// Draw the parts' casters into cascade `c`'s cache, the casters' pipeline, push constants
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

    /// The cards the parts draw: their rows by buffer and index, and their indirect draw.
    #[must_use]
    pub(crate) fn cards(&self) -> super::impostor::DeviceCards {
        super::impostor::DeviceCards {
            cards: (self.cards, self.cards_index),
            draw: self.card_draw,
        }
    }
}

/// The view and projection's planes as `shadow::frustum` makes them, kept for the checks.
#[must_use]
pub fn seen(clip: Mat4, centre: Vec3, radius: f32) -> bool {
    super::shadow::frustum(clip)
        .iter()
        .all(|p: &Vec4| p.truncate().dot(centre) + p.w >= -radius)
}
