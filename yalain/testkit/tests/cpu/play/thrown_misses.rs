//! A player's arrows and bolts: one that would land on a target that has moved more than a meter
//! since the throw is often missed; one on a target where it was aimed is not.

use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_store::StoredPosition;
use yalain_testkit::play::server_call;
use yalain_testkit::{Harness, TEST_CREATURE};
use yalain_world::combat::{self, Landing};
use yalain_world::player::quality;
use yalain_world::quals;
use yalain_world::rules::{self, Outcome};

const BOW: u32 = 0x4700_051B;
const DEADEYE: u32 = 207;

/// A player armed with a bow beside a creature that stays where it is and is not hurt to death.
fn bow_beside_a_creature() -> (Harness, usize, u64, u64) {
    let mut h = Harness::new();
    h.add_creatures(1);
    let (b, me) = h.player_in_world("one", "Elara");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
    let id = h
        .world_messages(b, 0)
        .iter()
        .find_map(|m| match m {
            ServerMessage::CreateObject(o) if o.weenie.entity_did == TEST_CREATURE => Some(o.id),
            _ => None,
        })
        .expect("told of it");
    let t = h.server.things.get_mut(id).expect("made");
    t.health = 1_000_000;
    t.max_health = 1_000_000;
    let c = t.creature.as_mut().expect("a creature");
    c.stats.natural_damage = 0.0;
    c.stats.fights = false;
    for p in h.server.players.values_mut() {
        quals::set_int(&mut p.qualities, quality::HEALTH_MAX, 100_000);
        quals::set_int(&mut p.qualities, quality::HEALTH, 100_000);
    }
    h.arm(b, me, BOW);
    (h, b, me, id)
}

/// The outcomes told in the creature's arrival stories of the player's throws since `from`.
fn arrivals(h: &Harness, b: usize, from: usize, me: u64, id: u64) -> Vec<u32> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::DoStory(e)
                if e.sender.id == id
                    && e.action.attacker == me
                    && e.action.bits == combat::BASIC_ARRIVAL =>
            {
                e.action
                    .hooks
                    .first()
                    .and_then(|k| u32::try_from(k.numbers[0]).ok())
            }
            _ => None,
        })
        .collect()
}

/// Where the creature stands.
fn place(h: &Harness, id: u64) -> StoredPosition {
    h.server.things.get(id).and_then(|t| t.at).expect("placed")
}

/// A basic arrow that would land on a creature that has moved more than a meter since it was
/// thrown misses about 0.32 of the time (2017: 656 of 2,332 such throws missed against 151 of
/// 10,001 on one that stood, the difference out of those that would have landed); on one where it
/// was aimed, none of them does.
/// Behaviour: server.combat.thrown-misses-by-moved
#[test]
fn an_arrow_that_would_land_on_a_moved_creature_is_often_missed() {
    let (mut h, b, me, id) = bow_beside_a_creature();
    let world = *h.server.players.keys().next().expect("in");
    let here = place(&h, id);
    let mut away = here;
    away.origin[0] += 5.0;
    let share = |h: &mut Harness, aimed: StoredPosition| {
        let from = h.bots[b].inbox.len();
        let n = 1_500;
        for _ in 0..n {
            let l = Landing {
                world,
                target: id,
                skill: 822,
                outcome: Outcome::Hit,
                damage: 0,
                back: 0,
                index: 1,
                thrown: true,
                anim_hook: 1,
                aimed: Some((aimed, false)),
            };
            let clock = h.clock();
            combat::lands(&mut h.server, &mut h.net, l, clock);
        }
        h.run(10);
        let told = arrivals(h, b, from, me, id);
        assert_eq!(told.len(), n, "each throw told as it arrives");
        let missed = told.iter().filter(|o| **o == Outcome::Miss.code()).count();
        #[allow(clippy::cast_precision_loss)]
        let share = missed as f64 / n as f64;
        share
    };
    let moved = share(&mut h, away);
    let expected = rules::ARROW_MISSES.moved;
    assert!(
        (moved - expected).abs() < 0.05,
        "moved off: {moved:.3} against {expected:.3}"
    );
    let still = share(&mut h, here);
    assert!(still.abs() < f64::EPSILON, "where it was aimed: {still:.3}");
}

/// A skill's arrow is missed far more often by a creature that moved while it flew than by one
/// that stood: the same rule as a basic arrow's.
/// Behaviour: server.combat.thrown-misses-by-moved
#[test]
fn a_skills_arrow_misses_a_creature_that_moved_while_it_flew_more_often() {
    use yalain_world::skill_use::function::USE;
    let misses = |shift: f32| {
        let (mut h, b, me, id) = bow_beside_a_creature();
        let mut n = 0;
        let mut missed = 0;
        for use_no in 1..=60 {
            for p in h.server.players.values_mut() {
                quals::set_int(&mut p.qualities, quality::VIGOR_MAX, 10_000);
                quals::set_int(&mut p.qualities, quality::VIGOR, 10_000);
            }
            let from = h.bots[b].inbox.len();
            let home = place(&h, id);
            h.bots[b].send(
                true,
                queue::WEENIE,
                &server_call(
                    USE,
                    &[Arg::Int(use_no), Arg::Int(DEADEYE), Arg::LongInt(id)],
                ),
            );
            let thrown = |h: &Harness| {
                h.world_messages(b, from).iter().any(|m| {
                    matches!(m, ServerMessage::DoStory(e) if e.action.attacker == me
                        && e.action.skill == DEADEYE
                        && e.action.hooks.iter().any(|k| k.bits & 0xF == 3))
                })
            };
            if !h.run_until(100, thrown) {
                continue;
            }
            if let Some(t) = h.server.things.get_mut(id) {
                if let Some(at) = t.at.as_mut() {
                    at.origin[0] += shift;
                }
            }
            h.run(120);
            let told: Vec<u32> = h
                .world_messages(b, from)
                .iter()
                .filter_map(|m| match m {
                    ServerMessage::DoStory(e)
                        if e.sender.id == id
                            && e.action.attacker == me
                            && e.action.skill == DEADEYE =>
                    {
                        e.action
                            .hooks
                            .first()
                            .and_then(|k| u32::try_from(k.numbers[0]).ok())
                    }
                    _ => None,
                })
                .collect();
            n += told.len();
            missed += told.iter().filter(|o| **o == Outcome::Miss.code()).count();
            if let Some(t) = h.server.things.get_mut(id) {
                t.at = Some(home);
            }
        }
        (n, missed)
    };
    let (n_still, still) = misses(0.0);
    let (n_moved, moved) = misses(5.0);
    assert!(
        n_still >= 30 && n_moved >= 30,
        "{n_still} and {n_moved} arrived"
    );
    assert!(
        moved >= still + 6,
        "moved off {moved} of {n_moved}, where it stood {still} of {n_still}"
    );
}
