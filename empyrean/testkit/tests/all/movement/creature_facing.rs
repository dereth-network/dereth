//! ACE: Source/ACE.Server/WorldObjects/Creature_Navigation.cs::TurnTo
//! Creature facing through game messages.
//! Fixture: shared virtual-time server state and recorded content where enabled.

mod facing {
    //! ACE: Source/ACE.Server/WorldObjects/Creature_Navigation.cs::TurnTo
    use crate::support::object_use_world::*;

    /// `Creature.Rotate` (via `Creature.TurnToObject`): the TurnToObject motion (movement type 8)
    /// reaches the player, and when the rotate delay has run the server-side Location faces the
    /// target (`Location.Rotate(GetDirection(...))`); the delay is 0 without a motion table's turn
    /// speed.
    #[test]
    fn a_creature_rotates_to_face_its_target() {
        use dereth_protocol::movement::MovementSetObjectMovement;

        let mut ts = server();
        let (alpha, _) = join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 20.0));
        let target = on_ground(&mut ts, LIFESTONE, at(25.0, 20.0)); // due east
        ts.advance(0.5);
        let p = ObjectGuid::new(ALPHA);

        let mut delay = f32::NAN;
        let a = during(&mut ts, alpha, 0.5, |ts| {
            delay = empyrean_world::dispatch::rotate::rotate(&mut ts.world, p, target)
        });
        assert_eq!(delay, 0.0);
        let turns: Vec<u8> = all(&a, 0xF74C)
            .iter()
            .map(|m| m.decode::<MovementSetObjectMovement>())
            .filter(|m| m.id.0 == ALPHA)
            .map(|m| {
                m.decoded_movement()
                    .expect("a movement buffer")
                    .body
                    .movement_type
            })
            .collect();
        assert_eq!(turns, [8], "one TurnToObject");
        let dir = obj(&ts, p).location().expect("placed").get_current_dir();
        assert!(
            (dir.x - 1.0).abs() < 1e-4 && dir.y.abs() < 1e-4,
            "faces east: {dir:?}"
        );
    }

    /// `Creature.TurnTo(Position)` (the emote system's turn): a TurnToHeading motion (movement type 9)
    /// reaches the player and, after the rotate delay, the Location faces the position's rotation.
    #[test]
    fn a_creature_turns_to_a_positions_heading() {
        use dereth_protocol::movement::MovementSetObjectMovement;

        let mut ts = server();
        let (alpha, _) = join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 20.0));
        ts.advance(0.5);
        let p = ObjectGuid::new(ALPHA);
        // facing east: a quarter turn about Z (`Position.Rotate` of +X)
        let mut east = at(20.0, 20.0);
        east.rotate(empyrean_common::dotnet::Vector3::new(1.0, 0.0, 0.0));

        let a = during(&mut ts, alpha, 0.5, |ts| {
            let _ = empyrean_world::world_objects::creature_navigation::turn_to_position(
                &mut ts.world,
                p,
                &east,
            );
        });
        let turns: Vec<u8> = all(&a, 0xF74C)
            .iter()
            .map(|m| m.decode::<MovementSetObjectMovement>())
            .filter(|m| m.id.0 == ALPHA)
            .map(|m| {
                m.decoded_movement()
                    .expect("a movement buffer")
                    .body
                    .movement_type
            })
            .collect();
        assert_eq!(turns, [9], "one TurnToHeading");
        let dir = obj(&ts, p).location().expect("placed").get_current_dir();
        assert!(
            (dir.x - 1.0).abs() < 1e-4 && dir.y.abs() < 1e-4,
            "faces east: {dir:?}"
        );
    }

    /// `WorldObject.IsDirectVisible` (WorldObject.cs): the sight probe's sweep from eye level reaches
    /// a creature standing in the open; a target more than a landblock away is never visible.
    #[test]
    fn direct_line_of_sight_reaches_a_nearby_creature() {
        let mut ts = server();
        join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 20.0));
        join(&mut ts, "bravo", ALPHA + 1, "Bravo", at(24.0, 20.0));
        ts.advance(0.5);
        // the sight probe's setup: a small synthetic sphere (no dats here)
        let probe = dereth_physics::SetupGeometry {
            spheres: vec![dereth_physics::geom::Sphere::new(
                dereth_primitives::Vec3::new(0.0, 0.0, 0.1),
                0.1,
            )],
            radius: 0.1,
            height: 0.2,
            ..dereth_physics::SetupGeometry::default()
        };
        empyrean_world::physics::phys_ext::register_setup(&mut ts.world, 0x0200_0124, probe);
        let (a, b) = (ObjectGuid::new(ALPHA), ObjectGuid::new(ALPHA + 1));
        assert!(
            empyrean_world::world_objects::world_object::is_direct_visible(&mut ts.world, a, b)
        );

        let far =
            Position::from_components(0xA9B7_0001, 20.0, 20.0, 0.0, 0.0, 0.0, 0.0, 1.0, false);
        assert!(
            !empyrean_world::world_objects::world_object::is_direct_visible_position(
                &mut ts.world,
                a,
                &far
            )
        );
    }
}
