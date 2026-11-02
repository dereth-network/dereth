//! Detection: the volumes a body sets to notice objects, what it keeps of each object noticed,
//! the rays its lines of sight cast, and the binary form of all of it.
//! Fixture: settings and volumes built in the test.

use auberean_assets::Context;
use auberean_dat::Reader;
use auberean_physics::detect::{
    BlockRays, Detector, DetectorSettings, InstantDetection, ObjectRays, ReportType, Shape,
    Sighting, Volume,
};
use auberean_physics::geom::Ray;
use auberean_physics::pack::{Pack, Writer};
use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};

/// An attack cone puts its radii in order (each at least 0), brings each heading into 0 to 360
/// degrees, and puts its elevations in order, each held to -90 to 90 degrees; the heights of the
/// cut are the sines of the elevations taken as given.
///
/// Behaviour: collision.detect.an-attack-cone-orders-and-clamps-its-bounds
#[test]
fn an_attack_cone_orders_and_clamps_its_bounds() {
    let mut s = DetectorSettings::default();
    s.set_shape_attack_cone(Vec3::ZERO, 5.0, -1.0, 370.0, -30.0, 120.0, -100.0);
    let Shape::PlaneCone(c) = s.volume.shape else {
        panic!("a plane cone");
    };
    assert_eq!((c.min_radius, c.max_radius), (0.0, 5.0));
    assert_eq!((c.min_theta, c.max_theta), (10.0, 330.0));
    assert_eq!((c.min_phi, c.max_phi), (-90.0, 90.0));
    // The sine of 90 taken as radians, not degrees.
    assert!((c.max_z - 0.893_996_7).abs() < 1e-6);
    assert_eq!(c.min_z, -c.max_z);
}

/// A detection cylinder reaches its height above its center and the given depth below it; a box
/// spans the two corners it is given, in either order.
///
/// Behaviour: collision.detect.cylinders-and-boxes-span-what-they-are-given
#[test]
fn cylinders_and_boxes_span_what_they_are_given() {
    let mut s = DetectorSettings::default();
    s.set_shape_cylinder(2.0, 3.0, 1.0, Vec3::new(1.0, 2.0, 5.0));
    let Shape::Cylinder(c) = s.volume.shape else {
        panic!("a cylinder");
    };
    assert_eq!(c.low_pt, Vec3::new(1.0, 2.0, 4.0));
    assert_eq!((c.height, c.radius), (4.0, 2.0));

    s.set_shape_box(-1.0, -4.0, 2.0, 3.0, 1.0, -6.0);
    let Shape::Box(b) = s.volume.shape else {
        panic!("a box");
    };
    assert_eq!(b.min, Vec3::new(-3.0, -1.0, 1.0));
    assert_eq!(b.max, Vec3::new(2.0, 4.0, 6.0));
}

/// A line of sight that finds an object hidden (coverage at least 0.9998) drops its sighting;
/// otherwise the sighting keeps the greatest coverage found. Objects without a sighting are
/// pruned from a block.
///
/// Behaviour: collision.detect.a-hidden-object-loses-its-sighting
#[test]
fn a_hidden_object_loses_its_sighting() {
    let mut d = InstantDetection::new();
    for id in [1, 2, 3] {
        let mut s = Sighting::default();
        s.init(id);
        s.los_coverage = 0.5;
        d.sightings.insert(id, s);
    }
    let mut b = BlockRays::new(0x1010_0000);
    for (id, coverage) in [(1, 0.9998), (2, 0.3), (3, 0.7), (4, 0.1)] {
        let mut o = ObjectRays::new(id);
        o.coverage = coverage;
        assert!(b.add_object(o));
    }
    d.update_profiles(&b);
    assert!(!d.sightings.contains_key(&1));
    assert_eq!(d.sightings[&2].los_coverage, 0.5);
    assert_eq!(d.sightings[&3].los_coverage, 0.7);
    d.prune_objects(&mut b);
    assert_eq!(b.objects.keys().copied().collect::<Vec<_>>(), vec![2, 3]);
}

/// Copying a point volume gives the zero point and fails; other volumes copy whole.
///
/// Behaviour: collision.detect.a-point-volume-copies-as-the-zero-point
#[test]
fn a_point_volume_copies_as_the_zero_point() {
    let v = Volume {
        shape: Shape::Point(Vec3::new(1.0, 2.0, 3.0)),
        ..Volume::default()
    };
    let mut to = Volume::default();
    assert!(!v.copy_into(&mut to));
    assert_eq!(to.shape, Shape::Point(Vec3::ZERO));

    let mut v = Volume::default();
    v.create_sphere(4.0, Vec3::new(0.0, 0.0, 1.0));
    assert!(v.copy_into(&mut to));
    assert_eq!(to, v);
}

fn round_trip<T: Pack + PartialEq + std::fmt::Debug>(v: &T) {
    let ctx = Context::empty();
    let mut w = Writer::new();
    v.pack(&mut w);
    let mut r = Reader::new(w.as_bytes());
    let back = T::unpack(&mut r, &ctx).expect("unpacks");
    assert!(r.at_end());
    assert_eq!(&back, v);
}

/// A detection's settings with every shape of volume, a detector, a sighting and an instant
/// detection with its blocks keep every field sent through their binary form.
///
/// Behaviour: collision.detect.detection-state-survives-packing
#[test]
fn detection_state_survives_packing() {
    let at = Position::new(
        CellId(0x1010_0012),
        Frame::new(Vec3::new(1.0, 2.0, 3.0), Quat::new(0.6, 0.0, 0.0, 0.8)),
    );
    let mut s = DetectorSettings {
        context: 9,
        ..DetectorSettings::default()
    };
    s.set_base_position(&at);
    s.set_origin_point(&at);
    s.add_target(0x55);
    s.add_exclude_object(0x66);
    s.report = ReportType { mask: 0x8000_0001 };
    s.object_blocking.add_to_solid_list(0x77);
    let shapes: [fn(&mut DetectorSettings); 5] = [
        |s| s.set_shape_box(1.0, 2.0, 3.0, 4.0, 5.0, 6.0),
        |s| s.set_shape_sphere(2.0, Vec3::new(0.0, 1.0, 0.0)),
        |s| s.set_shape_sphere_cone(Vec3::ZERO, 3.0, Vec3::new(0.0, 1.0, 0.0), 0.5),
        |s| s.set_shape_attack_cone(Vec3::ZERO, 1.0, 4.0, 10.0, 80.0, -20.0, 30.0),
        |s| s.set_shape_cylinder(1.0, 2.0, 0.5, Vec3::ZERO),
    ];
    for f in shapes {
        f(&mut s);
        round_trip(&s);
    }
    s.volume.shape = Shape::Ray(Ray::from_offset(Vec3::ZERO, Vec3::new(0.0, 3.0, 4.0)));
    round_trip(&s);

    round_trip(&Detector::new(4, 0x99, 12.5, ReportType { mask: 3 }));
    let mut g = Sighting::default();
    g.init(0x99);
    g.position = at;
    g.set_int_app(2, 7);
    g.set_float_app(3, 1.5);
    round_trip(&g);

    let mut d = InstantDetection::new();
    d.init(3, &s);
    d.create_block_list_info(0x1010_0000);
    d.add_block_ray(Ray::default(), 0x99, 0x1011_0000);
    d.add_block_ray(Ray::default(), 0x99, 0x1011_0000);
    assert_eq!(d.block_rays[0].objects[&0x99].rays.len(), 2);
    d.remote_block_count = 1;
    round_trip(&d);
}

/// A cylinder volume is streamed as its base, then its radius, then its height.
///
/// Behaviour: collision.detect.a-cylinder-volume-streams-its-radius-before-its-height
#[test]
fn a_cylinder_volume_streams_its_radius_before_its_height() {
    let mut v = volume(Shape::None);
    v.create_cylinder(1.5, Vec3::new(0.0, 0.0, -0.5), 4.0);
    let mut w = Writer::new();
    v.pack(&mut w);
    let bytes = w.finish();
    let tail: Vec<f32> = bytes[bytes.len() - 8..]
        .chunks(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect();
    assert_eq!(tail, vec![1.5, 4.0], "the radius, then the height");

    let mut r = Reader::new(&bytes);
    let back = Volume::unpack(&mut r, &Context::empty()).expect("unpacks");
    assert!(r.at_end());
    assert_eq!(back, v);
}

/// A point volume has no binary form: packed, it is its place and its kind only, and reading
/// that back fails.
///
/// Behaviour: collision.detect.a-point-volume-cannot-be-streamed
#[test]
fn a_point_volume_cannot_be_streamed() {
    let v = volume(Shape::Point(Vec3::new(1.0, 2.0, 3.0)));
    let mut w = Writer::new();
    v.pack(&mut w);
    let bytes = w.finish();
    assert_eq!(bytes.len(), 36, "a place (32 bytes) and the kind");
    assert_eq!(bytes[32..], 2u32.to_le_bytes());
    let mut r = Reader::new(&bytes);
    assert!(Volume::unpack(&mut r, &Context::empty()).is_err());
}

fn at_origin() -> Position {
    Position::new(
        CellId(0x4040_0001),
        Frame::new(Vec3::new(10.0, 10.0, 0.0), Quat::IDENTITY),
    )
}

fn volume(shape: Shape) -> Volume {
    Volume {
        position: at_origin(),
        shape,
    }
}

/// A cone reaches a sphere inside its angle and radius, or one at about its radius whose own size
/// reaches over the cone's edge there; not one behind it or beyond its radius, and not one off its
/// angle wholly inside its radius, however wide. A direction only longer than a unit is brought
/// to unit length before the angle test.
///
/// Behaviour: collision.detect.a-cone-reaches-what-lies-inside-its-angle
#[test]
fn a_cone_reaches_what_lies_inside_its_angle() {
    use auberean_physics::geom::{Cone, Sphere};
    // Ten meters along +y, thirty degrees each way.
    let cone = Cone::new(
        Vec3::ZERO,
        10.0,
        Vec3::new(0.0, 1.0, 0.0),
        std::f32::consts::FRAC_PI_6,
    );
    assert!(cone.intersects_sphere(&Sphere::new(Vec3::new(0.0, 5.0, 0.0), 0.1)));
    assert!(!cone.intersects_sphere(&Sphere::new(Vec3::new(0.0, -5.0, 0.0), 0.1)));
    assert!(!cone.intersects_sphere(&Sphere::new(Vec3::new(0.0, 12.0, 0.0), 0.5)));
    // Off the angle (45 degrees out): at the radius, wide enough to reach over the edge there.
    assert!(!cone.intersects_sphere(&Sphere::new(Vec3::new(4.0, 4.0, 0.0), 0.1)));
    assert!(cone.intersects_sphere(&Sphere::new(Vec3::new(7.07, 7.07, 0.0), 4.0)));
    assert!(
        !cone.intersects_sphere(&Sphere::new(Vec3::new(2.83, 2.83, 0.0), 2.0)),
        "wholly inside the radius"
    );
    // A long direction along the axis is inside; a short one off the axis is taken as it is.
    assert!(cone.angle_check(Vec3::new(0.0, 3.0, 0.0)));
    assert!(
        !cone.angle_check(Vec3::new(0.0, 0.5, 0.0)),
        "0.5 is under the cosine"
    );
}

/// A shell cut to headings and elevations reaches a sphere in its cut between its radii; one
/// outside the heading range only when the ray along the nearest heading the cut allows meets it;
/// a range whose least heading is the greater wraps through north.
///
/// Behaviour: collision.detect.a-shell-reaches-what-lies-in-its-cut
#[test]
fn a_shell_reaches_what_lies_in_its_cut() {
    use auberean_physics::geom::{PlaneCone, Sphere};
    // North-east quarter, level to straight up, 1 to 10 m.
    let shell = PlaneCone {
        min_z: 0.0,
        max_z: 1.0,
        ..PlaneCone::new(
            Vec3::ZERO,
            1.0,
            10.0,
            0.0,
            90.0,
            0.0,
            std::f32::consts::FRAC_PI_2,
        )
    };
    assert!(shell.intersects_sphere(&Sphere::new(Vec3::new(3.0, 3.0, 1.0), 0.2)));
    assert!(!shell.intersects_sphere(&Sphere::new(Vec3::new(-3.0, -3.0, 1.0), 0.2)));
    assert!(
        !shell.intersects_sphere(&Sphere::new(Vec3::new(0.2, 0.2, 0.0), 0.1)),
        "inside the inner radius"
    );
    // Just west of north: out of the range, but the ray along north (the nearest bound) meets it.
    assert!(shell.intersects_sphere(&Sphere::new(Vec3::new(-0.3, 5.0, 0.0), 0.5)));
    assert!(!shell.intersects_sphere(&Sphere::new(Vec3::new(-2.0, 5.0, 0.0), 0.5)));
    // A range from 315 round to 45 holds north and not south.
    let wrapped = PlaneCone {
        min_theta: 315.0,
        max_theta: 45.0,
        ..shell
    };
    assert!(wrapped.check_angle(Vec3::new(0.0, 1.0, 0.0)));
    assert!(!wrapped.check_angle(Vec3::new(0.0, -1.0, 0.0)));
}

/// Spheres, upright cylinders and rays: a cylinder reaches a sphere within their two radii of the
/// nearest point of its axis, above or beside it, and another cylinder whose height overlaps; a
/// ray finds a sphere or a cylinder at the fraction of its length where it first meets it, from
/// inside at once.
///
/// Behaviour: collision.detect.spheres-and-cylinders-meet-and-stop-rays
#[test]
fn spheres_and_cylinders_meet_and_stop_rays() {
    use auberean_physics::geom::{CylSphere, CylinderOps, Sphere, SphereOps};
    let post = CylSphere {
        low_pt: Vec3::ZERO,
        radius: 1.0,
        height: 2.0,
    };
    assert!(post.intersects_sphere(&Sphere::new(Vec3::new(1.4, 0.0, 1.0), 0.5)));
    assert!(!post.intersects_sphere(&Sphere::new(Vec3::new(1.6, 0.0, 1.0), 0.5)));
    assert!(
        post.intersects_sphere(&Sphere::new(Vec3::new(0.0, 0.0, 3.4), 0.5)),
        "above it"
    );
    assert!(!post.intersects_sphere(&Sphere::new(Vec3::new(0.0, 0.0, 3.6), 0.5)));
    let other = |z: f32| CylSphere {
        low_pt: Vec3::new(1.5, 0.0, z),
        radius: 1.0,
        height: 1.0,
    };
    assert!(post.intersects_cylinder(&other(1.5)));
    assert!(!post.intersects_cylinder(&other(3.5)));
    let ray = Ray {
        point: Vec3::new(-5.0, 0.0, 1.0),
        dir: Vec3::new(1.0, 0.0, 0.0),
        length: 10.0,
    };
    let t = post.trace_ray(&ray).expect("meets the post");
    assert!((t - 0.4).abs() < 1e-5, "{t}");
    let ball = Sphere::new(Vec3::new(0.0, 0.0, 1.0), 1.0);
    let t = ball.trace_ray(&ray).expect("meets the ball");
    assert!((t - 0.4).abs() < 1e-5, "{t}");
    let away = Ray {
        dir: Vec3::new(-1.0, 0.0, 0.0),
        ..ray
    };
    assert_eq!(post.trace_ray(&away), None);
    assert_eq!(ball.trace_ray(&away), None);
    let inside = Ray {
        point: Vec3::new(0.0, 0.0, 1.0),
        ..ray
    };
    assert_eq!(ball.trace_ray(&inside), Some(0.0));
}

/// A detection volume answers whether a sphere, a cylinder or a box elsewhere reaches it, each
/// carried into the volume's frame (a sphere scaled first when its scale is even); its global box
/// is the box around its shape where it stands; a point or a ray volume reaches nothing and has
/// no box.
///
/// Behaviour: collision.detect.a-volume-takes-in-shapes-from-elsewhere
#[test]
fn a_volume_takes_in_shapes_from_elsewhere() {
    use auberean_physics::geom::{BBox, CylSphere, Sphere};
    let ball = volume(Shape::Ball(Sphere::new(Vec3::ZERO, 2.0)));
    // Elsewhere: 4 m east of the volume's place.
    let east = Position::new(
        CellId(0x4040_0001),
        Frame::new(Vec3::new(14.0, 10.0, 0.0), Quat::IDENTITY),
    );
    let near = Sphere::new(Vec3::new(-2.0, 0.0, 0.0), 0.2);
    assert!(ball.is_within_sphere(&near, &east, Vec3::new(1.0, 1.0, 1.0)));
    assert!(
        !ball.is_within_sphere(&near, &east, Vec3::new(0.5, 0.5, 0.5)),
        "scaled away"
    );
    assert!(
        ball.is_within_sphere(&near, &east, Vec3::new(0.5, 1.0, 1.0)),
        "uneven: as it is"
    );
    let post = CylSphere {
        low_pt: Vec3::new(-2.5, 0.0, -1.0),
        radius: 0.6,
        height: 2.0,
    };
    assert!(ball.is_within_cylinder(&post, &east));
    let b = BBox::new(Vec3::new(-2.5, -0.5, -0.5), Vec3::new(-1.5, 0.5, 0.5));
    assert!(ball.is_within_box(&b, &east));
    let close = |a: Vec3, b: Vec3| (a.x - b.x).abs() + (a.y - b.y).abs() + (a.z - b.z).abs() < 1e-4;
    let g = ball.global_box().expect("a box");
    assert!(close(g.min, Vec3::new(8.0, 8.0, -2.0)) && close(g.max, Vec3::new(12.0, 12.0, 2.0)));
    let point = volume(Shape::Point(Vec3::ZERO));
    assert!(!point.is_within_sphere(&near, &east, Vec3::new(1.0, 1.0, 1.0)));
    assert_eq!(point.global_box(), None);
    let cyl = volume(Shape::Cylinder(post));
    let g = cyl.global_box().expect("a box");
    assert!(
        close(g.min, Vec3::new(6.9, 9.4, -1.0)) && close(g.max, Vec3::new(8.1, 10.6, 1.0)),
        "{g:?}"
    );
}

/// One question put to the September 2013 client's own shape code: a name, the test asked, its
/// inputs as the bit patterns of 32-bit floats (in the order center, radius, direction, angle for a
/// cone; center, radii, headings, elevations and the heights written over them for a shell;
/// center, radius for a sphere; low point, height, radius for a cylinder; corners for a box; point,
/// direction, length for a ray; a flag as 0 or 1; a position as its cell id, then its origin and
/// its rotation w, x, y, z), the client's answer, and the values it gave (a constructor's stored
/// values, the fraction a ray's test wrote, or a box's corners).
type Asked = (
    &'static str,
    &'static str,
    &'static [u32],
    Option<bool>,
    &'static [u32],
);

/// The client's answers, case by case.
static ANSWERS: &[Asked] = &[
    (
        "cone-cos-30",
        "cone_new",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x41200000, 0x00000000, 0x3f800000, 0x00000000,
            0x3f060a92,
        ],
        None,
        &[0x3f5db3d7],
    ),
    (
        "cone-cos-0",
        "cone_new",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x3f800000, 0x00000000, 0x00000000, 0x3f800000,
            0x00000000,
        ],
        None,
        &[0x3f800000],
    ),
    (
        "cone-cos-90",
        "cone_new",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x3f800000, 0x00000000, 0x00000000, 0x3f800000,
            0x3fc90fdb,
        ],
        None,
        &[0xb33bbd2e],
    ),
    (
        "cone-cos-2",
        "cone_new",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x3f800000, 0x00000000, 0x00000000, 0x3f800000,
            0x40000000,
        ],
        None,
        &[0xbed51133],
    ),
    (
        "shell-heights-90",
        "plane_cone_new",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x40a00000, 0x41200000, 0x43a50000,
            0xc2b40000, 0x42b40000,
        ],
        None,
        &[0xbf64dcf7, 0x3f64dcf7],
    ),
    (
        "shell-heights-30",
        "plane_cone_new",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x3f800000, 0x40800000, 0x41200000, 0x42a00000,
            0xc1a00000, 0x41f00000,
        ],
        None,
        &[0xbf69b6c8, 0xbf7cefa4],
    ),
    (
        "shell-heights-small",
        "plane_cone_new",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x3f800000, 0x40800000, 0x00000000, 0x42b40000,
            0x00000000, 0x3fc90fdb,
        ],
        None,
        &[0x00000000, 0x3f800000],
    ),
    (
        "cone-angle-long-axis",
        "cone_angle",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x41200000, 0x00000000, 0x3f800000, 0x00000000,
            0x3f060a92, 0x00000000, 0x40400000, 0x00000000,
        ],
        Some(true),
        &[],
    ),
    (
        "cone-angle-short-axis",
        "cone_angle",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x41200000, 0x00000000, 0x3f800000, 0x00000000,
            0x3f060a92, 0x00000000, 0x3f000000, 0x00000000,
        ],
        Some(false),
        &[],
    ),
    (
        "cone-angle-unit-axis",
        "cone_angle",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x41200000, 0x00000000, 0x3f800000, 0x00000000,
            0x3f060a92, 0x00000000, 0x3f800000, 0x00000000,
        ],
        Some(true),
        &[],
    ),
    (
        "cone-angle-edge",
        "cone_angle",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x41200000, 0x00000000, 0x3f800000, 0x00000000,
            0x3f060a92, 0x3f000000, 0x3f5db3d7, 0x00000000,
        ],
        Some(false),
        &[],
    ),
    (
        "cone-angle-just-long",
        "cone_angle",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x41200000, 0x00000000, 0x3f800000, 0x00000000,
            0x3f060a92, 0x00000000, 0x3f800347, 0x00000000,
        ],
        Some(true),
        &[],
    ),
    (
        "cone-angle-off",
        "cone_angle",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x41200000, 0x00000000, 0x3f800000, 0x00000000,
            0x3f060a92, 0x3f800000, 0x3f800000, 0x00000000,
        ],
        Some(false),
        &[],
    ),
    (
        "cone-angle-zero",
        "cone_angle",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x41200000, 0x00000000, 0x3f800000, 0x00000000,
            0x3f060a92, 0x00000000, 0x00000000, 0x00000000,
        ],
        Some(false),
        &[],
    ),
    (
        "cone-sphere-inside",
        "cone_sphere",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x41200000, 0x00000000, 0x3f800000, 0x00000000,
            0x3f060a92, 0x00000000, 0x40a00000, 0x00000000, 0x3dcccccd,
        ],
        Some(true),
        &[],
    ),
    (
        "cone-sphere-behind",
        "cone_sphere",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x41200000, 0x00000000, 0x3f800000, 0x00000000,
            0x3f060a92, 0x00000000, 0xc0a00000, 0x00000000, 0x3dcccccd,
        ],
        Some(false),
        &[],
    ),
    (
        "cone-sphere-beyond",
        "cone_sphere",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x41200000, 0x00000000, 0x3f800000, 0x00000000,
            0x3f060a92, 0x00000000, 0x41400000, 0x00000000, 0x3f000000,
        ],
        Some(false),
        &[],
    ),
    (
        "cone-sphere-off-small",
        "cone_sphere",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x41200000, 0x00000000, 0x3f800000, 0x00000000,
            0x3f060a92, 0x40800000, 0x40800000, 0x00000000, 0x3dcccccd,
        ],
        Some(false),
        &[],
    ),
    (
        "cone-sphere-off-wide",
        "cone_sphere",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x41200000, 0x00000000, 0x3f800000, 0x00000000,
            0x3f060a92, 0x40e23d71, 0x40e23d71, 0x00000000, 0x40800000,
        ],
        Some(true),
        &[],
    ),
    (
        "cone-sphere-off-inside-radius",
        "cone_sphere",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x41200000, 0x00000000, 0x3f800000, 0x00000000,
            0x3f060a92, 0x40351eb8, 0x40351eb8, 0x00000000, 0x40000000,
        ],
        Some(false),
        &[],
    ),
    (
        "cone-sphere-at-center",
        "cone_sphere",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x41200000, 0x00000000, 0x3f800000, 0x00000000,
            0x3f060a92, 0x00000000, 0x00000000, 0x00000000, 0x3f800000,
        ],
        Some(false),
        &[],
    ),
    (
        "cone-sphere-touching-radius",
        "cone_sphere",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x41200000, 0x00000000, 0x3f800000, 0x00000000,
            0x3f060a92, 0x00000000, 0x41300000, 0x00000000, 0x3f800000,
        ],
        Some(true),
        &[],
    ),
    (
        "cone-sphere-no-radius",
        "cone_sphere",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x41200000, 0x00000000, 0x3f800000, 0x00000000,
            0x3f060a92, 0x40c00000, 0x40c00000, 0x00000000, 0x00000000,
        ],
        Some(false),
        &[],
    ),
    (
        "cone-sphere-cone-no-radius",
        "cone_sphere",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x3f800000, 0x00000000,
            0x3f060a92, 0x3f000000, 0x3f000000, 0x00000000, 0x3f800000,
        ],
        Some(true),
        &[],
    ),
    (
        "cone-sphere-reaches-edge",
        "cone_sphere",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x41200000, 0x00000000, 0x3f800000, 0x00000000,
            0x3f060a92, 0x40a00000, 0x40c00000, 0x00000000, 0x40000000,
        ],
        Some(false),
        &[],
    ),
    (
        "cone-sphere-just-misses-edge",
        "cone_sphere",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x41200000, 0x00000000, 0x3f800000, 0x00000000,
            0x3f060a92, 0x40c00000, 0x40a00000, 0x00000000, 0x3f800000,
        ],
        Some(false),
        &[],
    ),
    (
        "cone-sphere-zero-angle",
        "cone_sphere",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x41200000, 0x00000000, 0x3f800000, 0x00000000,
            0x00000000, 0x00000000, 0x40a00000, 0x00000000, 0x3f000000,
        ],
        Some(false),
        &[],
    ),
    (
        "cone-sphere-half-space",
        "cone_sphere",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x41200000, 0x00000000, 0x3f800000, 0x00000000,
            0x3fc90fdb, 0x40a00000, 0x3dcccccd, 0x00000000, 0x3dcccccd,
        ],
        Some(true),
        &[],
    ),
    (
        "cone-cyl-ahead",
        "cone_cyl",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x41200000, 0x00000000, 0x3f800000, 0x00000000,
            0x3f060a92, 0x00000000, 0x40a00000, 0xbf800000, 0x40000000, 0x3f000000,
        ],
        Some(true),
        &[],
    ),
    (
        "cone-cyl-behind",
        "cone_cyl",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x41200000, 0x00000000, 0x3f800000, 0x00000000,
            0x3f060a92, 0x00000000, 0xc0a00000, 0xbf800000, 0x40000000, 0x3f000000,
        ],
        Some(false),
        &[],
    ),
    (
        "cone-cyl-beside",
        "cone_cyl",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x41200000, 0x00000000, 0x3f800000, 0x00000000,
            0x3f060a92, 0x40000000, 0x40c00000, 0xbf800000, 0x40000000, 0x3f000000,
        ],
        Some(true),
        &[],
    ),
    (
        "cone-cyl-off",
        "cone_cyl",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x41200000, 0x00000000, 0x3f800000, 0x00000000,
            0x3f060a92, 0x40c00000, 0x40000000, 0xbf800000, 0x40000000, 0x3f000000,
        ],
        Some(false),
        &[],
    ),
    (
        "cone-cyl-above",
        "cone_cyl",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x41200000, 0x00000000, 0x3f800000, 0x00000000,
            0x3f060a92, 0x00000000, 0x40a00000, 0x40400000, 0x40000000, 0x3f000000,
        ],
        Some(true),
        &[],
    ),
    (
        "cone-cyl-flat",
        "cone_cyl",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x41200000, 0x00000000, 0x3f800000, 0x00000000,
            0x3f060a92, 0x3f800000, 0x40a00000, 0x00000000, 0x00000000, 0x3f000000,
        ],
        Some(true),
        &[],
    ),
    (
        "cone-cyl-thin",
        "cone_cyl",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x41200000, 0x00000000, 0x3f800000, 0x00000000,
            0x3f060a92, 0x3f800000, 0x40a00000, 0xbf800000, 0x40000000, 0x00000000,
        ],
        Some(true),
        &[],
    ),
    (
        "cone-cyl-tall-up-cone",
        "cone_cyl",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x41200000, 0x00000000, 0x00000000, 0x3f800000,
            0x3f060a92, 0x40000000, 0x00000000, 0x00000000, 0x41a00000, 0x3f000000,
        ],
        Some(true),
        &[],
    ),
    (
        "cone-box-tip",
        "cone_box",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x41200000, 0x00000000, 0x3f800000, 0x00000000,
            0x3f060a92, 0xbf800000, 0x41100000, 0xbf800000, 0x3f800000, 0x41300000, 0x3f800000,
        ],
        Some(true),
        &[],
    ),
    (
        "cone-box-center",
        "cone_box",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x41200000, 0x00000000, 0x3f800000, 0x00000000,
            0x3f060a92, 0xbf800000, 0xbf800000, 0xbf800000, 0x3f800000, 0x3f800000, 0x3f800000,
        ],
        Some(true),
        &[],
    ),
    (
        "cone-box-in-angle",
        "cone_box",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x41200000, 0x00000000, 0x3f800000, 0x00000000,
            0x3f060a92, 0xbf000000, 0x40400000, 0xbf000000, 0x3f000000, 0x41400000, 0x3f000000,
        ],
        Some(true),
        &[],
    ),
    (
        "cone-box-off-angle",
        "cone_box",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x41200000, 0x00000000, 0x3f800000, 0x00000000,
            0x3f060a92, 0x40a00000, 0x00000000, 0xbf800000, 0x41000000, 0x40400000, 0x3f800000,
        ],
        Some(false),
        &[],
    ),
    (
        "cone-box-short",
        "cone_box",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x41200000, 0x00000000, 0x3f800000, 0x00000000,
            0x3f060a92, 0xbf000000, 0x40400000, 0xbf000000, 0x3f000000, 0x40800000, 0x3f000000,
        ],
        Some(false),
        &[],
    ),
    (
        "cone-box-far",
        "cone_box",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x41200000, 0x00000000, 0x3f800000, 0x00000000,
            0x3f060a92, 0xbf000000, 0x41f00000, 0xbf000000, 0x3f000000, 0x41f80000, 0x3f000000,
        ],
        Some(false),
        &[],
    ),
    (
        "shell-angle-north",
        "plane_angle",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x3f800000, 0x41200000, 0x00000000, 0x42b40000,
            0x00000000, 0x3fc90fdb, 0x00000000, 0x3f800000, 0x00000000, 0x3f800000, 0x00000000,
        ],
        Some(true),
        &[],
    ),
    (
        "shell-angle-east",
        "plane_angle",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x3f800000, 0x41200000, 0x00000000, 0x42b40000,
            0x00000000, 0x3fc90fdb, 0x00000000, 0x3f800000, 0x3f800000, 0x00000000, 0x00000000,
        ],
        Some(true),
        &[],
    ),
    (
        "shell-angle-south",
        "plane_angle",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x3f800000, 0x41200000, 0x00000000, 0x42b40000,
            0x00000000, 0x3fc90fdb, 0x00000000, 0x3f800000, 0x00000000, 0xbf800000, 0x00000000,
        ],
        Some(false),
        &[],
    ),
    (
        "shell-angle-ne-up",
        "plane_angle",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x3f800000, 0x41200000, 0x00000000, 0x42b40000,
            0x00000000, 0x3fc90fdb, 0x00000000, 0x3f800000, 0x3f000000, 0x3f000000, 0x3f350481,
        ],
        Some(true),
        &[],
    ),
    (
        "shell-angle-ne-down",
        "plane_angle",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x3f800000, 0x41200000, 0x00000000, 0x42b40000,
            0x00000000, 0x3fc90fdb, 0x00000000, 0x3f800000, 0x3f000000, 0x3f000000, 0xbdcccccd,
        ],
        Some(false),
        &[],
    ),
    (
        "shell-angle-west",
        "plane_angle",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x3f800000, 0x41200000, 0x00000000, 0x42b40000,
            0x00000000, 0x3fc90fdb, 0x00000000, 0x3f800000, 0xbf800000, 0x00000000, 0x00000000,
        ],
        Some(false),
        &[],
    ),
    (
        "wrap-angle-north",
        "plane_angle",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x3f800000, 0x41200000, 0x439d8000, 0x42340000,
            0x00000000, 0x3fc90fdb, 0x00000000, 0x3f800000, 0x00000000, 0x3f800000, 0x00000000,
        ],
        Some(true),
        &[],
    ),
    (
        "wrap-angle-south",
        "plane_angle",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x3f800000, 0x41200000, 0x439d8000, 0x42340000,
            0x00000000, 0x3fc90fdb, 0x00000000, 0x3f800000, 0x00000000, 0xbf800000, 0x00000000,
        ],
        Some(false),
        &[],
    ),
    (
        "wrap-angle-nw",
        "plane_angle",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x3f800000, 0x41200000, 0x439d8000, 0x42340000,
            0x00000000, 0x3fc90fdb, 0x00000000, 0x3f800000, 0xbf000000, 0x3f5db3d7, 0x00000000,
        ],
        Some(true),
        &[],
    ),
    (
        "wrap-angle-east",
        "plane_angle",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x3f800000, 0x41200000, 0x439d8000, 0x42340000,
            0x00000000, 0x3fc90fdb, 0x00000000, 0x3f800000, 0x3f800000, 0x00000000, 0x00000000,
        ],
        Some(false),
        &[],
    ),
    (
        "all-round-angle",
        "plane_angle",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x3f800000, 0x41200000, 0x41f00000, 0x41f00000,
            0x00000000, 0x3fc90fdb, 0x00000000, 0x3f800000, 0x00000000, 0xbf800000, 0x00000000,
        ],
        Some(true),
        &[],
    ),
    (
        "shell-sphere-in-cut",
        "plane_sphere",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x3f800000, 0x41200000, 0x00000000, 0x42b40000,
            0x00000000, 0x3fc90fdb, 0x00000000, 0x3f800000, 0x40400000, 0x40400000, 0x3f800000,
            0x3e4ccccd,
        ],
        Some(true),
        &[],
    ),
    (
        "shell-sphere-opposite",
        "plane_sphere",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x3f800000, 0x41200000, 0x00000000, 0x42b40000,
            0x00000000, 0x3fc90fdb, 0x00000000, 0x3f800000, 0xc0400000, 0xc0400000, 0x3f800000,
            0x3e4ccccd,
        ],
        Some(false),
        &[],
    ),
    (
        "shell-sphere-inside-inner",
        "plane_sphere",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x3f800000, 0x41200000, 0x00000000, 0x42b40000,
            0x00000000, 0x3fc90fdb, 0x00000000, 0x3f800000, 0x3e4ccccd, 0x3e4ccccd, 0x00000000,
            0x3dcccccd,
        ],
        Some(false),
        &[],
    ),
    (
        "shell-sphere-near-north",
        "plane_sphere",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x3f800000, 0x41200000, 0x00000000, 0x42b40000,
            0x00000000, 0x3fc90fdb, 0x00000000, 0x3f800000, 0xbe99999a, 0x40a00000, 0x00000000,
            0x3f000000,
        ],
        Some(true),
        &[],
    ),
    (
        "shell-sphere-off-north",
        "plane_sphere",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x3f800000, 0x41200000, 0x00000000, 0x42b40000,
            0x00000000, 0x3fc90fdb, 0x00000000, 0x3f800000, 0xc0000000, 0x40a00000, 0x00000000,
            0x3f000000,
        ],
        Some(false),
        &[],
    ),
    (
        "shell-sphere-beyond",
        "plane_sphere",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x3f800000, 0x41200000, 0x00000000, 0x42b40000,
            0x00000000, 0x3fc90fdb, 0x00000000, 0x3f800000, 0x41000000, 0x41000000, 0x00000000,
            0x3f000000,
        ],
        Some(false),
        &[],
    ),
    (
        "shell-sphere-below-cut",
        "plane_sphere",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x3f800000, 0x41200000, 0x00000000, 0x42b40000,
            0x00000000, 0x3fc90fdb, 0x00000000, 0x3f800000, 0x40400000, 0x40400000, 0xbf800000,
            0x3f000000,
        ],
        Some(false),
        &[],
    ),
    (
        "shell-sphere-below-cut-wide",
        "plane_sphere",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x3f800000, 0x41200000, 0x00000000, 0x42b40000,
            0x00000000, 0x3fc90fdb, 0x00000000, 0x3f800000, 0x40400000, 0x40400000, 0xbf19999a,
            0x3f4ccccd,
        ],
        Some(true),
        &[],
    ),
    (
        "shell-sphere-near-east",
        "plane_sphere",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x3f800000, 0x41200000, 0x00000000, 0x42b40000,
            0x00000000, 0x3fc90fdb, 0x00000000, 0x3f800000, 0x40a00000, 0xbe99999a, 0x00000000,
            0x3f000000,
        ],
        Some(true),
        &[],
    ),
    (
        "wrap-sphere-near-west-bound",
        "plane_sphere",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x3f800000, 0x41200000, 0x439d8000, 0x42340000,
            0x00000000, 0x3fc90fdb, 0x00000000, 0x3f800000, 0xc0733333, 0x40600000, 0x00000000,
            0x3f000000,
        ],
        Some(true),
        &[],
    ),
    (
        "wrap-sphere-near-east-bound",
        "plane_sphere",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x3f800000, 0x41200000, 0x439d8000, 0x42340000,
            0x00000000, 0x3fc90fdb, 0x00000000, 0x3f800000, 0x40733333, 0x40600000, 0x00000000,
            0x3f000000,
        ],
        Some(true),
        &[],
    ),
    (
        "wrap-sphere-south",
        "plane_sphere",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x3f800000, 0x41200000, 0x439d8000, 0x42340000,
            0x00000000, 0x3fc90fdb, 0x00000000, 0x3f800000, 0x00000000, 0xc0a00000, 0x00000000,
            0x3f000000,
        ],
        Some(false),
        &[],
    ),
    (
        "shell-cyl-in-cut",
        "plane_cyl",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x3f800000, 0x41200000, 0x00000000, 0x42b40000,
            0x00000000, 0x3fc90fdb, 0x00000000, 0x3f800000, 0x40400000, 0x40400000, 0xbf800000,
            0x40400000, 0x3f000000,
        ],
        Some(true),
        &[],
    ),
    (
        "shell-cyl-opposite",
        "plane_cyl",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x3f800000, 0x41200000, 0x00000000, 0x42b40000,
            0x00000000, 0x3fc90fdb, 0x00000000, 0x3f800000, 0xc0400000, 0xc0400000, 0xbf800000,
            0x40400000, 0x3f000000,
        ],
        Some(false),
        &[],
    ),
    (
        "shell-cyl-near-north",
        "plane_cyl",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x3f800000, 0x41200000, 0x00000000, 0x42b40000,
            0x00000000, 0x3fc90fdb, 0x00000000, 0x3f800000, 0xbecccccd, 0x40a00000, 0xbf800000,
            0x40400000, 0x3f000000,
        ],
        Some(true),
        &[],
    ),
    (
        "shell-cyl-below",
        "plane_cyl",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x3f800000, 0x41200000, 0x00000000, 0x42b40000,
            0x00000000, 0x3fc90fdb, 0x00000000, 0x3f800000, 0x40400000, 0x40400000, 0xc0a00000,
            0x40400000, 0x3f000000,
        ],
        Some(false),
        &[],
    ),
    (
        "shell-box-near",
        "plane_box",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x3f800000, 0x41200000, 0x00000000, 0x42b40000,
            0x00000000, 0x3fc90fdb, 0x00000000, 0x3f800000, 0x40000000, 0x40000000, 0xbf800000,
            0x40400000, 0x40400000, 0x3f800000,
        ],
        Some(true),
        &[],
    ),
    (
        "shell-box-far",
        "plane_box",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x3f800000, 0x41200000, 0x00000000, 0x42b40000,
            0x00000000, 0x3fc90fdb, 0x00000000, 0x3f800000, 0x41a00000, 0x41a00000, 0xbf800000,
            0x41a80000, 0x41a80000, 0x3f800000,
        ],
        Some(false),
        &[],
    ),
    (
        "shell-box-opposite",
        "plane_box",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x3f800000, 0x41200000, 0x00000000, 0x42b40000,
            0x00000000, 0x3fc90fdb, 0x00000000, 0x3f800000, 0xc0400000, 0xc0400000, 0xbf800000,
            0xc0000000, 0xc0000000, 0x3f800000,
        ],
        Some(true),
        &[],
    ),
    (
        "sphere-cyl-beside",
        "sphere_cyl",
        &[
            0x3fb33333, 0x00000000, 0x3f800000, 0x3f000000, 0x00000000, 0x00000000, 0x00000000,
            0x40000000, 0x3f800000, 0x00000000,
        ],
        Some(true),
        &[],
    ),
    (
        "sphere-cyl-beside-out",
        "sphere_cyl",
        &[
            0x3fcccccd, 0x00000000, 0x3f800000, 0x3f000000, 0x00000000, 0x00000000, 0x00000000,
            0x40000000, 0x3f800000, 0x00000000,
        ],
        Some(false),
        &[],
    ),
    (
        "sphere-cyl-above",
        "sphere_cyl",
        &[
            0x00000000, 0x00000000, 0x4059999a, 0x3f000000, 0x00000000, 0x00000000, 0x00000000,
            0x40000000, 0x3f800000, 0x00000000,
        ],
        Some(true),
        &[],
    ),
    (
        "sphere-cyl-above-out",
        "sphere_cyl",
        &[
            0x00000000, 0x00000000, 0x40666666, 0x3f000000, 0x00000000, 0x00000000, 0x00000000,
            0x40000000, 0x3f800000, 0x00000000,
        ],
        Some(false),
        &[],
    ),
    (
        "sphere-cyl-corner",
        "sphere_cyl",
        &[
            0x3f99999a, 0x00000000, 0x40133333, 0x3f000000, 0x00000000, 0x00000000, 0x00000000,
            0x40000000, 0x3f800000, 0x00000000,
        ],
        Some(true),
        &[],
    ),
    (
        "sphere-cyl-touching",
        "sphere_cyl",
        &[
            0x3fc00000, 0x00000000, 0x3f800000, 0x3f000000, 0x00000000, 0x00000000, 0x00000000,
            0x40000000, 0x3f800000, 0x00000000,
        ],
        Some(false),
        &[],
    ),
    (
        "sphere-cyl-edge-beside",
        "sphere_cyl",
        &[
            0x3fb33333, 0x00000000, 0x3f800000, 0x3f000000, 0x00000000, 0x00000000, 0x00000000,
            0x40000000, 0x3f800000, 0x3f800000,
        ],
        Some(false),
        &[],
    ),
    (
        "sphere-cyl-edge-out",
        "sphere_cyl",
        &[
            0x3fcccccd, 0x00000000, 0x3f800000, 0x3f000000, 0x00000000, 0x00000000, 0x00000000,
            0x40000000, 0x3f800000, 0x3f800000,
        ],
        Some(false),
        &[],
    ),
    (
        "sphere-cyl-edge-inside",
        "sphere_cyl",
        &[
            0x3e4ccccd, 0x3dcccccd, 0x3f800000, 0x3f000000, 0x00000000, 0x00000000, 0x00000000,
            0x40000000, 0x3f800000, 0x3f800000,
        ],
        Some(true),
        &[],
    ),
    (
        "sphere-cyl-edge-above",
        "sphere_cyl",
        &[
            0x00000000, 0x00000000, 0x40133333, 0x3f000000, 0x00000000, 0x00000000, 0x00000000,
            0x40000000, 0x3f800000, 0x3f800000,
        ],
        Some(true),
        &[],
    ),
    (
        "sphere-cyl-edge-diagonal",
        "sphere_cyl",
        &[
            0x3f800000, 0x3f800000, 0x3f800000, 0x3f000000, 0x00000000, 0x00000000, 0x00000000,
            0x40000000, 0x3f800000, 0x3f800000,
        ],
        Some(false),
        &[],
    ),
    (
        "sphere-box-touching",
        "sphere_box",
        &[
            0x40000000, 0x3f000000, 0x3f000000, 0x3f800000, 0x00000000, 0x00000000, 0x00000000,
            0x3f800000, 0x3f800000, 0x3f800000,
        ],
        Some(true),
        &[],
    ),
    (
        "sphere-box-corner-out",
        "sphere_box",
        &[
            0x3fe66666, 0x3fe66666, 0x3fe66666, 0x3f800000, 0x00000000, 0x00000000, 0x00000000,
            0x3f800000, 0x3f800000, 0x3f800000,
        ],
        Some(false),
        &[],
    ),
    (
        "sphere-box-inside",
        "sphere_box",
        &[
            0x3f000000, 0x3f000000, 0x3f000000, 0x3dcccccd, 0x00000000, 0x00000000, 0x00000000,
            0x3f800000, 0x3f800000, 0x3f800000,
        ],
        Some(true),
        &[],
    ),
    (
        "sphere-ray-meets",
        "sphere_ray",
        &[
            0x00000000, 0x00000000, 0x3f800000, 0x3f800000, 0xc0a00000, 0x00000000, 0x3f800000,
            0x3f800000, 0x00000000, 0x00000000, 0x41200000,
        ],
        Some(true),
        &[0x3ecccccd],
    ),
    (
        "sphere-ray-away",
        "sphere_ray",
        &[
            0x00000000, 0x00000000, 0x3f800000, 0x3f800000, 0xc0a00000, 0x00000000, 0x3f800000,
            0xbf800000, 0x00000000, 0x00000000, 0x41200000,
        ],
        Some(false),
        &[0x3f800000],
    ),
    (
        "sphere-ray-inside",
        "sphere_ray",
        &[
            0x00000000, 0x00000000, 0x3f800000, 0x3f800000, 0x00000000, 0x00000000, 0x3f800000,
            0x3f800000, 0x00000000, 0x00000000, 0x41200000,
        ],
        Some(true),
        &[0x00000000],
    ),
    (
        "sphere-ray-short",
        "sphere_ray",
        &[
            0x00000000, 0x00000000, 0x3f800000, 0x3f800000, 0xc0a00000, 0x00000000, 0x3f800000,
            0x3f800000, 0x00000000, 0x00000000, 0x40600000,
        ],
        Some(false),
        &[0x3f800000],
    ),
    (
        "sphere-ray-just-reaches",
        "sphere_ray",
        &[
            0x00000000, 0x00000000, 0x3f800000, 0x3f800000, 0xc0a00000, 0x00000000, 0x3f800000,
            0x3f800000, 0x00000000, 0x00000000, 0x40800000,
        ],
        Some(false),
        &[0x3f800000],
    ),
    (
        "sphere-ray-tangent",
        "sphere_ray",
        &[
            0x00000000, 0x00000000, 0x3f800000, 0x3f800000, 0xc0a00000, 0x3f800000, 0x3f800000,
            0x3f800000, 0x00000000, 0x00000000, 0x41200000,
        ],
        Some(false),
        &[0x3f800000],
    ),
    (
        "sphere-ray-glancing",
        "sphere_ray",
        &[
            0x00000000, 0x00000000, 0x3f800000, 0x3f800000, 0xc0a00000, 0x3f666666, 0x3f800000,
            0x3f800000, 0x00000000, 0x00000000, 0x41200000,
        ],
        Some(true),
        &[0x3ee9aeb2],
    ),
    (
        "sphere-ray-on-surface",
        "sphere_ray",
        &[
            0x00000000, 0x00000000, 0x3f800000, 0x3f800000, 0xbf800000, 0x00000000, 0x3f800000,
            0x3f800000, 0x00000000, 0x00000000, 0x41200000,
        ],
        Some(true),
        &[0x00000000],
    ),
    (
        "sphere-ray-diagonal",
        "sphere_ray",
        &[
            0x00000000, 0x00000000, 0x3f800000, 0x3f800000, 0xc0400000, 0xc0400000, 0x3f800000,
            0x3f3504f4, 0x3f3504f4, 0x00000000, 0x41200000,
        ],
        Some(true),
        &[0x3ea605ee],
    ),
    (
        "sphere-ray-no-length",
        "sphere_ray",
        &[
            0x00000000, 0x00000000, 0x3f800000, 0x3f800000, 0xc0400000, 0x00000000, 0x3f800000,
            0x3f800000, 0x00000000, 0x00000000, 0x00000000,
        ],
        Some(false),
        &[0x3f800000],
    ),
    (
        "cyl-ray-side",
        "cyl_ray",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x40000000, 0x3f800000, 0xc0a00000, 0x00000000,
            0x3f800000, 0x3f800000, 0x00000000, 0x00000000, 0x41200000,
        ],
        Some(true),
        &[0x3ecccccd],
    ),
    (
        "cyl-ray-away",
        "cyl_ray",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x40000000, 0x3f800000, 0xc0a00000, 0x00000000,
            0x3f800000, 0xbf800000, 0x00000000, 0x00000000, 0x41200000,
        ],
        Some(false),
        &[0x3f800000],
    ),
    (
        "cyl-ray-inside",
        "cyl_ray",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x40000000, 0x3f800000, 0x3e4ccccd, 0x00000000,
            0x3f800000, 0x3f800000, 0x00000000, 0x00000000, 0x41200000,
        ],
        Some(true),
        &[0x00000000],
    ),
    (
        "cyl-ray-from-above",
        "cyl_ray",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x40000000, 0x3f800000, 0x3e4ccccd, 0x00000000,
            0x40a00000, 0x00000000, 0x00000000, 0xbf800000, 0x41200000,
        ],
        Some(true),
        &[0x3e99999a],
    ),
    (
        "cyl-ray-from-below",
        "cyl_ray",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x40000000, 0x3f800000, 0x3e4ccccd, 0x00000000,
            0xc0400000, 0x00000000, 0x00000000, 0x3f800000, 0x41200000,
        ],
        Some(true),
        &[0x3e99999a],
    ),
    (
        "cyl-ray-above-going-up",
        "cyl_ray",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x40000000, 0x3f800000, 0x3e4ccccd, 0x00000000,
            0x40a00000, 0x00000000, 0x00000000, 0x3f800000, 0x41200000,
        ],
        Some(false),
        &[0xc0e00000],
    ),
    (
        "cyl-ray-over-top",
        "cyl_ray",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x40000000, 0x3f800000, 0xc0a00000, 0x00000000,
            0x40200000, 0x3f800000, 0x00000000, 0x00000000, 0x41200000,
        ],
        Some(true),
        &[0x3ecccccd],
    ),
    (
        "cyl-ray-down-onto-top",
        "cyl_ray",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x40000000, 0x3f800000, 0xc0400000, 0x00000000,
            0x40800000, 0x3f3504f4, 0x00000000, 0xbf3504f4, 0x41200000,
        ],
        Some(true),
        &[0x3e90d0c2],
    ),
    (
        "cyl-ray-up-into-base",
        "cyl_ray",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x40000000, 0x3f800000, 0xc0400000, 0x00000000,
            0xc0000000, 0x3f3504f4, 0x00000000, 0x3f3504f4, 0x41200000,
        ],
        Some(true),
        &[0x3e90d0c2],
    ),
    (
        "cyl-ray-short",
        "cyl_ray",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x40000000, 0x3f800000, 0xc0a00000, 0x00000000,
            0x3f800000, 0x3f800000, 0x00000000, 0x00000000, 0x40400000,
        ],
        Some(false),
        &[0xc0e00000],
    ),
    (
        "cyl-ray-tangent",
        "cyl_ray",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x40000000, 0x3f800000, 0xc0a00000, 0x3f800000,
            0x3f800000, 0x3f800000, 0x00000000, 0x00000000, 0x41200000,
        ],
        Some(false),
        &[0x3f800000],
    ),
    (
        "cyl-ray-vertical-outside",
        "cyl_ray",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x40000000, 0x3f800000, 0x40000000, 0x00000000,
            0x40a00000, 0x00000000, 0x00000000, 0xbf800000, 0x41200000,
        ],
        Some(false),
        &[0x3f800000],
    ),
    (
        "cyl-ray-no-length-inside",
        "cyl_ray",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x40000000, 0x3f800000, 0x3e4ccccd, 0x00000000,
            0x3f800000, 0x3f800000, 0x00000000, 0x00000000, 0x00000000,
        ],
        Some(true),
        &[0x00000000],
    ),
    (
        "cyl-ray-no-length-outside",
        "cyl_ray",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x40000000, 0x3f800000, 0x40400000, 0x00000000,
            0x3f800000, 0x3f800000, 0x00000000, 0x00000000, 0x00000000,
        ],
        Some(false),
        &[0x00000000],
    ),
    (
        "cyl-ray-nan-direction",
        "cyl_ray",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x40000000, 0x3f800000, 0xc0a00000, 0x00000000,
            0x3f800000, 0x7fc00000, 0x00000000, 0x00000000, 0x41200000,
        ],
        Some(true),
        &[0x7fc00000],
    ),
    (
        "cyl-ray-nan-length",
        "cyl_ray",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x40000000, 0x3f800000, 0xc0a00000, 0x00000000,
            0x3f800000, 0x3f800000, 0x00000000, 0x00000000, 0x7fc00000,
        ],
        Some(true),
        &[0x7fc00000],
    ),
    (
        "cyl-ray-inside-nan-up",
        "cyl_ray",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x40000000, 0x3f800000, 0x3e4ccccd, 0x00000000,
            0x40a00000, 0x00000000, 0x00000000, 0x7fc00000, 0x41200000,
        ],
        Some(false),
        &[0xc0e00000],
    ),
    (
        "cyl-ray-nan-start",
        "cyl_ray",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x40000000, 0x3f800000, 0x7fc00000, 0x00000000,
            0x3f800000, 0x3f800000, 0x00000000, 0x00000000, 0x41200000,
        ],
        Some(true),
        &[0x7fc00000],
    ),
    (
        "sphere-ray-nan-direction",
        "sphere_ray",
        &[
            0x00000000, 0x00000000, 0x3f800000, 0x3f800000, 0xc0a00000, 0x00000000, 0x3f800000,
            0x7fc00000, 0x00000000, 0x00000000, 0x41200000,
        ],
        Some(false),
        &[0x3f800000],
    ),
    (
        "sphere-ray-nan-length",
        "sphere_ray",
        &[
            0x00000000, 0x00000000, 0x3f800000, 0x3f800000, 0xc0a00000, 0x00000000, 0x3f800000,
            0x3f800000, 0x00000000, 0x00000000, 0x7fc00000,
        ],
        Some(false),
        &[0x3f800000],
    ),
    (
        "cone-sphere-nan-center",
        "cone_sphere",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x41200000, 0x00000000, 0x3f800000, 0x00000000,
            0x3f060a92, 0x7fc00000, 0x40a00000, 0x00000000, 0x3dcccccd,
        ],
        Some(false),
        &[],
    ),
    (
        "shell-sphere-nan-center",
        "plane_sphere",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x3f800000, 0x41200000, 0x00000000, 0x42b40000,
            0x00000000, 0x3fc90fdb, 0x00000000, 0x3f800000, 0x7fc00000, 0x40400000, 0x3f800000,
            0x3e4ccccd,
        ],
        Some(false),
        &[],
    ),
    (
        "sphere-cyl-nan",
        "sphere_cyl",
        &[
            0x7fc00000, 0x00000000, 0x3f800000, 0x3f000000, 0x00000000, 0x00000000, 0x00000000,
            0x40000000, 0x3f800000, 0x00000000,
        ],
        Some(false),
        &[],
    ),
    (
        "box-overlap-nan",
        "box_overlap",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x40000000, 0x40000000, 0x40000000, 0x7fc00000,
            0x00000000, 0x00000000, 0x40800000, 0x40000000, 0x40000000, 0x00000000,
        ],
        Some(false),
        &[],
    ),
    (
        "cyl-sphere-beside",
        "cyl_sphere",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x40000000, 0x3f800000, 0x3fb33333, 0x00000000,
            0x3f800000, 0x3f000000,
        ],
        Some(true),
        &[],
    ),
    (
        "cyl-sphere-above-out",
        "cyl_sphere",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x40000000, 0x3f800000, 0x00000000, 0x00000000,
            0x40666666, 0x3f000000,
        ],
        Some(false),
        &[],
    ),
    (
        "cyl-cyl-overlap",
        "cyl_cyl",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x40000000, 0x3f800000, 0x3fc00000, 0x00000000,
            0x3fc00000, 0x3f800000, 0x3f800000,
        ],
        Some(true),
        &[],
    ),
    (
        "cyl-cyl-above",
        "cyl_cyl",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x40000000, 0x3f800000, 0x3fc00000, 0x00000000,
            0x40600000, 0x3f800000, 0x3f800000,
        ],
        Some(false),
        &[],
    ),
    (
        "cyl-cyl-touching",
        "cyl_cyl",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x40000000, 0x3f800000, 0x40000000, 0x00000000,
            0x00000000, 0x40000000, 0x3f800000,
        ],
        Some(true),
        &[],
    ),
    (
        "cyl-cyl-stacked",
        "cyl_cyl",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x40000000, 0x3f800000, 0x00000000, 0x00000000,
            0x40000000, 0x3f800000, 0x3f800000,
        ],
        Some(true),
        &[],
    ),
    (
        "cyl-cyl-apart",
        "cyl_cyl",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x40000000, 0x3f800000, 0x40200000, 0x00000000,
            0x00000000, 0x40000000, 0x3f800000,
        ],
        Some(false),
        &[],
    ),
    (
        "cyl-box-beside",
        "cyl_box",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x40000000, 0x3f800000, 0x3f000000, 0xbf000000,
            0x00000000, 0x40000000, 0x3f000000, 0x3f800000,
        ],
        Some(true),
        &[],
    ),
    (
        "cyl-box-corner-out",
        "cyl_box",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x40000000, 0x3f800000, 0x3f4ccccd, 0x3f4ccccd,
            0x00000000, 0x40000000, 0x40000000, 0x3f800000,
        ],
        Some(false),
        &[],
    ),
    (
        "cyl-box-above",
        "cyl_box",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x40000000, 0x3f800000, 0xbf000000, 0xbf000000,
            0x40200000, 0x3f000000, 0x3f000000, 0x40400000,
        ],
        Some(false),
        &[],
    ),
    (
        "cyl-within-inside",
        "cyl_within",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x40000000, 0x3f800000, 0x3f000000, 0x00000000,
            0x3f800000,
        ],
        Some(true),
        &[],
    ),
    (
        "cyl-within-above",
        "cyl_within",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x40000000, 0x3f800000, 0x3f000000, 0x00000000,
            0x41100000,
        ],
        Some(true),
        &[],
    ),
    (
        "cyl-within-below",
        "cyl_within",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x40000000, 0x3f800000, 0x3f000000, 0x00000000,
            0xc1100000,
        ],
        Some(true),
        &[],
    ),
    (
        "cyl-within-outside",
        "cyl_within",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x40000000, 0x3f800000, 0x3fc00000, 0x00000000,
            0x3f800000,
        ],
        Some(false),
        &[],
    ),
    (
        "cyl-within-on-rim",
        "cyl_within",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x40000000, 0x3f800000, 0x3f800000, 0x00000000,
            0x3f800000,
        ],
        Some(false),
        &[],
    ),
    (
        "box-overlap",
        "box_overlap",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x40000000, 0x40000000, 0x40000000, 0x3f800000,
            0x3f800000, 0x3f800000, 0x40400000, 0x40400000, 0x40400000, 0x00000000,
        ],
        Some(true),
        &[],
    ),
    (
        "box-touching",
        "box_overlap",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x40000000, 0x40000000, 0x40000000, 0x40000000,
            0x00000000, 0x00000000, 0x40800000, 0x40000000, 0x40000000, 0x00000000,
        ],
        Some(true),
        &[],
    ),
    (
        "box-touching-margin",
        "box_overlap",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x40000000, 0x40000000, 0x40000000, 0x40000000,
            0x00000000, 0x00000000, 0x40800000, 0x40000000, 0x40000000, 0x3dcccccd,
        ],
        Some(false),
        &[],
    ),
    (
        "box-apart",
        "box_overlap",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x40000000, 0x40000000, 0x40000000, 0x40200000,
            0x00000000, 0x00000000, 0x40800000, 0x40000000, 0x40000000, 0x00000000,
        ],
        Some(false),
        &[],
    ),
    (
        "box-nested",
        "box_overlap",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x40800000, 0x40800000, 0x40800000, 0x3f800000,
            0x3f800000, 0x3f800000, 0x40000000, 0x40000000, 0x40000000, 0x00000000,
        ],
        Some(true),
        &[],
    ),
    (
        "box-apart-in-z",
        "box_overlap",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x40000000, 0x40000000, 0x40000000, 0x00000000,
            0x00000000, 0x40400000, 0x40000000, 0x40000000, 0x40800000, 0x00000000,
        ],
        Some(false),
        &[],
    ),
    (
        "box-negative-margin",
        "box_overlap",
        &[
            0x00000000, 0x00000000, 0x00000000, 0x40000000, 0x40000000, 0x40000000, 0x40200000,
            0x00000000, 0x00000000, 0x40800000, 0x40000000, 0x40000000, 0xbf800000,
        ],
        Some(true),
        &[],
    ),
    (
        "sphere-bbox-here",
        "sphere_bbox",
        &[
            0x3f800000, 0x40000000, 0x40400000, 0x3f000000, 0x40400001, 0x41200000, 0x41200000,
            0x00000000, 0x3f800000, 0x00000000, 0x00000000, 0x00000000, 0x40400001, 0x41200000,
            0x41200000, 0x00000000, 0x3f800000, 0x00000000, 0x00000000, 0x00000000,
        ],
        None,
        &[
            0x41280000, 0x41380000, 0x40200000, 0x41380000, 0x41480000, 0x40600000,
        ],
    ),
    (
        "sphere-bbox-turned",
        "sphere_bbox",
        &[
            0x3f800000, 0x40000000, 0x40400000, 0x3f000000, 0x40400001, 0x41200000, 0x41200000,
            0x00000000, 0x3f800000, 0x00000000, 0x00000000, 0x00000000, 0x40400001, 0x41200000,
            0x41200000, 0x00000000, 0x3f3504f3, 0x00000000, 0x00000000, 0x3f3504f3,
        ],
        None,
        &[
            0x40f00000, 0x41280000, 0x40200000, 0x41080000, 0x41380000, 0x40600000,
        ],
    ),
    (
        "sphere-bbox-next-block",
        "sphere_bbox",
        &[
            0x3f800000, 0x40000000, 0x40400000, 0x3f000000, 0x41400001, 0x00000000, 0x00000000,
            0x00000000, 0x3f800000, 0x00000000, 0x00000000, 0x00000000, 0x40400001, 0x41200000,
            0x41200000, 0x00000000, 0x3f3504f3, 0x00000000, 0x00000000, 0x3f3504f3,
        ],
        None,
        &[
            0xc3188000, 0x41280000, 0x40200000, 0xc3178000, 0x41380000, 0x40600000,
        ],
    ),
    (
        "cyl-bbox-upright",
        "cyl_bbox",
        &[
            0x3f000000, 0x00000000, 0xbf800000, 0x40000000, 0x3f19999a, 0x40400001, 0x41200000,
            0x41200000, 0x00000000, 0x3f800000, 0x00000000, 0x00000000, 0x00000000, 0x40400001,
            0x41200000, 0x41200000, 0x00000000, 0x3f3504f3, 0x00000000, 0x00000000, 0x3f3504f3,
        ],
        None,
        &[
            0x41166666, 0x411e6666, 0xbf800000, 0x4129999a, 0x4131999a, 0x3f800000,
        ],
    ),
    (
        "cyl-bbox-next-block",
        "cyl_bbox",
        &[
            0x3f000000, 0x00000000, 0xbf800000, 0x40000000, 0x3f19999a, 0x41400001, 0x00000000,
            0x00000000, 0x00000000, 0x3f800000, 0x00000000, 0x00000000, 0x00000000, 0x40400001,
            0x41200000, 0x41200000, 0x00000000, 0x3f800000, 0x00000000, 0x00000000, 0x00000000,
        ],
        None,
        &[
            0xc316199a, 0x41166666, 0xbf800000, 0xc314e666, 0x4129999a, 0x3f800000,
        ],
    ),
    (
        "cyl-bbox-on-its-side-about-x",
        "cyl_bbox",
        &[
            0x3f000000, 0x00000000, 0xbf800000, 0x40000000, 0x3f19999a, 0x40400001, 0x41200000,
            0x41200000, 0x00000000, 0x3f800000, 0x00000000, 0x00000000, 0x00000000, 0x40400001,
            0x41200000, 0x41200000, 0x40000000, 0x3f3504f3, 0x3f3504f3, 0x00000000, 0x00000000,
        ],
        None,
        &[
            0x411e6666, 0x41066666, 0x40000000, 0x4131999a, 0x4139999a, 0x40000000,
        ],
    ),
    (
        "cyl-bbox-on-its-side-about-y",
        "cyl_bbox",
        &[
            0x3f000000, 0x00000000, 0xbf800000, 0x40000000, 0x3f19999a, 0x40400001, 0x41200000,
            0x41200000, 0x00000000, 0x3f800000, 0x00000000, 0x00000000, 0x00000000, 0x40400001,
            0x41200000, 0x41200000, 0x40000000, 0x3f3504f3, 0x00000000, 0x3f3504f3, 0x00000000,
        ],
        None,
        &[
            0x41100000, 0x41166666, 0x3f666667, 0x41300000, 0x4129999a, 0x40066667,
        ],
    ),
    (
        "shell-bbox-turned",
        "plane_bbox",
        &[
            0x3f800000, 0x00000000, 0x3f000000, 0x3f800000, 0x40800000, 0x41200000, 0x42a00000,
            0xc1a00000, 0x41f00000, 0x40400001, 0x41200000, 0x41200000, 0x00000000, 0x3f800000,
            0x00000000, 0x00000000, 0x00000000, 0x40400001, 0x41200000, 0x41200000, 0x00000000,
            0x3f3504f3, 0x00000000, 0x00000000, 0x3f3504f3,
        ],
        None,
        &[
            0x40c00000, 0x40e00000, 0xc0600000, 0x41600000, 0x41700000, 0x40900000,
        ],
    ),
    (
        "random-cone-angle-0",
        "cone_angle",
        &[
            0x3fca1a17, 0x3f4a5874, 0x3e0d3a6a, 0x408afe08, 0xbccb4d63, 0xbdc08fa7, 0xbf7ec966,
            0x3ee24b47, 0xbe12171a, 0x3eaf9843, 0xbfb2a346,
        ],
        Some(true),
        &[],
    ),
    (
        "random-cone-sphere-0",
        "cone_sphere",
        &[
            0x3fca1a17, 0x3f4a5874, 0x3e0d3a6a, 0x408afe08, 0xbccb4d63, 0xbdc08fa7, 0xbf7ec966,
            0x3ee24b47, 0x3f9b6df8, 0xbe82ea9e, 0x3e591ee9, 0x3f751293,
        ],
        Some(false),
        &[],
    ),
    (
        "random-cone-cyl-0",
        "cone_cyl",
        &[
            0x3fca1a17, 0x3f4a5874, 0x3e0d3a6a, 0x408afe08, 0xbccb4d63, 0xbdc08fa7, 0xbf7ec966,
            0x3ee24b47, 0x404e41d7, 0x3f9859d1, 0xbfabed54, 0x402cb94c, 0x3f2e35fa,
        ],
        Some(false),
        &[],
    ),
    (
        "random-cone-box-0",
        "cone_box",
        &[
            0x3fca1a17, 0x3f4a5874, 0x3e0d3a6a, 0x408afe08, 0xbccb4d63, 0xbdc08fa7, 0xbf7ec966,
            0x3ee24b47, 0x40307be3, 0xbe95530f, 0xc0199a7e, 0x40c3bee0, 0x3f82a593, 0x3e7d03d0,
        ],
        Some(false),
        &[],
    ),
    (
        "random-shell-heights-0",
        "plane_cone_new",
        &[
            0x3f9e45c0, 0x3fdf3669, 0xbfb75fdb, 0x3fc94373, 0x40804dcd, 0x4239de65, 0x429ebadc,
            0x3d2d2314, 0x3f0a9ebf,
        ],
        None,
        &[0x3d2d15e1, 0x3f03f1d7],
    ),
    (
        "random-shell-angle-0",
        "plane_angle",
        &[
            0x3f9e45c0, 0x3fdf3669, 0xbfb75fdb, 0x3fc94373, 0x40804dcd, 0x4239de65, 0x429ebadc,
            0x3d2d2314, 0x3f0a9ebf, 0xbf4b9e6f, 0x3ee93574, 0xbeccbb50,
        ],
        Some(false),
        &[],
    ),
    (
        "random-shell-sphere-0",
        "plane_sphere",
        &[
            0x3f9e45c0, 0x3fdf3669, 0xbfb75fdb, 0x3fc94373, 0x40804dcd, 0x4239de65, 0x429ebadc,
            0x3d2d2314, 0x3f0a9ebf, 0x40e89e22, 0x40c91061, 0xc113b6ee, 0x3fedd0dd,
        ],
        Some(false),
        &[],
    ),
    (
        "random-shell-cyl-0",
        "plane_cyl",
        &[
            0x3f9e45c0, 0x3fdf3669, 0xbfb75fdb, 0x3fc94373, 0x40804dcd, 0x4239de65, 0x429ebadc,
            0x3d2d2314, 0x3f0a9ebf, 0x3fb5b72d, 0xbfd2f19f, 0xbfc95f83, 0x4092a550, 0x3fc9ab13,
        ],
        Some(false),
        &[],
    ),
    (
        "random-sphere-cyl-0",
        "sphere_cyl",
        &[
            0xbf9622e3, 0xbfb7466a, 0x3f583eed, 0x3f9cc365, 0x3e0fb4e4, 0x3f34e40c, 0x3fb4a6f8,
            0x40b356b1, 0x3feb9765, 0x00000000,
        ],
        Some(true),
        &[],
    ),
    (
        "random-sphere-box-0",
        "sphere_box",
        &[
            0x40dcc341, 0x3fcd2774, 0x3f8d1269, 0x401d8337, 0x4117910e, 0x3ef90cf6, 0x3f06b77d,
            0x412b1e34, 0x4035b71a, 0x3fbea9e2,
        ],
        Some(false),
        &[],
    ),
    (
        "random-sphere-ray-0",
        "sphere_ray",
        &[
            0x40dcc341, 0x3fcd2774, 0x3f8d1269, 0x401d8337, 0x4152d22e, 0xbe6c9a30, 0x3d70ca36,
            0xbf6b2536, 0x3eac4f43, 0x3e546a4b, 0x413a0f4a,
        ],
        Some(true),
        &[0x3eb8f494],
    ),
    (
        "random-cyl-ray-0",
        "cyl_ray",
        &[
            0x3e0fb4e4, 0x3f34e40c, 0x3fb4a6f8, 0x40b356b1, 0x3feb9765, 0xbe823c97, 0xc09b2fc0,
            0xc09f507a, 0x3e192f6d, 0x3f299fde, 0x3f3bdff1, 0x3f486d53,
        ],
        Some(false),
        &[0xc0e00000],
    ),
    (
        "random-cyl-cyl-0",
        "cyl_cyl",
        &[
            0x3e0fb4e4, 0x3f34e40c, 0x3fb4a6f8, 0x40b356b1, 0x3feb9765, 0xbebed4a8, 0x40611fae,
            0x3fd2962e, 0x3fed8e23, 0x3fdc5316,
        ],
        Some(true),
        &[],
    ),
    (
        "random-cyl-box-0",
        "cyl_box",
        &[
            0x3e0fb4e4, 0x3f34e40c, 0x3fb4a6f8, 0x40b356b1, 0x3feb9765, 0x3f586804, 0xbf2517d5,
            0x40277918, 0x4035551c, 0x3f3b3d01, 0x40a476ea,
        ],
        Some(true),
        &[],
    ),
    (
        "random-box-overlap-0",
        "box_overlap",
        &[
            0x3fb812ab, 0xc072e3d5, 0xbfe9ba2d, 0x407ce9c8, 0xbf6f0430, 0xbe1fd858, 0xc0154571,
            0x404ceccb, 0xc026d989, 0xbf463e28, 0x409e3f66, 0xbfb91795, 0x3ec0e593,
        ],
        Some(false),
        &[],
    ),
    (
        "random-sphere-bbox-0",
        "sphere_bbox",
        &[
            0x40dcc341, 0x3fcd2774, 0x3f8d1269, 0x401d8337, 0x40400001, 0x4213bf03, 0x4310b9f8,
            0x4189fd30, 0xbe70b37c, 0xbf0f7a38, 0xbf3055a0, 0xbeca56ea, 0x40400001, 0x42980644,
            0x4264759a, 0x42c1aac2, 0xbf0a11f6, 0xbf086cdc, 0x3efe76bd, 0x3ed814eb,
        ],
        None,
        &[
            0x4292c1ae, 0x423f3c63, 0x42c0fc6b, 0x429c99e2, 0x4252ecc9, 0x42cad49f,
        ],
    ),
    (
        "random-cyl-bbox-0",
        "cyl_bbox",
        &[
            0x3e0fb4e4, 0x3f34e40c, 0x3fb4a6f8, 0x40b356b1, 0x3feb9765, 0x41400001, 0x4288df71,
            0x429e1952, 0x42a83a1e, 0x3e43800f, 0x3f4eea96, 0x3e93beb9, 0xbef3ee96, 0x41400001,
            0x42b0c130, 0x42d3b4eb, 0x42b5e959, 0xbf0f15f1, 0xbf0ae5e3, 0x3f1eb094, 0x3dc1e363,
        ],
        None,
        &[
            0x42a1f9f9, 0x42c8ddfd, 0x42ad3ff5, 0x42b0a3ed, 0x42d6fa13, 0x42baedd5,
        ],
    ),
    (
        "random-cone-angle-1",
        "cone_angle",
        &[
            0x3f0f3d4b, 0x3df4bc46, 0x3d7a5bf8, 0x40c09276, 0xbf7c34a7, 0xbe2d44f4, 0xbce68b90,
            0x3d9d86fb, 0xbfb47985, 0xbe8be94c, 0xbe535440,
        ],
        Some(false),
        &[],
    ),
    (
        "random-cone-sphere-1",
        "cone_sphere",
        &[
            0x3f0f3d4b, 0x3df4bc46, 0x3d7a5bf8, 0x40c09276, 0xbf7c34a7, 0xbe2d44f4, 0xbce68b90,
            0x3d9d86fb, 0xc0884092, 0x3f8f4229, 0x3f51a450, 0x40248312,
        ],
        Some(true),
        &[],
    ),
    (
        "random-cone-cyl-1",
        "cone_cyl",
        &[
            0x3f0f3d4b, 0x3df4bc46, 0x3d7a5bf8, 0x40c09276, 0xbf7c34a7, 0xbe2d44f4, 0xbce68b90,
            0x3d9d86fb, 0xc0942a29, 0xbfb5eb48, 0xbfa6af0c, 0x3ef055a4, 0x3f271738,
        ],
        Some(false),
        &[],
    ),
    (
        "random-cone-box-1",
        "cone_box",
        &[
            0x3f0f3d4b, 0x3df4bc46, 0x3d7a5bf8, 0x40c09276, 0xbf7c34a7, 0xbe2d44f4, 0xbce68b90,
            0x3d9d86fb, 0xc00bf9f5, 0xc0619bd9, 0xbf33a6ef, 0xbfa278f5, 0x3ee28448, 0x3eca3ce2,
        ],
        Some(false),
        &[],
    ),
    (
        "random-shell-heights-1",
        "plane_cone_new",
        &[
            0x3f5e6cae, 0xbffaa08f, 0x3f24027f, 0x3fb32963, 0x411979fa, 0x4355bc1f, 0x419fbae4,
            0x3f43f743, 0x3f8bf8b5,
        ],
        None,
        &[0x3f316158, 0x3f636489],
    ),
    (
        "random-shell-angle-1",
        "plane_angle",
        &[
            0x3f5e6cae, 0xbffaa08f, 0x3f24027f, 0x3fb32963, 0x411979fa, 0x4355bc1f, 0x419fbae4,
            0x3f43f743, 0x3f8bf8b5, 0xbf647b15, 0xbd0506cf, 0x3ee6556e,
        ],
        Some(false),
        &[],
    ),
    (
        "random-shell-sphere-1",
        "plane_sphere",
        &[
            0x3f5e6cae, 0xbffaa08f, 0x3f24027f, 0x3fb32963, 0x411979fa, 0x4355bc1f, 0x419fbae4,
            0x3f43f743, 0x3f8bf8b5, 0xc06e4628, 0xc0ee87c3, 0xc068c49f, 0x3fbbbfa6,
        ],
        Some(false),
        &[],
    ),
    (
        "random-shell-cyl-1",
        "plane_cyl",
        &[
            0x3f5e6cae, 0xbffaa08f, 0x3f24027f, 0x3fb32963, 0x411979fa, 0x4355bc1f, 0x419fbae4,
            0x3f43f743, 0x3f8bf8b5, 0xc01a8395, 0xc084e738, 0x3fd133c1, 0x409e5d80, 0x3fd775d7,
        ],
        Some(true),
        &[],
    ),
    (
        "random-sphere-cyl-1",
        "sphere_cyl",
        &[
            0x40ccb261, 0x3c4ec0e4, 0x3f7e1e5b, 0x3fdd0b1a, 0x408f594d, 0x3f5ffdab, 0x3f7777cd,
            0x409e609a, 0x3fc13230, 0x3f800000,
        ],
        Some(false),
        &[],
    ),
    (
        "random-sphere-box-1",
        "sphere_box",
        &[
            0xc0f256cc, 0xc08c8bc7, 0x3e87a568, 0x3f69388e, 0xc11d01c6, 0xc0c91beb, 0x4024ac61,
            0xc1146779, 0xc0a65511, 0x40872a51,
        ],
        Some(false),
        &[],
    ),
    (
        "random-sphere-ray-1",
        "sphere_ray",
        &[
            0xc0f256cc, 0xc08c8bc7, 0x3e87a568, 0x3f69388e, 0xc0c55865, 0xc0f3b1c0, 0xc0eb284f,
            0xbe4502ba, 0x3ec74bfd, 0x3f669bd3, 0x417b2997,
        ],
        Some(true),
        &[0x3ef49b30],
    ),
    (
        "random-cyl-ray-1",
        "cyl_ray",
        &[
            0x408f594d, 0x3f5ffdab, 0x3f7777cd, 0x409e609a, 0x3fc13230, 0x3fc77481, 0xc0d4e6e3,
            0xc0ad38be, 0x3e723b10, 0x3f345eaf, 0x3f2b46aa, 0x40a9d280,
        ],
        Some(false),
        &[0xc0e00000],
    ),
    (
        "random-cyl-cyl-1",
        "cyl_cyl",
        &[
            0x408f594d, 0x3f5ffdab, 0x3f7777cd, 0x409e609a, 0x3fc13230, 0x40e5312a, 0x3fe795fe,
            0x3f176fa3, 0x4000568c, 0x3dd542e9,
        ],
        Some(false),
        &[],
    ),
    (
        "random-cyl-box-1",
        "cyl_box",
        &[
            0x408f594d, 0x3f5ffdab, 0x3f7777cd, 0x409e609a, 0x3fc13230, 0x4054a6b5, 0xbe136822,
            0x404149b6, 0x40b5333c, 0x402da147, 0x40de6da0,
        ],
        Some(true),
        &[],
    ),
    (
        "random-box-overlap-1",
        "box_overlap",
        &[
            0x40865f87, 0x4037dead, 0x401906ee, 0x40aef0e4, 0x40992400, 0x408de9a2, 0x40807d98,
            0xc0ad0896, 0x4090e8f8, 0x40dc6f4b, 0xc077ec66, 0x40d46019, 0x3e8acb23,
        ],
        Some(false),
        &[],
    ),
    (
        "random-sphere-bbox-1",
        "sphere_bbox",
        &[
            0xc0f256cc, 0xc08c8bc7, 0x3e87a568, 0x3f69388e, 0x40400001, 0x4182b3fe, 0x42cff4ca,
            0x4137bd88, 0xbf1352aa, 0x3f095774, 0x3e727000, 0x3f11ee3e, 0x40400001, 0x42de8b9c,
            0x42b18e6b, 0x41f8d8c4, 0x3ee3a9f4, 0xbf1c3bd4, 0xbf12d51e, 0x3ea29922,
        ],
        None,
        &[
            0x42d6740a, 0x42a07a8b, 0x420493f7, 0x42da18ec, 0x42a41f6d, 0x420bddbb,
        ],
    ),
    (
        "random-cyl-bbox-1",
        "cyl_bbox",
        &[
            0x408f594d, 0x3f5ffdab, 0x3f7777cd, 0x409e609a, 0x3fc13230, 0x40400001, 0x4259c075,
            0x42d327be, 0x427ee24b, 0x3da95acd, 0x3f6967d5, 0x3ecccdd8, 0xbd31d119, 0x41400001,
            0x42e9ea8f, 0x4311a986, 0x428e4ae2, 0x3e903b80, 0x3f16f1a3, 0x3ecd3d46, 0x3f2460fa,
        ],
        None,
        &[
            0x438a7b5e, 0x431313e6, 0x42902d50, 0x438d4d68, 0x431811e4, 0x4298a9ec,
        ],
    ),
    (
        "random-cone-angle-2",
        "cone_angle",
        &[
            0x3f26513a, 0xbe3e9a2c, 0x3f9fdbf3, 0x41004af7, 0x3c786b43, 0x3e34f6b0, 0xbf7bf0dc,
            0x3f8356ce, 0x3e862069, 0x3ef8c1b6, 0xbf97504a,
        ],
        Some(true),
        &[],
    ),
    (
        "random-cone-sphere-2",
        "cone_sphere",
        &[
            0x3f26513a, 0xbe3e9a2c, 0x3f9fdbf3, 0x41004af7, 0x3c786b43, 0x3e34f6b0, 0xbf7bf0dc,
            0x3f8356ce, 0x3fd6c23f, 0x3ff41d87, 0xbfd58d8c, 0x400706d1,
        ],
        Some(true),
        &[],
    ),
    (
        "random-cone-cyl-2",
        "cone_cyl",
        &[
            0x3f26513a, 0xbe3e9a2c, 0x3f9fdbf3, 0x41004af7, 0x3c786b43, 0x3e34f6b0, 0xbf7bf0dc,
            0x3f8356ce, 0x3fac5550, 0x400919cf, 0xc04c0a46, 0x4075a30d, 0x3f9eb0d6,
        ],
        Some(true),
        &[],
    ),
    (
        "random-cone-box-2",
        "cone_box",
        &[
            0x3f26513a, 0xbe3e9a2c, 0x3f9fdbf3, 0x41004af7, 0x3c786b43, 0x3e34f6b0, 0xbf7bf0dc,
            0x3f8356ce, 0xc009224d, 0xbc55b269, 0xc020d3f1, 0xbf84a2e7, 0x40632e5c, 0x3fb8089c,
        ],
        Some(false),
        &[],
    ),
    (
        "random-shell-heights-2",
        "plane_cone_new",
        &[
            0xbf23ca4b, 0xbfc118cd, 0x3f9128f7, 0x3feca4f2, 0x40a21c69, 0x43abee34, 0x4390772a,
            0x3ee1f37d, 0x3ee43d8d,
        ],
        None,
        &[0x3edab022, 0x3edcc17c],
    ),
    (
        "random-shell-angle-2",
        "plane_angle",
        &[
            0xbf23ca4b, 0xbfc118cd, 0x3f9128f7, 0x3feca4f2, 0x40a21c69, 0x43abee34, 0x4390772a,
            0x3ee1f37d, 0x3ee43d8d, 0xbf5dacbb, 0x3bf41bad, 0xbf0008ae,
        ],
        Some(false),
        &[],
    ),
    (
        "random-shell-sphere-2",
        "plane_sphere",
        &[
            0xbf23ca4b, 0xbfc118cd, 0x3f9128f7, 0x3feca4f2, 0x40a21c69, 0x43abee34, 0x4390772a,
            0x3ee1f37d, 0x3ee43d8d, 0xbebd5276, 0xbf50b53f, 0x4105a17b, 0x3ff2af1b,
        ],
        Some(false),
        &[],
    ),
    (
        "random-shell-cyl-2",
        "plane_cyl",
        &[
            0xbf23ca4b, 0xbfc118cd, 0x3f9128f7, 0x3feca4f2, 0x40a21c69, 0x43abee34, 0x4390772a,
            0x3ee1f37d, 0x3ee43d8d, 0x408bc47b, 0x4037bc86, 0x40bb0c9c, 0x40afde4d, 0x3f1dc4d4,
        ],
        Some(false),
        &[],
    ),
    (
        "random-sphere-cyl-2",
        "sphere_cyl",
        &[
            0xc00f6ec7, 0xc00b752a, 0xc09e69ef, 0x3f8423dc, 0xbf5da9dc, 0xbde1a4f6, 0xc098828f,
            0x40123465, 0x3ec89cbd, 0x00000000,
        ],
        Some(false),
        &[],
    ),
    (
        "random-sphere-box-2",
        "sphere_box",
        &[
            0xc0c89f17, 0x40b98027, 0x404e70b4, 0x3ffc20a4, 0xc0de690a, 0x404af622, 0x40b10413,
            0xc09e3d5c, 0x40a498aa, 0x410d1d95,
        ],
        Some(false),
        &[],
    ),
    (
        "random-sphere-ray-2",
        "sphere_ray",
        &[
            0xc0c89f17, 0x40b98027, 0x404e70b4, 0x3ffc20a4, 0xc0f43658, 0x40017725, 0x4086da13,
            0x3ea96ac9, 0x3f649cd3, 0xbe9c29cc, 0x419ce308,
        ],
        Some(true),
        &[0x3de2f869],
    ),
    (
        "random-cyl-ray-2",
        "cyl_ray",
        &[
            0xbf5da9dc, 0xbde1a4f6, 0xc098828f, 0x40123465, 0x3ec89cbd, 0xc00955a7, 0x40987c8d,
            0xc007903e, 0x3db1b113, 0xbf5f96ea, 0xbef55c29, 0x3fb2375a,
        ],
        Some(false),
        &[0xc0e00000],
    ),
    (
        "random-cyl-cyl-2",
        "cyl_cyl",
        &[
            0xbf5da9dc, 0xbde1a4f6, 0xc098828f, 0x40123465, 0x3ec89cbd, 0xc07384cb, 0xc03166c7,
            0xc0843255, 0x4061d680, 0x3fe94762,
        ],
        Some(false),
        &[],
    ),
    (
        "random-cyl-box-2",
        "cyl_box",
        &[
            0xbf5da9dc, 0xbde1a4f6, 0xc098828f, 0x40123465, 0x3ec89cbd, 0xbf81c079, 0x40100554,
            0xc0e70267, 0x3f2cf308, 0x40b2a740, 0xc08b7fbe,
        ],
        Some(false),
        &[],
    ),
    (
        "random-box-overlap-2",
        "box_overlap",
        &[
            0xc097bbbb, 0xc0339e82, 0xc08bc7a5, 0xc092c2ce, 0xbf222c04, 0xc015d9ab, 0xc0203210,
            0x40977b25, 0x40583bb2, 0xbfb07eea, 0x40cb6a90, 0x40c00482, 0xbedc9b93,
        ],
        Some(false),
        &[],
    ),
    (
        "random-sphere-bbox-2",
        "sphere_bbox",
        &[
            0xc0c89f17, 0x40b98027, 0x404e70b4, 0x3ffc20a4, 0x41400001, 0x42feb7c7, 0x42618f96,
            0x43065545, 0xbe19c4ab, 0x3f3d37e5, 0xbe85c013, 0xbf1a371b, 0x40400001, 0x413802ec,
            0x43013d44, 0x414558aa, 0xbef5d2b2, 0x3f07b69c, 0xbf147e42, 0x3ec79a98,
        ],
        None,
        &[
            0xc314e107, 0x43066e97, 0x409de5c1, 0xc310f085, 0x430a5f19, 0x410dfb0a,
        ],
    ),
    (
        "random-cyl-bbox-2",
        "cyl_bbox",
        &[
            0xbf5da9dc, 0xbde1a4f6, 0xc098828f, 0x40123465, 0x3ec89cbd, 0x40400001, 0x431119fe,
            0x4221180c, 0x421c7024, 0x3f201bab, 0xbf0e874e, 0x3f0a5e6e, 0xbda803f6, 0x40400001,
            0x423ef57f, 0x43019399, 0x43099b2d, 0xbe1f4082, 0xbf3fd042, 0x3f247f6d, 0x3d1f30f3,
        ],
        None,
        &[
            0x423f96a9, 0x4302752b, 0x430ba3d2, 0x424576cd, 0x4303bd18, 0x430e23c5,
        ],
    ),
    (
        "random-cone-angle-3",
        "cone_angle",
        &[
            0xbf213083, 0xbfd6b64f, 0xbfa12139, 0x4065dd81, 0x3d46c744, 0x3e2125f2, 0x3f7c8132,
            0x3f581a75, 0x3ebcc3d2, 0x3f362868, 0x3fb476e2,
        ],
        Some(true),
        &[],
    ),
    (
        "random-cone-sphere-3",
        "cone_sphere",
        &[
            0xbf213083, 0xbfd6b64f, 0xbfa12139, 0x4065dd81, 0x3d46c744, 0x3e2125f2, 0x3f7c8132,
            0x3f581a75, 0xc04ac44f, 0xbebaf3cc, 0xbf9db48c, 0x402d8e71,
        ],
        Some(true),
        &[],
    ),
    (
        "random-cone-cyl-3",
        "cone_cyl",
        &[
            0xbf213083, 0xbfd6b64f, 0xbfa12139, 0x4065dd81, 0x3d46c744, 0x3e2125f2, 0x3f7c8132,
            0x3f581a75, 0xc02a330e, 0xc069a867, 0x3f9df56c, 0x3fe8b71c, 0x3e66c1e2,
        ],
        Some(false),
        &[],
    ),
    (
        "random-cone-box-3",
        "cone_box",
        &[
            0xbf213083, 0xbfd6b64f, 0xbfa12139, 0x4065dd81, 0x3d46c744, 0x3e2125f2, 0x3f7c8132,
            0x3f581a75, 0x4004fd23, 0x3e13485d, 0x4060a682, 0x4058edb6, 0x4068af2d, 0x40d914a3,
        ],
        Some(false),
        &[],
    ),
    (
        "random-shell-heights-3",
        "plane_cone_new",
        &[
            0xbfae3f61, 0x3f9c924d, 0xbf04fec0, 0x3e161bb5, 0x411fc99d, 0x438bbcde, 0x43aa7d13,
            0xbe03b30d, 0x3d9af57f,
        ],
        None,
        &[0xbe03562e, 0x3d9acfa8],
    ),
    (
        "random-shell-angle-3",
        "plane_angle",
        &[
            0xbfae3f61, 0x3f9c924d, 0xbf04fec0, 0x3e161bb5, 0x411fc99d, 0x438bbcde, 0x43aa7d13,
            0xbe03b30d, 0x3d9af57f, 0xbe2930fe, 0x3f04ccc0, 0xbf56bc27,
        ],
        Some(false),
        &[],
    ),
    (
        "random-shell-sphere-3",
        "plane_sphere",
        &[
            0xbfae3f61, 0x3f9c924d, 0xbf04fec0, 0x3e161bb5, 0x411fc99d, 0x438bbcde, 0x43aa7d13,
            0xbe03b30d, 0x3d9af57f, 0x3fac8479, 0xc00ac203, 0xbfd04cae, 0x3fd768df,
        ],
        Some(false),
        &[],
    ),
    (
        "random-shell-cyl-3",
        "plane_cyl",
        &[
            0xbfae3f61, 0x3f9c924d, 0xbf04fec0, 0x3e161bb5, 0x411fc99d, 0x438bbcde, 0x43aa7d13,
            0xbe03b30d, 0x3d9af57f, 0xc09b2a1d, 0x40bf1da0, 0x408756b6, 0x400719ed, 0x3f9746fc,
        ],
        Some(false),
        &[],
    ),
    (
        "random-sphere-cyl-3",
        "sphere_cyl",
        &[
            0x3f3ddcf9, 0x402a314b, 0x4081b2bc, 0x3ff91e29, 0xbebd3a9c, 0x4031c991, 0x40518fe3,
            0x40b0127b, 0x3f3ba18e, 0x3f800000,
        ],
        Some(true),
        &[],
    ),
    (
        "random-sphere-box-3",
        "sphere_box",
        &[
            0xc04fe28c, 0xc0ed4d05, 0xc0316790, 0x3fd02558, 0xc0abe880, 0xc0c2329f, 0xc019ff89,
            0xc0906fe2, 0xc0b02b72, 0x3f441248,
        ],
        Some(false),
        &[],
    ),
    (
        "random-sphere-ray-3",
        "sphere_ray",
        &[
            0xc04fe28c, 0xc0ed4d05, 0xc0316790, 0x3fd02558, 0x3fbdfcea, 0xc149d19e, 0xc061b597,
            0xbf28a400, 0x3f409932, 0xbbbd49e2, 0x401884da,
        ],
        Some(false),
        &[0x3f800000],
    ),
    (
        "random-cyl-ray-3",
        "cyl_ray",
        &[
            0xbebd3a9c, 0x4031c991, 0x40518fe3, 0x40b0127b, 0x3f3ba18e, 0xc014dad5, 0x40ff8259,
            0x40919762, 0x3ead2090, 0xbf6bc0ef, 0xbe468346, 0x4171585b,
        ],
        Some(true),
        &[0x3ea71f67],
    ),
    (
        "random-cyl-cyl-3",
        "cyl_cyl",
        &[
            0xbebd3a9c, 0x4031c991, 0x40518fe3, 0x40b0127b, 0x3f3ba18e, 0x3fe512ba, 0x3e40bc5c,
            0x400f0785, 0x406f34a1, 0x3ee1a42a,
        ],
        Some(false),
        &[],
    ),
    (
        "random-cyl-box-3",
        "cyl_box",
        &[
            0xbebd3a9c, 0x4031c991, 0x40518fe3, 0x40b0127b, 0x3f3ba18e, 0xbfe467a5, 0x3fa23aa2,
            0x40813580, 0x3ed8ff3c, 0x408f3156, 0x40b49fa6,
        ],
        Some(true),
        &[],
    ),
    (
        "random-box-overlap-3",
        "box_overlap",
        &[
            0xc087c619, 0x4046f3ef, 0x4055bc6b, 0xbf898616, 0x406c1c00, 0x40a9abda, 0x40959e85,
            0x40b1de4f, 0x3fbeb4b2, 0x40dd72c8, 0x410e49aa, 0x409627b6, 0x3e10547a,
        ],
        Some(false),
        &[],
    ),
    (
        "random-sphere-bbox-3",
        "sphere_bbox",
        &[
            0xc04fe28c, 0xc0ed4d05, 0xc0316790, 0x3fd02558, 0x41400001, 0x4125282c, 0x42c00eb4,
            0x42fd2127, 0xbf131be3, 0xbf133aa1, 0xbf0b14e7, 0xbe5678a5, 0x41400001, 0x42d7b3b4,
            0x4305dab8, 0x42a47500, 0x3ca23b2f, 0x3eac3b95, 0x3f329dd9, 0x3f21d59d,
        ],
        None,
        &[
            0x42d05e74, 0x4300674e, 0x429272e0, 0x42d6df9e, 0x4303a7e4, 0x4298f40a,
        ],
    ),
    (
        "random-cyl-bbox-3",
        "cyl_bbox",
        &[
            0xbebd3a9c, 0x4031c991, 0x40518fe3, 0x40b0127b, 0x3f3ba18e, 0x40400001, 0x417bb92c,
            0x4238aa48, 0x4304be8e, 0x3e876b40, 0x3f6562c7, 0x3e8c71d2, 0xbe6951e8, 0x41400001,
            0x40d3b746, 0x4195fd82, 0x4188d3f5, 0x3ee6937b, 0xbf264c83, 0x3f0fbe8b, 0xbe7aa920,
        ],
        None,
        &[
            0x4327404b, 0x419bf252, 0x411d9e2c, 0x432ce550, 0x41b52a62, 0x415ff793,
        ],
    ),
    (
        "random-cone-angle-4",
        "cone_angle",
        &[
            0xbf97b846, 0xbf184a6b, 0x3ee88900, 0x40c45643, 0xbf151634, 0xbf3f2a97, 0xbea47e77,
            0x3ee1158a, 0xbf79b195, 0xbf00708a, 0xbf4787cd,
        ],
        Some(false),
        &[],
    ),
    (
        "random-cone-sphere-4",
        "cone_sphere",
        &[
            0xbf97b846, 0xbf184a6b, 0x3ee88900, 0x40c45643, 0xbf151634, 0xbf3f2a97, 0xbea47e77,
            0x3ee1158a, 0xbfd9ae2a, 0xc07945bd, 0xbfbe060d, 0x3fcda2a1,
        ],
        Some(false),
        &[],
    ),
    (
        "random-cone-cyl-4",
        "cone_cyl",
        &[
            0xbf97b846, 0xbf184a6b, 0x3ee88900, 0x40c45643, 0xbf151634, 0xbf3f2a97, 0xbea47e77,
            0x3ee1158a, 0xc04a6f2a, 0xbfd9108a, 0x3f79c6ed, 0x403478be, 0x3f27384f,
        ],
        Some(false),
        &[],
    ),
    (
        "random-cone-box-4",
        "cone_box",
        &[
            0xbf97b846, 0xbf184a6b, 0x3ee88900, 0x40c45643, 0xbf151634, 0xbf3f2a97, 0xbea47e77,
            0x3ee1158a, 0xc09accdb, 0xc0d4d44b, 0x3fc36856, 0xc08399db, 0xc0b47108, 0x400a2b71,
        ],
        Some(false),
        &[],
    ),
    (
        "random-shell-heights-4",
        "plane_cone_new",
        &[
            0x3fc7a976, 0x3f821364, 0x3e53e086, 0x3fb5ee96, 0x4081a7e1, 0x42e6fca3, 0x432e0c61,
            0xbf8d4a66, 0xbf791435,
        ],
        None,
        &[0xbf6497ae, 0xbf53994e],
    ),
    (
        "random-shell-angle-4",
        "plane_angle",
        &[
            0x3fc7a976, 0x3f821364, 0x3e53e086, 0x3fb5ee96, 0x4081a7e1, 0x42e6fca3, 0x432e0c61,
            0xbf8d4a66, 0xbf791435, 0xbe265ca7, 0x3f16327b, 0x3f4b17fa,
        ],
        Some(false),
        &[],
    ),
    (
        "random-shell-sphere-4",
        "plane_sphere",
        &[
            0x3fc7a976, 0x3f821364, 0x3e53e086, 0x3fb5ee96, 0x4081a7e1, 0x42e6fca3, 0x432e0c61,
            0xbf8d4a66, 0xbf791435, 0x40d64dd3, 0x40bbff15, 0xc0d8face, 0x3fef32bc,
        ],
        Some(false),
        &[],
    ),
    (
        "random-shell-cyl-4",
        "plane_cyl",
        &[
            0x3fc7a976, 0x3f821364, 0x3e53e086, 0x3fb5ee96, 0x4081a7e1, 0x42e6fca3, 0x432e0c61,
            0xbf8d4a66, 0xbf791435, 0x3ebbe467, 0x40182dd7, 0xc082d5a3, 0x4068719e, 0x3f41abc3,
        ],
        Some(false),
        &[],
    ),
    (
        "random-sphere-cyl-4",
        "sphere_cyl",
        &[
            0x40b56d3f, 0x408e1b48, 0x40043a5c, 0x3fb08404, 0x40563639, 0x4073689f, 0x3f96c82f,
            0x3fdaaa76, 0x3f84be11, 0x00000000,
        ],
        Some(true),
        &[],
    ),
    (
        "random-sphere-box-4",
        "sphere_box",
        &[
            0xc01392f5, 0xbf5dd081, 0xc0ac6e73, 0x4014b58f, 0xbf4dd222, 0xc060f82c, 0xc0cce01e,
            0x3e98cf6c, 0xbc8c1c00, 0xc01b976f,
        ],
        Some(true),
        &[],
    ),
    (
        "random-sphere-ray-4",
        "sphere_ray",
        &[
            0xc01392f5, 0xbf5dd081, 0xc0ac6e73, 0x4014b58f, 0xc015e40f, 0xc0937267, 0xc0a9d83a,
            0xbe111ce2, 0x3f7a51ed, 0x3e1df9df, 0x401d4e92,
        ],
        Some(true),
        &[0x3f1aa053],
    ),
    (
        "random-cyl-ray-4",
        "cyl_ray",
        &[
            0x40563639, 0x4073689f, 0x3f96c82f, 0x3fdaaa76, 0x3f84be11, 0x40e9f8bc, 0xc0639beb,
            0x3f464f6a, 0xbef9a0a6, 0x3f5f8305, 0x38e5df91, 0x419d8f4d,
        ],
        Some(true),
        &[0x3ebe8969],
    ),
    (
        "random-cyl-cyl-4",
        "cyl_cyl",
        &[
            0x40563639, 0x4073689f, 0x3f96c82f, 0x3fdaaa76, 0x3f84be11, 0x3fe950df, 0x408f876c,
            0x4080f5fb, 0x3fb03930, 0x3fa57ce0,
        ],
        Some(false),
        &[],
    ),
    (
        "random-cyl-box-4",
        "cyl_box",
        &[
            0x40563639, 0x4073689f, 0x3f96c82f, 0x3fdaaa76, 0x3f84be11, 0x3f92db13, 0x3f9721e7,
            0x400d6917, 0x40943b55, 0x4040f3d8, 0x4045b6a6,
        ],
        Some(true),
        &[],
    ),
    (
        "random-box-overlap-4",
        "box_overlap",
        &[
            0xc02a3cde, 0xc0480d5b, 0x4093ff9d, 0xbf8b2783, 0xbfb2cc79, 0x40d3188d, 0x406c834f,
            0xc093c7a1, 0x3f9002c0, 0x40d99318, 0xbf87d9f8, 0x403a08f8, 0xbe785d28,
        ],
        Some(false),
        &[],
    ),
    (
        "random-sphere-bbox-4",
        "sphere_bbox",
        &[
            0xc01392f5, 0xbf5dd081, 0xc0ac6e73, 0x4014b58f, 0x41400001, 0x430f2c72, 0x42750d99,
            0x42b4f949, 0x3f636454, 0xbd96979c, 0xbd3750dc, 0xbee706c8, 0x40400001, 0x43094716,
            0x411d77f6, 0x42da46d6, 0xbf62cf45, 0x3eb70aa5, 0x3cdf3bf9, 0x3e9694be,
        ],
        None,
        &[
            0xc1e2538a, 0x4095c28a, 0x42cd803b, 0xc1bd2626, 0x41153c0c, 0x42d6cb93,
        ],
    ),
    (
        "random-cyl-bbox-4",
        "cyl_bbox",
        &[
            0x40563639, 0x4073689f, 0x3f96c82f, 0x3fdaaa76, 0x3f84be11, 0x41400001, 0x4249df6a,
            0x424da976, 0x42e51727, 0x3e855949, 0xbf16f862, 0x3f155d89, 0x3efce42a, 0x41400001,
            0x43075389, 0x43020604, 0x4234aa6f, 0xbec8188d, 0x3f1ee45e, 0xbf2af156, 0x3e02205c,
        ],
        None,
        &[
            0x4304c560, 0x42fdbe0f, 0x4219f483, 0x43079a52, 0x4301be8f, 0x4226ef41,
        ],
    ),
    (
        "random-cone-angle-5",
        "cone_angle",
        &[
            0xbee89769, 0x3e67f006, 0xbf624ce3, 0x41052bd3, 0x3da6528b, 0x3e65eaa7, 0xbf78988b,
            0x3fa2f8b2, 0x3c81c0c3, 0xbe86ac2d, 0xbfb765b0,
        ],
        Some(true),
        &[],
    ),
    (
        "random-cone-sphere-5",
        "cone_sphere",
        &[
            0xbee89769, 0x3e67f006, 0xbf624ce3, 0x41052bd3, 0x3da6528b, 0x3e65eaa7, 0xbf78988b,
            0x3fa2f8b2, 0xbfecabaa, 0xbf540059, 0xbe951b50, 0x3fa204f9,
        ],
        Some(false),
        &[],
    ),
    (
        "random-cone-cyl-5",
        "cone_cyl",
        &[
            0xbee89769, 0x3e67f006, 0xbf624ce3, 0x41052bd3, 0x3da6528b, 0x3e65eaa7, 0xbf78988b,
            0x3fa2f8b2, 0x3fdda456, 0xbdcb437f, 0xc02ab107, 0x3f806be6, 0x3f1c8945,
        ],
        Some(true),
        &[],
    ),
    (
        "random-cone-box-5",
        "cone_box",
        &[
            0xbee89769, 0x3e67f006, 0xbf624ce3, 0x41052bd3, 0x3da6528b, 0x3e65eaa7, 0xbf78988b,
            0x3fa2f8b2, 0x3e8c8364, 0x3fbf6bd5, 0xc02ea746, 0x401a331e, 0x40123552, 0x3e9c53a8,
        ],
        Some(false),
        &[],
    ),
    (
        "random-shell-heights-5",
        "plane_cone_new",
        &[
            0xbf35d530, 0xbfedb1cb, 0x3e710516, 0x3f7d1dcc, 0x40a16550, 0x43a2ac14, 0x42ee97df,
            0xbf20a89f, 0xbef4a436,
        ],
        None,
        &[0xbf16518e, 0xbeeb7030],
    ),
    (
        "random-shell-angle-5",
        "plane_angle",
        &[
            0xbf35d530, 0xbfedb1cb, 0x3e710516, 0x3f7d1dcc, 0x40a16550, 0x43a2ac14, 0x42ee97df,
            0xbf20a89f, 0xbef4a436, 0x3f7a8cff, 0xbdfa2667, 0xbe28e1ea,
        ],
        Some(false),
        &[],
    ),
    (
        "random-shell-sphere-5",
        "plane_sphere",
        &[
            0xbf35d530, 0xbfedb1cb, 0x3e710516, 0x3f7d1dcc, 0x40a16550, 0x43a2ac14, 0x42ee97df,
            0xbf20a89f, 0xbef4a436, 0xc0d60199, 0xbf606bc5, 0x3f1e1a8a, 0x3d7ed27e,
        ],
        Some(false),
        &[],
    ),
    (
        "random-shell-cyl-5",
        "plane_cyl",
        &[
            0xbf35d530, 0xbfedb1cb, 0x3e710516, 0x3f7d1dcc, 0x40a16550, 0x43a2ac14, 0x42ee97df,
            0xbf20a89f, 0xbef4a436, 0xc0924281, 0xc05c5efe, 0x3e9b2538, 0x40660950, 0x3f0dc4f9,
        ],
        Some(false),
        &[],
    ),
    (
        "random-sphere-cyl-5",
        "sphere_cyl",
        &[
            0x40a2e89a, 0xc089568b, 0xc085ff72, 0x3ea331cd, 0x405ec557, 0xc0186d27, 0xc0957a0a,
            0x4073d6d2, 0x3ef06f37, 0x3f800000,
        ],
        Some(false),
        &[],
    ),
    (
        "random-sphere-box-5",
        "sphere_box",
        &[
            0x409e107f, 0xc0972e8e, 0xc0cc7eb1, 0x401d992d, 0x40ce1b0d, 0xc092c329, 0xc109b151,
            0x40fc0f25, 0xc07380e3, 0xc0f7cdb4,
        ],
        Some(true),
        &[],
    ),
    (
        "random-sphere-ray-5",
        "sphere_ray",
        &[
            0x409e107f, 0xc0972e8e, 0xc0cc7eb1, 0x401d992d, 0x41121036, 0xc0f7c111, 0xc13185f1,
            0xbf2e1fb1, 0x3e9703ef, 0x3f2bccfe, 0x416c7a94,
        ],
        Some(true),
        &[0x3ea2b88f],
    ),
    (
        "random-cyl-ray-5",
        "cyl_ray",
        &[
            0x405ec557, 0xc0186d27, 0xc0957a0a, 0x4073d6d2, 0x3ef06f37, 0xc0605ab9, 0x3fe16951,
            0x3fcdb909, 0x3f387299, 0xbedfdce0, 0xbf09ca8f, 0x4029911c,
        ],
        Some(false),
        &[0xc0e00000],
    ),
    (
        "random-cyl-cyl-5",
        "cyl_cyl",
        &[
            0x405ec557, 0xc0186d27, 0xc0957a0a, 0x4073d6d2, 0x3ef06f37, 0x3ff57216, 0xc03fbd99,
            0xc04cffb0, 0x406dba57, 0x3f485914,
        ],
        Some(false),
        &[],
    ),
    (
        "random-cyl-box-5",
        "cyl_box",
        &[
            0x405ec557, 0xc0186d27, 0xc0957a0a, 0x4073d6d2, 0x3ef06f37, 0x40c42eaf, 0xbf9df787,
            0xc00519fb, 0x411b5354, 0x3f996815, 0xbf0814f8,
        ],
        Some(false),
        &[],
    ),
    (
        "random-box-overlap-5",
        "box_overlap",
        &[
            0x4010edb8, 0x40bce024, 0x3e0b2ba6, 0x40941468, 0x40dc5883, 0x400437d9, 0x40802f5b,
            0x40a9cdf8, 0xc0a3b38f, 0x40ba2429, 0x40bddbec, 0xc02d60f1, 0xbed0c100,
        ],
        Some(false),
        &[],
    ),
    (
        "random-sphere-bbox-5",
        "sphere_bbox",
        &[
            0x409e107f, 0xc0972e8e, 0xc0cc7eb1, 0x401d992d, 0x40400001, 0x42cfb2f8, 0x42948a41,
            0x42d2dbaf, 0x3ee1e08a, 0x3d932421, 0xbf25b243, 0xbf1e12e3, 0x41400001, 0x41d1ae5a,
            0x42af6629, 0x429a0b5f, 0x3f5b18ea, 0x3eb5a739, 0x3eb6ea6d, 0xbdf26f3e,
        ],
        None,
        &[
            0x4335c306, 0x42ad03ab, 0x42830238, 0x433aafd0, 0x42b6dd3d, 0x428cdbca,
        ],
    ),
    (
        "random-cyl-bbox-5",
        "cyl_bbox",
        &[
            0x405ec557, 0xc0186d27, 0xc0957a0a, 0x4073d6d2, 0x3ef06f37, 0x41400001, 0x4307929d,
            0x42d1476b, 0x4290e369, 0xbe9ff796, 0x3e9ef274, 0xbe44c06b, 0xbf6081a5, 0x40400001,
            0x41a73390, 0x42866ac1, 0x421b0d5a, 0xbe45cd91, 0xbd981e9a, 0x3f53213e, 0xbf06bbe5,
        ],
        None,
        &[
            0xc30de1b9, 0x4285f7c5, 0x422787eb, 0xc30bcd08, 0x428dc536, 0x4231d5c1,
        ],
    ),
    (
        "random-cone-angle-6",
        "cone_angle",
        &[
            0x3f5f0168, 0xbfc4a1d6, 0x3f81f3df, 0x408cc84f, 0xbf67c395, 0x3ed1ecf8, 0xbde2e01e,
            0x3f692a20, 0xbf90bf3f, 0x3d7553d6, 0x3ef85298,
        ],
        Some(true),
        &[],
    ),
    (
        "random-cone-sphere-6",
        "cone_sphere",
        &[
            0x3f5f0168, 0xbfc4a1d6, 0x3f81f3df, 0x408cc84f, 0xbf67c395, 0x3ed1ecf8, 0xbde2e01e,
            0x3f692a20, 0xc030a50e, 0x3fe855ee, 0x3fc84032, 0x402ce756,
        ],
        Some(true),
        &[],
    ),
    (
        "random-cone-cyl-6",
        "cone_cyl",
        &[
            0x3f5f0168, 0xbfc4a1d6, 0x3f81f3df, 0x408cc84f, 0xbf67c395, 0x3ed1ecf8, 0xbde2e01e,
            0x3f692a20, 0x3ed51a3a, 0x3fe2e999, 0x40004261, 0x40b23875, 0x3eb39a03,
        ],
        Some(false),
        &[],
    ),
    (
        "random-cone-box-6",
        "cone_box",
        &[
            0x3f5f0168, 0xbfc4a1d6, 0x3f81f3df, 0x408cc84f, 0xbf67c395, 0x3ed1ecf8, 0xbde2e01e,
            0x3f692a20, 0xbfefa063, 0x3f25926e, 0x3f975071, 0x3e0ae798, 0x40847b6c, 0x408eaf0e,
        ],
        Some(true),
        &[],
    ),
    (
        "random-shell-heights-6",
        "plane_cone_new",
        &[
            0x3f07c45b, 0xbfcf1c37, 0xbe506259, 0x3dc2f090, 0x4071340c, 0x43a1c53e, 0x4320481e,
            0x3f862006, 0x3f8cff79,
        ],
        None,
        &[0x3f5dc948, 0x3f645411],
    ),
    (
        "random-shell-angle-6",
        "plane_angle",
        &[
            0x3f07c45b, 0xbfcf1c37, 0xbe506259, 0x3dc2f090, 0x4071340c, 0x43a1c53e, 0x4320481e,
            0x3f862006, 0x3f8cff79, 0xbf5e0163, 0x3eb19afd, 0x3eb6e7ff,
        ],
        Some(false),
        &[],
    ),
    (
        "random-shell-sphere-6",
        "plane_sphere",
        &[
            0x3f07c45b, 0xbfcf1c37, 0xbe506259, 0x3dc2f090, 0x4071340c, 0x43a1c53e, 0x4320481e,
            0x3f862006, 0x3f8cff79, 0x402d9ea5, 0xbed6802d, 0x40d31447, 0x3f7277ab,
        ],
        Some(false),
        &[],
    ),
    (
        "random-shell-cyl-6",
        "plane_cyl",
        &[
            0x3f07c45b, 0xbfcf1c37, 0xbe506259, 0x3dc2f090, 0x4071340c, 0x43a1c53e, 0x4320481e,
            0x3f862006, 0x3f8cff79, 0xc07f0048, 0xbf8c72e9, 0x3f722ce3, 0x407e57f9, 0x3fb8ee30,
        ],
        Some(false),
        &[],
    ),
    (
        "random-sphere-cyl-6",
        "sphere_cyl",
        &[
            0x40bb281a, 0x4032f964, 0x3fd75cad, 0x3fb89951, 0x40899b72, 0x3f307269, 0x3e86d82e,
            0x40bf4fb5, 0x3f570ec8, 0x00000000,
        ],
        Some(false),
        &[],
    ),
    (
        "random-sphere-box-6",
        "sphere_box",
        &[
            0x40a497ec, 0x3d01123f, 0x40e6de54, 0x3f508775, 0x401a105e, 0xc0195382, 0x40b611d9,
            0x40aad168, 0x3f3ec240, 0x40e9ecd6,
        ],
        Some(true),
        &[],
    ),
    (
        "random-sphere-ray-6",
        "sphere_ray",
        &[
            0x40a497ec, 0x3d01123f, 0x40e6de54, 0x3f508775, 0x41264c9a, 0xc02f9947, 0x4158e83f,
            0xbf243fcd, 0x3ec24eff, 0xbf2aa4c2, 0x418d2e42,
        ],
        Some(false),
        &[0x3f800000],
    ),
    (
        "random-cyl-ray-6",
        "cyl_ray",
        &[
            0x40899b72, 0x3f307269, 0x3e86d82e, 0x40bf4fb5, 0x3f570ec8, 0x4133214b, 0xbfbb66ed,
            0x40714b7f, 0xbf4fe626, 0x3e3a1493, 0xbf0df345, 0x3fbe70ba,
        ],
        Some(false),
        &[0xc0e00000],
    ),
    (
        "random-cyl-cyl-6",
        "cyl_cyl",
        &[
            0x40899b72, 0x3f307269, 0x3e86d82e, 0x40bf4fb5, 0x3f570ec8, 0x40dfd274, 0xbf8ef341,
            0xbf9e06c7, 0x4090ebce, 0x3f3265f4,
        ],
        Some(false),
        &[],
    ),
    (
        "random-cyl-box-6",
        "cyl_box",
        &[
            0x40899b72, 0x3f307269, 0x3e86d82e, 0x40bf4fb5, 0x3f570ec8, 0x40b6cfab, 0xbf7b0973,
            0x3fcedd24, 0x40d7191f, 0x3fa0f91a, 0x3feeb4d7,
        ],
        Some(false),
        &[],
    ),
    (
        "random-box-overlap-6",
        "box_overlap",
        &[
            0xc039cbf9, 0xbf51d818, 0xc09a5bba, 0xbef0ec98, 0x3e1918c0, 0xc01d125b, 0x3ed16900,
            0xbe9ca826, 0xbf75c6ca, 0x3fc51203, 0x3ff7026c, 0xbf48043f, 0xbddeecd9,
        ],
        Some(false),
        &[],
    ),
    (
        "random-sphere-bbox-6",
        "sphere_bbox",
        &[
            0x40a497ec, 0x3d01123f, 0x40e6de54, 0x3f508775, 0x40400001, 0x42e079ea, 0x3fbdc36f,
            0x42f33e71, 0xbf021806, 0x3ec80e01, 0x3eb8395d, 0xbf2d8e32, 0x40400001, 0x418bd7a6,
            0x4245e484, 0x4258fe5f, 0xbe864372, 0xbf555d08, 0x3ef1391c, 0xbdf7a4f3,
        ],
        None,
        &[
            0x41982947, 0x4223d36f, 0x4246f614, 0x41a531bf, 0x422a57ab, 0x424d7a50,
        ],
    ),
    (
        "random-cyl-bbox-6",
        "cyl_bbox",
        &[
            0x40899b72, 0x3f307269, 0x3e86d82e, 0x40bf4fb5, 0x3f570ec8, 0x40400001, 0x42d7da99,
            0x42367ab7, 0x430c53d9, 0x3d881af1, 0x3f4289f0, 0xbee65ae4, 0x3eedc8c8, 0x40400001,
            0x42475ba1, 0x43139eff, 0x415eb4a9, 0xbeac0b18, 0xbe013f62, 0x3e75a718, 0x3f66e9a1,
        ],
        None,
        &[
            0x422e0e3a, 0x430f470b, 0x415b9572, 0x42400308, 0x431395d5, 0x419e7105,
        ],
    ),
    (
        "random-cone-angle-7",
        "cone_angle",
        &[
            0x3f0bb237, 0xbfc30a97, 0xbfe40b99, 0x40daa7a3, 0xbf2d95ef, 0x3eabcb1c, 0x3f2768b4,
            0x3f918cc0, 0xbf81e3ae, 0x3f6c4336, 0x3f9d34a8,
        ],
        Some(true),
        &[],
    ),
    (
        "random-cone-sphere-7",
        "cone_sphere",
        &[
            0x3f0bb237, 0xbfc30a97, 0xbfe40b99, 0x40daa7a3, 0xbf2d95ef, 0x3eabcb1c, 0x3f2768b4,
            0x3f918cc0, 0xc095b687, 0x40540ea7, 0x403fcf76, 0x3fbd9d59,
        ],
        Some(false),
        &[],
    ),
    (
        "random-cone-cyl-7",
        "cone_cyl",
        &[
            0x3f0bb237, 0xbfc30a97, 0xbfe40b99, 0x40daa7a3, 0xbf2d95ef, 0x3eabcb1c, 0x3f2768b4,
            0x3f918cc0, 0xc0b010b2, 0x3feb2fb7, 0x4028cf8a, 0x40155d0f, 0x3fc8e0d4,
        ],
        Some(true),
        &[],
    ),
    (
        "random-cone-box-7",
        "cone_box",
        &[
            0x3f0bb237, 0xbfc30a97, 0xbfe40b99, 0x40daa7a3, 0xbf2d95ef, 0x3eabcb1c, 0x3f2768b4,
            0x3f918cc0, 0xbf65e670, 0x3ff93c69, 0x3fd5c178, 0x3e3bc3b0, 0x40b603d1, 0x4092cceb,
        ],
        Some(true),
        &[],
    ),
    (
        "random-shell-heights-7",
        "plane_cone_new",
        &[
            0xbf2f9285, 0xbf8c6c8e, 0xbf4cef66, 0x3f8cf959, 0x40579fd3, 0x4346161c, 0x42948e98,
            0xbf9a426f, 0x3fbae75a,
        ],
        None,
        &[0xbf6f13a9, 0x3f7e6f80],
    ),
    (
        "random-shell-angle-7",
        "plane_angle",
        &[
            0xbf2f9285, 0xbf8c6c8e, 0xbf4cef66, 0x3f8cf959, 0x40579fd3, 0x4346161c, 0x42948e98,
            0xbf9a426f, 0x3fbae75a, 0xbde45409, 0x3f037f7d, 0xbf59c898,
        ],
        Some(true),
        &[],
    ),
    (
        "random-shell-sphere-7",
        "plane_sphere",
        &[
            0xbf2f9285, 0xbf8c6c8e, 0xbf4cef66, 0x3f8cf959, 0x40579fd3, 0x4346161c, 0x42948e98,
            0xbf9a426f, 0x3fbae75a, 0x4070b3fa, 0xc0a30ffc, 0xc01644a5, 0x3fc3aa78,
        ],
        Some(false),
        &[],
    ),
    (
        "random-shell-cyl-7",
        "plane_cyl",
        &[
            0xbf2f9285, 0xbf8c6c8e, 0xbf4cef66, 0x3f8cf959, 0x40579fd3, 0x4346161c, 0x42948e98,
            0xbf9a426f, 0x3fbae75a, 0x409268aa, 0xbff46b3c, 0xc04efe7d, 0x3f7daffd, 0x3f875384,
        ],
        Some(false),
        &[],
    ),
    (
        "random-sphere-cyl-7",
        "sphere_cyl",
        &[
            0xc0bff350, 0x4060b5a7, 0xbdd70def, 0x3f2175dc, 0xc0aafd9c, 0x40a8e37d, 0x3fab1997,
            0x4039c464, 0x3f921d0c, 0x3f800000,
        ],
        Some(false),
        &[],
    ),
    (
        "random-sphere-box-7",
        "sphere_box",
        &[
            0xbfa752d5, 0xc0b35c67, 0x40a8c46e, 0x3e0cff60, 0xc0856e65, 0xc0b32c1d, 0x407d7523,
            0xc0793b0d, 0xc0a0eaf1, 0x40c255b9,
        ],
        Some(false),
        &[],
    ),
    (
        "random-sphere-ray-7",
        "sphere_ray",
        &[
            0xbfa752d5, 0xc0b35c67, 0x40a8c46e, 0x3e0cff60, 0xc0da22e3, 0xc11f3df7, 0x40901b82,
            0x3f557f4b, 0x3f0d355e, 0xbc78affc, 0x4134068a,
        ],
        Some(false),
        &[0x3f800000],
    ),
    (
        "random-cyl-ray-7",
        "cyl_ray",
        &[
            0xc0aafd9c, 0x40a8e37d, 0x3fab1997, 0x4039c464, 0x3f921d0c, 0xc0d06271, 0x40ebdd39,
            0xc02c0814, 0x3eaeb071, 0xbf26e263, 0x3f2d5e44, 0x41814253,
        ],
        Some(false),
        &[0x3f800000],
    ),
    (
        "random-cyl-cyl-7",
        "cyl_cyl",
        &[
            0xc0aafd9c, 0x40a8e37d, 0x3fab1997, 0x4039c464, 0x3f921d0c, 0xc0eb10e1, 0x4065c12c,
            0xbf933b27, 0x40980157, 0x3f87e0b8,
        ],
        Some(false),
        &[],
    ),
    (
        "random-cyl-box-7",
        "cyl_box",
        &[
            0xc0aafd9c, 0x40a8e37d, 0x3fab1997, 0x4039c464, 0x3f921d0c, 0xc0fa5b3b, 0x40832cba,
            0x3f8aac8e, 0xc0b36456, 0x40c40997, 0x40817274,
        ],
        Some(true),
        &[],
    ),
    (
        "random-box-overlap-7",
        "box_overlap",
        &[
            0x40b2495d, 0xc069fc80, 0xc03da1ee, 0x40c5c5a6, 0xbfbd296e, 0xc02b247e, 0x40bd4bd5,
            0xc04f5612, 0xc02ace34, 0x410d736a, 0xc0296c86, 0xbe890228, 0x3e0bfd30,
        ],
        Some(false),
        &[],
    ),
    (
        "random-sphere-bbox-7",
        "sphere_bbox",
        &[
            0xbfa752d5, 0xc0b35c67, 0x40a8c46e, 0x3e0cff60, 0x41400001, 0x426e701d, 0x4310f6ec,
            0x41af4367, 0xbf27cb3a, 0xbd14c919, 0x3ec9e96c, 0xbf24a0fb, 0x41400001, 0x42f2e43d,
            0x430c6048, 0x42810da7, 0xbe210e7f, 0x3ef8a774, 0x3f5129a5, 0x3e893ce3,
        ],
        None,
        &[
            0x42ea0fce, 0x430c4574, 0x42676f51, 0x42ea9cce, 0x430c8bf4, 0x4268894f,
        ],
    ),
    (
        "random-cyl-bbox-7",
        "cyl_bbox",
        &[
            0xc0aafd9c, 0x40a8e37d, 0x3fab1997, 0x4039c464, 0x3f921d0c, 0x41400001, 0x430c495d,
            0x426fb0a5, 0x41b33dd9, 0xbf1a376a, 0xbe85f6cf, 0xbe38eb49, 0xbf3b6dbb, 0x40400001,
            0x42c8f960, 0x43104fd7, 0x41c2f6fd, 0x3f211813, 0x3ef6e22d, 0x3edd68bd, 0x3edbebd9,
        ],
        None,
        &[
            0xc272c400, 0x4309ca58, 0x41e96e31, 0xc264297c, 0x430d09f0, 0x4200b4b3,
        ],
    ),
    (
        "random-cone-angle-8",
        "cone_angle",
        &[
            0xbebf9447, 0xbfb789f8, 0xbf816114, 0x40877923, 0x3f366807, 0xbedc3481, 0x3f0deb93,
            0x3f025882, 0x3f16bec0, 0xbeac91f0, 0x3e07a10a,
        ],
        Some(false),
        &[],
    ),
    (
        "random-cone-sphere-8",
        "cone_sphere",
        &[
            0xbebf9447, 0xbfb789f8, 0xbf816114, 0x40877923, 0x3f366807, 0xbedc3481, 0x3f0deb93,
            0x3f025882, 0x4044f9a9, 0xc03573e9, 0x3e1811fc, 0x3fbe80de,
        ],
        Some(true),
        &[],
    ),
    (
        "random-cone-cyl-8",
        "cone_cyl",
        &[
            0xbebf9447, 0xbfb789f8, 0xbf816114, 0x40877923, 0x3f366807, 0xbedc3481, 0x3f0deb93,
            0x3f025882, 0xc0006cb0, 0xbdbae5f6, 0x3fac58c1, 0x40130bfa, 0x3f9162c2,
        ],
        Some(false),
        &[],
    ),
    (
        "random-cone-box-8",
        "cone_box",
        &[
            0xbebf9447, 0xbfb789f8, 0xbf816114, 0x40877923, 0x3f366807, 0xbedc3481, 0x3f0deb93,
            0x3f025882, 0x403fbeba, 0xc07e94ed, 0x400dfe78, 0x4091c477, 0xbf893c18, 0x40a22f07,
        ],
        Some(false),
        &[],
    ),
    (
        "random-shell-heights-8",
        "plane_cone_new",
        &[
            0xbf775367, 0x3f8527cc, 0xbf758cd4, 0x3f967849, 0x40bda221, 0x435c86a1, 0x41b2cae3,
            0xbf8aa7ea, 0xbedba66a,
        ],
        None,
        &[0xbf622c0d, 0xbed4f967],
    ),
    (
        "random-shell-angle-8",
        "plane_angle",
        &[
            0xbf775367, 0x3f8527cc, 0xbf758cd4, 0x3f967849, 0x40bda221, 0x435c86a1, 0x41b2cae3,
            0xbf8aa7ea, 0xbedba66a, 0xbe562986, 0xbf3f1d57, 0x3f21b166,
        ],
        Some(false),
        &[],
    ),
    (
        "random-shell-sphere-8",
        "plane_sphere",
        &[
            0xbf775367, 0x3f8527cc, 0xbf758cd4, 0x3f967849, 0x40bda221, 0x435c86a1, 0x41b2cae3,
            0xbf8aa7ea, 0xbedba66a, 0x406c4da3, 0xbf6d61da, 0xbdada393, 0x3f0a5c27,
        ],
        Some(false),
        &[],
    ),
    (
        "random-shell-cyl-8",
        "plane_cyl",
        &[
            0xbf775367, 0x3f8527cc, 0xbf758cd4, 0x3f967849, 0x40bda221, 0x435c86a1, 0x41b2cae3,
            0xbf8aa7ea, 0xbedba66a, 0x3f2bb92a, 0x3df93b36, 0x3fba0101, 0x3f127969, 0x3fa8bd46,
        ],
        Some(false),
        &[],
    ),
    (
        "random-sphere-cyl-8",
        "sphere_cyl",
        &[
            0x3fd868a3, 0x40152a9b, 0x3fe5e1f4, 0x3fafe737, 0x3f32238b, 0x402bdb02, 0x3fc31f30,
            0x40a29e03, 0x3fa2be55, 0x00000000,
        ],
        Some(true),
        &[],
    ),
    (
        "random-sphere-box-8",
        "sphere_box",
        &[
            0x40d4de3a, 0xc05aeaec, 0xc085bc32, 0x3f1f7a3b, 0x40ccee82, 0xc0c18fa6, 0xbfcf7c3f,
            0x4121868b, 0xc02ce6ae, 0xbe4e8d50,
        ],
        Some(false),
        &[],
    ),
    (
        "random-sphere-ray-8",
        "sphere_ray",
        &[
            0x40d4de3a, 0xc05aeaec, 0xc085bc32, 0x3f1f7a3b, 0x40883b68, 0xc062d0c2, 0xc0cc2bbd,
            0x3f1ebd16, 0x3ee1e450, 0x3f26137b, 0x40b6fb0b,
        ],
        Some(false),
        &[0x3f800000],
    ),
    (
        "random-cyl-ray-8",
        "cyl_ray",
        &[
            0x3f32238b, 0x402bdb02, 0x3fc31f30, 0x40a29e03, 0x3fa2be55, 0x407519f0, 0x4126e0f4,
            0xc0ade06b, 0xbea9b0de, 0xbf34f3e4, 0x3f1ffab2, 0x40dfb87f,
        ],
        Some(false),
        &[0xc0e00000],
    ),
    (
        "random-cyl-cyl-8",
        "cyl_cyl",
        &[
            0x3f32238b, 0x402bdb02, 0x3fc31f30, 0x40a29e03, 0x3fa2be55, 0xbffca295, 0x402c4e72,
            0x3fb2aae3, 0x402675cb, 0x3fd35dc9,
        ],
        Some(true),
        &[],
    ),
    (
        "random-cyl-box-8",
        "cyl_box",
        &[
            0x3f32238b, 0x402bdb02, 0x3fc31f30, 0x40a29e03, 0x3fa2be55, 0x405893d6, 0x4096cd17,
            0x3fb55797, 0x408cf503, 0x41081a5e, 0x403ade9e,
        ],
        Some(false),
        &[],
    ),
    (
        "random-box-overlap-8",
        "box_overlap",
        &[
            0x4055fd99, 0x4086bbc8, 0xbfd2c063, 0x40b620b6, 0x40c56247, 0x3b4f9600, 0x40b449a8,
            0x40404e33, 0x40449b56, 0x40f3b2a2, 0x40543f80, 0x409b20ea, 0x3e6672ab,
        ],
        Some(false),
        &[],
    ),
    (
        "random-sphere-bbox-8",
        "sphere_bbox",
        &[
            0x40d4de3a, 0xc05aeaec, 0xc085bc32, 0x3f1f7a3b, 0x40400001, 0x40450f7e, 0x4288a22f,
            0x40ed2259, 0x3d8ec624, 0x3e94c252, 0x3f49ec4a, 0x3f09859d, 0x40400001, 0x42d596c4,
            0x428328ed, 0x4232583f, 0xbf3bdaea, 0xbe5b451f, 0xbf1286da, 0x3e97f080,
        ],
        None,
        &[
            0x42cbf8ae, 0x427fad1d, 0x42123ca9, 0x42ce7696, 0x42825477, 0x4217387b,
        ],
    ),
    (
        "random-cyl-bbox-8",
        "cyl_bbox",
        &[
            0x3f32238b, 0x402bdb02, 0x3fc31f30, 0x40a29e03, 0x3fa2be55, 0x41400001, 0x42a2661b,
            0x409a6bcb, 0x42d7f14b, 0xbf055f15, 0xbe6f96c1, 0xbe5ea553, 0x3f4aa359, 0x40400001,
            0x430a096a, 0x42bf9a39, 0x42ca4cfd, 0xbeb6cd76, 0x3ee3cf8b, 0xbf1a86f7, 0x3f0e942d,
        ],
        None,
        &[
            0xc1ae5048, 0x42b71bf2, 0x42c0be89, 0xc17d67c0, 0x42bf8d19, 0x42c7600c,
        ],
    ),
    (
        "random-cone-angle-9",
        "cone_angle",
        &[
            0xbe9c834a, 0x3f7536cd, 0xbe4a28cf, 0x40fe738f, 0xbf736893, 0x3e3f91e3, 0xbe7ccfdc,
            0x3d552180, 0xbf8020ca, 0x3d4f4f1a, 0x3e07cd33,
        ],
        Some(false),
        &[],
    ),
    (
        "random-cone-sphere-9",
        "cone_sphere",
        &[
            0xbe9c834a, 0x3f7536cd, 0xbe4a28cf, 0x40fe738f, 0xbf736893, 0x3e3f91e3, 0xbe7ccfdc,
            0x3d552180, 0x3e3e11a1, 0xbf89adab, 0x3f1a46c3, 0x400871fe,
        ],
        Some(false),
        &[],
    ),
    (
        "random-cone-cyl-9",
        "cone_cyl",
        &[
            0xbe9c834a, 0x3f7536cd, 0xbe4a28cf, 0x40fe738f, 0xbf736893, 0x3e3f91e3, 0xbe7ccfdc,
            0x3d552180, 0x3f18017b, 0x4027d07c, 0x3ef32555, 0x40bad099, 0x3f3d5338,
        ],
        Some(false),
        &[],
    ),
    (
        "random-cone-box-9",
        "cone_box",
        &[
            0xbe9c834a, 0x3f7536cd, 0xbe4a28cf, 0x40fe738f, 0xbf736893, 0x3e3f91e3, 0xbe7ccfdc,
            0x3d552180, 0xbe09df52, 0x3fa581b6, 0x3fbb47b2, 0x405738a0, 0x3fe68301, 0x409eddee,
        ],
        Some(false),
        &[],
    ),
    (
        "random-shell-heights-9",
        "plane_cone_new",
        &[
            0x3fc1d2b3, 0xbfd13b73, 0xbf7e8a89, 0x3f27ff11, 0x40dbe334, 0x4204a5b0, 0x420c8878,
            0xbf2c77ad, 0xbf10ec28,
        ],
        None,
        &[0xbf1fb6c7, 0xbf094e11],
    ),
    (
        "random-shell-angle-9",
        "plane_angle",
        &[
            0x3fc1d2b3, 0xbfd13b73, 0xbf7e8a89, 0x3f27ff11, 0x40dbe334, 0x4204a5b0, 0x420c8878,
            0xbf2c77ad, 0xbf10ec28, 0xbf276314, 0x3e62a57b, 0xbf3938bc,
        ],
        Some(false),
        &[],
    ),
    (
        "random-shell-sphere-9",
        "plane_sphere",
        &[
            0x3fc1d2b3, 0xbfd13b73, 0xbf7e8a89, 0x3f27ff11, 0x40dbe334, 0x4204a5b0, 0x420c8878,
            0xbf2c77ad, 0xbf10ec28, 0xc0a55e5e, 0x404379c8, 0x401fdfe4, 0x3f7593c1,
        ],
        Some(false),
        &[],
    ),
    (
        "random-shell-cyl-9",
        "plane_cyl",
        &[
            0x3fc1d2b3, 0xbfd13b73, 0xbf7e8a89, 0x3f27ff11, 0x40dbe334, 0x4204a5b0, 0x420c8878,
            0xbf2c77ad, 0xbf10ec28, 0x3f0eb9f2, 0x405864b5, 0x40960c9b, 0x3f77f673, 0x3e21746c,
        ],
        Some(false),
        &[],
    ),
    (
        "random-sphere-cyl-9",
        "sphere_cyl",
        &[
            0xc0826b7e, 0x3ffb7ab1, 0xc04393b3, 0x3f6f950f, 0xc05e684a, 0x4005226c, 0xc04bbbcb,
            0x3fc6a4a0, 0x3f539452, 0x3f800000,
        ],
        Some(true),
        &[],
    ),
    (
        "random-sphere-box-9",
        "sphere_box",
        &[
            0x401abf46, 0x407e55c9, 0xbf00b620, 0x40070f71, 0x40527ba0, 0x40911f6d, 0xc035672e,
            0x409b7990, 0x409c0327, 0x3b80d200,
        ],
        Some(true),
        &[],
    ),
    (
        "random-sphere-ray-9",
        "sphere_ray",
        &[
            0x401abf46, 0x407e55c9, 0xbf00b620, 0x40070f71, 0xc08d0e77, 0x4100311b, 0xc1015d3d,
            0x3f22c82f, 0xbeaff048, 0x3f30ea9d, 0x40dbf3d2,
        ],
        Some(false),
        &[0x3f800000],
    ),
    (
        "random-cyl-ray-9",
        "cyl_ray",
        &[
            0xc05e684a, 0x4005226c, 0xc04bbbcb, 0x3fc6a4a0, 0x3f539452, 0xc008cd11, 0x40af2809,
            0x40872588, 0xbe65577e, 0xbeefd185, 0xbf5acb09, 0x41545607,
        ],
        Some(true),
        &[0x3f04253f],
    ),
    (
        "random-cyl-cyl-9",
        "cyl_cyl",
        &[
            0xc05e684a, 0x4005226c, 0xc04bbbcb, 0x3fc6a4a0, 0x3f539452, 0xc01cee48, 0x407c0618,
            0xbf12309d, 0x3fc80643, 0x3f47a35c,
        ],
        Some(false),
        &[],
    ),
    (
        "random-cyl-box-9",
        "cyl_box",
        &[
            0xc05e684a, 0x4005226c, 0xc04bbbcb, 0x3fc6a4a0, 0x3f539452, 0xbffde062, 0x409b9dee,
            0xc00f9862, 0xbfae9430, 0x40dae398, 0x3f4fbb0c,
        ],
        Some(false),
        &[],
    ),
    (
        "random-box-overlap-9",
        "box_overlap",
        &[
            0xbfcf98fa, 0xc05432dd, 0xbffa0d0f, 0x3fbefadc, 0xbf7f1654, 0x3fdbbfaf, 0xc04e97b6,
            0xbfe71b85, 0x40836fa6, 0xbfe7d8a0, 0x3fa6300b, 0x40a45866, 0xbe705779,
        ],
        Some(false),
        &[],
    ),
    (
        "random-sphere-bbox-9",
        "sphere_bbox",
        &[
            0x401abf46, 0x407e55c9, 0xbf00b620, 0x40070f71, 0x41400001, 0x4276cdc6, 0x42f6de88,
            0x42ac9ab1, 0x3eb0498f, 0x3f153e7b, 0x3efe3d16, 0x3f0b0b96, 0x40400001, 0x42fe5109,
            0x41d73cd8, 0x42f07542, 0x3f616590, 0xbef1be7d, 0x3d122795, 0xbcc7610d,
        ],
        None,
        &[
            0xc2023313, 0x41d31dd5, 0x42e4de3a, 0xc1e2a24a, 0x41f4e1b1, 0x42ed4f32,
        ],
    ),
    (
        "random-cyl-bbox-9",
        "cyl_bbox",
        &[
            0xc05e684a, 0x4005226c, 0xc04bbbcb, 0x3fc6a4a0, 0x3f539452, 0x41400001, 0x42ff8ce8,
            0x43064fde, 0x42789e2a, 0x3e4a3a32, 0xbe9dfe31, 0x3e142c6b, 0x3f6b4d7d, 0x41400001,
            0x41842e61, 0x42a3398f, 0x42571164, 0x3f028843, 0xbf23eb21, 0xbf1272b1, 0xbd5748e7,
        ],
        None,
        &[
            0x41871a60, 0x429947df, 0x42481356, 0x419c66ef, 0x429e50ce, 0x42533e38,
        ],
    ),
];

/// What the port answers for one case: the answer and the values to compare.
fn port_answers(kind: &str, bits: &[u32], x: &[f32]) -> (Option<bool>, Vec<f32>) {
    use auberean_physics::geom::SphereOps;
    use auberean_physics::geom::{BBox, BoxOps, Cone, CylSphere, CylinderOps, PlaneCone, Sphere};
    let v = |i: usize| Vec3::new(x[i], x[i + 1], x[i + 2]);
    let cone = || Cone::new(v(0), x[3], v(4), x[7]);
    // A shell is nine numbers, or eleven with the heights written over its own; what follows it
    // is a point (3), a sphere (4), a cylinder (5), a box (6) or two positions (16).
    let shell_len = x.len()
        - match kind {
            "plane_angle" => 3,
            "plane_sphere" => 4,
            "plane_cyl" => 5,
            "plane_box" => 6,
            "plane_bbox" => 16,
            _ => 0,
        };
    let shell = || {
        let mut s = PlaneCone::new(v(0), x[3], x[4], x[5], x[6], x[7], x[8]);
        if shell_len > 9 {
            (s.min_z, s.max_z) = (x[9], x[10]);
        }
        s
    };
    let sphere = |i: usize| Sphere::new(v(i), x[i + 3]);
    let cyl = |i: usize| CylSphere {
        low_pt: v(i),
        height: x[i + 3],
        radius: x[i + 4],
    };
    let bbox = |i: usize| BBox::new(v(i), v(i + 3));
    let ray = |i: usize| Ray {
        point: v(i),
        dir: v(i + 3),
        length: x[i + 6],
    };
    let ray_answer = |t: Option<f32>| (Some(t.is_some()), t.into_iter().collect());
    let position = |i: usize| {
        Position::new(
            CellId(bits[i]),
            Frame::new(v(i + 1), Quat::new(x[i + 4], x[i + 5], x[i + 6], x[i + 7])),
        )
    };
    let corners = |b: BBox| {
        (
            None,
            vec![b.min.x, b.min.y, b.min.z, b.max.x, b.max.y, b.max.z],
        )
    };
    match kind {
        "cone_new" => (None, vec![cone().cos_angle]),
        "plane_cone_new" => {
            let s = shell();
            (None, vec![s.min_z, s.max_z])
        }
        "cone_angle" => (Some(cone().angle_check(v(8))), vec![]),
        "cone_sphere" => (Some(cone().intersects_sphere(&sphere(8))), vec![]),
        "cone_cyl" => (Some(cone().intersects_cylinder(&cyl(8))), vec![]),
        "cone_box" => (Some(cone().intersects_box(&bbox(8))), vec![]),
        "plane_angle" => (Some(shell().check_angle(v(shell_len))), vec![]),
        "plane_sphere" => (Some(shell().intersects_sphere(&sphere(shell_len))), vec![]),
        "plane_cyl" => (Some(shell().intersects_cylinder(&cyl(shell_len))), vec![]),
        "plane_box" => (Some(shell().intersects_box(&bbox(shell_len))), vec![]),
        "sphere_cyl" => (
            Some(sphere(0).intersects_cylinder(&cyl(4), x[9] != 0.0)),
            vec![],
        ),
        "sphere_box" => (Some(sphere(0).intersects_box(&bbox(4))), vec![]),
        "sphere_ray" => ray_answer(sphere(0).trace_ray(&ray(4))),
        "cyl_ray" => ray_answer(cyl(0).trace_ray(&ray(5))),
        "cyl_sphere" => (Some(cyl(0).intersects_sphere(&sphere(5))), vec![]),
        "cyl_cyl" => (Some(cyl(0).intersects_cylinder(&cyl(5))), vec![]),
        "cyl_box" => (Some(cyl(0).intersects_box(&bbox(5))), vec![]),
        "cyl_within" => (Some(cyl(0).within(v(5))), vec![]),
        "box_overlap" => (Some(bbox(0).does_overlap(&bbox(6), x[12])), vec![]),
        "sphere_bbox" => corners(SphereOps::bounding_box(
            &sphere(0),
            &position(4),
            &position(12),
        )),
        "cyl_bbox" => corners(CylinderOps::bounding_box(
            &cyl(0),
            &position(5),
            &position(13),
        )),
        "plane_bbox" => {
            corners(shell().bounding_box(&position(shell_len), &position(shell_len + 8)))
        }
        _ => panic!("no such test: {kind}"),
    }
}

/// Every detection shape test answers as the September 2013 client's own code does, case for
/// case: cones, shells, spheres, upright cylinders, boxes and rays, on the boundary, at no radius,
/// inside one another and off every angle; a constructor stores the same cosine and heights to
/// the bit, a ray that meets a shape gives the same fraction to the bit, and the box around a
/// shape carried into another position's frame (another landblock's, or one turned on its side)
/// has the same corners to the bit (two NaNs counting as the same).
///
/// Behaviour: shape.detect.every-shape-test-answers-as-the-client-does
#[test]
fn every_shape_test_answers_as_the_client_does() {
    let mut wrong = Vec::new();
    for &(name, kind, bits, answer, values) in ANSWERS {
        let x: Vec<f32> = bits.iter().map(|&b| f32::from_bits(b)).collect();
        let (got, got_values) = port_answers(kind, bits, &x);
        // A ray that misses leaves no fraction to compare.
        let want_values: Vec<u32> = if answer == Some(false) && kind.ends_with("_ray") {
            Vec::new()
        } else {
            values.to_vec()
        };
        // Two NaNs are the same answer, whatever their bits.
        let same = |a: f32, b: u32| a.to_bits() == b || (a.is_nan() && f32::from_bits(b).is_nan());
        let values_agree = got_values.len() == want_values.len()
            && got_values
                .iter()
                .zip(&want_values)
                .all(|(&a, &b)| same(a, b));
        if got != answer || !values_agree {
            wrong.push(format!(
                "{name}: client {answer:?} {:?}, port {got:?} {:?}",
                want_values
                    .iter()
                    .map(|&b| f32::from_bits(b))
                    .collect::<Vec<_>>(),
                got_values
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "{} of {}:\n{}",
        wrong.len(),
        ANSWERS.len(),
        wrong.join("\n")
    );
}
