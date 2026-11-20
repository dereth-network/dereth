//! An effect that picks: put on, it puts one of its list on in its place, drawn by weight, on its
//! player and the player's fellows alike (within its reach, where it has one), at the spellcraft it
//! was cast at. Fixture: players in the test world, some of them fellows, and a made-up picking
//! effect over Courage and Robustify from the test effects.

use yalain_net::ConnId;
use yalain_testkit::{Harness, COURAGE_EFFECT, ROBUSTIFY_EFFECT};
use yalain_world::effect_defs::{EffectDef, Kind, Pick, Scale};
use yalain_world::effects;
use yalain_world::fellowship::{Fellow, Fellowship, Vitals};

const PICKS: u32 = 0x7000_1EC7;

fn with_the_pick(h: &mut Harness, fellows: bool, reach: Option<u32>) {
    let base = h
        .server
        .effect_table
        .effects
        .get(&COURAGE_EFFECT)
        .cloned()
        .expect("the test world's Courage");
    h.server.effect_table.effects.insert(
        PICKS,
        EffectDef {
            state: PICKS,
            kind: Kind::Pick(Pick {
                effects: vec![(COURAGE_EFFECT, 3), (ROBUSTIFY_EFFECT, 1)],
                fellows,
                reach,
            }),
            duration: Scale::default(),
            fx: 0,
            ..base
        },
    );
}

fn fellowship(h: &mut Harness, leader: u64, all: &[u64]) {
    h.server.fellowships.list.push(Fellowship {
        name: "Seekers".into(),
        leader,
        flags: 0,
        room: 1,
        fellows: all
            .iter()
            .map(|&id| Fellow {
                id,
                joined: 0.0,
                sent: Vitals::default(),
                next_check: f64::INFINITY,
            })
            .collect(),
        last_claimant: 0,
    });
}

fn world_of(h: &Harness, me: u64) -> ConnId {
    *h.server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .map(|(c, _)| c)
        .expect("in the world")
}

/// The effects a player holds records of, and the spellcraft of each.
fn held(h: &Harness, who: u64) -> Vec<(u32, f32)> {
    h.server.players[&world_of(h, who)]
        .play
        .effects
        .iter()
        .map(|e| (e.state, e.spellcraft))
        .collect()
}

fn put_on(h: &mut Harness, who: u64, spellcraft: f32) -> bool {
    let world = world_of(h, who);
    let cast = effects::Cast {
        caster: who,
        spellcraft,
        ..effects::Cast::default()
    };
    let clock = h.clock();
    effects::apply(&mut h.server, &mut h.net, world, PICKS, cast, clock)
}

/// Put on with its fellows, the picking effect leaves no record of its own: its player and the
/// fellow hold one record each, of the same drawn effect, at the cast's spellcraft of 7; a player
/// outside the fellowship holds nothing.
/// Behaviour: server.effects.pick-one-of-a-list
#[test]
fn a_picking_effect_puts_one_drawn_effect_on_its_player_and_fellows_at_its_spellcraft() {
    let mut h = Harness::new();
    with_the_pick(&mut h, true, None);
    let (_, me) = h.player_in_world("one", "Elara");
    let (_, you) = h.player_in_world("two", "Bran");
    let (_, other) = h.player_in_world("three", "Cade");
    h.run(10);
    fellowship(&mut h, me, &[me, you]);
    assert!(put_on(&mut h, me, 7.0));
    h.run(3);
    let mine = held(&h, me);
    assert_eq!(mine.len(), 1, "one drawn record: {mine:?}");
    assert!([COURAGE_EFFECT, ROBUSTIFY_EFFECT].contains(&mine[0].0));
    assert!(
        (mine[0].1 - 7.0).abs() < f32::EPSILON,
        "at the cast's spellcraft"
    );
    assert_eq!(held(&h, you), mine, "the fellow has the same");
    assert!(held(&h, other).is_empty(), "no one else");
}

/// Drawn by weight: over 400 puttings on, Courage (weight 3) comes about three times as often as
/// Robustify (weight 1), each at least once.
/// Behaviour: server.effects.pick-by-weight
#[test]
fn a_picking_effect_draws_by_its_weights() {
    let mut h = Harness::new();
    with_the_pick(&mut h, false, None);
    let (_, me) = h.player_in_world("one", "Elara");
    h.run(10);
    let (mut courage, mut robustify) = (0, 0);
    for _ in 0..400 {
        h.server
            .players
            .get_mut(&world_of(&h, me))
            .expect("in")
            .play
            .effects
            .clear();
        assert!(put_on(&mut h, me, 1.0));
        match held(&h, me).first().map(|e| e.0) {
            Some(COURAGE_EFFECT) => courage += 1,
            Some(ROBUSTIFY_EFFECT) => robustify += 1,
            other => panic!("{other:?}"),
        }
    }
    assert!(
        robustify > 0 && courage > 2 * robustify,
        "{courage} to {robustify}"
    );
}

/// With a reach of 20 m, the drawn effect goes on its player and a fellow 15 m off, but not on a
/// fellow 25 m off nor on one in portal space beside it.
/// Behaviour: server.effects.pick-within-its-reach
#[test]
fn a_picking_effect_with_a_reach_puts_its_draw_only_on_fellows_within_it() {
    let mut h = Harness::new();
    with_the_pick(&mut h, true, Some(20));
    let (_, me) = h.player_in_world("one", "Elara");
    let (_, near) = h.player_in_world("two", "Bran");
    let (_, far) = h.player_in_world("three", "Cade");
    let (_, away) = h.player_in_world("four", "Dara");
    h.run(10);
    fellowship(&mut h, me, &[me, near, far, away]);
    let here = h.server.players[&world_of(&h, me)].position;
    for (who, off) in [(near, 15.0), (far, 25.0), (away, 0.0)] {
        let p = h.server.players.get_mut(&world_of(&h, who)).expect("in");
        p.position = here;
        p.position.origin[0] += off;
    }
    let trip = yalain_world::places::Trip {
        to: here,
        used: None,
        step: None,
        resurrect: false,
        sent: false,
        said: None,
        updates: Vec::new(),
    };
    h.server
        .players
        .get_mut(&world_of(&h, away))
        .expect("in")
        .trip = Some(trip);
    assert!(put_on(&mut h, me, 7.0));
    let mine = held(&h, me);
    assert_eq!(mine.len(), 1, "one drawn record: {mine:?}");
    assert_eq!(held(&h, near), mine, "the fellow within 20 m has the same");
    assert!(held(&h, far).is_empty(), "the fellow 25 m off has nothing");
    assert!(
        held(&h, away).is_empty(),
        "the fellow in portal space has nothing"
    );
}
