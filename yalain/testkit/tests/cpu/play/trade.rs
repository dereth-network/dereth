//! People: a storekeeper greets a player and opens its store, and sells for money; quest givers
//! give a quest and move it on.

use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_content::quests::{QuestDef, QuestTable, Step, Trigger, Update};
use yalain_content::stores::{Keeper, Sale, StoreTable};
use yalain_content::{ContentObject, ContentPack, Description, Facts, Kind};
use yalain_testkit::play::server_call;
use yalain_testkit::{test_catalog, Harness, TEST_ITEM};
use yalain_world::calls::{self, Value};
use yalain_world::graph::{Builder, Slot};

const USE: u32 = 0x8216_0016;
const USE_DONE: u32 = 0x8216_0021;
const HEAR_TELL: u32 = 0x8205_0017;
const ENTER_STORE: u32 = 0x9B5E_0056;
const PURCHASE: u32 = 0x9B5E_0031;
const UPDATE_QUEST: u32 = 0x8213_0024;

const KEEPER: u64 = 0x8130_0000_0000_0101;
const KEEPER_TEMPLATE: u32 = 0x4700_33D0;
const STORE: u32 = 0x6F00_16F6;
const GUARD: u64 = 0x8130_0000_0000_0102;
const GUARD_TEMPLATE: u32 = 0x4700_2DFD;
const LIEUTENANT: u64 = 0x8130_0000_0000_0103;
const LIEUTENANT_TEMPLATE: u32 = 0x4700_2DFF;
const QUEST: u32 = 0x4000_0088;

fn person(id: u64, template: u32, origin: [f32; 3]) -> ContentObject {
    ContentObject {
        object: Some(id),
        template,
        description: Description {
            package: 0x13A4,
            look: 0x1F00_0100,
            enter_fx: 0,
            scale: None,
            name: Some((0x1F6E_D407, 0x2500_2AFC)),
            numbers: [0; 6],
            source: "authored for a test".into(),
        },
        cell: 0x0299_0109,
        origin,
        rotation: [1.0, 0.0, 0.0, 0.0],
        kind: Kind::Npc,
        label: String::new(),
        placed: "authored for a test".into(),
        destination: None,
        facts: Facts::default(),
        walks: None,
        gate: None,
    }
}

fn use_of(item: u64, player: u64) -> Arg {
    let mut b = Builder::new();
    b.object(
        0x245,
        &[
            Slot::Ref(None),
            Slot::Ref(None),
            Slot::Long(item),
            Slot::Ref(None),
            Slot::Int(0),
            Slot::Long(player),
            Slot::Int((-1.0f32).to_bits()),
            Slot::Long(0),
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

fn calls_of(h: &Harness, b: usize, from: usize) -> Vec<(u32, Vec<Value>)> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c) => Some((
                c.function.0,
                c.args
                    .as_ref()
                    .and_then(|a| calls::values(&a.0).ok())
                    .unwrap_or_default(),
            )),
            _ => None,
        })
        .collect()
}

fn shop() -> Harness {
    let mut h = Harness::new();
    h.server.catalog = test_catalog();
    h.server.content = ContentPack {
        objects: vec![person(KEEPER, KEEPER_TEMPLATE, [61.0, 241.0, -44.6])],
    };
    h.server.stores = StoreTable {
        keepers: vec![Keeper {
            template: KEEPER_TEMPLATE,
            store: STORE,
            greeting: Some(yalain_content::Words::Ref(0x2824_938C, 0x2500_2AFC)),
            placed: "authored for a test".into(),
        }],
        sales: vec![Sale {
            store: STORE,
            ordinal: 0,
            product: TEST_ITEM,
            cost: 10.0,
            currency: 0x4700_0538,
            stack: 1,
            icon: vec![(1000, 0x4100_4552, 0xFFFF_FFFF)],
            name: (0x01A2_D809, 0x2500_23D3),
            placed: "authored for a test".into(),
        }],
    };
    h
}

/// Using a storekeeper in reach is answered, then it greets the player by name and opens its store
/// on its first page.
/// Behaviour: server.store.greeting-and-store
#[test]
fn a_storekeeper_used_greets_the_player_and_opens_its_store() {
    let mut h = shop();
    let (b, me) = h.player_in_world("one", "Elara");
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(USE, &[use_of(KEEPER, me)]),
    );
    h.run(10);
    let f: Vec<u32> = calls_of(&h, b, from).iter().map(|(f, _)| *f).collect();
    let at = |x: u32| f.iter().position(|y| *y == x);
    assert!(
        at(USE_DONE) < at(HEAR_TELL) && at(HEAR_TELL) < at(ENTER_STORE),
        "{f:x?}"
    );
    assert!(at(ENTER_STORE).is_some());
}

/// Buying a sale with enough money: the money goes down by its price, and the item is made in the
/// player's pack.
/// Behaviour: server.store.purchase
#[test]
fn a_purchase_takes_its_price_and_makes_the_item_in_the_pack() {
    let mut h = shop();
    let (b, me) = h.player_in_world("one", "Elara");
    for p in h.server.players.values_mut() {
        yalain_world::quals::set_int(&mut p.qualities, 3100, 50);
    }
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(USE, &[use_of(KEEPER, me)]),
    );
    h.run(10);
    let from = h.bots[b].inbox.len();
    let stock = yalain_world::store::STOCK_BASE | u64::from(STORE & 0xFFFF) << 8;
    let mut g = Builder::new();
    g.object(
        0x1B76,
        &[
            Slot::Long(stock),
            Slot::Ref(None),
            Slot::Long(me),
            Slot::Int(1),
            Slot::Long(KEEPER),
            Slot::Int(0),
            Slot::Int(0),
        ],
    );
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(PURCHASE, &[Arg::Object(g.graph())]),
    );
    h.run(10);
    let money = h.world_messages(b, from).iter().find_map(|m| match m {
        ServerMessage::UpdateInt(q) if q.quality == 3100 => Some(q.value),
        _ => None,
    });
    assert_eq!(money, Some(40));
    let made = h.world_messages(b, from).iter().any(|m| {
        matches!(m, ServerMessage::CreateObject(o) if o.weenie.entity_did == TEST_ITEM && o.weenie.objects[0] == me)
    });
    assert!(made, "the item is made in the pack");
}

const UPDATE_CATALOG: u32 = 0x9B5E_0062;
const LEAVE_CATALOG_DONE: u32 = 0x9B5E_005E;
const LEAVE_STORE: u32 = 0x9B5E_000C;
const REQUEST_DONE: u32 = 0x9B5E_004C;
const UPDATE_STORE: u32 = 0x9B5E_0049;

/// A player in a storekeeper's store.
fn in_store(h: &mut Harness) -> (usize, u64) {
    let (b, me) = h.player_in_world("one", "Elara");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(USE, &[use_of(KEEPER, me)]),
    );
    h.run(10);
    (b, me)
}

fn count(h: &Harness, b: usize, from: usize, f: u32) -> usize {
    calls_of(h, b, from).iter().filter(|(x, _)| *x == f).count()
}

/// While a shopper stays in a vendor's store, the store's size is sent again (as a catalog of
/// none) a few seconds after entering and about every half minute after; leaving the store ends
/// it.
/// Behaviour: server.store.vendor-store-sent-again-while-shopping
#[test]
fn a_vendors_store_is_sent_again_every_half_minute_until_the_shopper_leaves_it() {
    let mut h = shop();
    let (b, _) = in_store(&mut h);
    let from = h.bots[b].inbox.len();
    for _ in 0..70 {
        h.advance(1.0);
        h.run(1);
    }
    let sent = calls_of(&h, b, from);
    let updates: Vec<&Vec<Value>> = sent
        .iter()
        .filter(|(f, _)| *f == UPDATE_CATALOG)
        .map(|(_, v)| v)
        .collect();
    assert_eq!(
        updates.len(),
        3,
        "after a few seconds, then twice a half minute apart"
    );
    assert!(
        updates[0].contains(&Value::Int(0)) && updates[0].contains(&Value::Long(KEEPER)),
        "no catalog, the storekeeper: {:?}",
        updates[0]
    );
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(LEAVE_STORE, &[Arg::LongInt(KEEPER)]),
    );
    h.run(5);
    let from = h.bots[b].inbox.len();
    for _ in 0..70 {
        h.advance(1.0);
        h.run(1);
    }
    assert_eq!(count(&h, b, from, UPDATE_CATALOG), 0, "left: no more");
}

/// A shopper who walks more than eight meters from a vendor is told it left the store's catalog,
/// and its store is no longer sent again.
/// Behaviour: server.store.vendor-left-past-eight-meters
#[test]
fn a_shopper_walking_off_from_a_vendor_leaves_its_catalog() {
    let mut h = shop();
    let (b, me) = in_store(&mut h);
    if let Some(p) = h.server.players.values_mut().find(|p| p.character == me) {
        p.position.cell = 0x0299_0109;
        p.position.origin = [61.0, 252.0, -44.6];
    }
    let from = h.bots[b].inbox.len();
    for _ in 0..5 {
        h.advance(1.0);
        h.run(1);
    }
    assert_eq!(count(&h, b, from, LEAVE_CATALOG_DONE), 1, "told it left");
    let from = h.bots[b].inbox.len();
    for _ in 0..70 {
        h.advance(1.0);
        h.run(1);
    }
    assert_eq!(count(&h, b, from, UPDATE_CATALOG), 0);
}

/// Buying with a full pack: the purchase is refused for want of room and the page sent again;
/// nothing is paid and nothing made.
/// Behaviour: server.store.no-room-refuses-the-purchase
#[test]
fn a_purchase_into_a_full_pack_is_refused_and_nothing_is_paid() {
    let mut h = shop();
    let (b, me) = in_store(&mut h);
    for p in h.server.players.values_mut() {
        yalain_world::quals::set_int(&mut p.qualities, 3100, 50);
        for i in 0..yalain_world::inventory::PACK_ROOM {
            p.play.items.push(yalain_world::state::Held {
                id: 0x8130_0000_0100_0000 + u64::from(i),
                template: TEST_ITEM,
                slot: i,
                location: 0,
                quantity: 1,
            });
        }
    }
    let from = h.bots[b].inbox.len();
    let stock = yalain_world::store::STOCK_BASE | u64::from(STORE & 0xFFFF) << 8;
    let mut g = Builder::new();
    g.object(
        0x1B76,
        &[
            Slot::Long(stock),
            Slot::Ref(None),
            Slot::Long(me),
            Slot::Int(1),
            Slot::Long(KEEPER),
            Slot::Int(0),
            Slot::Int(0),
        ],
    );
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(PURCHASE, &[Arg::Object(g.graph())]),
    );
    h.run(10);
    let sent = calls_of(&h, b, from);
    let done = sent
        .iter()
        .position(|(f, v)| *f == REQUEST_DONE && v.contains(&Value::Int(45016)))
        .expect("refused for want of room");
    assert!(
        sent[done..].iter().any(|(f, _)| *f == UPDATE_STORE),
        "the page again"
    );
    let msgs = h.world_messages(b, from);
    assert!(
        !msgs
            .iter()
            .any(|m| matches!(m, ServerMessage::UpdateInt(q) if q.quality == 3100)),
        "nothing paid"
    );
    assert!(
        !msgs.iter().any(
            |m| matches!(m, ServerMessage::CreateObject(o) if o.weenie.entity_did == TEST_ITEM)
        ),
        "nothing made"
    );
}

fn quests() -> QuestTable {
    let step = |needs, template, update| Step {
        quest: QUEST,
        needs,
        trigger: Trigger::Use(template),
        tell: Some(yalain_content::Words::Ref(0x2824_FD4C, 0x2500_2881)),
        update,
        xp: 0,
        gives: Vec::new(),
        counts: Vec::new(),
        effects: Vec::new(),
        text: None,
        placed: "authored for a test".into(),
        requires: Vec::new(),
        takes: Vec::new(),
        races: Vec::new(),
        effects_last: false,
        fellows: false,
        to: None,
    };
    QuestTable {
        quests: vec![QuestDef {
            id: QUEST,
            name: (0xA860_7CEC, 0x2500_17EE),
            description: (0x6C45_9364, 0x2500_17EE),
            icon: 0x4100_034B,
            play_fx: true,
            journals: vec![(1, 0x2500_17EE), (2, 0x2500_17EE)],
            challenge: -999,
            duration: 72000.0,
            retry: 0.0,
            xp: 0,
            cancellable: false,
            placed: "authored for a test".into(),
            hidden: false,
            effects: Vec::new(),
            subquests: Vec::new(),
            min_level: 0,
            fails_on_death: false,
        }],
        steps: vec![
            step(0, GUARD_TEMPLATE, Update::Bestow),
            step(1, LIEUTENANT_TEMPLATE, Update::Advance(2)),
        ],
    }
}

/// The quest's update kinds as the updates carry them.
fn updates(h: &Harness, b: usize) -> Vec<u32> {
    calls_of(h, b, 0)
        .into_iter()
        .filter(|(f, _)| *f == UPDATE_QUEST)
        .filter_map(|(_, v)| match v.last() {
            Some(Value::Int(k)) => Some(*k),
            _ => None,
        })
        .collect()
}

/// Using the person a quest's first step names gives the quest (its giver's line first); using the
/// next one moves it on to its second phase; using the first again does nothing more.
/// Behaviour: server.quests.given-and-moved-on
#[test]
fn quest_givers_give_a_quest_and_move_it_on_in_order() {
    let mut h = Harness::new();
    h.server.content = ContentPack {
        objects: vec![
            person(GUARD, GUARD_TEMPLATE, [61.0, 241.0, -44.6]),
            person(LIEUTENANT, LIEUTENANT_TEMPLATE, [60.0, 238.0, -44.6]),
        ],
    };
    h.server.quests = quests();
    let (b, me) = h.player_in_world("one", "Elara");
    h.bots[b].send(true, queue::WEENIE, &server_call(USE, &[use_of(GUARD, me)]));
    h.run(10);
    assert_eq!(updates(&h, b), vec![0x1000_0000], "given");
    h.bots[b].send(true, queue::WEENIE, &server_call(USE, &[use_of(GUARD, me)]));
    h.run(10);
    assert_eq!(
        updates(&h, b),
        vec![0x1000_0000],
        "the giver again: nothing"
    );
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(USE, &[use_of(LIEUTENANT, me)]),
    );
    h.run(10);
    assert_eq!(updates(&h, b), vec![0x1000_0000, 0x2000_0002], "moved on");
}
