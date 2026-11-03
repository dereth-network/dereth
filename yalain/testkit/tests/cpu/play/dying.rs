//! A player's death: its tick, its corpse ten seconds later, the resurrection at its lifestone;
//! vitae.

use auberean_protocol::{queue, ServerMessage};
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
