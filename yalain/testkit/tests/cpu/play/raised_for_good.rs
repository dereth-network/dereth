//! An effect that raises a vital's own level: with no duration it adds to the greatest for good,
//! told as the greatest alone, with no record, and stays through a level-up; a record of one kept
//! from before comes back added to the greatest. Fixture: a player alone in the test world, a
//! made-up effect adding 50 to vigor's level and its greatest (as the training hall's quest
//! reward does), Courage from the test effects, a made-up level table.

use auberean_protocol::{queue, ServerMessage};
use yalain_net::ConnId;
use yalain_testkit::{Harness, COURAGE_EFFECT};
use yalain_world::effect_defs::{numeric, EffectDef, Kind, Scale};
use yalain_world::levels::{Level, LevelTable};
use yalain_world::player::quality;
use yalain_world::{effects, quals};

const ADD_EFFECT: u32 = 0x8208_0017;
/// The made-up raise: 50 to vigor's own level and its greatest, no duration.
const RAISE: u32 = 0x6F00_7F85;

fn with_the_raise(h: &mut Harness) {
    let base = h
        .server
        .effect_table
        .effects
        .get(&COURAGE_EFFECT)
        .cloned()
        .expect("the test world's Courage");
    h.server.effect_table.effects.insert(
        RAISE,
        EffectDef {
            state: RAISE,
            kind: Kind::Numeric {
                flags: 0x1 | numeric::ADD,
                stats: vec![0x4_0101, 0x4_0108],
                magnitude: Scale(vec![(0.0, 50.0)]),
            },
            duration: Scale::default(),
            fx: 0,
            eq_class: 17,
            ..base
        },
    );
}

fn player(h: &mut Harness) -> (usize, u64, ConnId) {
    let (b, me) = h.player_in_world("one", "Elara");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
    (b, me, world_of(h, me))
}

fn world_of(h: &Harness, me: u64) -> ConnId {
    *h.server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .map(|(c, _)| c)
        .expect("in the world")
}

fn held(h: &Harness, world: ConnId, q: u32) -> Option<i32> {
    quals::int(&h.server.players[&world].qualities, q)
}

fn set(h: &mut Harness, world: ConnId, q: u32, v: i32) {
    let p = h.server.players.get_mut(&world).expect("in the world");
    quals::set_int(&mut p.qualities, q, v);
}

fn ints(h: &Harness, b: usize, from: usize) -> Vec<(u32, i32)> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::UpdateInt(x) => Some((x.quality, x.value)),
            _ => None,
        })
        .collect()
}

fn log_off(h: &mut Harness, b: usize, id: u64) {
    use auberean_protocol::login::CharacterRef;
    use auberean_protocol::ClientMessage;
    let before = h.bots[b].inbox.len();
    h.bots[b].send(
        false,
        queue::LOGON,
        &ClientMessage::LogOff(CharacterRef { character: id }),
    );
    assert!(h.run_until(400, |h| h.bots[b].inbox[before..]
        .iter()
        .any(|r| matches!(r.message(), Some(ServerMessage::CharacterList(_))))));
}

/// The raise put on a level-15 player with greatest vigor 520 and vigor 300 makes the greatest 570,
/// sent alone: no record, no effect added on the client, vigor still 300, and the greatest with
/// effects 570 (not 620). Reaching level 16 (the table's vigor 520 to 550) makes it 600.
/// Behaviour: server.effects.raise-for-good
#[test]
fn an_effect_raising_vigors_own_level_adds_to_the_greatest_for_good_with_no_record() {
    let mut h = Harness::new();
    with_the_raise(&mut h);
    let (b, _, world) = player(&mut h);
    set(&mut h, world, quality::VIGOR_MAX, 520);
    set(&mut h, world, quality::VIGOR, 300);
    let from = h.bots[b].inbox.len();
    let clock = h.clock();
    assert!(effects::apply(
        &mut h.server,
        &mut h.net,
        world,
        RAISE,
        effects::Cast::default(),
        clock
    ));
    h.run(3);
    assert_eq!(held(&h, world, quality::VIGOR_MAX), Some(570));
    assert_eq!(
        held(&h, world, quality::VIGOR),
        Some(300),
        "vigor as it was"
    );
    assert_eq!(ints(&h, b, from), [(quality::VIGOR_MAX, 570)]);
    assert!(
        h.server.players[&world].play.effects.is_empty(),
        "no record"
    );
    assert!(
        !h.world_messages(b, from).iter().any(|m| matches!(
            m,
            ServerMessage::PlayerCall(c) if c.function.0 == ADD_EFFECT
        )),
        "no effect added"
    );
    let p = &h.server.players[&world];
    assert_eq!(effects::max_vital(&h.server, p, quality::VIGOR_MAX), 570);
    h.server.levels = LevelTable {
        levels: (1..=16)
            .map(|l: u64| Level {
                xp: l * 10,
                health: 100,
                vigor: if l >= 16 { 550 } else { 520 },
                skill_credits: 1,
                hero_credits: 0,
                hero_credit_cap: 0,
                armor_cap: 0,
                shield_cap: 0,
            })
            .collect(),
    };
    yalain_world::progress::level_up(&mut h.server, &mut h.net, world, 15, 16);
    assert_eq!(
        held(&h, world, quality::VIGOR_MAX),
        Some(600),
        "kept through the level-up"
    );
}

/// A character kept with a record of the raise (as one was before such raises were added to the
/// greatest) comes back with the 50 added to its greatest vigor, in its description, and the
/// record gone; its other records stay.
/// Behaviour: server.effects.raise-for-good-kept-record-added
#[test]
fn a_kept_record_of_a_raise_comes_back_added_to_the_greatest() {
    let mut h = Harness::new();
    with_the_raise(&mut h);
    let (b, me, world) = player(&mut h);
    set(&mut h, world, quality::VIGOR_MAX, 520);
    let clock = h.clock();
    let cast = effects::Cast {
        caster: me,
        spellcraft: 1.0,
        ..effects::Cast::default()
    };
    assert!(effects::apply(
        &mut h.server,
        &mut h.net,
        world,
        COURAGE_EFFECT,
        cast,
        clock
    ));
    {
        let p = h.server.players.get_mut(&world).expect("in the world");
        let courage = p.play.effects[0];
        p.play.effects.push(yalain_world::state::EffectState {
            eid: courage.eid + 1,
            state: RAISE,
            timeout: f32::MAX,
            ..courage
        });
        p.play.effect_counter = courage.eid + 1;
    }
    yalain_world::world::keep_player(&mut h.server, world);
    log_off(&mut h, b, me);
    let from = h.bots[b].inbox.len();
    h.enter_world(b, "one", me);
    let world = world_of(&h, me);
    assert_eq!(held(&h, world, quality::VIGOR_MAX), Some(570));
    let states: Vec<u32> = h.server.players[&world]
        .play
        .effects
        .iter()
        .map(|e| e.state)
        .collect();
    assert_eq!(
        states,
        [COURAGE_EFFECT],
        "the raise's record gone, Courage kept"
    );
    let described = h.world_messages(b, from).iter().find_map(|m| match m {
        ServerMessage::PlayerQualities(q) => q.0.ints.get(quality::VIGOR_MAX).copied(),
        _ => None,
    });
    assert_eq!(described, Some(570), "its description says so");
}
