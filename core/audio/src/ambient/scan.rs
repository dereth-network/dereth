//! The terrain scan: the landscape asks each landblock in the neighbourhood for its ambient
//! sounds, and each landblock resolves a terrain cell to a sound descriptor through the region.
//!
//! The scan preserves the terrain-only input described below.
//!
//! **The terrain word is the only input.** No day/night term, no weather, no season, no indoor
//! variant. A separate day-group calculation feeds the *sky*, not the ambience.
//!
//! This crate does not own the landscape, so the caller hands over the cells it already walked. The
//! input is the **3x3 landblock neighbourhood** around the viewer (the blocks whose LOD is 1, i.e.
//! `max(|bx - mid|, |by - mid|) < 2`), 8x8 terrain cells each, each cell contributing its SW corner
//! vertex as the sound position and its terrain word as the type and scene.

use dereth_assets::region::{Region, SoundDesc};
use dereth_primitives::Vec3;

/// The number of landblocks on a side of the scanned neighbourhood.
///
/// The landscape scan walks the whole `mid_width x mid_width` window but only calls into
/// blocks whose LOD is 1, which is `max(|bx - mid_radius|, |by - mid_radius|) < 2` — a 3x3 block
/// square, 576 m on a side. The 120 m ambient radius fits comfortably inside it.
pub const NEIGHBOURHOOD_BLOCKS: usize = 3;
/// `side_cell_count` — a landblock is 8x8 terrain cells.
pub const CELLS_PER_BLOCK_SIDE: usize = 8;

/// One terrain cell handed to the scan.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TerrainCell {
    /// The cell's SW corner vertex, in world space — `vertex_array.vertices[9 * row + col]`.
    pub pos: Vec3,
    /// `terrain[row * 9 + col]`, the 16-bit terrain word.
    pub terrain_word: u16,
}

impl TerrainCell {
    /// `(w >> 2) & 0x1F` — the terrain type, 0..31.
    #[must_use]
    pub fn terrain_type(self) -> u32 {
        (u32::from(self.terrain_word) >> 2) & 0x1F
    }

    /// `w >> 11` — the scene index within that terrain type.
    #[must_use]
    pub fn scene_index(self) -> u32 {
        u32::from(self.terrain_word) >> 11
    }
}

/// What the caller hands in when the player's cell or position changed.
///
/// `outdoors` is the position-change gate: the scan runs only when the current cell is
/// outdoors (`(objcell_id & 0xFFFF) < 0x100`) or has `seen_outside != 0`. Inside a dungeon the
/// landscape contributes nothing, so `total_sound_count` stays 0, every periodic ambient sound fades to
/// silence and every intermittent sound falls off the queue — **dungeons are ambient-silent**.
#[derive(Debug, Clone, Default)]
pub struct TerrainNeighbourhood {
    pub outdoors: bool,
    pub cells: Vec<TerrainCell>,
}

/// Resolve a terrain type and scene index to an ambient sound descriptor.
///
/// A terrain type refers to scene types, and each scene type refers to an ambient sound
/// description. The runtime model resolves those references to objects during unpacking.
/// The scene-type index and the sound-description index follow the same rule:
/// **-1 means no object**, so an out-of-range or negative index
/// contributes nothing. The decoders keep the indices, so resolution happens here.
///
/// The lazy load and its sticky "not found" latch are the
/// caller's business: this returns the descriptor, and [`crate::AudioAssets::sound_table`] does the
/// load.
#[must_use]
pub fn get_stb_desc(region: &Region, terrain_type: u32, scene_index: u32) -> Option<&SoundDesc> {
    let tt = region
        .terrain_types
        .get(usize::try_from(terrain_type).ok()?)?;
    // This bound is the one the landblock scan tests.
    let scene_ref = *tt.scene_types.get(usize::try_from(scene_index).ok()?)?;
    let scene = region
        .scene_info
        .as_ref()?
        .get(usize::try_from(scene_ref).ok()?)?;
    region
        .sound_info
        .as_ref()?
        .get(usize::try_from(scene.stb_index).ok()?)
}

/// The index of that descriptor within `Region::sound_info`, which is how the ambient runtime keys
/// its long-lived per-descriptor state.
#[must_use]
pub fn stb_desc_index(region: &Region, terrain_type: u32, scene_index: u32) -> Option<usize> {
    let tt = region
        .terrain_types
        .get(usize::try_from(terrain_type).ok()?)?;
    let scene_ref = *tt.scene_types.get(usize::try_from(scene_index).ok()?)?;
    let scene = region
        .scene_info
        .as_ref()?
        .get(usize::try_from(scene_ref).ok()?)?;
    let i = usize::try_from(scene.stb_index).ok()?;
    if i < region.sound_info.as_ref()?.len() {
        Some(i)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the recovered scan order — `terrainType = (w >> 2) & 0x1F`,
    /// `sceneIndex = w >> 11`.
    #[test]
    fn the_terrain_word_splits_into_type_and_scene_as_documented() {
        // bits 2..6 are the type, bits 11..15 the scene.
        let c = TerrainCell {
            pos: Vec3::ZERO,
            terrain_word: 0b0001_1000_0100_1100,
        };
        assert_eq!(c.terrain_type(), 0b1_0011);
        assert_eq!(c.scene_index(), 0b0_0011);
        let c = TerrainCell {
            pos: Vec3::ZERO,
            terrain_word: 0xFFFF,
        };
        assert_eq!(c.terrain_type(), 0x1F, "five bits, so never above 31");
        assert_eq!(c.scene_index(), 31, "five bits above bit 11");
        let c = TerrainCell {
            pos: Vec3::ZERO,
            terrain_word: 0x0003,
        };
        assert_eq!(
            c.terrain_type(),
            0,
            "the low two bits are not part of the type"
        );
        assert_eq!(c.scene_index(), 0);
    }

    /// The scanned window is 3x3 landblocks of 8x8 cells: 576 cells, comfortably enclosing the 120 m
    /// ambient radius. Oracle: section 4.4's LOD-1 note.
    #[test]
    fn the_neighbourhood_is_three_by_three_blocks_of_eight_by_eight_cells() {
        assert_eq!(NEIGHBOURHOOD_BLOCKS, 3);
        assert_eq!(CELLS_PER_BLOCK_SIDE, 8);
        let cells = NEIGHBOURHOOD_BLOCKS
            * NEIGHBOURHOOD_BLOCKS
            * CELLS_PER_BLOCK_SIDE
            * CELLS_PER_BLOCK_SIDE;
        assert_eq!(cells, 576);
        // Three 192 m landblocks a side is 576 m, and the ambient radius is 120 m.
        assert!(
            f32::from(u16::try_from(NEIGHBOURHOOD_BLOCKS).expect("small")) * 192.0 / 2.0
                > super::super::weight::AMBIENT_SOUND_MAX_DIST
        );
    }
}
