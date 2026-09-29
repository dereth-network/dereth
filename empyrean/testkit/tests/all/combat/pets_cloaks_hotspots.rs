//! ACE: Source/ACE.Server/WorldObjects/PetDevice.cs::SummonCreature
//! Pets cloaks hotspots through game messages.
//! Fixture: shared virtual-time server state and recorded content where enabled.

mod damage {
    //! ACE: Source/ACE.Server/WorldObjects/PetDevice.cs::SummonCreature
    use crate::support::creature_and_item_world::*;

    /// `PetDevice.ActOnUse` -> `SummonCreature` -> `CombatPet.Init` (`Pet.Init`), then the pet's
    /// monster AI (`CombatPet.HandleFindTarget`/`FindNextTarget`/`GetNearbyMonsters`, 4.7's chase and
    /// melee) finds the drudge beside its owner and strikes it; `PetOnAttackMonster` wakes the drudge,
    /// which turns on the pet.
    #[test]
    fn a_summoned_combat_pet_fights_beside_its_owner() {
        // the pet appears 3 m ahead of its owner (Position.InFrontOf), 5 cm up; the drudge stands
        // 15 m beyond it, out of melee range, so the pet must land and run to it
        let (mut ts, client) = server_with_monster_at(130.0);
        let essence = in_pack(&mut ts, ESSENCE_WCID);
        let from = ts.received_raw(client).len();

        dispatch::act_on_use::act_on_use(&mut ts.world, essence, PLAYER);
        ts.advance(0.1);

        // the pet is out: the owner's CurrentActivePet, the device's Pet and the pet's owner links
        let pet = empyrean_world::world_objects::player_use::fields(&ts.world, PLAYER)
            .current_active_pet
            .expect("a pet was summoned");
        assert!(obj(&ts, pet).is_combat_pet());
        assert_eq!(obj(&ts, essence).pet(), Some(pet.full()));
        assert_eq!(obj(&ts, pet).pet_device(), Some(essence.full()));
        assert_eq!(obj(&ts, pet).pet_owner(), Some(PLAYER.full()));
        assert_eq!(
            empyrean_world::world_objects::pet::p_pet_owner(&ts.world, pet),
            Some(PLAYER)
        );
        assert_eq!(
            dispatch::name::name(&ts.world, pet).as_deref(),
            Some("Alpha's Wolf")
        );
        assert!(obj(&ts, pet).no_corpse(), "Pet.Init: pets leave no corpse");
        assert!(
            obj(&ts, pet).current_landblock.is_some(),
            "EnterWorld placed it"
        );
        // CombatPet.Init: melee stance, awake
        assert_eq!(
            empyrean_world::world_objects::creature_combat::combat_mode(&ts.world, pet),
            empyrean_entity::enums::CombatMode::Melee
        );
        assert!(empyrean_world::world_objects::monster_awareness::is_awake(
            &ts.world, pet
        ));
        // PetDevice.ActOnUse: a charge used, told to the owner (PublicUpdatePropertyInt Structure 4)
        assert_eq!(obj(&ts, essence).structure(), Some(4));
        let sent = got(&ts, client, from);
        assert!(
            sent.iter().any(|m| m.kind == 0x02CE),
            "the public Structure update: {:?}",
            sent.iter().map(|m| m.kind).collect::<Vec<_>>()
        );

        // the pet targets the drudge and fights it
        let drudge_health = health(&ts, MONSTER);
        // it lands (Contact) and closes in with a MoveTo whose nodes are pending
        let h = phys_ext::physics_obj(&ts.world, pet).unwrap();
        assert!(
            ts.run_until(2.0, |ts| ts
                .world
                .physics
                .get(h)
                .is_some_and(|o| o.transient_state.in_contact())),
            "the pet lands"
        );
        assert!(
            ts.run_until(2.0, |ts| phys_ext::move_to_pending_actions(&ts.world, h)
                > 0),
            "the pet's MoveTo has pending actions"
        );
        let start_y = ts.world.physics.get(h).unwrap().position.frame.origin.y;
        assert!(
            ts.run_until(30.0, |ts| health(ts, MONSTER) < drudge_health),
            "the pet strikes the drudge"
        );
        assert!(
            ts.world.physics.get(h).unwrap().position.frame.origin.y > start_y + 5.0,
            "it ran to the drudge"
        );
        assert_eq!(
            empyrean_world::world_objects::monster_combat::attack_target(&ts.world, pet),
            Some(MONSTER)
        );
        let history = empyrean_world::entity::damage_history::of(&ts.world, MONSTER);
        assert_eq!(history.last_damager().map(|d| d.guid), Some(pet));
        // Pet_Monster.PetOnAttackMonster: the drudge wakes and turns on the pet
        assert!(empyrean_world::world_objects::monster_awareness::is_awake(
            &ts.world, MONSTER
        ));
        assert_eq!(
            empyrean_world::world_objects::monster_combat::attack_target(&ts.world, MONSTER),
            Some(pet)
        );

        // stowing: destroying the pet clears the owner's and the device's links (WorldObject.Destroy)
        empyrean_world::world_objects::world_object::destroy(&mut ts.world, pet, true, false);
        assert_eq!(
            empyrean_world::world_objects::player_use::fields(&ts.world, PLAYER).current_active_pet,
            None
        );
        assert_eq!(obj(&ts, essence).pet(), None);
    }

    /// `Player.TakeDamage` with a -200 cloak (`CloakWeaveProc` 2, item level 5): the first seed whose
    /// draw is under the proc chance procs (`Cloak.RollProc`), the damage is reduced by 200
    /// (`GetReducedAmount`), the player is told (`ShowMessage`) and the cloak's cooldown starts; a
    /// seed above the chance does not proc.
    #[test]
    fn a_cloak_procs_on_a_hit_per_a_seeded_roll() {
        let (mut ts, client) = server();
        let cloak = in_pack(&mut ts, CLOAK_WCID);
        assert!(container::try_remove_from_inventory(
            &mut ts.world,
            PLAYER,
            cloak,
            false
        ));
        assert!(creature_equipment::try_equip_object(
            &mut ts.world,
            PLAYER,
            cloak,
            EquipMask::Cloak
        ));

        // 90 of 100 health: percent 0.9, two thirds for a -200 cloak (0.6); MaxProcBase200 0.15 +
        // 4 * 0.0125 caps the chance at 0.2
        let chance = f64::from((0.15f32 + 4.0 * 0.0125).min(0.9f32 * (2.0f32 / 3.0)));
        let first_draw = |seed: u64| DotNetRandom::new(i32::try_from(seed).unwrap()).next_double();
        let procs = (1u64..).find(|&s| first_draw(s) < chance).unwrap();
        let misses = (1u64..).find(|&s| first_draw(s) >= chance).unwrap();

        // a miss: the full hit lands
        ThreadSafeRandom::seed(misses);
        let before = health(&ts, PLAYER);
        let taken = player_combat::take_damage(
            &mut ts.world,
            PLAYER,
            Some(MONSTER),
            DamageType::Slash,
            90.0,
            empyrean_world::entity::body_part::BodyPart::Chest,
            false,
            empyrean_entity::enums::AttackConditions::None,
        );
        assert_eq!(taken, 90);
        assert_eq!(health(&ts, PLAYER), before - 90);
        assert_eq!(
            obj(&ts, cloak).use_timestamp(),
            None,
            "no proc, no cooldown"
        );

        // heal, then a proc: 90 - 200 leaves nothing
        let hv = obj(&ts, PLAYER).health();
        empyrean_world::world_objects::creature_vitals::update_vital_delta(
            &mut ts.world,
            PLAYER,
            hv,
            90,
        );
        ts.advance(0.1);
        let from = ts.received_raw(client).len();
        ThreadSafeRandom::seed(procs);
        let now = ts.world.now.unix_time;
        let taken = player_combat::take_damage(
            &mut ts.world,
            PLAYER,
            Some(MONSTER),
            DamageType::Slash,
            90.0,
            empyrean_world::entity::body_part::BodyPart::Chest,
            false,
            empyrean_entity::enums::AttackConditions::None,
        );
        ts.advance(0.1);
        assert_eq!(taken, 0, "the cloak absorbed the hit");
        assert_eq!(health(&ts, PLAYER), 100);
        assert_eq!(
            obj(&ts, cloak).use_timestamp(),
            Some(now),
            "RollProc stamps the cooldown"
        );
        let lines = chats(&got(&ts, client, from));
        assert!(
            lines.contains(&"Your cloak reduced the damage from 90 down to 0!".to_owned()),
            "{lines:?}"
        );
    }

    /// `Hotspot.OnCollideObject` (as a player standing in it reaches it) starts the action loop; every
    /// `CycleTime` (2 s, no variance) `Activate` damages each creature still touching it (10 fire,
    /// resisted) and tells the player its `ActivationTalk`; a player who walked out is dropped.
    #[test]
    fn a_hotspot_damages_a_player_standing_in_it() {
        let (mut ts, client) = server();
        let hot = new_object(&mut ts.world, HOTSPOT_WCID);
        let spot = obj(&ts, PLAYER).location().unwrap();
        ts.world
            .objects
            .get_mut(hot)
            .unwrap()
            .set_location(Some(spot));
        assert!(
            lm::add_object(&mut ts.world, hot, false),
            "the hotspot joins its landblock"
        );

        let from = ts.received_raw(client).len();
        let before = health(&ts, PLAYER);
        dispatch::on_collide_object::on_collide_object(&mut ts.world, PLAYER, hot);
        if let empyrean_world::world_objects::kinds::KindData::Hotspot(d) = &obj(&ts, hot).kind {
            assert!(
                d.hotspot.creatures.contains(&PLAYER) && d.hotspot.action_loop,
                "the player is tracked, the loop armed"
            );
        } else {
            panic!("a hotspot");
        }
        ts.advance(2.1);
        assert_eq!(health(&ts, PLAYER), before - 10, "one activation");
        let lines = chats(&got(&ts, client, from));
        assert!(
            lines.contains(&"You burn for 10 damage!".to_owned()),
            "{lines:?}"
        );

        ts.advance(2.0);
        assert_eq!(
            health(&ts, PLAYER),
            before - 20,
            "the loop goes on while the player stands in it"
        );

        // the player walks away: the next activation drops it and the loop ends
        let away = empyrean_entity::Position::from_components(
            LB | 0x0001,
            100.0,
            140.0,
            20.0,
            0.0,
            0.0,
            0.0,
            1.0,
            false,
        );
        let h = phys_ext::physics_obj(&ts.world, PLAYER).unwrap();
        assert!(phys_ext::set_position(
            &mut ts.world,
            h,
            &phys_ext::to_physics_position(&away)
        ));
        ts.world
            .objects
            .get_mut(PLAYER)
            .unwrap()
            .set_location(Some(away));
        ts.advance(4.0);
        assert_eq!(health(&ts, PLAYER), before - 20, "no more damage");
    }
}
