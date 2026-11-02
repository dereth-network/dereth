//! The landscape's ambient sound points: the places of a landblock that name a kind of ambient
//! sound, which the client's ambient sound weighs by their distance from the player to pick the
//! place's sounds.
//!
//! A vertex names a kind through the same chain that picks its scenery: its terrain kind's scenery
//! for its scene field (or the terrain kind's default) gives a scenery kind, and the scenery kind
//! carries a sound word. A vertex whose chain ends without a scenery kind, at a scenery kind the
//! scene description lacks, or at a sound word of 0, names none.
//!
//! The points stand at the vertices' heights but not at their places on the ground: their spacing
//! is the block's side over the number of vertices along it (not over the number of squares), so
//! the points fall short of the block's north and east edges, the last row by nearly a square.

use auberean_assets::landblock::Landblock;
use auberean_assets::region::{SceneDesc, TerrainDesc};
use auberean_primitives::Vec3;

use crate::land::{LandConstants, HEIGHT_TABLE_LEN};

/// A point of a landblock that names a kind of ambient sound.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SoundPoint {
    /// The kind, as the region's ambient sound table lists them.
    pub kind: u32,
    /// Where it is, in the block's frame.
    pub origin: Vec3,
}

/// The ambient sound kind a vertex names: its terrain kind (or the description's default terrain
/// when the kind is not listed) maps its scene field to a scenery kind (or to the terrain's default
/// scenery), and that scenery kind's sound word is the kind. `None` when there is no terrain, the
/// scenery kind is 0 or not in the scene description, or its sound word is 0.
// D2-07449, D2-07434
#[must_use]
pub fn vertex_sound(
    terrain: &TerrainDesc,
    scenes: &SceneDesc,
    terrain_kind: u32,
    scene_field: u32,
) -> Option<u32> {
    let kind = terrain
        .terrains
        .iter()
        .find(|t| t.index == terrain_kind)
        .or(terrain.default_terrain.as_ref())?;
    let scenery = kind
        .scenery
        .iter()
        .find(|s| s.index == scene_field)
        .or(kind.default_scenery.as_ref())
        .map_or(0, |s| s.kind);
    if scenery == 0 {
        return None;
    }
    let sound = scenes
        .scene_types
        .iter()
        .find(|s| s.index == scenery)?
        .sound;
    (sound != 0).then_some(sound)
}

/// A landblock's ambient sound points, in its vertices' order (x-major, y fastest): one for each
/// vertex that names a kind, at `(x, y)` times the block's side over the vertices along it, at the
/// height its height byte gives.
// D2-03670, D2-03682
#[must_use]
pub fn block_sound_points(
    land: &LandConstants,
    terrain: &TerrainDesc,
    scenes: &SceneDesc,
    block: &Landblock,
) -> Vec<SoundPoint> {
    let side = land.grid().vertices_per_side();
    #[allow(clippy::cast_precision_loss)] // seventeen vertices a side
    let step = land.grid().block_length() / side as f32;
    let mut out = Vec::new();
    for k in 0..side * side {
        let (x, y) = (k / side, k % side);
        let (Some(&word), Some(&height)) = (block.vertices.get(k), block.heights.get(k)) else {
            continue;
        };
        let Some(kind) = vertex_sound(
            terrain,
            scenes,
            land.word.terrain.of(word),
            land.word.scene.of(word),
        ) else {
            continue;
        };
        #[allow(clippy::cast_precision_loss)] // vertex numbers below seventeen
        let origin = Vec3::new(
            x as f32 * step,
            y as f32 * step,
            land.heights[usize::from(height).min(HEIGHT_TABLE_LEN - 1)],
        );
        out.push(SoundPoint { kind, origin });
    }
    out
}
