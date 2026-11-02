//! Vertex layouts, detail levels and mask turns.

use auberean_assets::setup::DetailDistances;
use auberean_draw::mesh::{detail_at, layout};
use auberean_draw::scene::mask_uv;

/// A vertex stores its position, normal, diffuse and specular colors, the three tangent-space
/// vectors and its coordinate pairs in that order: the commonest 2013 format, `0x300002D2`, is 72
/// bytes with its two coordinate pairs at byte 56; `0x2D2` is 48 bytes.
///
/// Behaviour: draw.mesh.a-vertex-format-lays-its-elements-out-in-order
#[test]
fn a_vertex_format_lays_its_elements_out_in_order() {
    let l = layout(0x3000_02D2);
    assert_eq!(l.stride, 72);
    assert_eq!(
        (l.position, l.normal, l.diffuse),
        (Some(0), Some(12), Some(24))
    );
    assert_eq!((l.uv, l.uv_sets), (56, 2));
    assert_eq!(
        l.tangents,
        [Some(32), Some(44), None],
        "the two tangent directions follow the specular color"
    );
    let l = layout(0x2D2);
    assert_eq!((l.stride, l.uv), (48, 32));
    assert_eq!(
        layout(0x0100_0012).stride,
        32,
        "padding rounds past the next 32 bytes"
    );
}

/// An object is drawn at full detail inside its near distance, at none past its far one, half way
/// at its ideal distance, and not at all beyond its visible distance (never less than 20).
///
/// Behaviour: draw.mesh.detail-falls-from-near-to-far
#[test]
fn detail_falls_from_near_to_far() {
    let d = DetailDistances {
        visible: 300.0,
        near_degrade: 10.0,
        ideal_degrade: 50.0,
        far_degrade: 150.0,
        high_detail: 0.0,
        medium_detail: 0.0,
    };
    assert_eq!(detail_at(&d, 5.0), Some(1.0));
    assert_eq!(detail_at(&d, 50.0), Some(0.5));
    assert_eq!(detail_at(&d, 100.0), Some(0.25));
    assert_eq!(detail_at(&d, 200.0), Some(0.0));
    assert_eq!(detail_at(&d, 301.0), None);
    let tiny = DetailDistances { visible: 5.0, ..d };
    assert!(
        detail_at(&tiny, 15.0).is_some(),
        "the visible distance is at least 20"
    );
}

/// A mask covers one square and turns a quarter clockwise per step: unturned its top edge is the
/// square's north edge; a quarter turn puts the mask's top on the east edge.
///
/// Behaviour: draw.terrain.a-mask-turns-a-quarter-per-step
#[test]
fn a_mask_turns_a_quarter_per_step() {
    // (corner offset) -> coordinates, for the four turns.
    assert_eq!(mask_uv(0.0, 0.0, 1), [0.0, 1.0]);
    assert_eq!(
        mask_uv(0.0, 1.0, 1),
        [0.0, 0.0],
        "north-west is the mask's top left"
    );
    assert_eq!(
        mask_uv(1.0, 1.0, 2),
        [0.0, 0.0],
        "a quarter turn brings the top left to the north-east"
    );
    assert_eq!(
        mask_uv(1.0, 0.0, 3),
        [0.0, 0.0],
        "a half turn brings it to the south-east"
    );
    assert_eq!(
        mask_uv(0.0, 0.0, 4),
        [0.0, 0.0],
        "three quarters bring it to the south-west"
    );
}
