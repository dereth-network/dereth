//! Walks of a physics tree where a point or a sphere's center lies exactly on a splitting plane.
//! Fixture: synthetic trees of one splitting plane, a floor square in each leaf that holds one.

use dereth_physics::geom::{BspNode, BspNodeKind, BspTree, Plane, Polygon, Sphere};
use dereth_primitives::Vec3;

fn leaf(index: i32, polys: Vec<u32>) -> BspNode {
    BspNode {
        sphere: Sphere::new(Vec3::ZERO, 100.0),
        splitting_plane: Plane::default(),
        pos_child: None,
        neg_child: None,
        kind: BspNodeKind::Leaf {
            leaf_index: index,
            solid: false,
        },
        in_polys: polys,
    }
}

/// A tree split by the level plane through the origin, its front leaf `front` and its back leaf
/// `back`, over `polygons`.
fn split(front: BspNode, back: BspNode, polygons: Vec<Polygon>) -> BspTree {
    BspTree {
        nodes: vec![
            BspNode {
                sphere: Sphere::new(Vec3::ZERO, 100.0),
                splitting_plane: Plane {
                    normal: Vec3::new(0.0, 0.0, 1.0),
                    d: 0.0,
                },
                pos_child: Some(1),
                neg_child: Some(2),
                kind: BspNodeKind::Node,
                in_polys: Vec::new(),
            },
            front,
            back,
        ],
        polygons: polygons.into(),
    }
}

/// A level square 4 m on a side about the origin's column, `z` up.
fn floor_at(z: f32) -> Polygon {
    Polygon::new(vec![
        Vec3::new(-2.0, -2.0, z),
        Vec3::new(2.0, -2.0, z),
        Vec3::new(2.0, 2.0, z),
        Vec3::new(-2.0, 2.0, z),
    ])
}

/// A point exactly on a splitting plane is on its front side: with geometry in the front leaf
/// and none behind, the point is in solid; a point a hair behind the plane is not.
///
/// Behaviour: physics.trees.a-point-on-a-splitting-plane-is-in-front-of-it
#[test]
fn a_point_on_a_splitting_plane_is_in_front_of_it() {
    let tree = split(leaf(0, vec![0]), leaf(1, Vec::new()), vec![floor_at(0.5)]);
    assert!(
        tree.point_intersects_solid(Vec3::new(1.0, -0.5, 0.0)),
        "on the plane: the front leaf, which holds the floor"
    );
    assert!(!tree.point_intersects_solid(Vec3::new(1.0, -0.5, -1e-6)));
    assert_eq!(
        tree.leaf_path_for_point(Vec3::new(1.0, -0.5, 0.0)),
        vec![0, 1]
    );
}

/// A sphere whose center lies exactly on a splitting plane, reaching polygons on both sides, is
/// pushed off the one in front: the walk that names what the sphere is in takes the front half
/// first, and a center a hair behind the plane takes the back half first.
///
/// Behaviour: physics.trees.a-sphere-centered-on-a-splitting-plane-is-tested-in-front-first
#[test]
fn a_sphere_centered_on_a_splitting_plane_is_tested_in_front_first() {
    let tree = split(
        leaf(0, vec![0]),
        leaf(1, vec![1]),
        vec![floor_at(0.5), floor_at(-0.5)],
    );
    let named = |z: f32| {
        let sphere = Sphere::new(Vec3::new(0.3, 0.2, z), 1.0);
        let (mut center_solid, mut hit) = (false, None);
        tree.sphere_intersects_solid_poly(&sphere, 1.0, &mut center_solid, &mut hit, false);
        hit
    };
    assert_eq!(named(0.0), Some(0), "on the plane: the front floor");
    assert_eq!(named(-1e-6), Some(1), "behind it: the back floor");
}
