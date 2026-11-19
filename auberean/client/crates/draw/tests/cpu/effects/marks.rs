//! Marks an interface lays on the ground (a selection's ring): the box they are laid in, their
//! picture coordinates and color, and the layer they are drawn with.
//!
//! Behaviour: none (this client's own mark on the ground, not a behaviour of the retail client)

use auberean_draw::decal::{
    landscape_decal, mark_batch, mark_desc, mark_layer, Corner, MARK_MIN_RADIUS, OWN_PICTURES,
};
use auberean_draw::material::d3d;
use auberean_primitives::Vec3;

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-4
}

/// Flat ground of eight-meter squares at height `z`.
fn flat(at: Vec3) -> Option<[Corner; 6]> {
    let (x, y) = ((at.x / 8.0).floor() * 8.0, (at.y / 8.0).floor() * 8.0);
    let c = |dx: f32, dy: f32| (Vec3::new(x + dx, y + dy, 2.0), Vec3::new(0.0, 0.0, 1.0));
    Some([
        c(0.0, 0.0),
        c(8.0, 0.0),
        c(8.0, 8.0),
        c(0.0, 0.0),
        c(8.0, 8.0),
        c(0.0, 8.0),
    ])
}

#[test]
fn a_mark_covers_its_radius_about_its_object_and_no_less_than_half_a_meter() {
    let at = Vec3::new(20.0, 20.0, 2.0);
    let tris = landscape_decal(at, &mark_desc(1.5, 0.0), 8.0, &flat);
    let xs: Vec<f32> = tris.iter().flatten().map(|v| v.position.x).collect();
    let ys: Vec<f32> = tris.iter().flatten().map(|v| v.position.y).collect();
    let span = |v: &[f32]| {
        v.iter().copied().fold(f32::MIN, f32::max) - v.iter().copied().fold(f32::MAX, f32::min)
    };
    assert!(near(span(&xs), 3.0) && near(span(&ys), 3.0), "{xs:?}");
    let small = mark_desc(0.1, 0.0);
    assert!(near(small.size.x, 2.0 * MARK_MIN_RADIUS));
    assert!(
        small.size.z >= 2.0,
        "high enough to find the ground under it"
    );
}

#[test]
fn a_mark_shows_its_picture_north_at_the_top_in_its_tint_and_turns_with_it() {
    let at = Vec3::new(20.0, 20.0, 2.0);
    let tris = landscape_decal(at, &mark_desc(2.0, 0.0), 8.0, &flat);
    let b = mark_batch(&tris, [0.0, 0.0, 1.0, 1.0], 0x80FF_0000, OWN_PICTURES).expect("a draw");
    assert!(b.vertices.iter().all(|v| v.diffuse == 0x80FF_0000));
    let north = b
        .vertices
        .iter()
        .max_by(|a, c| a.position[1].total_cmp(&c.position[1]))
        .unwrap();
    assert!(
        near(north.uv[0][1], 0.0),
        "the north edge at the top: {:?}",
        north.uv[0]
    );
    let east = b
        .vertices
        .iter()
        .max_by(|a, c| a.position[0].total_cmp(&c.position[0]))
        .unwrap();
    assert!(
        near(east.uv[0][0], 1.0),
        "the east edge at the right: {:?}",
        east.uv[0]
    );
    // A quarter turn puts the picture's right toward the north.
    let turned = landscape_decal(at, &mark_desc(2.0, std::f32::consts::FRAC_PI_2), 8.0, &flat);
    let b = mark_batch(&turned, [0.0, 0.0, 1.0, 1.0], 0xFFFF_FFFF, OWN_PICTURES).unwrap();
    let north = b
        .vertices
        .iter()
        .max_by(|a, c| a.position[1].total_cmp(&c.position[1]))
        .unwrap();
    assert!(near(north.uv[0][0], 1.0), "{:?}", north.uv[0]);
    assert_eq!(
        mark_batch(&[], [0.0, 0.0, 1.0, 1.0], 0, OWN_PICTURES).map(|_| ()),
        None
    );
}

#[test]
fn a_mark_is_blended_over_the_ground_hidden_behind_what_stands_before_it_and_marks_no_depth() {
    let l = mark_layer(OWN_PICTURES + 3);
    assert_eq!(l.stages[0].texture, OWN_PICTURES + 3);
    assert_eq!(
        (l.source_blend, l.dest_blend),
        (d3d::BLEND_SRC_ALPHA, d3d::BLEND_INV_SRC_ALPHA)
    );
    assert_eq!(l.depth_test, d3d::CMP_LESS_EQUAL);
    assert!(!l.depth_write);
}
