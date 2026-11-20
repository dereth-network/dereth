//! A level-up: the greatest health and vigor raised by the level's step on what the player held,
//! the vitals filled to the greatest it then has with its vitae and effects, and the text naming
//! the skill credits held. Fixture: a player alone in the test world on a made-up level table
//! (health 100, 130, 160 and vigor 100, 150, 200 for levels 1 to 3, a credit a level), Courage
//! from the test effects.

use auberean_protocol::ServerMessage;
use auberean_script::native_object::VariableValue;
use yalain_net::ConnId;
use yalain_testkit::{Harness, COURAGE_EFFECT};
use yalain_world::calls::Node;
use yalain_world::levels::{Level, LevelTable};
use yalain_world::player::quality;
use yalain_world::progress::{text, var};
use yalain_world::{effects, quals};

/// A level-2 player in the world on the made-up level table.
fn at_level_two() -> (Harness, usize, ConnId) {
    let mut h = Harness::new();
    let (b, _) = h.player_in_world("one", "Elara");
    h.run(10);
    h.server.levels = LevelTable {
        levels: (1..=3)
            .map(|l: u64| Level {
                xp: l * 10,
                health: 70 + 30 * i32::try_from(l).expect("small"),
                vigor: 50 + 50 * i32::try_from(l).expect("small"),
                skill_credits: 1,
                hero_credits: 0,
                hero_credit_cap: 0,
                armor_cap: 0,
                shield_cap: 0,
            })
            .collect(),
    };
    let world = *h.server.players.keys().next().expect("in the world");
    let p = h.server.players.get_mut(&world).expect("in the world");
    quals::set_int(&mut p.qualities, 302, 2);
    (h, b, world)
}

/// The integer qualities a client was sent since `from`, in order.
fn ints(h: &Harness, b: usize, from: usize) -> Vec<(u32, i32)> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::UpdateInt(x) => Some((x.quality, x.value)),
            _ => None,
        })
        .collect()
}

/// The level-up text's word variables a client was shown since `from`, by id.
fn level_up_text(h: &Harness, b: usize, from: usize) -> Vec<(u32, u32)> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c)
                if c.function.0 == yalain_world::session::function::SHOW_TEXT =>
            {
                let v = c
                    .args
                    .as_ref()
                    .and_then(|a| yalain_world::calls::values(&a.0).ok())
                    .unwrap_or_default();
                v.iter().find_map(|x| match x.root() {
                    Some(Node::TextInfo(t)) if t.string_id == text::LEVEL_UP.0 => Some(
                        t.variables
                            .iter()
                            .filter_map(|v| match v.value {
                                VariableValue::Word(n) => Some((v.id, n)),
                                _ => None,
                            })
                            .collect(),
                    ),
                    _ => None,
                })
            }
            _ => None,
        })
        .next()
        .expect("the level-up text")
}

/// A player at level 2 with greatest health 180 (50 above the level's 130) and vigor 150, hurt, at
/// 50 vitae and with Courage's 30 more health, reaches level 3: its greatest health rises by the
/// step to 210 and vigor to 200, and it is filled to 120 health (half of 210 and Courage's 30
/// together) and 100 vigor, below the greatest it holds, as the text tells 30 and 50 gained.
/// Behaviour: server.experience.level-up-fills-to-the-greatest
#[test]
fn a_level_up_raises_the_greatest_by_its_step_and_fills_to_the_greatest_with_vitae_and_effects() {
    let (mut h, b, world) = at_level_two();
    let p = h.server.players.get_mut(&world).expect("in the world");
    for (k, v) in [
        (quality::HEALTH_MAX, 180),
        (quality::HEALTH, 20),
        (quality::VIGOR_MAX, 150),
        (quality::VIGOR, 10),
    ] {
        quals::set_int(&mut p.qualities, k, v);
    }
    quals::set_float(&mut p.qualities, yalain_world::death::vitae_q::VITAE, 50.0);
    let cast = effects::Cast {
        caster: p.character,
        spellcraft: 1.0,
        ..effects::Cast::default()
    };
    let clock = h.clock();
    assert!(effects::apply(
        &mut h.server,
        &mut h.net,
        world,
        COURAGE_EFFECT,
        cast,
        clock
    ));
    h.run(3);
    let from = h.bots[b].inbox.len();
    yalain_world::progress::level_up(&mut h.server, &mut h.net, world, 2, 3);
    h.run(3);
    let p = &h.server.players[&world];
    let held = |k: u32| quals::int(&p.qualities, k);
    assert_eq!(held(quality::HEALTH_MAX), Some(210), "greatest health");
    assert_eq!(held(quality::VIGOR_MAX), Some(200), "greatest vigor");
    assert_eq!(held(quality::HEALTH), Some(120), "health");
    assert_eq!(held(quality::VIGOR), Some(100), "vigor");
    let sent = ints(&h, b, from);
    for want in [
        (quality::HEALTH_MAX, 210),
        (quality::VIGOR_MAX, 200),
        (quality::HEALTH, 120),
        (quality::VIGOR, 100),
    ] {
        assert!(sent.contains(&want), "{want:?} sent: {sent:?}");
    }
    let told = level_up_text(&h, b, from);
    assert!(told.contains(&(var::HEALTH_GAINED, 30)), "{told:?}");
    assert!(told.contains(&(var::VIGOR_GAINED, 50)), "{told:?}");
}

/// A player holding 4 skill credits unspent reaches a level worth one: the text names the 5 it
/// now holds.
/// Behaviour: server.experience.level-up-text-names-the-credits-held
#[test]
fn a_level_ups_text_names_the_skill_credits_held() {
    let (mut h, b, world) = at_level_two();
    h.server
        .players
        .get_mut(&world)
        .expect("in the world")
        .play
        .credits = 4;
    let from = h.bots[b].inbox.len();
    yalain_world::progress::level_up(&mut h.server, &mut h.net, world, 2, 3);
    h.run(3);
    assert_eq!(h.server.players[&world].play.credits, 5);
    let told = level_up_text(&h, b, from);
    assert!(told.contains(&(var::CREDITS, 5)), "{told:?}");
}
