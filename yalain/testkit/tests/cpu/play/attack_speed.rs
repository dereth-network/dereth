//! A player's attack speed: what its effects make of the pace of its swings, shots and specials,
//! and of when they land and when the next one comes. Fixture: made-up effects of the size and
//! class of the install's, on a test character beside the test camp's beetle.

use auberean_protocol::types::PhysicsStory;
use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_testkit::play::{server_call, use_of};
use yalain_testkit::{Harness, TEST_CREATURE};
use yalain_world::combat::{self, hit_seconds, next_swing_after, swing_seconds};
use yalain_world::effect_defs::{numeric, EffectDef, Kind, Scale};
use yalain_world::effects::{self, Cast, On, ATTACK_SPEED};
use yalain_world::player::quality;
use yalain_world::quals;
use yalain_world::skill_use::implement as im;

const START_ATTACK: u32 = 0x8204_0008;
/// The human starting sword.
const STARTER_SWORD: u32 = 0x4700_051C;
/// The class of the effects that quicken attacks, and one of no competition with it.
const QUICKENING: u32 = 27;
const OTHER_CLASS: u32 = 4129;
const FIRST_STATE: u32 = 0x6F00_7500;

/// Where a swing's time scale falls at a pace: a tenth about the time scale times it.
fn band(pace: f32) -> std::ops::RangeInclusive<f32> {
    let at = combat::time_scale_at_delay(0) * pace;
    (at * 0.9 - 1e-3)..=(at * 1.1 + 1e-3)
}

/// A record on the player of a made-up attack-speed effect, multiplying or adding.
pub(super) fn speeded(h: &mut Harness, class: u32, value: f32, multiplies: bool) {
    let state = FIRST_STATE + u32::try_from(h.server.effect_table.effects.len()).unwrap_or(0);
    let flags = if multiplies {
        numeric::MULTIPLY
    } else {
        numeric::ADD
    };
    let def = EffectDef {
        state,
        kind: Kind::Numeric {
            flags,
            stats: vec![ATTACK_SPEED],
            magnitude: Scale(vec![(0.0, value)]),
        },
        duration: Scale(vec![(0.0, 100_000.0)]),
        priority: Scale(vec![(0.0, 1.0)]),
        fx: 0,
        external_flags: 0,
        internal_flags: 0x1100_0003,
        enum_value: 0,
        eq_class: class,
        name: None,
    };
    h.server.effect_table.effects.insert(state, def);
    let world = *h.server.players.keys().next().expect("in the world");
    let clock = h.clock();
    assert!(effects::apply_on(
        &mut h.server,
        &mut h.net,
        On::Player(world),
        state,
        Cast {
            spellcraft: 50.0,
            ..Cast::default()
        },
        clock,
    ));
}

/// The pace the player's effects give.
fn pace(h: &Harness) -> f32 {
    let world = *h.server.players.keys().next().expect("in the world");
    combat::player_pace(&h.server, world)
}

/// A player with a sword, the beetle in reach, standing still; both full of health.
fn with_a_sword() -> (Harness, usize, u64, u64) {
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
    h.arm(b, me, STARTER_SWORD);
    // It outlasts what the beetle does back.
    for p in h.server.players.values_mut() {
        quals::set_int(&mut p.qualities, quality::HEALTH_MAX, 10_000_000);
        quals::set_int(&mut p.qualities, quality::HEALTH, 10_000_000);
    }
    let at = h
        .server
        .players
        .values()
        .find(|p| p.character == me)
        .map(|p| p.position)
        .expect("in the world");
    if let Some(t) = h.server.things.get_mut(beetle) {
        if let Some(c) = t.creature.as_mut() {
            c.stats.movement.perception = 0.0;
            c.stats.scale = 0.0;
        }
        let mut place = at;
        place.origin[0] += 1.5;
        t.at = Some(place);
        t.max_health = 10_000_000;
        t.health = 10_000_000;
    }
    (h, b, me, beetle)
}

/// The player's swing stories since `from`.
fn stories(h: &Harness, b: usize, from: usize, me: u64) -> Vec<PhysicsStory> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::DoStory(e) if e.action.attacker == me && e.sender.id == me => {
                Some(e.action.clone())
            }
            _ => None,
        })
        .collect()
}

/// Each story of a run as it is told, for `steps` of the world: its time and time scale.
fn swings(h: &mut Harness, b: usize, me: u64, beetle: u64, steps: u32) -> Vec<(f64, f32)> {
    // A story of the last run still on its way is not of this one.
    h.run(10);
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(START_ATTACK, &[Arg::LongInt(beetle)]),
    );
    let mut at = Vec::new();
    for _ in 0..steps {
        h.step();
        let told = stories(h, b, from, me);
        if told.len() > at.len() {
            let ts = told.last().expect("told").attacker_behavior.time_scale;
            at.push((h.now.0, ts));
        }
    }
    at
}

/// Every swing of a run is told in its band, and the next comes when it is due.
fn assert_paced(at: &[(f64, f32)], pace: f32) {
    assert!(at.len() > 10, "{} swings", at.len());
    for (t, ts) in at {
        assert!(band(pace).contains(ts), "{ts} at {t} for a pace of {pace}");
    }
    for w in at.windows(2) {
        let gap = w[1].0 - w[0].0;
        let due = next_swing_after(swing_seconds(Some(im::SWORD), false), w[0].1);
        assert!(
            (gap - due).abs() <= 0.052,
            "{gap:.3} where {due:.3} was due"
        );
    }
}

/// A swing's time scale goes with the top attack speed effect of its class, and its next swing
/// comes that much sooner: a third on the plain, then the stronger of two of a class, whichever
/// else it holds. The gap keeps a pause the time scale does not touch.
/// Behaviour: server.weapons.attack-speed-effects
#[test]
fn a_players_attack_speed_effects_quicken_its_swings() {
    let (mut h, b, me, beetle) = with_a_sword();
    assert_eq!(pace(&h), 1.0);
    assert_paced(&swings(&mut h, b, me, beetle, 1200), 1.0);
    speeded(&mut h, QUICKENING, 1.3, true);
    assert!((pace(&h) - 1.3).abs() < 1e-4);
    assert_paced(&swings(&mut h, b, me, beetle, 1200), 1.3);
    speeded(&mut h, QUICKENING, 1.7, true);
    assert!((pace(&h) - 1.7).abs() < 1e-4, "not the two of them");
    assert_paced(&swings(&mut h, b, me, beetle, 1200), 1.7);
}

/// A faster swing is about a third sooner at 1.3 and not quite half at 1.7: the pause after it
/// doesn't shorten.
/// Behaviour: server.weapons.attack-speed-effects
#[test]
fn the_pause_after_a_swing_does_not_shorten_with_its_pace() {
    let at = |ts: f32| next_swing_after(1.64, ts);
    assert!((at(1.05) - 1.64 / 1.05).abs() < 1e-3);
    // 2017's gaps at these time scales were 1.17 s, 0.95 s and 0.74 s.
    assert!((at(1.43) - 1.179).abs() < 1e-3);
    assert!((at(1.80) - 0.962).abs() < 1e-3);
    assert!((at(2.39) - 0.755).abs() < 1e-3);
}

/// A strike lands that much sooner too, by its time scale.
/// Behaviour: server.weapons.attack-speed-effects
#[test]
fn a_faster_swing_lands_sooner() {
    let (mut h, b, me, beetle) = with_a_sword();
    speeded(&mut h, QUICKENING, 1.7, true);
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(START_ATTACK, &[Arg::LongInt(beetle)]),
    );
    let hits = |h: &Harness| {
        stories(h, b, from, me).into_iter().find(|s| {
            s.hooks
                .iter()
                .any(|k| k.target == beetle && k.numbers[3] < 0)
        })
    };
    assert!(h.run_until(4000, |h| hits(h).is_some()), "a hit told");
    let told = h.now.0;
    let s = hits(&h).expect("the hit");
    let health = |h: &Harness| h.server.things.get(beetle).map_or(0, |t| t.health);
    let full = 10_000_000;
    assert_eq!(health(&h), full, "not hurt as it is told");
    assert!(h.run_until(400, |h| health(h) < full), "it lands");
    let lands = hit_seconds(s.skill, Some(im::SWORD)) / f64::from(s.attacker_behavior.time_scale);
    let took = h.now.0 - told;
    assert!((took - lands).abs() < 0.12, "{took:.3}, not {lands:.3}");
    assert!(lands < 0.5, "{lands:.3}");
}

/// What adds to attack speed counts on top of what multiplies it, in another class; a hold brings
/// the pace down to a fifth.
/// Behaviour: server.weapons.attack-speed-effects
#[test]
fn an_added_attack_speed_counts_on_top_and_a_hold_gives_a_fifth() {
    let (mut h, _, _, _) = with_a_sword();
    speeded(&mut h, QUICKENING, 1.3, true);
    speeded(&mut h, OTHER_CLASS, 0.3, false);
    assert!((pace(&h) - 1.69).abs() < 1e-4, "{}", pace(&h));
    speeded(&mut h, 55, 0.0, true);
    assert!((pace(&h) - 0.2).abs() < 1e-4, "{}", pace(&h));
}

/// An effect slowing a player's attacks slows them as it quickens them, its whole size. Our
/// choice: no recording holds a swing of a slowed player.
/// Behaviour: server.weapons.attack-speed-slows
#[test]
fn an_effect_slowing_a_players_attacks_slows_its_swings() {
    let (mut h, b, me, beetle) = with_a_sword();
    speeded(&mut h, 53, 0.5, true);
    assert_paced(&swings(&mut h, b, me, beetle, 2400), 0.5);
}

/// An item of a template, with a combat delay of its own or none, put on by its use.
fn put_on(h: &mut Harness, b: usize, me: u64, template: u32, own: Option<i32>) -> u64 {
    let item = h.arm(b, me, template);
    if own.is_some() {
        let t = h.server.things.get_mut(item).expect("made");
        t.own.made = Some(yalain_world::made::Made {
            delay: own,
            ..Default::default()
        });
    }
    item
}

/// Take off what is worn, by its use.
fn use_it_off(h: &mut Harness, b: usize, me: u64, item: u64) {
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(yalain_world::requests::function::USE, &[use_of(item, me)]),
    );
    h.run(10);
}

/// The mean time scale of a run's swings, and that each falls within a tenth of what was asked.
fn asserted_base(at: &[(f64, f32)], base: f32) -> f32 {
    assert!(at.len() > 10, "{} swings", at.len());
    for (t, ts) in at {
        let near = (base * 0.9 - 1e-3)..=(base * 1.1 + 1e-3);
        assert!(near.contains(ts), "{ts} at {t} about {base}");
    }
    #[allow(clippy::cast_precision_loss)]
    let n = at.len() as f32;
    at.iter().map(|x| x.1).sum::<f32>() / n
}

/// What a player wears and wields, by each item's combat delay, its own number or else its
/// template's, sets where its swings' time scale falls, and so when the next comes; what is in its
/// pack does not; taking it off brings it back.
/// Behaviour: server.weapons.time-scale-follows-delay
#[test]
fn a_players_swings_slow_with_the_combat_delay_it_wears_and_wields() {
    const COAT: u32 = 0x4700_75A0;
    const HOOD: u32 = 0x4700_75A1;
    const SPARE: u32 = 0x4700_75A2;
    let (mut h, b, me, beetle) = with_a_sword();
    let world = *h.server.players.keys().next().expect("in the world");
    let piece = |h: &mut Harness, t: u32, place: u32, delay: i64| {
        let mut p = yalain_content::Props::default();
        p.set("Item_ArmorLevel", yalain_content::Prop::Int(10))
            .set("Item_CombatDelay", yalain_content::Prop::Int(delay))
            .set(
                "Item_ValidInventoryLocations",
                yalain_content::Prop::Word(place),
            )
            .set(
                "Item_PreferredInventoryLocation",
                yalain_content::Prop::Word(place),
            );
        h.server
            .catalog
            .insert(yalain_testkit::test_template(t, 0x180, p));
    };
    piece(&mut h, COAT, 2, 500);
    piece(&mut h, HOOD, 1, 62);
    piece(&mut h, SPARE, 4, 500);
    let plain = swings(&mut h, b, me, beetle, 600);
    let bare = asserted_base(&plain, 1.13);
    assert_eq!(combat::delay_worn(&h.server, world), 0);

    // The coat's own 8 and not its template's 500; the hood's template's 62.
    put_on(&mut h, b, me, COAT, Some(8));
    let hood = put_on(&mut h, b, me, HOOD, None);
    assert_eq!(combat::delay_worn(&h.server, world), 70);
    let spare = h.arm(b, me, SPARE);
    use_it_off(&mut h, b, me, spare);
    assert_eq!(combat::delay_worn(&h.server, world), 70, "in the pack");
    let dressed = swings(&mut h, b, me, beetle, 600);
    let at = 1.23 - 0.0024 * 70.0;
    let mean = asserted_base(&dressed, at);
    assert!(mean < bare - 0.03, "{mean} against {bare}");
    let gaps: Vec<f64> = dressed.windows(2).map(|w| w[1].0 - w[0].0).collect();
    #[allow(clippy::cast_precision_loss)]
    let gap = gaps.iter().sum::<f64>() / gaps.len() as f64;
    let due = next_swing_after(swing_seconds(Some(im::SWORD), false), mean);
    assert!((gap - due).abs() < 0.03, "{gap:.3} where {due:.3} was due");

    use_it_off(&mut h, b, me, hood);
    assert_eq!(combat::delay_worn(&h.server, world), 8);
}

/// Where the delays recorded end the time scale stays: nothing quicker than the lightest seen,
/// nothing slower than the heaviest.
/// Behaviour: server.weapons.time-scale-bounds-of-delay
#[test]
fn the_time_scale_of_delay_stays_within_what_was_seen() {
    assert!((combat::time_scale_at_delay(70) - 1.062).abs() < 1e-3);
    assert!((combat::time_scale_at_delay(50) - 1.11).abs() < 1e-3);
    for bare in [-5, 0, 20, 40] {
        assert!((combat::time_scale_at_delay(bare) - 1.13).abs() < 1e-6);
    }
    for heavy in [96, 200, 100_000] {
        assert!((combat::time_scale_at_delay(heavy) - 1.0).abs() < 1e-6);
    }
}

/// The basic attacks told of the run, for `steps` of the world.
fn attacks_told(h: &mut Harness, b: usize, me: u64, beetle: u64, steps: u32) -> Vec<u32> {
    h.run(10);
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(START_ATTACK, &[Arg::LongInt(beetle)]),
    );
    for _ in 0..steps {
        h.step();
    }
    stories(h, b, from, me).iter().map(|s| s.skill).collect()
}

/// The next made-up state, which [`speeded`] then puts on the player.
fn next_state(h: &Harness) -> u32 {
    FIRST_STATE + u32::try_from(h.server.effect_table.effects.len()).unwrap_or(0)
}

/// Each basic attack of the test's sword by its id: an even one wants the effect, an odd one is barred by it.
fn split_by(h: &mut Harness, state: u32) {
    for d in h.server.skill_table.skills.values_mut() {
        let id = d.id;
        if !d.auto_attack {
            continue;
        }
        let u = d.usage.as_mut().expect("used");
        if id % 2 == 0 {
            u.required_effects = vec![state];
        } else {
            u.barring_effects = vec![state];
        }
    }
}

/// A run swings only the basic attacks its player's effects allow: the plain ones without the
/// effect that bars them, the others with the effect they require, as an alchemist's turn fiery.
/// Behaviour: server.weapons.basic-attacks-go-by-their-effects
#[test]
fn a_run_swings_only_what_its_players_effects_allow() {
    let (mut h, b, me, beetle) = with_a_sword();
    let state = next_state(&h);
    split_by(&mut h, state);
    let plain = attacks_told(&mut h, b, me, beetle, 600);
    assert!(plain.len() > 5, "{plain:?}");
    assert!(plain.iter().all(|k| k % 2 == 1), "{plain:?}");
    speeded(&mut h, OTHER_CLASS, 1.0, true);
    let fiery = attacks_told(&mut h, b, me, beetle, 600);
    assert!(fiery.len() > 5, "{fiery:?}");
    assert!(fiery.iter().all(|k| k % 2 == 0), "{fiery:?}");
}

/// A basic attack that requires two effects wants both, however many of them are carried.
/// Behaviour: server.weapons.basic-attacks-go-by-their-effects
#[test]
fn a_basic_attack_wanting_two_effects_wants_both() {
    let (mut h, b, me, beetle) = with_a_sword();
    let first = next_state(&h);
    let second = first + 1;
    for d in h.server.skill_table.skills.values_mut() {
        if d.auto_attack {
            let u = d.usage.as_mut().expect("used");
            u.required_effects = vec![first, second];
        }
    }
    speeded(&mut h, OTHER_CLASS, 1.0, true);
    assert_eq!(attacks_told(&mut h, b, me, beetle, 200), Vec::<u32>::new());
    speeded(&mut h, OTHER_CLASS + 1, 1.0, true);
    assert!(!attacks_told(&mut h, b, me, beetle, 200).is_empty());
}

/// With every basic attack of its weapon withheld, a run tells nothing and does not walk; when the
/// effect comes, it swings.
/// Behaviour: server.weapons.withheld-run-waits
#[test]
fn a_run_with_nothing_it_may_swing_waits() {
    let (mut h, b, me, beetle) = with_a_sword();
    let state = next_state(&h);
    for d in h.server.skill_table.skills.values_mut() {
        if d.auto_attack {
            d.usage.as_mut().expect("used").required_effects = vec![state];
        }
    }
    h.run(10);
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(START_ATTACK, &[Arg::LongInt(beetle)]),
    );
    h.run(200);
    assert!(stories(&h, b, from, me).is_empty());
    assert!(h
        .world_messages(b, from)
        .iter()
        .all(|m| !matches!(m, ServerMessage::MoveTo(_))));
    speeded(&mut h, OTHER_CLASS, 1.0, true);
    h.run(100);
    assert!(!stories(&h, b, from, me).is_empty(), "with the effect on");
}

/// A basic attack that names skills to be trained is not swung until they are, and rated as named.
/// Behaviour: server.weapons.basic-attacks-want-their-skills
#[test]
fn a_basic_attack_waits_for_the_skills_it_names() {
    use yalain_world::state::SkillState;
    const LATER: u32 = 991;
    let (mut h, b, me, beetle) = with_a_sword();
    h.server.skill_table.skills.insert(
        LATER,
        yalain_world::skills::SkillDef {
            id: LATER,
            cap: 100,
            ..Default::default()
        },
    );
    for d in h.server.skill_table.skills.values_mut() {
        if d.auto_attack && d.id % 2 == 0 {
            d.usage.as_mut().expect("used").use_prereqs = vec![(LATER, 3)];
        }
    }
    let plain = attacks_told(&mut h, b, me, beetle, 600);
    assert!(plain.iter().all(|k| k % 2 == 1), "{plain:?}");
    let train = |h: &mut Harness, granted: u32| {
        let p = h.server.players.values_mut().next().expect("in the world");
        p.play.skills.retain(|s| s.skill != LATER);
        p.play.skills.push(SkillState {
            skill: LATER,
            granted,
            mask: 1,
            cost: 0,
            children: 0,
            dependents: 0,
            spent: 0,
        });
    };
    train(&mut h, 2);
    let low = attacks_told(&mut h, b, me, beetle, 600);
    assert!(low.iter().all(|k| k % 2 == 1), "{low:?}");
    train(&mut h, 3);
    let told = attacks_told(&mut h, b, me, beetle, 1200);
    assert!(told.iter().any(|k| k % 2 == 0), "{told:?}");
}
