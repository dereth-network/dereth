//! The paper doll: an item dragged onto it is worn or wielded at the place the client names (a
//! ring on either hand, a weapon in the off hand), whatever was there taken off to where the item
//! was in the pack, the answers flagged as directives; an item dragged off it goes to the pack slot
//! the client names, a weapon's slider moved to rest only for the primary hand. An item the player
//! may not put on is answered as the directive it asked for, with the refusal for its status.
//! Fixture: a new player given two made-up rings and a made-up sword by the world.

use auberean_protocol::ServerMessage;
use auberean_script::args::Arg;
use yalain_content::{Prop, Props};
use yalain_net::ConnId;
use yalain_testkit::{test_catalog, Harness};
use yalain_world::calls::Node;
use yalain_world::equip::{self, function};

const RING: u32 = 0x4700_10A0;
const OTHER_RING: u32 = 0x4700_10A3;
const SWORD: u32 = 0x4700_0746;
const LEFT_RING: u32 = 256;

/// The equipping description a client sends: the item, the place, the pack slot it names as the
/// target, and the item's slot.
fn directive(item: u64, me: u64, location: u32, target: u32, slot: u32) -> Arg {
    use yalain_world::graph::{Builder, Slot};
    let mut b = Builder::new();
    b.object(
        equip::EQUIP_DESC.0,
        &[
            Slot::Int(0),
            Slot::Ref(None),
            Slot::Int(1),
            Slot::Long(item),
            Slot::Int(0),
            Slot::Int(target),
            Slot::Int(location),
            Slot::Long(me),
            Slot::Long(me),
            Slot::Int(158),
            Slot::Int(0),
            Slot::Int(0),
            Slot::Int(location),
            Slot::Long(0),
            Slot::Int(slot),
        ],
    );
    Arg::Object(b.graph())
}

/// A player with two rings and a sword in its pack: its client, id, connection and the items.
fn player() -> (Harness, usize, u64, ConnId, [u64; 3]) {
    let mut h = Harness::new();
    h.server.catalog = test_catalog();
    for t in [RING, OTHER_RING] {
        let mut p = Props::default();
        p.set("Item_ValidInventoryLocations", Prop::Word(768))
            .set("Item_PreferredInventoryLocation", Prop::Word(LEFT_RING));
        h.server
            .catalog
            .insert(yalain_testkit::test_template(t, 0x200, p));
    }
    h.server.catalog.insert(yalain_testkit::test_template(
        SWORD,
        0x1067,
        Props::default(),
    ));
    let (b, me) = h.player_in_world("one", "Elara");
    let world = *h.server.players.keys().next().expect("a player");
    h.server.players.get_mut(&world).unwrap().play.items.clear();
    for t in [SWORD, OTHER_RING, RING] {
        yalain_world::inventory::give(&mut h.server, &mut h.net, world, t, 1);
    }
    h.run(3);
    let id = |t: u32| {
        h.server.players[&world]
            .play
            .items
            .iter()
            .find(|i| i.template == t)
            .expect("given")
            .id
    };
    let items = [id(RING), id(OTHER_RING), id(SWORD)];
    (h, b, me, world, items)
}

/// The equipping answers since `from`: (function, item, place, flags, status, target slot, slot,
/// the item in the way).
type Answer = (u32, u64, u32, u32, u32, u32, u32, u64);

fn answers(h: &Harness, b: usize, from: usize) -> Vec<Answer> {
    h.calls(b, from)
        .into_iter()
        .filter(|(f, _)| *f == function::EQUIP_DONE || *f == function::UNEQUIP_DONE)
        .filter_map(|(f, v)| {
            let Some(Node::Script { slots, .. }) = v.first()?.root() else {
                return None;
            };
            let long = |i: usize| u64::from(slots[i + 1].1) << 32 | u64::from(slots[i].1);
            Some((
                f,
                long(3),
                slots[7].1,
                slots[12].1,
                slots[13].1,
                slots[6].1,
                slots[18].1,
                long(16),
            ))
        })
        .collect()
}

fn slot_of(h: &Harness, world: ConnId, item: u64) -> (u32, u32) {
    let i = h.server.players[&world]
        .play
        .items
        .iter()
        .find(|i| i.id == item)
        .expect("held");
    (i.location, i.slot)
}

/// A ring dragged to the doll goes to the hand named; a second dragged to that hand takes the
/// first off to where the second was in the pack (answered as made room), and is answered as a
/// directive naming the ring it replaced, at its slot one further on.
/// Behaviour: server.inventory.paper-doll-equips-at-the-place-named
#[test]
fn a_ring_dragged_to_the_doll_goes_on_the_hand_named_and_takes_off_the_one_there() {
    let (mut h, b, me, world, [ring, other, _]) = player();
    let at = slot_of(&h, world, ring).1;
    let from = h.bots[b].inbox.len();
    h.ask(
        b,
        function::DIRECTIVE_EQUIP,
        &[directive(ring, me, 512, 0, at)],
        3,
    );
    assert_eq!(
        answers(&h, b, from),
        [(
            function::EQUIP_DONE,
            ring,
            512,
            equip::flags::DIRECTED,
            0,
            0,
            at,
            0
        )],
        "on the hand named, not the one it prefers"
    );
    let (_, other_at) = slot_of(&h, world, other);
    let from = h.bots[b].inbox.len();
    h.ask(
        b,
        function::DIRECTIVE_EQUIP,
        &[directive(other, me, 512, 0, other_at)],
        3,
    );
    assert_eq!(
        answers(&h, b, from),
        [
            (
                function::UNEQUIP_DONE,
                ring,
                512,
                equip::flags::MADE_ROOM,
                0,
                other_at,
                other_at,
                0
            ),
            (
                function::EQUIP_DONE,
                other,
                512,
                equip::flags::DIRECTED,
                0,
                0,
                other_at + 1,
                ring
            ),
        ]
    );
    assert_eq!(
        slot_of(&h, world, ring),
        (0, other_at),
        "where the other was"
    );
    assert_eq!(slot_of(&h, world, other).0, 512);
}

/// A sword dragged to the off hand is wielded there with no weapon slider; dragged off it goes to
/// the pack slot named. A second sword dragged to a hand that holds one takes the first off to
/// where the second was in the pack.
/// Behaviour: server.inventory.paper-doll-takes-off-to-the-slot-named
#[test]
fn a_sword_dragged_to_the_off_hand_and_off_again_goes_to_the_slot_named() {
    let (mut h, b, me, world, [_, _, first]) = player();
    yalain_world::inventory::give(&mut h.server, &mut h.net, world, SWORD, 1);
    h.run(3);
    let second = h.server.players[&world]
        .play
        .items
        .iter()
        .find(|i| i.template == SWORD && i.id != first)
        .expect("a second sword")
        .id;
    let at = slot_of(&h, world, first).1;
    let from = h.bots[b].inbox.len();
    h.ask(
        b,
        function::DIRECTIVE_EQUIP,
        &[directive(first, me, equip::OFF_HAND, 0, at)],
        3,
    );
    let a = answers(&h, b, from);
    assert_eq!(a.len(), 1);
    assert_eq!(
        (a[0].2, a[0].3),
        (equip::OFF_HAND, equip::flags::DIRECTED),
        "in the off hand, as a directive"
    );
    let slid = |h: &Harness, from: usize| {
        h.world_messages(b, from)
            .iter()
            .any(|m| matches!(m, ServerMessage::MoveSlider(_)))
    };
    assert!(!slid(&h, from), "no slider for the off hand");
    // The second, at the pack's front, to the same hand: the first comes off to the front.
    let at = slot_of(&h, world, second).1;
    let from = h.bots[b].inbox.len();
    h.ask(
        b,
        function::DIRECTIVE_EQUIP,
        &[directive(second, me, equip::OFF_HAND, 0, at)],
        3,
    );
    let a = answers(&h, b, from);
    assert_eq!(a.len(), 2, "the first taken off, the second wielded");
    assert_eq!(
        (a[0].0, a[0].1, a[0].3, a[0].5),
        (function::UNEQUIP_DONE, first, equip::flags::MADE_ROOM, at),
        "to where the second was"
    );
    assert_eq!(slot_of(&h, world, first), (0, at));
    assert_eq!(
        (a[1].2, a[1].3, a[1].7),
        (equip::OFF_HAND, equip::flags::DIRECTED, first),
        "naming the one it replaced"
    );
    assert_eq!(slot_of(&h, world, second).0, equip::OFF_HAND);
    let from = h.bots[b].inbox.len();
    h.ask(
        b,
        function::DIRECTIVE_UNEQUIP,
        &[directive(second, me, equip::OFF_HAND, 1, 0)],
        3,
    );
    let a = answers(&h, b, from);
    assert_eq!(
        a.iter().map(|x| (x.0, x.3, x.5)).collect::<Vec<_>>(),
        [(function::UNEQUIP_DONE, equip::flags::TAKEN_OFF, 1)],
        "taken off to slot 1"
    );
    assert!(!slid(&h, from));
    assert_eq!(slot_of(&h, world, second), (0, 1));
}

/// A ring that names a race other than the player's, dragged to the doll, is answered as the
/// directive it asked for with the wrong race for its status (10015), at the place named and the
/// ring's slot, and stays in the pack.
/// Behaviour: server.equip.requirements-refuse-directives
#[test]
fn a_ring_for_another_race_dragged_to_the_doll_is_refused_and_stays_in_the_pack() {
    let (mut h, b, me, world, [ring, _, _]) = player();
    let race = h.server.players[&world].race;
    let mut t = h.server.catalog.get(RING).expect("the ring").clone();
    t.props.set("Usage_RequiredRace", Prop::Word(!race & 0x7));
    h.server.catalog.insert(t);
    let at = slot_of(&h, world, ring).1;
    let from = h.bots[b].inbox.len();
    h.ask(
        b,
        function::DIRECTIVE_EQUIP,
        &[directive(ring, me, 512, 0, at)],
        3,
    );
    assert_eq!(
        answers(&h, b, from),
        [(
            function::EQUIP_DONE,
            ring,
            512,
            equip::flags::DIRECTED,
            equip::refusal::WRONG_RACE,
            0,
            at,
            0
        )]
    );
    assert_eq!(slot_of(&h, world, ring), (0, at), "still in the pack");
}

/// A weapon in the off hand is listed there at entry, taking that hand alone, its fellow in the
/// primary hand; and it hangs from its secondary place.
/// Behaviour: server.inventory.off-hand-listed-at-entry
#[test]
fn a_weapon_in_the_off_hand_stays_there_across_a_login() {
    let (mut h, b, me, world, [_, _, first]) = player();
    yalain_world::inventory::give(&mut h.server, &mut h.net, world, SWORD, 1);
    h.run(3);
    let second = h.server.players[&world]
        .play
        .items
        .iter()
        .find(|i| i.template == SWORD && i.id != first)
        .expect("a second sword")
        .id;
    for (item, hand) in [(first, equip::PRIMARY_HAND), (second, equip::OFF_HAND)] {
        let at = slot_of(&h, world, item).1;
        h.ask(
            b,
            function::DIRECTIVE_EQUIP,
            &[directive(item, me, hand, 0, at)],
            3,
        );
    }
    let p = &h.server.players[&world];
    let worn = yalain_world::inventory::worn(&h.server.catalog, &p.play.items, &[], 1, 1);
    let listed = |item: u64| {
        worn.iter()
            .find(|w| w.item == item)
            .map(|w| (w.location, w.taken))
    };
    assert_eq!(
        listed(first),
        Some((equip::PRIMARY_HAND, equip::PRIMARY_HAND))
    );
    assert_eq!(listed(second), Some((equip::OFF_HAND, equip::OFF_HAND)));
    let instance = p.instance;
    let hang = |item: u64| {
        let t = h.server.things.get(item).expect("the item");
        yalain_world::inventory::held_create(t, instance)
            .physics
            .parent
            .location
    };
    assert_eq!(hang(first), equip::SWORD.primary);
    assert_eq!(hang(second), equip::SWORD.secondary);
}
