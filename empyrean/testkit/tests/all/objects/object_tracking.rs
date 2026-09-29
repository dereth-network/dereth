//! ACE: Source/ACE.Server/WorldObjects/Player_Tracking.cs::TrackObject
//! Object tracking through game messages.
//! Fixture: shared virtual-time server state and recorded content where enabled.

mod visibility {
    //! ACE: Source/ACE.Server/WorldObjects/Player_Tracking.cs::TrackObject
    use crate::support::tracking_world::*;

    /// A gem on the ground both players see is destroyed: each gets one DeleteObject for it, carrying
    /// the gem's instance sequence, queued on its own action queue and sent on its next tick.
    #[test]
    fn a_destroyed_object_is_deleted_for_every_player_who_knew_it() {
        let mut ts = server();
        let a = join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 20.0));
        let b = join(&mut ts, "bravo", BRAVO, "Bravo", at(24.0, 20.0));
        let gem = on_ground(&mut ts.world, GEM, at(22.0, 20.0));
        ts.advance(0.5);
        assert!(
            knows(&ts.world, ALPHA, gem) && knows(&ts.world, BRAVO, gem),
            "both players track the gem"
        );

        let (na, nb) = (ts.received_raw(a).len(), ts.received_raw(b).len());
        world_object::destroy(&mut ts.world, gem, true, false);
        assert!(
            ts.world.objects.get(gem).is_none(),
            "the gem left the store"
        );
        // the removal waits on each player's own queue (`Player.EnqueueAction`)
        assert!(
            deleted(&got(&ts, a, na)).is_empty(),
            "nothing is sent before the players tick"
        );
        ts.advance(0.5);

        for (id, n) in [(a, na), (b, nb)] {
            let g = got(&ts, id, n);
            assert_eq!(deleted(&g), [gem.full()], "one DeleteObject for the gem");
            let d: ItemDeleteObject =
                decode(&g.iter().find(|(k, _)| *k == DELETE_OBJECT).unwrap().1);
            assert_eq!(
                d.instance_sequence, 0,
                "an object's instance sequence (never set but for players)"
            );
        }
    }

    /// A drudge wielding a sword is destroyed: the player who saw it gets the drudge's DeleteObject,
    /// then the sword's (`RemoveTrackedEquippedObject`: a melee weapon is selectable), although the
    /// sword was destroyed first and both have left the store when the player's action runs.
    #[test]
    fn a_destroyed_creature_takes_its_wielded_weapon_with_it() {
        let mut ts = server();
        let a = join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 20.0));
        let w = &mut ts.world;
        let drudge = new_object(w, DRUDGE);
        let sword = new_object(w, SWORD);
        assert!(
            ce::try_equip_object(w, drudge, sword, EquipMask::MeleeWeapon),
            "the drudge wields the sword"
        );
        w.objects
            .get_mut(drudge)
            .unwrap()
            .set_location(Some(at(22.0, 20.0)));
        assert!(landblock_manager::add_object(w, drudge, false));
        ts.advance(0.5);
        assert!(knows(&ts.world, ALPHA, drudge));

        let n = ts.received_raw(a).len();
        world_object::destroy(&mut ts.world, drudge, true, false);
        assert!(
            ts.world.objects.get(drudge).is_none() && ts.world.objects.get(sword).is_none(),
            "both left the store"
        );
        ts.advance(0.5);
        assert_eq!(deleted(&got(&ts, a, n)), [drudge.full(), sword.full()]);
    }

    /// Alpha picks up a gem Bravo also sees: the landblock removal says `fromPickup`, so Bravo gets a
    /// PickupEvent for it (the object stays known to the client), and no DeleteObject.
    #[test]
    fn a_picked_up_object_is_a_pickup_event_for_the_onlookers() {
        let mut ts = server();
        let a = join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 20.0));
        let b = join(&mut ts, "bravo", BRAVO, "Bravo", at(21.0, 21.0));
        let gem = on_ground(&mut ts.world, GEM, at(20.3, 20.0));
        ts.advance(0.5);

        let nb = ts.received_raw(b).len();
        ts.send_game_action(
            a,
            &InventoryPutItemInContainer {
                item: ObjectId(gem.full()),
                container: ObjectId(ALPHA),
                slot: 0,
            },
        );
        ts.advance(2.0);
        assert!(
            ts.world
                .objects
                .get(gem)
                .is_some_and(|o| o.current_landblock.is_none()),
            "the gem is in Alpha's pack"
        );

        let g = got(&ts, b, nb);
        let pickups: Vec<InventoryPickupEvent> = g
            .iter()
            .filter(|(k, _)| *k == PICKUP_EVENT)
            .map(|(_, b)| decode(b))
            .collect();
        assert_eq!(pickups.len(), 1, "one PickupEvent for Bravo");
        assert_eq!(pickups[0].id.0, gem.full());
        assert!(deleted(&g).is_empty(), "a picked-up object is not deleted");
    }

    /// A player who has forgotten an object still gets its DeleteObject when it goes, while it is in
    /// the object's 3×3 landblocks. Re-pinned for V260/V278/V286/V287/V308 stage 2c (retail, V286): ACE broadcast only to
    /// `KnownPlayers`, so Bravo heard nothing; retail's deletes (corpse decay) reached players who had
    /// forgotten the object.
    #[test]
    fn a_player_who_forgot_the_object_still_gets_its_delete() {
        let mut ts = server();
        let a = join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 20.0));
        let b = join(&mut ts, "bravo", BRAVO, "Bravo", at(22.0, 20.0));
        let gem = on_ground(&mut ts.world, GEM, at(21.0, 20.0));
        ts.advance(0.5);
        // Bravo forgets the gem, as its ObjectMaint does once the gem has been out of sight for the
        // destruction delay (ObjectMaint.RemoveObject, both directions).
        let me = phys_ext::physics_obj(&ts.world, ObjectGuid::new(BRAVO)).unwrap();
        let it = phys_ext::physics_obj(&ts.world, gem).unwrap();
        empyrean_world::physics::object_maint::remove_object(&mut ts.world, me, it, true);
        assert!(!knows(&ts.world, BRAVO, gem));

        let (na, nb) = (ts.received_raw(a).len(), ts.received_raw(b).len());
        world_object::destroy(&mut ts.world, gem, true, false);
        ts.advance(0.5);
        assert_eq!(deleted(&got(&ts, a, na)), [gem.full()]);
        assert_eq!(
            deleted(&got(&ts, b, nb)),
            [gem.full()],
            "Bravo no longer knew the gem, and is in its reach"
        );
    }

    /// `Player.HandleCloak`: Bravo, who sees Alpha, gets Alpha's DeleteObject; the CreateObject a
    /// second later goes only to players with admin vision (Alpha is `Visibility` by then), so Bravo
    /// gets none. `Player.DeCloak`: its DeleteObject is skipped for Bravo (still invisible), and the
    /// CreateObject half a second after `Visibility` clears reaches Bravo.
    #[test]
    fn cloaking_deletes_the_player_for_onlookers_and_decloaking_creates_it_again() {
        let mut ts = server();
        let _a = join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 20.0));
        let b = join(&mut ts, "bravo", BRAVO, "Bravo", at(22.0, 20.0));
        ts.advance(0.5);
        let alpha = ObjectGuid::new(ALPHA);
        assert!(knows(&ts.world, BRAVO, alpha));

        let n = ts.received_raw(b).len();
        empyrean_world::world_objects::player_tracking::handle_cloak(&mut ts.world, alpha);
        ts.advance(1.5);
        let g = got(&ts, b, n);
        assert_eq!(deleted(&g), [ALPHA], "Bravo loses Alpha");
        assert!(
            !g.iter().any(|(k, _)| *k == 0xF745),
            "no CreateObject without admin vision"
        );
        assert!(ts.world.objects.get(alpha).unwrap().visibility());

        let n = ts.received_raw(b).len();
        empyrean_world::world_objects::player_tracking::de_cloak(&mut ts.world, alpha);
        ts.advance(1.5);
        let g = got(&ts, b, n);
        assert!(
            deleted(&g).is_empty(),
            "the decloak's DeleteObject skips Bravo (Alpha was still invisible)"
        );
        assert_eq!(
            g.iter().filter(|(k, _)| *k == 0xF745).count(),
            1,
            "Alpha is created again for Bravo"
        );
        assert!(!ts.world.objects.get(alpha).unwrap().visibility());
    }

    /// `Player.HandlePreTeleportVisibility` with `teleport_visibility_fix` 1 (players): before Alpha
    /// teleports to another cell, Alpha and Bravo forget each other both ways and each client gets
    /// the other's DeleteObject. With the fix off (the default) nothing happens.
    #[test]
    fn the_teleport_visibility_fix_forgets_both_ways() {
        let mut ts = server();
        let a = join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 20.0));
        let b = join(&mut ts, "bravo", BRAVO, "Bravo", at(22.0, 20.0));
        ts.advance(0.5);
        let (alpha, bravo) = (ObjectGuid::new(ALPHA), ObjectGuid::new(BRAVO));
        let far =
            Position::from_components(LB | 0x0010, 100.0, 100.0, 0.0, 0.0, 0.0, 0.0, 1.0, false);

        let (na, nb) = (ts.received_raw(a).len(), ts.received_raw(b).len());
        empyrean_world::world_objects::player_tracking::handle_pre_teleport_visibility(
            &mut ts.world,
            alpha,
            &far,
        );
        ts.advance(0.2);
        assert!(
            deleted(&got(&ts, a, na)).is_empty() && deleted(&got(&ts, b, nb)).is_empty(),
            "off by default"
        );
        assert!(knows(&ts.world, ALPHA, bravo));

        assert!(empyrean_world::managers::property_manager::modify_long(
            &ts.world,
            "teleport_visibility_fix",
            1
        ));
        empyrean_world::world_objects::player_tracking::handle_pre_teleport_visibility(
            &mut ts.world,
            alpha,
            &far,
        );
        ts.advance(0.2);
        assert_eq!(deleted(&got(&ts, a, na)), [BRAVO]);
        assert_eq!(deleted(&got(&ts, b, nb)), [ALPHA]);
        assert!(
            !knows(&ts.world, ALPHA, bravo) && !knows(&ts.world, BRAVO, alpha),
            "forgotten both ways"
        );
    }
}
