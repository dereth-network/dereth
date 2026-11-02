//! Placement into a physics tree: when the spheres are pushed out of what they are in.
//! Fixture: synthetic trees of one splitting plane, a wall or a floor square on one side.

use dereth_physics::geom::{BspNode, BspNodeKind, BspTree, Plane, Polygon, Sphere};
use dereth_physics::transition::collide::bsp_placement_insert;
use dereth_physics::transition::{Transition, TransitionState};
use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};

fn leaf(index: i32, solid: bool, polys: Vec<u32>) -> BspNode {
    BspNode {
        sphere: Sphere::new(Vec3::ZERO, 100.0),
        splitting_plane: Plane::default(),
        pos_child: None,
        neg_child: None,
        kind: BspNodeKind::Leaf {
            leaf_index: index,
            solid,
        },
        in_polys: polys,
    }
}

/// A tree split by the plane through the origin facing `normal`, its front leaf `front`, its back
/// leaf `back`, over `polygons`.
fn split(normal: Vec3, front: BspNode, back: BspNode, polygons: Vec<Polygon>) -> BspTree {
    BspTree {
        nodes: vec![
            BspNode {
                sphere: Sphere::new(Vec3::ZERO, 100.0),
                splitting_plane: Plane { normal, d: 0.0 },
                pos_child: Some(1),
                neg_child: Some(2),
                kind: BspNodeKind::Node,
                in_polys: Vec::new(),
            },
            front,
            back,
        ],
        polygons,
    }
}

/// A transition placing `spheres` (in the tree's own frame, which is the world's) at the origin.
fn placing(spheres: &[Sphere]) -> Transition {
    let at = Position::new(CellId(0x1020_0001), Frame::new(Vec3::ZERO, Quat::IDENTITY));
    let mut t = Transition::default();
    t.sphere_path.init_sphere(spheres, 1.0);
    t.sphere_path.init_path(Some(at.cell), None, &at);
    t.sphere_path.set_check_pos(&at, Some(at.cell));
    t.sphere_path.localspace_pos = at;
    for (i, s) in spheres.iter().enumerate() {
        t.sphere_path.localspace_sphere[i] = *s;
    }
    t
}

/// Placement pushes the spheres out only of what they are in: a sphere reaching across a tree's
/// plane, its center on the open side and overlapping a wall there, is taken where it is; a body
/// whose first sphere is clear and whose second sinks 0.3 m into a floor is lifted the 0.2 m that
/// takes the second out.
///
/// Behaviour: physics.transition.placement-pushes-the-spheres-out-of-what-they-are-in
#[test]
fn placement_pushes_the_spheres_out_of_what_they_are_in() {
    let wall = Polygon::new(vec![
        Vec3::new(0.3, -2.0, -2.0),
        Vec3::new(0.3, 2.0, -2.0),
        Vec3::new(0.3, 2.0, 2.0),
        Vec3::new(0.3, -2.0, 2.0),
    ]);
    let tree = split(
        Vec3::new(1.0, 0.0, 0.0),
        leaf(0, false, vec![0]),
        leaf(1, false, Vec::new()),
        vec![wall],
    );
    let mut t = placing(&[Sphere::new(Vec3::new(0.1, 0.0, 0.0), 0.5)]);
    assert_eq!(bsp_placement_insert(&mut t, &tree), TransitionState::Ok);
    assert_eq!(t.sphere_path.check_pos.frame.origin, Vec3::ZERO);

    let floor = Polygon::new(vec![
        Vec3::new(-2.0, -2.0, 0.0),
        Vec3::new(2.0, -2.0, 0.0),
        Vec3::new(2.0, 2.0, 0.0),
        Vec3::new(-2.0, 2.0, 0.0),
    ]);
    let tree = split(
        Vec3::new(0.0, 0.0, 1.0),
        leaf(0, false, Vec::new()),
        leaf(1, true, vec![0]),
        vec![floor],
    );
    let mut t = placing(&[
        Sphere::new(Vec3::new(0.0, 0.0, 2.0), 0.5),
        Sphere::new(Vec3::new(0.0, 0.0, 0.3), 0.5),
    ]);
    assert_eq!(
        bsp_placement_insert(&mut t, &tree),
        TransitionState::Adjusted
    );
    let lifted = t.sphere_path.check_pos.frame.origin;
    assert!((lifted.z - 0.2).abs() < 1e-5, "{lifted:?}");
}
