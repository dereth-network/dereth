//! ACE: Source/ACE.Server/WorldObjects/Monster_Melee.cs::MeleeAttack
//! (real-content) An Academy monster's attack gaps against a stationary player lie in
//! [animLength, animLength+PowerupTime+0.2] and it wakes only when attacked.
//! Fixture: virtual-time bots, retail dats and world.pack.

#![allow(clippy::disallowed_methods)]

#[cfg(feature = "real-content")]
mod real {
    use dereth_primitives::ObjectId;
    use dereth_protocol::combat::{
        CombatChangeCombatMode, CombatTargetedMeleeAttack, DefenderNotification,
        EvasionDefenderNotification,
    };
    use dereth_protocol::login::CharacterLoginCompleteNotification;
    use dereth_protocol::movement::MovementSetObjectMovement;
    use dereth_protocol::objects::EffectsPlayerTeleport;
    use empyrean_entity::enums::{CombatMode, MotionCommand, MotionStance};
    use empyrean_entity::ObjectGuid;
    use empyrean_testkit::decode;
    use empyrean_world::physics::motion_table;
    use empyrean_world::world_objects::world_object::WorldObject;
    use empyrean_world::world_objects::{creature_combat, monster_awareness, monster_combat};

    use crate::support::real_content_bot::real::{create_and_enter, Loop};

    /// The Academy's melee monster (ACE's world DB weenie 12698) that `long-solo-play`'s character fought
    /// first: its landblock instance stands in cell 0x7F03023A at (60.92, -20.01, 0.009).
    const ACADEMY_MONSTER: u32 = 12698;
    const MONSTER_CELL: u32 = 0x7F03_023A;
    /// Where the character stands: 1.6 m west of the monster's home, in its cell.
    const STAND: (f32, f32, f32) = (59.3, -20.0, 0.01);
    /// ACE's `monsterTickInterval`.
    const MONSTER_TICK: f64 = 0.2;
    /// The server loop's frame (60 Hz), by which an attack can start late.
    const FRAME: f64 = 1.0 / 60.0;
    const WATCH: f64 = 60.0;
    const SEED: u64 = 1818;

    fn teleloc(l: &mut Loop, line: &str) {
        let mark = l.mark();
        l.admin_command(line);
        let id = l.id;
        assert!(
            l.ts.run_until(5.0, |ts| !decode::all_of::<EffectsPlayerTeleport>(
                &ts.received_raw(id)[mark..]
            )
            .is_empty()),
            "the character is teleported"
        );
        l.advance(1.0);
        l.action(&CharacterLoginCompleteNotification);
        l.advance(1.0);
    }

    fn awake_creatures(l: &Loop) -> Vec<ObjectGuid> {
        l.nearby(|o| o.creature.is_some() && !o.is_player())
            .into_iter()
            .filter(|&g| monster_awareness::is_awake(&l.ts.world, g))
            .collect()
    }

    #[test]
    fn an_academy_monster_attacks_a_stationary_player_at_aces_cadence() {
        let mut l = create_and_enter();
        empyrean_common::thread_safe_random::ThreadSafeRandom::seed(SEED);
        teleloc(
            &mut l,
            &format!(
                "@teleloc 0x{MONSTER_CELL:08X} {} {} {}",
                STAND.0, STAND.1, STAND.2
            ),
        );
        assert_eq!(l.location().cell(), MONSTER_CELL);
        let me = l.location();
        let monster = l
            .nearby_wcid(ACADEMY_MONSTER)
            .into_iter()
            .min_by(|a, b| {
                let d = |g: &ObjectGuid| {
                    let p = l.location_of(*g);
                    (p.position_x - me.position_x).hypot(p.position_y - me.position_y)
                };
                d(a).total_cmp(&d(b))
            })
            .expect("the Academy's monsters are spawned");
        assert_eq!(
            l.location_of(monster).cell(),
            MONSTER_CELL,
            "the nearest one is the monster of the character's cell"
        );

        // Standing beside it, unprovoked, for 10 s: nothing wakes (awareness 0.1 m).
        l.advance(10.0);
        assert!(
            awake_creatures(&l).is_empty(),
            "no monster notices a player who does not attack: {:?}",
            awake_creatures(&l)
        );

        // One melee attack wakes it, then the character stands still, out of combat.
        l.action(&CombatChangeCombatMode {
            combat_mode: u32::try_from(CombatMode::Melee.0).expect("a mode"),
        });
        l.advance(1.0);
        l.action(&CombatTargetedMeleeAttack {
            target: ObjectId(monster.full()),
            attack_height: 2,
            power_level: 0.5,
        });
        assert!(
            l.ts.run_until(5.0, |ts| monster_awareness::is_awake(&ts.world, monster)),
            "the attacked monster wakes"
        );
        l.advance(2.0);
        l.action(&CombatChangeCombatMode {
            combat_mode: u32::try_from(CombatMode::NonCombat.0).expect("a mode"),
        });
        let start = l.ts.seconds();

        // Watch for 60 s: the monster's attack motions (UpdateMotion) and strikes on the player.
        let mut attacks: Vec<(f64, u16)> = Vec::new();
        let mut strikes = 0usize;
        while l.ts.seconds() - start < WATCH {
            let mark = l.mark();
            l.advance(FRAME);
            let t = l.ts.seconds();
            for m in l
                .since::<MovementSetObjectMovement>(mark)
                .into_iter()
                .filter(|m| m.id.0 == monster.full())
            {
                let body = m
                    .decoded_movement()
                    .expect("the movement buffer decodes")
                    .body;
                if let Some(fc) = body.interpreted.and_then(|i| i.forward_command) {
                    attacks.push((t, fc));
                }
            }
            strikes += l.since::<DefenderNotification>(mark).len()
                + l.since::<EvasionDefenderNotification>(mark).len();
            assert!(
                l.health(l.g).unwrap_or(0) > 0,
                "the character survives the watch"
            );
            assert!(
                l.ts.world
                    .objects
                    .get(monster)
                    .is_some_and(|o| !monster_combat::is_dead(o)),
                "the monster lives through the watch"
            );
        }

        // The attack maneuvers of the monster's stance, with ACE's animation length for each.
        let w = &mut l.ts.world;
        let o = w.objects.get(monster).expect("the monster");
        let table = creature_combat::fields(o)
            .combat_table
            .clone()
            .expect("the monster's combat table is loaded");
        let stance =
            o.wo.world_object_properties
                .current_motion_state
                .as_ref()
                .map(|m| m.stance)
                .expect("a motion state");
        assert_ne!(
            stance,
            MotionStance::NonCombat,
            "the monster is in its combat stance"
        );
        let motion_table_id = o.motion_table_id();
        let powerup = o.powerup_time().unwrap_or(1.0);
        let anim_speed = creature_combat::get_anim_speed(w, monster);
        let anim_length = |w: &empyrean_world::World, low: u16| -> Option<f64> {
            table
                .maneuvers
                .iter()
                .find(|m| m.style == stance.0 && (m.motion & 0xFFFF) == u32::from(low))
                .map(|m| {
                    f64::from(motion_table::get_animation_length(
                        w,
                        motion_table_id,
                        stance,
                        MotionCommand(m.motion),
                        anim_speed,
                    ))
                })
        };
        let swings: Vec<(f64, f64)> = attacks
            .iter()
            .filter_map(|&(t, fc)| anim_length(w, fc).map(|len| (t, len)))
            .collect();
        let lengths: Vec<f64> = swings.iter().map(|s| s.1).collect();
        let (min_len, max_len) = lengths
            .iter()
            .fold((f64::MAX, 0.0f64), |(a, b), &x| (a.min(x), b.max(x)));

        // ACE's timing formula, gap by gap: from each attack's start to the next.
        for pair in swings.windows(2) {
            let (t0, len) = pair[0];
            let gap = pair[1].0 - t0;
            assert!(
                gap >= len - FRAME && gap <= len + powerup + MONSTER_TICK + 2.0 * FRAME,
                "an attack gap of {gap:.3} s after a {len:.3} s swing (PowerupTime {powerup}): ACE's is [{len:.3}, {:.3}]",
                len + powerup + MONSTER_TICK
            );
        }
        // And over the minute: between one attack per (longest swing + PowerupTime + a tick) and
        // one per shortest swing.
        #[allow(clippy::cast_precision_loss)]
        let n = swings.len() as f64;
        let low = (WATCH / (max_len + powerup + MONSTER_TICK)).floor() - 1.0;
        let high = (WATCH / min_len).ceil() + 1.0;
        assert!(n >= low && n <= high, "{n} attacks in {WATCH} s; ACE's timing allows {low}..={high} (swings {min_len:.3}..{max_len:.3} s, PowerupTime {powerup})");
        // The mean gap is ACE's expectation: the mean swing, half the PowerupTime (the uniform
        // `ThreadSafeRandom.Next(0, PowerupTime)` delay) and on average half a monster tick; the
        // margin is three standard deviations of the delay's mean over these gaps.
        let gaps = n - 1.0;
        let mean_gap = (swings[swings.len() - 1].0 - swings[0].0) / gaps;
        let mean_len = swings[..swings.len() - 1].iter().map(|s| s.1).sum::<f64>() / gaps;
        let expected = mean_len + powerup / 2.0 + MONSTER_TICK / 2.0;
        let margin = 3.0 * (powerup / 12f64.sqrt()) / gaps.sqrt() + MONSTER_TICK / 2.0;
        assert!(
            (mean_gap - expected).abs() <= margin,
            "a mean attack gap of {mean_gap:.3} s; ACE's timing expects {expected:.3} +- {margin:.3} s"
        );
        // An attack whose swing is still playing when the watch ends may not have struck yet (its
        // strike comes at the attack hook, as the client plays it: V352).
        let finished = swings.iter().filter(|s| s.0 + s.1 <= start + WATCH).count();
        assert!(strikes >= finished, "every finished attack strikes the player or is evaded ({strikes} strikes, {finished} finished of {} attacks)", swings.len());

        // Only the attacked monster engaged.
        assert_eq!(
            awake_creatures(&l),
            vec![monster],
            "only the attacked monster is awake"
        );
        assert_eq!(
            monster_combat::attack_target(&l.ts.world, monster),
            Some(l.g),
            "and it targets the character"
        );
        let others = l.nearby(|o: &WorldObject| o.creature.is_some() && !o.is_player());
        assert!(
            others
                .iter()
                .filter(|&&g| g != monster)
                .all(|&g| monster_combat::attack_target(&l.ts.world, g).is_none()),
            "no other creature targets anyone"
        );
        println!("Attack cadence: {n} attacks, {strikes} strikes in {WATCH} s; swings {min_len:.3}..{max_len:.3} s, PowerupTime {powerup}, allowed {low}..={high}; mean gap {mean_gap:.3} s, expected {expected:.3} +- {margin:.3}");
    }
}
