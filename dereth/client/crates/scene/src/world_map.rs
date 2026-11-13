//! The world map's picture, drawn from the landblocks of whichever data the client plays with.
//!
//! The picture draws three of its pixels to a landcell, [`SIDE`] to a side: the land in its
//! terrain's colour shaded by its relief, the roads, the sea with a lighter band along the shore,
//! and a grid line every five units. The colours are the original map's, by terrain name
//! ([`PALETTE`]), so the older data draws as the later does.
//!
//! A [`MapGrid`] holds the landblocks' vertices as read; [`MapGrid::prepare`] leaves out the
//! placeholder land and works out the shore and the shading once, into a [`WorldMap`], which draws
//! any window of the picture or the whole shrunk to [`OVERVIEW`]. [`MapBuild`] does the reading,
//! the preparing and the overview off the frame: on a thread of its own, or where there are no
//! threads a slice of each frame.

use std::sync::{Arc, Mutex, PoisonError};

use dereth_assets::world::CellLandblock;
use dereth_assets::Region;
use dereth_dat::RetailDatStore;

/// Landcell vertices to a side of the world.
pub const VERTS: usize = 2041;
const LAST: usize = VERTS - 1;
/// The picture's pixels to a landcell.
pub const CELL_PX: usize = 3;
/// The picture's side, in its pixels.
pub const SIDE: usize = VERTS * CELL_PX;
/// A tile's side, in the picture's pixels.
pub const TILE: usize = 1024;
/// Tiles to a side; the last row and column are short.
pub const TILES: usize = SIDE.div_ceil(TILE);
/// The side of the whole picture shrunk.
pub const OVERVIEW: usize = 2048;
/// Landblocks to a side of the world.
const BLOCKS: i32 = 255;
/// A landcell's side, in metres.
const CELL_M: f32 = 24.0;
/// Where 0E and 0N fall on the picture, in its pixels.
const ORIGIN: (f64, f64) = (3060.0, 3063.0);
/// The grid's spacing, in the picture's pixels: five units.
const GRID_EVERY: f64 = 150.0;
const GRID_COLOUR: [u8; 3] = [72, 72, 72];
/// How much steeper than it is the shading draws the land.
const RELIEF: f32 = 2.0;
/// How many landcells from the shore the sea's lighter band reaches.
const HALO_CELLS: f32 = 3.0;
/// A shore distance past every one the band reads: a landcell's neighbours are within a diagonal.
const FAR_FROM_SHORE: f32 = HALO_CELLS + 3.0;
/// The colour of a terrain the palette does not name.
const UNNAMED: [u8; 3] = [128, 128, 128];

/// The original map's colours: each terrain's by its name, then the open sea's, the shore's and
/// the roads'.
pub const PALETTE: &[(&str, [u8; 3])] = &[
    ("BarrenRock", [92, 83, 51]),
    ("Grassland", [68, 79, 39]),
    ("Ice", [191, 193, 196]),
    ("LushGrass", [65, 78, 36]),
    ("MarshSparseSwamp", [69, 69, 40]),
    ("MudRichDirt", [72, 71, 46]),
    ("ObsidianPlain", [36, 35, 38]),
    ("PackedDirt", [88, 78, 49]),
    ("PatchyDirt", [69, 73, 37]),
    ("PatchyGrassland", [67, 80, 37]),
    ("sand-yellow", [131, 103, 60]),
    ("sand-grey", [92, 93, 52]),
    ("sand-rockStrewn", [119, 94, 58]),
    ("SedimentaryRock", [95, 83, 50]),
    ("SemiBarrenRock", [84, 88, 53]),
    ("Snow", [185, 185, 185]),
    ("forestfloor", [51, 63, 32]),
    ("SeaSlime", [126, 131, 121]),
    ("Argila", [151, 140, 106]),
    ("Volcano1", [48, 51, 34]),
    ("Volcano2", [72, 64, 44]),
    ("BlueIce", [168, 181, 183]),
    ("Moss", [89, 87, 55]),
    ("DarkMoss", [91, 90, 58]),
    ("olthoi", [81, 80, 46]),
    ("DesolateLands", [112, 133, 137]),
];
const SEA: [u8; 3] = [112, 133, 159];
const SHORE: [u8; 3] = [148, 167, 190];
const ROAD: [u8; 3] = [186, 181, 167];

/// The sea's colour, for a map that is not drawn yet.
#[must_use]
pub const fn sea_colour() -> [u8; 3] {
    SEA
}

/// Pixels of the picture, four bytes to a pixel in blue, green, red, alpha order.
#[derive(Clone, PartialEq, Eq)]
pub struct Pixels {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

impl std::fmt::Debug for Pixels {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Pixels({}x{})", self.width, self.height)
    }
}

/// The world's landcell vertices as the landblocks give them: the height, the terrain and whether
/// a road passes.
#[derive(Clone)]
pub struct MapGrid {
    /// Each terrain's name, by its index.
    names: Vec<String>,
    heights: [f32; 256],
    /// Height-table index, by vertex, row by row from the south.
    height: Vec<u8>,
    /// Terrain index, `u8::MAX` where no landblock was read.
    kind: Vec<u8>,
    road: Vec<bool>,
    /// How many landblocks were read.
    pub blocks: usize,
}

impl std::fmt::Debug for MapGrid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MapGrid")
            .field("terrains", &self.names.len())
            .field("blocks", &self.blocks)
            .finish_non_exhaustive()
    }
}

impl MapGrid {
    /// A world with no land, over terrains named `names` and the height table `heights`.
    #[must_use]
    pub fn new(names: Vec<String>, heights: [f32; 256]) -> Self {
        Self {
            names,
            heights,
            height: vec![0; VERTS * VERTS],
            kind: vec![u8::MAX; VERTS * VERTS],
            road: vec![false; VERTS * VERTS],
            blocks: 0,
        }
    }

    /// A world with no land, over the region's terrains and heights.
    #[must_use]
    pub fn of_region(region: &Region) -> Self {
        let names = region
            .terrain_types
            .iter()
            .map(|t| t.terrain_name.clone())
            .collect();
        Self::new(names, dereth_terrain::land::mesh::height_table(region))
    }

    /// Set vertex `(x, y)`, counted from the south-west: its terrain index, height-table index and
    /// road.
    pub fn set(&mut self, (x, y): (usize, usize), kind: u8, height: u8, road: bool) {
        if x <= LAST && y <= LAST {
            let i = y * VERTS + x;
            self.kind[i] = kind;
            self.height[i] = height;
            self.road[i] = road;
        }
    }

    /// Lay landblock `(bx, by)`'s vertices on the grid.
    pub fn add_landblock(&mut self, (bx, by): (usize, usize), land: &CellLandblock) {
        for x in 0..9 {
            for y in 0..9 {
                let kind = u8::try_from(land.terrain_type(x, y)).unwrap_or(u8::MAX);
                let height = land.height[x * 9 + y];
                self.set((bx * 8 + x, by * 8 + y), kind, height, land.road(x, y) != 0);
            }
        }
        self.blocks += 1;
    }

    /// Every landblock the store's cell file holds, laid on a grid over its region.
    ///
    /// # Errors
    /// The region does not read, or the cell file holds no landblock.
    pub fn read(store: &RetailDatStore) -> Result<Self, String> {
        let mut grid = Self::start_read(store)?;
        for bx in 0..BLOCKS {
            grid.read_column(store, bx);
        }
        grid.finish_read()
    }

    fn start_read(store: &RetailDatStore) -> Result<Self, String> {
        let region = dereth_world_data::landblock::load_region(store).map_err(|e| e.to_string())?;
        Ok(Self::of_region(&region))
    }

    /// Read the landblocks of column `bx`, south to north.
    fn read_column(&mut self, store: &RetailDatStore, bx: i32) {
        for by in 0..BLOCKS {
            if let Some(land) = dereth_client_runtime::world_build::read_landblock(store, bx, by) {
                #[allow(clippy::cast_sign_loss)] // LINT-OK: both within 0..255.
                self.add_landblock((bx as usize, by as usize), &land);
            }
        }
    }

    fn finish_read(self) -> Result<Self, String> {
        if self.blocks == 0 {
            return Err("the cell file holds no landblock".to_owned());
        }
        Ok(self)
    }

    /// Whether each terrain is water, by its index.
    fn water_kinds(&self) -> Vec<bool> {
        self.names
            .iter()
            .map(|n| n.to_ascii_lowercase().contains("water"))
            .collect()
    }

    /// Whether vertex `i` is water: a water terrain, or no landblock's.
    fn wet(&self, water: &[bool], i: usize) -> bool {
        water
            .get(usize::from(self.kind[i]))
            .copied()
            .unwrap_or(true)
    }

    /// Leave out the land no one was meant to see, as sea: each patch of land that reaches the
    /// world's edge, the four placeholder deserts in the far north, and any patch of a landblock's
    /// vertices or more that is all one terrain on dead-flat ground (the older data's sand in the
    /// far north; the real ones, islets, are a few dozen at most). Returns what it left out.
    pub fn drop_placeholder_land(&mut self) -> Vec<Placeholder> {
        /// The south-west corners of the placeholder deserts, east and north.
        const DESERTS: [(f64, f64); 4] =
            [(77.5, 78.5), (90.7, 69.1), (-102.0, 97.7), (-96.2, 98.1)];
        let water = self.water_kinds();
        let sea = self
            .names
            .iter()
            .position(|n| n == "WaterDeepSea")
            .or_else(|| water.iter().position(|w| *w))
            .and_then(|i| u8::try_from(i).ok())
            .unwrap_or(u8::MAX);
        let mut seen = vec![false; VERTS * VERTS];
        let mut patch: Vec<u32> = Vec::new();
        let mut dropped = Vec::new();
        for start in 0..VERTS * VERTS {
            if seen[start] || self.wet(&water, start) {
                continue;
            }
            seen[start] = true;
            patch.clear();
            patch.push(index(start));
            let mut next = 0;
            while next < patch.len() {
                let i = patch[next] as usize;
                next += 1;
                let (x, y) = (i % VERTS, i / VERTS);
                for ny in y.saturating_sub(1)..=(y + 1).min(LAST) {
                    for nx in x.saturating_sub(1)..=(x + 1).min(LAST) {
                        let n = ny * VERTS + nx;
                        if !seen[n] && !self.wet(&water, n) {
                            seen[n] = true;
                            patch.push(index(n));
                        }
                    }
                }
            }
            let (mut x0, mut y0, mut x1, mut y1) = (VERTS, VERTS, 0, 0);
            let first = patch[0] as usize;
            let (mut flat, mut one_kind) = (true, true);
            for &i in &patch {
                let i = i as usize;
                let (x, y) = (i % VERTS, i / VERTS);
                (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
                flat &= self.heights[usize::from(self.height[i])]
                    == self.heights[usize::from(self.height[first])];
                one_kind &= self.kind[i] == self.kind[first];
            }
            let why = if x0 == 0 || y0 == 0 || x1 == LAST || y1 == LAST {
                Why::Edge
            } else if DESERTS
                .iter()
                .any(|&(e, n)| (at(x0) - e).abs() < 0.06 && (at(y0) - n).abs() < 0.06)
            {
                Why::Desert
            } else if flat && one_kind && patch.len() >= 81 {
                Why::Flat
            } else {
                continue;
            };
            for &i in &patch {
                self.kind[i as usize] = sea;
                self.road[i as usize] = false;
            }
            dropped.push(Placeholder {
                vertices: patch.len(),
                why,
                south_west: (at(x0), at(y0)),
                north_east: (at(x1), at(y1)),
            });
        }
        dropped
    }

    /// Each vertex's distance from the nearest land, in landcells: two chamfer passes.
    fn shore_distance(&self, water: &[bool]) -> Vec<f32> {
        let mut d: Vec<f32> = (0..VERTS * VERTS)
            .map(|i| if self.wet(water, i) { f32::MAX } else { 0.0 })
            .collect();
        let diagonal = std::f32::consts::SQRT_2;
        let at = |x: usize, y: usize| y * VERTS + x;
        for y in 0..VERTS {
            for x in 0..VERTS {
                let mut best = d[at(x, y)];
                if x > 0 {
                    best = best.min(d[at(x - 1, y)] + 1.0);
                }
                if y > 0 {
                    best = best.min(d[at(x, y - 1)] + 1.0);
                    if x > 0 {
                        best = best.min(d[at(x - 1, y - 1)] + diagonal);
                    }
                    if x < LAST {
                        best = best.min(d[at(x + 1, y - 1)] + diagonal);
                    }
                }
                d[at(x, y)] = best;
            }
        }
        for y in (0..VERTS).rev() {
            for x in (0..VERTS).rev() {
                let mut best = d[at(x, y)];
                if x < LAST {
                    best = best.min(d[at(x + 1, y)] + 1.0);
                }
                if y < LAST {
                    best = best.min(d[at(x, y + 1)] + 1.0);
                    if x < LAST {
                        best = best.min(d[at(x + 1, y + 1)] + diagonal);
                    }
                    if x > 0 {
                        best = best.min(d[at(x - 1, y + 1)] + diagonal);
                    }
                }
                d[at(x, y)] = best;
            }
        }
        // Past the band every distance draws alike.
        for v in &mut d {
            *v = v.min(FAR_FROM_SHORE);
        }
        d
    }

    /// Each vertex's light, lit from the north-west: 1 on the flat, more on a slope facing the
    /// light and less on one facing away.
    fn shading(&self) -> Vec<f32> {
        let h = |x: usize, y: usize| self.heights[usize::from(self.height[y * VERTS + x])];
        let light = {
            let (x, y, z) = (-0.5f32, 0.5f32, std::f32::consts::FRAC_1_SQRT_2);
            let n = (x * x + y * y + z * z).sqrt();
            (x / n, y / n, z / n)
        };
        let mut out = vec![1.0; VERTS * VERTS];
        for (y, row) in out.chunks_mut(VERTS).enumerate() {
            for (x, lit) in row.iter_mut().enumerate() {
                let (xl, xr) = (x.saturating_sub(1), (x + 1).min(LAST));
                let (yd, yu) = (y.saturating_sub(1), (y + 1).min(LAST));
                #[allow(clippy::cast_precision_loss)] // LINT-OK: one or two landcells.
                let dzdx = (h(xr, y) - h(xl, y)) / ((xr - xl) as f32 * CELL_M);
                #[allow(clippy::cast_precision_loss)] // LINT-OK: one or two landcells.
                let dzdy = (h(x, yu) - h(x, yd)) / ((yu - yd) as f32 * CELL_M);
                let (nx, ny, nz) = (-RELIEF * dzdx, -RELIEF * dzdy, 1.0f32);
                let len = (nx * nx + ny * ny + nz * nz).sqrt();
                let facing = (nx * light.0 + ny * light.1 + nz * light.2) / len / light.2;
                *lit = (0.3 + 0.7 * facing).clamp(0.45, 1.45);
            }
        }
        out
    }

    /// The placeholder land left out, and the shore and the shading worked out: everything the
    /// picture is drawn from.
    #[must_use]
    pub fn prepare(mut self) -> WorldMap {
        self.drop_placeholder_land();
        let water = self.water_kinds();
        let distance = self.shore_distance(&water);
        let shade = self.shading();
        WorldMap::new(self, water, distance, shade)
    }
}

/// Why a patch of land was left out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Why {
    /// It reaches the world's edge.
    Edge,
    /// It is one of the placeholder deserts in the far north.
    Desert,
    /// It is a landblock or more of one terrain, with no rise or fall.
    Flat,
}

/// A patch of land left out: how many vertices, why, and its corners east and north.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placeholder {
    pub vertices: usize,
    pub why: Why,
    pub south_west: (f64, f64),
    pub north_east: (f64, f64),
}

/// Where vertex `v` stands on either axis, in the game's units.
fn at(v: usize) -> f64 {
    #[allow(clippy::cast_precision_loss)] // LINT-OK: a vertex within 0..2041.
    let v = v as f64;
    v / 10.0 - 101.95
}

/// Vertex index `i` as the patches keep it.
fn index(i: usize) -> u32 {
    u32::try_from(i).unwrap_or(u32::MAX)
}

/// The world as the picture is drawn from it: each vertex's terrain, road, distance from the shore
/// and light, and the colours.
pub struct WorldMap {
    kind: Vec<u8>,
    road: Vec<bool>,
    distance: Vec<f32>,
    shade: Vec<f32>,
    /// Each terrain's colour and whether it is water, by its index.
    land: Vec<[f32; 3]>,
    water: Vec<bool>,
    /// Which of the picture's columns and rows are grid lines.
    grid_col: Vec<bool>,
    grid_row: Vec<bool>,
}

impl std::fmt::Debug for WorldMap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorldMap")
            .field("terrains", &self.land.len())
            .finish_non_exhaustive()
    }
}

fn colour(c: [u8; 3]) -> [f32; 3] {
    c.map(f32::from)
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

/// For each of `to` pixels shrunk from `from`, the source pixels it covers and by how much.
fn spans(from: usize, to: usize) -> Vec<Vec<(usize, f64)>> {
    #[allow(clippy::cast_precision_loss)] // LINT-OK: sides of a few thousand pixels.
    let scale = from as f64 / to as f64;
    (0..to)
        .map(|o| {
            #[allow(clippy::cast_precision_loss)] // LINT-OK: as above.
            let (a, b) = (o as f64 * scale, (o + 1) as f64 * scale);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            // LINT-OK: within 0..=from.
            let (first, last) = (a.floor() as usize, (b.ceil() as usize).min(from));
            #[allow(clippy::cast_precision_loss)] // LINT-OK: as above.
            (first..last)
                .map(|s| (s, (b.min((s + 1) as f64) - a.max(s as f64)) / scale))
                .collect()
        })
        .collect()
}

/// Fill `out`, rows of `row_len` bytes, by `rows(first_row, band)` over bands of `band` rows,
/// shared among the machine's threads (on the calling thread alone where it has none to share).
fn in_bands(out: &mut [u8], row_len: usize, band: usize, rows: &(dyn Fn(usize, &mut [u8]) + Sync)) {
    #[cfg(target_arch = "wasm32")]
    let threads = 1;
    #[cfg(not(target_arch = "wasm32"))]
    let threads = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
    let len = row_len * band;
    let bands = out.len().div_ceil(len.max(1));
    if threads <= 1 || bands <= 1 {
        for (b, chunk) in out.chunks_mut(len.max(1)).enumerate() {
            rows(b * band, chunk);
        }
        return;
    }
    let queue = Mutex::new(out.chunks_mut(len).enumerate());
    std::thread::scope(|scope| {
        for _ in 0..threads.min(bands) {
            scope.spawn(|| loop {
                let next = queue.lock().unwrap_or_else(PoisonError::into_inner).next();
                let Some((b, chunk)) = next else { break };
                rows(b * band, chunk);
            });
        }
    });
}

impl WorldMap {
    fn new(grid: MapGrid, water: Vec<bool>, distance: Vec<f32>, shade: Vec<f32>) -> Self {
        let land = grid
            .names
            .iter()
            .map(|n| {
                let c = PALETTE.iter().find(|(name, _)| name == n);
                colour(c.map_or(UNNAMED, |(_, c)| *c))
            })
            .collect();
        let (mut grid_col, mut grid_row) = (vec![false; SIDE], vec![false; SIDE]);
        for k in -25i32..=25 {
            #[allow(clippy::cast_possible_truncation)] // LINT-OK: within a few thousand.
            let col = (ORIGIN.0 + GRID_EVERY * f64::from(k)) as i64;
            #[allow(clippy::cast_possible_truncation)] // LINT-OK: as above.
            let row = (ORIGIN.1 - GRID_EVERY * f64::from(k)) as i64;
            if let Some(c) = usize::try_from(col).ok().filter(|c| *c < SIDE) {
                grid_col[c] = true;
            }
            if let Some(r) = usize::try_from(row).ok().filter(|r| *r < SIDE) {
                grid_row[r] = true;
            }
        }
        Self {
            kind: grid.kind,
            road: grid.road,
            distance,
            shade,
            land,
            water,
            grid_col,
            grid_row,
        }
    }

    fn is_water(&self, i: usize) -> bool {
        self.water
            .get(usize::from(self.kind[i]))
            .copied()
            .unwrap_or(true)
    }

    /// Picture row `j`, columns from `i0`, into `out` at four bytes a pixel.
    fn draw_row(&self, j: usize, i0: usize, out: &mut [u8]) {
        let grid = [GRID_COLOUR[2], GRID_COLOUR[1], GRID_COLOUR[0], 255];
        if self.grid_row[j] {
            for px in out.as_chunks_mut::<4>().0 {
                *px = grid;
            }
            return;
        }
        #[allow(clippy::cast_precision_loss)] // LINT-OK: a pixel within the picture.
        let v = (LAST as f32 - (j as f32 - 1.0) / CELL_PX as f32).clamp(0.0, LAST as f32);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // LINT-OK: clamped to the grid above.
        let y0 = (v.floor() as usize).min(LAST - 1);
        #[allow(clippy::cast_precision_loss)] // LINT-OK: as above.
        let fy = v - y0 as f32;
        let (sea, halo, road_colour) = (colour(SEA), colour(SHORE), colour(ROAD));
        for (n, px) in out.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let i = i0 + n;
            if self.grid_col[i] {
                *px = grid;
                continue;
            }
            #[allow(clippy::cast_precision_loss)] // LINT-OK: as above.
            let u = ((i as f32 - 1.0) / CELL_PX as f32).clamp(0.0, LAST as f32);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            // LINT-OK: as above.
            let x0 = (u.floor() as usize).min(LAST - 1);
            #[allow(clippy::cast_precision_loss)] // LINT-OK: as above.
            let fx = u - x0 as f32;
            let corners = [
                (y0 * VERTS + x0, (1.0 - fx) * (1.0 - fy)),
                (y0 * VERTS + x0 + 1, fx * (1.0 - fy)),
                ((y0 + 1) * VERTS + x0, (1.0 - fx) * fy),
                ((y0 + 1) * VERTS + x0 + 1, fx * fy),
            ];
            let mut land = [0.0f32; 3];
            let mut land_weight = 0.0;
            let (mut wet, mut road, mut shore) = (0.0, 0.0, 0.0);
            for &(k, w) in &corners {
                shore += w * self.distance[k];
                if self.is_water(k) {
                    wet += w;
                } else {
                    let base = self.land[usize::from(self.kind[k])];
                    for c in 0..3 {
                        land[c] += w * base[c] * self.shade[k];
                    }
                    land_weight += w;
                    if self.road[k] {
                        road += w;
                    }
                }
            }
            if land_weight > 0.0 {
                for c in &mut land {
                    *c /= land_weight;
                }
            }
            let sea = mix(halo, sea, (shore - HALO_CELLS).clamp(0.0, 1.0));
            let mut rgb = mix(land, sea, smoothstep(0.35, 0.65, wet));
            rgb = mix(rgb, road_colour, smoothstep(0.4, 0.6, road));
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            // LINT-OK: a colour channel, clamped to its range.
            let byte = |c: f32| c.round().clamp(0.0, 255.0) as u8;
            *px = [byte(rgb[2]), byte(rgb[1]), byte(rgb[0]), 255];
        }
    }

    /// The `w`×`h` window of the picture at pixel `(x0, y0)`.
    #[must_use]
    pub fn window(&self, (x0, y0): (usize, usize), (w, h): (usize, usize)) -> Pixels {
        let (w, h) = (w.min(SIDE - x0.min(SIDE)), h.min(SIDE - y0.min(SIDE)));
        let mut bgra = vec![0u8; w * h * 4];
        in_bands(&mut bgra, w * 4, 64, &|row, band| {
            for (r, out) in band.chunks_mut(w * 4).enumerate() {
                self.draw_row(y0 + row + r, x0, out);
            }
        });
        Pixels {
            width: u32::try_from(w).unwrap_or(0),
            height: u32::try_from(h).unwrap_or(0),
            bgra,
        }
    }

    /// Tile `(col, row)`: the picture's [`TILE`]-pixel square there, short at the east and south.
    #[must_use]
    pub fn tile(&self, (col, row): (usize, usize)) -> Pixels {
        self.window((col * TILE, row * TILE), (TILE, TILE))
    }

    /// Rows `rows` of the whole picture shrunk to [`OVERVIEW`], each drawn from the picture's own
    /// rows it covers, into `out`.
    fn overview_rows(&self, spans: &[Vec<(usize, f64)>], first: usize, out: &mut [u8]) {
        let mut src = vec![0u8; SIDE * 4];
        let mut acc = vec![0.0f64; OVERVIEW * 3];
        for (o, row) in out.chunks_mut(OVERVIEW * 4).enumerate() {
            acc.fill(0.0);
            for &(s, cover) in &spans[first + o] {
                self.draw_row(s, 0, &mut src);
                for (x, span) in spans.iter().enumerate() {
                    for c in 0..3 {
                        let mut sum = 0.0f64;
                        for &(si, part) in span {
                            sum += f64::from(src[si * 4 + 2 - c]) * part;
                        }
                        #[allow(clippy::cast_possible_truncation)]
                        // LINT-OK: kept to a single's precision, as the two-pass shrink keeps it.
                        let single = sum as f32;
                        acc[x * 3 + c] += f64::from(single) * cover;
                    }
                }
            }
            for (px, a) in row
                .as_chunks_mut::<4>()
                .0
                .iter_mut()
                .zip(acc.as_chunks::<3>().0)
            {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                // LINT-OK: a colour channel, clamped to its range.
                let byte = |c: f64| c.round().clamp(0.0, 255.0) as u8;
                *px = [byte(a[2]), byte(a[1]), byte(a[0]), 255];
            }
        }
    }

    /// The whole picture shrunk to [`OVERVIEW`] a side.
    #[must_use]
    pub fn overview(&self) -> Pixels {
        let spans = spans(SIDE, OVERVIEW);
        let mut bgra = vec![0u8; OVERVIEW * OVERVIEW * 4];
        in_bands(&mut bgra, OVERVIEW * 4, 16, &|row, band| {
            self.overview_rows(&spans, row, band);
        });
        overview_pixels(bgra)
    }
}

fn overview_pixels(bgra: Vec<u8>) -> Pixels {
    Pixels {
        width: u32::try_from(OVERVIEW).unwrap_or(0),
        height: u32::try_from(OVERVIEW).unwrap_or(0),
        bgra,
    }
}

/// A world map built: what tiles are drawn from, and the overview when it was asked for.
#[derive(Debug)]
pub struct Built {
    pub map: Arc<WorldMap>,
    pub overview: Option<Pixels>,
}

/// The work of a [`MapBuild`], a step at a time.
struct Steps {
    store: Arc<RetailDatStore>,
    overview: bool,
    stage: Stage,
}

enum Stage {
    Start,
    /// The landblocks read up to a column.
    Reading(MapGrid, i32),
    Placeholders(MapGrid),
    Shore(MapGrid),
    Shading(MapGrid, Vec<f32>),
    /// The overview drawn up to a row.
    Overview(Arc<WorldMap>, Vec<Vec<(usize, f64)>>, Vec<u8>, usize),
    Done,
}

impl Steps {
    /// One step: a column of landblocks read, a stage of the preparing, or a band of the overview.
    /// `Some` once the build is done.
    fn step(&mut self) -> Option<Result<Built, String>> {
        let stage = std::mem::replace(&mut self.stage, Stage::Done);
        self.stage = match stage {
            Stage::Start => match MapGrid::start_read(&self.store) {
                Ok(grid) => Stage::Reading(grid, 0),
                Err(e) => return Some(Err(e)),
            },
            Stage::Reading(mut grid, bx) if bx < BLOCKS => {
                grid.read_column(&self.store, bx);
                Stage::Reading(grid, bx + 1)
            }
            Stage::Reading(grid, _) => match grid.finish_read() {
                Ok(grid) => Stage::Placeholders(grid),
                Err(e) => return Some(Err(e)),
            },
            Stage::Placeholders(mut grid) => {
                grid.drop_placeholder_land();
                Stage::Shore(grid)
            }
            Stage::Shore(grid) => {
                let distance = grid.shore_distance(&grid.water_kinds());
                Stage::Shading(grid, distance)
            }
            Stage::Shading(grid, distance) => {
                let shade = grid.shading();
                let water = grid.water_kinds();
                let map = Arc::new(WorldMap::new(grid, water, distance, shade));
                if !self.overview {
                    return Some(Ok(Built {
                        map,
                        overview: None,
                    }));
                }
                let spans = spans(SIDE, OVERVIEW);
                Stage::Overview(map, spans, vec![0; OVERVIEW * OVERVIEW * 4], 0)
            }
            Stage::Overview(map, spans, mut bgra, row) if row < OVERVIEW => {
                let band = 16;
                let at = row * OVERVIEW * 4;
                let end = ((row + band) * OVERVIEW * 4).min(bgra.len());
                map.overview_rows(&spans, row, &mut bgra[at..end]);
                Stage::Overview(map, spans, bgra, row + band)
            }
            Stage::Overview(map, _, bgra, _) => {
                return Some(Ok(Built {
                    map,
                    overview: Some(overview_pixels(bgra)),
                }));
            }
            Stage::Done => return Some(Err("the map was built already".to_owned())),
        };
        None
    }

    /// Every step, the overview's bands shared among the machine's threads.
    #[cfg(not(target_arch = "wasm32"))]
    fn run(mut self) -> Result<Built, String> {
        loop {
            if let Stage::Overview(map, ..) = &self.stage {
                let map = Arc::clone(map);
                let overview = map.overview();
                return Ok(Built {
                    map,
                    overview: Some(overview),
                });
            }
            if let Some(done) = self.step() {
                return done;
            }
        }
    }
}

/// A world map read, prepared and drawn off the frame.
pub struct MapBuild {
    #[cfg(not(target_arch = "wasm32"))]
    done: std::sync::mpsc::Receiver<Result<Built, String>>,
    #[cfg(target_arch = "wasm32")]
    steps: Steps,
}

impl std::fmt::Debug for MapBuild {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MapBuild")
    }
}

/// The share of a frame a build in the frame's own thread takes.
#[cfg(target_arch = "wasm32")]
const FRAME_SHARE: std::time::Duration = std::time::Duration::from_millis(6);

impl MapBuild {
    /// Build the map from `store`'s landblocks, with its overview when `overview` is set.
    #[must_use]
    pub fn start(store: Arc<RetailDatStore>, overview: bool) -> Self {
        let steps = Steps {
            store,
            overview,
            stage: Stage::Start,
        };
        #[cfg(not(target_arch = "wasm32"))]
        {
            let (tx, done) = std::sync::mpsc::channel();
            let spawned = std::thread::Builder::new()
                .name("world map".to_owned())
                .spawn(move || {
                    let _ = tx.send(steps.run());
                });
            if let Err(e) = spawned {
                tracing::warn!("the world map's thread did not start: {e}");
            }
            Self { done }
        }
        #[cfg(target_arch = "wasm32")]
        {
            Self { steps }
        }
    }

    /// The map, once it is built. Where there are no threads this is where a share of the frame's
    /// time builds it.
    pub fn poll(&mut self) -> Option<Result<Built, String>> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            match self.done.try_recv() {
                Ok(done) => Some(done),
                Err(std::sync::mpsc::TryRecvError::Empty) => None,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    Some(Err("the world map's thread stopped".to_owned()))
                }
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            let until = web_time::Instant::now() + FRAME_SHARE;
            loop {
                if let Some(done) = self.steps.step() {
                    return Some(done);
                }
                if web_time::Instant::now() >= until {
                    return None;
                }
            }
        }
    }

    /// The map, waited for.
    pub fn wait(self) -> Result<Built, String> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.done
                .recv()
                .unwrap_or_else(|_| Err("the world map's thread stopped".to_owned()))
        }
        #[cfg(target_arch = "wasm32")]
        {
            let mut steps = self.steps;
            loop {
                if let Some(done) = steps.step() {
                    return done;
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "world_map_tests.rs"]
mod tests;
