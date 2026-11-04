//! How a placed object is drawn at a distance: its distances drawn in by the scenery draw
//! distance, its materials' detail lowered far off, and its fade at the edge of its visible
//! distance.

use std::sync::Arc;

use auberean_assets::setup::DetailDistances;
use auberean_draw::body::{adaptive_scale, HoverFade};
use auberean_draw::material::{d3d, DrawLayer, Faded};
use auberean_draw::mesh::{degrade, scenery_scale};
use auberean_draw::quality::FrameRate;
use auberean_draw::scene::{fade_objects, Cull, ObjectBatch, PointLights};
use auberean_primitives::Vec3;
use auberean_world::color::Color;
use auberean_world::math::Mat4;

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

fn layer() -> DrawLayer {
    DrawLayer {
        stages: Vec::new(),
        source_blend: d3d::BLEND_ONE,
        dest_blend: d3d::BLEND_ZERO,
        depth_test: 4,
        depth_write: true,
        cull: d3d::CULL_NONE,
        alpha_test: false,
        alpha_ref: 0.0,
        tests_alpha: false,
        switches: 0,
        ambient: Color::WHITE,
        diffuse: Color::WHITE,
        specular: Color::BLACK,
        specular_power: 0.0,
        pixel_program: None,
        light_variants: Vec::new(),
        motion: None,
    }
}

fn fade_batch(anchor: Vec3) -> ObjectBatch {
    let r = layer();
    ObjectBatch {
        vertices: Arc::new(Vec::new()),
        posed: None,
        indices: Arc::new(Vec::new()),
        transform: Mat4::IDENTITY,
        layer: r,
        tint: Color::WHITE,
        points: PointLights::NONE,
        mesh_type: 0,
        shade: None,
        outside: false,
        cull: Cull::Never,
        mesh: 0,
        casts_volume: false,
        caster: None,
        seen_through: false,
        kept_by: None,
        anchor: Some(anchor),
        step: None,
        visible: None,
        block: None,
        far: false,
        ground: None,
        lod: None,
    }
}

/// Every frame an object's opacity is worked out from where the camera now stands, and the
/// objects on a landblock fading in fade with it, both at once; an object with no visible distance
/// off no such block stays whole. Past 40 m it counts as far off.
///
/// Behaviour: draw.mesh.an-objects-opacity-follows-the-camera-and-its-landblock
#[test]
fn an_objects_opacity_follows_the_camera_and_its_landblock() {
    let at = Vec3::new(0.0, 288.75, 0.0);
    let mut b = fade_batch(at);
    b.visible = Some(300.0);
    let mut objects = vec![
        b.clone(),
        ObjectBatch {
            posed: None,
            block: Some((0xC1, 0x56)),
            ..b.clone()
        },
        ObjectBatch {
            posed: None,
            visible: None,
            anchor: Some(Vec3::new(0.0, 39.0, 0.0)),
            ..b
        },
    ];
    fade_objects(&mut objects, Vec3::ZERO, &[((0xC1, 0x56), 0.5)]);
    assert!((objects[0].tint.a - 0.5).abs() < 1e-4, "the edge's half");
    assert!((objects[1].tint.a - 0.25).abs() < 1e-4, "and its block's");
    assert_eq!(objects[2].tint.a, 1.0);
    assert!(objects[0].far && !objects[2].far);
    // nearer, the same frame's objects are whole again; past its reach the object is not drawn
    fade_objects(&mut objects, Vec3::new(0.0, 100.0, 0.0), &[]);
    assert_eq!(objects[0].tint.a, 1.0);
    assert!(objects[0].drawn());
    fade_objects(&mut objects, Vec3::new(0.0, -20.0, 0.0), &[]);
    assert!(!objects[0].drawn());
    assert!(objects[2].drawn());
}

/// The water's reflection shows an object out to 40% of its visible distance; one with no visible
/// distance always.
///
/// Behaviour: draw.mesh.the-waters-reflection-reaches-40-percent-of-an-objects-distance
#[test]
fn the_waters_reflection_reaches_40_percent_of_an_objects_distance() {
    let mut b = fade_batch(Vec3::new(0.0, 100.0, 0.0));
    assert!(b.reflected(Vec3::ZERO));
    b.visible = Some(300.0);
    assert!(b.reflected(Vec3::ZERO));
    b.visible = Some(200.0);
    assert!(!b.reflected(Vec3::ZERO));
}

/// A draw not wholly opaque, of a layer that tests or blends by alpha, stops marking depth and
/// passes every texel with some alpha: a layer only blending once under 0.8, one that tests alpha
/// only past 40 m. A layer doing neither is untouched.
///
/// Behaviour: draw.mesh.a-faded-draw-stops-marking-depth-and-testing-its-alpha
#[test]
fn a_faded_draw_stops_marking_depth_and_testing_its_alpha() {
    let shown = Faded {
        depth_write: true,
        alpha_ref: Some(0.5),
    };
    let through = Faded {
        depth_write: false,
        alpha_ref: Some(0.0),
    };
    let mut tested = layer();
    tested.alpha_test = true;
    tested.tests_alpha = true;
    tested.alpha_ref = 0.5;
    assert_eq!(tested.faded(1.0, true), shown, "whole");
    assert_eq!(tested.faded(0.5, false), shown, "near");
    assert_eq!(tested.faded(0.5, true), through, "far");
    let mut blended = layer();
    blended.alpha_test = true;
    blended.source_blend = d3d::BLEND_SRC_ALPHA;
    let held = Faded {
        depth_write: true,
        alpha_ref: Some(0.0),
    };
    assert_eq!(blended.faded(0.85, false), held);
    assert_eq!(blended.faded(0.79, false), through);
    assert_eq!(blended.faded(0.85, true), through);
    let opaque = layer();
    assert_eq!(
        opaque.faded(0.1, true),
        Faded {
            depth_write: true,
            alpha_ref: None,
        }
    );
}

/// The adaptive degrade leaves a body's distances whole at 41 frames a second and up, and draws
/// them in to a tenth of themselves as the frames stop; its bias moves the rate it aims at.
///
/// Behaviour: draw.mesh.the-adaptive-degrade-draws-moving-bodies-in-as-frames-slow
#[test]
fn the_adaptive_degrade_draws_moving_bodies_in_as_frames_slow() {
    assert!((adaptive_scale(120.0, 0.0) - 1.0).abs() < 1e-4);
    assert!((adaptive_scale(41.0 / 1.05, 0.0) - 1.0).abs() < 1e-4);
    assert!((adaptive_scale(0.0, 0.0) - 0.1).abs() < 1e-4);
    let half = adaptive_scale(20.5 / 1.05, 0.0);
    assert!((half - 0.55).abs() < 1e-4, "{half}");
    assert!(adaptive_scale(40.0, 0.5) < adaptive_scale(40.0, 0.0));
    assert!((adaptive_scale(1.0 / 1.05, -1.0) - 1.0).abs() < 1e-4);
    assert!(
        (adaptive_scale(1.0 / 1.05, -9.0) - 1.0).abs() < 1e-4,
        "held"
    );
}

/// The frame rate is the mean of the last forty frames' times.
///
/// Behaviour: draw.mesh.the-frame-rate-is-the-mean-of-the-last-forty-frames
#[test]
fn the_frame_rate_is_the_mean_of_the_last_forty_frames() {
    let mut rate = FrameRate::default();
    assert_eq!(rate.frames_per_second(), 0.0);
    let mut now = 0.0;
    rate.end_frame(now);
    assert_eq!(rate.frames_per_second(), 0.0, "no time spent yet");
    for _ in 0..40 {
        now += 0.05;
        rate.end_frame(now);
    }
    assert!((rate.frames_per_second() - 20.0).abs() < 1e-2);
    for _ in 0..20 {
        now += 0.025;
        rate.end_frame(now);
    }
    let mixed = 40.0 / (20.0 * 0.05 + 20.0 * 0.025);
    assert!((rate.frames_per_second() - mixed).abs() < 1e-2);
    for _ in 0..40 {
        now += 0.025;
        rate.end_frame(now);
    }
    assert!(
        (rate.frames_per_second() - 40.0).abs() < 1e-2,
        "the old gone"
    );
}

/// The player's body under the pointer fades at ten times its opacity a second, down to 0.3 at
/// the least, and comes back to whole the same way once the pointer leaves.
///
/// Behaviour: draw.mesh.the-players-body-turns-see-through-under-the-pointer
#[test]
fn the_players_body_turns_see_through_under_the_pointer() {
    let mut fade = HoverFade::default();
    fade.step(10.0, false);
    assert_eq!(fade.opacity, 1.0, "away from it: whole");
    fade.step(10.01, true);
    assert!((fade.opacity - 0.9).abs() < 1e-3, "{}", fade.opacity);
    fade.step(10.02, true);
    assert!((fade.opacity - 0.81).abs() < 1e-3, "{}", fade.opacity);
    fade.step(12.0, true);
    assert_eq!(fade.opacity, 0.3, "no less");
    fade.step(12.1, false);
    assert!((fade.opacity - 0.6).abs() < 1e-3, "{}", fade.opacity);
    fade.step(13.0, false);
    assert_eq!(fade.opacity, 1.0);
}

/// The water's reflection fades an object at the edge of its own reach, over the last 7.5% of 40%
/// of its visible distance, whatever the frame shows of it there; an object with no visible distance
/// keeps its opacity. What the frame has faded for its block stays faded.
///
/// Behaviour: draw.mesh.the-waters-reflection-fades-an-object-at-its-own-edge
#[test]
fn the_waters_reflection_fades_an_object_at_its_own_edge() {
    let mut b = fade_batch(Vec3::new(0.0, 100.0, 0.0));
    assert!((b.reflected_alpha(Vec3::ZERO) - 1.0).abs() < 1e-6);
    // the reach there 120 m, its fade from 111 m
    b.visible = Some(300.0);
    assert!((b.reflected_alpha(Vec3::ZERO) - 1.0).abs() < 1e-6);
    b.anchor = Some(Vec3::new(0.0, 115.5, 0.0));
    assert!((b.reflected_alpha(Vec3::ZERO) - 0.5).abs() < 1e-4);
    b.tint.a = 0.5;
    assert!((b.reflected_alpha(Vec3::ZERO) - 0.25).abs() < 1e-4);
}
