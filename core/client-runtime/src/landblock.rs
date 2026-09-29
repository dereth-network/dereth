//! The landblock and region identities, re-exported from `dereth_world_data::landblock`, and the
//! one item defined here: `block_shift`, which is the renderer's viewer-block-relative space.

pub use dereth_world_data::landblock::*;

/// Absolute world coordinates -> the renderer's viewer-block-relative space.
///
/// Landscape rendering places the south-west corner of the **viewer's**
/// landblock at the origin, and moves that block as the player walks. It is [`block_xy`] and one
/// multiply.
#[must_use]
pub fn block_shift(
    viewer_block: Option<(i32, i32)>,
    cfg_landblock: u16,
) -> dereth_primitives::Vec3 {
    use dereth_landscape::BLOCK_LENGTH;
    let v = viewer_block.unwrap_or_else(|| block_xy(cfg_landblock));
    #[allow(clippy::cast_precision_loss)] // a block index, 0..=254
    dereth_primitives::Vec3::new(
        -(v.0 as f32) * BLOCK_LENGTH,
        -(v.1 as f32) * BLOCK_LENGTH,
        0.0,
    )
}
