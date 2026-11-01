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
