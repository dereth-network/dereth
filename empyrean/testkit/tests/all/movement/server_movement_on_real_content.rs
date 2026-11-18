//! Divergence: V550
//! (real-content) A client that runs and turns as the server moves it, through Holtburg and the
//! Academy's halls, with its reports arriving late and unevenly, is never sent back, with the
//! motion correction on or off; with it on, one claiming to be 30 m under the ground is. A player
//! who is not a player killer, killed mid-run, stops on the server at once.
//! Fixture: virtual-time bots, retail dats and world.pack.

#![allow(clippy::disallowed_methods)]

#[cfg(feature = "real-content")]
mod real {
    use crate::movement::position_cadence::{positions_of, TURN_RIGHT};
    use crate::support::real_content_bot::real::{
        autonomous, create_character, decode, forward, move_to_state,
        CharacterLoginCompleteNotification, EffectsPlayerTeleport, Loop as Bot, Position,
        RawMotionState, HOLD_RUN, WALK_FORWARD,
    };
    use empyrean_world::physics::phys_ext;
    use empyrean_world::world_objects::player_tick::physics_position_to_location;

    /// Holtburg beside the life stone, outdoors.
    const HOLTBURG: &str = "@teleloc 0xA9B40019 84 7.1 94";

    /// What the client's keys do from a moment on.
    #[derive(Debug, Clone, Copy)]
    enum Keys {
        Run,
        RunTurning,
        Stand,
    }

    /// Running north-ish with turns: the keys from each time (seconds) on.
    const SCRIPT: [(f64, Keys); 8] = [
        (0.0, Keys::Run),
        (2.0, Keys::RunTurning),
        (2.8, Keys::Run),
        (4.5, Keys::RunTurning),
        (5.6, Keys::Run),
        (7.0, Keys::RunTurning),
        (7.5, Keys::Run),
        (9.0, Keys::Stand),
    ];
    const SCRIPT_END: f64 = 10.0;

    fn keys(at: &Position, k: Keys) -> crate::support::real_content_bot::real::MovementMoveToState {
        let run = matches!(k, Keys::Run | Keys::RunTurning);
        let turn = matches!(k, Keys::RunTurning);
        move_to_state(
            at,
            RawMotionState {
                current_holdkey: Some(HOLD_RUN),
                forward_command: run.then_some(WALK_FORWARD),
                forward_holdkey: run.then_some(HOLD_RUN),
                turn_command: turn.then_some(TURN_RIGHT),
                turn_holdkey: turn.then_some(HOLD_RUN),
                ..RawMotionState::default()
            },
        )
    }

    /// Turns the `server_motion_correction` option on or off.
    fn correction(b: &mut Bot, on: bool) {
        assert!(empyrean_world::managers::property_manager::modify_bool(
            &b.ts.world,
            "server_motion_correction",
            on
        ));
        empyrean_world::world_objects::player_tick::refresh_movement_options(&mut b.ts.world);
    }

    /// Where the server has the character's body.
    fn body(b: &Bot) -> Position {
        let h = phys_ext::physics_obj(&b.ts.world, b.g).expect("a body");
        physics_position_to_location(&phys_ext::position(&b.ts.world, h).expect("placed"))
    }

    /// Teleports the character with `line` and brings it out of portal space.
    fn teleloc(b: &mut Bot, line: &str) {
        let mark = b.mark();
        b.admin_command(line);
        let id = b.id;
        assert!(
            b.ts.run_until(5.0, |ts| !decode::all_of::<EffectsPlayerTeleport>(
                &ts.received_raw(id)[mark..]
            )
            .is_empty()),
            "teleported"
        );
        b.advance(1.0);
        b.action(&CharacterLoginCompleteNotification);
        b.advance(2.0);
    }

    /// How many times the character's own client has been sent somewhere new (a new
    /// force-position stamp) since message `mark`.
    fn sent_back(b: &Bot, mark: usize) -> usize {
        let all = positions_of(&b.ts, b.id, 0, b.g.full());
        let since = positions_of(&b.ts, b.id, mark, b.g.full()).len();
        let first = all.len() - since;
        let mut stamp = if first > 0 { all[first - 1].force } else { 0 };
        let mut n = 0;
        for p in &all[first..] {
            if p.force != stamp {
                n += 1;
                stamp = p.force;
            }
        }
        n
    }

    /// The client's reports of a run: the script's keys at their times, and an AutonomousPosition
    /// a second after the last report or at once in a new cell; each with where the client was.
    fn reports(path: &[(f64, Position)]) -> Vec<(f64, Report)> {
        let at = |t: f64| {
            path.iter()
                .find(|(pt, _)| *pt >= t)
                .map_or(path[path.len() - 1].1, |(_, p)| *p)
        };
        let mut out: Vec<(f64, Report)> = SCRIPT
            .iter()
            .map(|&(t, k)| (t, Report::Keys(k, at(t))))
            .collect();
        let mut last = 0.0;
        let mut cell = path[0].1.cell();
        for &(t, p) in path {
            let moving = SCRIPT
                .iter()
                .rev()
                .find(|(st, _)| *st <= t)
                .is_some_and(|(_, k)| !matches!(k, Keys::Stand));
            let keyed = SCRIPT.iter().any(|(st, _)| (*st - t).abs() < 1e-9);
            if keyed {
                last = t;
            }
            if moving && (p.cell() != cell || t - last >= 1.0) {
                out.push((t, Report::Position(p)));
                last = t;
            }
            cell = p.cell();
        }
        out.sort_by(|a, b| a.0.total_cmp(&b.0));
        out
    }

    #[derive(Debug, Clone, Copy)]
    enum Report {
        Keys(Keys, Position),
        Position(Position),
    }

    /// Runs the script from where `start` teleports the character, reporting where the server has
    /// the body (the client and the server agree at once); answers the path the body took, a
    /// position a tick.
    fn record(b: &mut Bot, start: &str, heading: (f32, f32)) -> (Vec<(f64, Position)>, usize) {
        teleloc(b, start);
        // the client turns to the run's heading before it sets off
        let mut facing = body(b);
        (facing.rotation_w, facing.rotation_z) = heading;
        b.action(&keys(&facing, Keys::Stand));
        b.advance(0.5);
        let mark = b.mark();
        let t0 = b.ts.seconds();
        let mut path = vec![(0.0, body(b))];
        let mut next = 0;
        let mut last_report = 0.0;
        let mut cell = body(b).cell();
        while b.ts.seconds() - t0 < SCRIPT_END {
            let t = b.ts.seconds() - t0;
            let here = body(b);
            if next < SCRIPT.len() && SCRIPT[next].0 <= t + 1e-9 {
                b.action(&keys(&here, SCRIPT[next].1));
                next += 1;
                last_report = t;
            } else if !matches!(SCRIPT[next - 1].1, Keys::Stand)
                && (here.cell() != cell || t - last_report >= 1.0)
            {
                b.action(&autonomous(&here));
                last_report = t;
            }
            cell = here.cell();
            b.ts.step();
            path.push((b.ts.seconds() - t0, body(b)));
        }
        b.advance(1.0);
        (path, sent_back(b, mark))
    }

    /// Runs the script again from the same start, the client's reports carrying where the
    /// recorded run had the client, each arriving 0.1 to 0.18 s after it was sent, in order.
    fn replay(b: &mut Bot, start: &str, path: &[(f64, Position)]) -> usize {
        teleloc(b, start);
        b.action(&keys(&path[0].1, Keys::Stand));
        b.advance(0.5);
        let mark = b.mark();
        let t0 = b.ts.seconds();
        let mut due: Vec<(f64, Report)> = Vec::new();
        let mut last = 0.0f64;
        for (i, (t, r)) in reports(path).into_iter().enumerate() {
            // an uneven delay, without reordering
            #[allow(clippy::cast_precision_loss)]
            let jitter = 0.08 * ((i * 7919) % 13) as f64 / 12.0;
            last = last.max(t + 0.1 + jitter);
            due.push((last, r));
        }
        let end = due.last().map_or(0.0, |d| d.0) + 1.0;
        let mut next = 0;
        while b.ts.seconds() - t0 < end {
            let t = b.ts.seconds() - t0;
            while next < due.len() && due[next].0 <= t + 1e-9 {
                match due[next].1 {
                    Report::Keys(k, p) => b.action(&keys(&p, k)),
                    Report::Position(p) => b.action(&autonomous(&p)),
                }
                next += 1;
            }
            b.ts.step();
        }
        b.advance(1.0);
        sent_back(b, mark)
    }

    /// The honest run, recorded and replayed late, from `start` facing `heading` (the rotation's
    /// w and z), with the motion correction on and then off.
    fn honest(start: &str, heading: (f32, f32)) {
        for on in [true, false] {
            let mut b = create_character(&[]);
            b.enter_world();
            correction(&mut b, on);
            let (path, recorded) = record(&mut b, start, heading);
            let ran: f32 = path.windows(2).map(|w| w[0].1.distance_to(&w[1].1)).sum();
            assert!(ran > 15.0, "the run went somewhere: {ran} m");
            assert_eq!(
                recorded, 0,
                "not sent back while the reports were the server's own (correction {on})"
            );
            assert_eq!(
                replay(&mut b, start, &path),
                0,
                "not sent back with late reports (correction {on})"
            );
        }
    }

    #[test]
    fn an_honest_client_running_and_turning_through_holtburg_is_never_sent_back() {
        honest(HOLTBURG, (1.0, 0.0));
    }

    #[test]
    fn an_honest_client_running_and_turning_through_the_academy_is_never_sent_back() {
        let mut b = create_character(&[]);
        b.enter_world();
        let at = b.location();
        let start = format!(
            "@teleloc 0x{:08X} {} {} {}",
            at.cell(),
            at.position_x,
            at.position_y,
            at.position_z
        );
        drop(b);
        // south-west, down the entrance hall
        honest(&start, (0.383, 0.924));
    }

    #[test]
    fn a_client_claiming_to_be_30_m_under_holtburg_is_sent_back_to_the_ground() {
        let mut b = create_character(&[]);
        b.enter_world();
        correction(&mut b, true);
        teleloc(&mut b, HOLTBURG);
        let mark = b.mark();
        let start = body(&b);
        let mut under = start;
        under.position_z -= 30.0;
        b.action(&autonomous(&under));
        b.advance(0.6);
        assert_eq!(sent_back(&b, mark), 1);
        let l = b.location();
        assert!((l.position_z - body(&b).position_z).abs() < 0.01);
        assert!(
            l.position_z > start.position_z - 1.0,
            "on the ground: {l:?}"
        );
    }

    #[test]
    fn a_player_who_is_not_a_player_killer_killed_mid_run_stops_on_the_server_at_once() {
        let mut b = create_character(&[]);
        b.enter_world();
        teleloc(&mut b, HOLTBURG);
        let mut north = body(&b);
        (north.rotation_w, north.rotation_z) = (1.0, 0.0);
        b.action(&forward(&north, HOLD_RUN));
        b.advance(1.5);
        let here = body(&b);
        assert!(here.distance_to(&north) > 5.0, "running: {here:?}");
        b.action(&autonomous(&here));
        b.advance(0.05);

        let status =
            b.ts.world.objects.get(b.g).map(
                empyrean_world::world_objects::world_object::WorldObject::player_killer_status,
            );
        {
            use empyrean_entity::enums::PlayerKillerStatus as S;
            assert!(
                status.is_some_and(|s| s != S::PK && s != S::PKLite),
                "not a player killer: {status:?}"
            );
        }
        let died = body(&b);
        let killer = empyrean_world::entity::damage_history_info::DamageHistoryInfo::new(
            &b.ts.world,
            b.g,
            0.0,
        );
        empyrean_world::dispatch::die::die(&mut b.ts.world, b.g, Some(killer), None);
        // before the death's teleport
        b.advance(2.0);
        assert!(
            body(&b).distance_to(&died) < 0.5,
            "the body stops: died at {died:?}, now {:?}",
            body(&b)
        );
        assert!(
            b.location().distance_to(&died) < 0.5,
            "and so does the location"
        );
    }
}
