//! What a player's hand shows of an item it takes out of a corpse, picks up or puts down: a copy of
//! the item as its template draws it, the entity it is drawn as and its appearance as it lies, and
//! the answers name the item so. A template whose data names no entity shows its look.

use auberean_protocol::types::BehaviorParams;
use auberean_protocol::{queue, ServerMessage};
use yalain_content::props::{LOOK_ENTITY, PILE_APPEARANCE};
use yalain_content::{Prop, Props};
use yalain_net::ConnId;
use yalain_testkit::play::{server_call, use_of};
use yalain_testkit::{Harness, STEP, TEST_ITEM};
use yalain_world::calls::{self, Node, Value};
use yalain_world::drops::{self, DROP};
use yalain_world::inventory::{self, move_graph, Move};
use yalain_world::things::{Corpse, Kind, Thing};

const USE: u32 = yalain_world::requests::function::USE;
const MOVE_DONE: u32 = 0x820E_005E;

/// The training hall's digging tool, as the install describes it: drawn as its own entity, with
/// its appearance as it lies.
const TOOL: u32 = 0x4700_1AF1;
const TOOL_ENTITY: u32 = 0x4700_0939;
const TOOL_APPEARANCE: u32 = 0x2000_0021;

/// Taking one item out of a container, picking up, putting down.
const TAKING: u32 = 0x4100_0020;
const TAKE_ONE_SCRIPT: u32 = 0x5700_0009;
const PICK_UP_SCRIPT: u32 = 0x5700_0008;
const PUTTING_DOWN: u32 = 0x4100_0021;

/// A behavior toward an item with a copy of it in the hand: the behavior, its script, the target,
/// flag `0x100`, the copy and the context.
const IN_HAND: u32 = BehaviorParams::BEHAVIOR
    | BehaviorParams::FX_SCRIPT
    | BehaviorParams::TARGET
    | 0x100
    | BehaviorParams::CLONE
    | BehaviorParams::CONTEXT;

fn world_of(h: &Harness, id: u64) -> ConnId {
    *h.server
        .players
        .iter()
        .find(|(_, p)| p.character == id)
        .expect("in the world")
        .0
}

fn steps(seconds: f64) -> usize {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let n = (seconds / STEP).ceil() as usize;
    n
}

/// A player in a world whose catalog holds the tool as the install describes it.
fn world() -> (Harness, usize, u64) {
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    let mut props = Props::default();
    props
        .set(LOOK_ENTITY, Prop::Word(TOOL_ENTITY))
        .set(PILE_APPEARANCE, Prop::Word(TOOL_APPEARANCE));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(TOOL, 0x200, props));
    let (b, me) = h.player_in_world("one", "Elara");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
    (h, b, me)
}

/// A corpse beside `me` holding one item of `template`; the corpse and the item.
fn corpse_holding(h: &mut Harness, me: u64, template: u32) -> (u64, u64) {
    let t = h.server.catalog.get(TEST_ITEM).cloned().expect("template");
    let id = h.server.things.new_id();
    let mut c = Thing::of(id, &t, Kind::Corpse);
    c.at = Some(h.server.players[&world_of(h, me)].position);
    let item = inventory::make_item(&mut h.server, template, 1, 0).expect("made");
    h.server.things.get_mut(item).expect("item").container = Some(id);
    c.corpse = Some(Corpse {
        killer: me,
        contents: vec![item],
        killer_level: 1,
        expires: 1.0e12,
        ..Default::default()
    });
    h.server.things.insert(c);
    (id, item)
}

/// The behaviors `me` played toward `item` since `from`: (behavior, bits, script, copy, its
/// appearance).
fn played_toward(
    h: &Harness,
    b: usize,
    from: usize,
    me: u64,
    item: u64,
) -> Vec<(u32, u32, u32, u32, u32)> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayBehavior(e) if e.sender.id == me && e.action.target == item => {
                let a = &e.action;
                Some((
                    a.behavior,
                    a.bits,
                    a.fx_script,
                    a.clone_visual,
                    a.clone_appearance,
                ))
            }
            _ => None,
        })
        .collect()
}

/// The moves answered since `from`: each one's move type, look entity and item appearance.
fn answered(h: &Harness, b: usize, from: usize) -> Vec<(u32, u32, u32)> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c) if c.function.0 == MOVE_DONE => c
                .args
                .as_ref()
                .and_then(|a| calls::values(&a.0).ok())
                .and_then(|v| {
                    v.iter().find_map(|x: &Value| match x.root() {
                        Some(Node::Script { slots, .. }) => {
                            Some((slots[12].1, slots[38].1, slots[6].1))
                        }
                        _ => None,
                    })
                }),
            _ => None,
        })
        .collect()
}

/// The client's request to put `item` down from slot `slot`.
fn put_down(me: u64, item: u64, slot: i32) -> auberean_protocol::ClientMessage {
    let m = Move {
        move_type: DROP,
        merge: true,
        unlock: true,
        check_take: true,
        all_taken: true,
        grab: true,
        contain: true,
        auto_merge: true,
        from: me,
        actual_from: me,
        item,
        split_item: item,
        quantity: 1,
        quantity_left: 1,
        from_slot: slot,
        actual_from_slot: slot,
        ..Move::default()
    };
    server_call(drops::function::MOVE_ITEM, &[move_graph(&m)])
}

/// Taking an item out of a corpse, the player plays its taking behavior with a copy of the item in
/// its hand: the entity the item's template is drawn as and its appearance as it lies (2017: every
/// one of 1,407 takes of one item); the take is answered naming the item so.
///
/// Behaviour: server.hands.a-take-shows-the-item-in-the-hand
#[test]
fn an_item_taken_from_a_corpse_is_in_the_hand_as_its_template_draws_it() {
    let (mut h, b, me) = world();
    let (corpse, tool) = corpse_holding(&mut h, me, TOOL);
    h.ask(b, USE, &[use_of(corpse, me)], 5);
    let from = h.bots[b].inbox.len();
    h.ask(b, USE, &[use_of(tool, me)], 5);
    assert_eq!(
        played_toward(&h, b, from, me, tool),
        vec![(
            TAKING,
            IN_HAND,
            TAKE_ONE_SCRIPT,
            TOOL_ENTITY,
            TOOL_APPEARANCE
        )]
    );
    h.run(steps(inventory::TAKING_SECONDS + 0.5));
    assert_eq!(
        answered(&h, b, from),
        vec![(inventory::move_type::TAKE, TOOL_ENTITY, TOOL_APPEARANCE)]
    );
}

/// Putting an item down and picking it up again, the hand holds a copy of it as its template draws
/// it, and both moves are answered naming the item so; an item whose template names no entity
/// shows its look, with no appearance.
///
/// Behaviour: server.hands.a-pickup-and-a-put-down-show-the-item-in-the-hand
#[test]
fn an_item_put_down_and_picked_up_is_in_the_hand_as_its_template_draws_it() {
    let (mut h, b, me) = world();
    let world = world_of(&h, me);
    for (template, look, appearance) in [
        (TOOL, TOOL_ENTITY, TOOL_APPEARANCE),
        (TEST_ITEM, 0x1F00_0000 | (TEST_ITEM & 0xFFFF), 0),
    ] {
        let item = inventory::give(&mut h.server, &mut h.net, world, template, 1).expect("made");
        h.run(3);
        let slot = h.server.players[&world]
            .play
            .items
            .iter()
            .find(|x| x.id == item)
            .map_or(0, |x| i32::try_from(x.slot).unwrap_or(0));
        let from = h.bots[b].inbox.len();
        h.bots[b].send(true, queue::WEENIE, &put_down(me, item, slot));
        h.run(steps(2.5));
        h.bots[b].send(true, queue::WEENIE, &server_call(USE, &[use_of(item, me)]));
        h.run(steps(2.5));
        assert_eq!(
            played_toward(&h, b, from, me, item),
            vec![
                (
                    PUTTING_DOWN,
                    IN_HAND,
                    drops::look::PUT_DOWN_SCRIPT,
                    look,
                    appearance
                ),
                (TAKING, IN_HAND, PICK_UP_SCRIPT, look, appearance),
            ],
            "{template:08X}"
        );
        assert_eq!(
            answered(&h, b, from),
            vec![
                (DROP, look, appearance),
                (inventory::PICKED_UP, look, appearance)
            ],
            "{template:08X}"
        );
    }
}

/// The client's request to take everything from `corpse`, as the 2017 clients sent it.
fn take_all_request(corpse: u64, me: u64) -> auberean_script::args::Arg {
    use yalain_world::graph::{tag, Builder, Slot};
    #[allow(clippy::cast_possible_truncation)]
    let long = |v: u64| {
        [
            Slot::Tagged(tag::LONG, v as u32),
            Slot::Tagged(tag::HIGH, (v >> 32) as u32),
        ]
    };
    let mut slots = vec![
        Slot::Int(0),
        Slot::Int(0),
        Slot::Int(1),
        Slot::Int(0),
        Slot::Int(0),
        Slot::Int(0),
    ];
    slots.extend(long(corpse));
    slots.extend([Slot::Ref(None), Slot::Int(0), Slot::Int(0), Slot::Ref(None)]);
    slots.extend(long(me));
    let mut g = Builder::new();
    g.object(yalain_world::pack::TAKE_ALL_DESC, &slots);
    auberean_script::args::Arg::Object(g.graph())
}

/// The moves of the reorganizations answered since `from`: each one's look entity and item
/// appearance.
fn reorganized(h: &Harness, b: usize, from: usize) -> Vec<(u32, u32)> {
    h.calls(b, from)
        .into_iter()
        .filter(|(f, _)| *f == yalain_world::pack::function::REORGANIZE_DONE)
        .filter_map(|(_, v)| {
            v.iter().find_map(|x| match x.root() {
                Some(Node::Script { slots, .. }) => Some((slots[38].1, slots[6].1)),
                _ => None,
            })
        })
        .collect()
}

/// An item taken with everything else in a corpse, moved within the pack and traded to another
/// player is answered each time naming it as its template draws it: the entity it is drawn as and
/// its appearance as it lies.
///
/// Behaviour: server.hands.every-answer-names-the-item-as-its-template-draws-it
#[test]
fn an_item_taken_with_the_rest_moved_and_traded_is_named_as_its_template_draws_it() {
    use yalain_world::trade::function as trade;
    let (mut h, b, me) = world();
    let (corpse, tool) = corpse_holding(&mut h, me, TOOL);
    h.ask(b, USE, &[use_of(corpse, me)], 5);
    let from = h.bots[b].inbox.len();
    h.ask(
        b,
        yalain_world::pack::function::TAKE_ALL,
        &[take_all_request(corpse, me)],
        5 + steps(yalain_world::pack::TAKE_ALL_SECONDS),
    );
    assert_eq!(
        answered(&h, b, from),
        vec![(inventory::move_type::TAKE, TOOL_ENTITY, TOOL_APPEARANCE)],
        "taken with the rest"
    );
    let world = world_of(&h, me);
    let slot = h.server.players[&world]
        .play
        .items
        .iter()
        .find(|x| x.id == tool)
        .map(|x| i32::try_from(x.slot).unwrap_or(0))
        .expect("taken");
    let from = h.bots[b].inbox.len();
    let reorganize = Move {
        merge: true,
        actual_from_slot: slot,
        target: me,
        actual_target: me,
        unlock: true,
        target_slot: 0,
        check_take: true,
        from: me,
        all_taken: true,
        quantity_left: 1,
        quantity: 1,
        split_item: tool,
        merge_container: me,
        grab: true,
        actual_from: me,
        from_slot: slot,
        item: tool,
        contain: true,
        auto_merge: true,
        ..Move::default()
    };
    h.ask(
        b,
        yalain_world::pack::function::REORGANIZE,
        &[move_graph(&reorganize)],
        5,
    );
    assert_eq!(
        reorganized(&h, b, from),
        vec![(TOOL_ENTITY, TOOL_APPEARANCE)],
        "moved within the pack"
    );
    let (c, other) = h.player_in_world("two", "Bran");
    let at = h.server.players[&world].position;
    let theirs = world_of(&h, other);
    if let Some(p) = h.server.players.get_mut(&theirs) {
        p.position = at;
        p.position.origin[0] += 1.0;
    }
    h.ask(
        b,
        trade::OPEN,
        &[auberean_script::args::Arg::LongInt(other)],
        10,
    );
    h.ask(
        c,
        trade::OPEN,
        &[auberean_script::args::Arg::LongInt(me)],
        10,
    );
    h.ask(
        b,
        trade::OFFER,
        &[
            auberean_script::args::Arg::Int(1),
            auberean_script::args::Arg::LongInt(tool),
        ],
        10,
    );
    let from = h.bots[c].inbox.len();
    h.ask(b, trade::ACCEPT, &[], 10);
    h.ask(c, trade::ACCEPT, &[], 10);
    let traded = answered(&h, c, from);
    assert_eq!(traded.len(), 1, "traded: {traded:x?}");
    assert_eq!(
        (traded[0].1, traded[0].2),
        (TOOL_ENTITY, TOOL_APPEARANCE),
        "traded"
    );
}
