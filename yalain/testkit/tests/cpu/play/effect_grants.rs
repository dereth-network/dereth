//! Items an effect gives: a world's rule names what an effect gives where its own data names no
//! item, and the effect makes it in its player's pack as the server's gift wherever it is put on,
//! alone or in a chain, told by nothing but its arrival. Fixture: a player alone in the test world,
//! the test item, and effects modeled on the install's quest grant of a leather hide.

use auberean_protocol::{queue, ServerMessage};
use yalain_net::ConnId;
use yalain_testkit::{Harness, TEST_CREATURE, TEST_ITEM};
use yalain_world::calls::{self, Node, Value};
use yalain_world::effect_defs::{EffectDef, Grant, Kind, Scale};
use yalain_world::effects::{self, On};

/// The effect that gives, a chain holding it, and an effect of the same kind no rule names.
const GRANT: u32 = 0x6F00_0D90;
const CHAIN: u32 = 0x6F00_0D91;
const UNNAMED: u32 = 0x6F00_0D92;
/// A move answered, and a line shown.
const MOVE_DONE: u32 = 0x820E_005E;
const SHOW_TEXT: u32 = 0x8205_0011;

fn effect(state: u32, kind: Kind) -> EffectDef {
    EffectDef {
        state,
        kind,
        duration: Scale(Vec::new()),
        priority: Scale(Vec::new()),
        fx: 0,
        external_flags: 113,
        internal_flags: 0x4140_0013,
        ends_at_logout: false,
        ends_at_teleport: false,
        enum_value: 2792,
        eq_class: 0,
        name: Some((0xA860_7CEC, 0x2500_1083)),
    }
}

/// A player in the world beside a beetle of the test camp, the effect giving `count` of the test
/// item in its server's table, with a chain of it and an effect that gives nothing (an item maker
/// whose data names no item, with no rule).
fn player_with(count: u32) -> (Harness, usize, ConnId) {
    let mut h = Harness::new();
    h.add_creatures(1);
    let (b, _) = h.player_in_world("one", "Elara");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(10);
    let world = *h.server.players.keys().next().expect("in the world");
    let t = &mut h.server.effect_table.effects;
    let grant = Grant {
        template: TEST_ITEM,
        count,
    };
    t.insert(GRANT, effect(GRANT, Kind::GrantItem(grant)));
    t.insert(CHAIN, effect(CHAIN, Kind::Instants(vec![GRANT])));
    let numeric = Kind::Numeric {
        flags: 0,
        stats: Vec::new(),
        magnitude: Scale(Vec::new()),
    };
    t.insert(UNNAMED, effect(UNNAMED, numeric));
    (h, b, world)
}

fn put_on(h: &mut Harness, world: ConnId, on: On, state: u32) -> bool {
    let caster = h.server.players[&world].character;
    let cast = effects::Cast {
        caster,
        spellcraft: 1.0,
        ..effects::Cast::default()
    };
    let clock = h.clock();
    effects::apply_on(&mut h.server, &mut h.net, on, state, cast, clock)
}

/// How many of the test item the player holds.
fn held(h: &Harness, world: ConnId) -> u32 {
    h.server.players[&world]
        .play
        .items
        .iter()
        .filter(|i| i.template == TEST_ITEM)
        .map(|i| i.quantity.max(1))
        .sum()
}

/// The moves answered since `from`: each one's move type and the object it came from.
fn moves(h: &Harness, b: usize, from: usize) -> Vec<(u32, u64)> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c) if c.function.0 == MOVE_DONE => c
                .args
                .as_ref()
                .and_then(|a| calls::values(&a.0).ok())
                .and_then(|v| {
                    v.iter().find_map(|x: &Value| match x.root() {
                        Some(Node::Script { slots, .. }) => Some(slots.clone()),
                        _ => None,
                    })
                }),
            _ => None,
        })
        .map(|s| (s[12].1, u64::from(s[16].1) | u64::from(s[17].1) << 32))
        .collect()
}

/// An effect a rule names an item for gives its player that many of it, made in its pack as the
/// server's gift (a gift's move, from nothing), and tells no line; put on again, it gives again.
/// Behaviour: server.effects.a-rule-names-what-an-effect-gives
#[test]
fn an_effect_a_rule_names_an_item_for_gives_it_to_its_player_as_a_gift() {
    let (mut h, b, world) = player_with(1);
    assert_eq!(held(&h, world), 0);
    let from = h.bots[b].inbox.len();
    assert!(
        put_on(&mut h, world, On::Player(world), GRANT),
        "it did something"
    );
    h.run(2);
    assert_eq!(held(&h, world), 1);
    assert_eq!(moves(&h, b, from), [(0, 0)], "a gift's move, from nothing");
    assert!(
        !h.calls(b, from).iter().any(|(f, _)| *f == SHOW_TEXT),
        "no line of its own"
    );
    assert!(put_on(&mut h, world, On::Player(world), GRANT));
    h.run(2);
    assert_eq!(held(&h, world), 2, "each time it is put on");
}

/// A rule's count is how many it gives; a chain holding the effect gives as the effect does.
/// Behaviour: server.effects.a-rule-names-what-an-effect-gives
#[test]
fn an_effect_gives_its_rules_count_and_gives_when_a_chain_puts_it_on() {
    let (mut h, b, world) = player_with(3);
    let from = h.bots[b].inbox.len();
    assert!(put_on(&mut h, world, On::Player(world), CHAIN));
    h.run(2);
    assert_eq!(held(&h, world), 3);
    assert_eq!(moves(&h, b, from).len(), 1, "one stack of three");
}

/// An effect of the same kind no rule names gives nothing, and the effect a rule names gives
/// nothing to a creature.
/// Behaviour: server.effects.a-rule-names-what-an-effect-gives
#[test]
fn without_a_rule_an_effect_gives_nothing_and_a_creature_is_given_nothing() {
    let (mut h, b, world) = player_with(1);
    let from = h.bots[b].inbox.len();
    put_on(&mut h, world, On::Player(world), UNNAMED);
    h.run(2);
    assert_eq!(held(&h, world), 0);
    assert!(moves(&h, b, from).is_empty());
    let creature = h
        .world_messages(b, 0)
        .iter()
        .find_map(|m| match m {
            ServerMessage::CreateObject(o) if o.weenie.entity_did == TEST_CREATURE => Some(o.id),
            _ => None,
        })
        .expect("a beetle");
    assert!(!put_on(&mut h, world, On::Creature(creature), GRANT));
    h.run(2);
    assert_eq!(held(&h, world), 0);
    assert!(moves(&h, b, from).is_empty());
}
