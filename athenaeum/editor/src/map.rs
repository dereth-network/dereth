//! The overview map: every landblock of the world at a glance, each [`PER_BLOCK`] pixels a side,
//! north up, colored by the kind of ground it has and darker where it lies low.

use crate::scene::WorldData;
use crate::view::BLOCK;

/// Landblocks a side.
pub const BLOCKS: usize = 256;
/// Pixels a landblock's side.
pub const PER_BLOCK: usize = 2;
/// The map's side, in pixels.
pub const SIDE: usize = BLOCKS * PER_BLOCK;

/// How many of a block's vertices a side, and where the samples lie among its 17.
const VERTICES: usize = 17;
const SAMPLES: [usize; PER_BLOCK] = [4, 12];

/// The height the shading reaches its brightest at, in meters.
const HIGHEST: f32 = 72.0;

/// The color of ground of this terrain kind, at this height, on a road or not.
#[must_use]
pub fn color_of(kind: &str, height: f32, road: bool) -> [u8; 3] {
    if road {
        return [190, 165, 125];
    }
    let name = kind.to_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| name.contains(w));
    let base: [u8; 3] = if has(&["town"]) {
        [225, 140, 70]
    } else if has(&["water", "ocean", "river", "sea", "barrenbed"]) {
        [55, 95, 150]
    } else if has(&["shore", "beach", "sand"]) {
        [215, 200, 145]
    } else if has(&["snow", "ice"]) {
        [235, 240, 245]
    } else if has(&["rock", "granite", "obsidian", "volcano", "burnt", "magma"]) {
        [135, 130, 125]
    } else if has(&["desert", "dirt", "mud"]) {
        [165, 135, 95]
    } else if has(&["swamp", "marsh", "slime"]) {
        [95, 110, 70]
    } else {
        [95, 140, 75]
    };
    let light = 0.65 + 0.35 * (height / HIGHEST).clamp(0.0, 1.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    base.map(|c| (f32::from(c) * light) as u8)
}

/// The whole map, red, green, blue, opaque, row by row from the north. A block the world lacks
/// is black.
#[must_use]
pub fn picture(data: &WorldData) -> Vec<u8> {
    let mut out = vec![0u8; SIDE * SIDE * 4];
    for a in out.iter_mut().skip(3).step_by(4) {
        *a = u8::MAX;
    }
    let kinds = &data.region.terrain.terrains;
    let name_of = |index: u32| {
        kinds
            .iter()
            .find(|k| k.index == index)
            .map_or("", |k| k.name.as_str())
    };
    let word = &data.region.land.word;
    for (x, y) in data.records.landblock_coordinates() {
        let (Ok(bx), Ok(by)) = (usize::try_from(x), usize::try_from(y)) else {
            continue;
        };
        let Some(b) = data.records.landblock(x, y) else {
            continue;
        };
        for (px, i) in SAMPLES.iter().enumerate() {
            for (py, j) in SAMPLES.iter().enumerate() {
                let at = VERTICES * i + j;
                let (Some(v), Some(h)) = (b.vertices.get(at), b.heights.get(at)) else {
                    continue;
                };
                let height = data.region.land.heights[usize::from(*h)];
                let c = color_of(name_of(word.terrain.of(*v)), height, word.road.of(*v) != 0);
                let row = (BLOCKS - 1 - by) * PER_BLOCK + (PER_BLOCK - 1 - py);
                let column = bx * PER_BLOCK + px;
                let start = (row * SIDE + column) * 4;
                if let Some(p) = out.get_mut(start..start + 3) {
                    p.copy_from_slice(&c);
                }
            }
        }
    }
    out
}

/// Where on the map a point of the world's grid lies, in map pixels from its north-west corner.
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn pixel_of(x: f32, y: f32) -> [f32; 2] {
    let per = PER_BLOCK as f32 / BLOCK;
    [x * per, SIDE as f32 - y * per]
}

/// The landblock under a map pixel, and the place in it; none off the map.
#[must_use]
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
pub fn place_at(pixel: [f32; 2]) -> Option<(u16, [f32; 2])> {
    let side = SIDE as f32;
    if !(0.0..side).contains(&pixel[0]) || !(0.0..side).contains(&pixel[1]) {
        return None;
    }
    let per = BLOCK / PER_BLOCK as f32;
    let (x, y) = (pixel[0] * per, (side - pixel[1]) * per);
    let (bx, by) = (
        (x / BLOCK).floor(),
        (y / BLOCK).floor().min(BLOCKS as f32 - 1.0),
    );
    let lb = u16::from_be_bytes([bx as u8, by as u8]);
    Some((lb, [x - bx * BLOCK, y - by * BLOCK]))
}
