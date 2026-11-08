//! Effects a player's skills put on creatures: a debuff's appearance from its add to its end and
//! the creature's numbers it changes, a hold's speed, a damage over time told by the creature, and
//! what a creature's death stops. Fixture: the test camp's beetles as practice targets (they never
//! fight or move), placed where each test needs them.

use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_store::StoredPosition;
use yalain_testkit::play::server_call;
use yalain_testkit::{
    Harness, CHARGED_AIR, CHARGED_AIR_EFFECT, PARALYZE, PARALYZE_EFFECT, TEST_CREATURE, VACUUM,
    VACUUM_EFFECT,
};
use yalain_world::effects::{self, On};
use yalain_world::player::quality;
use yalain_world::quals;
use yalain_world::skill_use::function;

const SWORD: u32 = 0x4700_051C;
const STAVE: u32 = 0x4700_051D;
const VACUUM_FX: u32 = 0x4100_0271;
const PARALYZE_FX: u32 = 0x4100_0290;
const OVER_TIME_FX: u32 = 0x4100_0031;

/// A new player with the scene drawn, vigor to spare, beside `n` of the camp's beetles made
/// practice targets; returns the client, the player and the beetles.
fn beside_beetles(n: u32) -> (Harness, usize, u64, Vec<u64>) {
    let mut h = Harness::new();
    h.add_creatures(n);
    let (b, me) = h.player_in_world("one", "Elara");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
    let beetles: Vec<u64> = h
        .world_messages(b, 0)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::CreateObject(o) if o.weenie.entity_did == TEST_CREATURE => Some(o.id),
            _ => None,
        })
        .collect();
    for id in &beetles {
        if let Some(c) = h
            .server
            .things
            .get_mut(*id)
            .and_then(|t| t.creature.as_mut())
        {
            c.stats.fights = false;
        }
    }
    let p = h
        .server
        .players
        .values_mut()
        .find(|p| p.character == me)
        .expect("in the world");
    quals::set_int(&mut p.qualities, quality::VIGOR_MAX, 5000);
    quals::set_int(&mut p.qualities, quality::VIGOR, 5000);
    (h, b, me, beetles)
}

/// Put a beetle so far (x, y) from the player.
fn place(h: &mut Harness, me: u64, beetle: u64, dx: f32, dy: f32) {
    let at = h
        .server
        .players
        .values()
        .find(|p| p.character == me)
        .map(|p| p.position)
        .expect("in the world");
    if let Some(t) = h.server.things.get_mut(beetle) {
        t.at = Some(StoredPosition {
            origin: [at.origin[0] + dx, at.origin[1] + dy, at.origin[2]],
            ..at
        });
    }
}

fn use_skill(h: &mut Harness, b: usize, id: u32, skill: u32, target: u64) -> usize {
    h.all_hold(skill);
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(
            function::USE,
            &[Arg::Int(id), Arg::Int(skill), Arg::LongInt(target)],
        ),
    );
    from
}

/// Step until a message `pick` accepts has come since `from`, at most `steps` steps; the clock
/// then.
fn step_until(
    h: &mut Harness,
    b: usize,
    from: usize,
    steps: usize,
    pick: impl Fn(&ServerMessage) -> bool,
) -> Option<f64> {
    for _ in 0..steps {
        if h.world_messages(b, from).iter().any(&pick) {
            return Some(h.now.0);
        }
        h.step();
    }
    None
}

fn shows(m: &ServerMessage, id: u64, fx: u32) -> bool {
    matches!(m, ServerMessage::DoFx(e) if e.sender.id == id && e.action.fx == fx)
}

fn stops(m: &ServerMessage, id: u64, fx: u32) -> bool {
    matches!(m, ServerMessage::StopFx(e) if e.sender.id == id && e.action.0 == fx)
}

/// Use a skill on a creature until one use lands its effect (its appearance shows); the clock
/// then and where the messages since that use begin.
fn until_it_lands(h: &mut Harness, b: usize, skill: u32, target: u64, fx: u32) -> (f64, usize) {
    for id in 1..30 {
        let from = use_skill(h, b, id, skill, target);
        if let Some(t) = step_until(h, b, from, 160, |m| shows(m, target, fx)) {
            return (t, from);
        }
    }
    panic!("no use landed");
}

/// A debuff a player's skill lands on a creature shows its appearance to the clients that see it
/// and stops it as its duration ends (Vacuum's thirty seconds), and while it runs it changes the
/// creature's numbers (Vacuum's -10 to its armor at a rating of 1).
/// Behaviour: server.effects.creature-debuff-shows-until-it-runs-out
#[test]
fn a_debuff_on_a_creature_shows_until_it_runs_out_and_changes_its_numbers() {
    let (mut h, b, me, beetles) = beside_beetles(1);
    let beetle = beetles[0];
    place(&mut h, me, beetle, 6.0, 0.0);
    h.arm(b, me, STAVE);
    let (on, from) = until_it_lands(&mut h, b, VACUUM, beetle, VACUUM_FX);
    assert!(effects::records_of(&h.server, On::Creature(beetle))
        .iter()
        .any(|e| e.state == VACUUM_EFFECT));
    assert_eq!(
        effects::creature_modifier(&h.server, beetle, 0x4_0125),
        (-10.0, 1.0)
    );
    let off = step_until(&mut h, b, from, 800, |m| stops(m, beetle, VACUUM_FX)).expect("it ends");
    assert!((off - on - 30.0).abs() < 0.2, "ran {}", off - on);
    assert_eq!(
        effects::creature_modifier(&h.server, beetle, 0x4_0125),
        (0.0, 1.0)
    );
}

/// A basic attack that names a target effect puts it on what it lands on, as a special's does; one
/// that names none puts nothing.
/// Behaviour: server.combat.a-basic-attacks-target-effects-land
#[test]
fn a_basic_attack_that_names_a_target_effect_lands_it() {
    for (named, expected) in [(vec![VACUUM_EFFECT], true), (Vec::new(), false)] {
        let (mut h, b, me, beetles) = beside_beetles(1);
        let beetle = beetles[0];
        place(&mut h, me, beetle, 1.0, 0.0);
        h.arm(b, me, SWORD);
        for d in h.server.skill_table.skills.values_mut() {
            if d.auto_attack {
                d.usage
                    .as_mut()
                    .expect("used")
                    .target_effects
                    .clone_from(&named);
            }
        }
        if let Some(t) = h.server.things.get_mut(beetle) {
            t.health = 1_000_000;
            t.creature.as_mut().expect("a creature").stats.fights = false;
        }
        h.bots[b].send(
            true,
            queue::WEENIE,
            &server_call(0x8204_0008, &[Arg::LongInt(beetle)]),
        );
        h.run(400);
        let on = effects::records_of(&h.server, On::Creature(beetle))
            .iter()
            .any(|e| e.state == VACUUM_EFFECT);
        assert_eq!(on, expected, "{named:x?}");
        let hit = h.world_messages(b, 0).iter().any(|m| {
            matches!(m, ServerMessage::DoStory(e) if e.action.attacker == me
                && e.action.hooks.iter().any(|k| k.target == beetle))
        });
        assert!(hit, "the run swung");
    }
}

/// A special's taunt counts for each use that lands, and for none that doesn't.
/// Behaviour: server.creatures.a-taunt-counts-at-the-draw
#[test]
fn a_specials_taunt_counts_only_for_the_uses_that_land() {
    const TAUNT: u32 = 0x6F00_7306;
    let (mut h, b, me, beetles) = beside_beetles(1);
    let beetle = beetles[0];
    place(&mut h, me, beetle, 6.0, 0.0);
    h.arm(b, me, STAVE);
    h.server.effect_table.effects.insert(
        TAUNT,
        yalain_world::effect_defs::EffectDef {
            state: TAUNT,
            kind: yalain_world::effect_defs::Kind::Taunt(0.05),
            ..h.server
                .effect_table
                .get(VACUUM_EFFECT)
                .cloned()
                .expect("read")
        },
    );
    h.server
        .skill_table
        .skills
        .get_mut(&VACUUM)
        .and_then(|d| d.usage.as_mut())
        .expect("used")
        .target_effects = vec![TAUNT];
    if let Some(t) = h.server.things.get_mut(beetle) {
        t.health = 1_000_000;
        t.max_health = 1_000;
    }
    let (mut landed, mut not) = (0, 0);
    for id in 1..40 {
        if let Some(c) = h
            .server
            .things
            .get_mut(beetle)
            .and_then(|t| t.creature.as_mut())
        {
            c.taunts.clear();
        }
        let from = use_skill(&mut h, b, id, VACUUM, beetle);
        h.run(160);
        let outcomes: Vec<u32> = h
            .world_messages(b, from)
            .iter()
            .filter_map(|m| match m {
                ServerMessage::DoStory(e) if e.action.attacker == me => Some(e),
                _ => None,
            })
            .flat_map(|e| e.action.hooks.iter())
            .filter(|k| k.target == beetle)
            .map(|k| k.numbers[0].cast_unsigned())
            .filter(|n| (0x4000_0007..=0x4000_000B).contains(n))
            .collect();
        // The use's own first hook only plays; the last says how it came out.
        let lands = outcomes
            .last()
            .is_some_and(|n| matches!(n, 0x4000_0008 | 0x4000_0009));
        let taunts = h
            .server
            .things
            .get(beetle)
            .and_then(|t| t.creature.as_ref())
            .expect("a creature")
            .taunts
            .clone();
        if lands {
            landed += 1;
            assert_eq!(
                taunts.iter().map(|e| (e.1, e.2)).collect::<Vec<_>>(),
                [(me, 50)],
                "use {id}"
            );
        } else if !outcomes.is_empty() {
            not += 1;
            assert!(taunts.is_empty(), "use {id}: {taunts:?}");
        }
    }
    assert!(landed > 0 && not > 0, "{landed} landed, {not} not");
}

/// The same debuff landed again while the first runs is a second record: the appearance is not
/// shown again, and it stops only as the later one runs out.
/// Behaviour: server.effects.creature-debuffs-stack
#[test]
fn a_second_debuff_keeps_the_appearance_until_the_later_one_runs_out() {
    let (mut h, b, me, beetles) = beside_beetles(1);
    let beetle = beetles[0];
    place(&mut h, me, beetle, 6.0, 0.0);
    h.arm(b, me, STAVE);
    let (_, first) = until_it_lands(&mut h, b, VACUUM, beetle, VACUUM_FX);
    // Again after the recovery, until a second record is on.
    let mut id = 100;
    let second = loop {
        h.run(110);
        let from = use_skill(&mut h, b, id, VACUUM, beetle);
        h.run(60);
        let n = effects::records_of(&h.server, On::Creature(beetle))
            .iter()
            .filter(|e| e.state == VACUUM_EFFECT)
            .count();
        if n == 2 {
            break from;
        }
        id += 1;
        assert!(id < 103, "a second record");
    };
    let later = effects::records_of(&h.server, On::Creature(beetle))
        .iter()
        .filter(|e| e.state == VACUUM_EFFECT)
        .map(|e| e.cast)
        .fold(0.0, f64::max);
    assert_eq!(
        h.world_messages(b, first)
            .iter()
            .filter(|m| shows(m, beetle, VACUUM_FX))
            .count(),
        1,
        "shown once"
    );
    let off = step_until(&mut h, b, second, 800, |m| stops(m, beetle, VACUUM_FX)).expect("it ends");
    assert!(
        (off - later - 30.0).abs() < 0.2,
        "{} after the later",
        off - later
    );
}

/// A hold on a creature (Paralyze) sends it a speed of nothing as it lands, and its speed again
/// as the hold ends, before its appearance stops; its acceleration is never sent (the hold does not
/// change it).
/// Behaviour: server.effects.creature-speed
#[test]
fn a_held_creature_is_sent_no_speed_and_its_own_again_when_the_hold_ends() {
    let (mut h, b, me, beetles) = beside_beetles(1);
    let beetle = beetles[0];
    place(&mut h, me, beetle, 0.8, 0.0);
    h.arm(b, me, SWORD);
    let (on, from) = until_it_lands(&mut h, b, PARALYZE, beetle, PARALYZE_FX);
    let off = step_until(&mut h, b, from, 400, |m| stops(m, beetle, PARALYZE_FX)).expect("it ends");
    assert!(
        (off - on - 3.0).abs() < 0.2,
        "a rating of 1 holds 3 s: {}",
        off - on
    );
    let msgs = h.world_messages(b, from);
    let speeds: Vec<f32> = msgs
        .iter()
        .filter_map(|m| match m {
            ServerMessage::VelocityScale(e) if e.sender.id == beetle => Some(e.action.0),
            _ => None,
        })
        .collect();
    assert_eq!(speeds, [0.0, 1.0]);
    assert!(!msgs
        .iter()
        .any(|m| matches!(m, ServerMessage::AccelerationScale(e) if e.sender.id == beetle)));
    let freed = msgs
        .iter()
        .position(|m| matches!(m, ServerMessage::VelocityScale(e) if e.sender.id == beetle && e.action.0 == 1.0))
        .expect("its speed again");
    let stopped = msgs
        .iter()
        .position(|m| stops(m, beetle, PARALYZE_FX))
        .expect("stopped");
    assert!(freed < stopped);
    assert!(effects::records_of(&h.server, On::Creature(beetle))
        .iter()
        .all(|e| e.state != PARALYZE_EFFECT));
}

/// The ticks of a damage over time on a creature: stories the creature tells, with the caster as
/// their attacker and one hook (`0x981`, a hit, the health lost); the first within ten and a half
/// seconds of the add, then every 5.6 to 11.3 seconds, the last as it runs out (Charged Air's
/// fifteen seconds); each the rate (-2 a second at a rating of 1) times the seconds since the last.
/// Behaviour: server.effects.creature-over-time
#[test]
fn a_damage_over_time_on_a_creature_ticks_as_the_creature_tells_it() {
    let (mut h, b, me, beetles) = beside_beetles(1);
    let beetle = beetles[0];
    place(&mut h, me, beetle, 6.0, 0.0);
    if let Some(t) = h.server.things.get_mut(beetle) {
        t.max_health = 100_000;
        t.health = 100_000;
    }
    h.arm(b, me, STAVE);
    let (on, _) = until_it_lands(&mut h, b, CHARGED_AIR, beetle, OVER_TIME_FX);
    assert!(effects::records_of(&h.server, On::Creature(beetle))
        .iter()
        .any(|e| e.state == CHARGED_AIR_EFFECT));
    let mut ticks: Vec<(f64, i32)> = Vec::new();
    let mut seen = h.bots[b].inbox.len();
    let mut off = None;
    for _ in 0..400 {
        h.step();
        for m in h.world_messages(b, seen) {
            match m {
                ServerMessage::DoStory(e)
                    if e.sender.id == beetle
                        && e.action.bits == 0x2
                        && e.action.attacker == me
                        && e.action.hooks.len() == 1
                        && e.action.hooks[0].bits == 0x981 =>
                {
                    ticks.push((h.now.0, e.action.hooks[0].numbers[3]));
                }
                m if stops(&m, beetle, OVER_TIME_FX) => off = Some(h.now.0),
                _ => {}
            }
        }
        seen = h.bots[b].inbox.len();
        if off.is_some() {
            break;
        }
    }
    let off = off.expect("it runs out");
    assert!((off - on - 15.0).abs() < 0.2, "{}", off - on);
    assert!(ticks.len() >= 2, "{ticks:?}");
    assert!(ticks[0].0 - on <= 10.4 + 0.1, "first {}", ticks[0].0 - on);
    for w in ticks.windows(2).take(ticks.len() - 2) {
        let gap = w[1].0 - w[0].0;
        assert!((5.6 - 0.1..=11.3 + 0.1).contains(&gap), "gap {gap}");
    }
    assert!(
        (ticks[ticks.len() - 1].0 - off).abs() < 0.06,
        "the last at the end"
    );
    let mut last = on;
    for (t, change) in &ticks {
        #[allow(clippy::cast_possible_truncation)]
        let want = (-2.0 * (t - last)).floor() as i32;
        assert!((change - want).abs() <= 1, "{change} after {}", t - last);
        last = *t;
    }
}

/// A creature dead while a damage over time runs on it still tells its ticks, which change nothing;
/// its record runs out as it would, its appearance stopped then.
/// Behaviour: server.effects.creature-over-time-after-death
#[test]
fn a_dead_creatures_damage_over_time_runs_on_and_changes_nothing() {
    let (mut h, b, me, beetles) = beside_beetles(1);
    let beetle = beetles[0];
    place(&mut h, me, beetle, 6.0, 0.0);
    h.arm(b, me, STAVE);
    if let Some(t) = h.server.things.get_mut(beetle) {
        t.max_health = 100_000;
        t.health = 100_000;
    }
    let (on, _) = until_it_lands(&mut h, b, CHARGED_AIR, beetle, OVER_TIME_FX);
    // Dead, and still in the world.
    if let Some(t) = h.server.things.get_mut(beetle) {
        t.health = 0;
    }
    let from = h.bots[b].inbox.len();
    let off = step_until(&mut h, b, from, 400, |m| stops(m, beetle, OVER_TIME_FX))
        .expect("its record runs out");
    assert!((off - on - 15.0).abs() < 0.2, "{}", off - on);
    let told = h
        .world_messages(b, from)
        .iter()
        .filter(|m| matches!(m, ServerMessage::DoStory(e) if e.sender.id == beetle && e.action.bits == 0x2))
        .count();
    assert!(told >= 1, "its ticks still told once it is dead");
    assert_eq!(
        h.server.things.get(beetle).map(|t| t.health),
        Some(0),
        "and change nothing"
    );
}

/// One caster's records of one effect on a creature stop at the most it holds, as on a player: the
/// oldest goes, and nothing is left waiting for its end or its next tick.
/// Behaviour: server.effects.one-caster-of-one-effect-stops-at-a-limit
#[test]
fn records_on_a_creature_stop_at_the_limit_with_nothing_left_waiting() {
    let (mut h, _, me, beetles) = beside_beetles(1);
    let beetle = beetles[0];
    if let Some(t) = h.server.things.get_mut(beetle) {
        t.max_health = 100_000;
        t.health = 100_000;
    }
    let cast = effects::Cast {
        caster: me,
        spellcraft: 1.0,
        ..effects::Cast::default()
    };
    let on = On::Creature(beetle);
    let put = |h: &mut Harness| {
        let clock = h.clock();
        effects::apply_on(
            &mut h.server,
            &mut h.net,
            on,
            CHARGED_AIR_EFFECT,
            cast,
            clock,
        );
    };
    for _ in 0..effects::MOST_OF_ONE_EFFECT {
        put(&mut h);
    }
    let waiting = |h: &Harness| {
        h.server
            .timers
            .iter()
            .filter(|(_, t)| {
                matches!(t,
                yalain_world::Timer::CreatureEffectEnds { creature, .. }
                | yalain_world::Timer::CreatureEffectTick { creature, .. }
                if *creature == beetle)
            })
            .count()
    };
    let before = waiting(&h);
    let first = effects::records_of(&h.server, on)[0].eid;
    put(&mut h);
    let held = effects::records_of(&h.server, on);
    assert_eq!(held.len(), effects::MOST_OF_ONE_EFFECT);
    assert!(held.iter().all(|e| e.eid != first), "the oldest went");
    assert_eq!(waiting(&h), before, "nothing of it waits");
    // A crowd may stack on a creature past what a player holds: none of it goes.
    let crowd = effects::MOST_FROM_OTHERS + 10;
    for caster in 0..crowd {
        let by = effects::Cast {
            caster: 0x2130_0000_0001_0000 + u64::try_from(caster).expect("few"),
            ..cast
        };
        let clock = h.clock();
        effects::apply_on(&mut h.server, &mut h.net, on, CHARGED_AIR_EFFECT, by, clock);
    }
    assert_eq!(
        effects::records_of(&h.server, on).len(),
        effects::MOST_OF_ONE_EFFECT + crowd
    );
}

/// The stun fixture: a harm of class 55, of another class, the short immunity and what it leaves
/// (their classes and times as the data has them), put in the world's table.
const SHORT: u32 = 0x7000_0720;
const LONG: u32 = 0x7000_0721;
const STUN: u32 = 0x7000_8101;
const OTHER_STUN: u32 = 0x7000_8102;
const SLOW: u32 = 0x7000_8103;

fn with_immunity(h: &mut Harness) {
    use yalain_world::effect_defs::{EffectDef, Kind, Scale};
    let def = |state: u32, kind: Kind, class: u32, seconds: f32| EffectDef {
        state,
        kind,
        duration: Scale(vec![(0.0, seconds)]),
        priority: Scale(vec![(0.0, 1.0)]),
        fx: 0,
        external_flags: 0,
        internal_flags: yalain_world::effect_defs::HARMFUL,
        enum_value: 0,
        eq_class: class,
        name: None,
    };
    let t = &mut h.server.effect_table.effects;
    t.insert(SHORT, def(SHORT, Kind::Immunity { then: LONG }, 100, 5.0));
    let ward = Kind::Ward {
        classes: vec![55, 100],
        chance: Scale(vec![(0.0, 1.0)]),
    };
    t.insert(LONG, def(LONG, ward, 100, 60.0));
    for (state, class) in [(STUN, 55), (OTHER_STUN, 55), (SLOW, 7)] {
        t.insert(state, def(state, Kind::Other, class, 10.0));
    }
}

/// A use that stuns: its stun and the short immunity, as a skill's target effects come on.
fn stunned(h: &mut Harness, on: On, stun: u32, caster: u64) -> bool {
    let clock = h.clock();
    let cast = effects::Cast {
        caster,
        spellcraft: 10.0,
        ..effects::Cast::default()
    };
    let landed = effects::apply_on(&mut h.server, &mut h.net, on, stun, cast, clock);
    if landed {
        effects::apply_on(&mut h.server, &mut h.net, on, SHORT, cast, clock);
    }
    landed
}

fn step_for(h: &mut Harness, seconds: u32) {
    for _ in 0..seconds * 10 {
        h.step_by(0.1);
    }
}

/// A stun that lands leaves its target open to another for five seconds, then keeps every stun off,
/// whoever's and of whatever skill, for sixty; a harm of another kind still lands, and when the
/// time is up a stun lands again. A second stun within the five seconds starts them over.
/// Behaviour: server.effects.crowd-control-immunity
#[test]
fn a_stun_keeps_its_kind_off_a_creature_for_a_minute_after_five_seconds() {
    let (mut h, _b, me, beetles) = beside_beetles(1);
    with_immunity(&mut h);
    let on = On::Creature(beetles[0]);
    assert!(stunned(&mut h, on, STUN, me), "the first lands");
    step_for(&mut h, 3);
    assert!(stunned(&mut h, on, STUN, me), "within the five seconds");
    step_for(&mut h, 3);
    let holds = |h: &Harness, state: u32| {
        effects::records_of(&h.server, on)
            .iter()
            .any(|e| e.state == state)
    };
    assert!(holds(&h, SHORT), "started over at the second");
    assert!(!holds(&h, LONG), "the first's end leaves nothing yet");
    assert!(stunned(&mut h, on, STUN, me), "still open");
    step_for(&mut h, 6);
    assert!(holds(&h, LONG), "the long one on");
    assert!(!holds(&h, SHORT));
    assert!(!stunned(&mut h, on, STUN, me), "kept off");
    assert!(
        !stunned(&mut h, on, OTHER_STUN, 7),
        "another's, another skill's"
    );
    let clock = h.clock();
    let cast = effects::Cast {
        caster: me,
        spellcraft: 10.0,
        ..effects::Cast::default()
    };
    assert!(
        effects::apply_on(&mut h.server, &mut h.net, on, SLOW, cast, clock),
        "another kind lands"
    );
    step_for(&mut h, 58);
    assert!(!stunned(&mut h, on, STUN, me), "58 s into the minute");
    step_for(&mut h, 3);
    assert!(!holds(&h, LONG), "the minute is over");
    assert!(stunned(&mut h, on, STUN, me), "lands again");
}

/// On a player the rule is the same; its client is never told of the five seconds, and is told of
/// the minute when it starts and ends.
/// Behaviour: server.effects.crowd-control-immunity-on-a-player
#[test]
fn a_stun_keeps_its_kind_off_a_player_and_only_the_minute_is_told() {
    let (mut h, b, me, beetles) = beside_beetles(1);
    with_immunity(&mut h);
    let world = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .map(|(w, _)| w)
        .expect("in the world");
    let on = On::Player(world);
    let from = h.bots[b].inbox.len();
    let told =
        |h: &Harness, from: usize, f: u32| h.functions(b, from).iter().filter(|x| **x == f).count();
    assert!(stunned(&mut h, on, STUN, beetles[0]));
    h.run(3);
    assert_eq!(told(&h, from, effects::function::ADD), 1, "the stun alone");
    step_for(&mut h, 6);
    assert_eq!(told(&h, from, effects::function::ADD), 2, "and the minute");
    assert!(!stunned(&mut h, on, OTHER_STUN, beetles[0]), "kept off");
    let removes = told(&h, from, effects::function::REMOVE);
    assert_eq!(removes, 0, "the five seconds' end is not told");
    step_for(&mut h, 61);
    assert!(stunned(&mut h, on, STUN, beetles[0]), "lands again");
}
