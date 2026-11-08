//! How the ground's cells are drawn.

/// The ground's settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TerrainSettings {
    /// Each cell drawn from its picture's layers, blended at each pixel as retail blends them,
    /// rather than from a composite picture of its own: the tiles and alpha maps are shared by
    /// every cell, where the composites take most of the scene's memory. Off, the composites.
    pub splat: bool,
    /// The ground drawn as a grid of patches over retail's surface, finer near the eye and
    /// coarser away from it, rather than as retail's grid of each block at its level of detail.
    pub grid: bool,
    /// With the grid, the scene's visual offset laid over retail's surface, with what stands on
    /// the ground drawn on it: smooth walkable ground, rock bands on cliffs, a fine relief
    /// leveled under bodies. Collision and picking keep retail's ground.
    pub drawn: bool,
    /// With the grid, the ground drawn out to the world's edge, not just across the blocks held.
    pub horizon: bool,
}

impl TerrainSettings {
    /// What the player draws the ground with while the remaster is on: every piece of it.
    #[must_use]
    pub const fn player() -> Self {
        Self {
            splat: true,
            grid: true,
            drawn: true,
            horizon: true,
        }
    }
}

/// What the scene's changes have counted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ChangeCounts {
    /// Blocks taken in, built again whole, and built again with only their ground changed.
    pub blocks_in: u64,
    pub blocks_changed: u64,
    pub grounds_changed: u64,
}
