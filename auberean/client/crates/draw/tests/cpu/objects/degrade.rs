//! How a placed object is drawn at a distance: its distances drawn in by the scenery draw
//! distance, its materials' detail lowered far off, and its fade at the edge of its visible
//! distance.

use auberean_assets::setup::DetailDistances;
use auberean_draw::mesh::{degrade, scenery_scale};

/// Distances a large building's setup gives at the highest model detail.
const BUILDING: DetailDistances = DetailDistances {
    visible: 300.0,
    near_degrade: 40.0,
    ideal_degrade: 90.0,
    far_degrade: 120.0,
    high_detail: 20.0,
    medium_detail: 40.0,
};

/// At the scenery draw distance's low and medium settings a placed object's distances are drawn
/// in to 0.4 and 0.7 of themselves: a building seen to 300 m at high is gone past 120 m at low
/// and past 210 m at medium, and halfway down its detail at 36 m and 63 m (90 m at high); the
/// visible distance is never under 20 m.
///
/// Behaviour: draw.mesh.the-scenery-draw-distance-draws-placed-objects-in
#[test]
fn the_scenery_draw_distance_draws_placed_objects_in() {
    assert_eq!([0, 1, 2, 7].map(scenery_scale), [0.4, 0.7, 1.0, 1.0]);
    let seen = |scale: f32, d: f32| degrade(&BUILDING, d, scale, 2).is_some();
    assert!(seen(1.0, 250.0) && !seen(0.7, 250.0) && !seen(0.4, 250.0));
    assert!(seen(0.7, 190.0) && !seen(0.7, 211.0));
    assert!(seen(0.4, 100.0) && !seen(0.4, 121.0));
    let half = |scale: f32| degrade(&BUILDING, 90.0 * scale, scale, 2).map(|g| g.detail);
    assert_eq!([half(0.4), half(0.7), half(1.0)], [Some(0.5); 3]);
    let small = DetailDistances {
        visible: 30.0,
        ..BUILDING
    };
    assert!(
        degrade(&small, 19.0, 0.4, 2).is_some(),
        "12 m drawn out to 20"
    );
}

/// An object far off draws its materials a step lower than the material detail asks past three
/// times its high-detail distance, and at the lowest past three times its medium-detail one;
/// a lower material detail is never raised.
///
/// Behaviour: draw.mesh.a-far-object-draws-its-materials-at-a-lower-detail
#[test]
fn a_far_object_draws_its_materials_at_a_lower_detail() {
    let materials = |d: f32, detail: u32, scale: f32| {
        degrade(&BUILDING, d, scale, detail).map(|g| g.material_detail)
    };
    assert_eq!(materials(59.0, 2, 1.0), Some(2));
    assert_eq!(materials(61.0, 2, 1.0), Some(1), "past 60 m");
    assert_eq!(materials(119.0, 2, 1.0), Some(1));
    assert_eq!(materials(121.0, 2, 1.0), Some(0), "past 120 m");
    assert_eq!(materials(10.0, 1, 1.0), Some(1), "medium stays medium");
    assert_eq!(materials(10.0, 0, 1.0), Some(0), "low stays low");
    assert_eq!(materials(150.0, 0, 1.0), Some(0));
    assert_eq!(
        materials(43.0, 2, 0.7),
        Some(1),
        "the distances drawn in: past 42 m at medium scenery"
    );
}

/// An object fades out over the last 7.5% of its visible distance: fully opaque before it,
/// half way through it at half, and not drawn once its opacity reaches nothing.
///
/// Behaviour: draw.mesh.an-object-fades-out-at-the-edge-of-its-visible-distance
#[test]
fn an_object_fades_out_at_the_edge_of_its_visible_distance() {
    let opacity = |d: f32| degrade(&BUILDING, d, 1.0, 2).map(|g| g.opacity);
    assert_eq!(opacity(277.0), Some(1.0));
    assert_eq!(opacity(277.5), Some(1.0), "the fade starts at 277.5 m");
    let half = opacity(288.75).expect("drawn");
    assert!((half - 0.5).abs() < 1e-4, "{half}");
    assert_eq!(opacity(300.0), None, "nothing left at the visible distance");
}
