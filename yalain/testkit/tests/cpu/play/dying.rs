//! A player's death: its tick, its corpse ten seconds later, the resurrection at its lifestone;
//! vitae.

use auberean_protocol::{queue, ServerMessage};
use yalain_store::Store;
use yalain_testkit::play::server_call;
use yalain_testkit::Harness;
use yalain_world::calls::{self, Value};
use yalain_world::death::{min_vitae, xp_to_raise};

const DEATH_MESSAGE: u32 = 0x8207_002B;
const WHO_KILLED: u32 = 0x8207_0030;
const DEATH_STATE: u32 = 0x8207_0033;
const RESURRECT: u32 = 0x8207_000E;
const CORPSE: u32 = 0x4700_0539;
const DEAD: u32 = 279;
const FOCUS: u32 = 266;

/// Let `seconds` pass, the connections talking.
fn wait(h: &mut Harness, seconds: f64) {
    let t = h.now.0 + seconds;
    while h.now.0 < t {
        h.step();
    }
}

fn scene_drawn(h: &mut Harness, b: usize) {
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
}

fn calls_of(h: &Harness, b: usize, from: usize) -> Vec<(u32, Vec<Value>)> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c) => Some((
                c.function.0,
                c.args
                    .as_ref()
                    .and_then(|a| calls::values(&a.0).ok())
                    .unwrap_or_default(),
            )),
            _ => None,
        })
        .collect()
}

/// A player one hit from death beside a creature that attacks it.
fn about_to_die(h: &mut Harness) -> (usize, u64) {
    h.add_creatures(1);
    let (b, me) = h.player_in_world("one", "Elara");
    scene_drawn(h, b);
    for p in h.server.players.values_mut() {
        yalain_world::quals::set_int(&mut p.qualities, 258, 1);
    }
    (b, me)
}

/// Run until the player's death message arrives; the index of the first message after it.
fn run_to_death(h: &mut Harness, b: usize, from: usize) -> usize {
    let died = h.run_until(4000, |h| {
        calls_of(h, b, from)
            .iter()
            .any(|(f, _)| *f == DEATH_MESSAGE)
    });
    assert!(died, "the creature kills the player");
    h.bots[b].inbox.len()
}

/// A player killed by a creature: its death message (the killer's name), and for all who see it:
/// dead, its killer announced on it, unselectable, posed, playing its death; the creature gives
/// up. Nothing more until its corpse ten seconds later: its client told it is dead, and a corpse
/// of the corpse template made where it fell, its originator the player.
/// Behaviour: server.death.player-dies
#[test]
fn a_player_killed_by_a_creature_dies_and_its_corpse_is_made_ten_seconds_later() {
    let mut h = Harness::new();
    let (b, me) = about_to_die(&mut h);
    let from = h.bots[b].inbox.len();
    let after = run_to_death(&mut h, b, from);
    let msgs = h.world_messages(b, from);
    assert!(
        msgs.iter()
            .any(|m| matches!(m, ServerMessage::UpdateBoolVisual(q)
            if q.object.id == me && q.quality == DEAD && q.value == 1)),
        "dead"
    );
    assert!(
        msgs.iter()
            .any(|m| matches!(m, ServerMessage::VisualObjectCall(c)
            if c.object.id == me && c.call.function.0 == WHO_KILLED)),
        "its killer announced"
    );
    assert!(
        calls_of(&h, b, from).iter().all(|(f, _)| *f != DEATH_STATE),
        "not yet told it is dead"
    );
    wait(&mut h, 9.0);
    assert!(calls_of(&h, b, after)
        .iter()
        .all(|(f, _)| *f != DEATH_STATE));
    wait(&mut h, 1.5);
    let states: Vec<Vec<Value>> = calls_of(&h, b, after)
        .into_iter()
        .filter(|(f, _)| *f == DEATH_STATE)
        .map(|(_, v)| v)
        .collect();
    assert_eq!(
        states,
        vec![vec![Value::Int(1)]],
        "dead, as its client knows"
    );
    let corpse = h
        .world_messages(b, after)
        .into_iter()
        .find_map(|m| match m {
            ServerMessage::CreateObject(o) if o.weenie.entity_did == CORPSE => Some(o),
            _ => None,
        });
    let corpse = corpse.expect("its corpse is made");
    assert_eq!(corpse.weenie.objects[3], me, "the player's corpse");
}

/// What follows an object made for a client since `from`: each message about it, after its
/// creation, as (what, its visual stamp).
fn block_after(h: &Harness, b: usize, from: usize, id: u64) -> Vec<(String, u16)> {
    let msgs = h.world_messages(b, from);
    let Some(at) = msgs
        .iter()
        .position(|m| matches!(m, ServerMessage::CreateObject(o) if o.id == id))
    else {
        return Vec::new();
    };
    msgs[at + 1..]
        .iter()
        .filter_map(|m| match m {
            ServerMessage::UpdateFloatVisual(q) if q.object.id == id => {
                Some((format!("{} {}", q.quality, q.value), q.object.other_stamp))
            }
            ServerMessage::DoFx(e) if e.sender.id == id => {
                Some((format!("fx {:x}", e.action.fx), e.sender.other_stamp))
            }
            ServerMessage::MoveSlider(e) if e.sender.id == id => {
                Some(("pose".into(), e.sender.other_stamp))
            }
            ServerMessage::SetMode(e) if e.sender.id == id => {
                Some((format!("mode {:x}", e.action.0), e.sender.other_stamp))
            }
            ServerMessage::UpdateInstanceIdVisual(q) if q.object.id == id => {
                Some((format!("{} {:x}", q.quality, q.value), q.object.other_stamp))
            }
            ServerMessage::UpdateStringInfoVisual(q) if q.object.id == id => {
                Some((format!("{} name", q.quality), q.object.other_stamp))
            }
            _ => None,
        })
        .collect()
}

/// A player's corpse is followed, as a creature's is, by the changes it was made up of: its scale,
/// its two effects, its pose, its mode, nobody's claim, its killer, the player it was and its name,
/// their visual stamps 2 to 10.
/// Behaviour: server.death.player-corpse-settles
#[test]
fn a_players_corpse_is_followed_by_its_settling_as_a_creatures_is() {
    let mut h = Harness::new();
    let (b, me) = about_to_die(&mut h);
    let from = h.bots[b].inbox.len();
    let after = run_to_death(&mut h, b, from);
    wait(&mut h, 10.5);
    let corpse = h
        .world_messages(b, after)
        .into_iter()
        .find_map(|m| match m {
            ServerMessage::CreateObject(o) if o.weenie.entity_did == CORPSE => Some(o),
            _ => None,
        })
        .expect("its corpse is made");
    let killer = h
        .server
        .things
        .get(corpse.id)
        .and_then(|t| t.corpse.as_ref())
        .map(|c| c.killer)
        .expect("a corpse");
    let block: Vec<(String, u16)> = block_after(&h, b, after, corpse.id)
        .into_iter()
        .take(9)
        .map(|(w, n)| {
            (
                if w.starts_with("298") {
                    "298".into()
                } else {
                    w
                },
                n,
            )
        })
        .collect();
    assert_eq!(
        block,
        vec![
            ("298".into(), 2),
            ("fx 41000002".into(), 3),
            ("fx 41000004".into(), 4),
            ("pose".into(), 5),
            ("mode 41000003".into(), 6),
            ("322 0".into(), 7),
            (format!("320 {killer:x}"), 8),
            (format!("321 {me:x}"), 9),
            ("256 name".into(), 10),
        ]
    );
}

/// Asked for once the corpse is made, the resurrection takes the player through portal space; a
/// second and a half later it is alive again with three quarters of its health.
/// Behaviour: server.death.resurrection
#[test]
fn a_dead_player_is_resurrected_with_three_quarters_of_its_health() {
    let mut h = Harness::new();
    let (b, me) = about_to_die(&mut h);
    let from = h.bots[b].inbox.len();
    let _ = run_to_death(&mut h, b, from);
    wait(&mut h, 11.0);
    let from = h.bots[b].inbox.len();
    h.bots[b].send(true, queue::WEENIE, &server_call(RESURRECT, &[]));
    wait(&mut h, 2.0);
    let states: Vec<Vec<Value>> = calls_of(&h, b, from)
        .into_iter()
        .filter(|(f, _)| *f == DEATH_STATE)
        .map(|(_, v)| v)
        .collect();
    assert_eq!(states, vec![vec![Value::Int(0)]], "alive again");
    let p = h.server.players.values().next().expect("the player");
    let max = yalain_world::quals::int(&p.qualities, 263).unwrap_or(100);
    let health = yalain_world::quals::int(&p.qualities, 258).unwrap_or(0);
    assert_eq!(health, max * 3 / 4, "three quarters of its health");
    assert!(p.dying.is_none());
    assert!(
        h.world_messages(b, from)
            .iter()
            .any(|m| matches!(m, ServerMessage::UpdateBoolVisual(q)
            if q.object.id == me && q.quality == DEAD && q.value == 0)),
        "seen alive"
    );
}

/// As it goes, a resurrected player is set standing at once, never played into it: the mode set
/// to peace, the resurrection's glow, then the arrival's effect as it goes into portal space.
/// Behaviour: server.death.resurrection-look
#[test]
fn a_resurrected_player_is_set_standing_then_glows_as_it_goes() {
    let mut h = Harness::new();
    let (b, me) = about_to_die(&mut h);
    let from = h.bots[b].inbox.len();
    let _ = run_to_death(&mut h, b, from);
    wait(&mut h, 11.0);
    let from = h.bots[b].inbox.len();
    h.bots[b].send(true, queue::WEENIE, &server_call(RESURRECT, &[]));
    h.run(3);
    let look: Vec<(&str, u32)> = h
        .world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::SetMode(e) if e.sender.id == me => Some(("set", e.action.0)),
            ServerMessage::DoMode(e) if e.sender.id == me => Some(("played", e.action.0)),
            ServerMessage::DoFx(e) if e.sender.id == me => Some(("effect", e.action.fx)),
            _ => None,
        })
        .collect();
    assert_eq!(
        look,
        vec![
            ("set", 0x4000_0001),
            ("effect", 0x4100_000F),
            ("effect", 0x4100_0010)
        ]
    );
}

/// Back from a resurrection, a player with more than ten focus has ten: sent after its health
/// and vigor, before it is told it is alive; one with ten or less keeps it, and nothing is sent.
/// Behaviour: server.focus.resurrection
#[test]
fn a_resurrected_player_with_more_than_ten_focus_comes_back_with_ten() {
    for (had, want) in [(60, Some(10)), (8, None)] {
        let mut h = Harness::new();
        let (b, _) = about_to_die(&mut h);
        for p in h.server.players.values_mut() {
            yalain_world::quals::set_int(&mut p.qualities, FOCUS, had);
        }
        let from = h.bots[b].inbox.len();
        let _ = run_to_death(&mut h, b, from);
        wait(&mut h, 11.0);
        let from = h.bots[b].inbox.len();
        h.bots[b].send(true, queue::WEENIE, &server_call(RESURRECT, &[]));
        wait(&mut h, 2.0);
        let seq: Vec<(u32, i32)> = h
            .world_messages(b, from)
            .iter()
            .filter_map(|m| match m {
                ServerMessage::UpdateInt(x) if [258, 259, FOCUS].contains(&x.quality) => {
                    Some((x.quality, x.value))
                }
                ServerMessage::PlayerCall(c) if c.function.0 == DEATH_STATE => Some((0, 0)),
                _ => None,
            })
            .collect();
        let order: Vec<u32> = seq.iter().map(|(k, _)| *k).take(4).collect();
        let focus: Vec<i32> = seq
            .iter()
            .filter(|(k, _)| *k == FOCUS)
            .map(|(_, v)| *v)
            .collect();
        assert_eq!(focus, want.into_iter().collect::<Vec<_>>(), "had {had}");
        let mut expected = vec![258, 259];
        if want.is_some() {
            expected.push(FOCUS);
        }
        expected.push(0);
        assert_eq!(order, expected, "health, vigor, focus, alive (had {had})");
    }
}

/// Vitae: none lost below level 6; above, at least 100 less 3 a level, between 60 and 99; what is
/// still to earn to raise it by one is the level's cost times the vitae's share, rounded, at
/// least 1, none at full vitae.
/// Behaviour: server.death.vitae
#[test]
fn vitae_has_its_floor_by_level_and_its_cost_to_raise() {
    assert!((min_vitae(5) - 100.0).abs() < f32::EPSILON);
    assert!((min_vitae(6) - 85.0).abs() < f32::EPSILON);
    assert!((min_vitae(14) - 61.0).abs() < f32::EPSILON);
    assert!((min_vitae(40) - 60.0).abs() < f32::EPSILON);
    let costs = [1, 1, 1, 1, 1, 2, 3, 4, 5, 6, 7, 8, 11, 15];
    assert_eq!(xp_to_raise(&costs, 14, 95.0), Some(14));
    assert_eq!(xp_to_raise(&costs, 6, 95.0), Some(2));
    assert_eq!(xp_to_raise(&costs, 1, 95.0), Some(1));
    assert_eq!(xp_to_raise(&costs, 14, 100.0), None);
}

const ADD_EFFECT: u32 = 0x8208_0017;
const REMOVE_EFFECT: u32 = 0x8208_0016;
const SHOW_TEXT: u32 = yalain_world::session::function::SHOW_TEXT;
const PROTECTION: u32 = 0x7000_0973;
const NO_LONGER_PROTECTED: u32 = 0x847E_5E83;
const GLOW: u32 = 0x4100_000F;

/// Resurrected: the protection added (its id, and a record of the protection's saved state for
/// three minutes); returns the effect's id.
fn resurrected(h: &mut Harness) -> (usize, u64, u32, usize) {
    let (b, me) = about_to_die(h);
    let from = h.bots[b].inbox.len();
    let _ = run_to_death(h, b, from);
    wait(h, 11.0);
    let from = h.bots[b].inbox.len();
    h.bots[b].send(true, queue::WEENIE, &server_call(RESURRECT, &[]));
    wait(h, 2.0);
    // Back in the world, its scene drawn: in sight of the creature that killed it.
    scene_drawn(h, b);
    let added: Vec<Vec<Value>> = calls_of(h, b, from)
        .into_iter()
        .filter(|(f, _)| *f == ADD_EFFECT)
        .map(|(_, v)| v)
        .collect();
    assert_eq!(added.len(), 1, "one effect added");
    let v = &added[0];
    let Some(Value::Int(eid)) = v.get(1) else {
        panic!("the effect's id: {v:?}")
    };
    let Some(yalain_world::calls::Node::Script { package, slots }) = v[0].root() else {
        panic!("a record")
    };
    assert_eq!(*package, 0x14E, "an effect record");
    assert_eq!(slots.len(), 26);
    assert_eq!(
        slots[6],
        (0x03, 180.0f32.to_bits()),
        "lasting three minutes"
    );
    assert_eq!(slots[20], (0x02, 0x44), "the protection's flags");
    assert_eq!(slots[23], (0x02, *eid), "its id in the record");
    let state = v[0]
        .entry(slots[14].1)
        .map(|n| matches!(n, yalain_world::calls::Node::Singleton(_)));
    assert_eq!(state, Some(true), "the effect a saved state's singleton");
    let raw = h.bots[b].inbox[from..]
        .iter()
        .filter_map(|r| match r.message() {
            Some(ServerMessage::PlayerCall(c)) if c.function.0 == ADD_EFFECT => {
                c.args.as_ref().map(|a| a.0.clone())
            }
            _ => None,
        })
        .next()
        .expect("the call's bytes");
    assert!(
        raw.windows(4).any(|w| w == PROTECTION.to_le_bytes()),
        "the lifestone protection's saved state"
    );
    (b, me, *eid, h.bots[b].inbox.len())
}

fn ended(h: &Harness, b: usize, from: usize, eid: u32, me: u64) -> bool {
    let calls = calls_of(h, b, from);
    let text = calls.iter().any(|(f, v)| {
        *f == SHOW_TEXT
            && v.iter().any(|x| match x {
                Value::Graph(g) => g.iter().any(|(_, n)| {
                    matches!(n, yalain_world::calls::Node::TextInfo(t) if t.string_id == NO_LONGER_PROTECTED)
                }),
                _ => false,
            })
    });
    let removed = calls
        .iter()
        .any(|(f, v)| *f == REMOVE_EFFECT && v.first() == Some(&Value::Int(eid)));
    let glow_stopped = h
        .world_messages(b, from)
        .iter()
        .any(|m| matches!(m, ServerMessage::StopFx(e) if e.sender.id == me && e.action.0 == GLOW));
    text && removed && glow_stopped
}

/// A resurrected player is under lifestone protection for three minutes: the protection added
/// with its record as it goes; creatures leave it be; when the three minutes are up, "You are no
/// longer protected by the Lifestone's Magic.", the protection taken away, its glow stopped.
/// Behaviour: server.death.lifestone-protection
#[test]
fn a_resurrected_player_is_protected_for_three_minutes_and_creatures_leave_it_be() {
    let mut h = Harness::new();
    let (b, me, eid, from) = resurrected(&mut h);
    assert!(h.server.players.values().all(|p| p.protected.is_some()));
    wait(&mut h, 170.0);
    assert!(
        !ended(&h, b, from, eid, me),
        "still protected before the time"
    );
    let attacked = h
        .world_messages(b, from)
        .iter()
        .any(|m| matches!(m, ServerMessage::DoStory(e) if e.sender.id != me));
    assert!(!attacked, "no creature takes on a protected player");
    wait(&mut h, 15.0);
    assert!(ended(&h, b, from, eid, me), "ended after three minutes");
    assert!(h.server.players.values().all(|p| p.protected.is_none()));
}

/// Attacking ends a lifestone protection at once: the text, the protection taken away, the glow
/// stopped.
/// Behaviour: server.death.lifestone-protection
#[test]
fn attacking_ends_a_lifestone_protection() {
    let mut h = Harness::new();
    let (b, me, eid, from) = resurrected(&mut h);
    let from = from.max(h.bots[b].inbox.len());
    h.arm(b, me, 0x4700_051C);
    let creature = h
        .server
        .things
        .placed()
        .find(|t| t.alive())
        .map(|t| t.id)
        .expect("a creature");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(
            yalain_world::combat::function::START_ATTACK,
            &[auberean_script::args::Arg::LongInt(creature)],
        ),
    );
    wait(&mut h, 1.0);
    assert!(ended(&h, b, from, eid, me), "ended by the attack");
}

const HEALTH: u32 = 258;
const VITAE: u32 = 286;
const LEVEL: u32 = 302;
/// More steps than a leaving takes.
const STEPS_TO_LEAVE: usize = 2000;

/// The bound lifestone of the tests that return to one.
fn a_lifestone() -> yalain_store::StoredPosition {
    yalain_store::StoredPosition {
        cell: 0x0102_0003,
        origin: [12.5, 34.5, 2.0],
        rotation: [1.0, 0.0, 0.0, 0.0],
    }
}

/// A player of level 30 (so a resurrection costs it vitae), bound to [`a_lifestone`], dead with its
/// corpse made and its box up: where a client may ask to be resurrected and has not.
fn dead_with_its_box_up(h: &mut Harness) -> (usize, u64) {
    let (b, me) = about_to_die(h);
    for p in h.server.players.values_mut() {
        yalain_world::quals::set_int(&mut p.qualities, LEVEL, 30);
        p.places
            .push((yalain_world::places::kind::LIFESTONE, a_lifestone()));
    }
    let from = h.bots[b].inbox.len();
    let _ = run_to_death(h, b, from);
    wait(h, 11.0);
    assert!(
        calls_of(h, b, from).contains(&(DEATH_STATE, vec![Value::Int(1)])),
        "its box is up"
    );
    (b, me)
}

fn log_off(h: &mut Harness, b: usize, id: u64) {
    let before = h.bots[b].inbox.len();
    h.bots[b].send(
        false,
        queue::LOGON,
        &auberean_protocol::ClientMessage::LogOff(auberean_protocol::login::CharacterRef {
            character: id,
        }),
    );
    assert!(h.run_until(STEPS_TO_LEAVE, |h| h.bots[b].inbox[before..]
        .iter()
        .any(|r| matches!(r.message(), Some(ServerMessage::CharacterList(_))))));
}

/// What an entry since `from` has to show of a character that left the world dead, once it has
/// come back: told it enters at no health, never told it is dead, nor of a death, nor to ask; then
/// its health and vigor, alive, standing and glowing where it entered, the place it returns to,
/// its vitae five less and three quarters of its health, protected, its actions no longer refused.
fn came_back_alive(h: &mut Harness, b: usize, me: u64, from: usize) {
    let messages = h.world_messages(b, from);
    let described = messages.iter().find_map(|m| match m {
        ServerMessage::PlayerQualities(q) => q.0.ints.get(HEALTH).copied(),
        _ => None,
    });
    assert_eq!(described, Some(0), "it enters at no health");
    wait(h, 2.0);
    let calls = calls_of(h, b, from);
    assert!(
        !calls.iter().any(|(f, _)| *f == DEATH_MESSAGE),
        "no death told"
    );
    let states: Vec<&Vec<Value>> = calls
        .iter()
        .filter(|(f, _)| *f == DEATH_STATE)
        .map(|(_, v)| v)
        .collect();
    assert_eq!(states, vec![&vec![Value::Int(0)]], "alive, never dead");
    assert_eq!(
        calls.iter().filter(|(f, _)| *f == ADD_EFFECT).count(),
        1,
        "protected"
    );
    let messages = h.world_messages(b, from);
    let look: Vec<(&str, u32)> = messages
        .iter()
        .filter_map(|m| match m {
            ServerMessage::SetMode(e) if e.sender.id == me => Some(("set", e.action.0)),
            ServerMessage::DoFx(e) if e.sender.id == me && e.action.fx == GLOW => {
                Some(("effect", e.action.fx))
            }
            ServerMessage::UpdateBoolVisual(q) if q.object.id == me && q.quality == DEAD => {
                Some(("dead", u32::from(q.value != 0)))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        look,
        vec![("set", 0x4000_0001), ("effect", GLOW), ("dead", 0)],
        "set standing, glowing, alive"
    );
    let p = h
        .server
        .players
        .values()
        .find(|p| p.character == me)
        .expect("in the world");
    assert!(p.trip.is_none(), "not through portal space");
    assert!(p.dying.is_none(), "no longer dying");
    assert_eq!(p.position.cell, a_lifestone().cell, "at its lifestone");
    assert_eq!(p.position.origin, a_lifestone().origin);
    let max = yalain_world::quals::int(&p.qualities, 263).unwrap_or(100);
    assert_eq!(
        yalain_world::quals::float(&p.qualities, VITAE),
        Some(95.0),
        "five vitae less"
    );
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    let back = ((max as f32 * 0.95).floor() * 0.75) as i32;
    assert_eq!(
        yalain_world::quals::int(&p.qualities, HEALTH),
        Some(back),
        "three quarters of its health at that vitae"
    );
    let kept = h
        .server
        .store
        .world_state(me)
        .expect("reads")
        .expect("kept");
    assert_eq!(
        yalain_world::quals::int(&kept.qualities, HEALTH),
        Some(back),
        "kept as it comes back, so a stop now does not take its vitae again"
    );
    let world = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .expect("in the world")
        .0;
    assert!(
        !yalain_world::places::actions_locked(&h.server, world),
        "its actions are no longer refused"
    );
}

/// A character that logs off dead, its box up, comes back alive by itself: at its lifestone, the
/// entry at no health, a moment later its vitae five less, three quarters of its health and vigor,
/// protected and playing its return, and no box and nothing to ask for.
/// Behaviour: server.death.back-at-entry
#[test]
fn a_character_that_logged_off_dead_comes_back_alive_at_its_lifestone() {
    let mut h = Harness::new();
    let (b, me) = dead_with_its_box_up(&mut h);
    log_off(&mut h, b, me);
    let from = h.bots[b].inbox.len();
    h.enter_world(b, "one", me);
    came_back_alive(&mut h, b, me, from);
}

/// A character whose client went away while it was dead comes back as one that logged off dead.
/// Behaviour: server.death.back-at-entry
#[test]
fn a_character_whose_client_went_away_dead_comes_back_alive_at_its_lifestone() {
    let mut h = Harness::new();
    let (b, me) = dead_with_its_box_up(&mut h);
    h.bots[b].leave();
    assert!(
        h.run_until(STEPS_TO_LEAVE, |h| h.server.players.is_empty()),
        "it lingers and leaves"
    );
    let again = h.at_character_list("one");
    let from = h.bots[again].inbox.len();
    h.enter_world(again, "one", me);
    came_back_alive(&mut h, again, me, from);
}

/// A character in the world dead as the server stops comes back as one that logged off dead when
/// the server is up again.
/// Behaviour: server.death.back-at-entry
#[test]
fn a_character_dead_at_a_server_restart_comes_back_alive_at_its_lifestone() {
    let mut h = Harness::new();
    let (_, me) = dead_with_its_box_up(&mut h);
    assert_eq!(
        yalain_world::shutdown::save_everyone(&mut h.server, &mut h.net),
        1
    );
    let store = std::mem::take(&mut h.server.store);
    let mut h = Harness::with(
        yalain_world::ServerConfig::default(),
        store,
        yalain_testkit::test_rules(),
    );
    let b = h.at_character_list("one");
    let from = h.bots[b].inbox.len();
    h.enter_world(b, "one", me);
    came_back_alive(&mut h, b, me, from);
}

/// A character kept dead by an older server, its place and everything else as it left them, needs
/// nothing done to what is kept: its next entry brings it back.
/// Behaviour: server.death.back-at-entry
#[test]
fn a_character_already_kept_dead_comes_out_of_it_at_its_next_entry() {
    let mut h = Harness::new();
    let (b, me) = h.player_in_world("one", "Elara");
    scene_drawn(&mut h, b);
    for p in h.server.players.values_mut() {
        yalain_world::quals::set_int(&mut p.qualities, LEVEL, 30);
        p.places
            .push((yalain_world::places::kind::LIFESTONE, a_lifestone()));
    }
    log_off(&mut h, b, me);
    let mut kept = h
        .server
        .store
        .world_state(me)
        .expect("reads")
        .expect("kept");
    yalain_world::quals::set_int(&mut kept.qualities, HEALTH, 0);
    h.server.store.set_world_state(me, &kept).expect("kept");
    let from = h.bots[b].inbox.len();
    h.enter_world(b, "one", me);
    came_back_alive(&mut h, b, me, from);
}

/// A character that was never bound comes back beside the nearest lifestone, as a resurrection
/// would take it: where it fell when the world has none near.
/// Behaviour: server.death.back-at-entry
#[test]
fn a_character_never_bound_comes_back_where_it_fell_with_no_lifestone_near() {
    let mut h = Harness::new();
    let (b, me) = about_to_die(&mut h);
    let from = h.bots[b].inbox.len();
    let _ = run_to_death(&mut h, b, from);
    let fell = h
        .server
        .players
        .values()
        .next()
        .expect("the player")
        .position;
    wait(&mut h, 11.0);
    log_off(&mut h, b, me);
    h.enter_world(b, "one", me);
    wait(&mut h, 2.0);
    let p = h.server.players.values().next().expect("the player");
    assert!(p.dying.is_none(), "alive");
    assert_eq!(p.position.cell, fell.cell);
    assert_eq!(p.position.origin, fell.origin);
    assert!(yalain_world::quals::int(&p.qualities, HEALTH).unwrap_or(0) > 0);
}

/// A character alive at its entry is not brought back: nothing of a return is sent.
/// Behaviour: server.death.back-at-entry
#[test]
fn a_character_alive_at_its_entry_is_not_brought_back() {
    let mut h = Harness::new();
    let (b, me) = h.player_in_world("one", "Elara");
    scene_drawn(&mut h, b);
    log_off(&mut h, b, me);
    let from = h.bots[b].inbox.len();
    h.enter_world(b, "one", me);
    wait(&mut h, 2.0);
    assert!(
        !calls_of(&h, b, from).iter().any(|(f, _)| *f == DEATH_STATE),
        "no death state"
    );
    assert!(
        !calls_of(&h, b, from).iter().any(|(f, _)| *f == ADD_EFFECT),
        "not protected"
    );
}

/// A creature's blow: its story first, and only as the blow reaches the player, most of a second
/// later, its health at none and the kill text, in that order, so the client has written the hit
/// before it writes the kill.
/// Behaviour: server.death.blow-before-kill-text
#[test]
fn a_creatures_killing_blow_is_told_before_the_health_and_the_kill_text() {
    let mut h = Harness::new();
    let (b, me) = about_to_die(&mut h);
    let from = h.bots[b].inbox.len();
    let told = |m: &ServerMessage| {
        matches!(m, ServerMessage::DoStory(e) if e.action.hooks.iter().any(|k| k.target == me
            && k.bits == yalain_world::combat::hook::HIT))
    };
    assert!(
        h.run_until(4000, |h| h.world_messages(b, from).iter().any(told)),
        "the creature lands a blow"
    );
    let struck = h.now.0;
    let at_none = |m: &ServerMessage| matches!(m, ServerMessage::UpdateInt(q) if q.quality == HEALTH && q.value == 0);
    assert!(
        !h.world_messages(b, from).iter().any(at_none),
        "not hurt as it is told"
    );
    assert!(
        h.run_until(4000, |h| h.world_messages(b, from).iter().any(at_none)),
        "the blow reaches it"
    );
    let gap = h.now.0 - struck;
    assert!(
        (0.4..1.1).contains(&gap),
        "the blow reached it {gap:.2} s after its story"
    );
    let _ = run_to_death(&mut h, b, from);
    let messages = h.world_messages(b, from);
    let at = |pick: &dyn Fn(&ServerMessage) -> bool| messages.iter().position(pick);
    let kill_text =
        at(&|m| matches!(m, ServerMessage::PlayerCall(c) if c.function.0 == DEATH_MESSAGE));
    assert!(at(&told) < at(&at_none), "the story before the health");
    assert!(at(&at_none) < kill_text, "the health before the kill text");
}

/// A blow told at a player who has since gone into portal space does not reach it.
/// Behaviour: server.death.blow-before-kill-text
#[test]
fn a_blow_at_a_player_gone_into_portal_space_does_not_hurt_it() {
    let mut h = Harness::new();
    let (b, me) = h.player_in_world("one", "Elara");
    scene_drawn(&mut h, b);
    let world = *h.server.players.keys().next().expect("the player");
    let clock = h.clock();
    let to = a_lifestone();
    yalain_world::places::resurrection_trip(&mut h.server, &mut h.net, world, to, clock);
    let before = yalain_world::quals::int(&h.server.players[&world].qualities, HEALTH);
    yalain_world::combat::creature_strike_lands(
        &mut h.server,
        &mut h.net,
        me,
        world,
        yalain_world::effects::Harm::plain(5),
        clock,
    );
    assert_eq!(
        yalain_world::quals::int(&h.server.players[&world].qualities, HEALTH),
        before
    );
}

/// A second blow told before the first killed the player does nothing when it reaches it: the
/// player does not die again.
/// Behaviour: server.death.blow-before-kill-text
#[test]
fn a_blow_at_a_player_already_dead_does_nothing() {
    let mut h = Harness::new();
    let (b, me) = h.player_in_world("one", "Elara");
    scene_drawn(&mut h, b);
    let world = *h.server.players.keys().next().expect("the player");
    for p in h.server.players.values_mut() {
        yalain_world::quals::set_int(&mut p.qualities, HEALTH, 0);
    }
    let clock = h.clock();
    yalain_world::combat::creature_strike_lands(
        &mut h.server,
        &mut h.net,
        me,
        world,
        yalain_world::effects::Harm::plain(5),
        clock,
    );
    assert!(h.server.players[&world].dying.is_none(), "no second death");
}

/// A player alive and in the world is hurt by a blow that reaches it: the same call's other side.
/// Behaviour: server.death.blow-before-kill-text
#[test]
fn a_blow_that_reaches_a_player_hurts_it_by_its_damage() {
    let mut h = Harness::new();
    let (b, me) = h.player_in_world("one", "Elara");
    scene_drawn(&mut h, b);
    let world = *h.server.players.keys().next().expect("the player");
    let clock = h.clock();
    let before = yalain_world::quals::int(&h.server.players[&world].qualities, HEALTH).unwrap_or(0);
    yalain_world::combat::creature_strike_lands(
        &mut h.server,
        &mut h.net,
        me,
        world,
        yalain_world::effects::Harm::plain(5),
        clock,
    );
    assert_eq!(
        yalain_world::quals::int(&h.server.players[&world].qualities, HEALTH),
        Some(before - 5)
    );
}

/// When a blow reaches its target by what it is: a creature's basic attacks, the specials measured
/// by their own, another special at the share of a player's strike.
/// Behaviour: server.death.blow-before-kill-text
#[test]
fn a_creatures_strike_reaches_its_target_by_its_skill() {
    use yalain_world::combat::creature_hit_seconds as at;
    assert!((at(1845, 0, false) - 0.778).abs() < 1e-9);
    assert!((at(1035, 0, true) - 0.524).abs() < 1e-9);
    assert!((at(462, 0, true) - 1.198).abs() < 1e-9);
    assert!((at(981, 0, true) - 1.207).abs() < 1e-9);
    assert!((at(1801, 0, true) - 2.066).abs() < 1e-9);
    assert!((at(7, 0, true) - 0.71 * 1.6).abs() < 1e-9);
    assert!((at(7, 0x4100_0077, true) - 0.71 * 1.444).abs() < 1e-9);
}

/// A record of an effect on a player, by hand.
fn record_on(h: &mut Harness, state: u32, flags: u32, timeout: f32) -> u32 {
    let cast = h.net.game_time(h.now);
    let p = h.server.players.values_mut().next().expect("the player");
    let eid = yalain_world::effects::next_eid(p);
    p.play.effects.push(yalain_world::state::EffectState {
        eid,
        state,
        cast,
        timeout,
        caster: 0,
        spellcraft: 10.0,
        pk: false,
        acting: 0,
        skill: 0,
        item: 0,
        flags,
        categories: 1,
    });
    eid
}

fn records_held(h: &Harness) -> Vec<u32> {
    let p = h.server.players.values().next().expect("the player");
    p.play.effects.iter().map(|e| e.eid).collect()
}

fn word(v: &Value) -> u32 {
    let Value::Int(i) = v else {
        panic!("a word: {v:?}")
    };
    *i
}

/// As a resurrection is asked for, the records whose effects a death ends are taken off, and only
/// then is the protection added; a worn item's record and one a death does not end stay. For all
/// who see it, it is set standing, what its death ended stops showing, and it glows.
/// Behaviour: server.death.resurrection-ends-effects
#[test]
fn a_resurrection_takes_off_what_a_death_ends_and_keeps_the_rest() {
    use yalain_testkit::{COURAGE_EFFECT, DASH_EFFECT, EASE_SUFFERING_EFFECT};
    use yalain_world::effects::{function as effect_fn, TIMED, WORN};
    let mut h = Harness::new();
    let (b, me) = about_to_die(&mut h);
    let dash = record_on(&mut h, DASH_EFFECT, TIMED, 3000.0);
    let worn = record_on(&mut h, EASE_SUFFERING_EFFECT, WORN, f32::MAX);
    let courage = record_on(&mut h, COURAGE_EFFECT, TIMED, 3000.0);
    let from = h.bots[b].inbox.len();
    let _ = run_to_death(&mut h, b, from);
    wait(&mut h, 11.0);
    assert_eq!(
        records_held(&h),
        vec![dash, worn, courage],
        "a death itself takes nothing off"
    );
    let from = h.bots[b].inbox.len();
    h.bots[b].send(true, queue::WEENIE, &server_call(RESURRECT, &[]));
    h.run(3);
    let told: Vec<(u32, Vec<Value>)> = calls_of(&h, b, from)
        .into_iter()
        .filter(|(f, _)| *f == effect_fn::REMOVE || *f == effect_fn::ADD)
        .collect();
    assert_eq!(told.len(), 2, "one record off, the protection on: {told:?}");
    assert_eq!(told[0].0, effect_fn::REMOVE);
    assert_eq!(word(&told[0].1[0]), dash);
    assert_eq!(told[1].0, effect_fn::ADD);
    assert_eq!(records_held(&h), vec![worn, courage]);
    let look: Vec<(&str, u32)> = h
        .world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::SetMode(e) if e.sender.id == me => Some(("set", e.action.0)),
            ServerMessage::StopFx(e) if e.sender.id == me => Some(("stop", e.action.0)),
            ServerMessage::DoFx(e) if e.sender.id == me => Some(("effect", e.action.fx)),
            _ => None,
        })
        .collect();
    assert_eq!(
        look,
        vec![
            ("set", 0x4000_0001),
            ("stop", 0x4100_04CB),
            ("effect", 0x4100_000F),
            ("effect", 0x4100_0010)
        ]
    );
}

/// A character that left the world dead loses what its death ends as it comes back, before its
/// protection, as one resurrected by asking does.
/// Behaviour: server.death.resurrection-ends-effects
#[test]
fn a_character_coming_back_at_its_entry_loses_what_a_death_ends() {
    use yalain_testkit::{COURAGE_EFFECT, DASH_EFFECT};
    use yalain_world::effects::{function as effect_fn, TIMED};
    let mut h = Harness::new();
    let (b, me) = dead_with_its_box_up(&mut h);
    let dash = record_on(&mut h, DASH_EFFECT, TIMED, 3000.0);
    let courage = record_on(&mut h, COURAGE_EFFECT, TIMED, 3000.0);
    log_off(&mut h, b, me);
    let from = h.bots[b].inbox.len();
    h.enter_world(b, "one", me);
    wait(&mut h, 2.0);
    let told: Vec<u32> = calls_of(&h, b, from)
        .into_iter()
        .filter(|(f, _)| *f == effect_fn::REMOVE || *f == effect_fn::ADD)
        .map(|(f, _)| f)
        .collect();
    assert_eq!(told, vec![effect_fn::REMOVE, effect_fn::ADD]);
    assert_eq!(records_held(&h), vec![courage], "not {dash}");
}

/// A player that sits dead with its box up and asks for nothing is sent back ten minutes after it
/// was told it was dead, as if it had asked: not before, then alive at its lifestone, protected.
/// Behaviour: server.death.sent-back-after-ten-minutes
#[test]
fn a_player_left_dead_ten_minutes_is_sent_back() {
    let mut h = Harness::new();
    let (b, _) = dead_with_its_box_up(&mut h);
    let from = h.bots[b].inbox.len();
    wait(&mut h, 595.0);
    assert!(
        h.server.players.values().all(|p| p.dying.is_some()),
        "still dead"
    );
    assert!(calls_of(&h, b, from).iter().all(|(f, _)| *f != DEATH_STATE));
    wait(&mut h, 8.0);
    let calls = calls_of(&h, b, from);
    assert_eq!(
        calls
            .iter()
            .filter(|(f, _)| *f == DEATH_STATE)
            .map(|(_, v)| v.clone())
            .collect::<Vec<_>>(),
        vec![vec![Value::Int(0)]],
        "alive again"
    );
    assert_eq!(
        calls.iter().filter(|(f, _)| *f == ADD_EFFECT).count(),
        1,
        "protected"
    );
    let p = h.server.players.values().next().expect("the player");
    assert!(p.dying.is_none());
    assert_eq!(p.position.cell, a_lifestone().cell, "at its lifestone");
}

/// A player that asked and died again is not sent back by its first death's ten minutes: the second
/// death has its own.
/// Behaviour: server.death.sent-back-after-ten-minutes
#[test]
fn a_second_death_is_not_cut_short_by_the_firsts_ten_minutes() {
    let mut h = Harness::new();
    let (b, _) = dead_with_its_box_up(&mut h);
    let world = *h.server.players.keys().next().expect("the player");
    let first = h.server.players[&world]
        .dying
        .and_then(|d| d.corpse)
        .expect("its corpse");
    h.bots[b].send(true, queue::WEENIE, &server_call(RESURRECT, &[]));
    wait(&mut h, 2.0);
    assert!(h.server.players[&world].dying.is_none());
    let clock = h.clock();
    yalain_world::death::player_dies(&mut h.server, &mut h.net, world, None, clock);
    wait(&mut h, 11.0);
    let second = h.server.players[&world]
        .dying
        .and_then(|d| d.corpse)
        .expect("a new corpse");
    assert_ne!(first, second);
    let clock = h.clock();
    yalain_world::death::sent_back(&mut h.server, &mut h.net, world, first, clock);
    assert!(h.server.players[&world].trip.is_none(), "the first's: no");
    yalain_world::death::sent_back(&mut h.server, &mut h.net, world, second, clock);
    assert!(h.server.players[&world].trip.is_some(), "its own: yes");
}
