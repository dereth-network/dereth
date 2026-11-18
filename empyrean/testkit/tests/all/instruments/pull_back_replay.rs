//! Behaviour: none (an instrument: it replays captured clients' report streams and prints how often
//! the server moved the player back; it asserts nothing)
//! (real-content) A captured retail client's own reports of its movement (its MoveToState,
//! AutonomousPosition and Jump messages, at their recorded times), sent again from a character on
//! the retail dats and world.pack with the server moving players and its motion correction on
//! (`server_motion_correction`). Counts the server's pull-backs an hour, by distance from the
//! client's last report, against the captured server's own forced positions in the same stream.
//!
//! - **Input:** `EMPYREAN_REPORT_STREAMS`, a folder of streams: `# player 0x…` then `t <tab> dir
//!   <tab> type (hex) <tab> message (hex)`, the client's movement messages and the server's
//!   positions of the player and teleports, a session each (unset: a printed skip).
//! - **Run speed:** the character's Run skill is set so its run rate gives the speed the stream's
//!   client ran at (the 95th percentile of its running reports' speeds), the rate's own curve
//!   solved for the skill.
//! - **Stamps:** each report carries the server's own instance, teleport and server-control
//!   stamps; its force-position stamp is the server's last one sent at least
//!   [`STAMP_TAKEN_S`] before (the client takes a new one when it gets it).
//! - **Harm:** the character takes none (a new character where the recorded one hunted would die
//!   and be sent to a lifestone). A teleport the server makes on its own (a portal walked into) is
//!   answered as a client answers one, counted, and the stream goes on from its next report.
//! - **Re-sync:** where the recording teleports (its teleport stamp changes), the character is put
//!   at the next report; after a pull-back, the server's body is put at the next report, so one
//!   disagreement is counted once and the stream goes on from where the recorded client was.
//! - **Actions** (unless `EMPYREAN_REPLAY_ACTIONS=0`): the client's other game actions in the
//!   stream (melee attacks, casts, uses, combat mode changes, attack cancels) are sent at their
//!   recorded times. Each object one names is the replay's counterpart of the captured one: of its
//!   weenie class, or a creature of its name, within [`MAP_RADIUS`] of where the captured server
//!   last had it, one attacking the character first, else the nearest; with none, one of its class
//!   is made there. The character knows each spell it casts, needs no components and has mana to
//!   spare; a missile attack is not sent (the character has no bow), but its target is mapped.
//!   Where the stream has a target's death (the captured server's Dead motion of it), its
//!   counterpart is killed then, so the replay's monsters die when retail's did.
//!
//! - **Output:** a line a stream (its hours, reports, running speed, the server's pull-backs, the
//!   first of each episode (none in the [`EPISODE_S`] before), those with the body stalled (it moved
//!   under half a meter in the half second before while the client's keys moved it), the captured
//!   server's forced positions and their episodes, the teleports the server made itself); a
//!   `pull` line a pull-back (time, meters, first, stalled, what the body was touching, when the
//!   server sent it, when the client last reported before it, the client's speed then, and what
//!   stopped the server's sweep to the report: a creature is the counterpart of one retail's player
//!   fought or named, one of the replay's own attacking the character, or another; where the
//!   instrument's sweep reached a report the server's did not, where the server's stopped; where
//!   no sweep stopped, `not the sweep`; then, when the server held the report short, the seconds
//!   since it last walked the character to a target, the body's speed over the half second
//!   before, and how far the report was from overlapping the nearest creature); a
//!   `retail` line a captured force (time, meters, first, seconds after the client's last report,
//!   the client's speed); a `win`
//!   line a pull-back over the reports just before it (the last report's offset from the body, flat
//!   and up; over the last interval the client's speed, the speed the server moved the body from
//!   the client's report before, the angle between the two; the ground under the body; the
//!   client's keys; the client's last step tried through the server's collision: how far short it
//!   stopped and what it hit); then the actions sent or skipped, by why, the creatures fought and
//!   killed, the errors the server sent the character, and the rates an hour by distance.
//! - **Trace:** with `EMPYREAN_TRACE_AT` (stream times, comma-separated), a `trace` line for every
//!   server step's body (cell, position, its interpreted forward motion, ground, what it touched)
//!   and every report sent (its keys), over the 30 s before each.
//!
//! Run: `EMPYREAN_REPORT_STREAMS=<folder> cargo test -p empyrean-testkit --features real-content
//! --profile test-release --test all -- --ignored pull_back_replay --nocapture`; with
//! `EMPYREAN_REPORT_STREAMS_LIMIT` the first so many streams only, with
//! `EMPYREAN_REPORT_STREAMS_ONLY` the one named.

#![allow(clippy::disallowed_methods)]

#[cfg(feature = "real-content")]
mod real {
    use std::collections::{BTreeMap, HashMap, HashSet};
    use std::path::Path;

    use dereth_protocol::movement::{
        MoveTimestamps, MovementAutonomousPosition, MovementJump, MovementMoveToState,
        MovementPositionEvent,
    };
    use dereth_protocol::types::space::PositionWire;
    use empyrean_entity::enums::Skill;
    use empyrean_entity::{ObjectGuid, Position};
    use empyrean_world::network::sequence::sequence_type::SequenceType;

    use crate::movement::position_cadence::positions_of;
    use crate::support::real_content_bot::real::{
        create_character, decode, CharacterLoginCompleteNotification, EffectsPlayerTeleport,
        Loop as Bot,
    };

    /// How long after the server sends a new force-position stamp the client's reports carry it,
    /// in seconds (retail: 86% of forces were answered by a report with the new stamp, a median
    /// 0.5 s heartbeat later).
    const STAMP_TAKEN_S: f64 = 0.3;
    /// How long after one pull-back another is part of the same episode, in seconds.
    const EPISODE_S: f64 = 10.0;
    /// How long the replay's client may say nothing before it asks the server for a ping, in
    /// seconds: a standing player sends no reports, and a session silent for 60 s is closed (a
    /// real client's transport acknowledges in the meantime).
    const KEEP_ALIVE_S: f64 = 20.0;
    /// Distance bands of a pull-back from the client's last report, in meters.
    const BANDS: [f32; 5] = [1.0, 5.0, 10.0, 15.0, 30.0];
    /// The fastest a report may move from the one before and still count toward the client's
    /// running speed, in meters a second (a teleport or a new place).
    const SPEED_CEILING: f32 = 25.0;

    #[derive(Debug, Clone)]
    enum Sent {
        Keys(MovementMoveToState),
        Place(MovementAutonomousPosition),
        Jump(MovementJump),
    }

    impl Sent {
        fn position(&self) -> &PositionWire {
            match self {
                Sent::Keys(m) => &m.0.position,
                Sent::Place(m) => &m.0.position,
                Sent::Jump(m) => &m.0.position,
            }
        }
        fn stamps(&self) -> MoveTimestamps {
            match self {
                Sent::Keys(m) => m.0.timestamps,
                Sent::Place(m) => m.0.timestamps,
                Sent::Jump(m) => m.0.timestamps,
            }
        }
        fn set_stamps(&mut self, s: MoveTimestamps) {
            match self {
                Sent::Keys(m) => m.0.timestamps = s,
                Sent::Place(m) => m.0.timestamps = s,
                Sent::Jump(m) => m.0.timestamps = s,
            }
        }
    }

    /// One of the client's other game actions, or what the captured server had told the client of an
    /// object an action names.
    #[derive(Debug, Clone)]
    enum Act {
        /// a game action: its type and its body (from offset 12)
        Do { opcode: u32, body: Vec<u8> },
        /// an Item_CreateObject of a named object, whole
        Created(Vec<u8>),
        /// an UpdatePosition of a named object, whole
        Placed(Vec<u8>),
        /// an attacked or cast-at object's death (its UpdateMotion to Dead, whole)
        Died(Vec<u8>),
    }

    /// A stream's events in order: a report (an index into its reports) or another action.
    #[derive(Debug, Clone)]
    enum Event {
        Report(usize),
        Act(Act),
    }

    /// The client's other game actions a stream may carry.
    const ACTION_TYPES: [&str; 8] = [
        "0008", "000A", "0048", "004A", "0035", "0036", "0053", "01B7",
    ];
    /// How far from where the captured server last had a named object the replay looks for one of
    /// its weenie class, in meters.
    const MAP_RADIUS: f32 = 12.0;

    /// One captured session: the client's reports, the captured server's forced positions (time and
    /// distance from the client's last report) and its length in hours.
    struct Stream {
        name: String,
        player: u32,
        reports: Vec<(f64, Sent)>,
        events: Vec<(f64, Event)>,
        forced: Vec<(f64, f32)>,
        /// at each captured force, when the client last reported before it
        forced_last: Vec<f64>,
        hours: f64,
    }

    fn position_of(p: &PositionWire) -> Position {
        let (o, q) = (p.frame.origin, p.frame.orientation);
        Position::from_components(p.objcell_id, o.x, o.y, o.z, q.x, q.y, q.z, q.w, false)
    }

    /// The distance between two places in global meters (landblocks are 192 m).
    fn apart(a: &Position, b: &Position) -> f32 {
        let (la, lb) = (a.cell() >> 16, b.cell() >> 16);
        #[allow(clippy::cast_precision_loss)]
        let dx = ((lb >> 8) as f32 - (la >> 8) as f32) * 192.0 + b.position_x - a.position_x;
        #[allow(clippy::cast_precision_loss)]
        let dy = ((lb & 0xFF) as f32 - (la & 0xFF) as f32) * 192.0 + b.position_y - a.position_y;
        let dz = b.position_z - a.position_z;
        (dx * dx + dy * dy + dz * dz).sqrt()
    }

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len() / 2)
            .map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).expect("hex"))
            .collect()
    }

    fn load(path: &Path) -> Option<Stream> {
        let text = std::fs::read_to_string(path).ok()?;
        let mut reports = Vec::new();
        let mut events = Vec::new();
        let player = text
            .lines()
            .find_map(|l| l.strip_prefix("# player 0x"))
            .and_then(|h| u32::from_str_radix(h.trim(), 16).ok())
            .unwrap_or(0);
        let mut own: Vec<(f64, Position, u16, u16)> = Vec::new();
        let mut first = f64::MAX;
        let mut last = 0.0f64;
        for line in text.lines().filter(|l| !l.starts_with('#')) {
            let mut f = line.split('\t');
            let (Some(t), Some(dir), Some(kind), Some(raw)) =
                (f.next(), f.next(), f.next(), f.next())
            else {
                continue;
            };
            let t: f64 = t.parse().ok()?;
            let raw = hex(raw);
            first = first.min(t);
            last = last.max(t);
            match (dir, kind) {
                // a game action: its opcode, sequence and type, then its body
                ("c2s", "F61C") => {
                    if let Ok(m) =
                        dereth_protocol::read_body_padded::<MovementMoveToState>(&raw[12..])
                    {
                        reports.push((t, Sent::Keys(m)));
                        events.push((t, Event::Report(reports.len() - 1)));
                    }
                }
                ("c2s", "F753") => {
                    if let Ok(m) =
                        dereth_protocol::read_body_padded::<MovementAutonomousPosition>(&raw[12..])
                    {
                        reports.push((t, Sent::Place(m)));
                        events.push((t, Event::Report(reports.len() - 1)));
                    }
                }
                ("c2s", "F61B") => {
                    if let Ok(m) = dereth_protocol::read_body_padded::<MovementJump>(&raw[12..]) {
                        reports.push((t, Sent::Jump(m)));
                        events.push((t, Event::Report(reports.len() - 1)));
                    }
                }
                ("c2s", k) if ACTION_TYPES.contains(&k) && raw.len() >= 12 => {
                    let opcode = u32::from_str_radix(k, 16).expect("hex");
                    let body = raw[12..].to_vec();
                    events.push((t, Event::Act(Act::Do { opcode, body })));
                }
                ("tgt", "F745") => events.push((t, Event::Act(Act::Created(raw)))),
                ("tgt", "F748") => events.push((t, Event::Act(Act::Placed(raw)))),
                ("tgt", "F74C") => events.push((t, Event::Act(Act::Died(raw)))),
                ("s2c", "F748") => {
                    if let Ok(m) =
                        dereth_protocol::read_body_padded::<MovementPositionEvent>(&raw[4..])
                    {
                        let p = &m.position;
                        let at = Position::from_components(
                            p.origin.objcell_id,
                            p.origin.origin.x,
                            p.origin.origin.y,
                            p.origin.origin.z,
                            p.orientation.x,
                            p.orientation.y,
                            p.orientation.z,
                            p.orientation.w,
                            false,
                        );
                        own.push((t, at, p.force_position_timestamp, p.teleport_timestamp));
                    }
                }
                _ => {}
            }
        }
        if reports.is_empty() {
            return None;
        }
        // The captured server's forced positions: a new force stamp with no new teleport stamp, the
        // distance from the client's last report before it.
        let mut forced = Vec::new();
        let mut forced_last = Vec::new();
        for w in own.windows(2) {
            let (a, b) = (&w[0], &w[1]);
            if b.2 != a.2 && b.3 == a.3 {
                let before = reports.iter().rev().find(|(rt, _)| *rt <= b.0);
                if let Some((rt, r)) = before {
                    forced.push((b.0, apart(&position_of(r.position()), &b.1)));
                    forced_last.push(*rt);
                }
            }
        }
        Some(Stream {
            name: path.file_stem()?.to_string_lossy().into_owned(),
            player,
            reports,
            events,
            forced,
            forced_last,
            hours: (last - first) / 3600.0,
        })
    }

    /// The client's speed at `t` (stream seconds): over its last two reports before then at least
    /// 50 ms apart; nothing when it sent none in the 2 s before.
    fn client_speed(s: &Stream, t: f64) -> f32 {
        let i = s.reports.partition_point(|(rt, _)| *rt <= t);
        let Some(last) = i.checked_sub(1).map(|i| &s.reports[i]) else {
            return 0.0;
        };
        if t - last.0 > 2.0 {
            return 0.0;
        }
        let Some(prev) = s.reports[..i - 1]
            .iter()
            .rev()
            .find(|(rt, _)| last.0 - *rt >= 0.05)
        else {
            return 0.0;
        };
        #[allow(clippy::cast_possible_truncation)] // seconds
        let dt = (last.0 - prev.0) as f32;
        apart(
            &position_of(prev.1.position()),
            &position_of(last.1.position()),
        ) / dt
    }

    /// The speed the stream's client ran at: the 95th percentile of the speeds between its reports
    /// of one landblock, 0.3 to 2 s apart, below [`SPEED_CEILING`].
    fn running_speed(s: &Stream) -> f32 {
        let mut speeds: Vec<f32> = s
            .reports
            .windows(2)
            .filter_map(|w| {
                let (a, b) = (
                    position_of(w[0].1.position()),
                    position_of(w[1].1.position()),
                );
                let dt = w[1].0 - w[0].0;
                if a.cell() >> 16 != b.cell() >> 16 || !(0.3..=2.0).contains(&dt) {
                    return None;
                }
                let dx = b.position_x - a.position_x;
                let dy = b.position_y - a.position_y;
                #[allow(clippy::cast_possible_truncation)]
                let v = (dx * dx + dy * dy).sqrt() / dt as f32;
                (v > 1.0 && v < SPEED_CEILING).then_some(v)
            })
            .collect();
        if speeds.is_empty() {
            return 0.0;
        }
        speeds.sort_by(f32::total_cmp);
        speeds[speeds.len() * 95 / 100]
    }

    /// The Run skill whose run rate (`(skill / (skill + 200)) x 11 + 4`, over 4; 4.5 from 800) gives
    /// `speed` at 4 m/s a unit of rate.
    fn skill_for(speed: f32) -> u32 {
        let v = speed.clamp(4.5, 18.0);
        if v >= 17.9 {
            return 800;
        }
        let s = 200.0 * (v - 4.0) / (11.0 - (v - 4.0));
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let skill = s.round().clamp(0.0, 799.0) as u32;
        skill
    }

    /// Sets the character's Run skill so its run rate is the one `skill` gives.
    fn set_run(b: &mut Bot, skill: u32) {
        let want = 0.25 * ((skill as f32 / (skill as f32 + 200.0)) * 11.0 + 4.0);
        let g = b.g;
        let mut init = skill;
        for _ in 0..12 {
            if let Some(o) = b.ts.world.objects.get_mut(g) {
                let (ps, _) = o.biota.get_or_add_skill(Skill::Run);
                ps.sac = empyrean_entity::enums::SkillAdvancementClass::Specialized;
                ps.init_level = init;
                ps.level_from_pp = 0;
            }
            let got =
                empyrean_world::world_objects::monster_navigation::get_run_rate(&mut b.ts.world, g);
            if (got - want).abs() < 0.005 {
                break;
            }
            // the rate rises with the skill: step toward it
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let step = (((want - got) * 400.0).round() as i64).clamp(-200, 200);
            init = u32::try_from((i64::from(init) + step).max(0)).unwrap_or(0);
        }
    }

    fn stamps_now(b: &mut Bot, force: u16) -> MoveTimestamps {
        let g = b.g;
        let mut seq = |t: SequenceType| {
            b.ts.world.objects.get_mut(g).map_or(0, |o| {
                empyrean_world::network::game_messages::game_message::ushort_sequence(
                    &o.sequences.get_current_sequence(t),
                )
            })
        };
        MoveTimestamps {
            instance: seq(SequenceType::ObjectInstance),
            server_control: seq(SequenceType::ObjectServerControl),
            teleport: seq(SequenceType::ObjectTeleport),
            force_position: force,
        }
    }

    /// The force-position stamp the server last gave the character.
    fn server_force(b: &mut Bot) -> u16 {
        let g = b.g;
        b.ts.world.objects.get_mut(g).map_or(0, |o| {
            empyrean_world::network::game_messages::game_message::ushort_sequence(
                &o.sequences
                    .get_current_sequence(SequenceType::ObjectForcePosition),
            )
        })
    }

    /// Puts the character at `at` by the admin's teleport, out of portal space.
    fn teleport_to(b: &mut Bot, at: &Position) {
        let mark = b.mark();
        b.admin_command(&format!(
            "@teleloc 0x{:08X} {} {} {}",
            at.cell(),
            at.position_x,
            at.position_y,
            at.position_z
        ));
        let id = b.id;
        let _ = b.ts.run_until(5.0, |ts| {
            !decode::all_of::<EffectsPlayerTeleport>(&ts.received_raw(id)[mark..]).is_empty()
        });
        b.advance(0.5);
        b.action(&CharacterLoginCompleteNotification);
        b.advance(1.0);
        out_of_portal_space(b);
        let there = b.ts.world.objects.get(b.g).and_then(|o| o.location());
        assert!(
            there.is_some_and(|l| l.cell() >> 16 == at.cell() >> 16),
            "the server did not take the replay's teleport to 0x{:08X}",
            at.cell()
        );
    }

    /// The character out of portal space after its LoginComplete: where the server still holds it
    /// there (it finished the teleport after the replay's answer), the answer is sent again once.
    ///
    /// # Panics
    /// When the server keeps the character in portal space all the same (the stream is skipped).
    fn out_of_portal_space(b: &mut Bot) {
        let teleporting = |b: &Bot| {
            b.ts.world
                .objects
                .get(b.g)
                .is_some_and(|o| o.wo.world_object.teleporting)
        };
        if teleporting(b) {
            b.advance(1.0);
            b.action(&CharacterLoginCompleteNotification);
            b.advance(1.0);
        }
        assert!(
            !teleporting(b),
            "the server kept the character in portal space"
        );
    }

    /// What the body is touching lately (its collision record): a creature, a door, another object,
    /// or nothing but the land and buildings.
    fn touched(b: &Bot) -> &'static str {
        use empyrean_entity::enums::WeenieType;
        use empyrean_world::physics::phys_ext;
        let Some(rec) = phys_ext::physics_obj(&b.ts.world, b.g)
            .and_then(|h| phys_ext::server_record(&b.ts.world, h))
        else {
            return "none";
        };
        let mut kind = "none";
        for id in rec.collision_table.keys() {
            let g = empyrean_entity::ObjectGuid::new(*id);
            let Some(o) = b.ts.world.objects.get(g) else {
                continue;
            };
            if o.is_creature() && !o.is_player() {
                return "creature";
            }
            kind = if o.weenie_type() == WeenieType::Door {
                "door"
            } else {
                "object"
            };
        }
        kind
    }

    /// Where the server has the character's body.
    fn body_now(b: &Bot) -> Option<Position> {
        use empyrean_world::physics::phys_ext;
        let h = phys_ext::physics_obj(&b.ts.world, b.g)?;
        phys_ext::position(&b.ts.world, h)
            .map(|p| empyrean_world::world_objects::player_tick::physics_position_to_location(&p))
    }

    /// Puts the server's body (and the player's location) where `at` is, as the server's own
    /// pull-back puts it, without telling the client.
    fn put_body(b: &mut Bot, at: &Position) {
        use empyrean_world::physics::phys_ext;
        let g = b.g;
        if let Some(h) = phys_ext::physics_obj(&b.ts.world, g) {
            phys_ext::set_position(&mut b.ts.world, h, &phys_ext::to_physics_position(at));
        }
        if let Some(o) = b.ts.world.objects.get_mut(g) {
            o.set_location(Some(*at));
        }
        empyrean_world::managers::landblock_manager::relocate_object_for_physics(
            &mut b.ts.world,
            g,
            true,
        );
    }

    /// One stream's replay.
    /// What one stream's replay did: its pull-backs (time from the stream's start, distance from the
    /// client's last report), whether the body was stalled at each and what it was touching, and
    /// the teleports the server made itself.
    struct Replayed {
        pulled: Vec<(f64, f32)>,
        /// at each pull-back: when the server sent it and when the client last reported before it
        /// (stream seconds)
        timing: Vec<(f64, f64)>,
        /// at each pull-back: what stopped the server's sweep (see [`sweep_blocker`])
        blockers: Vec<&'static str>,
        /// at each pull-back, when the server held the report short: seconds since the server last
        /// walked the character to a target (a move-to chain), and the body's speed over the half
        /// second before (meters a second)
        moveto: Vec<(f64, f32, f32)>,
        stalled: Vec<bool>,
        touching: Vec<&'static str>,
        teleports: usize,
        /// the client's other actions: sent, or why not
        acts: BTreeMap<&'static str, usize>,
        /// the errors the server sent the character, by code
        errors: BTreeMap<u32, usize>,
    }

    /// One report as the replay sent it: the stream's time, where the client was, and where the
    /// server had the body just before.
    struct Sample {
        t: f64,
        client: Position,
        body: Position,
        /// the body's contact plane's upward normal, and whether it stood on walkable ground
        nz: f32,
        walkable: bool,
        /// the client's last keys: forward command (low 16 bits), forward speed
        fwd: u32,
        fwd_speed: f32,
    }

    /// What would stop the server's sweep of the body to `at` now, swept as the server sweeps it (in
    /// pieces along the ground): nothing (it reaches the report), the height (it ends within reach
    /// across but more than the reach up or down), or what its last piece collided with (a creature,
    /// a player, another object of the world, a static of the landscape or a building, the
    /// environment, nothing), or why the sweep cannot be made.
    fn sweep_blocker(b: &mut Bot, at: &Position, acts: &Acts) -> Option<&'static str> {
        use empyrean_world::physics::phys_ext;
        use empyrean_world::world_objects::player_tick::{
            physics_position_to_location, SWEEP_REACHED, SWEEP_REACHED_UP, SWEEP_WALKABLE,
        };
        let h = phys_ext::physics_obj(&b.ts.world, b.g)?;
        let body = phys_ext::position(&b.ts.world, h).filter(|p| p.cell.0 != 0)?;
        let across = |p: &Position| {
            let (dx, dy) = step(at, p);
            dx * dx + dy * dy <= SWEEP_REACHED * SWEEP_REACHED
        };
        let close =
            |p: &Position| across(p) && (p.position_z - at.position_z).abs() <= SWEEP_REACHED_UP;
        if close(&physics_position_to_location(&body)) {
            return None;
        }
        // as the server: a body in the air takes the report unswept
        if b.ts
            .world
            .physics
            .get(h)
            .is_some_and(|o| !o.transient_state.in_contact())
        {
            return None;
        }
        if phys_ext::get_block_dist(body.cell.0, at.cell()) > 1 {
            return Some("out of reach");
        }
        let ground = b.ts.world.physics.get(h).and_then(|o| {
            (o.transient_state.in_contact() && o.contact_plane.normal.z >= SWEEP_WALKABLE)
                .then_some(o.contact_plane.normal)
        });
        // as the server: the sweep starts without the wall the body last slid along
        let kept =
            b.ts.world
                .physics
                .get(h)
                .map(|o| (o.transient_state, o.sliding_normal));
        if let Some(o) = b.ts.world.physics.get_mut(h) {
            o.transient_state.set_sliding(false);
        }
        let found = sweep_pieces_kind(b, h, at, body, ground, acts, &across, &close);
        if let (Some(o), Some((ts, sn))) = (b.ts.world.physics.get_mut(h), kept) {
            o.transient_state = ts;
            o.sliding_normal = sn;
        }
        found
    }

    /// [`sweep_blocker`]'s pieces.
    #[allow(clippy::too_many_arguments)] // an instrument's helper
    fn sweep_pieces_kind(
        b: &mut Bot,
        h: dereth_physics::PhysHandle,
        at: &Position,
        body: dereth_primitives::Position,
        mut ground: Option<dereth_primitives::Vec3>,
        acts: &Acts,
        across: &dyn Fn(&Position) -> bool,
        close: &dyn Fn(&Position) -> bool,
    ) -> Option<&'static str> {
        use empyrean_world::world_objects::player_tick::{
            physics_position_to_location, SWEEP_PIECE, SWEEP_PIECES, SWEEP_WALKABLE,
        };
        let mut from = body;
        let mut last: Option<&'static str> = None;
        for _ in 0..SWEEP_PIECES {
            let (lx, ly) = step(&physics_position_to_location(&from), at);
            let left = (lx * lx + ly * ly).sqrt();
            let part = if left > SWEEP_PIECE {
                SWEEP_PIECE / left
            } else {
                1.0
            };
            let (dx, dy) = (lx * part, ly * part);
            let mut to = from;
            to.frame.origin.x += dx;
            to.frame.origin.y += dy;
            to.frame.origin.z += ground.map_or(0.0, |n| -(n.x * dx + n.y * dy) / n.z);
            let Some(t) = b.ts.world.physics.transition(h, &from, &to, false) else {
                return Some(last.unwrap_or("refused"));
            };
            let end = t.sphere_path.curr_pos;
            let ci = &t.collision_info;
            let hit = if ci.restricted_by.is_some() {
                Some("house barrier")
            } else if let Some((oh, _)) = ci.collide_object.iter().find(|(oh, _)| {
                // an ethereal object (a pressure plate, an open door) is crossed, not hit
                b.ts.world
                    .physics
                    .get(*oh)
                    .is_some_and(|o| !o.state.is_ethereal())
            }) {
                let id = b.ts.world.physics.get(*oh).map_or(0, |o| o.id.0);
                let g = ObjectGuid::new(id);
                Some(match b.ts.world.objects.get(g) {
                    Some(o) if o.is_player() => "player",
                    Some(o) if o.is_creature() => creature_kind(b, g, acts),
                    Some(o) if o.weenie_type() == empyrean_entity::enums::WeenieType::Door => {
                        "closed door"
                    }
                    Some(_) => "world object",
                    None => "landscape or building static",
                })
            } else if ci.collided_with_environment {
                Some("environment")
            } else {
                None
            };
            last = hit.or(last);
            ground = (ci.contact_plane_valid && ci.contact_plane.normal.z >= SWEEP_WALKABLE)
                .then_some(ci.contact_plane.normal);
            let (mx, my) = step(
                &physics_position_to_location(&from),
                &physics_position_to_location(&end),
            );
            from = end;
            let here = physics_position_to_location(&from);
            if close(&here) {
                return None;
            }
            if mx * mx + my * my < 0.01 * 0.01 || part >= 1.0 {
                if across(&here) {
                    return Some("height");
                }
                return Some(hit.unwrap_or_else(|| overlapping(b, h, &from, acts)));
            }
        }
        Some(last.unwrap_or("nothing hit"))
    }

    /// A creature that stopped the sweep: the replay's counterpart of one the session attacked or
    /// cast at ([`Acts::fought`]) or of another of retail's it named ([`Acts::taken`]), one of the
    /// replay's own attacking the character, or another.
    fn creature_kind(b: &Bot, g: ObjectGuid, acts: &Acts) -> &'static str {
        if acts.fought.contains(&g) {
            "creature, fought"
        } else if acts.taken.contains(&g) {
            "creature, retail's"
        } else if empyrean_world::world_objects::monster_combat::attack_target(&b.ts.world, g)
            == Some(b.g)
        {
            "creature, attacking"
        } else {
            "creature"
        }
    }

    /// For a sweep that stopped with nothing hit: a creature's (as [`creature_kind`]) or a player's
    /// body the server's body overlaps where it stopped (the transition lets it out but records
    /// nothing), or "nothing hit".
    fn overlapping(
        b: &Bot,
        h: dereth_physics::PhysHandle,
        at: &dereth_primitives::Position,
        acts: &Acts,
    ) -> &'static str {
        use empyrean_world::world_objects::player_tick::physics_position_to_location;
        let me = physics_position_to_location(at);
        let my_r = b.ts.world.physics.get(h).map_or(0.0, |o| o.radius());
        for id in empyrean_world::managers::landblock_manager::get_loaded_landblocks(&b.ts.world) {
            if id.raw() >> 16 != me.cell() >> 16 {
                continue;
            }
            let Some(l) = b.ts.world.landblock_manager.landblocks.get(id) else {
                continue;
            };
            for g in l.get_all_world_objects_for_diagnostics() {
                if g == b.g {
                    continue;
                }
                let Some(o) = b.ts.world.objects.get(g) else {
                    continue;
                };
                if !o.is_creature() {
                    continue;
                }
                let Some(loc) = o.location() else {
                    continue;
                };
                let r = o
                    .phys
                    .and_then(|oh| b.ts.world.physics.get(oh))
                    .map_or(0.0, |po| po.radius());
                let (dx, dy) = step(&me, &loc);
                if (loc.position_z - me.position_z).abs() < 2.0
                    && dx * dx + dy * dy < (my_r + r) * (my_r + r)
                {
                    return if o.is_player() {
                        "player, overlapping"
                    } else {
                        match creature_kind(b, g, acts) {
                            "creature, fought" => "creature, fought, overlapping",
                            "creature, retail's" => "creature, retail's, overlapping",
                            "creature, attacking" => "creature, attacking, overlapping",
                            _ => "creature, overlapping",
                        }
                    };
                }
            }
        }
        "nothing hit"
    }

    /// The flat distance between two positions, landblocks counted.
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)] // meters and seconds
    fn flat_global(a: &Position, b: &Position) -> f32 {
        let dx =
            a.position_x - b.position_x + 192.0 * (a.landblock_x() as f32 - b.landblock_x() as f32);
        let dy =
            a.position_y - b.position_y + 192.0 * (a.landblock_y() as f32 - b.landblock_y() as f32);
        dx.hypot(dy)
    }

    /// The names of what a probe of the client's own step hit in the server's collision.
    fn hit_names(b: &Bot, t: &dereth_physics::transition::Transition) -> String {
        let mut out: Vec<String> = Vec::new();
        if t.collision_info.collided_with_environment {
            out.push("env".into());
        }
        for (h, _) in &t.collision_info.collide_object {
            let id = b.ts.world.physics.get(*h).map_or(0, |o| o.id.0);
            let g = empyrean_entity::ObjectGuid::new(id);
            let name = b.ts.world.objects.get(g).map_or_else(
                || format!("static:{id:08X}"),
                |o| {
                    format!(
                        "{}:{:?}",
                        o.get_property(empyrean_entity::enums::PropertyString::Name)
                            .unwrap_or_default(),
                        o.weenie_type()
                    )
                },
            );
            out.push(name.replace(['\t', ' '], "_"));
        }
        if out.is_empty() {
            "none".into()
        } else {
            out.join(",")
        }
    }

    /// The flat step from `a` to `b`, landblocks counted.
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)] // meters and seconds
    fn step(a: &Position, b: &Position) -> (f32, f32) {
        (
            b.position_x - a.position_x + 192.0 * (b.landblock_x() as f32 - a.landblock_x() as f32),
            b.position_y - a.position_y + 192.0 * (b.landblock_y() as f32 - a.landblock_y() as f32),
        )
    }

    /// The last reports before a pull-back: the last one's offset from the body (one report that
    /// strayed, or a lead summed over several), and over the last interval the client's step from
    /// its report before against where the server moved the body from that report, and the client's
    /// step through the server's collision. One `win` line.
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)] // meters and seconds
    fn win_line(b: &mut Bot, name: &str, t_pull: f64, d: f32, samples: &[Sample]) {
        use empyrean_world::physics::phys_ext;
        let w: Vec<&Sample> = samples.iter().rev().take(6).rev().collect();
        let Some(last) = w.last() else {
            println!("win\t{name}\t{t_pull:.2}\t{d:.1}\tn=0");
            return;
        };
        let last_off = flat_global(&last.client, &last.body);
        let last_dz = last.body.position_z - last.client.position_z;
        // the last pair at least 50 ms apart
        let pair = w
            .windows(2)
            .rev()
            .find(|p| p[1].t - p[0].t >= 0.05)
            .map(|p| (p[0], p[1]));
        let (cs, bs, ang, probe) = pair.map_or((0.0, 0.0, -1.0, "nopair".to_owned()), |(p, x)| {
            let dt = (x.t - p.t) as f32;
            let c = step(&p.client, &x.client);
            let bo = step(&p.client, &x.body);
            let (lc, lb) = (c.0.hypot(c.1), bo.0.hypot(bo.1));
            let ang = if lc > 0.2 && lb > 0.2 {
                ((c.0 * bo.0 + c.1 * bo.1) / (lc * lb)).clamp(-1.0, 1.0).acos().to_degrees()
            } else {
                -1.0
            };
            let probe = match phys_ext::physics_obj(&b.ts.world, b.g) {
                None => "nobody".to_owned(),
                Some(h) => {
                    let from = phys_ext::to_physics_position(&p.client);
                    let to = phys_ext::to_physics_position(&x.client);
                    match b.ts.world.physics.transition(h, &from, &to, false) {
                        None => "refused".into(),
                        Some(t) => {
                            let end = empyrean_world::world_objects::player_tick::physics_position_to_location(
                                &t.sphere_path.curr_pos,
                            );
                            format!("short={:.2}\thit={}", flat_global(&x.client, &end), hit_names(b, &t))
                        }
                    }
                }
            };
            (lc / dt, lb / dt, ang, probe)
        });
        println!(
            "win\t{name}\t{t_pull:.2}\t{d:.1}\tn={}\tlastoff={last_off:.1}\tlastdz={last_dz:.2}\tcs={cs:.1}\tbs={bs:.1}\tang={ang:.0}\tnz={:.2}\twalk={}\tkeys={:04X}@{:.2}\tcell={:08X}\tindoor={}\t{probe}\ttouch={}",
            w.len(),
            last.nz,
            last.walkable,
            last.fwd & 0xFFFF,
            last.fwd_speed,
            last.client.cell(),
            last.client.cell() & 0xFFFF >= 0x100,
            touched(b),
        );
    }

    /// Whether the server is walking the character to a target (a move-to chain for an attack or
    /// a use), in which the sweep is not run.
    fn moving_to(b: &Bot) -> bool {
        empyrean_world::world_objects::player_move::is_player_moving_to(&b.ts.world, b.g)
            || empyrean_world::world_objects::player_move2::fields(&b.ts.world, b.g)
                .is_player_moving_to2
            || empyrean_world::physics::phys_ext::physics_obj(&b.ts.world, b.g)
                .is_some_and(|h| empyrean_world::physics::motion::is_moving_to(&b.ts.world, h))
    }

    /// How far the report `at` is from overlapping the nearest creature's body (the two bodies'
    /// radii apart less their distance, flat, negative when they overlap), in meters.
    fn creature_clearance(b: &Bot, at: &Position) -> f32 {
        let my_r = empyrean_world::physics::phys_ext::physics_obj(&b.ts.world, b.g)
            .and_then(|h| b.ts.world.physics.get(h))
            .map_or(0.0, |o| o.radius());
        let mut best = f32::MAX;
        for id in empyrean_world::managers::landblock_manager::get_loaded_landblocks(&b.ts.world) {
            if id.raw() >> 16 != at.cell() >> 16 {
                continue;
            }
            let Some(l) = b.ts.world.landblock_manager.landblocks.get(id) else {
                continue;
            };
            for g in l.get_all_world_objects_for_diagnostics() {
                let Some(o) = b.ts.world.objects.get(g) else {
                    continue;
                };
                if g == b.g || !o.is_creature() || o.is_player() {
                    continue;
                }
                let Some(loc) = o.location() else {
                    continue;
                };
                if (loc.position_z - at.position_z).abs() > 2.0 {
                    continue;
                }
                let r = o
                    .phys
                    .and_then(|oh| b.ts.world.physics.get(oh))
                    .map_or(0.0, |po| po.radius());
                let (dx, dy) = step(at, &loc);
                best = best.min(dx.hypot(dy) - my_r - r);
            }
        }
        best
    }

    /// Whether the server holds the character's body short of its last report, to send it back
    /// (its motion correction's pending force).
    fn force_due(b: &Bot) -> bool {
        b.ts.world
            .objects
            .get(b.g)
            .and_then(|o| o.player.as_ref())
            .is_some_and(|p| p.player_tick.force_due.is_some())
    }

    /// Where the server's own sweep stopped short of the report `at` that the instrument's sweep
    /// reached: across the ground, or over it but too far below or above.
    fn server_stop(b: &Bot, at: &Position) -> &'static str {
        use empyrean_world::world_objects::player_tick::SWEEP_REACHED;
        match body_now(b) {
            Some(p)
                if {
                    let (dx, dy) = step(at, &p);
                    dx * dx + dy * dy <= SWEEP_REACHED * SWEEP_REACHED
                } =>
            {
                "server's sweep only, height"
            }
            Some(_) => "server's sweep only, short",
            None => "server's sweep only",
        }
    }

    /// The position an UpdatePosition carries.
    fn pack_position(p: &dereth_protocol::movement::PositionPack) -> Position {
        Position::from_components(
            p.origin.objcell_id,
            p.origin.origin.x,
            p.origin.origin.y,
            p.origin.origin.z,
            p.orientation.x,
            p.orientation.y,
            p.orientation.z,
            p.orientation.w,
            false,
        )
    }

    /// The client's other actions, sent as the replay's own: each object they name is the replay's
    /// nearest of its weenie class to where the captured server last had it (the recorded player is
    /// the replay's character), and a spell cast is one the character knows, with mana to spare and
    /// no components. What could not be sent is counted by why.
    struct Acts {
        player: u32,
        /// what the captured server had told the client of each object: its weenie class (0 when
        /// unknown), whether it was in a pack or wielded, and where it last was
        known: HashMap<u32, (u32, bool, Option<Position>)>,
        /// each object's name, as the captured server gave it
        names: HashMap<u32, String>,
        mapped: HashMap<u32, ObjectGuid>,
        /// the replay's creatures the session attacked or cast at
        fought: HashSet<ObjectGuid>,
        /// the replay's creatures standing for one of retail's
        taken: HashSet<ObjectGuid>,
        tally: BTreeMap<&'static str, usize>,
        /// what became of the last action
        last: &'static str,
    }

    impl Acts {
        fn new(player: u32) -> Self {
            Self {
                player,
                known: HashMap::new(),
                names: HashMap::new(),
                mapped: HashMap::new(),
                fought: HashSet::new(),
                taken: HashSet::new(),
                tally: BTreeMap::new(),
                last: "",
            }
        }

        fn count(&mut self, why: &'static str) {
            *self.tally.entry(why).or_insert(0) += 1;
            self.last = why;
        }

        /// The character casts anything it is asked to: trained in every magic skill far past any
        /// spell's difficulty, its attributes high, no components needed.
        fn prepare(b: &mut Bot) {
            use empyrean_entity::enums::{PropertyAttribute, SkillAdvancementClass};
            let g = b.g;
            if let Some(o) = b.ts.world.objects.get_mut(g) {
                o.set_spell_components_required(false);
                for sk in [
                    Skill::CreatureEnchantment,
                    Skill::ItemEnchantment,
                    Skill::LifeMagic,
                    Skill::WarMagic,
                    Skill::VoidMagic,
                    Skill::ManaConversion,
                ] {
                    let (ps, _) = o.biota.get_or_add_skill(sk);
                    ps.sac = SkillAdvancementClass::Specialized;
                    ps.init_level = 1000;
                }
                let attributes = o
                    .biota
                    .properties_attribute
                    .get_or_insert_with(Default::default);
                for a in &PropertyAttribute::ALL[1..] {
                    let e = attributes.get_or_insert_with(*a, Default::default);
                    e.init_level = e.init_level.max(300);
                }
            }
            Self::refill(b);
        }

        /// The character's mana full again, and far more than any spell costs.
        fn refill(b: &mut Bot) {
            use empyrean_entity::enums::PropertyAttribute2nd;
            let g = b.g;
            if let Some(o) = b.ts.world.objects.get_mut(g) {
                let vitals = o
                    .biota
                    .properties_attribute_2nd
                    .get_or_insert_with(Default::default);
                let m = vitals.get_or_insert_with(PropertyAttribute2nd::MaxMana, Default::default);
                m.init_level = m.init_level.max(100_000);
                m.current_level = 100_000;
            }
        }

        fn learn(b: &mut Bot, spell: u32) {
            let g = b.g;
            let _ = empyrean_world::world_objects::player_spells::add_known_spell(
                &mut b.ts.world,
                g,
                spell,
            );
            Self::refill(b);
        }

        /// The replay's object for the captured one `retail`, or why there is none.
        fn target(&mut self, b: &mut Bot, retail: u32) -> Result<u32, &'static str> {
            if retail == self.player {
                return Ok(b.g.full());
            }
            if (0x5000_0000..0x6000_0000).contains(&retail) {
                return Err("skipped: another player");
            }
            let Some(&(wcid, held, at)) = self.known.get(&retail) else {
                return Err("skipped: object not seen");
            };
            if held {
                return Err("skipped: an item in a pack or wielded");
            }
            if wcid == 0 {
                return Err("skipped: object's class not seen");
            }
            let Some(at) = at else {
                return Err("skipped: object's place not seen");
            };
            let near = |o: &empyrean_world::world_objects::world_object::WorldObject| {
                o.location().map(|l| apart(&l, &at))
            };
            if let Some(&g) = self.mapped.get(&retail) {
                if let Some(d) = b.ts.world.objects.get(g).and_then(near) {
                    if d < MAP_RADIUS {
                        return Ok(g.full());
                    }
                }
            }
            let (bx, by) = (at.cell() >> 24, (at.cell() >> 16) & 0xFF);
            let name = self.names.get(&retail).cloned().unwrap_or_default();
            // one of its weenie class, or a creature of its name (the world's own spawns can be
            // another class of the same monster); among those one attacking the character
            // (retail's was likely fighting the player), then the nearest; none already standing
            // for another of retail's objects, and no dead creature
            let mut best: Option<(bool, bool, f32, ObjectGuid)> = None;
            for id in
                empyrean_world::managers::landblock_manager::get_loaded_landblocks(&b.ts.world)
            {
                let (x, y) = (id.raw() >> 24, (id.raw() >> 16) & 0xFF);
                if x.abs_diff(bx) > 1 || y.abs_diff(by) > 1 {
                    continue;
                }
                let Some(l) = b.ts.world.landblock_manager.landblocks.get(id) else {
                    continue;
                };
                for g in l.get_all_world_objects_for_diagnostics() {
                    let Some(o) = b.ts.world.objects.get(g) else {
                        continue;
                    };
                    let other_class = o.weenie_class_id() != wcid;
                    if other_class
                        && !(o.is_creature()
                            && !name.is_empty()
                            && o.get_property(empyrean_entity::enums::PropertyString::Name)
                                .is_some_and(|n| n == name))
                    {
                        continue;
                    }
                    if self.taken.contains(&g)
                        || o.is_creature()
                            && empyrean_world::world_objects::monster_combat::is_dead(o)
                    {
                        continue;
                    }
                    if let Some(d) = near(o) {
                        let idle = !o.is_creature()
                            || empyrean_world::world_objects::monster_combat::attack_target(
                                &b.ts.world,
                                g,
                            ) != Some(b.g);
                        let key = (other_class, idle, d);
                        if d < MAP_RADIUS && best.is_none_or(|(bc, bi, bd, _)| key < (bc, bi, bd)) {
                            best = Some((other_class, idle, d, g));
                        }
                    }
                }
            }
            let g = match best {
                Some((other_class, _, _, g)) => {
                    if other_class {
                        *self.tally.entry("mapped by name").or_insert(0) += 1;
                    }
                    g
                }
                // none near: one of its class made where the captured server last had it
                None => match stand_in(b, wcid, &at) {
                    Some(g) => {
                        *self.tally.entry("stand-in made").or_insert(0) += 1;
                        g
                    }
                    None => return Err("skipped: none of its class near, none made"),
                },
            };
            if let Some(old) = self.mapped.insert(retail, g) {
                self.taken.remove(&old);
            }
            if b.ts.world.objects.get(g).is_some_and(|o| o.is_creature()) {
                self.taken.insert(g);
            }
            Ok(g.full())
        }

        fn handle(&mut self, b: &mut Bot, a: &Act) {
            match a {
                Act::Created(raw) => {
                    if let Ok(m) = dereth_protocol::read_body_padded::<
                        dereth_protocol::objects::ItemCreateObject,
                    >(&raw[4..])
                    {
                        let p = &m.0;
                        let held = p.wdesc.container_id.is_some()
                            || p.wdesc.wielder_id.is_some()
                            || p.physicsdesc.position.is_none();
                        let at = p.physicsdesc.position.as_ref().map(position_of);
                        self.known.insert(p.id.0, (p.wdesc.wcid, held, at));
                        self.names.insert(p.id.0, p.wdesc.name.clone());
                        self.mapped.remove(&p.id.0);
                    }
                }
                Act::Placed(raw) => {
                    if let Ok(m) =
                        dereth_protocol::read_body_padded::<MovementPositionEvent>(&raw[4..])
                    {
                        let at = pack_position(&m.position);
                        self.known
                            .entry(m.id.0)
                            .and_modify(|k| {
                                k.1 = false;
                                k.2 = Some(at);
                            })
                            .or_insert((0, false, Some(at)));
                    }
                }
                Act::Died(raw) => {
                    let Some(word) = raw.get(4..8) else {
                        return;
                    };
                    let retail = u32::from_le_bytes(word.try_into().expect("four bytes"));
                    let Some(g) = self.mapped.remove(&retail) else {
                        self.count("retail's target died: none of ours");
                        return;
                    };
                    self.taken.remove(&g);
                    let alive = b.ts.world.objects.get(g).is_some_and(|o| {
                        o.is_creature()
                            && !empyrean_world::world_objects::monster_combat::is_dead(o)
                    });
                    if alive {
                        let me = b.g;
                        empyrean_world::world_objects::creature_death::smite(
                            &mut b.ts.world,
                            g,
                            me,
                            true,
                        );
                        self.count("retail's target died: ours killed");
                    } else {
                        self.count("retail's target died: ours already dead");
                    }
                }
                Act::Do { opcode, body } => {
                    let (name, targets): (&'static str, usize) = match opcode {
                        0x0008 => ("sent: melee attack", 1),
                        0x000A => ("skipped: missile attack (no bow), its target mapped", 1),
                        0x0048 => ("sent: untargeted cast", 0),
                        0x004A => ("sent: targeted cast", 1),
                        0x0035 => ("sent: use with target", 2),
                        0x0036 => ("sent: use", 1),
                        0x0053 => ("sent: combat mode", 0),
                        0x01B7 => ("sent: cancel attack", 0),
                        _ => return,
                    };
                    let mut body = body.clone();
                    for k in 0..targets {
                        let Some(word) = body.get(4 * k..4 * k + 4) else {
                            self.count("skipped: too short");
                            return;
                        };
                        let retail = u32::from_le_bytes(word.try_into().expect("four bytes"));
                        match self.target(b, retail) {
                            Ok(g) => {
                                body[4 * k..4 * k + 4].copy_from_slice(&g.to_le_bytes());
                                let g = ObjectGuid::new(g);
                                if matches!(opcode, 0x0008 | 0x000A | 0x004A)
                                    && g != b.g
                                    && b.ts.world.objects.get(g).is_some_and(|o| o.is_creature())
                                {
                                    self.fought.insert(g);
                                }
                            }
                            Err(why) => {
                                self.count(why);
                                return;
                            }
                        }
                    }
                    let spell_at = match opcode {
                        0x0048 => Some(0),
                        0x004A => Some(4),
                        _ => None,
                    };
                    if let Some(i) = spell_at {
                        if let Some(word) = body.get(i..i + 4) {
                            Self::learn(
                                b,
                                u32::from_le_bytes(word.try_into().expect("four bytes")),
                            );
                        }
                    }
                    if *opcode != 0x000A {
                        let id = b.id;
                        b.ts.send_game_action_raw(id, dereth_protocol::Opcode(*opcode), &body);
                    }
                    self.count(name);
                }
            }
        }
    }

    /// A new object of weenie class `wcid` placed at `at`, as a generator places one.
    fn stand_in(b: &mut Bot, wcid: u32, at: &Position) -> Option<ObjectGuid> {
        use empyrean_world::factories::world_object_factory::create_new_world_object_by_wcid_in_world;
        let w = &mut b.ts.world;
        let mut o = create_new_world_object_by_wcid_in_world(w, wcid)?;
        o.set_location(Some(*at));
        let g = o.guid;
        if w.objects.insert(o).is_err() {
            return None;
        }
        empyrean_world::world_objects::creature::post_insert(w, g);
        empyrean_world::managers::landblock_manager::add_object(w, g, false).then_some(g)
    }

    /// The combat mode a stance (a motion state's style, low word) is held in, as
    /// Combat_ChangeCombatMode's body: none for an unknown stance.
    fn mode_of(style: u32) -> Option<u32> {
        match style & 0xFFFF {
            0x3D => Some(1),
            0x49 => Some(8),
            0x3F | 0x41 | 0x43 | 0x47 => Some(4),
            0x3C | 0x3E | 0x40 | 0x44 | 0x45 | 0x46 | 0x48 => Some(2),
            _ => None,
        }
    }

    /// The errors the server sent the character (Communication_WeenieError and its string form), by
    /// code, from message `from`.
    fn weenie_errors(b: &Bot, from: usize, into: &mut BTreeMap<u32, usize>) {
        for m in &b.ts.received_raw(b.id)[from..] {
            // an ordered game event: its object, sequence and type, then the error
            if m.opcode != 0xF7B0 || m.body.len() < 16 {
                continue;
            }
            let ty = u32::from_le_bytes(m.body[8..12].try_into().expect("four bytes"));
            if ty == 0x028A || ty == 0x028B {
                let code = u32::from_le_bytes(m.body[12..16].try_into().expect("four bytes"));
                *into.entry(code).or_insert(0) += 1;
            }
        }
    }

    fn replay(s: &Stream) -> Replayed {
        let mut b = create_character(&[]);
        b.enter_world();
        assert!(empyrean_world::managers::property_manager::modify_bool(
            &b.ts.world,
            "server_motion_correction",
            true
        ));
        empyrean_world::world_objects::player_tick::refresh_movement_options(&mut b.ts.world);
        // a new character where the recorded one hunted: nothing harms it, so no death takes it
        // off its path
        let g = b.g;
        if let Some(o) = b.ts.world.objects.get_mut(g) {
            o.set_invincible(true);
        }
        set_run(&mut b, skill_for(running_speed(s)));
        let first = position_of(s.reports[0].1.position());
        teleport_to(&mut b, &first);
        let pull_back_on =
            empyrean_world::world_objects::player_tick::motion_correction_enabled(&b.ts.world);
        // EMPYREAN_REPLAY_ACTIONS=0: the reports only, as before the actions were replayed
        let send_acts = std::env::var("EMPYREAN_REPLAY_ACTIONS").map_or(true, |v| v != "0");
        let mut acts = Acts::new(s.player);
        if send_acts {
            Acts::prepare(&mut b);
        }
        let errors_from = b.mark();
        // the stance the character was last put in from the client's own keys, and whether a
        // recorded mode change was sent since the client's last keys
        let mut synced_style: Option<u32> = None;
        let mut mode_act_since = false;
        let guid = b.g.full();
        let mut scanned = b.mark();
        let mut force_now = server_force(&mut b);
        let mut force_sent: Vec<(f64, u16)> = vec![(f64::MIN, force_now)];
        let mut pulled = Vec::new();
        let mut timing: Vec<(f64, f64)> = Vec::new();
        // what stopped the sweep of the first blocked report since the last pull-back, and at each
        let mut pending_block: Option<&'static str> = None;
        let mut blockers: Vec<&'static str> = Vec::new();
        let mut last_report_t = 0.0f64;
        let (t_rec0, mut anchor) = (s.reports[0].0, b.ts.seconds());
        let mut seg_start = t_rec0;
        let mut recorded_teleport = s.reports[0].1.stamps().teleport;
        let mut resync = false;
        let mut last_sent = b.ts.seconds();
        let mut server_teleports = 0usize;
        // at each pull-back: the body standing while the client's last keys moved it
        let mut stalled: Vec<bool> = Vec::new();
        let mut keys_moving = false;
        let mut body_trail: Vec<(f64, Position)> = Vec::new();
        let mut touching: Vec<&'static str> = Vec::new();
        let mut last_at = first;
        let mut samples: Vec<Sample> = Vec::new();
        let (mut keys_fwd, mut keys_fwd_speed) = (0u32, 1.0f32);
        // EMPYREAN_TRACE_AT: stream times to trace the 30 s before (comma-separated)
        let trace_at: Vec<f64> = std::env::var("EMPYREAN_TRACE_AT")
            .ok()
            .map(|v| v.split(',').filter_map(|x| x.trim().parse().ok()).collect())
            .unwrap_or_default();
        let tracing = |t: f64| trace_at.iter().any(|at| t <= *at + 0.5 && t >= at - 30.0);
        let mut motions_seen = b.mark();
        let mut due_seen = false;
        // when the server last had the character in a move-to chain, and at the pending force
        let mut last_moveto = f64::MIN;
        let mut due_moveto: Option<(f64, f32, f32)> = None;
        let mut moveto: Vec<(f64, f32, f32)> = Vec::new();
        for (t, e) in &s.events {
            let r = match e {
                Event::Report(i) => Some(&s.reports[*i].1),
                Event::Act(_) if !send_acts => continue,
                Event::Act(_) => None,
            };
            let at = r.map_or(last_at, |r| position_of(r.position()));
            if r.is_some_and(|r| r.stamps().teleport != recorded_teleport) {
                // the recording teleported: go on from its next report
                recorded_teleport = r.map_or(recorded_teleport, |r| r.stamps().teleport);
                teleport_to(&mut b, &at);
                scanned = b.mark();
                force_now = server_force(&mut b);
                force_sent.push((f64::MIN, force_now));
                anchor = b.ts.seconds();
                seg_start = *t;
            }
            let due = anchor + (t - seg_start);
            while b.ts.seconds() < due {
                if b.ts.seconds() - last_sent >= KEEP_ALIVE_S {
                    b.action(&dereth_protocol::admin::CharacterRequestPing);
                    last_sent = b.ts.seconds();
                }
                b.ts.step();
                // the server logged the character off (a stream it can't follow): skipped
                assert!(
                    b.ts.world.objects.contains(b.g),
                    "the character left the world"
                );
                // the server's own sweep stopped short of a report (it will send the player back):
                // where the instrument's sweep reached it, a class of its own
                if moving_to(&b) {
                    last_moveto = b.ts.seconds();
                }
                let due = force_due(&b);
                if due && !due_seen {
                    if pending_block.is_none() {
                        pending_block = Some(server_stop(&b, &last_at));
                    }
                    let now = b.ts.seconds();
                    let speed = body_trail
                        .iter()
                        .rev()
                        .find(|(when, _)| *when <= now - 0.5)
                        .zip(body_trail.last())
                        .map_or(0.0, |((t0, a), (t1, z))| {
                            #[allow(clippy::cast_possible_truncation)] // meters a second
                            let dt = (t1 - t0).max(0.01) as f32;
                            apart(a, z) / dt
                        });
                    due_moveto = Some((now - last_moveto, speed, creature_clearance(&b, &last_at)));
                }
                due_seen = due;
                if let Some(p) = body_now(&b) {
                    let st = b.ts.seconds() - anchor + (seg_start - t_rec0);
                    if tracing(st) {
                        // the motions the server sent the character since the last step: whether
                        // it echoes the client's own (autonomous) and the forward command
                        for m in &b.ts.received_raw(b.id)[motions_seen..] {
                            if m.opcode == 0xF74C && m.body.len() >= 20 {
                                let id = u32::from_le_bytes(
                                    m.body[0..4].try_into().expect("four bytes"),
                                );
                                if id == b.g.full() {
                                    let flags = u32::from_le_bytes(
                                        m.body[16..20].try_into().expect("four bytes"),
                                    );
                                    let mut fwd = 0u16;
                                    if m.body[12] == 0 && flags & 2 != 0 {
                                        let at = 20 + if flags & 1 != 0 { 2 } else { 0 };
                                        fwd = u16::from_le_bytes(
                                            m.body[at..at + 2].try_into().expect("two bytes"),
                                        );
                                    }
                                    println!(
                                        "trace\tmotion\t{st:.2}\t{}\ttype={}\tflags={flags:X}\tfwd={fwd:04X}",
                                        if m.body[10] == 0 { "server" } else { "own" },
                                        m.body[12],
                                    );
                                }
                            }
                        }
                        let (fc, fs, nz, walk, vz) =
                            empyrean_world::physics::phys_ext::physics_obj(&b.ts.world, b.g)
                                .map_or((0, 0.0, 0.0, false, 0.0), |h| {
                                    let (fc, fs) = empyrean_world::physics::motion::with_driver(
                                        &b.ts.world,
                                        h,
                                        |d| {
                                            let s = &d.movement.interp.interpreted_state;
                                            (s.forward_command.0 & 0xFFFF, s.forward_speed)
                                        },
                                    )
                                    .unwrap_or((0, 0.0));
                                    let o = b.ts.world.physics.get(h);
                                    (
                                        fc,
                                        fs,
                                        o.map_or(0.0, |o| o.contact_plane.normal.z),
                                        o.is_some_and(|o| o.transient_state.on_walkable()),
                                        o.map_or(0.0, |o| o.velocity_vector.z),
                                    )
                                });
                        println!(
                            "trace\tbody\t{st:.2}\t{:08X}\t{:.2},{:.2},{:.2}\tfwd={fc:04X}@{fs:.2}\tnz={nz:.2}\twalk={walk}\tvz={vz:.1}\ttouch={}",
                            p.cell(), p.position_x, p.position_y, p.position_z, touched(&b)
                        );
                    }
                    motions_seen = b.ts.received_raw(b.id).len();
                    body_trail.push((b.ts.seconds(), p));
                    if body_trail.len() > 120 {
                        body_trail.drain(..60);
                    }
                }
                // A teleport the server made on its own (a portal walked into, a death): the
                // client comes out of portal space, and the stream goes on from its next report.
                if !decode::all_of::<EffectsPlayerTeleport>(&b.ts.received_raw(b.id)[scanned..])
                    .is_empty()
                {
                    b.advance(0.5);
                    b.action(&CharacterLoginCompleteNotification);
                    b.advance(0.5);
                    out_of_portal_space(&mut b);
                    scanned = b.mark();
                    force_now = server_force(&mut b);
                    force_sent.push((f64::MIN, force_now));
                    server_teleports += 1;
                    resync = true;
                    continue;
                }
                let got = positions_of(&b.ts, b.id, scanned, guid);
                scanned = b.ts.received_raw(b.id).len();
                for p in got {
                    if p.force != force_now {
                        force_now = p.force;
                        force_sent.push((b.ts.seconds(), p.force));
                        let back = Position::from_components(
                            p.cell,
                            p.x,
                            p.y,
                            last_at.position_z,
                            0.0,
                            0.0,
                            0.0,
                            1.0,
                            false,
                        );
                        let mut flat_last = last_at;
                        flat_last.position_z = back.position_z;
                        pulled.push((t - t_rec0, apart(&flat_last, &back)));
                        timing.push((
                            b.ts.seconds() - anchor + (seg_start - t_rec0),
                            last_report_t,
                        ));
                        blockers.push(pending_block.take().unwrap_or("not the sweep"));
                        moveto.push(due_moveto.take().unwrap_or((-1.0, -1.0, f32::MAX)));
                        win_line(
                            &mut b,
                            &s.name,
                            t - t_rec0,
                            apart(&flat_last, &back),
                            &samples,
                        );
                        // the body moved less than half a meter in the half second before
                        let still = body_trail
                            .iter()
                            .rev()
                            .find(|(when, _)| *when <= b.ts.seconds() - 0.5)
                            .zip(body_trail.last())
                            .is_some_and(|((_, a), (_, z))| apart(a, z) < 0.5);
                        stalled.push(still && keys_moving);
                        touching.push(touched(&b));
                        resync = true;
                    }
                }
            }
            let Some(r) = r else {
                if let Event::Act(a) = e {
                    if matches!(a, Act::Do { opcode: 0x0053, .. }) {
                        mode_act_since = true;
                    }
                    acts.last = "";
                    acts.handle(&mut b, a);
                    if tracing(t - t_rec0) && !acts.last.is_empty() {
                        println!("trace\tact\t{:.2}\t{}", t - t_rec0, acts.last);
                    }
                }
                last_sent = b.ts.seconds();
                continue;
            };
            if resync {
                put_body(&mut b, &at);
                resync = false;
                samples.clear();
            } else if let Some(body) = body_now(&b) {
                let (nz, walkable) =
                    empyrean_world::physics::phys_ext::physics_obj(&b.ts.world, b.g)
                        .and_then(|h| b.ts.world.physics.get(h))
                        .map_or((1.0, true), |o| {
                            (o.contact_plane.normal.z, o.transient_state.on_walkable())
                        });
                samples.push(Sample {
                    t: t - t_rec0,
                    client: at,
                    body,
                    nz,
                    walkable,
                    fwd: keys_fwd,
                    fwd_speed: keys_fwd_speed,
                });
                if samples.len() > 400 {
                    samples.drain(..200);
                }
            }
            let now = b.ts.seconds();
            let taken = force_sent
                .iter()
                .rev()
                .find(|(when, _)| *when <= now - STAMP_TAKEN_S)
                .map_or(force_now, |(_, f)| *f);
            if let Sent::Keys(k) = r {
                let st = &k.0.raw_motion_state;
                keys_fwd = st.forward_command.unwrap_or(0);
                keys_fwd_speed = st.forward_speed.unwrap_or(1.0);
                keys_moving = st.forward_command.is_some()
                    || st.sidestep_command.is_some()
                    || st.turn_command.is_some();
            }
            if tracing(t - t_rec0) {
                let kind = match r {
                    Sent::Keys(k) => {
                        let st = &k.0.raw_motion_state;
                        format!(
                            "keys fwd={:04X}@{:.2} side={:04X} turn={:04X} hold={:?}",
                            st.forward_command.unwrap_or(0) & 0xFFFF,
                            st.forward_speed.unwrap_or(1.0),
                            st.sidestep_command.unwrap_or(0) & 0xFFFF,
                            st.turn_command.unwrap_or(0) & 0xFFFF,
                            st.current_holdkey
                        )
                    }
                    Sent::Place(_) => "place".into(),
                    Sent::Jump(_) => "jump".into(),
                };
                println!(
                    "trace\treport\t{:.2}\t{:08X}\t{:.2},{:.2},{:.2}\t{kind}",
                    t - t_rec0,
                    at.cell(),
                    at.position_x,
                    at.position_y,
                    at.position_z
                );
            }
            if pending_block.is_none() && pull_back_on {
                pending_block = sweep_blocker(&mut b, &at, &acts);
            }
            // The character in the combat mode of the client's stance: a session captured already
            // in combat, or a mode change the replay could not follow.
            if let (true, Sent::Keys(k)) = (send_acts, r) {
                if let Some(style) = k.0.raw_motion_state.current_style {
                    if synced_style != Some(style) && !mode_act_since {
                        if let Some(mode) = mode_of(style) {
                            let id = b.id;
                            b.ts.send_game_action_raw(
                                id,
                                dereth_protocol::Opcode(0x0053),
                                &mode.to_le_bytes(),
                            );
                            *acts
                                .tally
                                .entry("sent: combat mode from the client's stance")
                                .or_insert(0) += 1;
                        }
                    }
                    synced_style = Some(style);
                }
                mode_act_since = false;
            }
            let mut m = r.clone();
            m.set_stamps(stamps_now(&mut b, taken));
            match &m {
                Sent::Keys(x) => b.action(x),
                Sent::Place(x) => b.action(x),
                Sent::Jump(x) => b.action(x),
            }
            last_at = at;
            last_report_t = t - t_rec0;
            last_sent = b.ts.seconds();
        }
        // the creatures the session fought, and how many of them it killed (dead or gone by the end)
        let fought = acts.fought.len();
        let killed = acts
            .fought
            .iter()
            .filter(|g| {
                b.ts.world
                    .objects
                    .get(**g)
                    // a guid destroyed and taken again by another kind of object counts as killed
                    .is_none_or(|o| {
                        !o.is_creature()
                            || empyrean_world::world_objects::monster_combat::is_dead(o)
                    })
            })
            .count();
        *acts.tally.entry("fought: creatures").or_insert(0) += fought;
        *acts
            .tally
            .entry("fought: dead or gone by the end")
            .or_insert(0) += killed;
        let mut errors = BTreeMap::new();
        weenie_errors(&b, errors_from, &mut errors);
        Replayed {
            pulled,
            timing,
            blockers,
            moveto,
            stalled,
            touching,
            teleports: server_teleports,
            acts: acts.tally,
            errors,
        }
    }

    fn band_of(d: f32) -> usize {
        BANDS.iter().position(|b| d < *b).unwrap_or(BANDS.len())
    }

    fn per_hour(n: usize, hours: f64) -> f64 {
        #[allow(clippy::cast_precision_loss)]
        let n = n as f64;
        if hours > 0.0 {
            n / hours
        } else {
            0.0
        }
    }

    #[test]
    #[ignore = "an instrument over captured report streams; run by hand"]
    fn pull_back_replay() {
        let Some(dir) = std::env::var_os("EMPYREAN_REPORT_STREAMS") else {
            println!("pull_back_replay: skipped (EMPYREAN_REPORT_STREAMS unset)");
            return;
        };
        let limit: usize = std::env::var("EMPYREAN_REPORT_STREAMS_LIMIT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(usize::MAX);
        let mut paths: Vec<_> = std::fs::read_dir(&dir)
            .expect("the streams' folder")
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "tsv"))
            .collect();
        paths.sort();
        if let Ok(only) = std::env::var("EMPYREAN_REPORT_STREAMS_ONLY") {
            paths.retain(|p| p.file_stem().is_some_and(|n| n.to_string_lossy() == only));
        }
        // every pull-back; those with none in the 10 s before (an episode's first); those with the
        // body standing while the client's keys moved it
        let mut ours = [[0usize; BANDS.len() + 1]; 3];
        let mut theirs = [[0usize; BANDS.len() + 1]; 2];
        let mut hours = 0.0;
        let mut skipped = 0;
        let mut all_acts: BTreeMap<&'static str, usize> = BTreeMap::new();
        let mut all_errors: BTreeMap<u32, usize> = BTreeMap::new();
        let mut streams = 0;
        println!(
            "stream\thours\treports\tspeed\tours\tours first\tours stalled\tretail\tretail first\tserver teleports"
        );
        let first_of = |v: &[(f64, f32)]| -> Vec<bool> {
            v.iter()
                .enumerate()
                .map(|(i, (t, _))| i == 0 || *t - v[i - 1].0 > EPISODE_S)
                .collect()
        };
        for path in paths.into_iter().take(limit) {
            let Some(s) = load(&path) else { continue };
            if s.hours <= 0.0 {
                continue;
            }
            // A stream the replay can't follow (its character lost on a teleport the server refuses,
            // or kept in portal space, or logged off) is counted and left out.
            let replayed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| replay(&s)));
            let Ok(Replayed {
                pulled,
                timing,
                blockers,
                moveto,
                stalled,
                touching,
                teleports,
                acts,
                errors,
            }) = replayed
            else {
                let why = replayed.err().and_then(|e| {
                    e.downcast_ref::<String>()
                        .cloned()
                        .or_else(|| e.downcast_ref::<&str>().map(|s| (*s).to_owned()))
                });
                println!(
                    "{}\tskipped: the replay lost its character ({})",
                    s.name,
                    why.unwrap_or_default()
                );
                skipped += 1;
                continue;
            };
            let ours_first = first_of(&pulled);
            let theirs_first = first_of(&s.forced);
            for (i, (_, d)) in pulled.iter().enumerate() {
                let band = band_of(*d);
                ours[0][band] += 1;
                if ours_first[i] {
                    ours[1][band] += 1;
                }
                if stalled.get(i).copied().unwrap_or(false) {
                    ours[2][band] += 1;
                }
            }
            for (i, (_, d)) in s.forced.iter().enumerate() {
                theirs[0][band_of(*d)] += 1;
                if theirs_first[i] {
                    theirs[1][band_of(*d)] += 1;
                }
            }
            hours += s.hours;
            streams += 1;
            for (k, n) in acts {
                *all_acts.entry(k).or_insert(0) += n;
            }
            for (k, n) in errors {
                *all_errors.entry(k).or_insert(0) += n;
            }
            let count = |v: &[bool]| v.iter().filter(|x| **x).count();
            println!(
                "{}\t{:.2}\t{}\t{:.1}\t{}\t{}\t{}\t{}\t{}\t{}",
                s.name,
                s.hours,
                s.reports.len(),
                running_speed(&s),
                pulled.len(),
                count(&ours_first),
                count(&stalled),
                s.forced.len(),
                count(&theirs_first),
                teleports
            );
            let t_rec0 = s.reports[0].0;
            for (i, (t, d)) in s.forced.iter().enumerate() {
                println!(
                    "retail\t{}\t{:.2}\t{d:.2}\t{}\t{:.2}\t{:.2}",
                    s.name,
                    t - t_rec0,
                    u8::from(theirs_first[i]),
                    t - s.forced_last.get(i).copied().unwrap_or(*t),
                    client_speed(&s, *t),
                );
            }
            for (i, (t, d)) in pulled.iter().enumerate() {
                let sent = timing.get(i).map_or(*t, |x| x.0);
                println!(
                    "pull\t{}\t{t:.2}\t{d:.2}\t{}\t{}\t{}\t{:.2}\t{:.2}\t{:.2}\t{}\t{:.2}\t{:.2}\t{:.2}",
                    s.name,
                    u8::from(ours_first[i]),
                    u8::from(stalled.get(i).copied().unwrap_or(false)),
                    touching.get(i).copied().unwrap_or("none"),
                    sent,
                    timing.get(i).map_or(*t, |x| x.1),
                    client_speed(&s, sent + t_rec0),
                    blockers.get(i).copied().unwrap_or("unknown"),
                    moveto.get(i).map_or(-1.0, |m| m.0.min(1e6)),
                    moveto.get(i).map_or(-1.0, |m| m.1),
                    moveto.get(i).map_or(99.0, |m| m.2.min(99.0)),
                );
            }
        }
        println!("\n{hours:.1} hours over {streams} streams ({skipped} skipped)");
        for (k, n) in &all_acts {
            println!("acts\t{k}\t{n}\t{:.1}/h", per_hour(*n, hours));
        }
        for (k, n) in &all_errors {
            println!("errors\t0x{k:04X}\t{n}\t{:.1}/h", per_hour(*n, hours));
        }
        let names = ["<1 m", "1-5 m", "5-10 m", "10-15 m", "15-30 m", ">=30 m"];
        println!("band\tours/h\tours first/h\tours stalled/h\tretail/h\tretail first/h");
        for (i, name) in names.iter().enumerate() {
            println!(
                "{name}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}",
                per_hour(ours[0][i], hours),
                per_hour(ours[1][i], hours),
                per_hour(ours[2][i], hours),
                per_hour(theirs[0][i], hours),
                per_hour(theirs[1][i], hours)
            );
        }
        let all = |v: &[usize]| per_hour(v.iter().sum(), hours);
        println!(
            "all\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}",
            all(&ours[0]),
            all(&ours[1]),
            all(&ours[2]),
            all(&theirs[0]),
            all(&theirs[1])
        );
    }
}
