//! The wounded look: an object's look as its health crosses the fifths of its greatest, told by the
//! order the messages were sent in.
//! Fixture: a creature of the test camp beside a player where a character starts, and that player.

use auberean_protocol::ServerMessage;
use yalain_testkit::Harness;
use yalain_world::combat::WOUNDED_FX;
use yalain_world::player::quality;
use yalain_world::regen::rate::PEACE_HEALTH;

/// What a client was told of a health and a wounded look, in the order sent.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Told {
    /// Its player's own health.
    Health(i32),
    /// The look, how wounded.
    Look(f32),
    /// The look's end.
    Stop,
}

/// The health and the wounded looks of `id` a client was sent since `from`, by the connection's
/// sequence (the low word of each message's blob id).
fn told(h: &Harness, b: usize, from: usize, id: u64) -> Vec<Told> {
    let mut sent: Vec<(u32, Told)> = h.bots[b].inbox[from..]
        .iter()
        .filter(|r| r.world)
        .filter_map(|r| {
            let t = match r.message()? {
                ServerMessage::UpdateInt(q) if q.quality == quality::HEALTH => {
                    Told::Health(q.value)
                }
                ServerMessage::DoFx(e) if e.action.fx == WOUNDED_FX && e.sender.id == id => {
                    Told::Look(e.action.scalar)
                }
                ServerMessage::StopFx(e) if e.action.0 == WOUNDED_FX && e.sender.id == id => {
                    Told::Stop
                }
                _ => return None,
            };
            #[allow(clippy::cast_possible_truncation)]
            let sequence = r.blob_id as u32;
            Some((sequence, t))
        })
        .collect();
    sent.sort_by_key(|(q, _)| *q);
    sent.into_iter().map(|(_, t)| t).collect()
}

/// The look told as health moves from `was` to `now` of `most`, in whole numbers: none while the
/// health stays in one fifth (each fifth from its edge up to the next, full health one of its own),
/// the look at or below four fifths, else its end.
fn rule(was: i32, now: i32, most: i32) -> Option<Told> {
    if was * 5 / most == now * 5 / most {
        None
    } else if now * 5 <= most * 4 {
        #[allow(clippy::cast_precision_loss)]
        Some(Told::Look(1.0 - now as f32 / most as f32))
    } else {
        Some(Told::Stop)
    }
}

/// A creature hurt from full health in steps is told: its look's end as it leaves full health
/// above four fifths, nothing while it stays inside a fifth (nor as it comes down to four fifths
/// exactly, the edge of the fifth it is in), the look as it goes below four fifths and below three
/// fifths, nothing in between.
/// Behaviour: server.combat.the-wounded-look-moves-by-fifths
#[test]
fn a_creature_hurt_in_steps_is_told_its_look_only_as_its_health_crosses_a_fifth() {
    let mut h = Harness::new();
    h.add_creatures(1);
    let (b, me) = h.player_in_world("one", "Striker");
    h.run(10);
    let id = h
        .server
        .things
        .by_id
        .values()
        .find(|t| t.template == yalain_testkit::TEST_CREATURE && t.alive())
        .map(|t| t.id)
        .expect("a creature");
    {
        let t = h.server.things.get_mut(id).expect("the creature");
        t.health = 100;
        t.max_health = 100;
        t.creature.as_mut().expect("a creature").stats.attacks_first = false;
    }
    let mut health = 100;
    for harm in [5, 5, 5, 5, 1, 9, 15] {
        let from = h.bots[b].inbox.len();
        let clock = h.clock();
        yalain_world::combat::hurt_creature(&mut h.server, &mut h.net, id, me, harm, clock);
        h.run(2);
        let want: Vec<Told> = rule(health, health - harm, 100).into_iter().collect();
        assert_eq!(told(&h, b, from, id), want, "{health} to {}", health - harm);
        health -= harm;
    }
}

/// A player regaining health from below four fifths is told, in the order sent, each health it
/// reaches and the look's end only as it crosses four fifths and as it reaches full health; nothing
/// for a health inside a fifth.
/// Behaviour: server.combat.the-wounded-look-moves-by-fifths
#[test]
fn a_player_regaining_health_is_told_its_look_only_as_its_health_crosses_a_fifth() {
    let mut h = Harness::new();
    let (b, me) = h.player_in_world("one", "Mender");
    h.run(10);
    let from = h.bots[b].inbox.len();
    {
        let p = h
            .server
            .players
            .values_mut()
            .find(|p| p.character == me)
            .expect("in the world");
        yalain_world::quals::set_int(&mut p.qualities, quality::HEALTH_MAX, 100);
        yalain_world::quals::set_int(&mut p.qualities, quality::HEALTH, 70);
        yalain_world::quals::set_float(&mut p.qualities, PEACE_HEALTH, 1.0);
    }
    let t = h.now.0;
    h.run_to(yalain_net::LocalTime(t + 90.0));
    let sent = told(&h, b, from, me);
    let mut was = 70;
    let (mut crossings, mut inside) = (0, 0);
    for (i, t) in sent.iter().enumerate() {
        let Told::Health(now) = *t else {
            continue;
        };
        let next = sent
            .get(i + 1)
            .copied()
            .filter(|n| !matches!(n, Told::Health(_)));
        assert_eq!(next, rule(was, now, 100), "{was} to {now}: {sent:?}");
        if next.is_some() {
            crossings += 1;
        } else {
            inside += 1;
        }
        was = now;
    }
    assert_eq!(was, 100, "it regained it all: {sent:?}");
    assert!(crossings >= 2 && inside >= 1, "{sent:?}");
}

/// A creature regaining health a point at a time from below four fifths is told the look as it
/// comes up to four fifths exactly (the edge of the fifth above), then nothing until its look ends
/// as it reaches full health.
/// Behaviour: server.combat.the-wounded-look-moves-by-fifths
#[test]
fn a_creature_regaining_health_is_told_the_look_at_four_fifths_exactly_then_its_end_at_full() {
    let mut h = Harness::new();
    h.add_creatures(1);
    let (b, _) = h.player_in_world("one", "Watcher");
    h.run(10);
    let id = h
        .server
        .things
        .by_id
        .values()
        .find(|t| t.template == yalain_testkit::TEST_CREATURE && t.alive())
        .map(|t| t.id)
        .expect("a creature");
    let from = h.bots[b].inbox.len();
    {
        let t = h.server.things.get_mut(id).expect("the creature");
        t.max_health = 100;
        t.health = 79;
        t.creature.as_mut().expect("a creature").stats.attacks_first = false;
    }
    assert!(
        h.run_until(steps(60.0), |h| h
            .server
            .things
            .get(id)
            .is_some_and(|t| t.health == 100)),
        "it regains its health"
    );
    h.run(2);
    assert_eq!(
        told(&h, b, from, id),
        [rule(79, 80, 100).expect("the look"), Told::Stop],
        "the look at four fifths exactly, its end at full"
    );
}

/// How many of the harness's steps make `seconds` (rounded up).
fn steps(seconds: f64) -> usize {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let n = (seconds / yalain_testkit::STEP).ceil() as usize;
    n
}
