//! Chests the world makes: what an effect places at a player may be a chest, opened and filled as
//! a placed chest is and gone with what it holds when its time is up; and an item used on an
//! object its row names may make a chest and its guards standing there, the object taken from the
//! world. A chest of a kind that goes once closed leaves the world as it shuts, made or placed.
//! Fixture: the test world's beetles, one player holding a summoning stone and a machine, a cache
//! of a chest's kind with a loot profile of one crystal, a dig point.

use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_content::effect_rules::Placing;
use yalain_content::{LootProfile, Prop, Props};
use yalain_testkit::play::server_call;
use yalain_testkit::Harness;
use yalain_world::calls::{self, Value};
use yalain_world::things::{Kind, Mode};
use yalain_world::usable::interactions::{self, Interaction};

const USE: u32 = 0x8216_0016;
const OPEN_DONE: u32 = 0x820E_005D;
const CLOSE: u32 = 0x820E_000C;
const CLOSE_DONE: u32 = 0x820E_0060;

/// A chest's kind, and the profile its cache holds: one crystal, always.
const CHEST_PACKAGE: u32 = 0x0E8B;
const CACHE: u32 = 0x4700_0F3A;
const PROFILE: u32 = 0x65D;
const CRYSTAL: u32 = 0x4700_0F21;
/// A stone whose use names an effect a row says places the cache, which stands a minute.
const STONE: u32 = 0x4700_0821;
const SUMMON_CACHE: &str = "Eff_SummonTestCache";
/// A machine whose table names the dig point, playing its own behavior, the point used up; the
/// dig point an effect placed.
const MACHINE: u32 = 0x4700_1B34;
const MACHINE_TABLE: u32 = 0x6F00_1281;
const DIG_POINT: u32 = 0x4700_0F90;
const SUMMON_DIG_POINT: &str = "Eff_SummonTestDirt";

fn use_of(item: u64, me: u64, on: u64) -> Arg {
    use yalain_world::graph::{Builder, Slot};
    let mut b = Builder::new();
    b.object(
        0x245,
        &[
            Slot::Ref(None),
            Slot::Ref(None),
            Slot::Long(item),
            Slot::Ref(None),
            Slot::Int(0),
            Slot::Long(me),
            Slot::Int((-1.0f32).to_bits()),
            Slot::Long(on),
            Slot::Int(0),
            Slot::Long(0),
            Slot::Int(0),
            Slot::Ref(None),
            Slot::Int(0),
            Slot::Int(316),
            Slot::Int(0),
            Slot::Int(0),
        ],
    );
    Arg::Object(b.graph())
}

/// The world, one player in it holding a stone and a machine; the player's client, character,
/// world connection, the stone and the machine.
fn world() -> (Harness, usize, u64, yalain_net::ConnId, u64, u64) {
    let mut h = Harness::new();
    h.add_creatures(1);
    let (b, me) = h.player_in_world("one", "Kestrel");
    h.run(20);
    h.server.chests.packages.insert(CHEST_PACKAGE);
    let mut cache = Props::default();
    cache.set("GeneratorProfile", Prop::Word(PROFILE));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(CACHE, CHEST_PACKAGE, cache));
    h.server.catalog.insert(yalain_testkit::test_template(
        CRYSTAL,
        0x3A1,
        Props::default(),
    ));
    h.server
        .catalog
        .profiles
        .insert(PROFILE, "GP_TestMapsTier1Hoard".into());
    h.server.loot.profiles.push(LootProfile {
        name: "GP_TestMapsTier1Hoard".into(),
        counts: vec![1.0],
        items: Vec::new(),
        extras: vec![(CRYSTAL, 1.0)],
        placed: "authored for a test".into(),
        confidence: "authored".into(),
        chest: None,
    });
    h.server.effect_table.placings.insert(
        SUMMON_CACHE.into(),
        Placing {
            template: CACHE,
            offset: [0.0, 1.5],
            lasts: Some(60),
        },
    );
    let mut stone = Props::default();
    stone
        .set("Usage_Effect1", Prop::Text(SUMMON_CACHE.into()))
        .set("Item_IsDestroyOnUse", Prop::Bool(true));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(STONE, 0x0778, stone));
    let mut machine = Props::default();
    machine.set(interactions::TABLE, Prop::Word(MACHINE_TABLE));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(MACHINE, 0x1B34, machine));
    h.server.catalog.insert(yalain_testkit::test_template(
        DIG_POINT,
        0x019B,
        Props::default(),
    ));
    h.server.interactions.tables.insert(
        MACHINE_TABLE,
        vec![Interaction {
            targets: vec![DIG_POINT],
            reach: 3.0,
            behavior: Some(interactions::PLANT_BEHAVIOR),
            message: None,
            uses_up: false,
            uses_up_target: true,
            gate: None,
        }],
    );
    h.server.effect_table.placings.insert(
        SUMMON_DIG_POINT.into(),
        Placing {
            template: DIG_POINT,
            offset: [0.0, 1.0],
            lasts: Some(300),
        },
    );
    let world = *h.server.players.keys().next().expect("in");
    let stone =
        yalain_world::inventory::give(&mut h.server, &mut h.net, world, STONE, 1).expect("given");
    let machine =
        yalain_world::inventory::give(&mut h.server, &mut h.net, world, MACHINE, 1).expect("given");
    h.run(2);
    (h, b, me, world, stone, machine)
}

fn uses(h: &mut Harness, b: usize, me: u64, item: u64, on: u64, steps: usize) {
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(USE, &[use_of(item, me, on)]),
    );
    h.run(steps);
}

/// The things of a template standing in the world.
fn standing(h: &Harness, template: u32) -> Vec<u64> {
    let mut v: Vec<u64> = h
        .server
        .things
        .by_id
        .values()
        .filter(|t| t.template == template && t.at.is_some())
        .filter(|t| t.creature.as_ref().is_none_or(|c| c.mode != Mode::Dead))
        .map(|t| t.id)
        .collect();
    v.sort_unstable();
    v
}

/// What a client was told of a chest's opening and closing since `from`.
fn chest_calls(h: &Harness, b: usize, from: usize, chest: u64) -> Vec<String> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::UpdateBoolVisual(q) if q.object.id == chest => {
                Some(if q.value == 1 { "open" } else { "shut" }.to_string())
            }
            ServerMessage::CreateObject(o) if o.weenie.entity_did == CRYSTAL => {
                Some("crystal".into())
            }
            ServerMessage::ForgetObject(o) if o.0.id == chest => Some("gone".into()),
            ServerMessage::PlayerCall(c) => {
                let v = c.args.as_ref().and_then(|a| calls::values(&a.0).ok())?;
                match c.function.0 {
                    OPEN_DONE => match v.first() {
                        Some(Value::Int(s)) => Some(format!("opened {s}")),
                        _ => None,
                    },
                    CLOSE_DONE => Some("closed".into()),
                    _ => None,
                }
            }
            _ => None,
        })
        .collect()
}

/// A chest an effect places at a player is a chest as a placed one is: used, it opens, its opener
/// told what its profile fills it with; closed on request it shuts. When its time is up it goes,
/// forgotten by the client that saw it, and what it held goes with it.
/// Behaviour: server.places.chest-an-effect-places
#[test]
fn a_chest_an_effect_places_opens_as_any_chest_and_goes_when_its_time_is_up() {
    let (mut h, b, me, _, stone, _) = world();
    uses(&mut h, b, me, stone, 0, 20);
    let placed = standing(&h, CACHE);
    assert_eq!(placed.len(), 1, "the stone's effect places the cache");
    let cache = placed[0];
    let from = h.bots[b].inbox.len();
    uses(&mut h, b, me, cache, 0, 40);
    assert_eq!(
        chest_calls(&h, b, from, cache),
        ["opened 0", "open", "crystal"],
        "opened as any chest, filled from its profile"
    );
    let held = h.server.chests.by_id[&cache].contents.clone();
    assert_eq!(held.len(), 1);
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(CLOSE, &[Arg::LongInt(cache)]),
    );
    h.run(30);
    assert_eq!(chest_calls(&h, b, from, cache), ["closed", "shut"]);
    assert_eq!(standing(&h, CACHE), [cache], "it stands its time");
    let from = h.bots[b].inbox.len();
    h.run(1250);
    assert!(standing(&h, CACHE).is_empty(), "its time is up");
    assert_eq!(chest_calls(&h, b, from, cache), ["gone"]);
    assert!(!h.server.chests.by_id.contains_key(&cache));
    assert!(
        held.iter().all(|i| h.server.things.get(*i).is_none()),
        "what it held goes with it"
    );
}

/// An item used on an object whose row names what the use makes there makes it at the object, its
/// table's own behavior played first: a chest that opens as any chest, and a creature, each at its
/// offset; the object, one an effect placed, is taken from the world as the outcome uses it up.
/// Behaviour: server.places.an-item-used-on-an-object-makes-a-chest-there
#[test]
fn a_machine_used_on_a_dig_point_makes_a_cache_and_its_guard_there() {
    let (mut h, b, me, world, _, machine) = world();
    let guard = h
        .server
        .things
        .by_id
        .values()
        .find(|t| t.creature.is_some())
        .expect("a beetle")
        .template;
    h.server
        .item_rows
        .interactions
        .push(yalain_content::item_rows::Interaction {
            item: MACHINE,
            on: yalain_content::item_rows::On::Template(DIG_POINT),
            gives: Vec::new(),
            count: 1,
            gated_gives: Vec::new(),
            places: vec![
                Placing {
                    template: CACHE,
                    offset: [0.0, 0.0],
                    lasts: Some(1200),
                },
                Placing {
                    template: guard,
                    offset: [3.0, 0.0],
                    lasts: None,
                },
            ],
        });
    let place = yalain_content::effect_rules::Placing {
        template: DIG_POINT,
        offset: [0.0, 1.0],
        lasts: Some(300),
    };
    let clock = h.clock();
    let dig = yalain_world::placing::place(&mut h.server, &mut h.net, world, place, clock)
        .expect("placed");
    h.run(2);
    let guards_before = standing(&h, guard).len();
    let at = h
        .server
        .things
        .get(dig)
        .and_then(|t| t.at)
        .expect("standing");
    uses(&mut h, b, me, machine, dig, 60);
    assert!(
        h.server.things.get(dig).is_none(),
        "the dig point is used up"
    );
    let caches = standing(&h, CACHE);
    assert_eq!(caches.len(), 1, "the cache stands where the point stood");
    let c = h
        .server
        .things
        .get(caches[0])
        .and_then(|t| t.at)
        .expect("at");
    let d = |a: [f32; 3], b: [f32; 3]| ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt();
    assert!(d(c.origin, at.origin) < 0.05, "{c:?} {at:?}");
    let guards = standing(&h, guard);
    assert_eq!(guards.len(), guards_before + 1, "its guard is made");
    let made = guards
        .iter()
        .filter_map(|g| h.server.things.get(*g).and_then(|t| t.at))
        .any(|g| (d(g.origin, at.origin) - 3.0).abs() < 0.05);
    assert!(made, "the guard at its offset");
    assert!(
        h.server.players[&world]
            .play
            .items
            .iter()
            .any(|i| i.id == machine),
        "the machine is kept"
    );
    let from = h.bots[b].inbox.len();
    uses(&mut h, b, me, caches[0], 0, 40);
    assert_eq!(
        chest_calls(&h, b, from, caches[0]),
        ["opened 0", "open", "crystal"]
    );
}

/// Without a row naming what such a use makes, a table that plays a behavior of its own goes on as
/// any other use: nothing is made and the object stands.
/// Behaviour: server.places.an-item-used-on-an-object-makes-a-chest-there
#[test]
fn a_machine_whose_use_no_row_names_makes_nothing() {
    let (mut h, b, me, world, _, machine) = world();
    let place = yalain_content::effect_rules::Placing {
        template: DIG_POINT,
        offset: [0.0, 1.0],
        lasts: Some(300),
    };
    let clock = h.clock();
    let dig = yalain_world::placing::place(&mut h.server, &mut h.net, world, place, clock)
        .expect("placed");
    h.run(2);
    uses(&mut h, b, me, machine, dig, 60);
    assert!(h
        .server
        .things
        .get(dig)
        .is_some_and(|t| t.kind == Kind::Placed));
    assert!(standing(&h, CACHE).is_empty());
}

/// Open the chest, then close it; what its client saw of it, from its opening on.
fn open_and_close(h: &mut Harness, b: usize, me: u64, chest: u64) -> Vec<String> {
    let from = h.bots[b].inbox.len();
    uses(h, b, me, chest, 0, 40);
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(CLOSE, &[Arg::LongInt(chest)]),
    );
    h.run(30);
    chest_calls(h, b, from, chest)
}

/// A chest of a kind that goes once closed, one an effect placed, leaves the world as its closing
/// is told: forgotten by the client that saw it, gone with what it still held, long before its time
/// is up.
/// Behaviour: server.places.chest-gone-once-closed
#[test]
fn a_chest_whose_class_goes_once_closed_leaves_the_world_as_it_shuts() {
    let (mut h, b, me, _, stone, _) = world();
    h.server.chests.gone_once_closed.insert(CHEST_PACKAGE);
    uses(&mut h, b, me, stone, 0, 20);
    let cache = standing(&h, CACHE)[0];
    let saw = open_and_close(&mut h, b, me, cache);
    assert_eq!(
        saw,
        ["opened 0", "open", "crystal", "closed", "shut", "gone"]
    );
    assert!(h.server.things.get(cache).is_none(), "gone");
    assert!(!h.server.chests.by_id.contains_key(&cache));
    assert!(
        !h.server
            .things
            .by_id
            .values()
            .any(|t| t.template == CRYSTAL),
        "what it held goes with it"
    );
}

/// A placed chest of such a kind, a world's row, goes as it shuts too, and is not made again for
/// anyone who comes to its place; one of another kind stays.
/// Behaviour: server.places.chest-gone-once-closed
#[test]
fn a_placed_chest_that_goes_once_closed_is_kept_from_the_world() {
    use yalain_content::{ContentObject, ContentPack, Description, Facts};
    const CORPSE: u64 = 0x4002_9900_0000_0091;
    let (mut h, b, me, world, _, _) = world();
    h.server.chests.gone_once_closed.insert(CHEST_PACKAGE);
    let here = h.server.players[&world].position;
    h.server.content = ContentPack {
        objects: vec![ContentObject {
            object: Some(CORPSE),
            template: CACHE,
            description: Description {
                name_text: None,
                package: CHEST_PACKAGE,
                look: 0x1F00_0F58,
                enter_fx: 0,
                scale: None,
                name: Some((0x1C06_0002, 0x2500_0002)),
                numbers: [0; 6],
                source: "authored for a test".into(),
            },
            cell: here.cell,
            origin: [here.origin[0] + 1.0, here.origin[1], here.origin[2]],
            rotation: [1.0, 0.0, 0.0, 0.0],
            kind: yalain_content::Kind::Static,
            label: String::new(),
            placed: "authored for a test".into(),
            destination: None,
            facts: Facts::default(),
            walks: None,
            gate: None,
        }],
        ..ContentPack::default()
    };
    yalain_world::presence::refresh(&mut h.server, &mut h.net, world);
    h.run(20);
    assert!(h.server.players[&world].known.contains_key(&CORPSE), "seen");
    let saw = open_and_close(&mut h, b, me, CORPSE);
    assert_eq!(
        saw,
        ["opened 0", "open", "crystal", "closed", "shut", "gone"]
    );
    yalain_world::presence::refresh(&mut h.server, &mut h.net, world);
    h.run(40);
    assert!(
        !h.server.players[&world].known.contains_key(&CORPSE),
        "not made again"
    );
    assert!(yalain_world::usable::chests::chest(&h.server, CORPSE).is_none());
    // A chest of a kind that stays is opened again.
    h.server.chests.gone.clear();
    h.server.chests.gone_once_closed = [0x280D].into();
    yalain_world::presence::refresh(&mut h.server, &mut h.net, world);
    h.run(40);
    let saw = open_and_close(&mut h, b, me, CORPSE);
    assert_eq!(saw, ["opened 0", "open", "crystal", "closed", "shut"]);
}
