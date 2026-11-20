//! Effects that scale the heals their holder receives: held, each multiplies a heal of its vital
//! that comes with an effect by its factor at the record's spellcraft, a whole number dropping the
//! rest. Fixture: a player alone in the test world, Ease Suffering's heal (25 health at rating
//! one) and Inner Calm's (200 vigor) from the test effects, and made-up scaling effects shaped as
//! the install's.

use yalain_net::ConnId;
use yalain_testkit::{Harness, EASE_SUFFERING_EFFECT, INNER_CALM_EFFECT};
use yalain_world::effect_defs::{vital, EffectDef, Kind, Scale};
use yalain_world::effects;
use yalain_world::player::quality;
use yalain_world::quals;

/// Heals of health up: ×1.2 at spellcraft 1 to ×1.5 at 10; and down: ×0.5 to ×0.8. Heals of vigor
/// up the same.
const HEALTH_UP: u32 = 0x7000_1ED0;
const HEALTH_DOWN: u32 = 0x7000_1ECF;
const VIGOR_UP: u32 = 0x7000_1ED4;

fn scaling(h: &mut Harness, state: u32, which: u32, least: f32, most: f32, class: u32) {
    let base = h
        .server
        .effect_table
        .effects
        .get(&yalain_testkit::COURAGE_EFFECT)
        .cloned()
        .expect("the test world's Courage");
    h.server.effect_table.effects.insert(
        state,
        EffectDef {
            state,
            kind: Kind::HealScale {
                vital: which,
                factor: Scale(vec![(1.0, least), (10.0, most)]),
            },
            duration: Scale(vec![(0.0, 900.0)]),
            eq_class: class,
            fx: 0,
            ..base
        },
    );
}

fn in_world(h: &mut Harness) -> (u64, ConnId) {
    let (_, me) = h.player_in_world("one", "Elara");
    h.run(10);
    let world = *h.server.players.keys().next().expect("in the world");
    let p = h.server.players.get_mut(&world).expect("in");
    for (k, v) in [
        (quality::HEALTH_MAX, 600),
        (quality::HEALTH, 100),
        (quality::VIGOR_MAX, 600),
        (quality::VIGOR, 100),
    ] {
        quals::set_int(&mut p.qualities, k, v);
    }
    (me, world)
}

fn put_on(h: &mut Harness, world: ConnId, state: u32, spellcraft: f32) {
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

/// How much a heal raised a vital.
fn healed_by(h: &mut Harness, world: ConnId, heal: u32, of: u32) -> i32 {
    let now = |h: &Harness| quals::int(&h.server.players[&world].qualities, of).unwrap_or(0);
    let p = h.server.players.get_mut(&world).expect("in");
    quals::set_int(&mut p.qualities, of, 100);
    let before = now(h);
    put_on(h, world, heal, 1.0);
    now(h) - before
}

/// A heal of 25 health is 37 under a health heal scaling held at spellcraft 10 (×1.5), 30 at 1
/// (×1.2), and 15 under the downward one at 5 (×0.633...: 15.8, the rest dropped) in a class of its
/// own; a vigor scaling leaves it alone and makes a heal of 200 vigor 240.
/// Behaviour: server.effects.heal-scales
#[test]
fn a_held_heal_scaling_multiplies_the_heals_its_holder_receives() {
    let mut h = Harness::new();
    scaling(&mut h, HEALTH_UP, vital::HEALTH, 1.2, 1.5, 1127);
    scaling(&mut h, HEALTH_DOWN, vital::HEALTH, 0.5, 0.8, 7001);
    scaling(&mut h, VIGOR_UP, vital::VIGOR, 1.2, 1.5, 7002);
    let (_, world) = in_world(&mut h);
    assert_eq!(
        healed_by(&mut h, world, EASE_SUFFERING_EFFECT, quality::HEALTH),
        25
    );
    put_on(&mut h, world, HEALTH_UP, 10.0);
    assert_eq!(
        healed_by(&mut h, world, EASE_SUFFERING_EFFECT, quality::HEALTH),
        37
    );
    h.server
        .players
        .get_mut(&world)
        .expect("in")
        .play
        .effects
        .clear();
    put_on(&mut h, world, HEALTH_UP, 1.0);
    assert_eq!(
        healed_by(&mut h, world, EASE_SUFFERING_EFFECT, quality::HEALTH),
        30
    );
    h.server
        .players
        .get_mut(&world)
        .expect("in")
        .play
        .effects
        .clear();
    put_on(&mut h, world, HEALTH_DOWN, 5.0);
    assert_eq!(
        healed_by(&mut h, world, EASE_SUFFERING_EFFECT, quality::HEALTH),
        15
    );
    h.server
        .players
        .get_mut(&world)
        .expect("in")
        .play
        .effects
        .clear();
    put_on(&mut h, world, VIGOR_UP, 1.0);
    assert_eq!(
        healed_by(&mut h, world, EASE_SUFFERING_EFFECT, quality::HEALTH),
        25,
        "health alone"
    );
    assert_eq!(
        healed_by(&mut h, world, INNER_CALM_EFFECT, quality::VIGOR),
        240,
        "vigor"
    );
}
