//! A player's vitals coming back: one timer for health and vigor, each step a fresh 3.5 to 7.5 s
//! after the last, health filling in a minute and vigor in half of one at rest; a player that
//! fought or was hurt since the last step, or is moving at it, regaining only at its combat rates,
//! which only its effects give it; each step a share of the greatest with its effects. Fixture: a
//! player alone in the test world, its vitals set low by hand, Robustify and a worn item's bonus
//! from the test effects.

use auberean_protocol::ServerMessage;
use yalain_net::ConnId;
use yalain_testkit::{Harness, COURAGE_EFFECT, ROBUSTIFY_EFFECT, WORN_HEALTH_RATE_EFFECT};
use yalain_world::player::quality;
use yalain_world::regen::{gain, rate, HEALTH_FILLS_IN, PLAYER_STEP};
use yalain_world::world::IDLE_MOTION;
use yalain_world::{effects, quals};

/// A player in the world with health 100 of 600 and vigor 50 of 300.
fn hurt_player() -> (Harness, usize, ConnId) {
    let mut h = Harness::new();
    let (b, _) = h.player_in_world("one", "Elara");
    h.run(10);
    let world = *h.server.players.keys().next().expect("in the world");
    let p = h.server.players.get_mut(&world).expect("in the world");
    for (k, v) in [
        (quality::HEALTH_MAX, 600),
        (quality::HEALTH, 100),
        (quality::VIGOR_MAX, 300),
        (quality::VIGOR, 50),
    ] {
        quals::set_int(&mut p.qualities, k, v);
    }
    (h, b, world)
}

/// Run for `seconds`, noting each health and vigor value sent with the clock when it came, and
/// calling `each` before every step.
fn watch(
    h: &mut Harness,
    b: usize,
    seconds: f64,
    mut each: impl FnMut(&mut Harness),
) -> Vec<(f64, u32, i32)> {
    let mut seen = h.bots[b].inbox.len();
    let mut out = Vec::new();
    let end = h.now.0 + seconds;
    while h.now.0 < end {
        each(h);
        h.step();
        for m in h.world_messages(b, seen) {
            if let ServerMessage::UpdateInt(q) = m {
                if q.quality == quality::HEALTH || q.quality == quality::VIGOR {
                    out.push((h.now.0, q.quality, q.value));
                }
            }
        }
        seen = h.bots[b].inbox.len();
    }
    out
}

/// At rest, health and vigor come back together, health first, in steps 3.5 to 7.5 s apart and
/// not evenly spaced; each step gives the share of the greatest the time since the last earns
/// (health its greatest in 60 s, vigor in 30 s), rounded down, until each is full.
/// Behaviour: server.regen.steps-at-rest
#[test]
fn at_rest_health_and_vigor_come_back_together_in_steps_of_three_and_a_half_to_seven_and_a_half_seconds(
) {
    let (mut h, b, _world) = hurt_player();
    let got = watch(&mut h, b, 90.0, |_| {});
    let health: Vec<(f64, i32)> = got
        .iter()
        .filter(|(_, q, _)| *q == quality::HEALTH)
        .map(|(t, _, v)| (*t, *v))
        .collect();
    let vigor: Vec<(f64, i32)> = got
        .iter()
        .filter(|(_, q, _)| *q == quality::VIGOR)
        .map(|(t, _, v)| (*t, *v))
        .collect();
    assert!(health.len() >= 6, "{health:?}");
    assert_eq!(health.last().map(|x| x.1), Some(600), "health full");
    assert_eq!(vigor.last().map(|x| x.1), Some(300), "vigor full");
    // Each step's health and vigor in one instant, health sent first, while vigor still rises.
    for (t, v) in &vigor {
        let i = got
            .iter()
            .position(|(tt, q, vv)| tt == t && *q == quality::VIGOR && vv == v)
            .expect("seen");
        if i > 0 && got[i - 1].0 == *t {
            assert_eq!(got[i - 1].1, quality::HEALTH, "health before vigor");
        }
    }
    let gaps: Vec<f64> = health.windows(2).map(|w| w[1].0 - w[0].0).collect();
    for g in &gaps {
        let (lo, hi) = (f64::from(PLAYER_STEP.0), f64::from(PLAYER_STEP.1));
        assert!(
            *g >= lo - 0.3 && *g <= hi + 0.3,
            "a step {g} s after the last"
        );
    }
    let spread = gaps.iter().copied().fold(f64::MIN, f64::max)
        - gaps.iter().copied().fold(f64::MAX, f64::min);
    assert!(spread > 0.5, "the steps are not evenly spaced: {gaps:?}");
    // A step's gain is rounded down: 5.99 s earn 59.9 health of 600, and give 59.
    assert_eq!(gain(600, 1.0, 5.99, HEALTH_FILLS_IN), 59);
    // Below the top, a step gives what its time earns: 10 health a second, rounded down.
    for w in health.windows(2) {
        if w[1].1 < 600 {
            let gained = f64::from(w[1].1 - w[0].1);
            let earned = 600.0 * (w[1].0 - w[0].0) / 60.0;
            assert!(
                gained <= earned + 3.0 && gained >= earned - 3.0,
                "{gained} for {earned}"
            );
        }
    }
}

/// Put an effect on the player at a rating.
fn give(h: &mut Harness, world: ConnId, state: u32, spellcraft: f32) {
    let caster = h.server.players[&world].character;
    let cast = effects::Cast {
        caster,
        spellcraft,
        ..effects::Cast::default()
    };
    let clock = h.clock();
    assert!(effects::apply(
        &mut h.server,
        &mut h.net,
        world,
        state,
        cast,
        clock
    ));
}

/// Keep the player moving (or still) at every step.
fn walk(world: ConnId, moving: bool) -> impl FnMut(&mut Harness) {
    move |h: &mut Harness| {
        if let Some(p) = h.server.players.get_mut(&world) {
            p.heading_motion = if moving { 0x0000_0001 } else { IDLE_MOTION };
        }
    }
}

/// The health values among what was seen.
fn health(got: &[(f64, u32, i32)]) -> Vec<(f64, i32)> {
    got.iter()
        .filter(|(_, q, _)| *q == quality::HEALTH)
        .map(|(t, _, v)| (*t, *v))
        .collect()
}

/// Each health step gave what `rate` of `greatest` earns over its time, rounded down (within the
/// step's own rounding).
fn steps_at(health: &[(f64, i32)], greatest: f64, rate: f64) {
    assert!(health.len() >= 3, "{health:?}");
    for w in health.windows(2) {
        if f64::from(w[1].1) >= greatest {
            continue;
        }
        let gained = f64::from(w[1].1 - w[0].1);
        let earned = rate * greatest * (w[1].0 - w[0].0) / 60.0;
        assert!((gained - earned).abs() <= 1.5, "{gained} for {earned}");
    }
}

/// The combat rates are what the player's effects give it and nothing of its own: moving with no
/// such effect, it regains nothing; with Robustify at 50 (a combat health rate of 0.08 and the
/// peace health rate doubled) it regains 0.08 of its rest's health in combat and twice it at rest;
/// a worn item's 0.08 adds to the buff's; no effect gives it a combat vigor rate.
/// Behaviour: server.regen.combat-rates-from-effects
#[test]
fn the_combat_rates_are_what_the_players_effects_give_it_and_nothing_of_its_own() {
    let (mut h, b, world) = hurt_player();
    let got = watch(&mut h, b, 30.0, walk(world, true));
    assert!(got.is_empty(), "no combat rates, nothing back: {got:?}");

    let (mut h, b, world) = hurt_player();
    give(&mut h, world, ROBUSTIFY_EFFECT, 50.0);
    let got = watch(&mut h, b, 60.0, walk(world, true));
    steps_at(&health(&got), 600.0, 0.08);
    assert!(
        !got.iter().any(|(_, q, _)| *q == quality::VIGOR),
        "no combat vigor rate"
    );

    let (mut h, b, world) = hurt_player();
    give(&mut h, world, ROBUSTIFY_EFFECT, 50.0);
    give(&mut h, world, WORN_HEALTH_RATE_EFFECT, 1.0);
    let got = watch(&mut h, b, 60.0, walk(world, true));
    steps_at(&health(&got), 600.0, 0.16);

    let (mut h, b, world) = hurt_player();
    give(&mut h, world, ROBUSTIFY_EFFECT, 50.0);
    let got = watch(&mut h, b, 30.0, walk(world, false));
    steps_at(&health(&got), 600.0, 2.0);

    // A rate the player holds itself still counts, with the effects on top.
    let (mut h, b, world) = hurt_player();
    {
        let p = h.server.players.get_mut(&world).expect("in the world");
        quals::set_float(&mut p.qualities, rate::COMBAT_HEALTH, 0.5);
    }
    give(&mut h, world, WORN_HEALTH_RATE_EFFECT, 1.0);
    let got = watch(&mut h, b, 40.0, walk(world, true));
    steps_at(&health(&got), 600.0, 0.58);
}

/// Run to the player's next regeneration step and just past it, calling `before` first;
/// returns the health and vigor sent on the way.
fn to_next_step(
    h: &mut Harness,
    b: usize,
    world: ConnId,
    mut before: impl FnMut(&mut Harness),
) -> Vec<(f64, u32, i32)> {
    let next = h.server.players[&world].next_regen;
    before(h);
    // The world steps every quarter of a second: half a second past the step is after it.
    let seconds = next - h.now.0 + 0.5;
    watch(h, b, seconds, |_| {})
}

/// A player is at its combat rates for a step when it was hurt since the last step, or is moving
/// at the step itself: a hurt, or a walk under way at the step, makes that step give nothing to a
/// player with no combat rates, and the next, at rest, gives again; a walk that stopped before the
/// step leaves it at rest.
/// Behaviour: server.regen.combat-when-fighting-hurt-or-moving
#[test]
fn a_hurt_or_a_walk_under_way_at_the_step_puts_it_at_the_combat_rates_a_walk_that_stopped_does_not()
{
    let (mut h, b, world) = hurt_player();
    let rest = to_next_step(&mut h, b, world, |_| {});
    assert_eq!(rest.len(), 2, "at rest it regains: {rest:?}");

    // Hurt between steps: the next step gives nothing, health or vigor.
    let hurt = to_next_step(&mut h, b, world, |h| {
        let p = h.server.players.get_mut(&world).expect("in the world");
        let c = quals::int(&p.qualities, quality::HEALTH).expect("health");
        quals::set_int(&mut p.qualities, quality::HEALTH, c - 5);
    });
    assert!(hurt.is_empty(), "hurt since the last step: {hurt:?}");
    let after = to_next_step(&mut h, b, world, |_| {});
    assert_eq!(after.len(), 2, "back at rest: {after:?}");

    // A walk that stops a second into the wait leaves the step at rest.
    let walked = to_next_step(&mut h, b, world, |h| {
        walk(world, true)(h);
        for _ in 0..20 {
            h.step();
        }
        walk(world, false)(h);
    });
    assert_eq!(walked.len(), 2, "a walk that stopped: {walked:?}");

    // A walk under way at the step puts the step at the combat rates.
    let walking = to_next_step(&mut h, b, world, walk(world, true));
    assert!(walking.is_empty(), "moving at the step: {walking:?}");
}

/// Each step's gain is a share of the vital's greatest with the player's effects: with Courage's
/// 30 more health, health 100 of 630 comes back at 10.5 a second and fills to 630.
/// Behaviour: server.regen.share-of-the-greatest-with-effects
#[test]
fn each_step_is_a_share_of_the_greatest_with_the_players_effects() {
    let (mut h, b, world) = hurt_player();
    give(&mut h, world, COURAGE_EFFECT, 1.0);
    let got = watch(&mut h, b, 90.0, walk(world, false));
    let health = health(&got);
    steps_at(&health, 630.0, 1.0);
    assert_eq!(health.last().map(|x| x.1), Some(630), "full with the buff");
}

/// A creature hurt to 10 of its 100 health regains nothing for eight seconds; from then on it
/// regains its greatest in a minute, in whole points, never a point every step. Hurt again, it
/// rests again first.
/// Behaviour: server.regen.a-creature-unhurt-regains-a-share-a-second
#[test]
fn a_creature_unhurt_for_eight_seconds_regains_its_greatest_health_in_a_minute() {
    let mut h = Harness::new();
    h.add_creatures(1);
    h.run_until(400, |h| h.server.things.by_id.values().any(|t| t.alive()));
    let id = h
        .server
        .things
        .by_id
        .values()
        .find(|t| t.alive())
        .map(|t| t.id)
        .expect("the camp's creature");
    let hurt = h.now.0;
    if let Some(t) = h.server.things.get_mut(id) {
        t.max_health = 100;
        t.health = 10;
        t.last_hurt = hurt;
    }
    let health = |h: &Harness| h.server.things.get(id).map_or(0, |t| t.health);
    // The world's own steps, a quarter of a second apart.
    let run_to = |h: &mut Harness, t: f64| {
        while h.now.0 < t {
            h.step();
        }
    };
    run_to(&mut h, hurt + 7.5);
    assert_eq!(health(&h), 10, "nothing while it rests");
    run_to(&mut h, hurt + 8.0 + 20.0);
    // 20 s at 100 a minute, within a step either way.
    let got = health(&h);
    assert!((32..=34).contains(&(got - 10)), "regained {}", got - 10);
    // Hurt again, it rests again before it regains anything.
    let again = h.now.0;
    if let Some(t) = h.server.things.get_mut(id) {
        t.health = 10;
        t.last_hurt = again;
    }
    run_to(&mut h, again + 7.5);
    assert_eq!(health(&h), 10, "nothing while it rests again");
}
