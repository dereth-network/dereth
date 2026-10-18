//! The drawn ground: the landscape drawn by the presentation itself, retail's surface with a
//! visual offset near the viewer and the smooth surface with an eroded relief far off, out to the
//! horizon.
//!
//! While the frame carries a drawn ground ([`HifiDrawnGround`]) and an art pack's ground is laid,
//! this pass leaves the landscape's own cells out of the world's replay (the draws between each
//! [`Mark::TerrainCell`] and its [`Mark::TerrainCellEnd`]) and, at the first cell's end, draws
//! the ground itself into the re-shaded targets: its depth, its normal with the landscape's
//! material class, and a neutral color the art pack's ground lays its materials over. The
//! landscape-normal pass then leaves it alone ([`HifiPass::draws_ground`]), and the sun's shadow
//! cascades take it in place of the landscape's triangles ([`DrawnCasters`]).
//!
//! The scene draws everything that stands on the ground moved by the same offset, from the same
//! numbers, so bodies, items and scenery stand on what is drawn. Collision, picking and the
//! server keep retail's surface.
//!
//! **The mesh** is a grid of square patches round the eye, [`CELLS`] quads a side, each level of
//! patches twice the last's spacing ([`select`]); a vertex toward the edge of its level's ring
//! slides onto the next level's grid, so the levels meet without cracks.
//!
//! **The near offset** is a texture of the blocks round the viewer's, each block in the slot its
//! coordinates name, wrapping; a block moves in only when its slot's block leaves.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use glam::{Mat4, Vec2, Vec3};

use crate::build::Build;
use crate::build::Prebuilt;
use crate::graph::WarmCx;
use crate::graph::{EncodeCx, HifiPass, PrepareCx};
use crate::passes::lighting::CASCADES;
use crate::reshade::ReshadePlan;
use crate::shared::camera::HEIGHT_UP;
use crate::snapshot::{HifiDrawnGround, HifiGroundKnobs};
use crate::{
    Caps, DrawAction, DrawNote, HifiError, HifiFrame, HifiSettings, Mark, ReplayFilter, SkipRule,
};

const DRAWN_WGSL: &str = include_str!("drawn.wgsl");

/// The pass's shader, for validation without a device: the drawn ground's, with the water's shore
/// it lowers the bed under.
#[must_use]
pub fn shader_source() -> String {
    format!(
        "{DRAWN_WGSL}
{}",
        crate::passes::water::SHORE_WGSL
    )
}

/// Quads along a patch's side.
pub const CELLS: u32 = 16;
/// Vertices a patch draws: two triangles a quad.
const PATCH_VERTICES: u32 = CELLS * CELLS * 6;
/// Meters along the world's side: 255 blocks.
pub const WORLD_METERS: f32 = 255.0 * 192.0;
/// The level whose nodes the selection starts from: one node of it covers the world.
const TOP: u32 = 11;
/// How far a level's ring reaches, in nodes of its size.
const RANGE_NODES: f32 = 2.5;
/// The share of a level's ring past which its vertices slide onto the next level's grid.
const MORPH_FROM: f32 = 0.8;
/// The most bodies the fine relief is leveled under.
const BODIES: usize = 32;
/// The most near-texture slots: 13 blocks a side.
const SLOTS: usize = 256;
/// The most landscape cells whose buildings stamped their openings the ground is drawn again
/// round, nearest the eye first.
const STAMPED: usize = 64;
/// The bytes of the frame constants, as the shader's `Frame` lays them out.
const FRAME_BYTES: usize =
    64 * 2 + 16 * 6 + 16 * BODIES + 16 * (SLOTS / 4) + 16 + 16 * STAMPED + 16;
/// The stride of the shadow cascades' constant blocks.
const CASTER_STRIDE: u64 = 256;
/// The same, as an index into their bytes.
const CASTER_BLOCK: usize = 256;
/// The bytes of one instance: the patch's corner, spacing and level, and its slide.
const INSTANCE_BYTES: u64 = 32;
/// The same, as an index into their bytes.
const INSTANCE_LEN: usize = 32;
/// How far the sun's shadows take the drawn ground, meters from the eye.
const CASTER_REACH: f32 = 2500.0;
/// The share of the camera's far plane the ground is drawn to: the last stretch before it holds
/// depths a hair from the far plane's own, which the passes after it take for the sky.
const FAR_SHARE: f32 = 0.97;

/// One patch of the drawn ground.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Patch {
    /// Its south-west corner, world meters.
    pub at: Vec2,
    /// Meters between its vertices.
    pub spacing: f32,
    /// Its level: 0 the finest.
    pub level: u32,
    /// Where its vertices start sliding onto the next level's grid, and where they have, meters
    /// from the eye.
    pub morph: (f32, f32),
}

/// The spacing of level 0's grid at `detail`.
#[must_use]
pub fn finest_spacing(detail: f32) -> f32 {
    1.0 / detail.clamp(0.25, 4.0)
}

/// How far level `level`'s ring reaches from the eye at `detail`, meters.
#[must_use]
pub fn reach(level: u32, detail: f32) -> f32 {
    // LINT-OK: a level, at most a dozen.
    #[allow(clippy::cast_precision_loss)]
    let node = finest_spacing(detail) * CELLS as f32 * 2.0 * (1u32 << level.min(20)) as f32;
    node * RANGE_NODES
}

/// How far a patch vertex at world `p` has slid onto the next level's grid, `morph` its patch's
/// slide (where it starts and ends, meters across the ground from the eye at `eye`): the shader's
/// own arithmetic.
#[must_use]
pub fn slid(p: Vec2, morph: (f32, f32), eye: Vec2) -> f32 {
    let d = (p - eye).length();
    ((d - morph.0) / (morph.1 - morph.0).max(1e-3)).clamp(0.0, 1.0)
}

/// Where the vertex `grid` quads from its patch's corner, at world `p` on a grid `spacing` meters
/// apart, stands `k` of the way slid: each odd vertex moves onto its even neighbor to the south or
/// west. The shader's own arithmetic.
#[must_use]
pub fn slide(p: Vec2, grid: Vec2, spacing: f32, k: f32) -> Vec2 {
    let odd = Vec2::new((grid.x * 0.5).fract(), (grid.y * 0.5).fract()) * 2.0;
    p - odd * spacing * k
}

/// Where vertex `grid` of `patch` stands for the eye at world `eye`.
#[must_use]
pub fn vertex_of(patch: &Patch, grid: (u32, u32), eye: Vec2) -> Vec2 {
    // LINT-OK: a vertex index, 0..=16.
    #[allow(clippy::cast_precision_loss)]
    let g = Vec2::new(grid.0 as f32, grid.1 as f32);
    let at = patch.at + g * patch.spacing;
    slide(at, g, patch.spacing, slid(at, patch.morph, eye))
}

/// The patches the ground is drawn with round the eye at world `eye` (meters), at `detail`, out
/// to `horizon` meters, inside the world; `seen` says whether a box (world meters, its least and
/// greatest corners) may be seen, and those it says not are left out.
///
/// A node of level `l` is two patches of that level a side. From the one node that covers the
/// world, each node within the reach of the level below it is taken apart into its four
/// quarters, and each quarter is drawn at the node's own level wherever the level below does not
/// reach it: so neighboring patches are never more than one level apart.
#[must_use]
pub fn select(
    eye: Vec3,
    detail: f32,
    horizon: f32,
    seen: &dyn Fn(Vec3, Vec3) -> bool,
) -> Vec<Patch> {
    let mut s = Selector {
        eye,
        finest: finest_spacing(detail),
        ranges: (0..=TOP).map(|l| reach(l, detail)).collect(),
        horizon,
        seen,
        out: Vec::new(),
    };
    s.walk(Vec2::ZERO, TOP);
    // Nearest first, so the ground nearer the eye hides what lies behind it before that is
    // shaded.
    let eye = eye.truncate();
    let middle = |p: &Patch| {
        // LINT-OK: a patch's quads, 16.
        #[allow(clippy::cast_precision_loss)]
        let half = p.spacing * CELLS as f32 * 0.5;
        (p.at + Vec2::splat(half) - eye).length()
    };
    s.out.sort_by(|a, b| middle(a).total_cmp(&middle(b)));
    s.out
}

struct Selector<'a> {
    eye: Vec3,
    finest: f32,
    ranges: Vec<f32>,
    horizon: f32,
    seen: &'a dyn Fn(Vec3, Vec3) -> bool,
    out: Vec<Patch>,
}

impl Selector<'_> {
    /// The side of a node of level `level`, meters.
    fn node(&self, level: u32) -> f32 {
        // LINT-OK: a level, at most a dozen, and a few dozen quads.
        #[allow(clippy::cast_precision_loss)]
        let n = self.finest * CELLS as f32 * 2.0 * (1u32 << level) as f32;
        n
    }

    /// How far the square from `lo`, `n` a side, lies from the eye across the ground.
    fn distance(&self, lo: Vec2, n: f32) -> f32 {
        Vec2::new(
            (lo.x - self.eye.x).max(0.0).max(self.eye.x - (lo.x + n)),
            (lo.y - self.eye.y).max(0.0).max(self.eye.y - (lo.y + n)),
        )
        .length()
    }

    fn add(&mut self, at: Vec2, level: u32) {
        // LINT-OK: a level, at most a dozen.
        #[allow(clippy::cast_precision_loss)]
        let spacing = self.finest * (1u32 << level) as f32;
        let r = self.ranges[level as usize];
        let morph = if level == TOP {
            (f32::MAX, f32::MAX)
        } else {
            (r * MORPH_FROM, r)
        };
        self.out.push(Patch {
            at,
            spacing,
            level,
            morph,
        });
    }

    /// Take care of the node from `lo` of level `level`: false when it lies past its level's
    /// reach, for the level above to draw.
    fn walk(&mut self, lo: Vec2, level: u32) -> bool {
        let n = self.node(level);
        if lo.x >= WORLD_METERS || lo.y >= WORLD_METERS || lo.x + n <= 0.0 || lo.y + n <= 0.0 {
            return true;
        }
        let d = self.distance(lo, n);
        if d > self.horizon {
            return true;
        }
        if level < TOP && d > self.ranges[level as usize] {
            return false;
        }
        if !(self.seen)(lo.extend(-100.0), (lo + Vec2::splat(n)).extend(1200.0)) {
            return true;
        }
        let half = n * 0.5;
        let quarters = [
            lo,
            lo + Vec2::new(half, 0.0),
            lo + Vec2::new(0.0, half),
            lo + Vec2::new(half, half),
        ];
        if level == 0 || d > self.ranges[level as usize - 1] {
            for q in quarters {
                self.add(q, level);
            }
            return true;
        }
        for q in quarters {
            if !self.walk(q, level - 1) {
                self.add(q, level);
            }
        }
        true
    }
}

/// Whether the box from `lo` to `hi` (render space, height up) may be seen through
/// `clip_from_render`: not wholly outside any of the six planes.
fn in_view(clip_from_render: &Mat4, lo: Vec3, hi: Vec3) -> bool {
    let m = clip_from_render;
    let rows = [m.row(0), m.row(1), m.row(2), m.row(3)];
    let planes = [
        rows[3] + rows[0],
        rows[3] - rows[0],
        rows[3] + rows[1],
        rows[3] - rows[1],
        rows[2],
        rows[3] - rows[2],
    ];
    planes.iter().all(|p| {
        let corner = Vec3::new(
            if p.x >= 0.0 { hi.x } else { lo.x },
            if p.y >= 0.0 { hi.y } else { lo.y },
            if p.z >= 0.0 { hi.z } else { lo.z },
        );
        p.x * corner.x + p.y * corner.y + p.z * corner.z + p.w >= 0.0
    })
}

fn floats(values: &[f32]) -> Vec<u8> {
    values.iter().flat_map(|f| f.to_le_bytes()).collect()
}

/// The landscape cell `cell` of block `block`, drawn `side` cells a side, as world meters: its
/// south-west corner and its side.
#[must_use]
pub fn cell_square(block: u16, cell: u8, side: u8) -> [f32; 3] {
    let side = side.clamp(1, 8);
    let size = 192.0 / f32::from(side);
    let (i, j) = (cell / side, cell % side);
    [
        f32::from(block >> 8) * 192.0 + f32::from(i) * size,
        f32::from(block & 0xFF) * 192.0 + f32::from(j) * size,
        size,
    ]
}

/// From the frame's `marks`, the landscape cells whose buildings stamp their openings, nearest
/// the eye at world `eye` first and at most [`STAMPED`] of them; and where the ground is drawn
/// again over the stamps: at the end of the block that draws the last of those buildings, the
/// `n`th end of a block's cells in the frame (from 1), at command `at`.
#[must_use]
pub fn stamped(marks: &[(u32, Mark)], eye: Vec2) -> (Vec<[f32; 3]>, Option<(u32, u32)>) {
    let mut cells = Vec::new();
    let mut ends: Vec<u32> = Vec::new();
    let mut last_building = None;
    for (at, mark) in marks {
        match mark {
            Mark::BuildingCell { block, cell, side } => {
                cells.push(cell_square(*block, *cell, *side));
                last_building = Some(*at);
            }
            Mark::TerrainEnd => ends.push(*at),
            _ => {}
        }
    }
    let over = last_building.and_then(|last| {
        ends.iter()
            .enumerate()
            .find(|(_, at)| **at > last)
            .and_then(|(k, at)| Some((u32::try_from(k + 1).ok()?, *at)))
    });
    let distance = |c: &[f32; 3]| (Vec2::new(c[0], c[1]) + Vec2::splat(c[2] * 0.5) - eye).length();
    cells.sort_by(|a, b| distance(a).total_cmp(&distance(b)));
    cells.truncate(STAMPED);
    (cells, over)
}

/// The frame's constants, as the shader's `Frame` lays them out.
fn frame_constants(
    frame: &HifiFrame,
    drawn: &HifiDrawnGround,
    clip_from_render: Mat4,
    n: u32,
    slots: &[i32],
    stamped: &[[f32; 3]],
) -> Vec<u8> {
    let k: &HifiGroundKnobs = &drawn.knobs;
    let cam = &frame.camera;
    let mut f: Vec<f32> = Vec::with_capacity(FRAME_BYTES / 4);
    f.extend(clip_from_render.to_cols_array());
    f.extend((cam.view * HEIGHT_UP).to_cols_array());
    f.extend([cam.eye.x, cam.eye.y, cam.eye.z, 0.0]);
    // LINT-OK: a block count, at most 13.
    #[allow(clippy::cast_precision_loss)]
    f.extend([
        drawn.global.x,
        drawn.global.y,
        n as f32,
        cam.far * FAR_SHARE,
    ]);
    let (level_r, level_f) = k.level.unwrap_or((0.0, 0.0));
    f.extend([k.micro.0, k.micro.1, level_r, level_f]);
    let (band, cap, from) = k.terraces.unwrap_or((0.0, 0.0, 0.0));
    let (far_round, far_detail, far_scale, horizon) = k.far.unwrap_or((0.0, 0.0, 96.0, 4000.0));
    f.extend([band, cap, from, far_round]);
    f.extend([
        k.morph.0,
        k.morph.1,
        f32::from(u8::from(k.far.is_some())),
        far_detail,
    ]);
    let bodies = drawn.bodies.len().min(BODIES);
    // LINT-OK: a view number and a body count, a few dozen at most.
    #[allow(clippy::cast_precision_loss)]
    f.extend([far_scale, horizon, k.view as f32, bodies as f32]);
    for i in 0..BODIES {
        let b = drawn.bodies.get(i).copied().unwrap_or(Vec3::ZERO);
        f.extend([b.x, b.y, b.z, 0.0]);
    }
    let mut out = floats(&f);
    for i in 0..SLOTS {
        out.extend(slots.get(i).copied().unwrap_or(-1).to_le_bytes());
    }
    let count = stamped.len().min(STAMPED);
    // The forest cover's canopy: meters at its whole, and where it rises in, from where the far
    // trees thin out to their reach.
    let (canopy, from, to) = canopy_constants(frame, drawn);
    // LINT-OK: a cell count, at most 64.
    #[allow(clippy::cast_precision_loss)]
    out.extend(floats(&[count as f32, canopy, from, to]));
    for i in 0..STAMPED {
        let [x, y, side] = stamped.get(i).copied().unwrap_or([0.0; 3]);
        out.extend(floats(&[x, y, side, 0.0]));
    }
    // The bed lowered under the water, while it is drawn as water.
    out.extend(floats(&match k.water {
        Some(w) => [1.0, w.depth, w.shelf, 0.0],
        None => [0.0; 4],
    }));
    out
}

/// The forest cover's canopy: how many meters its whole raises the far ground (nought for no
/// cover or no canopy), and the distances it rises in over: where the far trees start to thin
/// out and their reach, or a stretch past the blocks round the viewer while they are not drawn.
#[must_use]
pub fn canopy_constants(frame: &HifiFrame, drawn: &HifiDrawnGround) -> (f32, f32, f32) {
    let f = &frame.far;
    let (from, to) = cover_band(f);
    let meters = if drawn.cover.is_some() && f.cover > 0.0 {
        f.canopy * 255.0 * CANOPY_STEP
    } else {
        0.0
    };
    (meters, from, to)
}

/// Where the forest cover comes in, meters from the eye: from where the far trees thin out to
/// their reach while they are drawn, and over a stretch past the blocks round the viewer while
/// they are not.
#[must_use]
pub fn cover_band(f: &crate::snapshot::HifiFarKnobs) -> (f32, f32) {
    if f.trees && f.reach > 0.0 {
        let to = f.reach.max(1.0);
        (f.thin_from.min(to - 1.0), to)
    } else {
        (COVER_FROM, COVER_TO)
    }
}

/// The forest cover's canopy height a step of its texture holds, meters.
const CANOPY_STEP: f32 = 0.25;
/// Where the forest cover comes in while the far trees are not drawn, meters from the eye.
const COVER_FROM: f32 = 400.0;
const COVER_TO: f32 = 800.0;

fn uniform_entry(binding: u32, dynamic: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: dynamic,
            min_binding_size: None,
        },
        count: None,
    }
}

fn texture_entry(binding: u32, sample: wgpu::TextureSampleType) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: sample,
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

/// The world's landscape vertices on the device, with where the water lies about them.
#[derive(Debug)]
struct WorldTextures {
    serial: u64,
    heights: wgpu::Texture,
    words: wgpu::Texture,
    water: wgpu::Texture,
    words_view: wgpu::TextureView,
    heights_view: wgpu::TextureView,
    water_view: wgpu::TextureView,
    bytes: u64,
}

/// The forest cover on the device: its colour and share, and its canopy.
#[derive(Debug)]
struct CoverTextures {
    /// The cover's serial; nought for the empty cover drawn while there is none.
    serial: u64,
    colour: wgpu::Texture,
    colour_view: wgpu::TextureView,
    canopy_view: wgpu::TextureView,
    bytes: u64,
}

/// The forest cover `cover` on the device; with none, a texel of no cover.
fn upload_cover(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    cover: Option<&crate::snapshot::HifiForestCover>,
) -> CoverTextures {
    let none = [vec![0u8; 4], vec![0u8; 1]];
    let (side, colour_bytes, canopy_bytes) = match cover {
        Some(c)
            if c.colour.len() >= (c.side * c.side) as usize * 4
                && c.canopy.len() >= (c.side * c.side) as usize
                && c.side > 0 =>
        {
            let texels = (c.side * c.side) as usize;
            (c.side, &c.colour[..texels * 4], &c.canopy[..texels])
        }
        _ => (1, &none[0][..], &none[1][..]),
    };
    let make = |label: &str, format: wgpu::TextureFormat, bytes: &[u8], texel: u32| {
        let t = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: side,
                height: side,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &t,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(side * texel),
                rows_per_image: Some(side),
            },
            wgpu::Extent3d {
                width: side,
                height: side,
                depth_or_array_layers: 1,
            },
        );
        t
    };
    let colour = make(
        "forest cover",
        wgpu::TextureFormat::Rgba8UnormSrgb,
        colour_bytes,
        4,
    );
    let canopy = make(
        "forest canopy",
        wgpu::TextureFormat::R8Unorm,
        canopy_bytes,
        1,
    );
    CoverTextures {
        serial: cover.map_or(0, |c| c.serial),
        colour_view: colour.create_view(&wgpu::TextureViewDescriptor::default()),
        canopy_view: canopy.create_view(&wgpu::TextureViewDescriptor::default()),
        colour,
        bytes: (colour_bytes.len() + canopy_bytes.len()) as u64,
    }
}

/// The near offset on the device.
#[derive(Debug)]
struct NearTexture {
    /// Blocks a side.
    n: u32,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    /// Each slot's block and the tile generation it holds.
    held: HashMap<u32, (u16, u64)>,
}

/// The pipelines and buffers, made once.
#[derive(Debug)]
struct Gpu {
    frame_layout: wgpu::BindGroupLayout,
    view_pipeline: wgpu::RenderPipeline,
    /// The ground drawn again over the buildings' stamps (see the shader's `fs_ground_over`).
    over_pipeline: wgpu::RenderPipeline,
    caster_pipeline: wgpu::RenderPipeline,
    frame_uniform: wgpu::Buffer,
    caster_uniform: wgpu::Buffer,
    caster_bind: wgpu::BindGroup,
    instances: Option<(wgpu::Buffer, u64)>,
    /// The patches drawn again over the stamps: those no farther than the farthest stamping cell.
    over_instances: Option<(wgpu::Buffer, u64)>,
    caster_instances: Option<(wgpu::Buffer, u64)>,
}

fn instance_layout() -> wgpu::VertexBufferLayout<'static> {
    const ATTRIBUTES: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4];
    wgpu::VertexBufferLayout {
        array_stride: INSTANCE_BYTES,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &ATTRIBUTES,
    }
}

impl Gpu {
    fn new(device: &wgpu::Device) -> Self {
        use wgpu::TextureSampleType as S;
        let frame_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("drawn ground"),
            entries: &[
                uniform_entry(0, false),
                texture_entry(1, S::Float { filterable: false }),
                texture_entry(2, S::Uint),
                texture_entry(3, S::Float { filterable: false }),
                texture_entry(4, S::Float { filterable: false }),
                texture_entry(5, S::Float { filterable: false }),
            ],
        });
        let caster_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("drawn ground shadow cascade"),
            entries: &[uniform_entry(0, true)],
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("drawn ground"),
            source: wgpu::ShaderSource::Wgsl(shader_source().into()),
        });
        let view_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("drawn ground"),
            bind_group_layouts: &[Some(&frame_layout)],
            immediate_size: 0,
        });
        let shadow_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("drawn ground shadow"),
            bind_group_layouts: &[Some(&frame_layout), Some(&caster_layout)],
            immediate_size: 0,
        });
        let buffers = [Some(instance_layout())];
        let targets = [
            Some(wgpu::ColorTargetState {
                format: crate::derive::pipeline::HDR_FORMAT,
                blend: None,
                write_mask: wgpu::ColorWrites::COLOR,
            }),
            Some(wgpu::ColorTargetState {
                format: crate::derive::pipeline::NORMAL_FORMAT,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            }),
        ];
        // The view's pipeline, and the one that draws the ground again over the stamps: only
        // where it lies nearer than what is there, so the ground it already drew stays as it is.
        let ground = |label: &str, entry: &str, compare: wgpu::CompareFunction| {
            device.build_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&view_layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs_ground"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &buffers,
                },
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull_mode: None,
                    ..wgpu::PrimitiveState::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: crate::SidecarContext::depth_format(),
                    depth_write_enabled: Some(true),
                    depth_compare: Some(compare),
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some(entry),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &targets,
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let view_pipeline = ground(
            "drawn ground",
            "fs_ground",
            wgpu::CompareFunction::LessEqual,
        );
        let over_pipeline = ground(
            "drawn ground over the stamps",
            "fs_ground_over",
            wgpu::CompareFunction::Less,
        );
        let unclipped = device
            .features()
            .contains(wgpu::Features::DEPTH_CLIP_CONTROL);
        let caster_pipeline = device.build_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("drawn ground shadow"),
            layout: Some(&shadow_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_caster"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &buffers,
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                unclipped_depth: unclipped,
                ..wgpu::PrimitiveState::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState {
                    constant: 0,
                    slope_scale: 1.5,
                    clamp: 0.0,
                },
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: None,
            multiview_mask: None,
            cache: None,
        });
        let buffer = |label: &str, size: u64| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let frame_uniform = buffer("drawn ground", FRAME_BYTES as u64);
        let caster_uniform = buffer(
            "drawn ground shadow cascade",
            CASTER_STRIDE * CASCADES as u64,
        );
        let caster_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("drawn ground shadow cascade"),
            layout: &caster_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &caster_uniform,
                    offset: 0,
                    size: std::num::NonZeroU64::new(64),
                }),
            }],
        });
        Self {
            frame_layout,
            view_pipeline,
            over_pipeline,
            caster_pipeline,
            frame_uniform,
            caster_uniform,
            caster_bind,
            instances: None,
            over_instances: None,
            caster_instances: None,
        }
    }
}

fn upload_world(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    drawn: &HifiDrawnGround,
) -> WorldTextures {
    let side = drawn.world.side.max(1);
    let make = |label: &str, format: wgpu::TextureFormat| {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: side,
                height: side,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        })
    };
    let heights = make("drawn ground heights", wgpu::TextureFormat::R32Float);
    let words = make("drawn ground words", wgpu::TextureFormat::R16Uint);
    let water = make("drawn ground water", wgpu::TextureFormat::Rg16Float);
    let write = |t: &wgpu::Texture, bytes: &[u8], texel: u32| {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: t,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(side * texel),
                rows_per_image: Some(side),
            },
            wgpu::Extent3d {
                width: side,
                height: side,
                depth_or_array_layers: 1,
            },
        );
    };
    let h: Vec<u8> = drawn
        .world
        .heights
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect();
    let w: Vec<u8> = drawn
        .world
        .words
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect();
    write(&heights, &h, 4);
    write(&words, &w, 2);
    let count = side as usize * side as usize;
    let field = if drawn.world.water.len() == count {
        crate::passes::water::field::field_bytes(&drawn.world.water)
    } else {
        crate::passes::water::field::field_bytes(&vec![
            [-crate::passes::water::field::REACH, 0.0];
            count
        ])
    };
    write(&water, &field, 4);
    WorldTextures {
        serial: drawn.world.serial,
        heights_view: heights.create_view(&wgpu::TextureViewDescriptor::default()),
        words_view: words.create_view(&wgpu::TextureViewDescriptor::default()),
        water_view: water.create_view(&wgpu::TextureViewDescriptor::default()),
        heights,
        words,
        water,
        bytes: u64::from(side) * u64::from(side) * 10,
    }
}

fn make_near(device: &wgpu::Device, n: u32) -> NearTexture {
    let side = n * 192;
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("drawn ground near offset"),
        size: wgpu::Extent3d {
            width: side,
            height: side,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rg32Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    NearTexture {
        n,
        view: texture.create_view(&wgpu::TextureViewDescriptor::default()),
        texture,
        held: HashMap::new(),
    }
}

/// The slot of the near texture block `(bx, by)` takes, with `n` blocks a side.
#[must_use]
pub fn slot_of(block: u16, n: u32) -> (u32, u32) {
    (
        u32::from(block >> 8) % n.max(1),
        u32::from(block & 0xFF) % n.max(1),
    )
}

fn upload_tile(
    queue: &wgpu::Queue,
    near: &NearTexture,
    slot: (u32, u32),
    offset: &[f32],
    weight: &[f32],
) {
    const SIDE: usize = 193;
    let mut bytes = Vec::with_capacity(192 * 192 * 8);
    for j in 0..192 {
        for i in 0..192 {
            let k = j * SIDE + i;
            bytes.extend(offset.get(k).copied().unwrap_or(0.0).to_le_bytes());
            bytes.extend(weight.get(k).copied().unwrap_or(1.0).to_le_bytes());
        }
    }
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &near.texture,
            mip_level: 0,
            origin: wgpu::Origin3d {
                x: slot.0 * 192,
                y: slot.1 * 192,
                z: 0,
            },
            aspect: wgpu::TextureAspect::All,
        },
        &bytes,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(192 * 8),
            rows_per_image: Some(192),
        },
        wgpu::Extent3d {
            width: 192,
            height: 192,
            depth_or_array_layers: 1,
        },
    );
}

fn write_instances(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    slot: &mut Option<(wgpu::Buffer, u64)>,
    patches: &[Patch],
) {
    let mut bytes = Vec::with_capacity(patches.len() * INSTANCE_LEN);
    for p in patches {
        // LINT-OK: a level, at most a dozen.
        #[allow(clippy::cast_precision_loss)]
        let level = p.level as f32;
        bytes.extend(floats(&[
            p.at.x, p.at.y, p.spacing, level, p.morph.0, p.morph.1, 0.0, 0.0,
        ]));
    }
    let need = (bytes.len() as u64).max(INSTANCE_BYTES);
    if slot.as_ref().is_none_or(|(_, cap)| *cap < need) {
        let cap = need.next_power_of_two();
        *slot = Some((
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("drawn ground patches"),
                size: cap,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            cap,
        ));
    }
    if let Some((b, _)) = slot.as_ref() {
        if !bytes.is_empty() {
            queue.write_buffer(b, 0, &bytes);
        }
    }
}

/// The drawn ground's height the shader works out at each of `points` (world meters east and
/// north, and meters from the eye) for `drawn` seen from `frame`'s camera, without the fine
/// relief: the vertex stage's own arithmetic, run on `device` and read back. For the checks that
/// the ground is drawn where the scene stands what is on it.
#[must_use]
pub fn heights_on_device(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    frame: &HifiFrame,
    drawn: &HifiDrawnGround,
    points: &[[f32; 3]],
) -> Vec<f32> {
    let input: Vec<[f32; 4]> = points.iter().map(|p| [p[0], p[1], p[2], 0.0]).collect();
    probe(
        device,
        queue,
        frame,
        drawn,
        &input,
        "probe_out[id.x] = drawn(q.xy, q.z, 0.0, 0.0).x;",
        1,
    )
}

/// The point each pixel's pictures are laid at, as the shader finds it from the point of the
/// mesh under the pixel: for each of `points` (world meters east and north and the mesh's height
/// there, and meters from the eye), east, north and height, for `drawn` seen from `frame`'s
/// camera, run on `device` and read back. For the checks that the ground's pictures lie on the
/// ground itself, wherever the mesh lies.
#[must_use]
pub fn surface_points_on_device(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    frame: &HifiFrame,
    drawn: &HifiDrawnGround,
    points: &[[f32; 4]],
) -> Vec<[f32; 3]> {
    let out = probe(
        device,
        queue,
        frame,
        drawn,
        points,
        "let s = on_surface(q.xyz, q.w, 0.0, 0.0);
    probe_out[id.x * 3u] = s.x;
    probe_out[id.x * 3u + 1u] = s.y;
    probe_out[id.x * 3u + 2u] = s.z;",
        3,
    );
    out.as_chunks::<3>().0.to_vec()
}

/// `body` of the shader's arithmetic run on `device` for each of `points` (`q` in it), writing
/// `width` floats a point into `probe_out`, with the constants and textures `frame` and `drawn`
/// make; the floats read back.
#[allow(clippy::too_many_lines)]
fn probe(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    frame: &HifiFrame,
    drawn: &HifiDrawnGround,
    points: &[[f32; 4]],
    body: &str,
    width: usize,
) -> Vec<f32> {
    use wgpu::util::DeviceExt;
    let source = format!(
        "{}
@group(1) @binding(0) var<storage, read> probe_in: array<vec4<f32>>;
@group(1) @binding(1) var<storage, read_write> probe_out: array<f32>;

@compute @workgroup_size(64)
fn probe_main(@builtin(global_invocation_id) id: vec3<u32>) {{
    if (id.x >= arrayLength(&probe_in)) {{
        return;
    }}
    let q = probe_in[id.x];
    {body}
}}
",
        shader_source()
    );
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("drawn ground probe"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let compute = |mut e: wgpu::BindGroupLayoutEntry| {
        e.visibility = wgpu::ShaderStages::COMPUTE;
        e
    };
    let storage = |binding: u32, read_only: bool| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    };
    use wgpu::TextureSampleType as S;
    let frame_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("drawn ground probe"),
        entries: &[
            compute(uniform_entry(0, false)),
            compute(texture_entry(1, S::Float { filterable: false })),
            compute(texture_entry(2, S::Uint)),
            compute(texture_entry(3, S::Float { filterable: false })),
            compute(texture_entry(4, S::Float { filterable: false })),
            compute(texture_entry(5, S::Float { filterable: false })),
        ],
    });
    let probe_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("drawn ground probe points"),
        entries: &[storage(0, true), storage(1, false)],
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("drawn ground probe"),
        bind_group_layouts: &[Some(&frame_layout), Some(&probe_layout)],
        immediate_size: 0,
    });
    let pipeline = device.build_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("drawn ground probe"),
        layout: Some(&layout),
        module: &module,
        entry_point: Some("probe_main"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    });
    let world = upload_world(device, queue, drawn);
    let cover = upload_cover(device, queue, drawn.cover.as_deref());
    let n = u32::from(drawn.reach) * 2 + 1;
    let near = make_near(device, n);
    let mut slots = vec![-1i32; SLOTS];
    for t in &drawn.tiles {
        let (sx, sy) = slot_of(t.block, n);
        upload_tile(queue, &near, (sx, sy), &t.offset, &t.weight);
        if let Some(slot) = slots.get_mut((sy * n + sx) as usize) {
            *slot = i32::from(t.block >> 8) * 256 + i32::from(t.block & 0xFF);
        }
    }
    let cam = &frame.camera;
    let constants = frame_constants(
        frame,
        drawn,
        cam.projection * cam.view * HEIGHT_UP,
        n,
        &slots,
        &[],
    );
    let constants = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("drawn ground probe constants"),
        contents: &constants,
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let input: Vec<u8> = points.iter().flat_map(|p| floats(p)).collect();
    let input = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("drawn ground probe points"),
        contents: &input,
        usage: wgpu::BufferUsages::STORAGE,
    });
    let size = (points.len().max(1) * 4 * width) as u64;
    let output = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("drawn ground probe out"),
        size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("drawn ground probe readback"),
        size,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let g0 = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("drawn ground probe"),
        layout: &frame_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: constants.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&world.heights_view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&world.words_view),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(&near.view),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::TextureView(&world.water_view),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::TextureView(&cover.canopy_view),
            },
        ],
    });
    let g1 = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("drawn ground probe points"),
        layout: &probe_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: input.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: output.as_entire_binding(),
            },
        ],
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &g0, &[]);
        pass.set_bind_group(1, &g1, &[]);
        let groups = u32::try_from(points.len().div_ceil(64)).unwrap_or(u32::MAX);
        pass.dispatch_workgroups(groups, 1, 1);
    }
    encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, size);
    queue.submit([encoder.finish()]);
    readback.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    if device.poll(wgpu::PollType::wait_indefinitely()).is_err() {
        return Vec::new();
    }
    let Ok(bytes) = readback.slice(..).get_mapped_range() else {
        return Vec::new();
    };
    bytes
        .as_chunks::<4>()
        .0
        .iter()
        .take(points.len() * width)
        .map(|c| f32::from_le_bytes(*c))
        .collect()
}

/// Whether the pass draws the ground: an art pack's ground is laid, with the light, and the frame
/// carries the drawn ground.
fn draws(s: &HifiSettings, f: &HifiFrame) -> bool {
    s.art_pack && s.lighting.is_on() && f.drawn.is_some() && f.ground.is_some()
}

/// What the pass keeps between frames and shares with the sun's shadows.
#[derive(Debug, Default)]
struct State {
    gpu: Option<Gpu>,
    /// The same, built ahead by the warm-up.
    pre_gpu: Prebuilt<Gpu>,
    world: Option<WorldTextures>,
    cover: Option<CoverTextures>,
    near: Option<NearTexture>,
    bind: Option<wgpu::BindGroup>,
    /// The patches drawn this frame, those drawn again over the stamps, and those the shadows
    /// draw.
    patches: Vec<Patch>,
    over_patches: Vec<Patch>,
    caster_patches: Vec<Patch>,
    /// The commands of the landscape's own cells.
    landscape: crate::shared::runs::Runs,
    first_mark: Option<u32>,
    /// Whether the ground is drawn this frame, in place of the landscape's cells.
    active: bool,
    drawn: bool,
    /// The landscape cells whose buildings stamp their openings this frame, nearest first.
    stamped: Vec<[f32; 3]>,
    /// Where the ground is drawn again over the stamps: at the end of the block that draws the
    /// last building, the `n`th end of a block's cells this frame (from 1), at command `at`.
    over_at: Option<(u32, u32)>,
    /// The ends of blocks' cells the replay has reached this frame.
    ends_seen: u32,
    /// How many landscape cells' buildings stamped their openings this frame.
    stamps_seen: usize,
    /// Whether the ground is drawn again over the stamps this frame, and whether it has been.
    over_active: bool,
    over_drawn: bool,
}

/// The drawn ground pass.
#[derive(Debug)]
pub struct Drawn(Rc<RefCell<State>>);

/// The sun's shadows' way to the drawn ground.
#[derive(Debug, Clone)]
pub struct DrawnCasters(Rc<RefCell<State>>);

/// The pass, and the handle the sun's shadows draw the ground through.
#[must_use]
pub fn pass() -> (Drawn, DrawnCasters) {
    let state = Rc::new(RefCell::new(State::default()));
    (Drawn(Rc::clone(&state)), DrawnCasters(state))
}

impl DrawnCasters {
    /// Whether the drawn ground is drawn this frame: the shadows then take it in place of the
    /// landscape's triangles.
    #[must_use]
    pub fn replacing(&self) -> bool {
        self.0.borrow().active
    }

    /// Hand the device the cascades' transforms, `light_from_render` each.
    pub fn prepare(&self, queue: &wgpu::Queue, light_from_render: &[Mat4; CASCADES]) {
        let s = self.0.borrow();
        let Some(gpu) = s.gpu.as_ref() else {
            return;
        };
        let mut bytes = vec![0u8; CASTER_BLOCK * CASCADES];
        for (c, m) in light_from_render.iter().enumerate() {
            let at = c * CASTER_BLOCK;
            bytes[at..at + 64].copy_from_slice(&floats(&m.to_cols_array()));
        }
        queue.write_buffer(&gpu.caster_uniform, 0, &bytes);
    }

    /// Draw the ground into `pass`, shadow cascade `c`.
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>, c: usize) {
        let s = self.0.borrow();
        let (Some(gpu), Some(bind), Some((instances, _))) = (
            s.gpu.as_ref(),
            s.bind.as_ref(),
            s.gpu.as_ref().and_then(|g| g.caster_instances.as_ref()),
        ) else {
            return;
        };
        let count = u32::try_from(s.caster_patches.len()).unwrap_or(0);
        if count == 0 {
            return;
        }
        let offset = u32::try_from(c as u64 * CASTER_STRIDE).unwrap_or(0);
        pass.set_pipeline(&gpu.caster_pipeline);
        pass.set_bind_group(0, bind, &[]);
        pass.set_bind_group(1, &gpu.caster_bind, &[offset]);
        pass.set_vertex_buffer(0, instances.slice(..));
        pass.draw(0..PATCH_VERTICES, 0..count);
    }
}

impl State {
    /// Draw the ground into `pass`, the re-shaded world's: whole, or `over` the stamps.
    fn draw_view(&self, pass: &mut wgpu::RenderPass<'_>, over: bool) -> bool {
        let (Some(gpu), Some(bind), Some((instances, _))) = (
            self.gpu.as_ref(),
            self.bind.as_ref(),
            self.gpu.as_ref().and_then(|g| {
                if over {
                    g.over_instances.as_ref()
                } else {
                    g.instances.as_ref()
                }
            }),
        ) else {
            return false;
        };
        let patches = if over {
            &self.over_patches
        } else {
            &self.patches
        };
        let count = u32::try_from(patches.len()).unwrap_or(0);
        if count == 0 {
            return false;
        }
        pass.set_pipeline(if over {
            &gpu.over_pipeline
        } else {
            &gpu.view_pipeline
        });
        pass.set_bind_group(0, bind, &[]);
        pass.set_vertex_buffer(0, instances.slice(..));
        pass.draw(0..PATCH_VERTICES, 0..count);
        true
    }
}

impl HifiPass for Drawn {
    fn name(&self) -> &'static str {
        "drawn ground"
    }

    fn notes(&self, notes: &mut Vec<(&'static str, u64)>) {
        let s = self.0.borrow();
        notes.push(("drawn ground", u64::from(s.drawn)));
        notes.push(("drawn ground patches", s.patches.len() as u64));
        notes.push(("drawn ground stamped cells", s.stamps_seen as u64));
        notes.push(("drawn ground runs looked at", s.landscape.looked()));
        notes.push((
            "drawn ground tiles",
            s.near.as_ref().map_or(0, |n| n.held.len() as u64),
        ));
        notes.push((
            "drawn ground forest cover bytes",
            s.cover.as_ref().map_or(0, |c| c.bytes),
        ));
        notes.push((
            "drawn ground bytes",
            s.world.as_ref().map_or(0, |w| w.bytes)
                + s.near
                    .as_ref()
                    .map_or(0, |n| u64::from(n.n * 192) * u64::from(n.n * 192) * 8),
        ));
    }

    fn wanted(&self, s: &HifiSettings, f: &HifiFrame, _caps: &Caps) -> bool {
        let state = self.0.borrow();
        draws(s, f) && (state.gpu.is_some() || !state.pre_gpu.waits())
    }

    fn warm(&mut self, cx: &mut WarmCx<'_>) {
        let mut s = self.0.borrow_mut();
        if draws(cx.settings, cx.frame) && s.gpu.is_none() {
            let device = cx.device.clone();
            s.pre_gpu.ask(cx.warm, move || Gpu::new(&device));
        }
    }

    fn draws_ground(&self) -> bool {
        self.0.borrow().active
    }

    #[allow(clippy::too_many_lines)]
    fn prepare(&mut self, cx: &mut PrepareCx<'_, '_>) -> Result<(), HifiError> {
        let mut guard = self.0.borrow_mut();
        let s = &mut *guard;
        s.drawn = false;
        s.active = false;
        s.over_active = false;
        s.over_drawn = false;
        s.ends_seen = 0;
        let Some(drawn) = cx.frame.drawn.as_ref() else {
            return Ok(());
        };
        let device = cx.seam.device().clone();
        let queue = cx.seam.queue().clone();
        if s.gpu.is_none() {
            s.gpu = Some(s.pre_gpu.take().unwrap_or_else(|| Gpu::new(&device)));
        }
        if s.world
            .as_ref()
            .is_none_or(|w| w.serial != drawn.world.serial)
        {
            s.world = Some(upload_world(&device, &queue, drawn));
            s.bind = None;
        }
        let cover = drawn.cover.as_deref();
        if s.cover
            .as_ref()
            .is_none_or(|c| c.serial != cover.map_or(0, |c| c.serial))
        {
            s.cover = Some(upload_cover(&device, &queue, cover));
            s.bind = None;
        }
        let n = u32::from(drawn.reach) * 2 + 1;
        if s.near.as_ref().is_none_or(|t| t.n != n) {
            s.near = Some(make_near(&device, n));
            s.bind = None;
        }
        // The tiles, each into its slot when its block or its generation is new there.
        let mut slots = vec![-1i32; SLOTS];
        if let Some(near) = s.near.as_mut() {
            for t in &drawn.tiles {
                let (sx, sy) = slot_of(t.block, n);
                let key = sy * n + sx;
                if near.held.get(&key) != Some(&(t.block, t.generation)) {
                    upload_tile(&queue, near, (sx, sy), &t.offset, &t.weight);
                    near.held.insert(key, (t.block, t.generation));
                }
                if let Some(slot) = slots.get_mut(key as usize) {
                    *slot = i32::from(t.block >> 8) * 256 + i32::from(t.block & 0xFF);
                }
            }
            // A slot whose block left keeps its pixels, but no longer names it.
            near.held.retain(|k, (block, _)| {
                slots
                    .get(*k as usize)
                    .is_some_and(|b| *b == i32::from(*block >> 8) * 256 + i32::from(*block & 0xFF))
            });
        }
        let (Some(gpu), Some(world), Some(near), Some(cover)) = (
            s.gpu.as_ref(),
            s.world.as_ref(),
            s.near.as_ref(),
            s.cover.as_ref(),
        ) else {
            return Ok(());
        };
        if s.bind.is_none() {
            s.bind = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("drawn ground"),
                layout: &gpu.frame_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: gpu.frame_uniform.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&world.heights_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(&world.words_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(&near.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::TextureView(&world.water_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: wgpu::BindingResource::TextureView(&cover.canopy_view),
                    },
                ],
            }));
        }
        // The forest cover, for the art pack's ground far off.
        cx.resources.insert(
            crate::resources::ResourceName::ForestCover,
            crate::resources::Resource::Texture(cover.colour.clone(), cover.colour_view.clone()),
            0,
        )?;
        // The world's terrain words, for the art pack's ground far off.
        cx.resources.insert(
            crate::resources::ResourceName::DrawnGroundWords,
            crate::resources::Resource::Texture(world.words.clone(), world.words_view.clone()),
            0,
        )?;
        cx.resources.insert(
            crate::resources::ResourceName::DrawnGroundHeights,
            crate::resources::Resource::Texture(world.heights.clone(), world.heights_view.clone()),
            0,
        )?;
        cx.resources.insert(
            crate::resources::ResourceName::DrawnGroundWater,
            crate::resources::Resource::Texture(world.water.clone(), world.water_view.clone()),
            0,
        )?;
        // The cells whose buildings stamp their openings, nearest the eye first, and the end of
        // the block that draws the last of them, where the ground is drawn again over them.
        let eye_at = cx.frame.camera.eye.truncate() + drawn.global;
        (s.stamped, s.over_at) = stamped(&cx.seam.tables().marks, eye_at);
        s.stamps_seen = cx
            .seam
            .tables()
            .marks
            .iter()
            .filter(|(_, m)| matches!(m, Mark::BuildingCell { .. }))
            .count();
        let cam = &cx.frame.camera;
        let clip_from_render = cam.projection * cam.view * HEIGHT_UP;
        let eye = cam.eye + drawn.global.extend(0.0);
        let global = drawn.global;
        let horizon = drawn
            .knobs
            .far
            .map_or(cam.far, |f| f.3)
            .min(cam.far.max(4000.0));
        let seen = |lo: Vec3, hi: Vec3| {
            in_view(
                &clip_from_render,
                lo - global.extend(0.0),
                hi - global.extend(0.0),
            )
        };
        s.patches = select(eye, drawn.knobs.detail, horizon, &seen);
        // Drawn again over the stamps only out to the farthest stamping cell: no ground past it
        // can lie in front of a stamp.
        let stamps_reach = s
            .stamped
            .iter()
            .map(|c| {
                let far = Vec2::new(
                    (eye.x - c[0]).abs().max((eye.x - c[0] - c[2]).abs()),
                    (eye.y - c[1]).abs().max((eye.y - c[1] - c[2]).abs()),
                );
                far.length()
            })
            .fold(0.0f32, f32::max);
        s.over_patches = s
            .patches
            .iter()
            .filter(|p| {
                let side = p.spacing * CELLS as f32;
                let d = Vec2::new(
                    (p.at.x - eye.x).max(0.0).max(eye.x - (p.at.x + side)),
                    (p.at.y - eye.y).max(0.0).max(eye.y - (p.at.y + side)),
                );
                d.length() <= stamps_reach
            })
            .copied()
            .collect();
        s.caster_patches = select(
            eye,
            drawn.knobs.detail * 0.5,
            horizon.min(CASTER_REACH),
            &|_, _| true,
        );
        let mut gpu_mut = s.gpu.take();
        if let Some(g) = gpu_mut.as_mut() {
            write_instances(&device, &queue, &mut g.instances, &s.patches);
            write_instances(&device, &queue, &mut g.over_instances, &s.over_patches);
            write_instances(&device, &queue, &mut g.caster_instances, &s.caster_patches);
            queue.write_buffer(
                &g.frame_uniform,
                0,
                &frame_constants(
                    cx.frame,
                    drawn,
                    cam.jittered(clip_from_render),
                    n,
                    &slots,
                    &s.stamped,
                ),
            );
        }
        s.gpu = gpu_mut;
        // The landscape's own cells, which the ground is drawn in place of.
        s.landscape.clear();
        s.first_mark = None;
        let mut open = None;
        for (at, mark) in &cx.seam.tables().marks {
            match mark {
                Mark::TerrainCell { .. } => {
                    open = Some(*at);
                }
                Mark::TerrainCellEnd => {
                    // The ground is drawn at the first cell's end: past a landscape draw, so
                    // inside the re-shaded world.
                    s.first_mark.get_or_insert(*at);
                    if let Some(from) = open.take() {
                        s.landscape.push(from..*at);
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn replay_filter(&mut self) -> Option<&mut dyn ReplayFilter> {
        Some(self)
    }

    fn planned(&mut self, plan: Option<&ReshadePlan>) {
        let mut s = self.0.borrow_mut();
        s.active = s.gpu.is_some()
            && s.bind.is_some()
            && !s.patches.is_empty()
            && plan.is_some_and(|p| s.first_mark.is_some_and(|m| p.opaque.contains(&m)));
        s.over_active = s.active
            && plan.is_some_and(|p| s.over_at.is_some_and(|(_, at)| p.opaque.contains(&at)));
    }

    fn encode(&mut self, _cx: &mut EncodeCx<'_, '_>) -> Result<(), HifiError> {
        Ok(())
    }
}

impl ReplayFilter for Drawn {
    fn draw(&mut self, cmd: u32, _note: Option<&DrawNote>) -> DrawAction<'_> {
        let s = self.0.borrow();
        if s.active && s.landscape.contains(cmd) {
            DrawAction::Skip(SkipRule::ReplacedByPass)
        } else {
            DrawAction::Legacy
        }
    }

    fn mark(&mut self, mark: Mark, pass: &mut wgpu::RenderPass<'_>) -> bool {
        let mut s = self.0.borrow_mut();
        match mark {
            Mark::TerrainCellEnd => {
                if !s.active || s.drawn {
                    return false;
                }
                s.drawn = true;
                s.draw_view(pass, false)
            }
            Mark::TerrainEnd => {
                s.ends_seen += 1;
                let here = s.over_at.is_some_and(|(n, _)| n == s.ends_seen);
                if !here || !s.over_active || !s.drawn || s.over_drawn {
                    return false;
                }
                s.over_drawn = true;
                s.draw_view(pass, true)
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the drawn ground, an art-pack presentation's own)
    use super::*;

    #[test]
    fn the_drawn_ground_shader_parses_and_validates_with_its_entry_points() {
        let source = shader_source();
        let module = naga::front::wgsl::parse_str(&source)
            .unwrap_or_else(|e| panic!("{}", e.emit_to_string(&source)));
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap_or_else(|e| panic!("{e:?}"));
        let entries: Vec<&str> = module
            .entry_points
            .iter()
            .map(|e| e.name.as_str())
            .collect();
        assert_eq!(
            entries,
            ["vs_ground", "fs_ground", "fs_ground_over", "vs_caster"]
        );
    }

    #[test]
    fn patches_grow_coarser_away_from_the_eye_and_neighbors_differ_by_one_level_at_most() {
        let eye = Vec3::new(20_000.0, 20_000.0, 120.0);
        let patches = select(eye, 1.0, 30_000.0, &|_, _| true);
        assert!(!patches.is_empty());
        // The finest patches lie round the eye and the coarsest far off.
        let level_at = |x: f32, y: f32| {
            patches
                .iter()
                .find(|p| {
                    let side = p.spacing * CELLS as f32;
                    (p.at.x..p.at.x + side).contains(&x) && (p.at.y..p.at.y + side).contains(&y)
                })
                .map(|p| p.level)
        };
        assert_eq!(level_at(eye.x + 1.0, eye.y + 1.0), Some(0));
        let far = level_at(eye.x + 9000.0, eye.y).expect("ground nine kilometers off");
        assert!(far >= 6, "{far}");
        // No point of the ground is covered twice, and every patch's neighbors across each edge
        // are within one level of it.
        for p in &patches {
            let side = p.spacing * CELLS as f32;
            let middle = p.at + Vec2::splat(side * 0.5);
            assert_eq!(level_at(middle.x, middle.y), Some(p.level));
            for (dx, dy) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
                let q = middle + Vec2::new(dx, dy) * (side * 0.5 + 0.01);
                if let Some(l) = level_at(q.x, q.y) {
                    assert!(l.abs_diff(p.level) <= 1, "{p:?} beside level {l}");
                }
            }
        }
        // The ground stops at the horizon.
        let near = select(eye, 1.0, 3000.0, &|_, _| true);
        assert!(near.len() < patches.len());
        assert!(near.iter().all(|p| {
            let side = p.spacing * CELLS as f32;
            (p.at.x - eye.x).abs().min((p.at.x + side - eye.x).abs()) < 3000.0 + side
        }));
    }

    #[test]
    fn patches_are_drawn_nearest_the_eye_first() {
        let eye = Vec3::new(20_000.0, 20_000.0, 120.0);
        let patches = select(eye, 1.0, 30_000.0, &|_, _| true);
        let middle = |p: &Patch| {
            let side = p.spacing * CELLS as f32;
            (p.at + Vec2::splat(side * 0.5) - eye.truncate()).length()
        };
        assert!(patches.len() > 100, "{} patches", patches.len());
        for pair in patches.windows(2) {
            assert!(
                middle(&pair[0]) <= middle(&pair[1]),
                "{:?} drawn before the nearer {:?}",
                pair[0],
                pair[1]
            );
        }
    }

    /// The distinct places, rounded to a millimeter, of the vertices of `patch`'s edge `edge`
    /// (0 west, 1 east, 2 south, 3 north) that lie between `from` and `to` along it.
    fn edge_points(patch: &Patch, edge: u32, eye: Vec2, from: f32, to: f32) -> Vec<(i64, i64)> {
        let mut out: Vec<(i64, i64)> = (0..=CELLS)
            .map(|i| match edge {
                0 => (0, i),
                1 => (CELLS, i),
                2 => (i, 0),
                _ => (i, CELLS),
            })
            .map(|g| vertex_of(patch, g, eye))
            .filter(|v| {
                let along = if edge < 2 { v.y } else { v.x };
                along >= from - 1e-3 && along <= to + 1e-3
            })
            .map(|v| {
                // LINT-OK: world meters to millimeters, far inside i64.
                #[allow(clippy::cast_possible_truncation)]
                let mm = ((v.x * 1000.0).round() as i64, (v.y * 1000.0).round() as i64);
                mm
            })
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }

    #[test]
    fn every_edge_two_patches_share_has_the_same_vertices_on_both_sides() {
        // Eyes low and high over the ground, on and off the grids, at each detail.
        let mut eyes = vec![
            Vec3::new(20_000.0, 20_000.0, 2.0),
            Vec3::new(20_013.37, 19_871.5, 60.0),
            Vec3::new(31_744.0, 8_192.0, 150.0),
            Vec3::new(9_000.25, 40_000.75, 600.0),
        ];
        // And a dozen more, scattered.
        let mut seed = 0x2545_F491_u32;
        let mut next = || {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            // LINT-OK: sixteen bits, exact in f32.
            #[allow(clippy::cast_precision_loss)]
            let v = (seed >> 16) as f32 / 65_536.0;
            v
        };
        for _ in 0..12 {
            eyes.push(Vec3::new(
                next() * WORLD_METERS,
                next() * WORLD_METERS,
                next() * 300.0,
            ));
        }
        let mut shared = 0usize;
        for detail in [0.5, 1.0, 2.0] {
            for &eye in &eyes {
                let patches = select(eye, detail, 30_000.0, &|_, _| true);
                let e2 = eye.truncate();
                for p in &patches {
                    let side = p.spacing * CELLS as f32;
                    // Each neighbor to the east and to the north, and the edge between.
                    for q in &patches {
                        let q_side = q.spacing * CELLS as f32;
                        let east = (q.at.x - (p.at.x + side)).abs() < 1e-3
                            && q.at.y < p.at.y + side
                            && q.at.y + q_side > p.at.y;
                        let north = (q.at.y - (p.at.y + side)).abs() < 1e-3
                            && q.at.x < p.at.x + side
                            && q.at.x + q_side > p.at.x;
                        if !(east || north) {
                            continue;
                        }
                        let (edge_p, edge_q) = if east { (1, 0) } else { (3, 2) };
                        let (lo, hi) = if east {
                            (p.at.y.max(q.at.y), (p.at.y + side).min(q.at.y + q_side))
                        } else {
                            (p.at.x.max(q.at.x), (p.at.x + side).min(q.at.x + q_side))
                        };
                        let a = edge_points(p, edge_p, e2, lo, hi);
                        let b = edge_points(q, edge_q, e2, lo, hi);
                        assert_eq!(
                            a, b,
                            "detail {detail}, eye {eye}: {p:?} and {q:?} disagree along their edge"
                        );
                        shared += 1;
                    }
                }
            }
        }
        assert!(shared > 2000, "{shared} edges checked");
    }

    #[test]
    fn the_ground_is_drawn_again_over_the_stamps_after_the_last_building_but_not_in_its_cells() {
        // A block of full-detail cells, a building in its cell (2, 3); then a farther block's end;
        // a nearer block with a building in its coarse cell 1 of two a side.
        let marks = [
            (
                10,
                Mark::TerrainCell {
                    block: 0xA9B4,
                    cell: 19,
                },
            ),
            (11, Mark::TerrainCellEnd),
            (
                12,
                Mark::BuildingCell {
                    block: 0xA9B4,
                    cell: 19,
                    side: 8,
                },
            ),
            (13, Mark::Interiors),
            (20, Mark::InteriorsEnd),
            (21, Mark::TerrainEnd),
            (
                30,
                Mark::BuildingCell {
                    block: 0xAAB4,
                    cell: 1,
                    side: 2,
                },
            ),
            (40, Mark::TerrainEnd),
            (50, Mark::TerrainEnd),
        ];
        let eye = Vec2::new(169.0 * 192.0 + 60.0, 180.0 * 192.0 + 80.0);
        let (cells, over) = stamped(&marks, eye);
        // After the block that draws the last building: the second end of a block's cells.
        assert_eq!(over, Some((2, 40)));
        // Its own cells, nearest first, as world squares.
        assert_eq!(cells[0], [169.0 * 192.0 + 48.0, 180.0 * 192.0 + 72.0, 24.0]);
        assert_eq!(cells[1], [170.0 * 192.0, 180.0 * 192.0 + 96.0, 96.0]);
        // No building, nothing drawn again.
        assert_eq!(stamped(&marks[..2], eye), (Vec::new(), None));
    }

    #[test]
    fn a_tile_takes_the_slot_its_block_names_wrapping() {
        assert_eq!(slot_of(0xA9B4, 7), (0xA9 % 7, 0xB4 % 7));
        // Neighboring blocks take neighboring slots, the last wrapping to the first.
        let (a, _) = slot_of(0x0610, 7);
        let (b, _) = slot_of(0x0710, 7);
        assert_eq!((a + 1) % 7, b);
    }

    #[test]
    fn the_frame_constants_fill_the_shaders_frame_exactly() {
        let mut frame = HifiFrame::default();
        let drawn = HifiDrawnGround {
            world: std::sync::Arc::default(),
            tiles: Vec::new(),
            reach: 3,
            global: Vec2::ZERO,
            knobs: HifiGroundKnobs::default(),
            bodies: vec![Vec3::ONE; 40],
            cover: None,
        };
        frame.camera.view = Mat4::IDENTITY;
        let bytes = frame_constants(
            &frame,
            &drawn,
            Mat4::IDENTITY,
            7,
            &[1, 2, 3],
            &[[1.0; 3]; 70],
        );
        assert_eq!(bytes.len(), FRAME_BYTES);
    }
}
