//! The ground drawn as a grid of patches over retail's surface (`shaders/drawn.slang`): finer
//! near the eye and coarser away from it, out to the draw distance or to the world's edge, with
//! the scene's visual offset over it where the scene draws one.
//!
//! **The mesh** is a grid of square patches round the eye, [`CELLS`] quads a side, each level of
//! patches twice the last's spacing ([`select`]); a vertex toward the edge of its level's ring
//! slides onto the next level's grid, so the levels meet without cracks. Patches are chosen anew
//! each frame on the processor, a few hundred of them; nothing of the ground is built again as
//! the eye moves.
//!
//! **The world's vertices** (heights, terrain words, where the water lies) are held on the device
//! once, and **the near offset** a block a slot, each block in the slot its coordinates name, a
//! block moving in only when its slot's block leaves.

use dereth_primitives::num::{floor_to_i32, to_i32};
use std::collections::BTreeMap;

use dereth_gfx::{
    Buffer, BufferDesc, BufferUsage, ComputeDesc, Cull, DepthState, Device, Draw, Location,
    Pipeline, Touch, Use,
};
use glam::{Mat4, Vec2, Vec3};

use super::scene::{GpuScene, NONE};
use super::terrain::TerrainSettings;
use super::{push_words, shader, PerFrame};
use crate::scene::{DrawnGround, FrameDelta};
use crate::shaders::Entry;

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
/// The first level whose ring reaches [`FAR_RANGE_NODES`]: its grid is 16 meters apart at detail 1.
const FAR_LEVEL: u32 = 4;
/// How far a far level's ring reaches, in nodes of its size: far enough that a distant mountain is
/// drawn from a grid fine enough to keep its shape, not one coarser than the mountain. The levels
/// nearer keep [`RANGE_NODES`], and so do the sun's shadow casters ([`select_casters`]).
const FAR_RANGE_NODES: f32 = 8.0;
/// The share of a level's ring past which its vertices slide onto the next level's grid.
const MORPH_FROM: f32 = 0.8;
/// The most bodies the fine relief is leveled under.
const BODIES: usize = 32;
/// The most near slots: 16 blocks a side.
const SLOTS: usize = 256;
/// How many blocks the table of held blocks reaches from the frame's origin, each way.
const TABLE_REACH: i32 = 32;
/// The table's side.
const TABLE_SIDE: i32 = 2 * TABLE_REACH + 1;
/// How far the sun's shadows take the ground, meters from the eye.
const CASTER_REACH: f32 = 2500.0;
/// The share of the camera's far plane the ground is drawn to: the last stretch before it holds
/// depths a hair from the far plane's own.
const FAR_SHARE: f32 = 0.97;
/// The constants' rows, as the shader's `D_*` lay them out.
const ROWS: usize = 142 + 1 + 32 + 1 + 8;
/// The world's landscape vertices a side.
pub const WORLD_SIDE: u32 = 2041;

/// One patch of the ground.
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
    let nodes = if level >= FAR_LEVEL {
        FAR_RANGE_NODES
    } else {
        RANGE_NODES
    };
    node_size(level, detail) * nodes
}

/// How far level `level`'s ring reaches for the sun's shadow casters, meters: every level's ring
/// [`RANGE_NODES`] across, as before the far levels reached farther.
#[must_use]
pub fn caster_reach(level: u32, detail: f32) -> f32 {
    node_size(level, detail) * RANGE_NODES
}

/// The side of one of level `level`'s nodes at `detail`, meters.
fn node_size(level: u32, detail: f32) -> f32 {
    #[allow(clippy::cast_precision_loss)] // a level, at most a dozen
    let node = finest_spacing(detail) * CELLS as f32 * 2.0 * (1u32 << level.min(20)) as f32;
    node
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
    #[allow(clippy::cast_precision_loss)] // a vertex index, 0..=16
    let g = Vec2::new(grid.0 as f32, grid.1 as f32);
    let at = patch.at + g * patch.spacing;
    slide(at, g, patch.spacing, slid(at, patch.morph, eye))
}

/// Whether retail's cell `c` (world cells, 24 meters a side) is cut from its south-west corner to
/// its north-east: the shader's own arithmetic.
#[must_use]
pub fn sw_to_ne(c: (i32, i32)) -> bool {
    let (x, y) = (
        u32::from_ne_bytes(c.0.to_ne_bytes()),
        u32::from_ne_bytes(c.1.to_ne_bytes()),
    );
    let v = x
        .wrapping_mul(0x0CCA_C033)
        .wrapping_add(0x6C1A_C587)
        .wrapping_mul(y)
        .wrapping_add(x.wrapping_mul(0xBDE4_1C43))
        .wrapping_add(0xAE64_70DB);
    (v >> 31) != 0
}

/// Retail's height at world `p` over `heights` (the world's vertices, [`WORLD_SIDE`] a side): the
/// plane of the triangle `p` stands on, as the shader works it out.
#[must_use]
pub fn retail_height(heights: &[f32], p: Vec2) -> f32 {
    #[allow(clippy::cast_possible_wrap)]
    let side = WORLD_SIDE as i32; // LINT-OK: an integer, 2041
    let at = |x: i32, y: i32| {
        let (x, y) = (x.clamp(0, side - 1), y.clamp(0, side - 1));
        #[allow(clippy::cast_sign_loss)] // clamped to the world
        heights
            .get(y as usize * side as usize + x as usize)
            .copied()
            .unwrap_or(0.0)
    };
    let q = p / CELL_METERS;
    let c = (q.x.floor(), q.y.floor());
    let (cx, cy) = (to_i32(c.0), to_i32(c.1));
    let f = Vec2::new((q.x - c.0).clamp(0.0, 1.0), (q.y - c.1).clamp(0.0, 1.0));
    let (h00, h10, h01, h11) = (
        at(cx, cy),
        at(cx + 1, cy),
        at(cx, cy + 1),
        at(cx + 1, cy + 1),
    );
    if sw_to_ne((cx, cy)) {
        if f.x >= f.y {
            return h00 + f.x * (h10 - h00) + f.y * (h11 - h10);
        }
        return h00 + f.y * (h01 - h00) + f.x * (h11 - h01);
    }
    if f.x + f.y <= 1.0 {
        return h00 + f.x * (h10 - h00) + f.y * (h01 - h00);
    }
    h11 + (1.0 - f.x) * (h01 - h11) + (1.0 - f.y) * (h10 - h11)
}

/// A side of retail's cells, meters.
const CELL_METERS: f32 = 24.0;
/// How far under the drawn height the ground's mesh is laid, meters.
const BELOW: f32 = 0.01;

/// The triangle of `patches`' mesh (the eye at world `eye`) over world point `p`: its three
/// vertices as the vertex shader places them (x and y, world meters, and the distance from the
/// eye their heights are worked out at) and each one's share of `p`; `None` where no patch
/// covers `p`.
#[must_use]
pub fn mesh_triangle(
    patches: &[Patch],
    heights: &[f32],
    eye: Vec3,
    p: Vec2,
) -> Option<([[f32; 3]; 3], [f32; 3])> {
    #[allow(clippy::cast_precision_loss)] // a patch's quads, 16
    let cells = CELLS as f32;
    let patch = patches.iter().find(|q| {
        let hi = q.at + Vec2::splat(q.spacing * cells);
        p.x >= q.at.x && p.y >= q.at.y && p.x < hi.x && p.y < hi.y
    })?;
    let local = (p - patch.at) / patch.spacing;
    let (qx, qy) = (floor_to_i32(local.x), floor_to_i32(local.y));
    let mut best: Option<([[f32; 3]; 3], [f32; 3], f32)> = None;
    // The quad over `p` on the grid as laid, or, where its vertices have slid onto the next
    // level's, one beside it.
    #[allow(clippy::cast_possible_wrap)] // 16
    let last = CELLS as i32 - 1;
    for (dx, dy) in (-1..=1).flat_map(|dx| (-1..=1).map(move |dy| (dx, dy))) {
        let (x, y) = (qx + dx, qy + dy);
        if !(0..=last).contains(&x) || !(0..=last).contains(&y) {
            continue;
        }
        #[allow(clippy::cast_sign_loss)] // 0..16
        let (x, y) = (x as u32, y as u32);
        #[allow(clippy::cast_precision_loss)] // 0..16
        let centre = patch.at + (Vec2::new(x as f32, y as f32) + 0.5) * patch.spacing;
        let cut = sw_to_ne((
            floor_to_i32(centre.x / CELL_METERS),
            floor_to_i32(centre.y / CELL_METERS),
        ));
        let triangles: [[(u32, u32); 3]; 2] = if cut {
            [[(0, 0), (1, 0), (1, 1)], [(0, 0), (1, 1), (0, 1)]]
        } else {
            [[(0, 0), (1, 0), (0, 1)], [(1, 0), (1, 1), (0, 1)]]
        };
        for t in triangles {
            let v = t.map(|(cx, cy)| vertex_of(patch, (x + cx, y + cy), eye.truncate()));
            let (a, b, c) = (v[0], v[1], v[2]);
            let area = (b - a).perp_dot(c - a);
            if area.abs() < 1e-6 {
                continue;
            }
            let w1 = (p - a).perp_dot(c - a) / area;
            let w2 = (b - a).perp_dot(p - a) / area;
            let w = [1.0 - w1 - w2, w1, w2];
            let worst = w.iter().copied().fold(f32::MAX, f32::min);
            if best.as_ref().is_none_or(|b| worst > b.2) {
                let placed = v.map(|q| {
                    let h = retail_height(heights, q);
                    [q.x, q.y, (q - eye.truncate()).extend(h - eye.z).length()]
                });
                best = Some((placed, w, worst));
            }
        }
    }
    best.map(|(v, w, _)| (v, w))
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
    select_in(eye, detail, horizon, seen, &|l| reach(l, detail))
}

/// [`select`] for the sun's shadow casters, every ring at [`caster_reach`]: the far levels' longer
/// rings would give them more patches than shadows need.
#[must_use]
pub fn select_casters(eye: Vec3, detail: f32, horizon: f32) -> Vec<Patch> {
    select_in(eye, detail, horizon, &|_, _| true, &|l| {
        caster_reach(l, detail)
    })
}

fn select_in(
    eye: Vec3,
    detail: f32,
    horizon: f32,
    seen: &dyn Fn(Vec3, Vec3) -> bool,
    ranges: &dyn Fn(u32) -> f32,
) -> Vec<Patch> {
    let mut s = Selector {
        eye,
        finest: finest_spacing(detail),
        ranges: (0..=TOP).map(ranges).collect(),
        horizon,
        seen,
        out: Vec::new(),
    };
    s.walk(Vec2::ZERO, TOP);
    // Nearest first, so the ground nearer the eye hides what lies behind it before that is
    // shaded.
    let eye = eye.truncate();
    let middle = |p: &Patch| {
        #[allow(clippy::cast_precision_loss)] // a patch's quads, 16
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
        #[allow(clippy::cast_precision_loss)] // a level, at most a dozen
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
        #[allow(clippy::cast_precision_loss)] // a level, at most a dozen
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

/// Whether the box from `lo` to `hi` may be seen through `clip`: not wholly outside any of its
/// six planes.
fn in_view(clip: &Mat4, lo: Vec3, hi: Vec3) -> bool {
    let rows = [clip.row(0), clip.row(1), clip.row(2), clip.row(3)];
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

/// The slot of the near offset block `block` takes, with `n` blocks a side.
#[must_use]
pub fn slot_of(block: u16, n: u32) -> (u32, u32) {
    (
        u32::from(block >> 8) % n.max(1),
        u32::from(block & 0xFF) % n.max(1),
    )
}

fn floats(values: &[f32]) -> Vec<u8> {
    values.iter().flat_map(|f| f.to_le_bytes()).collect()
}

/// A device buffer, bound, written once.
#[derive(Debug, Clone, Copy)]
struct Bound {
    buffer: Buffer,
    index: u32,
    bytes: u64,
}

fn bound(d: &mut Device, label: &'static str, bytes: &[u8]) -> Result<Bound, dereth_gfx::Error> {
    let buffer = d.create_buffer(&BufferDesc {
        label,
        size: (bytes.len() as u64).max(16),
        usage: BufferUsage::STORAGE,
        location: Location::Gpu,
    })?;
    if !bytes.is_empty() {
        d.write_buffer(buffer, 0, bytes)?;
    }
    Ok(Bound {
        buffer,
        index: d.bind_buffer(buffer)?,
        bytes: (bytes.len() as u64).max(16),
    })
}

/// The world's vertices on the device.
#[derive(Debug)]
struct World {
    serial: u64,
    heights: Bound,
    words: Bound,
    water: Bound,
}

/// The near offset on the device: its slots, and what each holds.
#[derive(Debug)]
struct Near {
    n: u32,
    buffer: Bound,
    /// Each slot's block and the generation of its offset.
    held: BTreeMap<(u32, u32), (u16, u64)>,
}

/// The drawn ground's pipelines.
#[derive(Debug, Clone, Copy)]
struct Pipelines {
    ground: Pipeline,
    caster: Pipeline,
    probe: Pipeline,
}

/// The drawn ground's pass: its pipelines and what it holds on the device.
#[derive(Debug)]
pub struct DrawnPass {
    pipelines: Pipelines,
    world: Option<World>,
    near: Option<Near>,
    constants: PerFrame,
    patches: PerFrame,
    blocks: PerFrame,
    /// Each terrain type's color far off, as linear-free display values, once worked out.
    pub(crate) colors: [[f32; 4]; 32],
    /// Whether the grass field is drawn: the ground is then shaded under it and takes its color
    /// past it.
    pub(crate) grass: bool,
    /// What each terrain type grows in the grass field.
    pub(crate) grass_kinds: [super::grass::GrassKind; 32],
    /// Whether the flowers are drawn: past their reach the ground carries their colors.
    pub(crate) flowers: bool,
    /// The patches the last frame drew, for the view and for the sun's shadows.
    pub last_patches: (u32, u32),
    /// The last frame's patches for the view, and its eye, world meters.
    view: (Vec<Patch>, Vec3),
}

/// One frame's drawn ground.
#[derive(Debug, Clone, Copy)]
pub struct DrawnPlan {
    pipelines: Pipelines,
    words: [u32; 13],
    view: (u32, u32),
    casters: (u32, u32),
    buffers: [Buffer; 5],
}

impl DrawnPlan {
    /// The world's landscape heights: their index in the table and their buffer, what another
    /// pass reads for the shape of the land.
    pub(crate) fn heights(&self) -> (u32, Buffer) {
        (self.words[2], self.buffers[0])
    }

    /// The frame's constants' index, the terrain words' index, and the two buffers they are in:
    /// what another pass reads for the terrain's colors.
    pub(crate) fn terrain_inputs(&self) -> (u32, u32, [Buffer; 2]) {
        (
            self.words[1],
            self.words[3],
            [self.buffers[4], self.buffers[1]],
        )
    }
}

impl DrawnPass {
    /// The pass, its pipelines made now; the ground is drawn into the G-buffer's `targets` and
    /// `depth`, and its casters into maps of `caster_depth`.
    ///
    /// # Errors
    /// What the device refuses.
    pub fn new(
        d: &mut Device,
        targets: &[dereth_gfx::ColorTarget],
        depth: Option<DepthState>,
        caster_depth: Option<DepthState>,
    ) -> Result<Self, dereth_gfx::Error> {
        let pipelines = Pipelines {
            ground: d.create_graphics_pipeline(&dereth_gfx::GraphicsDesc {
                label: "remaster drawn ground",
                vertex: shader(Entry::DrawnVsGround),
                fragment: Some(shader(Entry::DrawnFsGround)),
                color: targets,
                depth,
                cull: Cull::None,
            })?,
            caster: d.create_graphics_pipeline(&dereth_gfx::GraphicsDesc {
                label: "remaster drawn ground casters",
                vertex: shader(Entry::DrawnVsCaster),
                fragment: None,
                color: &[],
                depth: caster_depth,
                cull: Cull::None,
            })?,
            probe: d.create_compute_pipeline(&ComputeDesc {
                label: "remaster drawn ground probe",
                shader: shader(Entry::DrawnCsProbe),
            })?,
        };
        Ok(Self {
            pipelines,
            world: None,
            near: None,
            constants: PerFrame::new(
                d,
                "remaster drawn ground",
                (ROWS * 16) as u64,
                BufferUsage::default(),
            )?,
            patches: PerFrame::new(d, "remaster drawn patches", 1 << 16, BufferUsage::default())?,
            blocks: PerFrame::new(
                d,
                "remaster drawn blocks",
                (TABLE_SIDE * TABLE_SIDE * 16) as u64,
                BufferUsage::default(),
            )?,
            colors: [[0.5, 0.5, 0.5, 1.0]; 32],
            grass: false,
            grass_kinds: super::grass::kinds(),
            flowers: false,
            last_patches: (0, 0),
            view: (Vec::new(), Vec3::ZERO),
        })
    }

    /// The world's vertex heights on the device, [`WORLD_SIDE`] a side, once they are read.
    #[must_use]
    pub fn heights(&self) -> Option<u32> {
        self.world.as_ref().map(|w| w.heights.index)
    }

    /// The height the last frame's mesh draws the ground at, at each of the world `points`, as
    /// its triangles lay it between their vertices; without the fine relief. `None` where no
    /// patch of the last frame covers a point.
    ///
    /// # Errors
    /// What the device refuses.
    pub fn mesh_heights(
        &mut self,
        d: &mut Device,
        scene: &GpuScene,
        frame: &FrameDelta,
        settings: TerrainSettings,
        points: &[Vec2],
    ) -> Result<Vec<Option<f32>>, dereth_gfx::Error> {
        let Some(ground) = frame.drawn.as_ref() else {
            return Ok(vec![None; points.len()]);
        };
        let (view, eye) = self.view.clone();
        let triangles: Vec<_> = points
            .iter()
            .map(|p| mesh_triangle(&view, &ground.world.heights, eye, *p))
            .collect();
        let vertices: Vec<[f32; 3]> = triangles
            .iter()
            .flatten()
            .flat_map(|(v, _)| v.iter().copied())
            .collect();
        let heights = self.heights_at(d, scene, frame, settings, &vertices)?;
        let mut at = heights.as_chunks::<3>().0.iter();
        Ok(triangles
            .iter()
            .map(|t| {
                let (_, w) = t.as_ref()?;
                let h = at.next()?;
                Some(w[0] * h[0] + w[1] * h[1] + w[2] * h[2] - BELOW)
            })
            .collect())
    }

    /// The bytes the pass holds on the device: the world's vertices and the near offset.
    #[must_use]
    pub fn bytes(&self) -> u64 {
        self.world
            .as_ref()
            .map_or(0, |w| w.heights.bytes + w.words.bytes + w.water.bytes)
            + self.near.as_ref().map_or(0, |n| n.buffer.bytes)
    }

    /// Hold the world's vertices, when `ground`'s are not the ones held.
    fn hold_world(
        &mut self,
        d: &mut Device,
        ground: &DrawnGround,
    ) -> Result<(), dereth_gfx::Error> {
        let w = &ground.world;
        if self.world.as_ref().is_some_and(|h| h.serial == w.serial) {
            return Ok(());
        }
        if let Some(old) = self.world.take() {
            for b in [old.heights, old.words, old.water] {
                d.destroy_buffer(b.buffer);
            }
        }
        let heights = floats(&w.heights);
        let mut words = Vec::with_capacity(w.words.len() * 2 + 2);
        for x in w.words.iter() {
            words.extend_from_slice(&x.to_le_bytes());
        }
        if words.len() % 4 != 0 {
            words.extend_from_slice(&[0, 0]);
        }
        let mut water = Vec::with_capacity(w.heights.len() * 8);
        if w.water.len() == w.heights.len() {
            for [a, b] in w.water.iter() {
                water.extend(a.to_le_bytes());
                water.extend(b.to_le_bytes());
            }
        } else {
            // No water anywhere: every vertex far outside it.
            for _ in 0..w.heights.len() {
                water.extend((-1000.0f32).to_le_bytes());
                water.extend(0.0f32.to_le_bytes());
            }
        }
        self.world = Some(World {
            serial: w.serial,
            heights: bound(d, "remaster world heights", &heights)?,
            words: bound(d, "remaster world terrain words", &words)?,
            water: bound(d, "remaster world water", &water)?,
        });
        Ok(())
    }

    /// Hold `ground`'s near offsets, each block in its slot; the slots' blocks, for the shader.
    fn hold_near(
        &mut self,
        d: &mut Device,
        ground: &DrawnGround,
    ) -> Result<(u32, Vec<i32>), dereth_gfx::Error> {
        let n = (u32::from(ground.reach) * 2 + 1).min(16);
        if ground.tiles.is_empty() {
            return Ok((0, vec![-1; SLOTS]));
        }
        if self.near.as_ref().is_none_or(|h| h.n != n) {
            if let Some(old) = self.near.take() {
                d.destroy_buffer(old.buffer.buffer);
            }
            let bytes = u64::from(n * n) * 192 * 192 * 8;
            let buffer = d.create_buffer(&BufferDesc {
                label: "remaster drawn near offset",
                size: bytes,
                usage: BufferUsage::STORAGE,
                location: Location::Gpu,
            })?;
            self.near = Some(Near {
                n,
                buffer: Bound {
                    buffer,
                    index: d.bind_buffer(buffer)?,
                    bytes,
                },
                held: BTreeMap::new(),
            });
        }
        let Some(near) = self.near.as_mut() else {
            return Ok((0, vec![-1; SLOTS]));
        };
        for t in &ground.tiles {
            let slot = slot_of(t.block, n);
            if near.held.get(&slot) == Some(&(t.block, t.generation)) {
                continue;
            }
            const SIDE: usize = 193;
            let mut bytes = Vec::with_capacity(192 * 192 * 8);
            for j in 0..192 {
                for i in 0..192 {
                    let k = j * SIDE + i;
                    bytes.extend(t.offset.get(k).copied().unwrap_or(0.0).to_le_bytes());
                    bytes.extend(t.weight.get(k).copied().unwrap_or(1.0).to_le_bytes());
                }
            }
            let at = u64::from(slot.1 * n + slot.0) * 192 * 192 * 8;
            d.write_buffer(near.buffer.buffer, at, &bytes)?;
            near.held.insert(slot, (t.block, t.generation));
        }
        let mut slots = vec![-1i32; SLOTS];
        for (&(x, y), &(block, _)) in &near.held {
            if let Some(s) = slots.get_mut((y * n + x) as usize) {
                *s = i32::from(block >> 8) * 256 + i32::from(block & 0xFF);
            }
        }
        Ok((n, slots))
    }

    /// Work out the frame's drawn ground: the world and the near offset held, the patches chosen
    /// for the view (through `clip`, the unjittered view and projection) and for the sun's
    /// shadows, and the constants written. `None` while the ground is drawn from retail's grid.
    ///
    /// # Errors
    /// What the device refuses.
    #[allow(clippy::too_many_lines)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn prepare(
        &mut self,
        d: &mut Device,
        scene: &GpuScene,
        frame: &FrameDelta,
        settings: TerrainSettings,
        clip: Mat4,
        window_meters: f32,
        cover: super::forest::CoverRows,
    ) -> Result<Option<DrawnPlan>, dereth_gfx::Error> {
        if !settings.grid || !frame.landscape {
            return Ok(None);
        }
        let Some(ground) = frame.drawn.as_ref() else {
            return Ok(None);
        };
        if ground.world.heights.len() != (WORLD_SIDE * WORLD_SIDE) as usize {
            return Ok(None);
        }
        self.hold_world(d, ground)?;
        let offset = settings.drawn && !ground.tiles.is_empty();
        let (n, slots) = if offset {
            self.hold_near(d, ground)?
        } else {
            (0, vec![-1; SLOTS])
        };
        let k = &ground.knobs;
        let global = ground.global;
        let eye_frame = frame.camera.eye;
        let eye = Vec3::new(eye_frame.x + global.x, eye_frame.y + global.y, eye_frame.z);
        // To the camera's far plane, and short of the world's edge only where it is asked for.
        let far = frame.camera.far.max(1.0) * FAR_SHARE;
        let horizon = if settings.horizon {
            far
        } else {
            far.min(window_meters)
        };
        let g3 = Vec3::new(global.x, global.y, 0.0);
        let view = select(eye, k.detail, horizon, &|lo, hi| {
            in_view(&clip, lo - g3, hi - g3)
        });
        let casters = select_casters(eye, k.detail, horizon.min(CASTER_REACH));
        let mut bytes = Vec::with_capacity((view.len() + casters.len()) * 32);
        for p in view.iter().chain(&casters) {
            #[allow(clippy::cast_precision_loss)] // a level, at most a dozen
            let level = p.level as f32;
            bytes.extend(floats(&[
                p.at.x, p.at.y, p.spacing, level, p.morph.0, p.morph.1, 0.0, 0.0,
            ]));
        }
        self.patches.write(d, &bytes)?;
        let view_count = u32::try_from(view.len()).unwrap_or(0);
        let caster_count = u32::try_from(casters.len()).unwrap_or(0);
        self.last_patches = (view_count, caster_count);
        self.view = (view, eye);

        // The constants, row by row as the shader's `D_*` lay them out.
        let mut f: Vec<f32> = Vec::with_capacity(ROWS * 4);
        f.extend([eye_frame.x, eye_frame.y, eye_frame.z, 0.0]);
        #[allow(clippy::cast_precision_loss)] // a block count, at most 16
        f.extend([global.x, global.y, n as f32, far]);
        let (level_r, level_f) = if offset {
            k.level.unwrap_or((0.0, 0.0))
        } else {
            (0.0, 0.0)
        };
        let micro = if offset { k.micro } else { (0.0, k.micro.1) };
        f.extend([micro.0, micro.1, level_r, level_f]);
        let (band, cap, from) = if offset {
            k.terraces.unwrap_or((0.0, 0.0, 0.0))
        } else {
            (0.0, 0.0, 0.0)
        };
        let far_ground = if offset && settings.horizon {
            k.far
        } else {
            None
        };
        let (far_round, far_detail, far_scale, _) = far_ground.unwrap_or((0.0, 0.0, 96.0, 0.0));
        f.extend([band, cap, from, far_round]);
        f.extend([
            k.morph.0,
            k.morph.1,
            f32::from(u8::from(far_ground.is_some())),
            far_detail,
        ]);
        let bodies = if offset {
            ground.bodies.len().min(BODIES)
        } else {
            0
        };
        #[allow(clippy::cast_precision_loss)] // a body count, at most 32
        f.extend([far_scale, 0.0, 0.0, bodies as f32]);
        match (offset, k.water) {
            (true, Some((depth, shelf))) => f.extend([1.0, depth, shelf, 0.0]),
            _ => f.extend([0.0; 4]),
        }
        let mut out = floats(&f);
        // The table's origin: the frame's origin block, the reach and the side.
        out.extend(push_words(&[
            u32::from_ne_bytes(frame.origin[0].to_ne_bytes()),
            u32::from_ne_bytes(frame.origin[1].to_ne_bytes()),
            TABLE_REACH.unsigned_abs(),
            TABLE_SIDE.unsigned_abs(),
        ]));
        for i in 0..BODIES {
            let b = if i < bodies {
                ground.bodies[i]
            } else {
                Vec3::ZERO
            };
            out.extend(floats(&[b.x, b.y, b.z, 0.0]));
        }
        for i in 0..SLOTS {
            out.extend(slots.get(i).copied().unwrap_or(-1).to_le_bytes());
        }
        for c in &self.colors {
            out.extend(floats(c));
        }
        match k.slope {
            Some(s) => {
                out.extend(floats(&[s.scree.0, s.scree.1, s.rock.0, s.rock.1]));
                out.extend(floats(&[s.strength, s.smooth, s.on_snow, 0.0]));
            }
            None => out.extend(floats(&[0.0; 8])),
        }
        out.extend(floats(&cover.tint));
        out.extend(floats(&cover.canopy));
        out.extend(push_words(&[cover.pictures[0], cover.pictures[1], 0, 0]));
        let banks = if offset && k.water.is_some() {
            1.0
        } else {
            0.0
        };
        out.extend(floats(&[banks, k.bank.0, k.bank.1, 0.0]));
        let grass = if self.grass { 1.0 } else { 0.0 };
        out.extend(floats(&[
            grass,
            super::grass::NEAR_REACH,
            super::grass::FAR_REACH,
            0.0,
        ]));
        for g in &self.grass_kinds {
            out.extend(floats(&[g.share, g.height, g.dryness, g.patchy]));
        }
        let flowers = if self.flowers { 1.0 } else { 0.0 };
        out.extend(floats(&[flowers, super::grass::SCATTER_REACH, 0.0, 0.0]));
        for c in super::grass::PALETTE {
            out.extend(floats(&[c[0], c[1], c[2], 1.0]));
        }
        self.constants.write(d, &out)?;

        // The held blocks round the frame's origin: each its first ground cell and its cells a
        // side.
        let mut table = vec![NONE; (TABLE_SIDE * TABLE_SIDE * 4) as usize];
        for (id, g) in &scene.ground {
            let (bx, by) = (i32::from(id >> 8), i32::from(id & 0xFF));
            let (x, y) = (
                bx - frame.origin[0] + TABLE_REACH,
                by - frame.origin[1] + TABLE_REACH,
            );
            if (0..TABLE_SIDE).contains(&x) && (0..TABLE_SIDE).contains(&y) {
                let at = ((y * TABLE_SIDE + x) * 4) as usize;
                table[at] = g.cells.start;
                table[at + 1] = g.side;
            }
        }
        self.blocks.write(d, &push_words(&table))?;

        let (Some(world), Some(near_index)) = (
            self.world.as_ref(),
            Some(self.near.as_ref().map_or(NONE, |n| n.buffer.index)),
        ) else {
            return Ok(None);
        };
        let near_buffer = self
            .near
            .as_ref()
            .map_or(world.heights.buffer, |n| n.buffer.buffer);
        Ok(Some(DrawnPlan {
            pipelines: self.pipelines,
            words: [
                0, // the frame's constants, set at the draw
                self.constants.index,
                world.heights.index,
                world.words.index,
                world.water.index,
                near_index,
                self.patches.index,
                scene.ground_cells.index,
                self.blocks.index,
                scene.splats.index,
                0, // the samplers, set at the draw
                0,
                0, // the first patch, set at the draw
            ],
            view: (0, view_count),
            casters: (view_count, caster_count),
            buffers: [
                world.heights.buffer,
                world.words.buffer,
                world.water.buffer,
                near_buffer,
                self.constants.buffer,
            ],
        }))
    }

    /// The drawn height at `points` (world x and y, and the distance from the eye), worked out on
    /// the device with `frame`'s ground: what the vertex stage puts under each, without the fine
    /// relief. For the checks that the ground is drawn where the scene stands what is on it.
    ///
    /// # Errors
    /// What the device refuses.
    pub fn heights_at(
        &mut self,
        d: &mut Device,
        scene: &GpuScene,
        frame: &FrameDelta,
        settings: TerrainSettings,
        points: &[[f32; 3]],
    ) -> Result<Vec<f32>, dereth_gfx::Error> {
        // The frame's own patches stand: the probe's are chosen for no eye.
        let (view, last) = (self.view.clone(), self.last_patches);
        let plan = self.prepare(
            d,
            scene,
            frame,
            settings,
            Mat4::IDENTITY,
            f32::MAX,
            super::forest::CoverRows::default(),
        );
        (self.view, self.last_patches) = (view, last);
        let Some(plan) = plan? else {
            return Ok(Vec::new());
        };
        let count = u32::try_from(points.len()).unwrap_or(0);
        let mut input = push_words(&[count, 0, 0, 0]);
        for p in points {
            input.extend(floats(&[p[0], p[1], p[2], 0.0]));
        }
        let points_buffer = bound(d, "remaster drawn probe points", &input)?;
        let out = d.create_buffer(&BufferDesc {
            label: "remaster drawn probe heights",
            size: u64::from(count.max(1)) * 4,
            usage: BufferUsage::STORAGE | BufferUsage::TRANSFER_SRC,
            location: Location::Gpu,
        })?;
        let out_index = d.bind_buffer(out)?;
        let back = d.create_buffer(&BufferDesc {
            label: "remaster drawn probe read-back",
            size: u64::from(count.max(1)) * 4,
            usage: BufferUsage::TRANSFER_DST,
            location: Location::Readback,
        })?;
        let mut words = plan.words;
        words[6] = points_buffer.index;
        words[7] = out_index;
        let mut r = d.begin()?;
        let mut touches = vec![
            Touch::Buffer(points_buffer.buffer, Use::StorageRead),
            Touch::Buffer(out, Use::StorageWrite),
        ];
        touches.extend(
            plan.buffers
                .iter()
                .map(|b| Touch::Buffer(*b, Use::StorageRead)),
        );
        r.compute(&touches, |c| {
            c.set_pipeline(plan.pipelines.probe)?;
            c.push(0, &push_words(&words))?;
            c.dispatch(count.div_ceil(64).max(1), 1, 1);
            Ok(())
        })?;
        r.copy_buffer(out, 0, back, 0, u64::from(count) * 4)?;
        r.touch(&[Touch::Buffer(back, Use::HostRead)])?;
        let ticket = r.submit()?;
        d.wait(ticket)?;
        let bytes = d.read_buffer(back, 0, u64::from(count) * 4)?;
        for b in [points_buffer.buffer, out, back] {
            d.destroy_buffer(b);
        }
        Ok(bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| f32::from_le_bytes(*b))
            .collect())
    }
}

impl DrawnPlan {
    /// What the ground's draws read.
    pub(crate) fn touches(&self) -> Vec<Touch> {
        self.buffers
            .iter()
            .map(|b| Touch::Buffer(*b, Use::StorageRead))
            .collect()
    }

    /// The push of the grass field's placing: the field's constants, the clumps it writes, the
    /// draws' commands and the bytes of one, and the weather's constants (`NONE`: none).
    pub(crate) fn grass_push(
        &self,
        constants: u32,
        clumps: u32,
        args: u32,
        stride: u32,
        weather: u32,
    ) -> Vec<u8> {
        let mut w = self.words;
        w[6] = constants;
        w[7] = clumps;
        w[8] = args;
        w[12] = stride;
        let mut out = push_words(&w);
        out.extend(weather.to_le_bytes());
        out
    }

    fn push(&self, frame: u32, wrap: u32, clamp: u32, first: u32) -> Vec<u8> {
        let mut w = self.words;
        w[0] = frame;
        w[10] = wrap;
        w[11] = clamp;
        w[12] = first;
        push_words(&w)
    }

    /// Draw the ground into the G-buffer, seen through the frame's constants `frame`, wet and
    /// puddled as the weather's constants `weather` say (`NONE`: dry).
    ///
    /// # Errors
    /// What the layer refuses.
    pub(crate) fn draw_ground(
        &self,
        d: &mut Draw<'_>,
        frame: u32,
        wrap: u32,
        clamp: u32,
        weather: u32,
    ) -> Result<(), dereth_gfx::Error> {
        if self.view.1 == 0 {
            return Ok(());
        }
        d.set_pipeline(self.pipelines.ground)?;
        let mut push = self.push(frame, wrap, clamp, self.view.0);
        push.extend(weather.to_le_bytes());
        d.push(0, &push)?;
        d.draw(PATCH_VERTICES, self.view.1, 0, 0);
        Ok(())
    }

    /// Draw the ground into a shadow cascade's map, seen through the cascade's constants.
    ///
    /// # Errors
    /// What the layer refuses.
    pub(crate) fn draw_casters(
        &self,
        d: &mut Draw<'_>,
        cascade: u32,
    ) -> Result<(), dereth_gfx::Error> {
        if self.casters.1 == 0 {
            return Ok(());
        }
        d.set_pipeline(self.pipelines.caster)?;
        d.push(0, &self.push(cascade, 0, 0, self.casters.0))?;
        d.draw(PATCH_VERTICES, self.casters.1, 0, 0);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental)
    use super::*;

    fn always(_: Vec3, _: Vec3) -> bool {
        true
    }

    /// **Patches grow coarser away from the eye, and neighbors differ by one level at most**, so
    /// the slides that close the seams between levels always meet a level just one coarser.
    #[test]
    fn patches_grow_coarser_away_from_the_eye_and_neighbors_differ_by_one_level_at_most() {
        let eye = Vec3::new(20_000.0, 20_000.0, 50.0);
        let patches = select(eye, 1.0, 20_000.0, &always);
        assert!(patches.len() > 50, "{} patches", patches.len());
        let near = patches.first().unwrap();
        let far = patches.last().unwrap();
        assert_eq!(near.level, 0, "the nearest is the finest");
        assert!(far.level > 4, "the farthest is coarse: {}", far.level);
        for a in &patches {
            for b in &patches {
                let (sa, sb) = (a.spacing * 16.0, b.spacing * 16.0);
                let touch = a.at.x <= b.at.x + sb
                    && b.at.x <= a.at.x + sa
                    && a.at.y <= b.at.y + sb
                    && b.at.y <= a.at.y + sa;
                if touch {
                    assert!(
                        a.level.abs_diff(b.level) <= 1,
                        "levels {} and {} touch",
                        a.level,
                        b.level
                    );
                }
            }
        }
    }

    /// **A far level's ring reaches farther than a near one's**: from level 4 up eight of its nodes,
    /// so far mountains are drawn from a grid fine enough to keep their shape; nearer, two and a
    /// half. The shadow casters keep every ring at two and a half, so they draw no more patches.
    #[test]
    fn far_levels_reach_farther_and_the_shadow_casters_keep_the_near_rings() {
        let node = |l: u32| 32.0 * f32::from(1u16 << l);
        for l in 0..FAR_LEVEL {
            assert_eq!(reach(l, 1.0), node(l) * 2.5, "level {l}");
        }
        for l in FAR_LEVEL..=TOP {
            assert_eq!(reach(l, 1.0), node(l) * 8.0, "level {l}");
            assert_eq!(caster_reach(l, 1.0), node(l) * 2.5, "level {l}'s casters");
        }
        // The same stretch of ground: the view draws it finer than the casters do.
        let eye = Vec3::new(20_000.0, 20_000.0, 50.0);
        let finest_past = |patches: &[Patch], meters: f32| {
            patches
                .iter()
                .filter(|p| (p.at - eye.truncate()).length() > meters)
                .map(|p| p.level)
                .min()
        };
        let view = select(eye, 1.0, 2_500.0, &always);
        let casters = select_casters(eye, 1.0, 2_500.0);
        assert_eq!(finest_past(&view, 2_000.0), Some(4));
        assert!(finest_past(&casters, 2_000.0) > Some(4));
        assert!(casters.len() < view.len());
    }

    /// **Every edge two patches of different levels share has the same vertices on both sides**:
    /// where a patch meets a coarser one, its vertices have slid all the way onto the coarser
    /// grid.
    #[test]
    fn every_edge_two_patches_share_has_the_same_vertices_on_both_sides() {
        let eye = Vec3::new(10_003.0, 9_991.0, 40.0);
        let patches = select(eye, 1.0, 4_000.0, &always);
        let e = eye.truncate();
        let mut checked = 0;
        for a in &patches {
            for b in &patches {
                if b.level != a.level + 1 {
                    continue;
                }
                // a's east edge on b's west edge.
                let sa = a.spacing * 16.0;
                if (a.at.x + sa - b.at.x).abs() < 1e-3
                    && a.at.y >= b.at.y - 1e-3
                    && a.at.y + sa <= b.at.y + b.spacing * 16.0 + 1e-3
                {
                    for j in 0..=16u32 {
                        let va = vertex_of(a, (16, j), e);
                        // The coarser grid's vertex at the same place, or between two of its
                        // vertices, where a's odd vertex slid onto its even neighbor.
                        let on_coarse = ((va.y - b.at.y) / b.spacing).fract().abs() < 1e-3
                            || ((va.y - b.at.y) / b.spacing).fract().abs() > 1.0 - 1e-3;
                        assert!(on_coarse, "{va:?} is not on the coarser grid");
                        checked += 1;
                    }
                }
            }
        }
        assert!(checked > 0, "an edge between levels was checked");
    }

    /// A block takes the slot its coordinates name, wrapping.
    #[test]
    fn a_block_takes_the_slot_its_coordinates_name_wrapping() {
        assert_eq!(slot_of(0xA9B4, 5), (0xA9 % 5, 0xB4 % 5));
        assert_eq!(slot_of(0x0000, 5), (0, 0));
    }
}
