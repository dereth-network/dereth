//! ACE: Source/ACE.Server/WorldObjects/Player.cs::HandleActionQueryItemMana
//! Lifestone through game messages.
//! Fixture: shared virtual-time server state and recorded content where enabled.

mod binding {
    //! ACE: Source/ACE.Server/WorldObjects/Player.cs::HandleActionQueryItemMana
    use crate::support::inventory_action_world::*;

    /// A lifestone can be used twice over the wire.
    #[test]
    fn a_lifestone_can_be_used_twice_over_the_wire() {
        let mut ts = server();
        let (alpha, _) = join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 20.0));
        let stone = on_ground(&mut ts, LIFESTONE, at(21.5, 20.0));
        ts.advance(0.5);

        for i in 0..3 {
            let a = during(&mut ts, alpha, 1.0, |ts| {
                ts.send_game_action(
                    alpha,
                    &InventoryUseEvent {
                        object: ObjectId(stone.full()),
                    },
                )
            });
            assert_eq!(
                chats(&a),
                ["You have attuned your spirit to this Lifestone."],
                "use {i}: {:04X?}",
                kinds(&a)
            );
            assert_eq!(use_dones(&a), [0], "use {i}");
        }
    }
}

#[cfg(feature = "real-content")]
mod binding_real {
    //! ACE: Source/ACE.Server/WorldObjects/Player.cs::HandleActionQueryItemMana
    use crate::support::inventory_action_world::real::*;

    #[test]
    fn a_pk_character_can_use_the_life_stone_twice() {
        let mut l = create_and_enter();
        teleport(&mut l, "@teleloc 0xA9B40019 84 7.1 94");
        let stone = *l
            .nearby_wcid(LIFE_STONE)
            .first()
            .expect("Holtburg's life stone");
        let at = l.location_of(stone);
        teleport(
            &mut l,
            &format!(
                "@teleloc 0x{:08X} {} {} {}",
                at.cell(),
                at.position_x + 1.5,
                at.position_y,
                at.position_z
            ),
        );
        let g = l.g;
        let o = l.ts.world.objects.get_mut(g).unwrap();
        o.set_property(
            PropertyInt::PlayerKillerStatus,
            i32::try_from(PlayerKillerStatus::PK.0).unwrap(),
        );
        assert!(
            empyrean_world::world_objects::player_tick::fast_tick(&l.ts.world, g),
            "a PK player is FastTick"
        );

        for i in 0..3 {
            let mark = l.mark();
            l.action(&InventoryUseEvent {
                object: ObjectId(stone.full()),
            });
            // the Sanctuary motion plays before the attunement (`Lifestone.ActOnUse`)
            l.advance(12.0);
            let chats: Vec<String> = l
                .since::<CommunicationTextboxString>(mark)
                .into_iter()
                .map(|c| c.text)
                .collect();
            assert!(
                chats.iter().any(|c| c.contains("attuned your spirit")),
                "use {i}: {chats:?}"
            );
            let dones: Vec<u32> = l
                .since::<ItemUseDone>(mark)
                .into_iter()
                .map(|d| d.failure_type)
                .collect();
            assert_eq!(dones, [0], "use {i}");
        }
    }
}
