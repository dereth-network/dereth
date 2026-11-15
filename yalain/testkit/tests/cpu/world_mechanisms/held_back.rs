//! An effect that holds back the rises other effects give a quality while it is held.

use yalain_testkit::{Harness, DASH_EFFECT};
use yalain_world::effect_defs::{EffectDef, HoldBack, Kind, Scale};
use yalain_world::effects;

const VELOCITY: u32 = 0x3_0127;
/// A slowing effect: speed times 0.9.
const SLOW: u32 = 0x6F00_A001;
/// The effect that holds speed's rises back.
const REGULATOR: u32 = 0x6F00_A002;

/// A player in the world, the effect table holding a slow and the regulator, the regulator holding
/// back speed's rises but those of the effects it spares.
fn regulated(spares: Vec<u32>) -> (Harness, yalain_net::ConnId) {
    let mut h = Harness::new();
    let (_, me) = h.player_in_world("one", "Elara");
    let dash = h
        .server
        .effect_table
        .get(DASH_EFFECT)
        .cloned()
        .expect("dash");
    let numeric = |state: u32, class: u32, stat: u32, factor: f32| EffectDef {
        state,
        kind: Kind::Numeric {
            flags: 0xA,
            stats: vec![stat],
            magnitude: Scale(vec![(1.0, factor)]),
        },
        eq_class: class,
        ..dash.clone()
    };
    let t = &mut h.server.effect_table;
    t.effects.insert(SLOW, numeric(SLOW, 52, VELOCITY, 0.9));
    t.effects
        .insert(REGULATOR, numeric(REGULATOR, 4103, 0x4_0107, 1.0));
    t.holds_back.insert(
        REGULATOR,
        HoldBack {
            quality: VELOCITY,
            except: spares,
        },
    );
    let world = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .expect("in the world")
        .0;
    (h, world)
}

fn put_on(h: &mut Harness, world: yalain_net::ConnId, state: u32) {
    let me = h.server.players[&world].character;
    let cast = effects::Cast {
        caster: me,
        spellcraft: 1.0,
        pk: true,
        ..effects::Cast::default()
    };
    let clock = h.clock();
    effects::apply(&mut h.server, &mut h.net, world, state, cast, clock);
    h.run(2);
}

fn speed(h: &Harness, world: yalain_net::ConnId) -> f32 {
    effects::player_speed(&h.server, world).0
}

/// While a player holds the regulator, Dash's rise in speed counts for nothing, and counts again
/// as it ends; a slow still slows. Put on before or after the buff, the same.
/// Behaviour: server.effects.held-back-rises-count-for-nothing
#[test]
fn a_rise_an_effect_holds_back_counts_for_nothing_while_it_is_held() {
    let (mut h, world) = regulated(Vec::new());
    put_on(&mut h, world, DASH_EFFECT);
    assert!(
        (speed(&h, world) - 1.08).abs() < 1e-4,
        "{}",
        speed(&h, world)
    );
    put_on(&mut h, world, REGULATOR);
    assert!(
        (speed(&h, world) - 1.0).abs() < 1e-4,
        "held back: {}",
        speed(&h, world)
    );
    put_on(&mut h, world, SLOW);
    assert!(
        (speed(&h, world) - 0.9).abs() < 1e-4,
        "a slow still slows: {}",
        speed(&h, world)
    );
}

/// An effect the rule spares keeps its rise while the regulator is held: a mount's speed counts.
/// Behaviour: server.effects.held-back-rises-count-for-nothing
#[test]
fn an_effect_the_rule_spares_keeps_its_rise() {
    let (mut h, world) = regulated(vec![DASH_EFFECT]);
    put_on(&mut h, world, REGULATOR);
    put_on(&mut h, world, DASH_EFFECT);
    assert!(
        (speed(&h, world) - 1.08).abs() < 1e-4,
        "{}",
        speed(&h, world)
    );
}
