//! A flat-terrain `LandSource` for tests: the shared crate's `StaticLandSource` with the linear
//! height table and flat blocks. Nothing here is ported from ACE.

use dereth_physics::source::StaticLandSource;
use dereth_primitives::LandblockId;

/// A land source whose listed blocks are flat at height-table index `height_index` (the linear
/// table puts index `i` at `2 * i` metres), with no buildings and no interior cells.
#[must_use]
pub fn flat_land_source(blocks: &[LandblockId], height_index: u8) -> StaticLandSource {
    let mut s = StaticLandSource::linear();
    for &b in blocks {
        s.add_flat_block(b, height_index);
    }
    s
}

/// The linear height table `StaticLandSource::linear` uses: index `i` at `2 * i` metres.
#[must_use]
pub fn linear_height_table() -> [f32; dereth_physics::globals::LAND_HEIGHT_TABLE_LEN] {
    let mut t = [0.0_f32; dereth_physics::globals::LAND_HEIGHT_TABLE_LEN];
    for (i, v) in t.iter_mut().enumerate() {
        *v = f32::from(u8::try_from(i).unwrap_or(u8::MAX)) * 2.0;
    }
    t
}
