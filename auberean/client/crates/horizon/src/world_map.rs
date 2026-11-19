//! The map window's picture, drawn from the world's landblocks as Dereth's Horizon draws its own
//! world: the land in its far landscape's colors lit by its relief, the roads, the sea with a
//! lighter band along the shore, and a grid line every five units of the map's coordinates.
//!
//! The picture draws a pixel to each square of the land ([`SQUARE_PX`]), sixteen to a landblock's
//! side, north at its top. A [`MapGrid`] holds the world's vertices as the landblocks give them:
//! each one's height, its far color (as the far landscape colors it), whether a road passes and
//! whether it stands under water; [`MapGrid::prepare`] leaves out the placeholder land and works
//! out the light and the shore once, into a [`WorldMap`], which draws any window of the picture, a [`TILE`] of it or the whole
//! shrunk to [`OVERVIEW`]. [`MapBuild`] does the reading, the preparing and the overview off the
//! frame, through the drawing both games' maps share ([`dereth_map_picture`]).

use std::collections::HashMap;
use std::sync::Arc;

use auberean_assets::landblock::Landblock;
use auberean_assets::region::{SurfaceDesc, TerrainDesc};
use auberean_world::land::LandConstants;
use auberean_world::records::{LandblockReader, RegionSet};
use auberean_world::terrain::{far_colors, LandSource, Surfaces};
use auberean_world::water::WaterSet;
use dereth_map_picture::build::{OffFrame, Steps};
use dereth_map_picture::{mix, smoothstep, Rows};

pub use dereth_map_picture::Pixels;

/// The picture's pixels to a square of the land (a landblock is sixteen squares a side).
pub const SQUARE_PX: usize = 1;
/// A tile's side, in the picture's pixels.
pub const TILE: usize = 1024;
/// The longer side of the whole picture shrunk.
pub const OVERVIEW: usize = 2048;
/// How much steeper than it is the light draws the land.
const RELIEF: f32 = 2.0;
/// How far from the shore the sea's lighter band reaches, and over how much more it fades, in
/// meters.
const HALO_M: f32 = 72.0;
const FADE_M: f32 = 24.0;
/// A shore distance past every one the band reads, in meters: a square's corners are within a
/// diagonal of each other.
const FAR_FROM_SHORE_M: f32 = 144.0;
/// The steps of a vertex's distance from the shore as it is kept: sixteenths of a vertex step.
const SHORE_STEPS: f32 = 16.0;
/// The grid's spacing, in the map's coordinates, and the fewest pixels apart its lines are drawn.
const GRID_EVERY: f64 = 5.0;
const GRID_FEWEST_PX: f64 = 16.0;
const GRID_COLOR: [u8; 3] = [72, 72, 72];
/// The open sea's color, the shore's and the roads', as Dereth's Horizon draws its own map's.
const SEA: [u8; 3] = [112, 133, 159];
const SHORE: [u8; 3] = [148, 167, 190];
const ROAD: [u8; 3] = [186, 181, 167];

/// What a vertex is, as [`MapGrid`] keeps it: a landblock laid it, it stands under its block's
/// water, a road passes.
const READ: u8 = 1;
const UNDER_WATER: u8 = 2;
const ROADED: u8 = 4;

/// How far, in landblocks, a speck of flat ground lies from any other land to be left out
/// ([`MapGrid::drop_placeholder_land`]).
const SPECK_GAP: usize = 32;

/// The sea's color, for a map that is not drawn yet.
#[must_use]
pub const fn sea_color() -> [u8; 3] {
    SEA
}

/// What the map is drawn from besides the landblocks: the landscape's constants, the surfaces the
/// far colors come from, the water, and the world's size in landblocks (across, up).
#[derive(Clone, Debug)]
pub struct MapLand {
    pub land: LandConstants,
    pub terrain: TerrainDesc,
    pub surface: SurfaceDesc,
    pub water: Option<WaterSet>,
    pub blocks: (usize, usize),
}

impl MapLand {
    /// What the region gives the map.
    #[must_use]
    pub fn of(region: &RegionSet) -> Self {
        let blocks = |n: u32| usize::try_from(n).unwrap_or(0);
        Self {
            land: region.land.clone(),
            terrain: region.terrain.clone(),
            surface: region.surface.clone(),
            water: region.water.clone(),
            blocks: (
                blocks(region.region.blocks_wide),
                blocks(region.region.blocks_long),
            ),
        }
    }

    fn surfaces(&self) -> Surfaces<'_> {
        Surfaces {
            terrain: &self.terrain,
            surface: &self.surface,
        }
    }

    /// Squares to a landblock's side.
    fn squares(&self) -> usize {
        self.land
            .grid()
            .vertices_per_side()
            .saturating_sub(1)
            .max(1)
    }

    /// An empty grid of the world's vertices.
    #[must_use]
    pub fn grid(&self) -> MapGrid {
        MapGrid::new(
            self.blocks,
            self.squares(),
            self.land.grid().vertex_spacing(),
            self.land.heights,
        )
    }

    /// Landblock `(x, y)` of `source` as the map lays it: its vertices' heights, far colors,
    /// roads and water; `None` when the world has no landblock there.
    #[must_use]
    pub fn lay(&self, source: &dyn LandSource, x: i32, y: i32) -> Option<Laid> {
        let lb = source.landblock(x, y)?;
        let colors = far_colors(&self.land, self.surfaces(), source, x, y)?;
        let lowest = lb
            .heights
            .iter()
            .map(|&h| self.land.heights[usize::from(h)])
            .fold(f32::INFINITY, f32::min);
        let surface = self
            .water
            .as_ref()
            .and_then(|w| w.block(&self.land, x, y, lowest))
            .map(|w| w.height);
        let roads = self.surfaces();
        let flags = lb
            .heights
            .iter()
            .zip(&lb.vertices)
            .map(|(&h, &word)| {
                let mut f = READ;
                if surface.is_some_and(|s| self.land.heights[usize::from(h)] < s) {
                    f |= UNDER_WATER;
                }
                if roads.road_surface(self.land.word.road.of(word)).is_some() {
                    f |= ROADED;
                }
                f
            })
            .collect();
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // a color channel, clamped to its range
        let byte = |c: f32| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
        Some(Laid {
            heights: lb.heights.clone(),
            colors: colors
                .iter()
                .map(|c| [byte(c.r), byte(c.g), byte(c.b)])
                .collect(),
            flags,
        })
    }
}

/// A landblock as the map lays it, a vertex at a time, x major as the landblock keeps them.
#[derive(Clone, Debug, PartialEq)]
pub struct Laid {
    /// Height-table indexes.
    pub heights: Vec<u8>,
    /// Far colors, red, green, blue.
    pub colors: Vec<[u8; 3]>,
    flags: Vec<u8>,
}

/// The world's vertices as the landblocks give them.
#[derive(Clone)]
pub struct MapGrid {
    blocks: (usize, usize),
    /// Squares to a block's side.
    squares: usize,
    /// Vertices across and up.
    verts: (usize, usize),
    /// Meters between neighboring vertices.
    spacing: f32,
    heights: [f32; 256],
    /// Height-table index, far color and what it is ([`READ`] and the rest), by vertex, row by row
    /// from the south.
    height: Vec<u8>,
    color: Vec<[u8; 3]>,
    flags: Vec<u8>,
    /// How many landblocks were laid.
    pub laid: usize,
}

impl std::fmt::Debug for MapGrid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MapGrid")
            .field("blocks", &self.blocks)
            .field("laid", &self.laid)
            .finish_non_exhaustive()
    }
}

impl MapGrid {
    /// A world of `blocks` landblocks (across, up) of `squares` squares a side, its vertices
    /// `spacing` meters apart, with no land laid, over the height table `heights`.
    #[must_use]
    pub fn new(blocks: (usize, usize), squares: usize, spacing: f32, heights: [f32; 256]) -> Self {
        let verts = (blocks.0 * squares + 1, blocks.1 * squares + 1);
        let n = verts.0 * verts.1;
        Self {
            blocks,
            squares,
            verts,
            spacing,
            heights,
            height: vec![0; n],
            color: vec![[0; 3]; n],
            flags: vec![0; n],
            laid: 0,
        }
    }

    /// The picture's size, in its pixels.
    #[must_use]
    pub fn picture_size(&self) -> (usize, usize) {
        (
            (self.verts.0 - 1) * SQUARE_PX,
            (self.verts.1 - 1) * SQUARE_PX,
        )
    }

    /// Set vertex `(x, y)`, counted from the south-west: its height-table index, its far color,
    /// whether it is under water and whether a road passes.
    pub fn set(
        &mut self,
        (x, y): (usize, usize),
        height: u8,
        color: [u8; 3],
        wet: bool,
        road: bool,
    ) {
        if x < self.verts.0 && y < self.verts.1 {
            let i = y * self.verts.0 + x;
            self.height[i] = height;
            self.color[i] = color;
            self.flags[i] =
                READ | if wet { UNDER_WATER } else { 0 } | if road { ROADED } else { 0 };
        }
    }

    /// Lay landblock `(bx, by)` on the grid.
    pub fn lay(&mut self, (bx, by): (usize, usize), laid: &Laid) {
        let n = self.squares + 1;
        if laid.heights.len() < n * n || laid.colors.len() < n * n || laid.flags.len() < n * n {
            return;
        }
        for i in 0..n {
            for j in 0..n {
                let (x, y) = (bx * self.squares + i, by * self.squares + j);
                if x < self.verts.0 && y < self.verts.1 {
                    let (k, at) = (n * i + j, y * self.verts.0 + x);
                    self.height[at] = laid.heights[k];
                    self.color[at] = laid.colors[k];
                    self.flags[at] = laid.flags[k];
                }
            }
        }
        self.laid += 1;
    }

    /// Whether vertex `i` is sea: no landblock laid it, or it stands under water.
    fn wet(&self, i: usize) -> bool {
        self.flags[i] & READ == 0 || self.flags[i] & UNDER_WATER != 0
    }

    /// The placeholder land left out, and the light and the shore worked out: everything the
    /// picture is drawn from.
    #[must_use]
    pub fn prepare(mut self) -> WorldMap {
        self.drop_placeholder_land();
        let w = self.verts.0;
        let h = |x: usize, y: usize| self.heights[usize::from(self.height[y * w + x])];
        let light = dereth_map_picture::relief_light(self.verts, self.spacing, RELIEF, h);
        let far = FAR_FROM_SHORE_M / self.spacing.max(1e-3);
        let distance = dereth_map_picture::distance_from(self.verts, far, |i| !self.wet(i));
        let land = self
            .color
            .iter()
            .zip(&light)
            .map(|(c, l)| {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                // a color channel, clamped to its range
                c.map(|v| (f32::from(v) * l).round().clamp(0.0, 255.0) as u8)
            })
            .collect();
        let sea = distance
            .iter()
            .enumerate()
            .map(|(i, d)| {
                if self.wet(i) {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    // within 0..=254
                    let steps = (d * SHORE_STEPS).round().clamp(0.0, 254.0) as u8;
                    steps + 1
                } else {
                    0
                }
            })
            .collect();
        let road = self.flags.iter().map(|f| f & ROADED != 0).collect();
        let masses = self.landmasses();
        let bounds = self.land_bounds();
        WorldMap::new(&self, land, sea, road, masses, bounds)
    }

    /// The stretches of land, largest first: the landblocks holding any dry ground, joined where
    /// they touch (a corner will do).
    #[must_use]
    pub fn landmasses(&self) -> Vec<Landmass> {
        let bw = self.blocks.0;
        let mut masses: Vec<Landmass> = self
            .stretches()
            .iter()
            .map(|blocks| {
                let (mut sx, mut sy) = (0.0f64, 0.0f64);
                for &k in blocks {
                    #[allow(clippy::cast_precision_loss)] // a block within the world
                    let (fx, fy) = ((k % bw) as f64, (k / bw) as f64);
                    (sx, sy) = (sx + fx + 0.5, sy + fy + 0.5);
                }
                #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
                // a share of the world
                let centre = (
                    (sx / blocks.len() as f64 / bw as f64) as f32,
                    (sy / blocks.len() as f64 / self.blocks.1 as f64) as f32,
                );
                Landmass {
                    blocks: blocks.len(),
                    centre,
                }
            })
            .collect();
        masses.sort_by_key(|m| std::cmp::Reverse(m.blocks));
        masses
    }

    /// The bounds of the land, the placeholder land left out first ([`Self::prepare`]); `None`
    /// with no land.
    #[must_use]
    pub fn land_bounds(&self) -> Option<LandBounds> {
        let (bw, bh) = self.blocks;
        let land: Vec<usize> = self.stretches().into_iter().flatten().collect();
        let xs = land.iter().map(|k| k % bw);
        let ys = land.iter().map(|k| k / bw);
        let (x0, x1) = (xs.clone().min()?, xs.max()?);
        let (y0, y1) = (ys.clone().min()?, ys.max()?);
        #[allow(clippy::cast_precision_loss)] // a block within the world
        let share = |n: usize, of: usize| n as f32 / of.max(1) as f32;
        Some(LandBounds {
            west: share(x0, bw),
            south: share(y0, bh),
            east: share(x1 + 1, bw),
            north: share(y1 + 1, bh),
        })
    }

    /// Whether landblock `(bx, by)` holds any dry ground.
    fn dry_block(&self, bx: usize, by: usize) -> bool {
        let n = self.squares;
        (0..=n).any(|i| {
            (0..=n).any(|j| {
                let (x, y) = (bx * n + i, by * n + j);
                x < self.verts.0 && y < self.verts.1 && !self.wet(y * self.verts.0 + x)
            })
        })
    }

    /// Leave out the land no one was meant to see, as sea: each stretch of land that is a copy of
    /// another, its landblocks laid out as the other's are and its heights the other's at all but
    /// a hundredth of their vertices (the placeholder islands in the far north are copies of one
    /// island, touched up here and there; no real land repeats another); and each speck of
    /// dead-flat ground, one height wherever it is dry, more than [`SPECK_GAP`] landblocks from
    /// all the larger land left (real land lies within a few landblocks of more). Returns what it
    /// left out, in the order the stretches begin, row by row from the south.
    pub fn drop_placeholder_land(&mut self) -> Vec<Placeholder> {
        let bw = self.blocks.0;
        let stretches = self.stretches();
        // Each stretch as it would be left out, and its blocks from its south-west corner, in
        // order.
        let laid_out: Vec<(Placeholder, Vec<(usize, usize)>)> = stretches
            .iter()
            .map(|blocks| {
                let xs = blocks.iter().map(|k| k % bw);
                let ys = blocks.iter().map(|k| k / bw);
                let sw = (xs.clone().min().unwrap_or(0), ys.clone().min().unwrap_or(0));
                let ne = (xs.max().unwrap_or(0), ys.max().unwrap_or(0));
                let mut shape: Vec<_> = blocks
                    .iter()
                    .map(|k| (k % bw - sw.0, k / bw - sw.1))
                    .collect();
                shape.sort_unstable();
                let stretch = Placeholder {
                    why: Why::Copy,
                    blocks: blocks.len(),
                    south_west: sw,
                    north_east: ne,
                };
                (stretch, shape)
            })
            .collect();
        let mut alike: HashMap<&[(usize, usize)], Vec<usize>> = HashMap::new();
        for (n, (_, shape)) in laid_out.iter().enumerate() {
            alike.entry(shape.as_slice()).or_default().push(n);
        }
        let mut copy = vec![false; stretches.len()];
        for same in alike.values().filter(|s| s.len() > 1) {
            for (i, &a) in same.iter().enumerate() {
                for &b in &same[i + 1..] {
                    let (from, to) = (laid_out[a].0.south_west, laid_out[b].0.south_west);
                    if self.heights_match(from, to, &laid_out[a].1) {
                        (copy[a], copy[b]) = (true, true);
                    }
                }
            }
        }
        // Landblocks between two stretches' bounds, along whichever way they are further apart.
        let gap = |a: &Placeholder, b: &Placeholder| {
            let across = (a.south_west.0.saturating_sub(b.north_east.0))
                .max(b.south_west.0.saturating_sub(a.north_east.0));
            let up = (a.south_west.1.saturating_sub(b.north_east.1))
                .max(b.south_west.1.saturating_sub(a.north_east.1));
            across.max(up)
        };
        let mut left_out: Vec<Option<Why>> = copy.iter().map(|&c| c.then_some(Why::Copy)).collect();
        for (k, (stretch, shape)) in laid_out.iter().enumerate() {
            // The land larger than it: the largest stretch is never a speck.
            let mut others = laid_out
                .iter()
                .enumerate()
                .filter(|&(o, (other, _))| !copy[o] && other.blocks > stretch.blocks)
                .peekable();
            let alone = others.peek().is_some()
                && others.all(|(_, (other, _))| gap(stretch, other) > SPECK_GAP);
            if !copy[k] && alone && self.dead_flat(stretch.south_west, shape) {
                left_out[k] = Some(Why::Speck);
            }
        }
        let n = self.squares;
        let mut dropped = Vec::new();
        for ((mut stretch, shape), why) in laid_out.into_iter().zip(left_out) {
            let Some(why) = why else {
                continue;
            };
            stretch.why = why;
            for &(dx, dy) in &shape {
                let (bx, by) = (stretch.south_west.0 + dx, stretch.south_west.1 + dy);
                for i in 0..=n {
                    for j in 0..=n {
                        let (x, y) = (bx * n + i, by * n + j);
                        if x < self.verts.0 && y < self.verts.1 {
                            self.flags[y * self.verts.0 + x] = READ | UNDER_WATER;
                        }
                    }
                }
            }
            dropped.push(stretch);
        }
        dropped
    }

    /// Whether the ground of the blocks `shape` lays out from `at` is one height wherever it is
    /// dry.
    fn dead_flat(&self, at: (usize, usize), shape: &[(usize, usize)]) -> bool {
        let n = self.squares;
        let mut first = None;
        for &(dx, dy) in shape {
            for i in 0..=n {
                for j in 0..=n {
                    let (x, y) = ((at.0 + dx) * n + i, (at.1 + dy) * n + j);
                    let k = y * self.verts.0 + x;
                    if x >= self.verts.0 || y >= self.verts.1 || self.wet(k) {
                        continue;
                    }
                    if *first.get_or_insert(self.height[k]) != self.height[k] {
                        return false;
                    }
                }
            }
        }
        true
    }

    /// Whether the blocks `shape` lays out from `a` and from `b` hold the same heights at all but
    /// a hundredth of their vertices.
    fn heights_match(
        &self,
        a: (usize, usize),
        b: (usize, usize),
        shape: &[(usize, usize)],
    ) -> bool {
        let n = self.squares;
        let (mut same, mut all) = (0usize, 0usize);
        for &(dx, dy) in shape {
            for i in 0..=n {
                for j in 0..=n {
                    let at = |(bx, by): (usize, usize)| {
                        let (x, y) = ((bx + dx) * n + i, (by + dy) * n + j);
                        (x < self.verts.0 && y < self.verts.1)
                            .then(|| self.height[y * self.verts.0 + x])
                    };
                    all += 1;
                    same += usize::from(at(a).is_some() && at(a) == at(b));
                }
            }
        }
        all > 0 && same * 100 >= all * 99
    }

    /// The stretches of land as the landblocks they hold, by index (`x + y * across`), each in
    /// the order they were reached, the stretches in the order they begin, row by row from the south.
    #[must_use]
    pub fn stretches(&self) -> Vec<Vec<usize>> {
        let (bw, bh) = self.blocks;
        let land: Vec<bool> = (0..bw * bh)
            .map(|k| self.dry_block(k % bw, k / bw))
            .collect();
        let mut seen = vec![false; bw * bh];
        let mut stretches = Vec::new();
        for start in 0..bw * bh {
            if seen[start] || !land[start] {
                continue;
            }
            seen[start] = true;
            let mut todo = vec![start];
            let mut blocks = Vec::new();
            while let Some(k) = todo.pop() {
                let (x, y) = (k % bw, k / bw);
                blocks.push(k);
                for ny in y.saturating_sub(1)..=(y + 1).min(bh - 1) {
                    for nx in x.saturating_sub(1)..=(x + 1).min(bw - 1) {
                        let m = ny * bw + nx;
                        if land[m] && !seen[m] {
                            seen[m] = true;
                            todo.push(m);
                        }
                    }
                }
            }
            stretches.push(blocks);
        }
        stretches
    }
}

/// A stretch of land left out of the picture: why, how many landblocks it held, and its
/// south-west and north-east landblocks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placeholder {
    pub why: Why,
    pub blocks: usize,
    pub south_west: (usize, usize),
    pub north_east: (usize, usize),
}

/// Why a stretch of land was left out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Why {
    /// It copies another stretch.
    Copy,
    /// It is a speck of dead-flat ground far from any larger land.
    Speck,
}

/// The land's own bounds, as shares of the world across from the west and up from the south.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LandBounds {
    pub west: f32,
    pub south: f32,
    pub east: f32,
    pub north: f32,
}

/// A stretch of land: how many landblocks it holds, and its middle as a share of the world, across
/// from the west and up from the south.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Landmass {
    pub blocks: usize,
    pub centre: (f32, f32),
}

/// The world as the picture is drawn from it: each vertex's lit color, its distance from the shore
/// when it is sea, and its road, and the grid's lines.
pub struct WorldMap {
    verts: (usize, usize),
    spacing: f32,
    size: (usize, usize),
    land: Vec<[u8; 3]>,
    /// 0 for land; for sea, one more than its distance from the shore in [`SHORE_STEPS`].
    sea: Vec<u8>,
    road: Vec<bool>,
    /// The stretches of land, largest first, and the bounds the map frames.
    masses: Vec<Landmass>,
    bounds: Option<LandBounds>,
    /// Which of the picture's columns and rows are grid lines.
    grid_col: Vec<bool>,
    grid_row: Vec<bool>,
}

impl std::fmt::Debug for WorldMap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorldMap")
            .field("size", &self.size)
            .finish_non_exhaustive()
    }
}

/// Which of a side's `size` pixels are grid lines: every [`GRID_EVERY`] units of the map's
/// coordinates (a hundredth of half the side each) through its middle, the map's 0, short of its
/// edges at 100 either way; none on a side too short for them to stand apart.
fn grid_lines(size: usize) -> Vec<bool> {
    let mut lines = vec![false; size];
    #[allow(clippy::cast_precision_loss)] // a side of a few thousand pixels
    let (half, unit) = (size as f64 / 2.0, size as f64 / 200.0);
    if GRID_EVERY * unit < GRID_FEWEST_PX {
        return lines;
    }
    let mut k = 0.0;
    while k * GRID_EVERY < 100.0 {
        for at in [half + k * GRID_EVERY * unit, half - k * GRID_EVERY * unit] {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            // within the picture, checked below
            let px = at as usize;
            if let Some(l) = lines.get_mut(px) {
                *l = true;
            }
        }
        k += 1.0;
    }
    lines
}

impl WorldMap {
    fn new(
        grid: &MapGrid,
        land: Vec<[u8; 3]>,
        sea: Vec<u8>,
        road: Vec<bool>,
        masses: Vec<Landmass>,
        bounds: Option<LandBounds>,
    ) -> Self {
        let size = grid.picture_size();
        Self {
            verts: grid.verts,
            spacing: grid.spacing,
            size,
            land,
            sea,
            road,
            masses,
            bounds,
            grid_col: grid_lines(size.0),
            grid_row: grid_lines(size.1),
        }
    }

    /// The picture's size, in its pixels.
    #[must_use]
    pub fn size(&self) -> (usize, usize) {
        self.size
    }

    /// The stretches of land, largest first.
    #[must_use]
    pub fn landmasses(&self) -> &[Landmass] {
        &self.masses
    }

    /// The bounds of the land the map frames ([`MapGrid::land_bounds`]), `None` with no land.
    #[must_use]
    pub fn land_bounds(&self) -> Option<LandBounds> {
        self.bounds
    }

    /// The size of the whole picture shrunk: [`OVERVIEW`] along its longer side.
    #[must_use]
    pub fn overview_size(&self) -> (usize, usize) {
        let long = self.size.0.max(self.size.1).max(1);
        (
            (self.size.0 * OVERVIEW).div_ceil(long),
            (self.size.1 * OVERVIEW).div_ceil(long),
        )
    }

    /// Tiles across and up; the last column and row are short.
    #[must_use]
    pub fn tiles(&self) -> (usize, usize) {
        (self.size.0.div_ceil(TILE), self.size.1.div_ceil(TILE))
    }

    /// The `w`×`h` window of the picture at pixel `(x0, y0)`.
    #[must_use]
    pub fn window(&self, at: (usize, usize), size: (usize, usize)) -> Pixels {
        dereth_map_picture::window(self, at, size)
    }

    /// Tile `(col, row)`: the picture's [`TILE`]-pixel square there, short at the east and south.
    #[must_use]
    pub fn tile(&self, (col, row): (usize, usize)) -> Pixels {
        self.window((col * TILE, row * TILE), (TILE, TILE))
    }

    /// The whole picture shrunk to [`Self::overview_size`].
    #[must_use]
    pub fn overview(&self) -> Pixels {
        dereth_map_picture::shrink(self, self.overview_size())
    }
}

impl Rows for WorldMap {
    fn size(&self) -> (usize, usize) {
        self.size
    }

    /// Picture row `j`, columns from `i0`: each pixel the mix of the four vertices about its
    /// middle.
    fn draw_row(&self, j: usize, i0: usize, out: &mut [u8]) {
        let grid = [GRID_COLOR[2], GRID_COLOR[1], GRID_COLOR[0], 255];
        if self.grid_row[j] {
            for px in out.as_chunks_mut::<4>().0 {
                *px = grid;
            }
            return;
        }
        let (w, h) = self.verts;
        let (last_x, last_y) = (w - 1, h - 1);
        #[allow(clippy::cast_precision_loss)] // a pixel within the picture
        let v = (last_y as f32 - (j as f32 + 0.5) / SQUARE_PX as f32).clamp(0.0, last_y as f32);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // clamped to the grid above
        let y0 = (v.floor() as usize).min(last_y - 1);
        #[allow(clippy::cast_precision_loss)]
        let fy = v - y0 as f32;
        let to_f = |c: [u8; 3]| c.map(f32::from);
        let (sea, halo, road_color) = (to_f(SEA), to_f(SHORE), to_f(ROAD));
        for (n, px) in out.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let i = i0 + n;
            if self.grid_col[i] {
                *px = grid;
                continue;
            }
            #[allow(clippy::cast_precision_loss)]
            let u = ((i as f32 + 0.5) / SQUARE_PX as f32).clamp(0.0, last_x as f32);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let x0 = (u.floor() as usize).min(last_x - 1);
            #[allow(clippy::cast_precision_loss)]
            let fx = u - x0 as f32;
            let corners = [
                (y0 * w + x0, (1.0 - fx) * (1.0 - fy)),
                (y0 * w + x0 + 1, fx * (1.0 - fy)),
                ((y0 + 1) * w + x0, (1.0 - fx) * fy),
                ((y0 + 1) * w + x0 + 1, fx * fy),
            ];
            let mut land = [0.0f32; 3];
            let mut land_weight = 0.0;
            let (mut wet, mut road, mut shore) = (0.0, 0.0, 0.0);
            for &(k, wt) in &corners {
                if self.sea[k] > 0 {
                    wet += wt;
                    shore += wt * f32::from(self.sea[k] - 1) / SHORE_STEPS * self.spacing;
                } else {
                    for (c, v) in land.iter_mut().zip(self.land[k]) {
                        *c += wt * f32::from(v);
                    }
                    land_weight += wt;
                    if self.road[k] {
                        road += wt;
                    }
                }
            }
            if land_weight > 0.0 {
                for c in &mut land {
                    *c /= land_weight;
                }
            }
            let sea = mix(halo, sea, ((shore - HALO_M) / FADE_M).clamp(0.0, 1.0));
            let mut rgb = mix(land, sea, smoothstep(0.35, 0.65, wet));
            rgb = mix(rgb, road_color, smoothstep(0.4, 0.6, road));
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            // a color channel, clamped to its range
            let byte = |c: f32| c.round().clamp(0.0, 255.0) as u8;
            *px = [byte(rgb[2]), byte(rgb[1]), byte(rgb[0]), 255];
        }
    }
}

/// A world map built: what tiles are drawn from, and the overview when it was asked for.
#[derive(Debug)]
pub struct Built {
    pub map: Arc<WorldMap>,
    pub overview: Option<Pixels>,
}

/// The landblocks of a few columns about the one being laid, which its vertices' normals reach
/// into.
struct Columns {
    reader: LandblockReader,
    rows: i32,
    blocks: HashMap<(i32, i32), Landblock>,
}

impl Columns {
    fn new(reader: LandblockReader, rows: usize) -> Self {
        Self {
            reader,
            rows: i32::try_from(rows).unwrap_or(0),
            blocks: HashMap::new(),
        }
    }

    /// Columns `x - 1` to `x + 1` held, and no others.
    fn about(&mut self, x: i32) {
        self.blocks
            .retain(|&(bx, _), _| (x - 1..=x + 1).contains(&bx));
        for bx in x - 1..=x + 1 {
            if self.blocks.keys().any(|&(c, _)| c == bx) || bx < 0 {
                continue;
            }
            for by in 0..self.rows {
                if let Some(lb) = self.reader.landblock(bx, by) {
                    self.blocks.insert((bx, by), lb);
                }
            }
        }
    }
}

impl LandSource for Columns {
    fn landblock(&self, x: i32, y: i32) -> Option<&Landblock> {
        self.blocks.get(&(x, y))
    }
}

/// Column `x`'s landblocks as the map lays them, south to north.
fn lay_column(land: &MapLand, columns: &mut Columns, x: i32) -> Vec<(usize, Laid)> {
    columns.about(x);
    (0..land.blocks.1)
        .filter_map(|by| Some((by, land.lay(columns, x, i32::try_from(by).ok()?)?)))
        .collect()
}

/// The work of a [`MapBuild`], a step at a time.
struct Build {
    reader: LandblockReader,
    land: Arc<MapLand>,
    overview: bool,
    /// How many threads lay the columns where there are threads.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    threads: usize,
    stage: Stage,
}

enum Stage {
    Start,
    /// The landblocks laid up to a column.
    Reading(Box<MapGrid>, Box<Columns>, usize),
    Preparing(Box<MapGrid>),
    /// The overview drawn up to a row.
    Overview(
        Arc<WorldMap>,
        Vec<Vec<(usize, f64)>>,
        Vec<Vec<(usize, f64)>>,
        Vec<u8>,
        usize,
    ),
    Done,
}

impl Build {
    /// The overview of `map` to draw, a band at a time.
    fn overview_of(map: WorldMap) -> Stage {
        let ((w, h), to) = (map.size(), map.overview_size());
        let across = dereth_map_picture::spans(w, to.0);
        let down = dereth_map_picture::spans(h, to.1);
        Stage::Overview(Arc::new(map), across, down, vec![0; to.0 * to.1 * 4], 0)
    }
}

impl Steps for Build {
    type Done = Built;

    /// One step: a column of landblocks laid, the preparing, or a band of the overview.
    fn step(&mut self) -> Option<Result<Built, String>> {
        let stage = std::mem::replace(&mut self.stage, Stage::Done);
        self.stage = match stage {
            Stage::Start => {
                let columns = Box::new(Columns::new(self.reader.clone(), self.land.blocks.1));
                Stage::Reading(Box::new(self.land.grid()), columns, 0)
            }
            Stage::Reading(mut grid, mut columns, bx) if bx < self.land.blocks.0 => {
                let x = i32::try_from(bx).unwrap_or(i32::MAX);
                for (by, laid) in lay_column(&self.land, &mut columns, x) {
                    grid.lay((bx, by), &laid);
                }
                Stage::Reading(grid, columns, bx + 1)
            }
            Stage::Reading(grid, ..) if grid.laid == 0 => {
                return Some(Err("the cell file holds no landblock".to_owned()));
            }
            Stage::Reading(grid, ..) => Stage::Preparing(grid),
            Stage::Preparing(grid) => {
                let map = grid.prepare();
                if !self.overview {
                    return Some(Ok(Built {
                        map: Arc::new(map),
                        overview: None,
                    }));
                }
                Self::overview_of(map)
            }
            Stage::Overview(map, across, down, mut bgra, row) if row < down.len() => {
                let band = 16;
                let line = across.len() * 4;
                let end = ((row + band) * line).min(bgra.len());
                dereth_map_picture::shrink_rows(
                    &*map,
                    &across,
                    &down,
                    row,
                    &mut bgra[row * line..end],
                );
                Stage::Overview(map, across, down, bgra, row + band)
            }
            Stage::Overview(map, across, down, bgra, _) => {
                let overview = Pixels::new((across.len(), down.len()), bgra);
                return Some(Ok(Built {
                    map,
                    overview: Some(overview),
                }));
            }
            Stage::Done => return Some(Err("the map was built already".to_owned())),
        };
        None
    }

    /// Every step, the columns laid on [`Build::threads`] threads and the overview's bands shared
    /// among the machine's.
    #[cfg(not(target_arch = "wasm32"))]
    fn run(self) -> Result<Built, String> {
        let grid = laid_on(&self.reader, &self.land, self.threads);
        if grid.laid == 0 {
            return Err("the cell file holds no landblock".to_owned());
        }
        let map = Arc::new(grid.prepare());
        let overview = self.overview.then(|| map.overview());
        Ok(Built { map, overview })
    }
}

/// Every landblock `reader` reads over `land` laid on a grid, the columns read on `threads`
/// threads and laid from the west, each from the south, whichever is read first: a vertex two
/// landblocks share is the one laid last's, the same however many threads read them.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn laid_on(reader: &LandblockReader, land: &MapLand, threads: usize) -> MapGrid {
    /// The grid, the next column to lay on it, and the columns read before it, waiting.
    struct Laying {
        grid: MapGrid,
        next: usize,
        waiting: std::collections::BTreeMap<usize, Vec<(usize, Laid)>>,
    }
    let laying = std::sync::Mutex::new(Laying {
        grid: land.grid(),
        next: 0,
        waiting: std::collections::BTreeMap::new(),
    });
    let next = std::sync::Mutex::new(0usize);
    std::thread::scope(|scope| {
        for _ in 0..threads.min(land.blocks.0).max(1) {
            scope.spawn(|| {
                let mut columns = Columns::new(reader.clone(), land.blocks.1);
                loop {
                    let bx = {
                        let mut n = next
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        let bx = *n;
                        *n += 1;
                        bx
                    };
                    if bx >= land.blocks.0 {
                        break;
                    }
                    let x = i32::try_from(bx).unwrap_or(i32::MAX);
                    let laid = lay_column(land, &mut columns, x);
                    let mut guard = laying
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    let l = &mut *guard;
                    l.waiting.insert(bx, laid);
                    while let Some(column) = l.waiting.remove(&l.next) {
                        for (by, block) in column {
                            l.grid.lay((l.next, by), &block);
                        }
                        l.next += 1;
                    }
                }
            });
        }
    });
    laying
        .into_inner()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .grid
}

/// A world map read, prepared and drawn off the frame.
pub struct MapBuild(OffFrame<Build>);

impl std::fmt::Debug for MapBuild {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MapBuild")
    }
}

impl MapBuild {
    /// Build the map from the landblocks `reader` reads over `land`, with its overview when
    /// `overview` is set.
    #[must_use]
    pub fn start(reader: LandblockReader, land: Arc<MapLand>, overview: bool) -> Self {
        let threads = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
        Self::start_on(reader, land, overview, threads)
    }

    /// [`Self::start`], its columns laid on `threads` threads where there are threads.
    #[must_use]
    pub fn start_on(
        reader: LandblockReader,
        land: Arc<MapLand>,
        overview: bool,
        threads: usize,
    ) -> Self {
        let build = Build {
            reader,
            land,
            overview,
            threads,
            stage: Stage::Start,
        };
        Self(OffFrame::start("world map", build))
    }

    /// The map, once it is built. Where there are no threads this is where a share of the frame's
    /// time builds it.
    pub fn poll(&mut self) -> Option<Result<Built, String>> {
        self.0.poll()
    }

    /// The map, waited for.
    ///
    /// # Errors
    /// The world holds no landblock, or the build's thread stopped.
    pub fn wait(self) -> Result<Built, String> {
        self.0.wait()
    }
}
