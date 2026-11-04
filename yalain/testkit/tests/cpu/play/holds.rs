//! How a thing is held: its template says the hands it may go to, those it rules out and where it
//! hangs, and a weapon's answer differs from a tool's or a shield's. Fixture: a player with made-up
//! templates in its pack, a sword in its right hand.

use auberean_protocol::ServerMessage;
use yalain_content::{Prop, Props};
use yalain_testkit::play::{server_call, use_of};
use yalain_testkit::Harness;
use yalain_world::calls::{self, Node, Value};
use yalain_world::equip::{self, flags, function, OFF_HAND, PRIMARY_HAND};
use yalain_world::skill_use::implement as im;

const BOW: u32 = 0x4700_7B01;
const SWORD: u32 = 0x4700_7B02;
const SHIELD: u32 = 0x4700_7B03;
const HANDBLADE: u32 = 0x4700_7B04;
const TOOL: u32 = 0x4700_7B05;
const BOW_STANCE: u32 = 0x4100_0010;

/// A template that names its places and how it hangs: valid, preferred, ruled out, then its two
/// parenting places and its stance when it has them.
fn held(
    t: u32,
    places: (u32, u32, u32),
    hang: Option<(u32, u32)>,
    stance: Option<u32>,
) -> yalain_content::Template {
    let mut p = Props::default();
    p.set(
        "Item_ValidInventoryLocations",
        Prop::Int(i64::from(places.0)),
    )
    .set("Item_PreferredInventoryLocation", Prop::Word(places.1))
    .set(
        "Item_PrecludedInventoryLocations",
        Prop::Int(i64::from(places.2)),
    );
    if let Some((a, b)) = hang {
        p.set("Item_PrimaryParentingLocation", Prop::Word(a))
            .set("Item_SecondaryParentingLocation", Prop::Word(b));
    }
    if let Some(s) = stance {
        p.set("Weapon_SingleWeaponStance", Prop::Word(s));
    }
    yalain_testkit::test_template(t, 0x1067, p)
}

/// A player with one of each in its pack: its client, id and connection.
fn player() -> (Harness, usize, u64, yalain_net::ConnId) {
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    for t in [
        held(BOW, (64, 64, 128), Some((2, 1)), Some(BOW_STANCE)),
        held(
            SWORD,
            (192, 64, 0),
            Some((1, 0x4000_0006)),
            Some(0x4100_000A),
        ),
        held(SHIELD, (128, 128, 0), Some((2, 1)), None),
        held(HANDBLADE, (192, 64, 0), Some((1, 0x11)), Some(0x4100_000F)),
        held(TOOL, (64, 64, 128), None, None),
    ] {
        h.server.catalog.insert(t);
    }
    let (b, me) = h.player_in_world("one", "Elara");
    let world = *h.server.players.keys().next().expect("a player");
    h.server.players.get_mut(&world).unwrap().play.items.clear();
    for t in [BOW, SWORD, SHIELD, HANDBLADE, TOOL] {
        yalain_world::inventory::give(&mut h.server, &mut h.net, world, t, 1);
    }
    h.run(3);
    (h, b, me, world)
}

fn id_of(h: &Harness, world: yalain_net::ConnId, t: u32) -> u64 {
    h.server.players[&world]
        .play
        .items
        .iter()
        .find(|i| i.template == t)
        .expect("given")
        .id
}

fn use_it(h: &mut Harness, b: usize, me: u64, item: u64) {
    h.bots[b].send(
        true,
        auberean_protocol::queue::WEENIE,
        &server_call(yalain_world::requests::function::USE, &[use_of(item, me)]),
    );
    h.run(10);
}

/// The equipping answers since `from`: wielding, the description's slots, the profile's.
fn answers(h: &Harness, b: usize, from: usize) -> Vec<(bool, Vec<u32>, Vec<u32>)> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c)
                if c.function.0 == function::EQUIP_DONE
                    || c.function.0 == function::UNEQUIP_DONE =>
            {
                let v = calls::values(&c.args.as_ref()?.0).ok()?;
                let Value::Graph(g) = v.first()? else {
                    return None;
                };
                let slots = |pkg: u16| {
                    g.iter().find_map(|(_, n)| match n {
                        Node::Script { package, slots } if *package == pkg => {
                            Some(slots.iter().map(|(_, x)| *x).collect::<Vec<u32>>())
                        }
                        _ => None,
                    })
                };
                Some((
                    c.function.0 == function::EQUIP_DONE,
                    slots(equip::EQUIP_DESC.0)?,
                    slots(equip::EQUIP_PROFILE.0)?,
                ))
            }
            _ => None,
        })
        .collect()
}

/// A bow takes both hands whatever made it: the answer says so, it hangs where its template says,
/// and what is in either hand makes room for it.
/// Behaviour: server.weapons.hold-from-template
#[test]
fn a_looted_bow_takes_both_hands_and_hangs_as_its_template_says() {
    let (mut h, b, me, world) = player();
    let (sword, bow) = (id_of(&h, world, SWORD), id_of(&h, world, BOW));
    use_it(&mut h, b, me, sword);
    let from = h.bots[b].inbox.len();
    use_it(&mut h, b, me, bow);
    let a = answers(&h, b, from);
    let (_, d, f) = a.last().expect("an answer");
    assert_eq!(
        (d[14], d[5], d[7], d[12]),
        (192, 128, PRIMARY_HAND, flags::WIELDED),
        "taken, ruled out, where, flags"
    );
    assert_eq!(
        (f[1], f[2], f[3], f[5]),
        (2, 1, 64, 64),
        "its hang and places"
    );
    let at = |h: &Harness, id: u64| {
        h.server.players[&world]
            .play
            .items
            .iter()
            .find(|i| i.id == id)
            .map(|i| i.location)
    };
    assert_eq!(at(&h, bow), Some(192), "both hands");
    assert_eq!(at(&h, sword), Some(0), "the sword made room");
}

/// A weapon of another kind takes only its hand, and hangs from its own places.
/// Behaviour: server.weapons.hold-from-template
#[test]
fn a_one_handed_weapon_says_both_its_hands_may_hold_it() {
    let (mut h, b, me, world) = player();
    let blade = id_of(&h, world, HANDBLADE);
    let from = h.bots[b].inbox.len();
    use_it(&mut h, b, me, blade);
    let a = answers(&h, b, from);
    let (_, d, f) = a.last().expect("an answer");
    assert_eq!((d[14], d[5], d[12]), (64, 0, flags::WIELDED));
    assert_eq!((f[1], f[2], f[3], f[5]), (1, 0x11, 192, 64));
}

/// What is not a weapon is answered as worn: a shield goes to the off hand, a tool with no places
/// of its own to hang from hangs from the first.
/// Behaviour: server.weapons.hold-from-template
#[test]
fn a_shield_and_a_tool_are_answered_as_worn() {
    let (mut h, b, me, world) = player();
    let shield = id_of(&h, world, SHIELD);
    let from = h.bots[b].inbox.len();
    use_it(&mut h, b, me, shield);
    let a = answers(&h, b, from);
    let (_, d, f) = a.last().expect("an answer");
    assert_eq!((d[7], d[14], d[12]), (OFF_HAND, OFF_HAND, flags::WORN));
    assert_eq!((f[1], f[2], f[3], f[5]), (2, 1, 128, 128));
    let tool = id_of(&h, world, TOOL);
    let from = h.bots[b].inbox.len();
    use_it(&mut h, b, me, tool);
    let a = answers(&h, b, from);
    let (_, d, f) = a.last().expect("an answer");
    assert_eq!(
        (d[7], d[14], d[5], d[12]),
        (PRIMARY_HAND, 192, 128, flags::WORN)
    );
    assert_eq!((f[1], f[2]), (1, 1));
}

/// A bow is made strung however it came, and nothing else is.
/// Behaviour: server.weapons.bow-made-strung
#[test]
fn a_looted_bow_is_made_strung() {
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    let mut bow = held(BOW, (64, 64, 128), Some((2, 1)), Some(BOW_STANCE));
    bow.props
        .set("Item_ImplementType", Prop::Int(i64::from(im::BOW)));
    let mut sword = held(SWORD, (192, 64, 0), Some((1, 0x11)), Some(0x4100_000A));
    sword
        .props
        .set("Item_ImplementType", Prop::Int(i64::from(im::SWORD)));
    h.server.catalog.insert(bow);
    h.server.catalog.insert(sword);
    let (b, _) = h.player_in_world("one", "Elara");
    let world = *h.server.players.keys().next().expect("a player");
    let from = h.bots[b].inbox.len();
    for t in [BOW, SWORD] {
        yalain_world::inventory::give(&mut h.server, &mut h.net, world, t, 1);
    }
    h.run(3);
    let made: Vec<(u32, u32)> = h
        .world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::CreateObject(o) => Some((o.weenie.entity_did, o.physics.mode)),
            _ => None,
        })
        .collect();
    assert_eq!(made, [(BOW, 0x4100_0010), (SWORD, 0x4000_0001)]);
}
