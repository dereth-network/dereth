//! Skills used by name: a player's special attacks (from rest and inside an attack run), their
//! refusals, the requests that wait, the recall, and the openings creatures leave.

use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_testkit::play::server_call;
use yalain_testkit::{Harness, TEST_CREATURE};
use yalain_world::calls::{self, Value};
use yalain_world::skill_use::{function, refusal, LIFESTONE_RECALL};

const ATTACK_STATE: u32 = 0x8204_001B;
const START_ATTACK: u32 = 0x8204_0008;
const SKILL_INFO: u32 = 0x8214_000D;
const SHOW_TEXT: u32 = 0x8205_0011;
const BOW: u32 = 0x4700_051B;
const SWORD: u32 = 0x4700_051C;
const DEADEYE: u32 = 207;
const RIPOSTE: u32 = 176;
const VIGOR: u32 = 259;

/// The calls a client received since `from`: their functions and values.
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

fn functions(h: &Harness, b: usize, from: usize) -> Vec<u32> {
    calls_of(h, b, from).into_iter().map(|(f, _)| f).collect()
}

/// A new player with the scene drawn beside the test camp's beetle; returns the client, the
/// player and the beetle.
fn beside_a_beetle() -> (Harness, usize, u64, u64) {
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
    (h, b, me, beetle)
}

/// Make the beetle a practice target (as the hall's Straw Drudge: it never fights or moves), so it
/// stays where it is, beyond Deadeye's least range.
fn practice_target(h: &mut Harness, beetle: u64) {
    if let Some(c) = h
        .server
        .things
        .get_mut(beetle)
        .and_then(|t| t.creature.as_mut())
    {
        c.stats.fights = false;
    }
}

/// Ask to use a skill on a target, as the client packs it (the use's number, the skill, the
/// target).
fn use_skill(h: &mut Harness, b: usize, id: u32, skill: u32, target: u64) -> usize {
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

fn vigor_sent(h: &Harness, b: usize, from: usize) -> Vec<i32> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::UpdateInt(q) if q.quality == VIGOR => Some(q.value),
            _ => None,
        })
        .collect()
}

/// A special is critical only by what its weapon adds: never from a weapon that adds nothing, every
/// time from one that adds it all.
/// Behaviour: server.specials.critical-by-weapon-only
#[test]
fn a_specials_critical_comes_only_from_its_weapon() {
    use yalain_world::combat::CRITICAL_BONUS;
    use yalain_world::effect_defs::{numeric, EffectDef, Kind, Scale};
    use yalain_world::rules::Outcome;
    const BONUS: u32 = 0x6F00_0001;
    let results = |bonus: bool| -> Vec<u32> {
        let (mut h, b, me, beetle) = beside_a_beetle();
        practice_target(&mut h, beetle);
        let bow = h.arm(b, me, BOW);
        h.server.effect_table.effects.insert(
            BONUS,
            EffectDef {
                state: BONUS,
                kind: Kind::Numeric {
                    flags: numeric::ADD,
                    stats: vec![CRITICAL_BONUS],
                    magnitude: Scale(vec![(0.0, 1.0)]),
                },
                duration: Scale::default(),
                priority: Scale::default(),
                fx: 0,
                external_flags: 0,
                internal_flags: 0,
                enum_value: 0,
                eq_class: 0,
                name: None,
            },
        );
        if bonus {
            h.server
                .things
                .get_mut(bow)
                .expect("held")
                .enchantments
                .push((BONUS, 9.0));
        }
        let t = h.server.things.get_mut(beetle).expect("the beetle");
        t.max_health = 100_000_000;
        t.health = 100_000_000;
        let mut out = Vec::new();
        for use_id in 0..120 {
            let from = use_skill(&mut h, b, 1000 + use_id, DEADEYE, beetle);
            h.run(600);
            for m in h.world_messages(b, from) {
                if let ServerMessage::DoStory(e) = m {
                    for k in &e.action.hooks {
                        if k.target == beetle && k.numbers[3] < 0 {
                            out.push(k.numbers[0].cast_unsigned());
                        }
                    }
                }
            }
            for p in h.server.players.values_mut() {
                yalain_world::quals::set_int(&mut p.qualities, VIGOR, 100);
            }
        }
        out
    };
    let plain = results(false);
    assert!(plain.len() > 30, "{}", plain.len());
    assert!(
        plain.iter().all(|r| *r == Outcome::Hit.code()),
        "{plain:x?}"
    );
    let with = results(true);
    assert!(with.len() > 30, "{}", with.len());
    assert!(
        with.iter().all(|r| *r == Outcome::Critical.code()),
        "{with:x?}"
    );
}

/// Deadeye from rest with the starting bow: the player attacking, in the bow's combat mode,
/// turned to its target; its vigor paid (32 and the bow's 4); the special's story from the player
/// with the use's number, the skill and its missile naming the bow; the beetle tells how the
/// missile came out; then the skill's information (with a time it was last used), the player not
/// attacking, stopped and at peace. No use-over message: it was carried out.
/// Behaviour: server.specials.from-rest
#[test]
fn deadeye_from_rest_shoots_and_ends_with_the_skills_information() {
    let (mut h, b, me, beetle) = beside_a_beetle();
    practice_target(&mut h, beetle);
    let bow = h.arm(b, me, BOW);
    let from = use_skill(&mut h, b, 6, DEADEYE, beetle);
    h.run(80);
    let msgs = h.world_messages(b, from);
    let calls = functions(&h, b, from);
    assert_eq!(
        calls.iter().filter(|f| **f == ATTACK_STATE).count(),
        2,
        "raised and lowered: {calls:x?}"
    );
    assert!(msgs.iter().any(|m| matches!(m, ServerMessage::DoMode(e) if e.sender.id == me && e.action.0 == 0x4100_0010)), "the bow's mode");
    assert!(msgs.iter().any(|m| matches!(m, ServerMessage::MoveTo(e) if e.sender.id == me && e.action.params.flags == 0x46A)), "turned to the target");
    assert_eq!(vigor_sent(&h, b, from).first(), Some(&64), "100 less 36");
    let story = msgs
        .iter()
        .find_map(|m| match m {
            ServerMessage::DoStory(e) if e.sender.id == me => Some(e.action.clone()),
            _ => None,
        })
        .expect("the special's story");
    assert_eq!(story.bits, 0x1E);
    assert_eq!((story.attack_context, story.skill), (6, DEADEYE));
    assert_eq!(story.attacker_behavior.behavior, 0x4100_007A);
    assert_eq!(story.hooks[1].bits, 0x323);
    assert_eq!(
        (story.hooks[1].target, story.hooks[1].weapon),
        (beetle, bow)
    );
    assert!(
        msgs.iter().any(|m| matches!(m, ServerMessage::DoStory(e) if e.sender.id == beetle && e.action.bits == 0x12 && e.action.attacker == me)),
        "the beetle tells how the missile came out"
    );
    let info = calls_of(&h, b, from)
        .into_iter()
        .find(|(f, _)| *f == SKILL_INFO)
        .expect("the skill's information");
    assert!(!info.1.is_empty());
    assert!(
        !calls.contains(&function::ENDED),
        "a use carried out is not told over"
    );
    assert_eq!(
        calls.last(),
        Some(&ATTACK_STATE),
        "not attacking at the end"
    );
    assert!(msgs.iter().any(|m| matches!(m, ServerMessage::DoMode(e) if e.sender.id == me && e.action.0 == 0x4000_0001)), "at peace");
    assert!(h.server.players.values().all(|p| p.using.is_none()));
}

/// A special goes at its player's pace, from rest and inside a run.
/// Behaviour: server.weapons.attack-speed-effects
#[test]
fn a_special_goes_at_its_players_pace() {
    for run in [false, true] {
        let (mut h, b, me, beetle) = beside_a_beetle();
        practice_target(&mut h, beetle);
        h.arm(b, me, BOW);
        super::attack_speed::speeded(&mut h, 27, 1.7, true);
        if run {
            h.bots[b].send(
                true,
                queue::WEENIE,
                &server_call(START_ATTACK, &[Arg::LongInt(beetle)]),
            );
            h.run(30);
        }
        let from = use_skill(&mut h, b, 6, DEADEYE, beetle);
        h.run(80);
        let ts = h
            .world_messages(b, from)
            .iter()
            .find_map(|m| match m {
                ServerMessage::DoStory(e) if e.sender.id == me && e.action.skill == DEADEYE => {
                    Some(e.action.attacker_behavior.time_scale)
                }
                _ => None,
            })
            .expect("the special's story");
        assert!((1.72..=2.12).contains(&ts), "{ts} in a run: {run}");
    }
}

/// A skill whose definition says it goes faster tells its story at the square of that, and what
/// comes next follows: at Sinew Snap's 1.6, 2.56 times what the player's other skills are told at.
/// Behaviour: server.specials.combat-speed-squared
#[test]
fn a_skill_that_says_it_goes_faster_is_told_at_the_square_of_it() {
    const LONG: u32 = 0x4100_007B;
    let told = |speed: Option<f32>, run: bool| -> Vec<(f32, f64)> {
        let (mut h, b, me, beetle) = beside_a_beetle();
        practice_target(&mut h, beetle);
        h.arm(b, me, BOW);
        let u = h
            .server
            .skill_table
            .skills
            .get_mut(&DEADEYE)
            .and_then(|d| d.usage.as_mut())
            .expect("Deadeye");
        u.combat_speed = speed;
        u.behavior_other = [LONG; 3];
        if run {
            h.bots[b].send(
                true,
                queue::WEENIE,
                &server_call(START_ATTACK, &[Arg::LongInt(beetle)]),
            );
            h.run(30);
        }
        let from = use_skill(&mut h, b, 6, DEADEYE, beetle);
        let mut at = Vec::new();
        for _ in 0..200 {
            h.step();
            let n = at.len();
            for (i, m) in h.world_messages(b, from).iter().enumerate() {
                if i < n {
                    continue;
                }
                let ts = match m {
                    ServerMessage::DoStory(e) if e.sender.id == me && e.action.attacker == me => {
                        Some(e.action.attacker_behavior.time_scale)
                    }
                    _ => None,
                };
                at.push((ts.unwrap_or(0.0), h.now.0));
            }
        }
        at.retain(|x| x.0 > 0.0);
        at
    };
    let plain = told(None, true);
    let fast = told(Some(1.6), true);
    assert!(plain.len() > 2 && fast.len() > 2);
    let alone = told(Some(1.6), false);
    let bare = yalain_world::combat::time_scale_at_delay(0);
    assert!(
        (bare * 0.9..=bare * 1.1).contains(&plain[0].0),
        "{:?}",
        plain[0]
    );
    let at = bare * 2.56;
    assert!((at * 0.9..=at * 1.1).contains(&fast[0].0), "{:?}", fast[0]);
    assert!(
        (at * 0.9..=at * 1.1).contains(&alone[0].0),
        "{:?}",
        alone[0]
    );
    // The next story comes the animation over the time scale and the pause on.
    for (run, name) in [(&plain, "plain"), (&fast, "fast")] {
        let gap = run[1].1 - run[0].1;
        let due = 3.25 / f64::from(run[0].0) + yalain_world::skill_use::NEXT_SWING_AFTER;
        assert!(
            (gap - due).abs() < 0.11,
            "{name}: {gap:.3} where {due:.3} was due"
        );
    }
}

/// A special waiting for a run's next swing still goes when the run has nothing it may swing.
/// Behaviour: server.weapons.withheld-run-waits
#[test]
fn a_special_inside_a_run_with_nothing_to_swing_still_goes() {
    const NEVER_CARRIED: u32 = 0x6F00_7E00;
    let (mut h, b, me, beetle) = beside_a_beetle();
    practice_target(&mut h, beetle);
    h.arm(b, me, BOW);
    for d in h.server.skill_table.skills.values_mut() {
        if d.auto_attack {
            d.usage.as_mut().expect("used").required_effects = vec![NEVER_CARRIED];
        }
    }
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(START_ATTACK, &[Arg::LongInt(beetle)]),
    );
    h.run(30);
    let from = use_skill(&mut h, b, 6, DEADEYE, beetle);
    h.run(80);
    let told: Vec<u32> = h
        .world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::DoStory(e) if e.sender.id == me => Some(e.action.skill),
            _ => None,
        })
        .collect();
    assert_eq!(told, [DEADEYE]);
}

/// A special's hook that adds by rating adds at the player's rating in it: Deadeye with a hook of
/// points does that much more than without.
/// Behaviour: server.specials.hook-adds-by-rating
#[test]
fn a_specials_hook_adds_by_the_players_rating() {
    let hits = |points: Vec<(f32, f32)>| -> i32 {
        let (mut h, b, me, beetle) = beside_a_beetle();
        practice_target(&mut h, beetle);
        const BOW_OF_KNOWN_DAMAGE: u32 = 0x4700_07A3;
        let mut p = yalain_content::Props::default();
        p.set("Weapon_Damage", yalain_content::Prop::Int(100));
        h.server.catalog.insert(yalain_testkit::test_template(
            BOW_OF_KNOWN_DAMAGE,
            0x1067,
            p,
        ));
        h.server
            .catalog
            .implements
            .insert(BOW_OF_KNOWN_DAMAGE, yalain_world::skill_use::implement::BOW);
        h.arm(b, me, BOW_OF_KNOWN_DAMAGE);
        let u = h
            .server
            .skill_table
            .skills
            .get_mut(&DEADEYE)
            .and_then(|d| d.usage.as_mut())
            .expect("Deadeye");
        for hook in &mut u.hooks {
            hook.add_by_rating.clone_from(&points);
        }
        if let Some(t) = h.server.things.get_mut(beetle) {
            t.max_health = 10_000_000;
            t.health = 10_000_000;
        }
        for _ in 0..50 {
            let from = use_skill(&mut h, b, 6, DEADEYE, beetle);
            h.run(120);
            let plain = h.world_messages(b, from).iter().find_map(|m| match m {
                ServerMessage::DoStory(e) => e
                    .action
                    .hooks
                    .iter()
                    .find(|k| k.numbers[0] == 0x4000_0008 && k.numbers[3] < 0)
                    .map(|k| -k.numbers[3]),
                _ => None,
            });
            if let Some(d) = plain {
                return d;
            }
        }
        panic!("no plain hit in 50 uses");
    };
    let without = hits(Vec::new());
    let with = hits(vec![(1.0, 40.0), (100.0, 40.0)]);
    assert_eq!(with - without, 40, "{with} {without}");
}

/// The rating a skill's information caches.
fn cached_rating(info: &[Value]) -> u32 {
    match info.first().and_then(Value::root) {
        Some(yalain_world::calls::Node::Script { slots, .. }) => slots[2].1,
        other => panic!("the skill's information: {other:?}"),
    }
}

/// An effect raising a skill raises its rating: the hook's add and the rating its information
/// caches are its holder's with the effect, by whole points.
/// Behaviour: server.skills.rating-with-effects
#[test]
fn an_effect_raising_a_skill_raises_what_its_hook_adds_and_the_rating_cached() {
    use yalain_world::effect_defs::{numeric, EffectDef, Kind, Scale};
    const RAISES: u32 = 0x6F00_0002;
    // Each point of rating adds a point.
    let used = |raised: bool| -> (i32, u32) {
        let (mut h, b, me, beetle) = beside_a_beetle();
        practice_target(&mut h, beetle);
        const BOW_OF_KNOWN_DAMAGE: u32 = 0x4700_07A3;
        let mut p = yalain_content::Props::default();
        p.set("Weapon_Damage", yalain_content::Prop::Int(100));
        h.server.catalog.insert(yalain_testkit::test_template(
            BOW_OF_KNOWN_DAMAGE,
            0x1067,
            p,
        ));
        h.server
            .catalog
            .implements
            .insert(BOW_OF_KNOWN_DAMAGE, yalain_world::skill_use::implement::BOW);
        h.arm(b, me, BOW_OF_KNOWN_DAMAGE);
        let d = h
            .server
            .skill_table
            .skills
            .get_mut(&DEADEYE)
            .expect("Deadeye");
        d.cap = 50;
        for hook in &mut d.usage.as_mut().expect("used").hooks {
            hook.add_by_rating = vec![(0.0, 0.0), (1000.0, 1000.0)];
        }
        h.server.effect_table.effects.insert(
            RAISES,
            EffectDef {
                state: RAISES,
                kind: Kind::Numeric {
                    flags: numeric::ADD,
                    stats: vec![yalain_world::effects::SKILL_STAT | DEADEYE],
                    magnitude: Scale(vec![(0.0, 6.82)]),
                },
                duration: Scale::default(),
                priority: Scale::default(),
                fx: 0,
                external_flags: 0,
                internal_flags: 0,
                enum_value: 0,
                eq_class: 0,
                name: None,
            },
        );
        if raised {
            let cast = h.net.game_time(h.now);
            let p = h.server.players.values_mut().next().expect("the player");
            let eid = yalain_world::effects::next_eid(p);
            p.play.effects.push(yalain_world::state::EffectState {
                eid,
                state: RAISES,
                cast,
                timeout: 1.0e9,
                caster: 0,
                spellcraft: 10.0,
                pk: false,
                acting: 0,
                skill: 0,
                item: 0,
                flags: 0,
                categories: 1,
            });
        }
        if let Some(t) = h.server.things.get_mut(beetle) {
            t.max_health = 10_000_000;
            t.health = 10_000_000;
        }
        for _ in 0..50 {
            let from = use_skill(&mut h, b, 6, DEADEYE, beetle);
            h.run(120);
            let plain = h.world_messages(b, from).iter().find_map(|m| match m {
                ServerMessage::DoStory(e) => e
                    .action
                    .hooks
                    .iter()
                    .find(|k| k.numbers[0] == 0x4000_0008 && k.numbers[3] < 0)
                    .map(|k| -k.numbers[3]),
                _ => None,
            });
            if let Some(d) = plain {
                let info = calls_of(&h, b, from)
                    .into_iter()
                    .find(|(f, _)| *f == SKILL_INFO)
                    .expect("the skill's information");
                return (d, cached_rating(&info.1));
            }
        }
        panic!("no plain hit in 50 uses");
    };
    let (d0, r0) = used(false);
    let (d1, r1) = used(true);
    assert_eq!(r1, r0 + 6, "cached {r1} over {r0}");
    assert_eq!(d1 - d0, 6, "hit {d1} over {d0}");
}

/// A special's critical does a quarter more than its plain hit, not double, its hook's add with the
/// rest.
/// Behaviour: server.specials.critical-size
#[test]
fn a_specials_critical_does_a_quarter_more_than_its_plain_hit() {
    let (mut h, b, me, beetle) = beside_a_beetle();
    practice_target(&mut h, beetle);
    const BOW_OF_KNOWN_DAMAGE: u32 = 0x4700_07A3;
    let mut p = yalain_content::Props::default();
    p.set("Weapon_Damage", yalain_content::Prop::Int(100))
        .set("Weapon_Variance", yalain_content::Prop::Float(0.0));
    h.server.catalog.insert(yalain_testkit::test_template(
        BOW_OF_KNOWN_DAMAGE,
        0x1067,
        p,
    ));
    h.server
        .catalog
        .implements
        .insert(BOW_OF_KNOWN_DAMAGE, yalain_world::skill_use::implement::BOW);
    let bow = h.arm(b, me, BOW_OF_KNOWN_DAMAGE);
    // A special is critical only by its weapon's bonus: this one, half the time.
    const BONUS: u32 = 0x6F00_0001;
    h.server.effect_table.effects.insert(
        BONUS,
        yalain_world::effect_defs::EffectDef {
            state: BONUS,
            kind: yalain_world::effect_defs::Kind::Numeric {
                flags: yalain_world::effect_defs::numeric::ADD,
                stats: vec![yalain_world::combat::CRITICAL_BONUS],
                magnitude: yalain_world::effect_defs::Scale(vec![(0.0, 0.5)]),
            },
            duration: yalain_world::effect_defs::Scale::default(),
            priority: yalain_world::effect_defs::Scale::default(),
            fx: 0,
            external_flags: 0,
            internal_flags: 0,
            enum_value: 0,
            eq_class: 0,
            name: None,
        },
    );
    h.server
        .things
        .get_mut(bow)
        .expect("held")
        .enchantments
        .push((BONUS, 9.0));
    let u = h
        .server
        .skill_table
        .skills
        .get_mut(&DEADEYE)
        .and_then(|d| d.usage.as_mut())
        .expect("Deadeye");
    for hook in &mut u.hooks {
        hook.add_by_rating = vec![(1.0, 40.0), (100.0, 40.0)];
    }
    if let Some(t) = h.server.things.get_mut(beetle) {
        t.max_health = 10_000_000;
        t.health = 10_000_000;
    }
    h.server.rng = yalain_world::rng::Rng::new(7);
    let (mut plain, mut critical) = (None, None);
    for _ in 0..1000 {
        if plain.is_some() && critical.is_some() {
            break;
        }
        let from = use_skill(&mut h, b, 6, DEADEYE, beetle);
        h.run(120);
        for m in h.world_messages(b, from) {
            let ServerMessage::DoStory(e) = m else {
                continue;
            };
            for k in &e.action.hooks {
                if k.numbers[3] < 0 && k.numbers[0] == 0x4000_0008 {
                    plain = Some(-k.numbers[3]);
                } else if k.numbers[3] < 0 && k.numbers[0] == 0x4000_0009 {
                    critical = Some(-k.numbers[3]);
                }
            }
        }
    }
    let (plain, critical) = (plain.expect("a plain hit"), critical.expect("a critical"));
    assert_eq!(critical, (plain * 5 + 2) / 4, "{plain}");
}

/// A hook of the left hand does the damage of what that hand holds, a shield's too, and the right
/// hand's weapon's with nothing there that does damage. A hook of the right hand is untouched.
/// Behaviour: server.specials.hook-of-the-left-hand
#[test]
fn a_hook_of_the_left_hand_does_the_damage_of_what_it_holds() {
    use yalain_content::{Prop, Props};
    use yalain_world::skill_use::hook_flag::LEFT_HAND;
    const SHIELD: u32 = 0x4700_07A4;
    const CLOTH: u32 = 0x4700_07A5;
    const BOW_OF_KNOWN_DAMAGE: u32 = 0x4700_07A3;
    let hit = |flags: u32, left: Option<u32>| -> i32 {
        let (mut h, b, me, beetle) = beside_a_beetle();
        practice_target(&mut h, beetle);
        let known = |h: &mut Harness, t: u32, damage: i64| {
            let mut p = Props::default();
            p.set("Weapon_Damage", Prop::Int(damage))
                .set("Weapon_Variance", Prop::Float(0.0));
            h.server
                .catalog
                .insert(yalain_testkit::test_template(t, 0x1067, p));
        };
        known(&mut h, BOW_OF_KNOWN_DAMAGE, 100);
        known(&mut h, SHIELD, 30);
        h.server.catalog.insert(yalain_testkit::test_template(
            CLOTH,
            0x200,
            Props::default(),
        ));
        h.server
            .catalog
            .implements
            .insert(BOW_OF_KNOWN_DAMAGE, yalain_world::skill_use::implement::BOW);
        h.arm(b, me, BOW_OF_KNOWN_DAMAGE);
        if let Some(template) = left {
            let id =
                yalain_world::inventory::make_item(&mut h.server, template, 1, 0).expect("made");
            let p = h.server.players.values_mut().next().expect("in the world");
            let slot = u32::try_from(p.play.items.len()).unwrap_or(0);
            p.play.items.push(yalain_world::state::Held {
                id,
                template,
                slot,
                location: yalain_world::equip::OFF_HAND,
                quantity: 1,
            });
        }
        let u = h
            .server
            .skill_table
            .skills
            .get_mut(&DEADEYE)
            .and_then(|d| d.usage.as_mut())
            .expect("Deadeye");
        u.implements_left = -1;
        for hook in &mut u.hooks {
            hook.flags |= flags;
            hook.add = 0.0;
            hook.add_by_rating.clear();
        }
        if let Some(t) = h.server.things.get_mut(beetle) {
            t.max_health = 10_000_000;
            t.health = 10_000_000;
            if let Some(c) = t.creature.as_mut() {
                c.stats.armor = 0;
            }
        }
        h.server.rng = yalain_world::rng::Rng::new(7);
        for _ in 0..200 {
            let from = use_skill(&mut h, b, 6, DEADEYE, beetle);
            h.run(120);
            for m in h.world_messages(b, from) {
                let ServerMessage::DoStory(e) = m else {
                    continue;
                };
                for k in &e.action.hooks {
                    if k.numbers[3] < 0 && k.numbers[0] == 0x4000_0008 {
                        return -k.numbers[3];
                    }
                }
            }
        }
        panic!("no plain hit");
    };
    assert_eq!(hit(0, Some(SHIELD)), 100, "the right hand's own hook");
    assert_eq!(hit(LEFT_HAND, None), 100, "nothing in the left hand");
    assert_eq!(
        hit(LEFT_HAND, Some(CLOTH)),
        100,
        "nothing there with damage"
    );
    assert_eq!(hit(LEFT_HAND, Some(SHIELD)), 30, "the shield's");
}

/// An effect that lowers a skill's cost does, a fraction rounded up, down to half the skill's own:
/// Deadeye costs 32 and the bow's 4.
/// Behaviour: server.specials.vigor-lowered-by-effects
#[test]
fn an_effect_lowers_what_a_skill_costs_down_to_half_its_own() {
    use yalain_world::effect_defs::{EffectDef, Kind, Scale};
    const LOWERS: u32 = 0x6F00_7777;
    let paid = |by: f32| -> i32 {
        let (mut h, b, me, beetle) = beside_a_beetle();
        practice_target(&mut h, beetle);
        h.arm(b, me, BOW);
        let mut d: EffectDef = h
            .server
            .effect_table
            .effects
            .values()
            .next()
            .cloned()
            .expect("an effect");
        d.state = LOWERS;
        d.fx = 0;
        d.eq_class = 0;
        d.duration = Scale(vec![(0.0, 3000.0)]);
        d.kind = Kind::Numeric {
            flags: 0,
            stats: vec![yalain_world::effects::VIGOR_COST_ADD],
            magnitude: Scale(vec![(0.0, by)]),
        };
        h.server.effect_table.effects.insert(LOWERS, d);
        let world = *h.server.players.keys().next().expect("in the world");
        let clock = h.clock();
        let cast = yalain_world::effects::Cast {
            caster: me,
            spellcraft: 1.0,
            ..yalain_world::effects::Cast::default()
        };
        assert!(yalain_world::effects::apply(
            &mut h.server,
            &mut h.net,
            world,
            LOWERS,
            cast,
            clock
        ));
        h.run(2);
        let from = use_skill(&mut h, b, 6, DEADEYE, beetle);
        h.run(80);
        100 - vigor_sent(&h, b, from).first().copied().expect("it paid")
    };
    assert_eq!(paid(0.0), 36);
    assert_eq!(paid(-10.0), 26);
    assert_eq!(paid(-10.5), 26, "rounded up");
    assert_eq!(paid(-1000.0), 16, "half its own");
}

/// What an aura multiplies a held item's cost by comes off that item, rounded down: Deadeye costs its
/// 32 and a bow of 18, which at 0.72 costs 12.
/// Behaviour: server.specials.vigor-of-what-is-held-under-an-aura
#[test]
fn an_aura_on_what_is_held_lowers_its_cost_rounded_down() {
    use yalain_world::effect_defs::{aura_target, numeric, Aura, EffectDef, Kind, Scale};
    const LOWERS: u32 = 0x6F00_7778;
    const LOOTED: u32 = 0x4700_07A3;
    let paid = |by: Option<f32>| -> i32 {
        let (mut h, b, me, beetle) = beside_a_beetle();
        practice_target(&mut h, beetle);
        let mut p = yalain_content::Props::default();
        p.set("Weapon_VigorCost", yalain_content::Prop::Int(18));
        h.server
            .catalog
            .insert(yalain_testkit::test_template(LOOTED, 0x1067, p));
        h.server
            .catalog
            .implements
            .insert(LOOTED, yalain_world::skill_use::implement::BOW);
        h.arm(b, me, LOOTED);
        if let Some(by) = by {
            let mut d: EffectDef = h
                .server
                .effect_table
                .effects
                .values()
                .next()
                .cloned()
                .expect("an effect");
            d.state = LOWERS;
            d.fx = 0;
            d.eq_class = 0;
            d.duration = Scale(vec![(0.0, 3000.0)]);
            d.kind = Kind::Numeric {
                flags: numeric::MULTIPLY,
                stats: vec![yalain_world::made::stat::VIGOR],
                magnitude: Scale(vec![(0.0, by)]),
            };
            let aura = EffectDef {
                state: LOWERS + 1,
                kind: Kind::Aura(Aura {
                    pulsed: vec![LOWERS],
                    targets: aura_target::ITEM,
                    range: 300.0,
                }),
                ..d.clone()
            };
            h.server.effect_table.effects.insert(LOWERS, d);
            h.server.effect_table.effects.insert(LOWERS + 1, aura);
            let world = *h.server.players.keys().next().expect("in the world");
            let clock = h.clock();
            yalain_world::effects::aura_on(
                &mut h.server,
                &mut h.net,
                world,
                LOWERS + 1,
                0,
                1.0,
                clock,
            );
            h.run(2);
        }
        let from = use_skill(&mut h, b, 6, DEADEYE, beetle);
        h.run(80);
        100 - vigor_sent(&h, b, from).first().copied().expect("it paid")
    };
    assert_eq!(paid(None), 32 + 18);
    assert_eq!(paid(Some(0.72)), 32 + 12);
}

/// A bow the player came by costs what it says, not what a starting bow does.
/// Behaviour: server.specials.vigor-of-what-is-held
#[test]
fn deadeye_with_a_looted_bow_pays_that_bows_cost() {
    let (mut h, b, me, beetle) = beside_a_beetle();
    practice_target(&mut h, beetle);
    const LOOTED: u32 = 0x4700_07A3;
    let mut p = yalain_content::Props::default();
    p.set("Weapon_VigorCost", yalain_content::Prop::Int(18));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(LOOTED, 0x1067, p));
    h.server
        .catalog
        .implements
        .insert(LOOTED, yalain_world::skill_use::implement::BOW);
    h.arm(b, me, LOOTED);
    let from = use_skill(&mut h, b, 6, DEADEYE, beetle);
    h.run(80);
    assert_eq!(
        vigor_sent(&h, b, from).first(),
        Some(&50),
        "100 less 32 and 18"
    );
}

/// A special with nothing in hand is refused in one tick: the player attacking, the use over,
/// why (not wielding the right item), not attacking. The recall too needs something in the right
/// hand.
/// Behaviour: server.specials.refused
#[test]
fn a_special_or_the_recall_with_an_empty_hand_is_refused() {
    let (mut h, b, _, beetle) = beside_a_beetle();
    for (id, skill, target) in [(1, DEADEYE, beetle), (2, LIFESTONE_RECALL, 0)] {
        let from = use_skill(&mut h, b, id, skill, target);
        h.run(10);
        let got = calls_of(&h, b, from);
        assert_eq!(
            got,
            vec![
                (ATTACK_STATE, vec![Value::Int(1)]),
                (function::ENDED, vec![Value::Int(id)]),
                (
                    function::REFUSED,
                    vec![
                        Value::Int(skill),
                        Value::Long(target),
                        Value::Int(refusal::WRONG_IMPLEMENT)
                    ]
                ),
                (ATTACK_STATE, vec![Value::Int(0)]),
            ],
            "skill {skill}"
        );
    }
}

/// Used again before its recovery (two seconds for Deadeye, from its last use's end), a special is
/// refused as not reset; used after, it is carried out.
/// Behaviour: server.specials.recovery
#[test]
fn a_special_waits_its_recovery_from_its_last_use() {
    let (mut h, b, me, beetle) = beside_a_beetle();
    practice_target(&mut h, beetle);
    h.arm(b, me, BOW);
    use_skill(&mut h, b, 1, DEADEYE, beetle);
    h.run(60);
    let from = use_skill(&mut h, b, 2, DEADEYE, beetle);
    h.run(10);
    assert!(
        calls_of(&h, b, from).contains(&(
            function::REFUSED,
            vec![
                Value::Int(DEADEYE),
                Value::Long(beetle),
                Value::Int(refusal::NOT_RESET)
            ]
        )),
        "not reset yet"
    );
    h.run(40);
    let from = use_skill(&mut h, b, 3, DEADEYE, beetle);
    h.run(20);
    assert!(
        h.world_messages(b, from)
            .iter()
            .any(|m| matches!(m, ServerMessage::DoStory(e) if e.sender.id == me && e.action.attack_context == 3)),
        "used once its recovery is over"
    );
}

/// Inside a sword run, Riposte waits for the run's next swing and takes it: its story with the
/// use's number (no attack state of its own), then the run's basic swings go on after it.
/// Behaviour: server.specials.inside-a-run
#[test]
fn riposte_inside_a_run_takes_one_swing_and_the_run_goes_on() {
    let (mut h, b, me, beetle) = beside_a_beetle();
    h.arm(b, me, SWORD);
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(START_ATTACK, &[Arg::LongInt(beetle)]),
    );
    // Wait for the beetle to come into reach and the first swings.
    assert!(h.run_until(400, |h| h
        .world_messages(b, 0)
        .iter()
        .filter(|m| matches!(m, ServerMessage::DoStory(e) if e.sender.id == me))
        .count()
        >= 2));
    let from = use_skill(&mut h, b, 12, RIPOSTE, beetle);
    h.run(100);
    let msgs = h.world_messages(b, from);
    let stories: Vec<(u32, u32)> = msgs
        .iter()
        .filter_map(|m| match m {
            ServerMessage::DoStory(e) if e.sender.id == me => {
                Some((e.action.bits, e.action.attack_context))
            }
            _ => None,
        })
        .collect();
    let special = stories
        .iter()
        .position(|s| *s == (0x1E, 12))
        .unwrap_or_else(|| panic!("the special's story: {stories:x?}"));
    assert!(
        stories[special + 1..].iter().any(|s| s.0 == 0x36),
        "basic swings after it: {stories:x?}"
    );
    assert!(
        !functions(&h, b, from).contains(&ATTACK_STATE)
            || h.server.things.get(beetle).is_none_or(|t| !t.alive()),
        "no attack state of its own while the run goes on"
    );
}

/// While a special runs, a request waits; a newer one replaces it, and the replaced one is over
/// with no reason.
/// Behaviour: server.specials.one-waits
#[test]
fn a_newer_request_replaces_the_one_waiting() {
    let (mut h, b, me, beetle) = beside_a_beetle();
    practice_target(&mut h, beetle);
    h.arm(b, me, BOW);
    use_skill(&mut h, b, 1, DEADEYE, beetle);
    h.run(5);
    let from = use_skill(&mut h, b, 2, DEADEYE, beetle);
    h.run(1);
    use_skill(&mut h, b, 3, DEADEYE, beetle);
    h.run(10);
    let got = calls_of(&h, b, from);
    assert_eq!(
        got,
        vec![(function::ENDED, vec![Value::Int(2)])],
        "the waiting one is over, with no reason"
    );
}

/// A player never bound to a lifestone recalls: the whole recall plays (its power-up, its story,
/// its vigor, all of it), then it is told it failed, and the skill's information and the wait every
/// skill shares follow; it goes nowhere.
/// Behaviour: server.places.recall-never-bound
#[test]
fn a_recall_never_bound_plays_and_fails() {
    let (mut h, b, me, _) = beside_a_beetle();
    h.arm(b, me, SWORD);
    let from = use_skill(&mut h, b, 1, LIFESTONE_RECALL, 0);
    h.run(400);
    let msgs = h.world_messages(b, from);
    assert!(
        msgs.iter().any(|m| matches!(m, ServerMessage::DoStory(e) if e.sender.id == me && e.action.skill == LIFESTONE_RECALL)),
        "its story"
    );
    assert_eq!(vigor_sent(&h, b, from).first(), Some(&0), "all its vigor");
    let failed = calls_of(&h, b, from).into_iter().any(|(f, v)| {
        f == SHOW_TEXT
            && v.iter().any(|x| matches!(x.root(), Some(calls::Node::TextInfo(t)) if t.string_id == 0xCF46_E5A6))
    });
    assert!(failed, "told it failed");
    assert!(functions(&h, b, from).contains(&SKILL_INFO));
    assert!(
        !msgs.iter().any(
            |m| matches!(m, ServerMessage::Position(p) if p.sender.id == me && p.teleport_stamp > 0)
        ),
        "it goes nowhere"
    );
}

/// Moving while the recall powers up cuts it short: the use over, interrupted, not attacking;
/// nothing spent (no vigor, no wait).
/// Behaviour: server.places.recall-interrupted
#[test]
fn moving_while_the_recall_powers_up_interrupts_it() {
    let (mut h, b, me, _) = beside_a_beetle();
    h.arm(b, me, SWORD);
    let from = use_skill(&mut h, b, 1, LIFESTONE_RECALL, 0);
    h.run(40);
    h.bots[b].send(
        true,
        queue::WEENIE,
        &yalain_testkit::play::report(
            0x0299_0109,
            auberean_primitives::Vec3::new(58.0, 240.0, -44.6),
            0,
            yalain_world::world::motion(127, 255, 127),
            0,
        ),
    );
    h.run(10);
    let got = calls_of(&h, b, from);
    assert!(
        got.contains(&(
            function::REFUSED,
            vec![
                Value::Int(LIFESTONE_RECALL),
                Value::Long(me),
                Value::Int(refusal::INTERRUPTED)
            ]
        )),
        "interrupted: {got:x?}"
    );
    assert!(vigor_sent(&h, b, from).is_empty(), "no vigor spent");
    assert!(!got.iter().any(|(f, _)| *f == SKILL_INFO), "no wait");
}

/// A recall costs all of the player's vigor: short of it, the recall is refused.
/// Behaviour: server.specials.refused
#[test]
fn a_recall_short_of_full_vigor_is_refused() {
    let (mut h, b, me, _) = beside_a_beetle();
    h.arm(b, me, SWORD);
    for p in h.server.players.values_mut() {
        yalain_world::quals::set_int(&mut p.qualities, VIGOR, 90);
    }
    let from = use_skill(&mut h, b, 1, LIFESTONE_RECALL, 0);
    h.run(10);
    assert!(calls_of(&h, b, from).contains(&(
        function::REFUSED,
        vec![
            Value::Int(LIFESTONE_RECALL),
            Value::Long(0),
            Value::Int(refusal::NOT_ENOUGH_VIGOR)
        ]
    )));
}

/// A creature that attacks is open a moment later (the golden rays on it) for seven and a half
/// seconds; a starting special that hits it then exploits the opening: the player's private
/// effect, the rays stopped, the creature's story of 20 more damage.
/// Behaviour: server.specials.opening-exploited
#[test]
fn a_starting_special_exploits_a_creatures_opening() {
    let (mut h, b, me, beetle) = beside_a_beetle();
    h.arm(b, me, SWORD);
    // The beetle comes and strikes, and is open after.
    assert!(
        h.run_until(600, |h| h.world_messages(b, 0).iter().any(
            |m| matches!(m, ServerMessage::DoFx(e) if e.sender.id == beetle && e.action.fx == 0x4100_0003)
        )),
        "the beetle's opening"
    );
    // Riposte until one lands on the open beetle.
    let mut id = 10;
    let exploited = loop {
        if !h.server.things.get(beetle).is_some_and(|t| t.alive()) || id > 40 {
            break false;
        }
        let from = use_skill(&mut h, b, id, RIPOSTE, beetle);
        h.run(40);
        let msgs = h.world_messages(b, from);
        if msgs
            .iter()
            .any(|m| matches!(m, ServerMessage::PrivateFx(f) if f.0 == vec![0x4100_051A]))
        {
            assert!(msgs.iter().any(|m| matches!(m, ServerMessage::StopFx(e) if e.sender.id == beetle && e.action.0 == 0x4100_0003)));
            let exploit = msgs
                .iter()
                .find_map(|m| match m {
                    ServerMessage::DoStory(e) if e.sender.id == beetle && e.action.bits == 0x2 => {
                        Some(e.action.hooks[0].clone())
                    }
                    _ => None,
                })
                .expect("the creature's story of it");
            assert_eq!((exploit.bits, exploit.numbers[3]), (0x981, -20));
            break true;
        }
        id += 1;
    };
    assert!(exploited, "a hit on the open beetle exploits it");
}

/// An empty-handed player's attack is refused: attacking, its basic attacks failed (not wielding
/// the right item), not attacking; no swing.
/// Behaviour: server.combat.empty-hand-refused
#[test]
fn an_attack_with_an_empty_hand_is_refused() {
    let (mut h, b, me, beetle) = beside_a_beetle();
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(START_ATTACK, &[Arg::LongInt(beetle)]),
    );
    h.run(40);
    let got = calls_of(&h, b, from);
    assert_eq!(
        got[..3],
        [
            (ATTACK_STATE, vec![Value::Int(1)]),
            (0x8204_001C, vec![Value::Int(30_026)]),
            (ATTACK_STATE, vec![Value::Int(0)]),
        ]
    );
    assert!(!h
        .world_messages(b, from)
        .iter()
        .any(|m| matches!(m, ServerMessage::DoStory(e) if e.sender.id == me)));
}

/// A recall spends all of the player's vigor, and the vigor comes back at rest, from nothing (the
/// beetle beside it never fights, so nothing hurts the player).
/// Behaviour: server.regen.vigor-from-nothing
#[test]
fn vigor_spent_to_nothing_by_a_recall_comes_back_at_rest() {
    let (mut h, b, me, beetle) = beside_a_beetle();
    practice_target(&mut h, beetle);
    h.arm(b, me, SWORD);
    let from = use_skill(&mut h, b, 1, LIFESTONE_RECALL, 0);
    h.run(400);
    assert_eq!(vigor_sent(&h, b, from).first(), Some(&0), "all of it spent");
    let rest = h.bots[b].inbox.len();
    h.run(300);
    let back = vigor_sent(&h, b, rest);
    assert!(
        back.last().is_some_and(|v| *v > 0),
        "vigor back at rest: {back:?}"
    );
}

/// Wielding names the item's place in the pack as the client keeps it (the pack's order, closed
/// up behind anything taken out), and whatever was in hand goes back where it was first.
/// Behaviour: server.inventory.wield-keeps-the-packs-order
#[test]
fn wielding_names_the_items_place_and_the_pack_closes_up() {
    let (mut h, b, me, _) = beside_a_beetle();
    let sword = h.arm(b, me, SWORD);
    let from = h.bots[b].inbox.len();
    let bow = h.arm(b, me, BOW);
    let first_call = h.world_messages(b, from).into_iter().find_map(|m| match m {
        ServerMessage::PlayerCall(c) => Some(c.function.0),
        _ => None,
    });
    // The sword goes back first (its answer), then the bow is wielded.
    assert_eq!(first_call, Some(0x820E_0056), "the sword put away first");
    let p = h.server.players.values().next().expect("the player");
    let held = |id: u64| {
        p.play
            .items
            .iter()
            .find(|x| x.id == id)
            .map(|x| (x.slot, x.location))
    };
    assert_eq!(held(sword).map(|x| x.1), Some(0), "the sword in the pack");
    assert_ne!(held(bow).map(|x| x.1), Some(0), "the bow in hand");
    assert_eq!(
        held(sword).map(|x| x.0),
        Some(0),
        "the pack closed up: the sword first"
    );
}

/// Move the beetle `d` meters east of the player.
fn beetle_east(h: &mut Harness, me: u64, beetle: u64, d: f32) {
    let at = h
        .server
        .players
        .values()
        .find(|p| p.character == me)
        .map(|p| p.position)
        .expect("in the world");
    if let Some(t) = h.server.things.get_mut(beetle) {
        let mut place = at;
        place.origin[0] += d;
        t.at = Some(place);
    }
}

/// Moving after Deadeye's story cuts it short, and it counts as used: the skill's information
/// first, the use over, why (interrupted, naming its target), not attacking, its behavior stopped
/// on the player and the bow, the player stopped (it was turned), at peace. The missile already
/// thrown still arrives.
/// Behaviour: server.specials.cut-short
#[test]
fn moving_after_a_specials_story_cuts_it_short_and_it_counts_as_used() {
    let (mut h, b, me, beetle) = beside_a_beetle();
    practice_target(&mut h, beetle);
    let bow = h.arm(b, me, BOW);
    let from = use_skill(&mut h, b, 7, DEADEYE, beetle);
    h.run(6);
    assert!(
        h.world_messages(b, from)
            .iter()
            .any(|m| matches!(m, ServerMessage::DoStory(e) if e.sender.id == me)),
        "the story told"
    );
    let cut = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &yalain_testkit::play::report(
            0x0299_0109,
            auberean_primitives::Vec3::new(58.0, 240.0, -44.6),
            0,
            yalain_world::world::motion(127, 255, 127),
            0,
        ),
    );
    h.run(4);
    let got = functions(&h, b, cut);
    let at = |f: u32| got.iter().position(|x| *x == f);
    assert!(at(SKILL_INFO) < at(function::ENDED), "{got:x?}");
    assert!(calls_of(&h, b, cut).contains(&(
        function::REFUSED,
        vec![
            Value::Int(DEADEYE),
            Value::Long(beetle),
            Value::Int(refusal::INTERRUPTED)
        ]
    )));
    let msgs = h.world_messages(b, cut);
    for who in [me, bow] {
        assert!(msgs.iter().any(|m| matches!(m, ServerMessage::StopBehavior(e) if e.sender.id == who && e.action.0 == 0x4100_007A)), "{who:x}");
    }
    assert!(msgs.iter().any(|m| matches!(m, ServerMessage::MoveTo(e) if e.sender.id == me && e.action.params.flags == 0x40)));
    assert!(msgs.iter().any(
        |m| matches!(m, ServerMessage::DoMode(e) if e.sender.id == me && e.action.0 == 0x4000_0001)
    ));
    h.run(60);
    assert!(
        h.world_messages(b, cut)
            .iter()
            .any(|m| matches!(m, ServerMessage::DoStory(e) if e.sender.id == beetle && e.action.bits == 0x12)),
        "the missile still arrives"
    );
}

/// Whether a use of Deadeye was cut short (interrupted) in the messages since `from`.
fn interrupted(h: &Harness, b: usize, from: usize) -> bool {
    calls_of(h, b, from).iter().any(|(f, v)| {
        *f == function::REFUSED
            && v.first() == Some(&Value::Int(DEADEYE))
            && v.last() == Some(&Value::Int(refusal::INTERRUPTED))
    })
}

/// Using an item does not cut Deadeye short, before its story or after; a transmute does, before
/// its story or after.
/// Behaviour: server.specials.cut-by-a-transmute
#[test]
fn a_transmute_cuts_a_special_short_and_an_item_used_does_not() {
    let (mut h, b, me, beetle) = beside_a_beetle();
    practice_target(&mut h, beetle);
    h.arm(b, me, BOW);
    let item_used = |h: &mut Harness| {
        h.bots[b].send(
            true,
            queue::WEENIE,
            &server_call(
                yalain_world::requests::function::USE,
                &[yalain_testkit::play::use_of(0x8130_0000_0000_0042, me)],
            ),
        );
    };
    let transmute = |h: &mut Harness| {
        h.bots[b].send(true, queue::WEENIE, &server_call(0x820E_0065, &[]));
    };
    let told = |h: &Harness, from: usize| {
        h.world_messages(b, from)
            .iter()
            .any(|m| matches!(m, ServerMessage::DoStory(e) if e.sender.id == me))
    };
    // An item used, before the story and after: the use goes on to its end.
    for (n, wait) in [(7, 0), (8, 6)] {
        let from = use_skill(&mut h, b, n, DEADEYE, beetle);
        h.run(wait);
        assert_eq!(told(&h, from), wait > 0);
        item_used(&mut h);
        h.run(100);
        assert!(!interrupted(&h, b, from), "an item used cuts nothing");
        assert!(
            functions(&h, b, from).contains(&SKILL_INFO),
            "it went on to its end"
        );
    }
    // A transmute, before the story and after: cut short.
    for (n, wait) in [(9, 0), (10, 6)] {
        use_skill(&mut h, b, n, DEADEYE, beetle);
        h.run(wait);
        let cut = h.bots[b].inbox.len();
        transmute(&mut h);
        h.run(4);
        assert!(interrupted(&h, b, cut), "a transmute cuts it short");
        h.run(100);
    }
}

/// Stopping attacks on a special's target cuts it short; stopping attacks on another does not.
/// Behaviour: server.specials.cut-short
#[test]
fn stopping_attacks_on_its_target_cuts_a_special_aimed_at_it() {
    let (mut h, b, me, beetle) = beside_a_beetle();
    practice_target(&mut h, beetle);
    h.arm(b, me, BOW);
    for (stopped, cut) in [(beetle + 1, false), (beetle, true)] {
        let from = use_skill(&mut h, b, 8, DEADEYE, beetle);
        h.bots[b].send(
            true,
            queue::WEENIE,
            &server_call(0x8204_0020, &[Arg::LongInt(stopped)]),
        );
        h.run(4);
        let refused = calls_of(&h, b, from).iter().any(|(f, v)| {
            *f == function::REFUSED && v.last() == Some(&Value::Int(refusal::INTERRUPTED))
        });
        assert_eq!(refused, cut, "stopping {stopped:x}");
        h.run(100);
    }
}

/// A skill's range is measured between the bodies (each a cylinder of its look's radius): Deadeye,
/// from 2 m, is too close with 3 m between the centres (less than 2 m between the bodies) and
/// used with 4.2 m.
/// Behaviour: server.specials.range-between-bodies
#[test]
fn a_skills_range_is_measured_between_the_bodies() {
    let (mut h, b, me, beetle) = beside_a_beetle();
    practice_target(&mut h, beetle);
    h.arm(b, me, BOW);
    beetle_east(&mut h, me, beetle, 3.0);
    let from = use_skill(&mut h, b, 9, DEADEYE, beetle);
    h.run(6);
    assert!(calls_of(&h, b, from).contains(&(
        function::REFUSED,
        vec![
            Value::Int(DEADEYE),
            Value::Long(beetle),
            Value::Int(refusal::TOO_CLOSE)
        ]
    )));
    beetle_east(&mut h, me, beetle, 4.2);
    let from = use_skill(&mut h, b, 10, DEADEYE, beetle);
    h.run(6);
    assert!(!functions(&h, b, from).contains(&function::REFUSED));
}
