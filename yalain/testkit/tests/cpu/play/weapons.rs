//! The basic attacks of each weapon: a bow shoots from afar and strikes within a meter, a stave
//! strikes near and throws far; a run that stands is turned to its target before each swing, is
//! told when it is too far or moving, and takes its combat mode with its first swing; a sword's run
//! walks to its target with one order and is told nothing on the way.

use auberean_primitives::Vec3;
use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_testkit::play::{report, server_call};
use yalain_testkit::{Harness, TEST_CREATURE};
use yalain_world::calls::{self, Value};
use yalain_world::combat::{function, next_swing_after, swing_seconds};
use yalain_world::skill_use::implement as im;

const BOW: u32 = 0x4700_051B;
const STAVE: u32 = 0x4700_051D;
const SWORD: u32 = 0x4700_051C;

/// A player beside the test camp's beetle (made one that stands where it is put), the beetle put
/// `d` meters east of the player; returns the client, the player and the beetle.
fn beetle_at(d: f32) -> (Harness, usize, u64, u64) {
    let mut h = Harness::new();
    h.add_creatures(1);
    let (b, me) = h.player_in_world("one", "Elara");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
    let beetle = h
        .world_messages(b, 0)
        .iter()
        .find_map(|m| match m {
            ServerMessage::CreateObject(o) if o.weenie.entity_did == TEST_CREATURE => Some(o.id),
            _ => None,
        })
        .expect("the camp's creature");
    let at = h
        .server
        .players
        .values()
        .find(|p| p.character == me)
        .map(|p| p.position)
        .expect("in the world");
    if let Some(t) = h.server.things.get_mut(beetle) {
        // It stands where it is put: it notices no one, and goes after the one who hits it at no
        // speed.
        if let Some(c) = t.creature.as_mut() {
            c.stats.movement.perception = 0.0;
            c.stats.scale = 0.0;
        }
        let mut place = at;
        place.origin[0] += d;
        t.at = Some(place);
    }
    (h, b, me, beetle)
}

fn attack(h: &mut Harness, b: usize, target: u64) -> usize {
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(function::START_ATTACK, &[Arg::LongInt(target)]),
    );
    from
}

fn stories(
    h: &Harness,
    b: usize,
    from: usize,
    of: u64,
) -> Vec<auberean_protocol::types::PhysicsStory> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::DoStory(e) if e.sender.id == of => Some(e.action.clone()),
            _ => None,
        })
        .collect()
}

fn failures(h: &Harness, b: usize, from: usize) -> Vec<u32> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c) if c.function.0 == function::BASIC_ATTACKS_FAILED => c
                .args
                .as_ref()
                .and_then(|a| calls::values(&a.0).ok())
                .and_then(|v| match v.first() {
                    Some(Value::Int(e)) => Some(*e),
                    _ => None,
                }),
            _ => None,
        })
        .collect()
}

/// With a bow and a target beyond a meter between the bodies, a run stands and shoots: the attack
/// state, the bow's mode and a turn to the target; before each swing the player stopped and turned
/// again; each swing's story one of the bow's thrown attacks (skills 199-203, their behaviors)
/// naming the bow, the bow playing the behavior; the target tells each arrival (a basic attack's
/// story), and no swing walks the player.
/// Behaviour: server.weapons.bow-shoots
#[test]
fn a_bow_shoots_from_afar_and_its_target_tells_each_arrival() {
    let (mut h, b, me, beetle) = beetle_at(6.0);
    let bow = h.arm(b, me, BOW);
    let from = attack(&mut h, b, beetle);
    h.run(120);
    let msgs = h.world_messages(b, from);
    assert!(msgs.iter().any(
        |m| matches!(m, ServerMessage::DoMode(e) if e.sender.id == me && e.action.0 == 0x4100_0010)
    ));
    assert!(
        !msgs
            .iter()
            .any(|m| matches!(m, ServerMessage::MoveTo(e) if e.action.params.flags == 0x46B)),
        "a bow run never walks"
    );
    let turns = msgs
        .iter()
        .filter(|m| matches!(m, ServerMessage::MoveTo(e) if e.action.params.flags == 0x46A))
        .count();
    let swings = stories(&h, b, from, me);
    assert!(swings.len() >= 2, "{} swings", swings.len());
    assert!(
        turns > swings.len(),
        "turned at the start and before each swing"
    );
    for s in &swings {
        assert!((199..=203).contains(&s.skill), "{}", s.skill);
        assert_eq!(s.attacker_behavior.behavior, 0x4100_00E2 + (s.skill - 199));
        assert_eq!(s.bits, 0x36);
        assert_eq!(
            (s.hooks[1].bits, s.hooks[1].target, s.hooks[1].weapon),
            (0x323, beetle, bow)
        );
    }
    assert!(msgs
        .iter()
        .any(|m| matches!(m, ServerMessage::PlayBehavior(e) if e.sender.id == bow)));
    let arrivals = stories(&h, b, from, beetle);
    assert!(!arrivals.is_empty());
    assert!(arrivals
        .iter()
        .all(|a| a.bits == 0x32 && a.attacker == me && [0xB81, 0x381].contains(&a.hooks[0].bits)));
}

/// A bow's shots go at its player's pace, and the next comes that much sooner.
/// Behaviour: server.weapons.attack-speed-effects
#[test]
fn a_bows_shots_go_at_its_players_pace() {
    let (mut h, b, me, beetle) = beetle_at(6.0);
    h.arm(b, me, BOW);
    super::attack_speed::speeded(&mut h, 27, 1.7, true);
    let from = attack(&mut h, b, beetle);
    let mut told = Vec::new();
    for _ in 0..400 {
        h.step();
        let shots = stories(&h, b, from, me);
        if shots.len() > told.len() {
            let s = shots.last().expect("told");
            told.push((h.now.0, s.attacker_behavior.time_scale));
        }
    }
    assert!(told.len() > 3, "{} shots", told.len());
    for (_, ts) in &told {
        assert!((1.72..=2.12).contains(ts), "{ts}");
    }
    for w in told.windows(2) {
        let gap = w[1].0 - w[0].0;
        let due = next_swing_after(swing_seconds(Some(im::BOW), true), w[0].1);
        assert!(
            (gap - due).abs() <= 0.052,
            "{gap:.3} where {due:.3} was due"
        );
    }
}

/// With a bow and a target within a meter between the bodies, the run strikes with the bow (skills
/// 194-198, the melee behaviors), no weapon behavior and no thrown hook.
/// Behaviour: server.weapons.bow-shoots
#[test]
fn a_bow_strikes_a_target_within_a_meter() {
    let (mut h, b, me, beetle) = beetle_at(2.2);
    h.arm(b, me, BOW);
    let from = attack(&mut h, b, beetle);
    h.run(80);
    let swings = stories(&h, b, from, me);
    assert!(!swings.is_empty());
    for s in &swings {
        assert!((194..=198).contains(&s.skill), "{}", s.skill);
        assert_eq!(s.attacker_behavior.behavior, 0x4100_0089 + (s.skill - 194));
        assert!([0xB81, 0x381].contains(&s.hooks[1].bits));
    }
}

/// A sword's run on a target out of reach has one order to walk and is told nothing while it walks,
/// however long it takes; arrived, it swings at once, with no second order for that swing.
/// Behaviour: server.weapons.walking-run-walks-silently
#[test]
fn a_sword_run_out_of_reach_is_ordered_once_and_told_nothing_until_it_arrives() {
    let (mut h, b, me, beetle) = beetle_at(25.0);
    h.arm(b, me, SWORD);
    let from = attack(&mut h, b, beetle);
    h.run(100);
    let orders = |h: &Harness, from: usize| {
        h.world_messages(b, from)
            .iter()
            .filter(|m| matches!(m, ServerMessage::MoveTo(e) if e.sender.id == me))
            .count()
    };
    assert_eq!(orders(&h, from), 1, "one order to walk");
    assert_eq!(
        failures(&h, b, from),
        Vec::<u32>::new(),
        "no line while it walks"
    );
    assert!(stories(&h, b, from, me).is_empty());
    assert!(
        h.server.players.values().any(|p| p.attacking.is_some()),
        "the run goes on"
    );
    // It arrives.
    if let Some(t) = h.server.things.get_mut(beetle) {
        if let Some(at) = t.at.as_mut() {
            at.origin[0] -= 23.0;
        }
    }
    h.run(3);
    assert_eq!(
        orders(&h, from),
        1,
        "no second order for the swing it arrives on"
    );
    assert_eq!(stories(&h, b, from, me).len(), 1);
    // The swing after has its own.
    h.run(60);
    assert!(orders(&h, from) >= 2);
}

/// A bow run beyond its greatest range (80 m between the bodies) is told it is too far, about a
/// second apart, without its combat mode; moving within range, it is told it cannot shoot while
/// moving.
/// Behaviour: server.weapons.standing-run-refusals
#[test]
fn a_bow_run_too_far_or_moving_is_told_so_each_second() {
    let (mut h, b, me, beetle) = beetle_at(85.0);
    h.arm(b, me, BOW);
    let from = attack(&mut h, b, beetle);
    h.run(70);
    let fails = failures(&h, b, from);
    assert!(
        fails.len() >= 2 && fails.iter().all(|e| *e == 30_019),
        "{fails:?}"
    );
    assert!(
        !h.world_messages(b, from)
            .iter()
            .any(|m| matches!(m, ServerMessage::DoMode(e) if e.sender.id == me)),
        "no combat mode out of range"
    );
    assert!(stories(&h, b, from, me).is_empty());
    // Within range, but moving.
    if let Some(t) = h.server.things.get_mut(beetle) {
        if let Some(at) = t.at.as_mut() {
            at.origin[0] -= 79.0;
        }
    }
    let at = h
        .server
        .players
        .values()
        .find(|p| p.character == me)
        .map(|p| p.position)
        .expect("in the world");
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &report(
            at.cell,
            Vec3::new(at.origin[0], at.origin[1], at.origin[2]),
            0x40,
            yalain_world::world::motion(127, 255, 127),
            0,
        ),
    );
    h.run(30);
    assert!(
        failures(&h, b, from).contains(&30_027),
        "{:?}",
        failures(&h, b, from)
    );
}

/// A stave strikes within ten meters between the bodies (skills 62-64: a hook that plays, then
/// the strike as the second) and throws beyond (65-67: the thrown hook second, naming the stave).
/// Behaviour: server.weapons.stave-near-and-far
#[test]
fn a_stave_strikes_near_and_throws_far() {
    for (d, near) in [(5.0, true), (16.0, false)] {
        let (mut h, b, me, beetle) = beetle_at(d);
        let stave = h.arm(b, me, STAVE);
        let from = attack(&mut h, b, beetle);
        h.run(80);
        let swings = stories(&h, b, from, me);
        assert!(!swings.is_empty(), "at {d} m");
        for s in &swings {
            assert_eq!(s.hooks[1].bits, 0x180);
            if near {
                assert!((62..=64).contains(&s.skill), "{}", s.skill);
                assert!([0xF81, 0x781].contains(&s.hooks[2].bits));
            } else {
                assert!((65..=67).contains(&s.skill), "{}", s.skill);
                assert_eq!((s.hooks[2].bits, s.hooks[2].weapon), (0x723, stave));
            }
        }
    }
}
