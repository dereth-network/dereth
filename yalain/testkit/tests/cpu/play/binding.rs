//! Items that bind when used in combat: a made item a recipe binds is equipped with a warning, at
//! every equip, until its holder fights with it on.

use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_content::{Prop, Props};
use yalain_testkit::play::{server_call, use_of};
use yalain_testkit::{Harness, TEST_CREATURE};
use yalain_world::calls::{self, Node, Value};
use yalain_world::equip::{self, BINDS_WARNING, DISPLAY_MESSAGE};

const CHEST: u32 = 0x4700_7301;
const SWORD: u32 = 0x4700_051C;
const START_ATTACK: u32 = 0x8204_0008;

/// A player beside the test camp's beetle (made a practice target that never fights back), a
/// sword in hand and a chest piece in the pack that binds when used in combat; returns the
/// harness, the client, the player, the beetle and the chest piece.
fn marked_chest() -> (Harness, usize, u64, u64, u64) {
    let mut h = Harness::new();
    h.add_creatures(1);
    let mut props = Props::default();
    props
        .set("Item_ValidInventoryLocations", Prop::Word(2))
        .set("Item_PreferredInventoryLocation", Prop::Word(2));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(CHEST, 0x180, props));
    let (b, me) = h.player_in_world("one", "Elara");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
    let beetle = h
        .world_messages(b, 0)
        .iter()
        .find_map(|m| match m {
            ServerMessage::CreateObject(o) if o.weenie.entity_did == TEST_CREATURE => Some(o.id),
            _ => None,
        })
        .expect("the camp's creature");
    if let Some(c) = h
        .server
        .things
        .get_mut(beetle)
        .and_then(|t| t.creature.as_mut())
    {
        c.stats.fights = false;
    }
    h.arm(b, me, SWORD);
    // Worn by arming it, then taken off to the pack.
    let chest = h.arm(b, me, CHEST);
    if let Some(t) = h.server.things.get_mut(chest) {
        t.binds_on_use = true;
    }
    used(&mut h, b, me, chest);
    (h, b, me, beetle, chest)
}

/// Use an item of the pack (wear it, or take it off); returns where the client's inbox stood.
fn used(h: &mut Harness, b: usize, me: u64, item: u64) -> usize {
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(yalain_world::requests::function::USE, &[use_of(item, me)]),
    );
    h.run(10);
    from
}

/// The equipping answers since `from`: their item profile's bind-on-use word.
fn binds_words(h: &Harness, b: usize, from: usize) -> Vec<u32> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c) if c.function.0 == equip::function::EQUIP_DONE => {
                let v = calls::values(&c.args.as_ref()?.0).ok()?;
                let Value::Graph(g) = v.first()? else {
                    return None;
                };
                g.iter().find_map(|(_, n)| match n {
                    Node::Script { package, slots } if *package == equip::EQUIP_PROFILE.0 => {
                        Some(slots[4].1)
                    }
                    _ => None,
                })
            }
            _ => None,
        })
        .collect()
}

/// The warnings a client was shown since `from`: each one's string, table and whether it is on
/// top.
fn warnings(h: &Harness, b: usize, from: usize) -> Vec<(u32, u32, u32)> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c) if c.function.0 == DISPLAY_MESSAGE => {
                let v = calls::values(&c.args.as_ref()?.0).ok()?;
                let topmost = match v.first()? {
                    Value::Int(t) => *t,
                    _ => return None,
                };
                let text = v.iter().find_map(|x| match x.root() {
                    Some(Node::TextInfo(t)) => Some((t.string_id, t.table)),
                    _ => None,
                })?;
                Some((text.0, text.1, topmost))
            }
            _ => None,
        })
        .collect()
}

/// An item that binds when used in combat is answered with the mark in its profile and the
/// warning at every equip, not shown on top; once its holder has fought with it on, it is bound:
/// equipped again with no mark and no warning. An item without the mark is never warned.
/// Behaviour: server.items.binds-on-use-warning
#[test]
fn an_item_that_binds_in_combat_warns_at_every_equip_until_fought_with() {
    let (mut h, b, me, beetle, chest) = marked_chest();
    let warned = (BINDS_WARNING.0, BINDS_WARNING.1, 0);
    let from = used(&mut h, b, me, chest);
    assert_eq!(binds_words(&h, b, from), [1], "the mark in the profile");
    assert_eq!(warnings(&h, b, from), [warned], "warned");
    used(&mut h, b, me, chest);
    let from = used(&mut h, b, me, chest);
    assert_eq!(binds_words(&h, b, from), [1]);
    assert_eq!(
        warnings(&h, b, from),
        [warned],
        "warned again at the next equip"
    );
    // A fight with it on binds it, with nothing said.
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(START_ATTACK, &[Arg::LongInt(beetle)]),
    );
    h.run(40);
    assert!(warnings(&h, b, from).is_empty());
    assert!(h.server.things.get(chest).is_some_and(|t| !t.binds_on_use));
    used(&mut h, b, me, chest);
    let from = used(&mut h, b, me, chest);
    assert_eq!(binds_words(&h, b, from), [0], "bound: no mark");
    assert!(warnings(&h, b, from).is_empty(), "nor a warning");
}

/// Being hit binds what a player wears and wields that binds when used in combat; what it only
/// carries keeps the mark.
/// Behaviour: server.items.binds-when-hit
#[test]
fn being_hit_binds_what_is_worn_but_not_what_is_carried() {
    let (mut h, b, me, beetle, chest) = marked_chest();
    let carried = h.arm(b, me, CHEST);
    used(&mut h, b, me, carried);
    if let Some(t) = h.server.things.get_mut(carried) {
        t.binds_on_use = true;
    }
    used(&mut h, b, me, chest);
    let worn = |h: &Harness, id: u64| {
        h.server
            .players
            .values()
            .flat_map(|p| p.play.items.iter())
            .any(|i| i.id == id && i.location != 0)
    };
    assert!(worn(&h, chest) && !worn(&h, carried));
    let world = *h.server.players.keys().next().expect("in the world");
    let clock = h.clock();
    yalain_world::combat::after_hurt(&mut h.server, &mut h.net, world, beetle, clock);
    assert!(
        h.server.things.get(chest).is_some_and(|t| !t.binds_on_use),
        "worn: bound"
    );
    assert!(
        h.server.things.get(carried).is_some_and(|t| t.binds_on_use),
        "carried: still marked"
    );
}
