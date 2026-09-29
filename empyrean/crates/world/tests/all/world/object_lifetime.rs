//! ACE: Source/ACE.Server/WorldObjects/Player_Combat.cs::DamageTarget
//! Tests of object lifetime.
//! Fixture: isolated world state and the shared area fixtures.

mod lifetime {
    use crate::support::clock_and_object_lifetime::*;

    /// Damage target on a target destroyed since the swing returns none.
    #[test]
    fn damage_target_on_a_target_destroyed_since_the_swing_returns_none() {
        let mut w = world();
        let me = object(&mut w, PLAYER, Class::Player, WeenieType::Creature);
        let gone = ObjectGuid::new(0x8000_0123);
        assert!(player_combat::damage_target(&mut w, me, gone, None).is_none());
    }

    /// Drop unreferenced takes an unplaced creature and what it holds.
    #[test]
    fn drop_unreferenced_takes_an_unplaced_creature_and_what_it_holds() {
        let mut w = world();
        let drudge = object(&mut w, 0x8000_0010, Class::Creature, WeenieType::Creature);
        let wielded = object(
            &mut w,
            0x8000_0011,
            Class::MeleeWeapon,
            WeenieType::MeleeWeapon,
        );
        let pack_item = object(
            &mut w,
            0x8000_0012,
            Class::GenericObject,
            WeenieType::Generic,
        );
        o(&mut w, drudge)
            .creature
            .as_mut()
            .unwrap()
            .creature_equipment
            .equipped_objects
            .insert(wielded, ());
        o(&mut w, wielded).set_wielder_id(Some(drudge.full()));
        o(&mut w, drudge)
            .container
            .as_mut()
            .unwrap()
            .container
            .inventory
            .insert(pack_item, ());
        o(&mut w, pack_item).set_container_id(Some(drudge.full()));

        // held items are referenced by their holder: left alone on their own
        world_object::drop_unreferenced(&mut w, wielded);
        world_object::drop_unreferenced(&mut w, pack_item);
        assert!(w.objects.get(wielded).is_some() && w.objects.get(pack_item).is_some());

        // on a landblock: referenced
        o(&mut w, drudge).current_landblock = Some(LandblockId::new(0xA9B4_FFFF));
        world_object::drop_unreferenced(&mut w, drudge);
        assert!(w.objects.get(drudge).is_some(), "on a landblock: kept");

        o(&mut w, drudge).current_landblock = None;
        world_object::drop_unreferenced(&mut w, drudge);
        assert!(
            w.objects.get(drudge).is_none()
                && w.objects.get(wielded).is_none()
                && w.objects.get(pack_item).is_none()
        );
    }

    /// Release hands the offline player the players final biota.
    #[test]
    fn release_hands_the_offline_player_the_players_final_biota() {
        let mut w = world();
        let g = object(&mut w, PLAYER, Class::Player, WeenieType::Creature);
        o(&mut w, g).set_num_deaths(16);
        let at_switch = w.objects.get(g).unwrap().biota.clone();
        let offline = OfflinePlayer {
            biota: at_switch,
            guid: g,
            account: None,
            last_requested_database_save: DotNetDateTime::MIN_VALUE,
            changes_detected: false,
            allegiance: None,
            allegiance_node: None,
        };
        assert!(w.player_manager.offline_players.try_add(PLAYER, offline));

        // after the switch
        o(&mut w, g).set_num_deaths(17);
        player::release_logged_off_player(&mut w, g);

        assert!(w.objects.get(g).is_none(), "released");
        let off = player_manager::get_offline_player(&w, PLAYER).unwrap();
        let mut probe = WorldObject::allocate(Class::Player);
        probe.biota = off.biota.clone();
        assert_eq!(probe.num_deaths(), 17, "the offline player has the death");
        assert!(
            off.changes_detected,
            "V323: the hand-over is marked for saving (ACE: never saved)"
        );

        // the offline players' save writes it
        player_manager::save_offline_players_with_changes(&mut w);
        assert!(
            !player_manager::get_offline_player(&w, PLAYER)
                .unwrap()
                .changes_detected,
            "saved"
        );
    }
}
