//! Items that bind when used in combat: a made item a recipe binds is equipped with a warning, at
//! every equip, until a blow binds it to its holder, who is told; from then on nobody else may
//! equip it.

use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_content::{Prop, Props};
use yalain_testkit::play::{server_call, use_of};
use yalain_testkit::{Harness, TEST_CREATURE};
use yalain_world::calls::{self, Node, Value};
use yalain_world::equip::{self, BINDS_WARNING, BOUND_TEXT, BOUND_TO_ANOTHER, DISPLAY_MESSAGE};

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

/// How many times since `from` a client was told an item is bound to it.
fn told_bound(h: &Harness, b: usize, from: usize) -> usize {
    h.calls(b, from)
        .iter()
        .filter(|(f, args)| {
            *f == yalain_world::session::function::SHOW_TEXT
                && args.iter().any(|v| {
                    matches!(v.root(), Some(Node::TextInfo(t))
                        if (t.string_id, t.table) == BOUND_TEXT && !t.variables.is_empty())
                })
        })
        .count()
}

/// The player is hit once.
fn hit(h: &mut Harness, beetle: u64) {
    let world = *h.server.players.keys().next().expect("in the world");
    let clock = h.clock();
    yalain_world::combat::after_hurt(&mut h.server, &mut h.net, world, beetle, clock);
}

/// An item that binds when used in combat is answered with the mark in its profile and the
/// warning at every equip, not shown on top; once a blow has bound it, its holder is told once,
/// naming it, and it is equipped again with no mark and no warning. An item without the mark is
/// never warned.
/// Behaviour: server.items.binds-on-use-warning
#[test]
fn an_item_that_binds_in_combat_warns_at_every_equip_until_a_blow_binds_it() {
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
    // A blow binds it, and says so.
    let from = h.bots[b].inbox.len();
    hit(&mut h, beetle);
    h.run(2);
    assert!(warnings(&h, b, from).is_empty());
    assert_eq!(told_bound(&h, b, from), 1);
    assert!(h.server.things.get(chest).is_some_and(|t| !t.binds_on_use));
    assert_eq!(h.server.things.get(chest).map(|t| t.own.bound_to), Some(me));
    used(&mut h, b, me, chest);
    let from = used(&mut h, b, me, chest);
    assert_eq!(binds_words(&h, b, from), [0], "bound: no mark");
    assert!(warnings(&h, b, from).is_empty(), "nor a warning");
}

/// Being hit binds a marked piece worn; what a player only carries keeps the mark.
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
    hit(&mut h, beetle);
    assert!(
        h.server.things.get(chest).is_some_and(|t| !t.binds_on_use),
        "worn: bound"
    );
    assert!(
        h.server.things.get(carried).is_some_and(|t| t.binds_on_use),
        "carried: still marked"
    );
}

/// A blow lands on one piece: of two marked pieces worn, a hit binds at most one, and hits go on
/// until both are bound, each told once.
/// Behaviour: server.items.a-blow-binds-the-piece-it-lands-on
#[test]
fn a_hit_binds_one_piece_of_what_is_worn_each_told_once() {
    const HEAD: u32 = 0x4700_7302;
    let (mut h, b, me, beetle, chest) = marked_chest();
    let mut props = Props::default();
    props
        .set("Item_ValidInventoryLocations", Prop::Word(1))
        .set("Item_PreferredInventoryLocation", Prop::Word(1));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(HEAD, 0x180, props));
    let helm = h.arm(b, me, HEAD);
    if let Some(t) = h.server.things.get_mut(helm) {
        t.binds_on_use = true;
    }
    used(&mut h, b, me, chest);
    let from = h.bots[b].inbox.len();
    let marked = |h: &Harness| {
        [chest, helm]
            .iter()
            .filter(|i| h.server.things.get(**i).is_some_and(|t| t.binds_on_use))
            .count()
    };
    let mut hits = 0;
    while marked(&h) > 0 {
        let before = marked(&h);
        hit(&mut h, beetle);
        assert!(before - marked(&h) <= 1, "one piece a blow");
        hits += 1;
        assert!(hits < 200, "both bind in the end");
    }
    h.run(2);
    assert_eq!(told_bound(&h, b, from), 2);
}

/// Starting a fight binds nothing; what is held binds in the end as its blows land, and armor is
/// not bound by the blows its wearer strikes.
/// Behaviour: server.items.a-weapon-binds-as-its-blows-land
#[test]
fn a_weapon_binds_as_its_blows_land_and_armor_does_not() {
    let (mut h, b, me, beetle, chest) = marked_chest();
    used(&mut h, b, me, chest);
    let sword = h
        .server
        .players
        .values()
        .flat_map(|p| p.play.items.iter())
        .find(|i| i.template == SWORD)
        .map(|i| i.id)
        .expect("in hand");
    if let Some(t) = h.server.things.get_mut(sword) {
        t.binds_on_use = true;
    }
    // In reach, and whole for as long as this takes.
    let at = h.server.players.values().next().map(|p| p.position);
    if let Some(t) = h.server.things.get_mut(beetle) {
        t.health = 1_000_000;
        t.at = at;
    }
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(START_ATTACK, &[Arg::LongInt(beetle)]),
    );
    h.run(1);
    assert!(
        h.server.things.get(sword).is_some_and(|t| t.binds_on_use),
        "starting binds nothing"
    );
    assert!(
        h.run_until(40_000, |h| h
            .server
            .things
            .get(sword)
            .is_some_and(|t| !t.binds_on_use)),
        "its blows bind it in the end"
    );
    assert_eq!(h.server.things.get(sword).map(|t| t.own.bound_to), Some(me));
    assert!(h.server.things.get(chest).is_some_and(|t| t.binds_on_use));
}

/// A blow binds what is held one time in ten, marked to; the armor its wearer has on stays marked,
/// however many blows it strikes.
/// Behaviour: server.items.a-weapon-binds-as-its-blows-land
#[test]
fn a_blow_binds_what_is_held_one_time_in_ten_and_never_what_is_worn() {
    let (mut h, b, me, _, chest) = marked_chest();
    used(&mut h, b, me, chest);
    let world = *h.server.players.keys().next().expect("in the world");
    let sword = h.server.players[&world]
        .play
        .items
        .iter()
        .find(|i| i.template == SWORD)
        .map(|i| i.id)
        .expect("in hand");
    let blows = 4000;
    let mut bound = 0;
    for _ in 0..blows {
        h.server.things.get_mut(sword).expect("held").binds_on_use = true;
        equip::blow_landed(&mut h.server, &mut h.net, world);
        bound += i32::from(!h.server.things.get(sword).expect("held").binds_on_use);
    }
    let share = f64::from(bound) / f64::from(blows);
    assert!((share - 0.1).abs() < 0.02, "{share}");
    assert!(h.server.things.get(chest).is_some_and(|t| t.binds_on_use));
}

/// A marked item's examination says it binds; once bound it says so instead, to its holder as bound
/// to it, to anyone else as bound to another, and whichever of them holds it now.
/// Behaviour: server.examination.an-item-says-whom-it-is-bound-to
#[test]
fn an_items_examination_says_whom_it_is_bound_to() {
    use yalain_world::profile::{self, item_line};
    let (mut h, b, me, _, chest) = marked_chest();
    let lines = |h: &mut Harness, b: usize| {
        let from = h.bots[b].inbox.len();
        let mut bytes = profile::request::OBJECT.to_le_bytes().to_vec();
        bytes.extend(0u32.to_le_bytes());
        bytes.extend(chest.to_le_bytes());
        bytes.extend(0x3FFu32.to_le_bytes());
        let mut g = yalain_world::graph::Builder::new();
        g.native(
            profile::REQUEST_TYPE,
            auberean_script::native_object::NativeObject::Packed {
                ty: u32::from(profile::REQUEST_TYPE),
                bytes,
            },
        );
        h.bots[b].send(
            true,
            queue::WEENIE,
            &server_call(
                yalain_world::requests::function::EXAMINE,
                &[Arg::Object(g.graph())],
            ),
        );
        h.run(5);
        let said = h
            .world_messages(b, from)
            .into_iter()
            .find_map(|m| match m {
                ServerMessage::PlayerCall(c) if c.function.0 == profile::EXAMINATION_PROFILE => {
                    Some(c.args?.0)
                }
                _ => None,
            })
            .expect("the profile");
        let has = |id: u32| said.windows(4).any(|w| w == id.to_le_bytes());
        [
            has(item_line::BINDS),
            has(item_line::BOUND_TO_YOU),
            has(item_line::BOUND_TO_ANOTHER) && has(profile::look::BOUND_TO_ANOTHER),
        ]
    };
    assert_eq!(lines(&mut h, b), [true, false, false]);
    if let Some(t) = h.server.things.get_mut(chest) {
        t.binds_on_use = false;
        t.own.bound_to = me;
    }
    assert_eq!(lines(&mut h, b), [false, true, false]);
    if let Some(t) = h.server.things.get_mut(chest) {
        t.own.bound_to = me + 1;
    }
    assert_eq!(lines(&mut h, b), [false, false, true]);
}

/// An item bound to another player is refused its equip with the status that says so, by a use and
/// by a directive; its own holder wears it.
/// Behaviour: server.items.bound-to-another-is-not-equipped
#[test]
fn an_item_bound_to_another_is_not_equipped() {
    let (mut h, b, me, _, chest) = marked_chest();
    if let Some(t) = h.server.things.get_mut(chest) {
        t.binds_on_use = false;
        t.own.bound_to = me + 1;
    }
    let worn = |h: &Harness| {
        h.server
            .players
            .values()
            .flat_map(|p| p.play.items.iter())
            .any(|i| i.id == chest && i.location != 0)
    };
    assert!(!worn(&h));
    let from = used(&mut h, b, me, chest);
    assert!(!worn(&h));
    let refused = h.calls(b, from).iter().any(|(_, args)| {
        args.iter().any(|v| match v {
            Value::Graph(g) => g.iter().any(|(_, n)| match n {
                Node::Script { slots, .. } => slots.iter().any(|x| x.1 == BOUND_TO_ANOTHER),
                _ => false,
            }),
            _ => false,
        })
    });
    assert!(refused, "the status that says so");
    if let Some(t) = h.server.things.get_mut(chest) {
        t.own.bound_to = me;
    }
    used(&mut h, b, me, chest);
    assert!(worn(&h));
}
