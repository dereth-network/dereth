//! The interface scenes' cameras, as a scene's camera entity sets them up.
//! Fixture: a camera entity made by hand as the paper doll's scene keeps its own (turned half
//! round, its properties in the camera group), and one with no properties.

use auberean_assets::entity::{Entity, StoredFrame};
use auberean_assets::property::{Property, PropertyCollection, PropertyGroup, PropertyValue};
use auberean_assets::ui::UiScene;
use auberean_primitives::{Frame, Quat, Vec3};
use auberean_scene::view::{
    scene_camera, ViewCamera, CAMERA_GROUP, CAMERA_KIND, FAR, FOV, NEAR, ORTHOGRAPHIC,
    VERTICAL_SCALE,
};

fn property(id: u32, value: PropertyValue) -> Property {
    Property {
        id,
        value_type: 0,
        group: CAMERA_GROUP,
        value,
    }
}

fn camera(name: &str, properties: Option<PropertyCollection>) -> Entity {
    Entity {
        kind: Some(CAMERA_KIND),
        name: Some(name.to_owned()),
        offset: Some(StoredFrame {
            frame: Frame {
                origin: Vec3::new(1.0, 2.0, 3.0),
                rotation: Quat {
                    w: 0.0,
                    x: 0.0,
                    y: 0.0,
                    z: 1.0,
                },
            },
            matrix: [0.0; 9],
        }),
        properties,
        ..Entity::default()
    }
}

/// A scene's camera stands where its entity does, looking along the entity's +y, and takes the
/// properties of its camera group: seen straight on, its field of view, the height it sees, near
/// and far (the paper doll's: 90°, 1 m, -20 and 20). A property in another group is not its own.
/// A camera with no properties sees in perspective, 65° high, from 0.1 to 2000 m, and a height of
/// 1 m when it is turned straight on.
///
/// Behaviour: draw.view.a-scenes-camera-takes-the-properties-of-its-camera-group
#[test]
fn a_scenes_camera_takes_the_properties_of_its_camera_group() {
    let own = PropertyGroup {
        group: CAMERA_GROUP,
        properties: vec![
            property(ORTHOGRAPHIC, PropertyValue::Bool(true)),
            property(FOV, PropertyValue::Float(90.0)),
            property(VERTICAL_SCALE, PropertyValue::Float(1.0)),
            property(NEAR, PropertyValue::Float(-20.0)),
            property(FAR, PropertyValue::Float(20.0)),
        ],
        optional: Vec::new(),
    };
    let other = PropertyGroup {
        group: 0x4000_0001,
        properties: vec![property(FOV, PropertyValue::Float(10.0))],
        optional: Vec::new(),
    };
    let scene = UiScene {
        entities: vec![
            camera(
                "Camera_1",
                Some(PropertyCollection {
                    groups: vec![own, other],
                }),
            ),
            camera("Camera_2", None),
        ],
        ..UiScene::default()
    };
    let c = scene_camera(&scene, "Camera_1").expect("the camera");
    assert_eq!(c.position, Vec3::new(1.0, 2.0, 3.0));
    assert!(
        (c.heading - 180.0).abs() < 1e-3,
        "looking south: {}",
        c.heading
    );
    assert!(c.pitch.abs() < 1e-3, "level: {}", c.pitch);
    assert!(c.orthographic, "seen straight on");
    assert_eq!(
        (c.fov_y, c.vertical_scale, c.near, c.far),
        (90.0, 1.0, -20.0, 20.0),
        "its group's values, not the other group's"
    );
    assert_eq!(c.drawn().orthographic, Some(1.0), "drawn straight on");

    let plain = scene_camera(&scene, "Camera_2").expect("the second camera");
    assert!(!plain.orthographic, "in perspective");
    assert_eq!(
        (plain.fov_y, plain.vertical_scale, plain.near, plain.far),
        (65.0, 1.0, 0.1, 2000.0)
    );
    assert_eq!(plain.drawn().orthographic, None, "drawn in perspective");
    assert!(scene_camera(&scene, "Camera_3").is_none(), "no such camera");
}

/// A camera takes only its own properties, each of its own kind: a field of view given as a
/// yes-or-no, or a property it has not, is refused and changes nothing.
///
/// Behaviour: draw.view.a-scenes-camera-takes-the-properties-of-its-camera-group
#[test]
fn a_camera_refuses_a_property_it_has_not_or_of_another_kind() {
    let mut c = ViewCamera::default();
    assert!(!c.apply_property(FOV, &PropertyValue::Bool(true)));
    assert!(!c.apply_property(ORTHOGRAPHIC, &PropertyValue::Float(1.0)));
    assert!(!c.apply_property(0x4000_0099, &PropertyValue::Float(1.0)));
    assert_eq!(c, ViewCamera::default());
    assert!(c.apply_property(VERTICAL_SCALE, &PropertyValue::Float(2.5)));
    assert_eq!(c.vertical_scale, 2.5);
}

/// A ray meets a list of triangles as the client's trace finds it: the nearest triangle whose plane
/// the ray reaches, as a share of the ray's length, with the triangle's normal (its first edge across
/// its second) and the point weighted as the client weights it; one-sided, only from in front of a
/// triangle; a triangle whose normal is under 0.0002 long, a ray that ends short of the plane, and a
/// ray running within 0.0002 of the plane all miss. (The point is weighted as the client weights it,
/// not where the ray met; no pick reads it.)
///
/// Behaviour: desktop.interface.a-ray-meets-triangles-as-the-clients-trace-finds-them
#[test]
fn a_ray_meets_triangles_as_the_clients_trace_finds_them() {
    use auberean_scene::view::ray_intersects_tri_list as trace;
    let v = |x: f32, y: f32, z: f32| Vec3::new(x, y, z);
    // A triangle in the plane z = 0, facing up (its first edge across its second).
    let floor = [v(0.0, 0.0, 0.0), v(2.0, 0.0, 0.0), v(0.0, 2.0, 0.0)];
    let down = v(0.0, 0.0, -1.0);
    let hit = trace(v(0.5, 0.5, 10.0), down, 100.0, [floor], false).expect("from in front");
    assert!((hit.fraction - 0.1).abs() < 1e-6);
    assert_eq!(hit.normal, v(0.0, 0.0, 1.0));
    // Its edges' shares there are a quarter each, and the client weights the first corner by the
    // first edge's share, the second by the second's, the third by the rest: (0.5, 1) where the ray
    // met (0.5, 0.5).
    assert!((hit.point.x - 0.5).abs() < 1e-5 && (hit.point.y - 1.0).abs() < 1e-5);
    let up = v(0.0, 0.0, 1.0);
    assert_eq!(
        trace(v(0.5, 0.5, -10.0), up, 100.0, [floor], false),
        None,
        "from behind"
    );
    assert!(
        trace(v(0.5, 0.5, -10.0), up, 100.0, [floor], true).is_some(),
        "two-sided"
    );
    assert_eq!(
        trace(v(0.5, 0.5, 10.0), down, 5.0, [floor], true),
        None,
        "ends short"
    );
    let tiny = [v(0.5, 0.5, 0.0), v(0.51, 0.5, 0.0), v(0.5, 0.51, 0.0)];
    assert_eq!(
        trace(v(0.505, 0.505, 1.0), down, 10.0, [tiny], true),
        None,
        "too small"
    );
    // Nearly along the plane, within 0.0002 of it end to end: its determinant is under 0.0002.
    let e = 0.000_02_f32;
    let along = v(1.0 / (1.0 + e * e).sqrt(), 0.0, -e / (1.0 + e * e).sqrt());
    assert_eq!(
        trace(v(0.0, 0.5, 0.000_01), along, 10.0, [floor], true),
        None
    );
    // The nearer of two.
    let high = [v(0.0, 0.0, 5.0), v(2.0, 0.0, 5.0), v(0.0, 2.0, 5.0)];
    let hit = trace(v(0.5, 0.5, 10.0), down, 100.0, [floor, high], false).expect("met");
    assert!((hit.fraction - 0.05).abs() < 1e-6);
    // Shares a half and an eighth: (0.25, 0.75).
    let hit = trace(v(1.0, 0.25, 1.0), down, 10.0, [floor], false).expect("met");
    assert!((hit.point.x - 0.25).abs() < 1e-5 && (hit.point.y - 0.75).abs() < 1e-5);
}

/// The lights standing outdoors lay their pools on the land whenever the land is drawn: with the
/// camera outdoors, or in a room that looks out, but not in a room that doesn't, and never with
/// light effects off.
///
/// Behaviour: draw.decal.light-pools-show-from-a-room-that-looks-out
#[test]
fn light_pools_show_from_a_room_that_looks_out() {
    use auberean_scene::lays_light_pools;
    const OUTDOORS: u32 = 0x7D64_0001;
    const ROOM: u32 = 0x7D64_0100;
    assert!(lays_light_pools(true, OUTDOORS, false));
    assert!(lays_light_pools(true, ROOM, true));
    assert!(!lays_light_pools(true, ROOM, false));
    assert!(!lays_light_pools(false, OUTDOORS, false));
    assert!(!lays_light_pools(false, ROOM, true));
}
