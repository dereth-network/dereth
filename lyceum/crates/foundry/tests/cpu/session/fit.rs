//! Behaviour: none (tooling: fit measurements on shapes with known dimensions)

use std::sync::Arc;

use dereth_physics::SetupGeometry;
use dereth_primitives::shape::{CylSphere, Sphere};
use dereth_primitives::Vec3;
use lyceum_foundry::fit::{self, Model, Shape};

fn cube(radius: f32) -> Vec<[Vec3; 3]> {
    let points: Vec<_> = (0..8)
        .map(|i| {
            let v = |b| if i & (1 << b) == 0 { -radius } else { radius };
            Vec3::new(v(0), v(1), v(2))
        })
        .collect();
    [
        [0, 2, 3, 1],
        [4, 5, 7, 6],
        [1, 3, 7, 5],
        [0, 4, 6, 2],
        [2, 6, 7, 3],
        [0, 1, 5, 4],
    ]
    .into_iter()
    .flat_map(|[a, b, c, d]| {
        [
            [points[a], points[b], points[c]],
            [points[a], points[c], points[d]],
        ]
    })
    .collect()
}

fn model(drawn: Vec<[Vec3; 3]>, collision: Vec<Shape>) -> Model {
    Model {
        drawn: vec![drawn],
        collision,
        physics: Arc::new(SetupGeometry::default()),
    }
}

#[test]
fn equal_closed_boxes_have_no_surface_gap_or_excess_collision_volume() {
    let m = model(cube(1.0), vec![Shape::Mesh(cube(1.0))]);
    let report = fit::measure(&m, 16).unwrap();
    assert_eq!(report.samples, 84);
    assert_eq!(report.within_five_cm, 84);
    assert_eq!(report.largest_gap, Some(0.0));
    let volume = report.volume.unwrap();
    assert_eq!(volume.collision_cells, 4096);
    assert_eq!(volume.excess_volume(), 0.0);
    assert_eq!(volume.cell_volume * 4096.0, 8.0);
}

#[test]
fn a_larger_collision_box_reports_its_one_meter_gap_and_fifty_six_excess_cubic_meters() {
    let m = model(cube(1.0), vec![Shape::Mesh(cube(2.0))]);
    let report = fit::measure(&m, 16).unwrap();
    assert_eq!(report.within_five_cm, 0);
    assert_eq!(report.largest_gap, Some(1.0));
    assert_eq!(report.volume.unwrap().excess_volume(), 56.0);
}

#[test]
fn open_surfaces_measure_their_gap_but_do_not_claim_an_enclosed_volume() {
    let ground = [
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
    ];
    let lifted = ground.map(|p| Vec3::new(p.x, p.y, 0.25));
    let m = model(vec![lifted], vec![Shape::Mesh(vec![ground])]);
    let report = fit::measure(&m, 8).unwrap();
    assert_eq!(report.samples, 7);
    assert_eq!(report.within_five_cm, 0);
    assert_eq!(report.largest_gap, Some(0.25));
    assert_eq!(report.volume, None);
}

#[test]
fn curved_collision_measures_distance_to_its_surface_including_cylinder_caps() {
    let t = [
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
        Vec3::new(0.0, 0.0, 1.0),
    ];
    let m = model(vec![t], vec![Shape::Sphere(Sphere::new(Vec3::ZERO, 1.0))]);
    let r = fit::measure(&m, 8).unwrap();
    assert_eq!(r.within_five_cm, 3);
    assert!((r.largest_gap.unwrap() - (1.0 - (1.0f32 / 3.0).sqrt())).abs() < 1e-6);
    let cap = [
        Vec3::new(0.0, 0.0, 2.25),
        Vec3::new(0.5, 0.0, 2.25),
        Vec3::new(0.0, 0.5, 2.25),
    ];
    let m = model(
        vec![cap],
        vec![Shape::Cylinder(CylSphere {
            low_pt: Vec3::ZERO,
            radius: 1.0,
            height: 2.0,
        })],
    );
    assert_eq!(fit::measure(&m, 8).unwrap().largest_gap, Some(0.25));
}

#[test]
fn overlapping_drawn_parts_are_a_union_and_missing_collision_is_reported() {
    let mut m = model(cube(1.0), vec![Shape::Mesh(cube(1.0))]);
    m.drawn.push(cube(1.0));
    assert_eq!(
        fit::measure(&m, 8).unwrap().volume.unwrap().excess_volume(),
        0.0
    );
    m.collision.clear();
    let r = fit::measure(&m, 8).unwrap();
    assert_eq!(r.largest_gap, None);
    assert_eq!(r.within_five_cm, 0);
    assert_eq!(r.volume.unwrap().excess_volume(), 0.0);
}

#[test]
fn transition_comparison_detects_a_new_blocker_and_agrees_for_identical_models() {
    let blocker = Arc::new(SetupGeometry {
        cyl_spheres: vec![CylSphere {
            low_pt: Vec3::ZERO,
            radius: 1.0,
            height: 4.0,
        }],
        radius: 1.0,
        height: 4.0,
        ..SetupGeometry::default()
    });
    let empty = Arc::new(SetupGeometry::default());
    let equal = fit::compare(&blocker, &blocker, 2.0, 8).unwrap();
    assert_eq!(equal.len(), 8);
    assert!(
        equal
            .iter()
            .all(|w| w.separation == 0.0 && w.candidate_stopped && w.original_stopped),
        "{equal:?}"
    );
    let changed = fit::compare(&blocker, &empty, 2.0, 8).unwrap();
    assert!(
        changed
            .iter()
            .all(|w| w.candidate_stopped && !w.original_stopped && w.separation > 1.0),
        "{changed:?}"
    );
}

#[test]
fn invalid_sampling_and_geometry_are_refused_before_measurement() {
    let mut m = model(cube(1.0), vec![Shape::Mesh(cube(1.0))]);
    assert!(fit::measure(&m, 0).is_err());
    assert!(fit::measure(&m, 65).is_err());
    m.drawn[0][0][0].x = f32::NAN;
    assert!(fit::measure(&m, 8).is_err());
    let huge = model(
        vec![[Vec3::new(f32::MAX, 0.0, 0.0); 3]],
        vec![Shape::Sphere(Sphere::new(Vec3::ZERO, 1.0))],
    );
    assert!(fit::measure(&huge, 8).is_err());
    let empty = Arc::new(SetupGeometry::default());
    assert!(fit::compare(&empty, &empty, 31.0, 8).is_err());
    assert!(fit::compare(&empty, &empty, 1.0, 0).is_err());
}
