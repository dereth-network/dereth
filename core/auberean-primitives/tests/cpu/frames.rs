use auberean_primitives::math::{Frame, Quat, Sphere, Vec3};

/// A new frame stands at the origin with no rotation (its scalar part one), a copy of a
/// frame has its origin and rotation, and a copy of a sphere its center and radius.
///
/// Behaviour: geometry.frame.a-new-frame-is-unturned-at-the-origin
#[test]
fn a_new_frame_is_unturned_at_the_origin() {
    let f = Frame::default();
    assert_eq!(f.origin, Vec3::new(0.0, 0.0, 0.0));
    assert_eq!(
        (f.rotation.w, f.rotation.x, f.rotation.y, f.rotation.z),
        (1.0, 0.0, 0.0, 0.0)
    );
    let g = Frame {
        origin: Vec3::new(1.0, -2.0, 3.5),
        rotation: Quat {
            w: 0.5,
            x: 0.5,
            y: -0.5,
            z: 0.5,
        },
    };
    let copy = g;
    assert_eq!(copy, g);
    let s = Sphere {
        center: Vec3::new(4.0, 5.0, -6.0),
        radius: 0.75,
    };
    let copy = s;
    assert_eq!(
        (copy.center, copy.radius),
        (Vec3::new(4.0, 5.0, -6.0), 0.75)
    );
}
