//! A top-down picture of the world: a pixel a world vertex, north up, each the color of its
//! terrain type, shaded by the slope as a light from the north-west lights it, with roads drawn
//! over. The world editor's map shows it, and redraws a landblock when an edit changes it.

use crate::brush::GRID;
use crate::dats::DatSet;
use dereth_assets::world::CellLandblock;
use dereth_assets::Region;

/// The picture's width and height, in pixels: room for every world vertex, 2,041 a side.
pub const SIDE: usize = 2048;

/// The color a road is drawn.
const ROAD: [u8; 3] = [150, 140, 120];

/// The picture.
#[derive(Clone, PartialEq, Eq)]
pub struct WorldMap {
    /// Red, green, blue and alpha, a row at a time from the north.
    pub rgba: Vec<u8>,
}

impl std::fmt::Debug for WorldMap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorldMap").finish_non_exhaustive()
    }
}

impl Default for WorldMap {
    fn default() -> Self {
        let mut rgba = vec![0u8; SIDE * SIDE * 4];
        for px in rgba.as_chunks_mut::<4>().0 {
            px.copy_from_slice(&[16, 16, 20, 255]);
        }
        Self { rgba }
    }
}

/// The map pixel (column, row) of world meters `(x, y)`.
#[must_use]
pub fn pixel_of(x: f32, y: f32) -> (f32, f32) {
    #[allow(clippy::cast_precision_loss)] // the grid's last point, 2,040
    let top = (crate::brush::GRID_MAX) as f32;
    (x / GRID, top - y / GRID)
}

/// The world meters of map pixel (column, row): [`pixel_of`] undone.
#[must_use]
pub fn world_of(px: f32, py: f32) -> (f32, f32) {
    #[allow(clippy::cast_precision_loss)] // the grid's last point, 2,040
    let top = (crate::brush::GRID_MAX) as f32;
    (px * GRID, (top - py) * GRID)
}

impl WorldMap {
    /// Every landblock of the files `set` holds, drawn with `colors` (a terrain type's color by
    /// its value) over `region`'s heights.
    #[must_use]
    pub fn read(set: &DatSet, region: &Region, colors: &[[u8; 3]]) -> Self {
        let mut map = Self::default();
        let heights = dereth_terrain::land::mesh::height_table(region);
        for id in set.cell.iter_ids() {
            if id.raw() & 0xFFFF != 0xFFFF {
                continue;
            }
            let Some(t) = set
                .cell
                .read(id)
                .ok()
                .and_then(|b| set.decode::<CellLandblock>(id, &b).ok())
            else {
                continue;
            };
            #[allow(clippy::cast_possible_truncation)] // LINT-OK: the high half is the landblock.
            map.draw_block((id.raw() >> 16) as u16, &t, &heights, colors);
        }
        map
    }

    /// Landblock `lb`'s pixels from its terrain `t`, with `heights` its height table and
    /// `colors` the terrain types' colors.
    pub fn draw_block(&mut self, lb: u16, t: &CellLandblock, heights: &[f32], colors: &[[u8; 3]]) {
        let (bx, by) = (usize::from(lb >> 8), usize::from(lb & 0xFF));
        let z = |x: usize, y: usize| {
            heights
                .get(usize::from(t.height[x * 9 + y]))
                .copied()
                .unwrap_or(0.0)
        };
        for x in 0..8 {
            for y in 0..8 {
                let c = if t.road(x, y) != 0 {
                    ROAD
                } else {
                    colors
                        .get(usize::from(t.terrain_type(x, y)))
                        .copied()
                        .unwrap_or([255, 0, 255])
                };
                // Lit from the north-west: brighter rising to the east and falling to the north.
                let slope = (z(x + 1, y) - z(x, y)) - (z(x, y + 1) - z(x, y));
                let shade = (1.0 + slope / GRID * 1.5).clamp(0.45, 1.6);
                let (gx, gy) = (bx * 8 + x, by * 8 + y);
                let row = crate::brush::GRID_MAX as usize - gy;
                let o = (row * SIDE + gx) * 4;
                if o + 4 > self.rgba.len() {
                    continue;
                }
                for (k, v) in c.iter().enumerate() {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // clamped
                    let lit = (f32::from(*v) * shade).clamp(0.0, 255.0) as u8;
                    self.rgba[o + k] = lit;
                }
                self.rgba[o + 3] = 255;
            }
        }
    }

    /// The top-left pixel (column, row) of landblock `lb`'s 8×8 pixels.
    #[must_use]
    pub fn block_corner(lb: u16) -> (usize, usize) {
        let (bx, by) = (usize::from(lb >> 8), usize::from(lb & 0xFF));
        (bx * 8, crate::brush::GRID_MAX as usize - (by * 8 + 7))
    }

    /// Landblock `lb`'s 8×8 pixels, red, green, blue and alpha, a row at a time from the north.
    #[must_use]
    pub fn block_pixels(&self, lb: u16) -> Vec<u8> {
        let (x0, y0) = Self::block_corner(lb);
        let mut out = Vec::with_capacity(8 * 8 * 4);
        for row in y0..y0 + 8 {
            let o = (row * SIDE + x0) * 4;
            out.extend_from_slice(&self.rgba[o..o + 8 * 4]);
        }
        out
    }

    /// The picture as a PNG file at `path`.
    ///
    /// # Errors
    /// The file cannot be written.
    pub fn save(&self, path: &std::path::Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let part = path.with_extension("part");
        {
            let f = std::fs::File::create(&part)?;
            #[allow(clippy::cast_possible_truncation)] // 2,048
            let mut enc = png::Encoder::new(std::io::BufWriter::new(f), SIDE as u32, SIDE as u32);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            enc.write_header()
                .and_then(|mut w| w.write_image_data(&self.rgba))
                .map_err(std::io::Error::other)?;
        }
        std::fs::rename(&part, path)
    }

    /// The picture [`Self::save`] wrote at `path`, when it is one.
    #[must_use]
    pub fn load(path: &std::path::Path) -> Option<Self> {
        let f = std::fs::File::open(path).ok()?;
        let mut r = png::Decoder::new(std::io::BufReader::new(f))
            .read_info()
            .ok()?;
        let mut rgba = vec![0u8; r.output_buffer_size()];
        let info = r.next_frame(&mut rgba).ok()?;
        (info.width as usize == SIDE
            && info.height as usize == SIDE
            && info.color_type == png::ColorType::Rgba)
            .then_some(Self { rgba })
    }
}

impl WorldMap {
    /// Landblocks `lbs` drawn again as `session`'s world holds them now, with `colors` the
    /// terrain types' colors.
    pub fn redraw(
        &mut self,
        session: &crate::edit::Session,
        lbs: impl IntoIterator<Item = u16>,
        colors: &[[u8; 3]],
    ) {
        let Some(region) = session.region() else {
            return;
        };
        let heights = dereth_terrain::land::mesh::height_table(region);
        for lb in lbs {
            if let Some(t) = session.terrain(lb) {
                self.draw_block(lb, &t, &heights, colors);
            }
        }
    }
}

/// The landblocks whose terrain record `records` (a compiled world's cell records) holds.
pub fn terrain_blocks<'a, V>(
    records: Option<&'a std::collections::BTreeMap<u32, V>>,
) -> impl Iterator<Item = u16> + 'a {
    records
        .into_iter()
        .flat_map(|r| r.keys())
        .filter(|id| *id & 0xFFFF == 0xFFFF)
        .filter_map(|id| u16::try_from(id >> 16).ok())
}
