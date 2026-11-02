//! A sphere reaching a polygon's edge or corner exactly, in the polygon tests the transition asks.
//! Fixture: a level square 8 m on a side, its plane exact.

use dereth_physics::geom::{Polygon, Sphere};
use dereth_physics::globals::EPSILON;
use dereth_primitives::Vec3;

fn square() -> Polygon {
    let p = Polygon::new(vec![
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(8.0, 0.0, 0.0),
        Vec3::new(8.0, 8.0, 0.0),
        Vec3::new(0.0, 8.0, 0.0),
    ]);
    assert_eq!(p.plane.normal, Vec3::new(0.0, 0.0, 1.0));
    assert_eq!(p.plane.d, 0.0);
    p
}

/// A sphere that reaches exactly to the far end of a polygon's edge, or exactly to a corner,
/// touches the polygon: the overlap test (whose reach is the radius less 0.0002) and the walkable
/// footprint tests (the whole radius, and half of it) all count the exact reach as a touch.
///
/// Behaviour: physics.polygons.a-sphere-reaching-exactly-to-an-edge-or-a-corner-touches-it
#[test]
fn a_sphere_reaching_exactly_to_an_edge_or_a_corner_touches_it() {
    let p = square();
    let up = Vec3::new(0.0, 0.0, 1.0);
    // 1 m off the south edge, level with its east end: the edge's far end, exactly.
    let edge_end = Vec3::new(8.0, -1.0, 0.0);
    // 3 m east and 4 m south of the south-east corner: 5 m from it, exactly.
    let corner = Vec3::new(11.0, -4.0, 0.0);
    for (at, reach) in [(edge_end, 1.0_f32), (corner, 5.0)] {
        let mut contact = Vec3::ZERO;
        let overlap = Sphere::new(at, reach + 0.0002);
        assert!(
            p.polygon_hits_sphere_slow_but_sure(&overlap, &mut contact),
            "overlap at {at:?}"
        );
        let above = Vec3::new(at.x, at.y, 2.0);
        assert!(
            p.check_walkable(&Sphere::new(above, reach), up, EPSILON),
            "footprint at {at:?}"
        );
        assert!(
            p.check_small_walkable(&Sphere::new(above, 2.0 * reach), up, EPSILON),
            "half footprint at {at:?}"
        );
    }
}
