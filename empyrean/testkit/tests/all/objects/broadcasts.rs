//! ACE: Source/ACE.Server/WorldObjects/WorldObject_Networking.cs::EnqueueBroadcast
//! Broadcasts through game messages.
//! Fixture: shared virtual-time server state and recorded content where enabled.

mod updates {
    //! ACE: Source/ACE.Server/WorldObjects/WorldObject_Networking.cs::EnqueueBroadcast
    use crate::support::object_use_world::*;

    /// `WorldObject.PlaySoundEffect` (WorldObject.cs): a `GameMessageSound` (0xF750) broadcast to the
    /// object's watchers and itself.
    #[test]
    fn a_sound_effect_is_broadcast() {
        let mut ts = server();
        let (alpha, _) = join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 20.0));
        ts.advance(0.5);
        let p = ObjectGuid::new(ALPHA);
        let a = during(&mut ts, alpha, 0.2, |ts| {
            empyrean_world::world_objects::world_object::play_sound_effect(
                &mut ts.world,
                p,
                empyrean_entity::enums::Sound::ItemManaDepleted,
                p,
                1.0,
            );
        });
        assert_eq!(all(&a, 0xF750).len(), 1, "{:04X?}", kinds(&a));
    }

    /// `Creature.BroadcastMoveTo` (Creature_Navigation.cs): a player who starts seeing a creature
    /// with no attack target is sent its MoveToPosition (type 7) home.
    #[test]
    fn a_new_viewer_is_sent_the_creatures_move_home() {
        use dereth_protocol::movement::MovementSetObjectMovement;

        let mut ts = server();
        join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 20.0));
        let (bravo, _) = join(&mut ts, "bravo", ALPHA + 1, "Bravo", at(24.0, 20.0));
        ts.advance(0.5);
        let p = ObjectGuid::new(ALPHA);
        obj_mut(&mut ts, p).set_position(
            empyrean_entity::enums::PositionType::Home,
            Some(at(30.0, 20.0)),
        );

        let b = during(&mut ts, bravo, 0.1, |ts| {
            empyrean_world::dispatch::broadcast_move_to::broadcast_move_to(
                &mut ts.world,
                p,
                ObjectGuid::new(ALPHA + 1),
            )
        });
        let moves: Vec<u8> = all(&b, 0xF74C)
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
        assert_eq!(moves, [7], "one MoveToPosition");
    }

    /// `WorldObject.EnqueueBroadcastUpdateObject` (WorldObject.cs): an UpdateObject (0xF7DB) of the
    /// object reaches its watchers.
    #[test]
    fn an_update_object_is_broadcast() {
        let mut ts = server();
        let (alpha, _) = join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 20.0));
        let stone = on_ground(&mut ts, LIFESTONE, at(22.0, 20.0));
        ts.advance(0.5);
        let a = during(&mut ts, alpha, 0.2, |ts| {
            empyrean_world::world_objects::world_object::enqueue_broadcast_update_object(
                &mut ts.world,
                stone,
            )
        });
        assert_eq!(all(&a, 0xF7DB).len(), 1, "{:04X?}", kinds(&a));
    }
}

mod object_messages {
    //! ACE: Source/ACE.Server/WorldObjects/WorldObject_Networking.cs::EnqueueBroadcast
    use crate::support::object_message_world::*;

    /// Objsend is limited by the players prev obj send.
    #[test]
    fn objsend_is_limited_by_the_players_prev_obj_send() {
        let mut ts = server();
        empyrean_command::command_manager::initialize(None);
        let (alpha, _) = join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 20.0));
        let me = ObjectGuid::new(ALPHA);
        assert_eq!(
            empyrean_world::world_objects::player::fields(&ts.world, me).prev_obj_send,
            empyrean_common::dotnet::DotNetDateTime::MIN_VALUE
        );

        let too_recently = |ts: &mut empyrean_testkit::TestServer| -> Vec<String> {
            let a = during(ts, alpha, 0.5, |ts| {
                ts.send_game_action(
                    alpha,
                    &CommunicationTalk {
                        message: "@objsend".to_owned(),
                    },
                )
            });
            all(&a, TRANSIENT)
                .iter()
                .map(|m| m.decode::<CommunicationTransientString>().text)
                .collect()
        };

        assert!(too_recently(&mut ts).is_empty(), "the first @objsend runs");
        let sent_at = empyrean_world::world_objects::player::fields(&ts.world, me).prev_obj_send;
        assert!(
            sent_at > empyrean_common::dotnet::DotNetDateTime::MIN_VALUE,
            "PrevObjSend is set"
        );

        ts.advance(60.0);
        assert_eq!(
            too_recently(&mut ts),
            ["You have used this command too recently!"]
        );
        assert_eq!(
            empyrean_world::world_objects::player::fields(&ts.world, me).prev_obj_send,
            sent_at,
            "a refused @objsend leaves it"
        );

        ts.advance(300.0);
        assert!(
            too_recently(&mut ts).is_empty(),
            "5 minutes later it runs again"
        );
        assert!(
            empyrean_world::world_objects::player::fields(&ts.world, me).prev_obj_send > sent_at
        );
    }

    /// Attackable broadcasts the public update.
    #[test]
    fn attackable_broadcasts_the_public_update() {
        let mut ts = server();
        empyrean_command::command_manager::initialize(None);
        let (alpha, _) = join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 20.0));
        let (bravo, _) = join(&mut ts, "bravo", ALPHA + 1, "Bravo", at(22.0, 20.0));
        ts.world
            .objects
            .get_mut(ObjectGuid::new(ALPHA))
            .unwrap()
            .set_is_admin_prop(true);
        ts.advance(0.5);

        let from_bravo = ts.received_raw(bravo).len();
        let a = during(&mut ts, alpha, 0.5, |ts| {
            ts.send_game_action(
                alpha,
                &CommunicationTalk {
                    message: "@attackable off".to_owned(),
                },
            )
        });
        assert_eq!(
            ts.world
                .objects
                .get(ObjectGuid::new(ALPHA))
                .unwrap()
                .get_property(empyrean_entity::enums::PropertyBool::Attackable),
            Some(false)
        );
        let own: Vec<&[u8]> = all(&a, PUBLIC_BOOL).iter().map(|g| &g.blob[..]).collect();
        assert_eq!(own.len(), 1, "the advocate's own copy");
        let theirs: Vec<Vec<u8>> = ts.received_raw(bravo)[from_bravo..]
            .iter()
            .filter(|m| m.opcode == PUBLIC_BOOL)
            .map(|m| {
                let mut blob = m.opcode.to_le_bytes().to_vec();
                blob.extend_from_slice(&m.body);
                blob
            })
            .collect();
        assert_eq!(theirs.len(), 1, "the bystander's copy (EnqueueBroadcast)");
        // opcode, sequence (1), guid, property, value: the same message body
        assert_eq!(&theirs[0][5..], &own[0][5..]);
        assert_eq!(u32::from_le_bytes(own[0][5..9].try_into().unwrap()), ALPHA);
    }

    /// `WorldObject.EnqueueBroadcast` (WorldObject_Networking.cs): an object without a `PhysicsObj`
    /// relays through `Container` (the field `TryAddToInventory` sets), not through its `ContainerId`
    /// property: an item in the pack reaches its owner; an object that only names a container in
    /// `ContainerId` (a vendor's shop item) goes nowhere.
    #[test]
    fn a_broadcast_from_a_packed_item_relays_through_its_container_field() {
        use empyrean_world::network::game_messages::messages::game_message_sound::game_message_sound;
        use empyrean_world::world_objects::world_object_networking::enqueue_broadcast;
        let mut ts = server();
        let (alpha, _) = join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 20.0));
        let wand = in_pack(&mut ts, WAND);
        assert_eq!(
            ts.world
                .objects
                .get(wand)
                .unwrap()
                .wo
                .world_object_properties
                .container,
            Some(ObjectGuid::new(ALPHA))
        );

        let sounds = |ts: &mut empyrean_testkit::TestServer, g: ObjectGuid| {
            let a = during(ts, alpha, 0.2, |ts| {
                let msg = game_message_sound(g, empyrean_entity::enums::Sound(0x77), 1.0);
                let _ = enqueue_broadcast(&mut ts.world, g, true, &[msg]);
            });
            all(&a, SOUND).len()
        };
        assert_eq!(sounds(&mut ts, wand), 1, "relayed to the owner");

        let loose = crate::support::object_use_world::new_object(&mut ts.world, WAND);
        ts.world
            .objects
            .get_mut(loose)
            .unwrap()
            .set_container_id(Some(ALPHA));
        assert_eq!(
            sounds(&mut ts, loose),
            0,
            "ContainerId alone relays nothing"
        );
    }
}
