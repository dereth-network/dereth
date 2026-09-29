//! ACE: Source/ACE.Server/WorldObjects/Corpse.cs::Corpse
//! Corpses through game messages.
//! Fixture: shared virtual-time server state and recorded content where enabled.

mod posture {
    //! ACE: Source/ACE.Server/WorldObjects/Corpse.cs::Corpse
    use crate::support::event_world::*;

    /// A drudge dies: its CreateObject'd corpse carries `NonCombat`/`Dead`, and the drudge itself had
    /// broadcast its death motion first.
    #[test]
    fn a_monster_corpse_is_created_lying_dead() {
        let mut ts = server();
        let alpha = join(
            &mut ts,
            "i15alpha",
            ALPHA,
            "Alpha Admin",
            at(20.0, 20.0),
            false,
        );
        let drudge = on_ground(&mut ts.world, DRUDGE, at(22.0, 20.0));
        ts.advance(0.5);
        let n = ts.received::<ItemCreateObject>(alpha).len();

        creature_death::die(&mut ts.world, drudge);
        assert!(
            ts.run_until(5.0, |ts| ts.world.objects.get(drudge).is_none()),
            "the drudge is destroyed"
        );
        ts.advance(0.5);

        let creates = ts.received::<ItemCreateObject>(alpha);
        let corpse = creates[n..]
            .iter()
            .find(|c| wcid_of(&ts, c.0.id.0) == Some(CORPSE))
            .expect("the corpse's CreateObject");
        assert_lies_dead(corpse, "a monster corpse");
        let motion = ts.received::<dereth_protocol::movement::MovementSetObjectMovement>(alpha);
        let death = motion
            .iter()
            .filter(|m| m.id.0 == drudge.full())
            .filter_map(|m| m.decoded_movement().ok())
            .filter_map(|b| b.body.interpreted)
            .any(|s| s.forward_command == Some(DEAD));
        assert!(
            death,
            "the drudge broadcast its death motion (Creature.Die's ExecuteMotion)"
        );
    }

    /// A player's corpse (`Creature.CreateCorpse` for a player, as `Player.Die` calls it; the whole
    /// `Player.Die` runs in the real-content tier, which has the Vitae spell) is created lying dead for
    /// the player watching.
    #[test]
    fn a_player_corpse_is_created_lying_dead() {
        let mut ts = server();
        let alpha = join(
            &mut ts,
            "i15alpha",
            ALPHA,
            "Alpha Admin",
            at(20.0, 20.0),
            false,
        );
        let _bravo = join(
            &mut ts,
            "i15bravo",
            BRAVO,
            "Bravo Player",
            at(22.0, 20.0),
            false,
        );
        ts.advance(0.5);
        let n = ts.received::<ItemCreateObject>(alpha).len();

        creature_death::create_corpse(&mut ts.world, ObjectGuid::new(BRAVO), None, false);
        let found = ts.run_until(10.0, |ts| {
            ts.received::<ItemCreateObject>(alpha)[n..]
                .iter()
                .any(|c| wcid_of(ts, c.0.id.0) == Some(CORPSE))
        });
        assert!(found, "the player's corpse is created");
        let creates = ts.received::<ItemCreateObject>(alpha);
        let corpse = creates[n..]
            .iter()
            .find(|c| wcid_of(&ts, c.0.id.0) == Some(CORPSE))
            .expect("the corpse's CreateObject");
        assert_lies_dead(corpse, "a player corpse");
    }
}

#[cfg(feature = "real-content")]
mod posture_real {
    //! ACE: Source/ACE.Server/WorldObjects/Corpse.cs::Corpse
    use crate::support::event_world::real::*;

    #[test]
    fn a_chicken_a_drudge_and_a_player_leave_corpses_lying_dead() {
        let mut ts = server();
        let alpha = join(
            &mut ts,
            "i15alpha",
            ALPHA,
            "Alpha Admin",
            holtburg(84.0, 7.1, 94.0),
            false,
        );
        let _bravo = join(
            &mut ts,
            "i15bravo",
            BRAVO,
            "Bravo Player",
            holtburg(86.0, 7.1, 94.0),
            false,
        );
        ts.advance(1.0);

        for (wcid, what, x) in [
            (CHICKEN, "a chicken's corpse", 88.0),
            (DRUDGE_SKULKER, "a drudge's corpse", 90.0),
        ] {
            let victim = on_ground(&mut ts.world, wcid, holtburg(x, 7.1, 94.0));
            ts.advance(1.0);
            let n = ts.received::<ItemCreateObject>(alpha).len();
            assert!(
                ts.world
                    .objects
                    .get(victim)
                    .is_some_and(|o| o.creature.is_some()),
                "{what}: the victim is a live creature"
            );
            creature_death::die(&mut ts.world, victim);
            assert!(
                ts.run_until(10.0, |ts| ts.world.objects.get(victim).is_none()),
                "{what}: the victim is destroyed after its death animation"
            );
            ts.advance(0.5);
            let creates = ts.received::<ItemCreateObject>(alpha);
            let corpse = creates[n..]
                .iter()
                .find(|c| wcid_of(&ts, c.0.id.0) == Some(CORPSE))
                .unwrap_or_else(|| panic!("{what}: a CreateObject"));
            assert_lies_dead(corpse, what);
        }

        let n = ts.received::<ItemCreateObject>(alpha).len();
        creature_death::die(&mut ts.world, ObjectGuid::new(BRAVO));
        let found = ts.run_until(15.0, |ts| {
            ts.received::<ItemCreateObject>(alpha)[n..]
                .iter()
                .any(|c| wcid_of(ts, c.0.id.0) == Some(CORPSE))
        });
        assert!(found, "the player's corpse is created");
        let creates = ts.received::<ItemCreateObject>(alpha);
        let corpse = creates[n..]
            .iter()
            .find(|c| wcid_of(&ts, c.0.id.0) == Some(CORPSE))
            .unwrap();
        assert_lies_dead(corpse, "a player's corpse");
    }
}

#[cfg(feature = "real-content")]
mod decay_real {
    //! ACE: Source/ACE.Server/WorldObjects/Corpse.cs::Corpse
    use crate::support::real_content_bot::real::*;

    /// A monster's corpse rots: `WorldObject_Decay`'s default 5 minutes (it still holds items), so
    /// ten minutes after the kill it is gone.
    #[test]
    fn a_monster_corpse_rots_away() {
        let mut l = create_and_enter();
        pick_up_and_wield(&mut l);
        let drudge = create_a_drudge(&mut l);
        fight(&mut l, drudge);
        let corpses = l.nearby(|o| o.is_corpse());
        assert_eq!(corpses.len(), 1);
        l.advance(600.0);
        assert!(
            l.ts.world.objects.get(corpses[0]).is_none(),
            "the corpse rotted away"
        );
    }
}
