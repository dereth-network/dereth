use super::*;
use Interface::{Classic, Retail};

/// Behaviour: radar.shared-roles-and-projection-variants
#[test]
fn roles_and_projection_preserve_interface_conventions() {
    let mut entry = RadarEntry {
        in_world: true,
        radar_enum: 4,
        bitfield: 0x4200,
        player_space: (-1.0, 1.0, 5.0),
        ..Default::default()
    };
    assert_eq!(color_role(Some(&entry), Classic), ColorRole::LifeStone);
    assert_eq!(color_role(Some(&entry), Retail), ColorRole::Vendor);
    entry.bitfield |= 0x40000;
    for face in [Classic, Retail] {
        assert_eq!(color_role(Some(&entry), face), ColorRole::Portal);
    }
    entry.blip_color = 99;
    for face in [Classic, Retail] {
        assert_eq!(color_role(Some(&entry), face), ColorRole::Override(99));
    }
    entry.bitfield |= 0x80;
    for face in [Classic, Retail] {
        assert_eq!(color_role(Some(&entry), face), ColorRole::Default);
        assert_eq!(shape_role(Some(&entry), None, face), ShapeRole::Hidden);
    }
    entry.bitfield = 0x8000_0000;
    let geometry = Geometry {
        radius: 50,
        center: (60.0, 60.0),
    };
    assert_eq!(
        project(&entry, geometry, 75.0, Classic),
        Some(Projection {
            x: 60,
            y: 60,
            bright: false
        })
    );
    assert_eq!(
        project(&entry, geometry, 75.0, Retail),
        Some(Projection {
            x: 59,
            y: 59,
            bright: false
        })
    );
    assert_eq!(shape_role(Some(&entry), None, Classic), ShapeRole::Ordinary);
    assert_eq!(shape_role(Some(&entry), None, Retail), ShapeRole::Ordinary);
    entry.is_player = true;
    entry.is_pk = true;
    entry.is_allegiance_member = true;
    assert_eq!(
        shape_role(Some(&entry), None, Classic),
        ShapeRole::Allegiance
    );
    assert_eq!(shape_role(Some(&entry), None, Retail), ShapeRole::Ordinary);
    let viewer = Some(Viewer {
        pk: true,
        pk_lite: false,
    });
    for face in [Classic, Retail] {
        assert_eq!(
            shape_role(Some(&entry), viewer, face),
            ShapeRole::Allegiance
        );
    }
    entry.is_allegiance_member = false;
    for face in [Classic, Retail] {
        assert_eq!(shape_role(Some(&entry), viewer, face), ShapeRole::Threat);
    }
    entry.is_fellow = true;
    entry.is_fellowship_leader = true;
    entry.blip_color = 0;
    for face in [Classic, Retail] {
        assert_eq!(
            shape_role(Some(&entry), viewer, face),
            ShapeRole::FellowshipLeader
        );
        assert_eq!(color_role(Some(&entry), face), ColorRole::Fellowship);
    }
    assert_eq!((radar_range(true), radar_range(false)), (75.0, 25.0));
    for face in [Classic, Retail] {
        for range in [75.0, 25.0] {
            entry.player_space = (range - 1.0, 0.0, 0.0);
            assert!(project(&entry, geometry, range, face).is_none());
            entry.player_space.0 -= 0.01;
            assert!(project(&entry, geometry, range, face).is_some());
        }
        entry.player_space = (0.0, 0.0, 4.999);
        assert!(project(&entry, geometry, 75.0, face).unwrap().bright);
        entry.player_space.2 = -5.0;
        assert!(!project(&entry, geometry, 75.0, face).unwrap().bright);
        entry.bitfield = 0x80;
        assert!(project(&entry, geometry, 75.0, face).is_none());
        entry.bitfield = 0;
        for radar_enum in [0, 1, 5] {
            entry.radar_enum = radar_enum;
            assert!(project(&entry, geometry, 75.0, face).is_none());
        }
        for radar_enum in [2, 3, 4] {
            entry.radar_enum = radar_enum;
            assert!(project(&entry, geometry, 75.0, face).is_some());
        }
        entry.is_self = true;
        assert!(project(&entry, geometry, 75.0, face).is_none());
        entry.is_self = false;
        entry.in_world = false;
        assert!(project(&entry, geometry, 75.0, face).is_none());
        entry.in_world = true;
    }
}
