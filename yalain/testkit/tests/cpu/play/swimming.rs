//! Swimming: a player whose feet are 1.25 m or more under its block's water surface regains
//! nothing and loses vigor, all of it in 30 s, then health, all of it in two minutes; Swimming
//! Mastery takes its share off; with no health left it drowns. One wading less deep regains as on
//! land. Fixture: the start's player moved outdoors over made-up ground with water everywhere.

use yalain_net::ConnId;
use yalain_store::StoredPosition;
use yalain_testkit::{Harness, TestGround};
use yalain_world::player::quality;
use yalain_world::quals;

/// The water's surface, and the ground 30 m under it.
const SURFACE: f32 = 70.0;
/// Every vital's greatest.
const GREATEST: i32 = 1200;
/// Swimming Mastery's third rank, which takes 0.6 off.
const MASTERY: u32 = 2149;

/// A player outdoors with its feet `depth` m under the water's surface, its health and vigor
/// `health` and `vigor` of 1,200, and Swimming Mastery's third rank when `mastered`.
fn in_the_water(depth: f32, health: i32, vigor: i32, mastered: bool) -> (Harness, ConnId) {
    let mut h = Harness::new();
    let (_, id) = h.player_in_world("one", "Elara");
    h.run(5);
    h.server.ground = Some(Box::new(TestGround {
        land: Some(SURFACE - 30.0),
        water: Some(SURFACE),
        ..TestGround::default()
    }));
    if mastered {
        h.server.skill_table.skills.insert(
            MASTERY,
            yalain_world::skills::SkillDef {
                perk: Some(yalain_world::skills::Perk {
                    kind: 17,
                    value: 0.6,
                    priority: 3,
                }),
                ..yalain_world::skills::SkillDef::default()
            },
        );
    }
    let world = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == id)
        .expect("in the world")
        .0;
    let (cell, x, y) = yalain_world::spawn::outdoor(0x0299, 80.0, 80.0);
    let p = h.server.players.get_mut(&world).expect("in the world");
    p.position = StoredPosition {
        cell,
        origin: [x, y, SURFACE - depth],
        rotation: [1.0, 0.0, 0.0, 0.0],
    };
    for (q, v) in [
        (quality::HEALTH_MAX, GREATEST),
        (quality::HEALTH, health),
        (quality::VIGOR_MAX, GREATEST),
        (quality::VIGOR, vigor),
    ] {
        quals::set_int(&mut p.qualities, q, v);
    }
    if mastered {
        p.play.skills.push(yalain_world::state::SkillState {
            skill: MASTERY,
            granted: 0,
            mask: yalain_world::skills::mask::TRAINED,
            cost: 0,
            children: 0,
            dependents: 0,
            spent: 0,
        });
    }
    (h, world)
}

/// Health and vigor, second by second for `seconds`.
fn second_by_second(h: &mut Harness, world: ConnId, seconds: usize) -> Vec<(i32, i32)> {
    (0..seconds)
        .map(|_| {
            h.step_by(1.0);
            let q = &h.server.players[&world].qualities;
            (
                quals::int(q, quality::HEALTH).unwrap_or(0),
                quals::int(q, quality::VIGOR).unwrap_or(0),
            )
        })
        .collect()
}

/// A player floating 1.6 m under the surface loses vigor and none of its health until the vigor
/// is gone, about 30 s in; then health, about half of it in the next minute.
/// Behaviour: server.vitals.swimming-costs-vigor-then-health
#[test]
fn a_swimmer_loses_all_its_vigor_in_thirty_seconds_then_its_health_in_two_minutes() {
    let (mut h, world) = in_the_water(1.6, GREATEST, GREATEST, false);
    let seen = second_by_second(&mut h, world, 160);
    assert!(
        seen.windows(2).all(|w| w[1].1 <= w[0].1),
        "vigor only falls"
    );
    assert!(seen[20].1 > 0, "some vigor left at 20 s: {:?}", seen[20]);
    let empty = seen.iter().position(|&(_, v)| v == 0).expect("vigor gone");
    assert!(empty < 40, "vigor gone by 40 s: {empty}");
    assert!(
        seen[..empty].iter().all(|&(hp, _)| hp == GREATEST),
        "no health lost while vigor lasts"
    );
    let lost = GREATEST - seen[empty + 60].0;
    assert!(
        (520..=680).contains(&lost),
        "half the health in the minute after: {lost}"
    );
}

/// A player wading with its feet a meter under the surface does not swim: it regains vigor.
/// Behaviour: server.vitals.swimming-costs-vigor-then-health
#[test]
fn a_player_wading_less_deep_regains_vigor_as_on_land() {
    let (mut h, world) = in_the_water(1.0, GREATEST, 600, false);
    let seen = second_by_second(&mut h, world, 20);
    assert!(seen[19].1 > 600, "regained: {:?}", seen[19]);
}

/// Swimming Mastery of 0.6 leaves four tenths of what swimming takes.
/// Behaviour: server.vitals.swimming-costs-vigor-then-health
#[test]
fn swimming_mastery_takes_its_share_off_what_swimming_costs() {
    let lost = |mastered: bool| {
        let (mut h, world) = in_the_water(1.6, GREATEST, GREATEST, mastered);
        GREATEST - second_by_second(&mut h, world, 20)[19].1
    };
    let (plain, mastered) = (lost(false), lost(true));
    assert!(plain > 400, "{plain}");
    assert!(
        (mastered * 10 - plain * 4).abs() <= 40,
        "four tenths: {mastered} of {plain}"
    );
}

/// A swimmer with no vigor and little health drowns.
/// Behaviour: server.vitals.swimming-costs-vigor-then-health
#[test]
fn a_swimmer_with_no_vigor_left_drowns_when_its_health_runs_out() {
    let (mut h, world) = in_the_water(1.6, 40, 0, false);
    second_by_second(&mut h, world, 20);
    assert!(
        h.server.players[&world].dying.is_some(),
        "drowned: {:?}",
        quals::int(&h.server.players[&world].qualities, quality::HEALTH)
    );
}
