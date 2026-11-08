//! The retained GPU scene: one vertex and one index pool, the materials and textures, each
//! block's ground, and the instances and draw items of every block, room and object.
//!
//! An instance is one placed part: its transform (block-local, or in the frame's space for an
//! object), its per-axis scale, the block it stands in, its light class, its light set and the
//! object's material override. A draw item is one instance and one surface's triangles of one of
//! its levels' meshes. What a frame draws is a list of draw items; the cull makes them indirect
//! draw commands.

use std::collections::BTreeMap;
use std::ops::Range;

use dereth_gfx::{
    Buffer, BufferDesc, BufferUsage, Device, Format, Location, Texture as GpuTexture, TextureDesc,
    TextureUsage,
};
use glam::{Mat4, Vec3};

use super::pool::Pool;
use crate::assets::terrain::{cell_corners, Splat};
use crate::assets::{
    AssetKey, AssetSink, AssetTables, BlendFactor, Kind, Material, MaterialKey, Mesh, MeshKey,
    SurfaceRef, TerrainSurface, Texture, TextureFormat, TextureKey,
};
use crate::scene::{
    Block, BlockId, BlockLevels, Object, ObjectKey, Part, Placement, PlacementKind, Terrain,
};

/// No index: no block, no override, no texture.
pub const NONE: u32 = u32::MAX;

/// How many rooms can be held at once, each with its own key in the material target's two
/// bytes; the last value is left for a room without one.
pub const ROOM_KEYS: u32 = 0xFFFF;

/// The light classes, as `shaders/retail.slang` names them.
pub mod class {
    pub const GROUND: u32 = 1;
    pub const SUN: u32 = 2;
    pub const INDOOR: u32 = 3;
    pub const CELL: u32 = 4;
    pub const SKY: u32 = 5;
}

/// The material flags `shaders/gbuffer.slang` reads.
pub mod flag {
    pub const ALPHA_TEST: u32 = 1;
    pub const TRANSLUCENT: u32 = 2;
    pub const ADDITIVE: u32 = 4;
    pub const FOGGED: u32 = 8;
    pub const Z_WRITE: u32 = 16;
    pub const BLENDED: u32 = 32;
    pub const TEXTURED: u32 = 64;
    /// It adds its color weighted by its alpha, or by one less its alpha.
    pub const ADDS_BY_ALPHA: u32 = 128;
    pub const ADDS_BY_INVERSE_ALPHA: u32 = 256;
    /// It is drawn under its vertices' colors.
    pub const VERTEX_COLORED: u32 = 512;
}

/// The item flags: the texture repeats; a building's shell, under the building detail texture; a
/// room's walls, under the rooms' detail texture.
pub const ITEM_TILED: u32 = 1;
pub const ITEM_BUILDING: u32 = 2;
pub const ITEM_ROOM: u32 = 4;

/// An instance's bytes: seven rows of sixteen.
pub const INSTANCE_BYTES: u32 = 128;

/// The most particles a frame draws, and the most draw items they make. Past them a frame's
/// farthest particles are left out.
pub const PARTICLE_INSTANCES: u32 = 4096;
pub const PARTICLE_ITEMS: u32 = 8192;

/// A draw item's bucket: which pass and pipeline draws it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Bucket {
    /// The G-buffer pass, front faces culled away.
    Opaque = 0,
    /// The G-buffer pass, both faces drawn.
    OpaqueTwoSided = 1,
    /// The forward pass: blended over what is drawn.
    Blended = 2,
    /// The forward pass: added to what is drawn.
    Additive = 3,
}

/// The buckets the cull fills.
pub const CULLED_BUCKETS: u32 = 4;

/// The buckets one group of the G-buffer's items takes: the outdoors', and, when the viewer
/// stands inside, the interior's after it.
pub const GROUP_BUCKETS: u32 = 2;

/// Whether an opening is a placeholder, every corner on one side of a 24-unit square around the
/// cell's own origin: retail never stamps one.
#[must_use]
pub fn placeholder_opening(polygon: &[Vec3]) -> bool {
    !polygon.is_empty()
        && [
            |v: &Vec3| v.x == 12.0,
            |v: &Vec3| v.x == -12.0,
            |v: &Vec3| v.y == 12.0,
            |v: &Vec3| v.y == -12.0,
        ]
        .iter()
        .any(|side| polygon.iter().all(side))
}

/// A pool's buffer, its elements of `stride` bytes handed out in runs.
#[derive(Debug)]
pub struct Pooled {
    pub buffer: Buffer,
    pub index: u32,
    pub pool: Pool,
    stride: u32,
    usage: BufferUsage,
    label: &'static str,
    /// Buffers outgrown, kept until the next recording has copied them.
    pub retired: Vec<Buffer>,
}

impl Pooled {
    fn new(
        d: &mut Device,
        label: &'static str,
        capacity: u32,
        stride: u32,
        usage: BufferUsage,
    ) -> Result<Self, dereth_gfx::Error> {
        let usage = usage | BufferUsage::STORAGE | BufferUsage::TRANSFER_SRC;
        let buffer = d.create_buffer(&BufferDesc {
            label,
            size: u64::from(capacity) * u64::from(stride),
            usage,
            location: Location::Gpu,
        })?;
        let index = d.bind_buffer(buffer)?;
        Ok(Self {
            buffer,
            index,
            pool: Pool::new(capacity),
            stride,
            usage,
            label,
            retired: Vec::new(),
        })
    }

    /// The bytes its buffer takes on the device.
    #[must_use]
    pub fn bytes(&self) -> u64 {
        u64::from(self.pool.capacity()) * u64::from(self.stride)
    }

    /// A run of `len` elements, the pool grown to fit when it must.
    fn take(&mut self, d: &mut Device, len: u32) -> Result<Range<u32>, dereth_gfx::Error> {
        if let Some(r) = self.pool.take(len) {
            return Ok(r);
        }
        let old = self.pool.capacity();
        let capacity = (old * 2).max(old + len).next_power_of_two();
        let bigger = d.create_buffer(&BufferDesc {
            label: self.label,
            size: u64::from(capacity) * u64::from(self.stride),
            usage: self.usage,
            location: Location::Gpu,
        })?;
        d.copy_buffer_later(self.buffer, bigger, u64::from(old) * u64::from(self.stride))?;
        self.retired
            .push(std::mem::replace(&mut self.buffer, bigger));
        self.index = d.bind_buffer(self.buffer)?;
        self.pool.grow(capacity);
        self.pool
            .take(len)
            .ok_or_else(|| dereth_gfx::Error::Invalid("a pool that grew still has no room".into()))
    }

    /// Write `bytes` at element `at`.
    fn write(&self, d: &mut Device, at: u32, bytes: &[u8]) -> Result<(), dereth_gfx::Error> {
        d.write_buffer(self.buffer, u64::from(at) * u64::from(self.stride), bytes)
    }
}

/// A mesh in the pools.
#[derive(Debug, Clone)]
pub struct MeshGpu {
    pub vertices: Range<u32>,
    pub indices: Range<u32>,
    pub subsets: Vec<crate::assets::Subset>,
    pub sphere: Option<([f32; 3], f32)>,
    pub sort_center: [f32; 3],
    /// Its vertices' lowest and highest corner, its own space.
    pub bounds: ([f32; 3], [f32; 3]),
}

/// A material: its row's index, and what the passes ask of it.
#[derive(Debug, Clone)]
pub struct MaterialGpu {
    pub index: u32,
    pub blended: bool,
    pub alpha_test: bool,
    /// It is added onto what is behind it: retail's blend for it adds the destination whole.
    pub additive: bool,
    /// Its maps' place in the maps table, when it has maps.
    pub maps: Option<u32>,
    /// Its picture's place in the table (`NONE` when it has none or it is not held).
    pub picture: u32,
}

/// One item as the processor sees it: what a command for it needs, and whether it is a blended
/// cut-out, whose partly transparent texels the edge pass draws; and, for the acceleration
/// structures, its mesh's first vertex, its material's row and whether it is alpha-tested.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ItemDraw {
    pub index_count: u32,
    pub first_index: u32,
    pub two_sided: bool,
    pub edge: bool,
    pub vertex_base: u32,
    pub material: u32,
    pub alpha_test: bool,
}

/// One sky object's instance and draw items.
#[derive(Debug, Clone)]
pub struct SkyGpu {
    pub instance: u32,
    pub items: Range<u32>,
}

/// One block's ground on the device.
#[derive(Debug, Clone)]
pub struct GroundGpu {
    pub vertices: Range<u32>,
    pub cells: Range<u32>,
    pub side: u32,
    /// Its lowest and highest heights, block-local.
    pub z: [f32; 2],
    /// Each drawn cell's cut, kept so its cells can be written again.
    pub cuts: Vec<bool>,
    /// The block's terrain words, kept so its cells can be written again and the weather can read
    /// its land.
    pub words: [u16; 81],
}

/// The bytes of one mesh vertex: its position, normal and texture coordinates, then its
/// tangent and the way its second texture coordinate runs, then its color.
pub const VERTEX_BYTES: u32 = 64;

/// The rows of one splat in the device's table: the base, each overlay, then the maps of the base
/// and of each overlay, what each lays on steep ground, and what a pack lays by the water.
#[allow(clippy::cast_possible_truncation)] // LINT-OK: five overlays.
pub const SPLAT_ROWS: u32 = 4 * (1 + crate::assets::terrain::SPLAT_OVERLAYS as u32) + 3;

/// One placed part's draw items, by level.
#[derive(Debug, Clone, Default)]
pub struct PartItems {
    /// The part's instance.
    pub instance: u32,
    /// Its degrade record in its block, when it degrades.
    pub degrade: Option<u32>,
    /// The items of each level, one range a level.
    pub levels: Vec<Range<u32>>,
    /// The instance's sphere in its own space, for its light set indoors.
    pub sphere: ([f32; 3], f32),
    /// Its transform, block-local or in the frame's space, its scale applied.
    pub transform: Mat4,
    /// Its scale, per axis.
    pub scale: Vec3,
    /// The instance its levels that turn toward the viewer draw with, when any level does: it
    /// is turned frame by frame while such a level is drawn, and the part's own instance, which
    /// its shadow is cast with, stays as placed.
    pub turned: Option<u32>,
    /// How far its placement is drawn above where it stands, on the near ground and the far
    /// (see [`crate::scene::Placement::lifts`]).
    pub lifts: [f32; 2],
}

/// One room on the device.
#[derive(Debug, Clone, Default)]
pub struct RoomGpu {
    pub id: u32,
    /// The key its walls carry in the material target, unique among the rooms held, below
    /// [`ROOM_KEYS`]; [`NONE`] when every key is taken.
    pub key: u32,
    pub walls: PartItems,
    pub statics: Vec<PartItems>,
    /// Its openings to the outdoors, block-local: each one's polygon.
    pub openings: Vec<Vec<Vec3>>,
}

/// One block on the device.
#[derive(Debug, Clone, Default)]
pub struct BlockGpu {
    pub slot: u32,
    pub generation: u32,
    pub instances: Range<u32>,
    pub items: Range<u32>,
    pub parts: Vec<PartItems>,
    pub rooms: Vec<RoomGpu>,
    /// Each degrade record's level, as the frames last said.
    pub levels: Vec<u8>,
    /// The degrade records of the block's own placements, then each room's.
    pub degrades: Vec<Vec<u32>>,
}

/// One object on the device.
#[derive(Debug, Clone, Default)]
pub struct ObjectGpu {
    pub instances: Range<u32>,
    pub items: Range<u32>,
    pub parts: Vec<PartItems>,
}

/// The retained scene on the device.
#[derive(Debug)]
pub struct GpuScene {
    pub vertices: Pooled,
    pub indices: Pooled,
    pub instances: Pooled,
    pub items: Pooled,
    pub materials: Pooled,
    /// The materials' maps: a normal map, an occlusion-roughness-metalness map and the normal's
    /// strength, for the materials that have them.
    pub material_maps: Pooled,
    pub ground_vertices: Pooled,
    pub ground_cells: Pooled,
    /// The ground's splats, [`SPLAT_ROWS`] rows each, and each one's place by its key.
    pub splats: Pooled,
    pub splat_index: BTreeMap<u32, u32>,
    pub meshes: BTreeMap<MeshKey, MeshGpu>,
    pub material_index: BTreeMap<MaterialKey, MaterialGpu>,
    pub textures: BTreeMap<TextureKey, (GpuTexture, u32)>,
    /// Each texture's bytes, every level.
    texture_bytes: BTreeMap<TextureKey, u64>,
    pub ground: BTreeMap<BlockId, GroundGpu>,
    pub blocks: BTreeMap<BlockId, BlockGpu>,
    pub objects: BTreeMap<ObjectKey, ObjectGpu>,
    /// The block offsets' slots.
    pub slots: Pool,
    /// The rooms' keys.
    pub room_keys: Pool,
    /// The bucket of every item, by its index.
    pub buckets: Vec<Bucket>,
    /// Every item as the processor sees it, by its index.
    pub item_draws: Vec<ItemDraw>,
    /// The sky's objects, by their id.
    pub sky: BTreeMap<u32, SkyGpu>,
    /// Counts the changes to what is resident: a block or its ground in, changed or out.
    pub static_generation: u64,
    /// The instances and draw items the frame's particles are written into, anew each frame.
    pub particle_instances: Range<u32>,
    pub particle_items: Range<u32>,
    /// Ground cells and terrain words held back until the block they belong to arrives.
    pending_ground: BTreeMap<BlockId, TerrainSurface>,
    /// Bytes written to the device by this scene, cumulative.
    pub uploaded: u64,
    /// Quarter turns added to every ground cell's own, for the checks that a cell's pictures and
    /// their maps turn with it.
    pub turn: u8,
}

/// The item's material, flags and bucket for a subset with material `m`.
fn bucket_of(m: &MaterialGpu, two_sided: bool) -> Bucket {
    if m.additive {
        Bucket::Additive
    } else if m.blended && !m.alpha_test {
        Bucket::Blended
    } else if two_sided {
        Bucket::OpaqueTwoSided
    } else {
        Bucket::Opaque
    }
}

fn mat_bytes(m: Mat4) -> Vec<u8> {
    m.to_cols_array()
        .iter()
        .flat_map(|f| f.to_le_bytes())
        .collect()
}

fn words(w: &[u32]) -> Vec<u8> {
    w.iter().flat_map(|x| x.to_le_bytes()).collect()
}

fn kind_index(k: Kind) -> u32 {
    match k {
        Kind::Ground => 0,
        Kind::Foliage => 1,
        Kind::Character => 2,
        Kind::Item => 3,
        Kind::Architecture => 4,
        Kind::Effect => 5,
        Kind::Sky => 6,
        Kind::Water => 7,
    }
}

impl GpuScene {
    /// An empty scene, its pools made at sizes a resident window of blocks fits in.
    ///
    /// The pools start small, sized for a town's few blocks, and grow by doubling as the scene
    /// does: what they hold follows what the scene holds.
    ///
    /// # Errors
    /// A buffer the device would not make.
    pub fn new(d: &mut Device) -> Result<Self, dereth_gfx::Error> {
        let mut s = Self {
            vertices: Pooled::new(
                d,
                "remaster vertices",
                1 << 17,
                VERTEX_BYTES,
                BufferUsage::ACCELERATION_INPUT,
            )?,
            indices: Pooled::new(
                d,
                "remaster indices",
                1 << 19,
                4,
                BufferUsage::INDEX | BufferUsage::ACCELERATION_INPUT,
            )?,
            instances: Pooled::new(
                d,
                "remaster instances",
                1 << 14,
                INSTANCE_BYTES,
                BufferUsage::default(),
            )?,
            items: Pooled::new(d, "remaster items", 1 << 15, 32, BufferUsage::default())?,
            materials: Pooled::new(d, "remaster materials", 1 << 13, 32, BufferUsage::default())?,
            material_maps: Pooled::new(
                d,
                "remaster material maps",
                1 << 8,
                16,
                BufferUsage::default(),
            )?,
            ground_vertices: Pooled::new(
                d,
                "remaster ground",
                1 << 14,
                32,
                BufferUsage::ACCELERATION_INPUT,
            )?,
            ground_cells: Pooled::new(
                d,
                "remaster ground cells",
                1 << 14,
                16,
                BufferUsage::default(),
            )?,
            splats: Pooled::new(
                d,
                "remaster ground splats",
                1 << 9,
                SPLAT_ROWS * 16,
                BufferUsage::default(),
            )?,
            splat_index: BTreeMap::new(),
            meshes: BTreeMap::new(),
            material_index: BTreeMap::new(),
            textures: BTreeMap::new(),
            texture_bytes: BTreeMap::new(),
            ground: BTreeMap::new(),
            blocks: BTreeMap::new(),
            objects: BTreeMap::new(),
            slots: Pool::new(1 << 12),
            room_keys: Pool::new(ROOM_KEYS),
            buckets: Vec::new(),
            item_draws: Vec::new(),
            sky: BTreeMap::new(),
            static_generation: 0,
            particle_instances: 0..0,
            particle_items: 0..0,
            pending_ground: BTreeMap::new(),
            uploaded: 0,
            turn: 0,
        };
        s.particle_instances = s.instances.take(d, PARTICLE_INSTANCES)?;
        s.particle_items = s.items.take(d, PARTICLE_ITEMS)?;
        Ok(s)
    }

    /// Write the frame's own draw items from `first`, `rows` of them, each with its bucket and
    /// what a command for it needs.
    ///
    /// # Errors
    /// A buffer the device would not write.
    pub fn write_frame_items(
        &mut self,
        d: &mut Device,
        first: u32,
        rows: &[(Vec<u8>, Bucket, ItemDraw)],
    ) -> Result<u64, dereth_gfx::Error> {
        let mut bytes = Vec::new();
        for (k, (row, bucket, draw)) in rows.iter().enumerate() {
            bytes.extend_from_slice(row);
            self.set_bucket(first + u32::try_from(k).unwrap_or(0), *bucket, *draw);
        }
        if !bytes.is_empty() {
            self.items.write(d, first, &bytes)?;
        }
        Ok(bytes.len() as u64)
    }

    /// One draw item's row: its instance, its mesh's first vertex, its index count, its material,
    /// its flags, its first index and its bucket.
    #[must_use]
    pub fn item_row(
        instance: u32,
        mesh: &MeshGpu,
        subset: &crate::assets::Subset,
        material: u32,
        bucket: Bucket,
    ) -> Vec<u8> {
        let flags = if subset.tiled { ITEM_TILED } else { 0 };
        words(&[
            instance,
            mesh.vertices.start,
            subset.index_count,
            material,
            flags,
            0,
            mesh.indices.start + subset.first_index,
            bucket as u32,
        ])
    }

    /// Every buffer outgrown, to be destroyed once a recording has copied it.
    pub fn take_retired(&mut self) -> Vec<Buffer> {
        let mut out = Vec::new();
        for p in [
            &mut self.vertices,
            &mut self.indices,
            &mut self.instances,
            &mut self.items,
            &mut self.materials,
            &mut self.material_maps,
            &mut self.ground_vertices,
            &mut self.ground_cells,
            &mut self.splats,
        ] {
            out.append(&mut p.retired);
        }
        out
    }

    /// The asset tables' new entries taken to the device, and the entries let go released.
    pub fn upload(&mut self, d: &mut Device, tables: &mut AssetTables) {
        let mut sink = Sink { d, scene: self };
        tables.upload(&mut sink);
    }

    fn set_bucket(&mut self, item: u32, b: Bucket, draw: ItemDraw) {
        let i = item as usize;
        if self.buckets.len() <= i {
            self.buckets.resize(i + 1, Bucket::Opaque);
            self.item_draws.resize(i + 1, ItemDraw::default());
        }
        self.buckets[i] = b;
        self.item_draws[i] = draw;
    }

    /// Write one instance's row.
    ///
    /// # Errors
    /// What the device refuses.
    pub fn write_instance(
        &mut self,
        d: &mut Device,
        i: u32,
        row: &[u8],
    ) -> Result<(), dereth_gfx::Error> {
        self.instances.write(d, i, row)
    }

    /// The draw items of instance `instance` drawing `mesh`, with the material of each subset
    /// from `material`, written into `items` from `at`; the next free item.
    fn write_items(
        &mut self,
        d: &mut Device,
        instance: u32,
        mesh: &MeshGpu,
        material: &dyn Fn(usize, SurfaceRef) -> Option<MaterialKey>,
        at: u32,
        extra_flags: u32,
    ) -> Result<u32, dereth_gfx::Error> {
        let mut next = at;
        let mut bytes = Vec::new();
        for (k, s) in mesh.subsets.iter().enumerate() {
            let key = material(k, s.surface);
            let Some(m) = key.and_then(|key| self.material_index.get(&key)).cloned() else {
                continue;
            };
            let bucket = bucket_of(&m, s.two_sided);
            let flags = extra_flags | if s.tiled { ITEM_TILED } else { 0 };
            bytes.extend(words(&[
                instance,
                mesh.vertices.start,
                s.index_count,
                m.index,
                flags,
                0,
                mesh.indices.start + s.first_index,
                bucket as u32,
            ]));
            self.set_bucket(
                next,
                bucket,
                ItemDraw {
                    index_count: s.index_count,
                    first_index: mesh.indices.start + s.first_index,
                    two_sided: s.two_sided,
                    edge: m.blended && m.alpha_test,
                    vertex_base: mesh.vertices.start,
                    material: m.index,
                    alpha_test: m.alpha_test,
                },
            );
            next += 1;
        }
        self.items.write(d, at, &bytes)?;
        self.uploaded += bytes.len() as u64;
        Ok(next)
    }

    /// How many items `mesh` makes.
    fn item_count(&self, mesh: &MeshGpu) -> u32 {
        u32::try_from(mesh.subsets.len()).unwrap_or(u32::MAX)
    }

    /// One instance's row: its transform, its per-axis scale's inverse square, its block slot,
    /// light class, light set and override.
    #[allow(clippy::too_many_arguments)]
    pub fn instance_bytes(
        transform: Mat4,
        scale: Vec3,
        slot: u32,
        class: u32,
        set: u32,
        override_: u32,
    ) -> Vec<u8> {
        Self::instance_bytes_scrolled(transform, scale, slot, class, set, override_, [0.0; 2])
    }

    /// [`Self::instance_bytes`] for a part fading from one level into another: how far (0 not
    /// fading), the level it fades from and the level it fades into.
    #[must_use]
    pub fn instance_bytes_faded(
        transform: Mat4,
        scale: Vec3,
        slot: u32,
        class: u32,
        set: u32,
        override_: u32,
        (fade, lower, next): (f32, u8, u8),
    ) -> Vec<u8> {
        let mut b = Self::instance_bytes(transform, scale, slot, class, set, override_);
        super::lod::fade_row(&mut b, fade, lower, next);
        b
    }

    /// [`Self::instance_bytes`] with the texture scrolled by `uv`.
    #[allow(clippy::too_many_arguments)]
    pub fn instance_bytes_scrolled(
        transform: Mat4,
        scale: Vec3,
        slot: u32,
        class: u32,
        set: u32,
        override_: u32,
        uv: [f32; 2],
    ) -> Vec<u8> {
        let mut b = mat_bytes(transform);
        let uniform = (scale.x - scale.y).abs() < 1e-6 && (scale.x - scale.z).abs() < 1e-6;
        let inv = |s: f32| if s.abs() > 1e-12 { 1.0 / (s * s) } else { 1.0 };
        for f in [
            inv(scale.x),
            inv(scale.y),
            inv(scale.z),
            if uniform { 0.0 } else { 1.0 },
        ] {
            b.extend_from_slice(&f.to_le_bytes());
        }
        b.extend(words(&[slot, class, set, override_]));
        for f in [uv[0], uv[1], 0.0, 0.0] {
            b.extend_from_slice(&f.to_le_bytes());
        }
        // How far it is drawn above where it stands: nought until `lift_row` says.
        b.extend_from_slice(&[0u8; 16]);
        b
    }

    /// Write into an instance's row how far its placement is drawn above where it stands, on the
    /// near ground and the far.
    pub fn lift_row(row: &mut [u8], lifts: [f32; 2]) {
        let at = 7 * 16;
        if row.len() >= at + 8 {
            row[at..at + 4].copy_from_slice(&lifts[0].to_le_bytes());
            row[at + 4..at + 8].copy_from_slice(&lifts[1].to_le_bytes());
        }
    }

    /// Whether level `k` of a part's degrade record turns toward the viewer.
    fn level_turns(block: &Block, part: &Part, k: usize) -> bool {
        part.degrade
            .and_then(|d| block.degrades.get(d as usize))
            .and_then(|r| r.modes.get(k))
            .is_some_and(|m| (2..=5).contains(m))
    }

    /// Whether any level of a part's degrade record turns toward the viewer.
    fn part_turns(block: &Block, part: &Part) -> bool {
        part.degrade
            .and_then(|d| block.degrades.get(d as usize))
            .is_some_and(|r| r.modes.iter().any(|m| (2..=5).contains(m)))
    }

    /// The meshes a part's levels draw: its own GfxObj, or each level of its degrade record.
    fn part_levels(block: &Block, part: &Part) -> Vec<u32> {
        match part.degrade {
            Some(d) => block
                .degrades
                .get(d as usize)
                .map_or_else(Vec::new, |r| r.levels.clone()),
            None => vec![part.gfxobj],
        }
    }

    /// Hold a block: its ground, its placements' and rooms' instances and draw items. A block
    /// held already is replaced.
    ///
    /// # Errors
    /// A buffer the device would not make or write.
    pub fn block_in(
        &mut self,
        d: &mut Device,
        tables: &AssetTables,
        block: &Block,
    ) -> Result<(), dereth_gfx::Error> {
        self.block_out(block.block);
        let slot = self.slots.take(1).map_or(NONE, |r| r.start);
        self.ground_in(d, tables, block.block, &block.terrain)?;

        // The parts, every level each, and the rooms' walls and statics.
        let room_statics: Vec<(usize, &Placement)> = block
            .rooms
            .iter()
            .enumerate()
            .flat_map(|(r, room)| room.placements.iter().map(move |p| (r, p)))
            .collect();
        let mut instance_count = 0u32;
        let mut item_count = 0u32;
        let mesh_of = |id: u32| self.meshes.get(&MeshKey::GfxObj(id));
        for p in block
            .placements
            .iter()
            .chain(room_statics.iter().map(|(_, p)| *p))
        {
            for part in &p.parts {
                instance_count += 1 + u32::from(Self::part_turns(block, part));
                for g in Self::part_levels(block, part) {
                    if let Some(m) = mesh_of(g) {
                        item_count += self.item_count(m);
                    }
                }
            }
        }
        for room in &block.rooms {
            instance_count += 1;
            if let Some(m) = self
                .meshes
                .get(&MeshKey::CellPiece(crate::assets::CellPieceKey {
                    environment: room.environment,
                    structure: room.structure,
                }))
            {
                item_count += self.item_count(m);
            }
        }
        let instances = self.instances.take(d, instance_count)?;
        let items = self.items.take(d, item_count)?;
        let mut gpu = BlockGpu {
            slot,
            generation: block.generation,
            instances: instances.clone(),
            items: items.clone(),
            levels: vec![0; block.degrades.len()],
            degrades: block.degrades.iter().map(|r| r.levels.clone()).collect(),
            ..BlockGpu::default()
        };
        let mut inst = instances.start;
        let mut item = items.start;
        let mut instance_rows = Vec::new();
        let mut place = |scene: &mut Self,
                         d: &mut Device,
                         (part, lifts): (&Part, [f32; 2]),
                         class: u32,
                         flags: u32,
                         instance_rows: &mut Vec<u8>|
         -> Result<PartItems, dereth_gfx::Error> {
            let this = inst;
            inst += 1;
            let transform = part.transform();
            let row = || {
                let mut r = Self::instance_bytes(transform, part.scale, slot, class, 0, NONE);
                Self::lift_row(&mut r, lifts);
                r
            };
            instance_rows.extend(row());
            let turned = Self::part_turns(block, part).then(|| {
                let t = inst;
                inst += 1;
                instance_rows.extend(row());
                t
            });
            let mut levels = Vec::new();
            let mut sphere = ([0.0; 3], 0.0);
            for (k, g) in Self::part_levels(block, part).into_iter().enumerate() {
                let start = item;
                if let Some(m) = scene.meshes.get(&MeshKey::GfxObj(g)).cloned() {
                    if k == 0 {
                        sphere = m.sphere.unwrap_or((m.sort_center, 0.0));
                    }
                    let owner = match turned {
                        Some(t) if Self::level_turns(block, part, k) => t,
                        _ => this,
                    };
                    item = scene.write_items(
                        d,
                        owner,
                        &m,
                        &|_, s| match s {
                            SurfaceRef::Surface(id) => Some(MaterialKey::plain(id)),
                            SurfaceRef::Missing => Some(MaterialKey::plain(0)),
                            SurfaceRef::Slot(_) => None,
                        },
                        start,
                        flags | (u32::try_from(k).unwrap_or(0).min(0xFF) << 8),
                    )?;
                }
                levels.push(start..item);
            }
            Ok(PartItems {
                instance: this,
                degrade: part.degrade,
                levels,
                sphere,
                transform,
                scale: part.scale,
                turned,
                lifts,
            })
        };
        for p in &block.placements {
            let flags = if p.kind == PlacementKind::Building {
                ITEM_BUILDING
            } else {
                0
            };
            for part in &p.parts {
                let items = place(
                    self,
                    d,
                    (part, p.lifts),
                    class::SUN,
                    flags,
                    &mut instance_rows,
                )?;
                gpu.parts.push(items);
            }
        }
        for room in &block.rooms {
            let mut r = RoomGpu {
                id: room.id,
                key: self.room_keys.take(1).map_or(NONE, |k| k.start),
                openings: room
                    .portals
                    .iter()
                    .filter(|p| p.other.is_none() && !placeholder_opening(&p.polygon))
                    .map(|p| {
                        p.polygon
                            .iter()
                            .map(|v| room.frame.transform_point3(*v))
                            .collect()
                    })
                    .collect(),
                ..RoomGpu::default()
            };
            for p in &room.placements {
                for part in &p.parts {
                    r.statics.push(place(
                        self,
                        d,
                        (part, [0.0; 2]),
                        class::INDOOR,
                        0,
                        &mut instance_rows,
                    )?);
                }
            }
            gpu.rooms.push(r);
        }
        // The rooms' own walls: each room's Environment piece, its surfaces by the room's slots.
        for (k, room) in block.rooms.iter().enumerate() {
            let this = inst;
            inst += 1;
            instance_rows.extend(Self::instance_bytes(
                room.frame,
                Vec3::ONE,
                slot,
                class::CELL,
                gpu.rooms[k].key.min(ROOM_KEYS),
                NONE,
            ));
            let start = item;
            let key = MeshKey::CellPiece(crate::assets::CellPieceKey {
                environment: room.environment,
                structure: room.structure,
            });
            if let Some(m) = self.meshes.get(&key).cloned() {
                let slots = tables.cell_materials(room.id).map(<[MaterialKey]>::to_vec);
                item = self.write_items(
                    d,
                    this,
                    &m,
                    &|_, s| match s {
                        SurfaceRef::Slot(n) => {
                            slots.as_ref().and_then(|v| v.get(usize::from(n)).cloned())
                        }
                        SurfaceRef::Surface(id) => Some(MaterialKey::plain(id)),
                        SurfaceRef::Missing => Some(MaterialKey::plain(0)),
                    },
                    start,
                    ITEM_ROOM,
                )?;
            }
            gpu.rooms[k].walls = PartItems {
                instance: this,
                degrade: None,
                levels: std::iter::once(start..item).collect(),
                sphere: ([0.0; 3], 0.0),
                transform: room.frame,
                scale: Vec3::ONE,
                turned: None,
                lifts: [0.0; 2],
            };
        }
        self.instances.write(d, instances.start, &instance_rows)?;
        self.uploaded += instance_rows.len() as u64;
        self.blocks.insert(block.block, gpu);
        Ok(())
    }

    /// Let a block go: its instances, items, ground and slot.
    pub fn block_out(&mut self, id: BlockId) {
        if let Some(b) = self.blocks.remove(&id) {
            self.static_generation += 1;
            self.instances.pool.give(b.instances);
            self.items.pool.give(b.items);
            if b.slot != NONE {
                self.slots.give(b.slot..b.slot + 1);
            }
            for r in b.rooms.iter().filter(|r| r.key != NONE) {
                self.room_keys.give(r.key..r.key + 1);
            }
        }
        self.ground_out(id);
    }

    /// Let a block's ground go.
    fn ground_out(&mut self, id: BlockId) {
        if let Some(g) = self.ground.remove(&id) {
            self.static_generation += 1;
            self.ground_vertices.pool.give(g.vertices);
            self.ground_cells.pool.give(g.cells);
        }
    }

    /// Each pool's name, the bytes of it in use, and the bytes its buffer takes.
    #[must_use]
    pub fn pool_bytes(&self) -> Vec<(&'static str, u64, u64)> {
        self.pools()
            .iter()
            .map(|p| {
                (
                    p.label,
                    u64::from(p.pool.used()) * u64::from(p.stride),
                    p.bytes(),
                )
            })
            .collect()
    }

    /// Every pool.
    fn pools(&self) -> [&Pooled; 9] {
        [
            &self.vertices,
            &self.indices,
            &self.instances,
            &self.items,
            &self.materials,
            &self.material_maps,
            &self.ground_vertices,
            &self.ground_cells,
            &self.splats,
        ]
    }

    /// The bytes the scene holds on the device: every pool's buffer, whole, and every texture.
    #[must_use]
    pub fn resident_bytes(&self) -> u64 {
        self.pools().iter().map(|p| p.bytes()).sum::<u64>()
            + self.texture_bytes.values().sum::<u64>()
    }

    /// One ground cell's four words: its composite picture's index, its quarter turns, its cut,
    /// kind and how readily it gathers puddles (from its corners' terrain words, `corners`)
    /// together, and its splat's place in the table; each picture `NONE` when it is not held.
    fn cell_words(
        &self,
        cell: Option<&crate::assets::TerrainCell>,
        cut: bool,
        corners: [u16; 4],
    ) -> [u32; 4] {
        let texture = cell
            .and_then(|c| self.textures.get(&c.texture))
            .map_or(NONE, |(_, i)| *i);
        let splat = cell
            .and_then(|c| match c.texture {
                TextureKey::Terrain(k) => self
                    .splat_index
                    .get(&crate::assets::terrain::splat_key(k))
                    .copied(),
                _ => None,
            })
            .unwrap_or(NONE);
        let rotation = cell.map_or(0, |c| u32::from(c.rotation.wrapping_add(self.turn) & 3));
        let kind = cell.map_or(0, |c| kind_index(c.kind));
        let puddles = super::weather::cell_puddle_weight(corners);
        [
            texture,
            rotation,
            u32::from(cut) | (kind << 1) | (puddles << 4),
            splat,
        ]
    }

    /// Write every held block's ground cells again, from what the tables hold now: after the
    /// ground changed between its composites and its splats.
    ///
    /// # Errors
    /// A buffer the device would not write.
    pub fn rewrite_ground_cells(
        &mut self,
        d: &mut Device,
        tables: &AssetTables,
    ) -> Result<(), dereth_gfx::Error> {
        let blocks: Vec<BlockId> = self.ground.keys().copied().collect();
        for b in blocks {
            let Some(surface) = tables.terrain(b) else {
                continue;
            };
            let g = &self.ground[&b];
            let start = g.cells.start;
            let side = g.side as usize;
            let mut cb = Vec::with_capacity(g.cuts.len() * 16);
            for (c, cut) in g.cuts.iter().enumerate() {
                let corners = cell_corners(&g.words, side, c / side.max(1), c % side.max(1));
                cb.extend(words(&self.cell_words(surface.cells.get(c), *cut, corners)));
            }
            self.ground_cells.write(d, start, &cb)?;
            self.uploaded += cb.len() as u64;
        }
        Ok(())
    }

    /// Take a block's ground again, alone: its level of detail or its stitching changed, and
    /// nothing else of it did.
    ///
    /// # Errors
    /// A buffer the device would not make or write.
    pub fn ground_changed(
        &mut self,
        d: &mut Device,
        tables: &AssetTables,
        block: BlockId,
        terrain: &Terrain,
    ) -> Result<(), dereth_gfx::Error> {
        self.ground_in(d, tables, block, terrain)
    }

    /// The levels a block's degrading parts draw at, as the frame says.
    pub fn levels(&mut self, l: &BlockLevels) {
        if let Some(b) = self.blocks.get_mut(&l.block) {
            b.levels.clone_from(&l.levels);
        }
    }

    /// Hold a block's ground: its vertices with retail's normals, and each drawn cell's picture,
    /// turn, cut and kind.
    fn ground_in(
        &mut self,
        d: &mut Device,
        tables: &AssetTables,
        block: BlockId,
        terrain: &Terrain,
    ) -> Result<(), dereth_gfx::Error> {
        if let Some(g) = self.ground.remove(&block) {
            self.ground_vertices.pool.give(g.vertices);
            self.ground_cells.pool.give(g.cells);
        }
        let surface = tables
            .terrain(block)
            .cloned()
            .or_else(|| self.pending_ground.remove(&block));
        let Some(surface) = surface else {
            return Ok(());
        };
        let n = u32::try_from(terrain.vertices.len()).unwrap_or(0);
        let side = u32::from(terrain.side);
        let cells = side * side;
        let vertices = self.ground_vertices.take(d, n)?;
        let cell_range = self.ground_cells.take(d, cells)?;
        let mut vb = Vec::with_capacity(terrain.vertices.len() * 32);
        for (k, v) in terrain.vertices.iter().enumerate() {
            let nrm = surface.normals.get(k).copied().unwrap_or([0.0, 0.0, 1.0]);
            for f in [v.x, v.y, v.z, 1.0, nrm[0], nrm[1], nrm[2], 0.0] {
                vb.extend_from_slice(&f.to_le_bytes());
            }
        }
        let cuts: Vec<bool> = (0..cells as usize)
            .map(|c| terrain.cuts.get(c).copied().unwrap_or(false))
            .collect();
        let mut cb = Vec::with_capacity(cells as usize * 16);
        let s = (side as usize).max(1);
        for (c, cut) in cuts.iter().enumerate() {
            let corners = cell_corners(&terrain.words, s, c / s, c % s);
            cb.extend(words(&self.cell_words(surface.cells.get(c), *cut, corners)));
        }
        self.ground_vertices.write(d, vertices.start, &vb)?;
        self.ground_cells.write(d, cell_range.start, &cb)?;
        self.uploaded += (vb.len() + cb.len()) as u64;
        let z = terrain
            .vertices
            .iter()
            .fold([f32::INFINITY, f32::NEG_INFINITY], |z, v| {
                [z[0].min(v.z), z[1].max(v.z)]
            });
        self.ground.insert(
            block,
            GroundGpu {
                vertices,
                cells: cell_range,
                side,
                z,
                cuts,
                words: terrain.words,
            },
        );
        self.static_generation += 1;
        Ok(())
    }

    /// Hold an object: one instance a part, written each frame from its pose, and every level's
    /// draw items.
    ///
    /// # Errors
    /// A buffer the device would not make or write.
    pub fn object_in(
        &mut self,
        d: &mut Device,
        tables: &AssetTables,
        object: &Object,
    ) -> Result<(), dereth_gfx::Error> {
        self.object_out(object.key);
        let parts = u32::try_from(object.parts.len()).unwrap_or(0);
        let mut item_count = 0;
        for p in &object.parts {
            for g in &p.levels {
                if let Some(m) = self.meshes.get(&MeshKey::GfxObj(*g)) {
                    item_count += self.item_count(m);
                }
            }
        }
        let instances = self.instances.take(d, parts)?;
        let items = self.items.take(d, item_count)?;
        let mut gpu = ObjectGpu {
            instances: instances.clone(),
            items: items.clone(),
            parts: Vec::new(),
        };
        let mut item = items.start;
        for (k, p) in object.parts.iter().enumerate() {
            let this = instances.start + u32::try_from(k).unwrap_or(0);
            let mut levels = Vec::new();
            let mut sphere = ([0.0; 3], 0.0);
            for (level, g) in p.levels.iter().enumerate() {
                let start = item;
                if let Some(m) = self.meshes.get(&MeshKey::GfxObj(*g)).cloned() {
                    if level == 0 {
                        sphere = m.sphere.unwrap_or((m.sort_center, 0.0));
                    }
                    let keys = tables
                        .object_materials(object.key, k, level)
                        .map(<[MaterialKey]>::to_vec)
                        .unwrap_or_default();
                    item = self.write_items(
                        d,
                        this,
                        &m,
                        &|subset, s| {
                            keys.get(subset).cloned().or(match s {
                                SurfaceRef::Surface(id) => Some(MaterialKey::plain(id)),
                                _ => None,
                            })
                        },
                        start,
                        0,
                    )?;
                }
                levels.push(start..item);
            }
            gpu.parts.push(PartItems {
                instance: this,
                degrade: None,
                levels,
                sphere,
                transform: Mat4::IDENTITY,
                scale: Vec3::ONE,
                turned: None,
                lifts: [0.0; 2],
            });
        }
        self.objects.insert(object.key, gpu);
        Ok(())
    }

    /// Hold a sky object, once its mesh is held: an instance, written each frame, and its draw
    /// items.
    ///
    /// # Errors
    /// A buffer the device would not make or write.
    pub fn sky_in(&mut self, d: &mut Device, id: u32) -> Result<(), dereth_gfx::Error> {
        if self.sky.contains_key(&id) {
            return Ok(());
        }
        let Some(m) = self.meshes.get(&MeshKey::Sky(id)).cloned() else {
            return Ok(());
        };
        let instance = self.instances.take(d, 1)?.start;
        let items = self.items.take(d, self.item_count(&m))?;
        let end = self.write_items(
            d,
            instance,
            &m,
            &|_, s| match s {
                SurfaceRef::Surface(id) => Some(MaterialKey::plain(id)),
                _ => None,
            },
            items.start,
            0,
        )?;
        self.sky.insert(
            id,
            SkyGpu {
                instance,
                items: items.start..end,
            },
        );
        Ok(())
    }

    /// The picture of the sky's object `id`, its first surface's, by its place in the table.
    #[must_use]
    pub fn sky_picture(&self, id: u32) -> Option<u32> {
        let m = self.meshes.get(&MeshKey::Sky(id))?;
        let SurfaceRef::Surface(surface) = m.subsets.first()?.surface else {
            return None;
        };
        self.material_index
            .get(&MaterialKey::plain(surface))
            .map(|g| g.picture)
            .filter(|p| *p != NONE)
    }

    /// Let an object go.
    pub fn object_out(&mut self, key: ObjectKey) {
        if let Some(o) = self.objects.remove(&key) {
            self.instances.pool.give(o.instances);
            self.items.pool.give(o.items);
        }
    }
}

/// The asset tables' sink: each entry made on the device.
struct Sink<'a> {
    d: &'a mut Device,
    scene: &'a mut GpuScene,
}

impl std::fmt::Debug for Sink<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Sink")
    }
}

pub(crate) fn gpu_format(f: TextureFormat) -> Format {
    match f {
        TextureFormat::Bc1 => Format::Bc1,
        TextureFormat::Bc2 | TextureFormat::Bc2Premultiplied => Format::Bc2,
        TextureFormat::Bc3 | TextureFormat::Bc3Premultiplied => Format::Bc3,
        TextureFormat::Bc5 => Format::Bc5,
        TextureFormat::Bc7 => Format::Bc7,
        _ => Format::Bgra8Unorm,
    }
}

impl Sink<'_> {
    /// One splat's rows: its base's picture, tiling, overlay count and whether it is water, then
    /// each overlay's alpha map, quarter turns (with whether it is water from bit 2), tile and
    /// tiling, every picture by its index (`NONE` when not held).
    fn splat_in(&mut self, key: u32, s: &Splat) -> Result<(), dereth_gfx::Error> {
        let index = match self.scene.splat_index.get(&key) {
            Some(i) => *i,
            None => self.scene.splats.take(self.d, 1)?.start,
        };
        let tex = |k: Option<&TextureKey>| {
            k.and_then(|k| self.scene.textures.get(k))
                .map_or(NONE, |(_, i)| *i)
        };
        let overlays = s.overlays.len().min(crate::assets::terrain::SPLAT_OVERLAYS);
        let water = |g: Option<u32>| u32::from(g.is_some_and(crate::assets::kind::is_water_type));
        let mud = |g: Option<u32>| u32::from(g.is_some_and(crate::assets::kind::is_mud_type));
        // How readily each layer gathers puddles, 0 to 3, by its terrain type.
        let puddles = |g: Option<u32>| g.map_or(0, super::weather::puddle_weight);
        let mapped = s.base_maps.is_some() || s.overlays.iter().any(|o| o.maps.is_some());
        let mut w = vec![
            tex(s.base.as_ref()),
            s.base_tiling.max(1),
            u32::try_from(overlays).unwrap_or(0),
            water(s.base_ground)
                | (u32::from(mapped) << 1)
                | (puddles(s.base_ground) << 2)
                | (u32::from(!s.slopes.is_empty()) << 4)
                | (mud(s.base_ground) << 5)
                | (u32::from(s.bank.is_some()) << 6),
        ];
        for k in 0..crate::assets::terrain::SPLAT_OVERLAYS {
            match s.overlays.get(k) {
                Some(o) => w.extend([
                    tex(Some(&o.alpha)),
                    u32::from(o.rotation & 3)
                        | (water(o.ground) << 2)
                        | (puddles(o.ground) << 3)
                        | (mud(o.ground) << 5),
                    tex(o.tile.as_ref()),
                    o.tiling.max(1),
                ]),
                None => w.extend([NONE, 0, NONE, 1]),
            }
        }
        // The maps' rows: the normal picture, the surface picture and the normal's strength, then
        // the retail picture a pack's tile replaces (its index and one, its repeats from bit 24;
        // nought for none).
        let broad = |b: Option<&crate::assets::terrain::BroadPicture>| {
            b.and_then(|b| {
                let i = tex(Some(&b.picture));
                (i != NONE).then_some((i + 1) | (b.tiling.clamp(1, 255) << 24))
            })
            .unwrap_or(0)
        };
        let maps_row = |m: Option<&crate::assets::terrain::GroundMaps>, b: u32| match m {
            #[allow(clippy::cast_precision_loss)] // thousandths, a few thousand at most
            Some(m) => [
                tex(m.normal.as_ref()),
                tex(m.surface.as_ref()),
                (m.normal_scale as f32 / 1000.0).to_bits(),
                b,
            ],
            None => [NONE, NONE, 1.0f32.to_bits(), 0],
        };
        w.extend(maps_row(s.base_maps.as_ref(), broad(s.base_broad.as_ref())));
        for k in 0..crate::assets::terrain::SPLAT_OVERLAYS {
            let o = s.overlays.get(k);
            w.extend(maps_row(
                o.and_then(|o| o.maps.as_ref()),
                broad(o.and_then(|o| o.broad.as_ref())),
            ));
        }
        // A picture laid by the slope or the water: its picture, its repeats (with its normal's
        // strength in thousandths from bit 8), and its normal and surface pictures.
        let laid = |t: &crate::assets::terrain::SteepTile| {
            let m = t.maps.as_ref();
            [
                tex(Some(&t.tile)),
                t.tiling.clamp(1, 255) | (m.map_or(1000, |m| m.normal_scale.min(0xFFF)) << 8),
                tex(m.and_then(|m| m.normal.as_ref())),
                tex(m.and_then(|m| m.surface.as_ref())),
            ]
        };
        // What each layer lays on steep ground, scree then rock, with whether it is the ground's
        // own at bit 30 of its repeats, and whether the layer is snow at bit 31.
        for rock in [false, true] {
            for k in 0..=crate::assets::terrain::SPLAT_OVERLAYS {
                let layer = s.slopes.get(k);
                let steep = layer.and_then(|l| {
                    if rock {
                        l.rock.as_ref()
                    } else {
                        l.scree.as_ref()
                    }
                });
                match steep {
                    Some(t) => {
                        let mut row = laid(t);
                        row[1] |= (u32::from(t.own) << 30)
                            | (u32::from(layer.is_some_and(|l| l.snow)) << 31);
                        w.extend(row);
                    }
                    None => w.extend([NONE, 1, NONE, NONE]),
                }
            }
        }
        // What a pack lays by the water: the grass, the land (whether it is mud at bit 29 of its
        // repeats) and the bed.
        match s.bank.as_ref() {
            Some(bank) => {
                w.extend(laid(&bank.grass));
                let mut land = laid(&bank.land);
                land[1] |= u32::from(bank.land_is_mud) << 29;
                w.extend(land);
                w.extend(laid(&bank.bed));
            }
            None => {
                for _ in 0..3 {
                    w.extend([NONE, 1, NONE, NONE]);
                }
            }
        }
        let b = words(&w);
        self.scene.splats.write(self.d, index, &b)?;
        self.scene.uploaded += b.len() as u64;
        self.scene.splat_index.insert(key, index);
        Ok(())
    }

    fn texture_in(&mut self, key: &TextureKey, t: &Texture) -> Result<(), dereth_gfx::Error> {
        let mips = u32::try_from(t.levels.len()).unwrap_or(1).max(1);
        let tex = self.d.create_texture(&TextureDesc {
            label: "remaster picture",
            width: t.width,
            height: t.height,
            mips,
            format: gpu_format(t.format),
            usage: TextureUsage::SAMPLED,
        })?;
        for (k, level) in t.levels.iter().enumerate() {
            self.d
                .upload_texture(tex, u32::try_from(k).unwrap_or(0), level)?;
            self.scene.uploaded += level.len() as u64;
        }
        let index = self.d.bind_texture(tex)?;
        self.scene
            .texture_bytes
            .insert(key.clone(), t.levels.iter().map(|l| l.len() as u64).sum());
        if let Some((old, _)) = self.scene.textures.insert(key.clone(), (tex, index)) {
            self.d.destroy_texture(old);
        }
        Ok(())
    }

    fn material_in(&mut self, key: &MaterialKey, m: &Material) -> Result<(), dereth_gfx::Error> {
        let index = match self.scene.material_index.get(key) {
            Some(g) => g.index,
            None => self.scene.materials.take(self.d, 1)?.start,
        };
        let texture = m
            .texture
            .as_ref()
            .and_then(|t| self.scene.textures.get(t))
            .map_or(NONE, |(_, i)| *i);
        let mut flags = 0;
        if m.alpha_test.is_some() {
            flags |= flag::ALPHA_TEST;
        }
        if m.translucent.is_some() {
            flags |= flag::TRANSLUCENT;
        }
        if m.additive {
            flags |= flag::ADDITIVE;
        }
        if m.fogged {
            flags |= flag::FOGGED;
        }
        if m.z_write {
            flags |= flag::Z_WRITE;
        }
        if m.blended {
            flags |= flag::BLENDED;
        }
        if texture != NONE {
            flags |= flag::TEXTURED;
        }
        if m.vertex_colored {
            flags |= flag::VERTEX_COLORED;
        }
        // Retail's blend, as its surface table works it out, decides whether the surface adds:
        // an additive surface that is also a translucent cut-out is blended over instead.
        let adds = m.blended && m.dst_blend == BlendFactor::One;
        if adds {
            match m.src_blend {
                BlendFactor::SrcAlpha => flags |= flag::ADDS_BY_ALPHA,
                BlendFactor::InvSrcAlpha => flags |= flag::ADDS_BY_INVERSE_ALPHA,
                BlendFactor::One | BlendFactor::Zero => {}
            }
        }
        // Its maps, in the maps table: the normal map's and the roughness map's pictures, the
        // normal's strength and the roughness's factor.
        let held = self.scene.material_index.get(key).and_then(|g| g.maps);
        let maps = match &m.maps {
            Some(mp) => {
                let at = match held {
                    Some(i) => i,
                    None => self.scene.material_maps.take(self.d, 1)?.start,
                };
                let tex = |k: Option<&TextureKey>| {
                    k.and_then(|k| self.scene.textures.get(k))
                        .map_or(NONE, |(_, i)| *i)
                };
                let row = words(&[
                    tex(mp.normal.as_ref()),
                    tex(mp.orm.as_ref()),
                    mp.normal_scale.to_bits(),
                    mp.roughness.clamp(0.0, 1.0).to_bits(),
                ]);
                self.scene.material_maps.write(self.d, at, &row)?;
                Some(at)
            }
            None => {
                if let Some(i) = held {
                    self.scene.material_maps.pool.give(i..i + 1);
                }
                None
            }
        };
        let mut b = words(&[
            texture,
            flags,
            u32::from(m.alpha_test.unwrap_or(0)),
            u32::from(m.vertex_alpha),
        ]);
        #[allow(clippy::cast_precision_loss)] // a kind's index
        for f in [m.luminosity, m.diffuse, kind_index(m.kind) as f32] {
            b.extend_from_slice(&f.to_le_bytes());
        }
        b.extend_from_slice(&maps.unwrap_or(NONE).to_le_bytes());
        self.scene.materials.write(self.d, index, &b)?;
        self.scene.uploaded += b.len() as u64;
        self.scene.material_index.insert(
            key.clone(),
            MaterialGpu {
                index,
                blended: m.blended,
                alpha_test: m.alpha_test.is_some(),
                additive: adds,
                maps,
                picture: texture,
            },
        );
        Ok(())
    }

    fn mesh_in(&mut self, key: MeshKey, m: &Mesh) -> Result<(), dereth_gfx::Error> {
        if let Some(old) = self.scene.meshes.remove(&key) {
            self.scene.vertices.pool.give(old.vertices);
            self.scene.indices.pool.give(old.indices);
        }
        let nv = u32::try_from(m.vertices.len()).unwrap_or(0);
        let ni = u32::try_from(m.indices.len()).unwrap_or(0);
        let vertices = self.scene.vertices.take(self.d, nv)?;
        let indices = self.scene.indices.take(self.d, ni)?;
        let worked;
        let tangents = if m.tangents.len() == m.vertices.len() {
            &m.tangents
        } else {
            worked = crate::assets::tangents(m);
            &worked
        };
        let mut vb = Vec::with_capacity(m.vertices.len() * VERTEX_BYTES as usize);
        for (k, (v, t)) in m.vertices.iter().zip(tangents).enumerate() {
            let c = m.colors.get(k).copied().unwrap_or([1.0; 4]);
            for f in [
                v.position[0],
                v.position[1],
                v.position[2],
                v.normal[0],
                v.normal[1],
                v.normal[2],
                v.uv[0],
                v.uv[1],
                t[0],
                t[1],
                t[2],
                t[3],
                c[0],
                c[1],
                c[2],
                c[3],
            ] {
                vb.extend_from_slice(&f.to_le_bytes());
            }
        }
        self.scene.vertices.write(self.d, vertices.start, &vb)?;
        let ib = words(&m.indices);
        self.scene.indices.write(self.d, indices.start, &ib)?;
        self.scene.uploaded += (vb.len() + ib.len()) as u64;
        let bounds = m.vertices.iter().fold(
            ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]),
            |(lo, hi), v| {
                (
                    [0, 1, 2].map(|k| lo[k].min(v.position[k])),
                    [0, 1, 2].map(|k| hi[k].max(v.position[k])),
                )
            },
        );
        self.scene.meshes.insert(
            key,
            MeshGpu {
                vertices,
                indices,
                subsets: m.subsets.clone(),
                sphere: m.sphere,
                sort_center: m.sort_center,
                bounds,
            },
        );
        Ok(())
    }
}

impl AssetSink for Sink<'_> {
    fn texture(&mut self, key: &TextureKey, texture: &Texture) {
        if let Err(e) = self.texture_in(key, texture) {
            tracing::warn!(target: "dereth::remaster", "a picture was not made: {e}");
        }
    }

    fn material(&mut self, key: &MaterialKey, material: &Material) {
        if let Err(e) = self.material_in(key, material) {
            tracing::warn!(target: "dereth::remaster", "a material was not made: {e}");
        }
    }

    fn mesh(&mut self, key: MeshKey, mesh: &Mesh) {
        if let Err(e) = self.mesh_in(key, mesh) {
            tracing::warn!(target: "dereth::remaster", "a mesh was not made: {e}");
        }
    }

    fn terrain(&mut self, block: BlockId, surface: &TerrainSurface) {
        self.scene.pending_ground.insert(block, surface.clone());
    }

    fn splat(&mut self, key: u32, splat: &Splat) {
        if let Err(e) = self.splat_in(key, splat) {
            tracing::warn!(target: "dereth::remaster", "a ground splat was not made: {e}");
        }
    }

    fn release(&mut self, key: &AssetKey) {
        match key {
            AssetKey::Texture(t) => {
                self.scene.texture_bytes.remove(t);
                if let Some((tex, _)) = self.scene.textures.remove(t) {
                    self.d.destroy_texture(tex);
                }
            }
            AssetKey::Splat(k) => {
                if let Some(i) = self.scene.splat_index.remove(k) {
                    self.scene.splats.pool.give(i..i + 1);
                }
            }
            AssetKey::Material(m) => {
                if let Some(g) = self.scene.material_index.remove(m) {
                    self.scene.materials.pool.give(g.index..g.index + 1);
                    if let Some(i) = g.maps {
                        self.scene.material_maps.pool.give(i..i + 1);
                    }
                }
            }
            AssetKey::Mesh(m) => {
                if let Some(g) = self.scene.meshes.remove(m) {
                    self.scene.vertices.pool.give(g.vertices);
                    self.scene.indices.pool.give(g.indices);
                }
            }
            AssetKey::Terrain(b) => {
                self.scene.pending_ground.remove(b);
            }
        }
    }
}
