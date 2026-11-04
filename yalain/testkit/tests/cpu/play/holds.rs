//! How a thing is held: its template says the hands it may go to, those it rules out and where it
//! hangs, and a weapon's answer differs from a tool's or a shield's. Fixture: a player with made-up
//! templates in its pack, a sword in its right hand.

use auberean_protocol::ServerMessage;
use yalain_content::{Prop, Props};
use yalain_testkit::play::{server_call, use_of};
use yalain_testkit::Harness;
use yalain_world::calls::{self, Node, Value};
use yalain_world::equip::{self, flags, function, OFF_HAND, PRIMARY_HAND};
use yalain_world::skill_use::{implement as im, HeldVigor};

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
    let p = &h.server.players[&world];
    let made = yalain_world::inventory::held_create(
        &h.server.catalog,
        h.server.things.get(shield).expect("the shield"),
        p.instance,
    );
    assert_eq!(
        made.physics.parent.location, 2,
        "its first place: its own hand"
    );
    let attached = h.world_messages(b, from).iter().find_map(|m| match m {
        ServerMessage::Parent(e) if e.sender.id == shield => Some(e.action.location),
        _ => None,
    });
    assert_eq!(attached, Some(2), "attached there too");
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

/// A skill's cost: its own, what its weapons cost and what its shield costs, each unless the skill
/// ignores it; used on its user alone it costs the same.
/// Behaviour: server.specials.vigor-of-what-is-held
#[test]
fn a_skill_costs_its_own_and_what_the_hands_hold_unless_it_ignores_them() {
    use yalain_world::skill_use::{ui_flag, vigor_cost};
    let held = HeldVigor {
        weapons: 16,
        shield: 5,
    };
    let cost = |ui_flags: u32| {
        let u = yalain_world::skills::Usage {
            vigor_add: -100.0,
            ui_flags,
            ..yalain_world::skills::Usage::default()
        };
        vigor_cost(&u, 1000, held)
    };
    assert_eq!(cost(0), 121);
    assert_eq!(cost(ui_flag::SELF_TARGETED), 121, "on its user alone");
    assert_eq!(cost(ui_flag::IGNORE_WEAPON_VIGOR), 105);
    assert_eq!(cost(ui_flag::IGNORE_SHIELD_VIGOR), 116);
    assert_eq!(
        cost(ui_flag::IGNORE_WEAPON_VIGOR | ui_flag::IGNORE_SHIELD_VIGOR),
        100
    );
    let share = yalain_world::skills::Usage {
        vigor_mult: 0.5,
        ..yalain_world::skills::Usage::default()
    };
    assert_eq!(
        vigor_cost(&share, 1000, held),
        521,
        "a share of the greatest"
    );
}

/// Two weapons each cost theirs, by the item's own number where it has one, a template's or a
/// starting weapon's otherwise, a shield its own beside them.
/// Behaviour: server.specials.vigor-of-what-is-held
#[test]
fn a_use_pays_for_each_weapon_and_the_shield_by_their_own_numbers() {
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    let (mut right, mut left) = (
        held(SWORD, (192, 64, 0), Some((1, 2)), Some(0x4100_000A)),
        held(HANDBLADE, (192, 64, 0), Some((1, 2)), Some(0x4100_000F)),
    );
    right.props.set("Weapon_VigorCost", Prop::Int(18));
    left.props.set("Weapon_VigorCost", Prop::Int(7));
    let mut shield = held(SHIELD, (128, 128, 0), Some((2, 1)), None);
    shield.props.set("Weapon_VigorCost", Prop::Int(3));
    for t in [right, left, shield] {
        h.server.catalog.insert(t);
    }
    h.server.catalog.implements.insert(SWORD, im::SWORD);
    h.server.catalog.implements.insert(HANDBLADE, im::HANDBLADE);
    h.server.catalog.implements.insert(SHIELD, im::SHIELD);
    let (_, me) = h.player_in_world("one", "Elara");
    let world = *h.server.players.keys().next().expect("a player");
    h.server.players.get_mut(&world).unwrap().play.items.clear();
    let cost = |h: &Harness| yalain_world::skill_use::held_vigor(&h.server, me);
    let first = hold(&mut h, world, SWORD, PRIMARY_HAND);
    assert_eq!(
        cost(&h),
        HeldVigor {
            weapons: 18,
            shield: 0
        },
        "its template's"
    );
    if let Some(t) = h.server.things.get_mut(first) {
        t.own.made = Some(yalain_world::made::Made {
            vigor: Some(25),
            ..yalain_world::made::Made::default()
        });
    }
    assert_eq!(cost(&h).weapons, 25, "the item's own");
    let second = hold(&mut h, world, HANDBLADE, OFF_HAND);
    assert_eq!(cost(&h).weapons, 32, "both weapons'");
    let p = h.server.players.get_mut(&world).unwrap();
    p.play.items.retain(|i| i.id != second);
    hold(&mut h, world, SHIELD, OFF_HAND);
    assert_eq!(
        cost(&h),
        HeldVigor {
            weapons: 25,
            shield: 3
        },
        "and a shield's"
    );
    let p = h.server.players.get_mut(&world).unwrap();
    p.play.items.clear();
    h.server.catalog.insert(yalain_testkit::test_template(
        0x4700_051C,
        0x1067,
        Props::default(),
    ));
    hold(&mut h, world, 0x4700_051C, PRIMARY_HAND);
    assert_eq!(cost(&h).weapons, 4, "a starting weapon's");
}

/// A thing made of a template, placed straight at a place on the player.
fn hold(h: &mut Harness, world: yalain_net::ConnId, template: u32, location: u32) -> u64 {
    let id = yalain_world::inventory::make_item(&mut h.server, template, 1, 0).expect("made");
    let p = h.server.players.get_mut(&world).unwrap();
    let slot = u32::try_from(p.play.items.len()).unwrap_or(0);
    p.play.items.push(yalain_world::state::Held {
        id,
        template,
        slot,
        location,
        quantity: 1,
    });
    id
}

/// An item that names no hand or body place, and isn't known as a weapon, is refused by its use and
/// stays in the pack, nothing wielded; the same template known as a weapon is wielded.
/// Behaviour: server.inventory.use-with-no-place-refused
#[test]
fn an_item_with_no_place_is_refused_by_its_use_and_stays_in_the_pack() {
    let (mut h, b, me, world) = player();
    let tool = yalain_testkit::test_template(0x4700_7B07, 0x200, Props::default());
    h.server.catalog.insert(tool);
    yalain_world::inventory::give(&mut h.server, &mut h.net, world, 0x4700_7B07, 1);
    h.run(3);
    let item = id_of(&h, world, 0x4700_7B07);
    let from = h.bots[b].inbox.len();
    use_it(&mut h, b, me, item);
    let at = |h: &Harness| {
        h.server.players[&world]
            .play
            .items
            .iter()
            .find(|i| i.id == item)
            .expect("still held")
            .location
    };
    assert_eq!(at(&h), 0, "in the pack");
    assert!(answers(&h, b, from).is_empty(), "no equip answer");
    let told: Vec<u32> = h
        .world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c)
                if c.function.0 == yalain_world::places::function::USE_DONE =>
            {
                let v = calls::values(&c.args.as_ref()?.0).ok()?;
                let Value::Graph(g) = v.first()? else {
                    return None;
                };
                g.iter().find_map(|(_, n)| match n {
                    Node::Script {
                        package: 0x245,
                        slots,
                    } => slots.get(11).map(|(_, s)| *s),
                    _ => None,
                })
            }
            _ => None,
        })
        .collect();
    assert_eq!(told, [10_001], "cannot be used");
    h.server.catalog.implements.insert(0x4700_7B07, im::SWORD);
    use_it(&mut h, b, me, item);
    assert_eq!(at(&h), equip::SWORD.preferred, "known as a weapon");
}

/// A player's combat mode is the stance the right hand's template names for what the left holds; a
/// stave that names the sword's stance is fought in that, not the stave's.
/// Behaviour: server.weapons.mode-by-stance
#[test]
fn the_combat_mode_is_the_stance_the_right_hands_template_names_for_the_left() {
    const WAND: u32 = 0x4700_7B06;
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    let mut wand = held(WAND, (192, 64, 0), Some((1, 2)), Some(0x4100_000A));
    wand.props
        .set("Weapon_ShieldStance", Prop::Word(0x4100_000B))
        .set("Weapon_TwoWeaponStance", Prop::Word(0x4100_000E));
    h.server.catalog.insert(wand);
    let second = held(HANDBLADE, (192, 64, 0), Some((1, 2)), Some(0x4100_000F));
    let shield = held(SHIELD, (128, 128, 0), Some((2, 1)), None);
    h.server.catalog.insert(second);
    h.server.catalog.insert(shield);
    h.server.catalog.implements.insert(WAND, im::STAVE);
    h.server.catalog.implements.insert(HANDBLADE, im::HANDBLADE);
    h.server.catalog.implements.insert(SHIELD, im::SHIELD);
    let (_, me) = h.player_in_world("one", "Elara");
    let world = *h.server.players.keys().next().expect("a player");
    h.server.players.get_mut(&world).unwrap().play.items.clear();
    hold(&mut h, world, WAND, PRIMARY_HAND);
    let mode = |h: &Harness| yalain_world::skill_use::player_mode(&h.server, me);
    assert_eq!(mode(&h), 0x4100_000A, "alone");
    let other = hold(&mut h, world, HANDBLADE, OFF_HAND);
    assert_eq!(mode(&h), 0x4100_000E, "with a second weapon");
    let p = h.server.players.get_mut(&world).unwrap();
    p.play.items.retain(|i| i.id != other);
    hold(&mut h, world, SHIELD, OFF_HAND);
    assert_eq!(mode(&h), 0x4100_000B, "with a shield");
}

/// A template that names no hand is held as a sword, not nowhere.
/// Behaviour: server.weapons.hold-from-template
#[test]
fn a_template_that_names_no_hand_is_held_as_a_sword() {
    let mut catalog = yalain_content::Catalog::default();
    catalog.insert(held(TOOL, (0, 0, 0), None, None));
    assert_eq!(equip::hold_of(&catalog, TOOL), equip::SWORD);
}

/// A wielded thing put away into a pack with every segment full goes to the pack's front like
/// anything else past them: it is in the pack itself, with nothing lost of what was there.
/// Behaviour: server.inventory.past-every-segment-the-pack-itself-grows
#[test]
fn a_weapon_put_away_past_every_segment_goes_to_the_packs_front() {
    use yalain_world::inventory;
    let (mut h, b, me, world) = player();
    let sword = id_of(&h, world, SWORD);
    use_it(&mut h, b, me, sword);
    let in_pack = |h: &Harness| {
        h.server.players[&world]
            .play
            .items
            .iter()
            .filter(|x| x.location == 0)
            .count()
    };
    while in_pack(&h) < inventory::PACK_ROOM as usize {
        inventory::give(&mut h.server, &mut h.net, world, TOOL, 1);
    }
    h.run(2);
    use_it(&mut h, b, me, sword);
    let items = &h.server.players[&world].play.items;
    let put = items.iter().find(|x| x.id == sword).expect("held");
    assert_eq!((put.location, put.slot), (0, 0), "at the front");
    assert_eq!(
        inventory::pack_segments(items),
        [(42, 43), (12, 12), (12, 12), (12, 12)]
    );
    let mut slots: Vec<u32> = items.iter().map(|x| x.slot).collect();
    slots.sort_unstable();
    slots.dedup();
    assert_eq!(slots.len(), items.len(), "every slot its own item's");
}
