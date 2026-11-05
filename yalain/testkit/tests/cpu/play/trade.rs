//! People: a storekeeper greets a player and opens its store, and sells for money; quest givers
//! give a quest and move it on.

use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_content::quests::{QuestDef, QuestTable, Step, Trigger, Update};
use yalain_content::stores::{Keeper, Sale, SaleGate, StoreTable};
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
            name_text: None,
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
        ..ContentPack::default()
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
            gate: SaleGate::default(),
        }],
        hiding: Vec::new(),
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

/// A store whose second sale has no icon layers: every sale's references point at its own icon
/// and name, the one without layers has no icon object and names its product's icon instead; a
/// stack of one goes as a whole number, a larger one as a value set.
/// Behaviour: server.store.sale-without-icon-layers
#[test]
fn a_sale_without_icon_layers_keeps_every_sales_references_on_its_own_objects() {
    const PLAIN: u32 = 0x4700_06C3;
    const PICTURE: u32 = 0x4100_66DB;
    let mut h = shop();
    let mut props = yalain_content::Props::default();
    props.set("Icon", yalain_content::Prop::Word(PICTURE));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(PLAIN, 0x200, props));
    let first = h.server.stores.sales[0].clone();
    let sale = |ordinal: u32, product: u32, icon: Vec<(u32, u32, u32)>, name: u32| Sale {
        ordinal,
        product,
        icon,
        name: (name, 0x2500_23D3),
        ..first.clone()
    };
    h.server.stores.sales = vec![
        sale(
            0,
            TEST_ITEM,
            vec![(1000, 0x4100_4552, 0xFFFF_FFFF)],
            0x0000_0A00,
        ),
        sale(1, PLAIN, Vec::new(), 0x0000_0A01),
        sale(
            2,
            TEST_ITEM,
            vec![(1000, 0x4100_4553, 0xFFFF_FFFF)],
            0x0000_0A02,
        ),
    ];
    h.server.stores.sales[1].stack = 1_000_000;
    let (b, me) = h.player_in_world("one", "Elara");
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(USE, &[use_of(KEEPER, me)]),
    );
    h.run(10);
    let calls = calls_of(&h, b, from);
    let view = &calls
        .iter()
        .find(|(f, _)| *f == ENTER_STORE)
        .expect("the store opens")
        .1[0];
    let Some(calls::Node::Script { slots, .. }) = view.root() else {
        panic!("a store view");
    };
    let Some(calls::Node::Native { bytes, .. }) = view.entry(slots[0].1) else {
        panic!("the list of sales");
    };
    let word = |i: usize| u32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().expect("4"));
    assert_eq!(word(0), 3, "three sales listed");
    for (i, want) in [
        (TEST_ITEM, Some(0x4100_4552_u32), 0x0A00, 0),
        (PLAIN, None, 0x0A01, PICTURE),
        (TEST_ITEM, Some(0x4100_4553), 0x0A02, 0),
    ]
    .into_iter()
    .enumerate()
    {
        let (product, picture, name, icon_did) = want;
        let Some(calls::Node::Script { slots: p, .. }) = view.entry(word(i + 1)) else {
            panic!("sale {i} is a profile");
        };
        assert_eq!(p[0].1, product, "sale {i}: its product");
        match (picture, view.entry(p[1].1)) {
            (
                Some(pic),
                Some(calls::Node::Native {
                    ty: 0x2E,
                    bytes: icon,
                }),
            ) => {
                assert_eq!(&icon[8..12], &pic.to_le_bytes(), "sale {i}: its own icon");
            }
            (None, None) => assert_eq!(p[1].1, 0xFFFF_FFFF, "sale {i}: no icon"),
            (_, other) => panic!("sale {i}: icon {other:?}"),
        }
        assert_eq!(
            p[4].1, icon_did,
            "sale {i}: the product's icon in place of layers"
        );
        // A stack of one as a whole number, a larger one as a value set.
        let stack = if i == 1 { (0x04, 1_000_000) } else { (0x02, 1) };
        assert_eq!(p[8], stack, "sale {i}: its stack");
        let Some(calls::Node::TextInfo(t)) = view.entry(p[5].1) else {
            panic!("sale {i}: a name");
        };
        assert_eq!(t.string_id, name, "sale {i}: its own name");
    }
}

/// A store's view: its size and each listed sale's slots.
fn listed(view: &Value) -> (u32, Vec<Vec<(u8, u32)>>) {
    let Some(calls::Node::Script { slots, .. }) = view.root() else {
        panic!("a store view");
    };
    let Some(calls::Node::Native { bytes, .. }) = view.entry(slots[0].1) else {
        panic!("the list of sales");
    };
    let word = |i: usize| u32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().expect("4"));
    let sales = (1..=word(0) as usize)
        .map(|i| match view.entry(word(i)) {
            Some(calls::Node::Script { slots, .. }) => slots.clone(),
            other => panic!("a sale profile, not {other:?}"),
        })
        .collect();
    (slots[2].1, sales)
}

/// A store of five: a sale for anyone, one for Lugians, one for players who finished a quest, one
/// for anyone, one from level 50; the player a human of a low level.
const GATED_QUEST: u32 = 0x8000_0342;
fn gated_shop(hides: bool) -> Harness {
    let mut h = shop();
    let first = h.server.stores.sales[0].clone();
    let gates = [
        SaleGate::default(),
        SaleGate {
            race: 2,
            ..SaleGate::default()
        },
        SaleGate {
            quests: vec![(GATED_QUEST, 0x3000_0000)],
            ..SaleGate::default()
        },
        SaleGate::default(),
        SaleGate {
            level: 50,
            ..SaleGate::default()
        },
    ];
    h.server.stores.sales = gates
        .into_iter()
        .zip(0..)
        .map(|(gate, ordinal)| Sale {
            ordinal,
            gate,
            ..first.clone()
        })
        .collect();
    if hides {
        h.server.stores.hiding = vec![STORE];
    }
    h
}

fn enter_gated(h: &mut Harness) -> (usize, u64, Value) {
    let (b, me) = h.player_in_world("one", "Elara");
    for p in h.server.players.values_mut() {
        p.race = 1;
    }
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(USE, &[use_of(KEEPER, me)]),
    );
    h.run(10);
    let view = calls_of(h, b, from)
        .into_iter()
        .find(|(f, _)| *f == ENTER_STORE)
        .expect("the store opens")
        .1
        .remove(0);
    (b, me, view)
}

/// A store that hides what a player may not have leaves off its page the sales of another race
/// and of a quest the player has not finished (under way is not enough), still counting them in
/// its size; once the quest is finished, its sale is listed.
/// Behaviour: server.store.sales-for-others-hidden
#[test]
fn a_store_that_hides_sales_for_others_leaves_them_off_the_page_but_counts_them() {
    let mut h = gated_shop(true);
    let (b, _, view) = enter_gated(&mut h);
    let (size, sales) = listed(&view);
    assert_eq!(size, 5, "every sale counted");
    let ordinals: Vec<u32> = sales.iter().map(|p| p[9].1).collect();
    assert_eq!(
        ordinals,
        [0, 3],
        "the race's, the quest's and the level's sales left off"
    );
    let page = |h: &mut Harness| {
        let from = h.bots[b].inbox.len();
        h.bots[b].send(
            true,
            queue::WEENIE,
            &server_call(yalain_world::store::function::PREV_SALES, &[]),
        );
        h.run(10);
        let view = calls_of(h, b, from)
            .into_iter()
            .find(|(f, _)| *f == UPDATE_STORE)
            .expect("the page again")
            .1
            .remove(0);
        listed(&view).1.iter().map(|p| p[9].1).collect::<Vec<u32>>()
    };
    let hold = |h: &mut Harness, status: u32| {
        for p in h.server.players.values_mut() {
            p.play.quests = vec![yalain_world::state::QuestState {
                quest: GATED_QUEST,
                status,
                phase: 0,
                started: 0,
                updated: 0,
            }];
        }
    };
    hold(&mut h, 0x1000_0000);
    assert_eq!(page(&mut h), [0, 3], "the quest under way is not enough");
    hold(&mut h, 0x3000_0000);
    assert_eq!(page(&mut h), [0, 2, 3], "the finished quest's sale listed");
}

/// A store that does not hide them shows the sales a player may not have restricted, and buying
/// one finds no sale and takes nothing.
/// Behaviour: server.store.sales-for-others-restricted
#[test]
fn a_store_shows_sales_for_others_restricted_and_will_not_sell_them() {
    let mut h = gated_shop(false);
    let (b, me, view) = enter_gated(&mut h);
    let (size, sales) = listed(&view);
    assert_eq!(size, 5);
    let restricted: Vec<u32> = sales.iter().map(|p| p[10].1).collect();
    assert_eq!(
        restricted,
        [0, 1, 1, 0, 1],
        "the race's, the quest's and the level's sales restricted"
    );
    for p in h.server.players.values_mut() {
        yalain_world::quals::set_int(&mut p.qualities, 3100, 50);
    }
    let from = h.bots[b].inbox.len();
    let stock = yalain_world::store::STOCK_BASE | u64::from(STORE & 0xFFFF) << 8 | 1;
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
    let done: Vec<u32> = calls_of(&h, b, from)
        .into_iter()
        .filter(|(f, _)| *f == REQUEST_DONE)
        .filter_map(|(_, v)| match v.first() {
            Some(Value::Int(e)) => Some(*e),
            _ => None,
        })
        .collect();
    assert_eq!(done, [45001], "no such sale");
    let paid = h
        .world_messages(b, from)
        .iter()
        .any(|m| matches!(m, ServerMessage::UpdateInt(q) if q.quality == 3100));
    assert!(!paid, "nothing paid");
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

/// A sale of a template the world can't make is refused: nothing is paid and nothing made.
/// A sale bought for what its acquiring does, which the world can't do, is refused and nothing is
/// paid; a sale whose acquiring only moves a quest is sold.
/// Behaviour: server.store.a-sale-the-world-cannot-make-is-not-paid-for
#[test]
fn a_sale_whose_acquiring_the_world_cannot_do_is_refused_and_nothing_is_paid() {
    let mut h = shop();
    let (b, me) = in_store(&mut h);
    for p in h.server.players.values_mut() {
        yalain_world::quals::set_int(&mut p.qualities, 3100, 50);
    }
    let product = h.server.stores.sales_of(STORE)[0].product;
    let buy = |h: &mut Harness| {
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
        let paid = h
            .world_messages(b, from)
            .iter()
            .any(|m| matches!(m, ServerMessage::UpdateInt(q) if q.quality == 3100));
        let refused = calls_of(h, b, from)
            .iter()
            .any(|(f, v)| *f == REQUEST_DONE && v.contains(&Value::Int(45001)));
        (paid, refused)
    };
    let effect = |h: &mut Harness, name: &str| {
        let mut t = h
            .server
            .catalog
            .get(product)
            .cloned()
            .expect("its template");
        t.props.set(
            "Item_AcquireEffect1",
            yalain_content::Prop::Text(name.into()),
        );
        h.server.catalog.insert(t);
    };
    effect(&mut h, "Eff_SummonSomeone");
    assert_eq!(buy(&mut h), (false, true), "refused, nothing paid");
    effect(&mut h, "Eff_AdvanceQuestHuntPhase2");
    assert_eq!(buy(&mut h), (true, false), "a quest's effect is sold");
}

/// Behaviour: server.store.a-sale-the-world-cannot-make-is-not-paid-for
#[test]
fn a_purchase_of_what_the_world_cannot_make_is_refused_and_nothing_is_paid() {
    let mut h = shop();
    let (b, me) = in_store(&mut h);
    for p in h.server.players.values_mut() {
        yalain_world::quals::set_int(&mut p.qualities, 3100, 50);
    }
    h.server.catalog = yalain_content::Catalog::default();
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
    assert!(sent
        .iter()
        .any(|(f, v)| *f == REQUEST_DONE && v.contains(&Value::Int(45001))));
    let msgs = h.world_messages(b, from);
    assert!(
        !msgs
            .iter()
            .any(|m| matches!(m, ServerMessage::UpdateInt(q) if q.quality == 3100)),
        "nothing paid"
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
        wants: None,
        fellows: false,
        levels: None,
        xp_share: 0.0,
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
            timed_phase: 0,
            retry: 0.0,
            xp: 0,
            cancellable: false,
            placed: "authored for a test".into(),
            hidden: false,
            effects: Vec::new(),
            subquests: Vec::new(),
            min_level: 0,
            fails_on_death: false,
            once: false,
            fellows: yalain_content::quests::Fellows::default(),
            last_waits: false,
            hero_only: false,
            pay: yalain_content::quests::CompletionPay::default(),
            required: None,
        }],
        steps: vec![
            step(0, GUARD_TEMPLATE, Update::Bestow),
            step(1, LIEUTENANT_TEMPLATE, Update::Advance(2)),
        ],
        gives: Vec::new(),
        rooms: Vec::new(),
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
        ..ContentPack::default()
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

/// A purchase of more than a stack, or from beyond the storekeeper's reach, finds no such sale, and
/// nothing is paid or made.
/// Behaviour: server.store.purchase-within-a-stack-and-in-reach
#[test]
fn a_purchase_of_too_many_or_from_afar_is_refused() {
    let mut h = shop();
    h.server.stores.sales[0].cost = 0.0;
    let (b, me) = in_store(&mut h);
    let stock = yalain_world::store::stock_id(&h.server.stores.sales[0]);
    let w = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .expect("in the world")
        .0;
    let items = |h: &Harness| h.server.players[&w].play.items.len();
    let ask = |h: &mut Harness, quantity: u32| {
        let mut g = Builder::new();
        g.object(
            0x1B76,
            &[
                Slot::Long(stock),
                Slot::Ref(None),
                Slot::Long(me),
                Slot::Int(quantity),
                Slot::Long(KEEPER),
                Slot::Int(0),
                Slot::Int(0),
            ],
        );
        let from = h.bots[b].inbox.len();
        h.bots[b].send(
            true,
            queue::WEENIE,
            &server_call(PURCHASE, &[Arg::Object(g.graph())]),
        );
        h.run(10);
        calls_of(h, b, from)
            .into_iter()
            .filter(|(f, _)| *f == REQUEST_DONE)
            .filter_map(|(_, v)| match v.first() {
                Some(Value::Int(e)) => Some(*e),
                _ => None,
            })
            .collect::<Vec<u32>>()
    };
    let before = items(&h);
    assert_eq!(ask(&mut h, u32::MAX), [45001], "more than a stack");
    assert_eq!(items(&h), before);
    let at = h.server.players[&w].position;
    h.server
        .players
        .get_mut(&w)
        .expect("in the world")
        .position
        .origin[0] += 100.0;
    assert_eq!(ask(&mut h, 1), [45001], "from afar");
    assert_eq!(items(&h), before);
    h.server.players.get_mut(&w).expect("in the world").position = at;
    assert!(
        ask(&mut h, 1).is_empty() || items(&h) > before,
        "in reach it goes"
    );
    assert_eq!(items(&h), before + 1);
}

/// A shop selling a medic's and a pet keeper's contract, and a player in it with coin: the harness,
/// its client, the player and its world connection.
fn merc_shop(coin: i32) -> (Harness, usize, u64, yalain_net::ConnId) {
    use yalain_world::pets::mercenary;
    let mut h = shop();
    let sale = h.server.stores.sales[0].clone();
    h.server.stores.sales = [mercenary::MEDIC, mercenary::KEEPER]
        .into_iter()
        .enumerate()
        .map(|(i, product)| Sale {
            product,
            ordinal: u32::try_from(i).unwrap_or(0),
            cost: 500.0,
            stack: 5,
            ..sale.clone()
        })
        .collect();
    for (id, makes) in [
        (mercenary::MEDIC, 0x4700_3457),
        (mercenary::KEEPER, 0x4700_3455),
    ] {
        let mut props = yalain_content::Props::default();
        props.set("Icon", yalain_content::Prop::Word(0x4100_7D5C));
        h.server
            .catalog
            .insert(yalain_testkit::test_template(id, 0x1067, props));
        let mut pet = h
            .server
            .catalog
            .get(yalain_testkit::TEST_CREATURE)
            .cloned()
            .expect("a creature");
        pet.id = makes;
        h.server.catalog.insert(pet);
    }
    let (b, me) = in_store(&mut h);
    for p in h.server.players.values_mut() {
        yalain_world::quals::set_int(&mut p.qualities, 3100, coin);
    }
    let world = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .map(|(c, _)| c)
        .expect("in the world");
    (h, b, me, world)
}

/// Ask to buy the contract at an ordinal: whether it was refused.
fn buy_contract(h: &mut Harness, b: usize, me: u64, ordinal: u32) -> bool {
    buy_contracts(h, b, me, ordinal, 1)
}

fn buy_contracts(h: &mut Harness, b: usize, me: u64, ordinal: u32, quantity: u32) -> bool {
    let from = h.bots[b].inbox.len();
    let stock =
        yalain_world::store::STOCK_BASE | u64::from(STORE & 0xFFFF) << 8 | u64::from(ordinal);
    let mut g = Builder::new();
    g.object(
        0x1B76,
        &[
            Slot::Long(stock),
            Slot::Ref(None),
            Slot::Long(me),
            Slot::Int(quantity),
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
    calls_of(h, b, from)
        .iter()
        .any(|(f, v)| *f == REQUEST_DONE && v.contains(&Value::Int(45001)))
}

/// A storekeeper opening and closing for a sale shows to everyone beside it, who take the keeper's
/// visual messages in turn with the buyer.
/// Behaviour: server.store.a-keeper-opens-for-all-who-see-it
#[test]
fn a_keeper_opening_for_a_sale_is_told_to_those_beside_it() {
    let (mut h, b, me, _) = merc_shop(1000);
    let (w, _) = h.player_in_world("two", "Bran");
    h.run(10);
    for sale in 1_u16..=2 {
        let from = h.bots[w].inbox.len();
        assert!(!buy_contract(&mut h, b, me, 0), "bought");
        let opened: Vec<(i32, u16)> = h
            .world_messages(w, from)
            .iter()
            .filter_map(|m| match m {
                ServerMessage::UpdateBoolVisual(q)
                    if q.object.id == KEEPER && q.quality == yalain_world::store::OPEN =>
                {
                    Some((q.value, q.object.other_stamp))
                }
                _ => None,
            })
            .collect();
        let first = 2 * sale;
        assert_eq!(opened, [(1, first), (0, first + 1)], "sale {sale}");
    }
}

fn steps(seconds: f64) -> usize {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let n = (seconds / yalain_testkit::STEP).round() as usize;
    n
}

fn coin(h: &Harness, world: yalain_net::ConnId) -> i32 {
    yalain_world::quals::int(&h.server.players[&world].qualities, 3100).unwrap_or(0)
}

fn contracts(h: &Harness, world: yalain_net::ConnId) -> usize {
    h.server.players[&world]
        .play
        .items
        .iter()
        .filter(|i| yalain_world::pets::is_contract(i.template))
        .count()
}

fn mercs(h: &Harness, me: u64) -> Vec<u32> {
    use yalain_world::pets::mercenary;
    h.server
        .pets
        .by_id
        .values()
        .filter(|p| p.owner == me && p.class == mercenary::CLASS)
        .map(|p| p.template)
        .collect()
}

/// A contract bought is paid for, hires its mercenary at once and is used up; the next dismisses the
/// one before it, so one is hired at a time. The mercenary stands in the world as a pet, on its
/// template's numbers.
/// Behaviour: server.pets.a-contract-hires-a-mercenary
#[test]
fn a_contract_hires_its_mercenary_at_once_and_is_used_up_and_the_next_takes_its_place() {
    let (mut h, b, me, world) = merc_shop(2000);
    assert!(!buy_contract(&mut h, b, me, 0));
    assert_eq!(coin(&h, world), 1500);
    assert_eq!(contracts(&h, world), 0, "used up");
    assert_eq!(mercs(&h, me), [0x4700_3457]);
    let first = *h.server.pets.of(me).first().expect("hired");
    assert!(h.server.things.get(first).is_some_and(|t| t.at.is_some()));
    assert!(!buy_contract(&mut h, b, me, 1));
    assert_eq!(coin(&h, world), 1000);
    assert_eq!(mercs(&h, me), [0x4700_3455], "the medic is dismissed");
    assert!(h.server.things.get(first).is_none());
}

/// Someone who can't hire now is refused the contract: nothing paid, nothing in the pack, no
/// mercenary. A template that can't make it is refused the same.
/// Behaviour: server.pets.a-contract-that-cannot-hire-costs-nothing
#[test]
fn a_contract_that_cannot_hire_now_is_refused_and_costs_nothing() {
    let (mut h, b, me, world) = merc_shop(2000);
    let clock = h.clock();
    yalain_world::death::player_dies(&mut h.server, &mut h.net, world, None, clock);
    h.run(2);
    assert!(buy_contract(&mut h, b, me, 0), "refused the dying");
    assert_eq!((coin(&h, world), contracts(&h, world)), (2000, 0));
    assert!(mercs(&h, me).is_empty());
    let (mut h, b, me, world) = merc_shop(2000);
    if let Some(p) = h.server.players.get_mut(&world) {
        yalain_world::quals::set_int(&mut p.qualities, yalain_world::player::quality::HEALTH, 0);
    }
    assert!(buy_contract(&mut h, b, me, 0), "refused the dead");
    assert_eq!((coin(&h, world), contracts(&h, world)), (2000, 0));
    assert!(mercs(&h, me).is_empty());
    let (mut h, b, me, world) = merc_shop(2000);
    assert!(buy_contracts(&mut h, b, me, 0, 2), "one at a time");
    assert_eq!((coin(&h, world), contracts(&h, world)), (2000, 0));
    h.server.catalog.insert(yalain_testkit::test_template(
        0x4700_3457,
        0x180,
        yalain_content::Props::default(),
    ));
    assert!(
        buy_contract(&mut h, b, me, 0),
        "refused with nothing to make"
    );
    assert_eq!((coin(&h, world), contracts(&h, world)), (2000, 0));
    assert!(mercs(&h, me).is_empty());
}

/// A contract in the pack that can't hire when used stays, the player is told, and it hires when it
/// can; no second mercenary comes of it.
/// Behaviour: server.pets.a-contract-that-cannot-hire-costs-nothing
#[test]
fn a_contract_that_fails_to_hire_is_kept_and_hires_when_it_can() {
    use yalain_world::pets::mercenary;
    let (mut h, b, me, world) = merc_shop(2000);
    yalain_world::inventory::give(&mut h.server, &mut h.net, world, mercenary::MEDIC, 1);
    let id = h.server.players[&world]
        .play
        .items
        .iter()
        .find(|i| i.template == mercenary::MEDIC)
        .expect("given")
        .id;
    let template = h.server.catalog.get(0x4700_3457).cloned().expect("there");
    h.server.catalog.insert(yalain_testkit::test_template(
        0x4700_3457,
        0x180,
        yalain_content::Props::default(),
    ));
    h.bots[b].send(true, queue::WEENIE, &server_call(USE, &[use_of(id, me)]));
    h.run(10);
    assert_eq!(contracts(&h, world), 1, "kept");
    assert!(mercs(&h, me).is_empty());
    h.server.catalog.insert(template);
    h.bots[b].send(true, queue::WEENIE, &server_call(USE, &[use_of(id, me)]));
    h.run(10);
    assert_eq!(contracts(&h, world), 0, "used up");
    assert_eq!(mercs(&h, me).len(), 1);
    h.bots[b].send(true, queue::WEENIE, &server_call(USE, &[use_of(id, me)]));
    h.run(10);
    assert_eq!(mercs(&h, me).len(), 1, "a contract gone hires no more");
}

/// A mercenary goes at its time, with its owner's death, and with its owner leaving the world, and
/// does not come back with its owner.
/// Behaviour: server.pets.a-mercenary-ends
#[test]
fn a_mercenary_ends_with_its_time_its_owners_death_and_its_owner_leaving() {
    let (mut h, b, me, _) = merc_shop(2000);
    assert!(!buy_contract(&mut h, b, me, 0));
    assert_eq!(mercs(&h, me).len(), 1);
    let pet = h.server.pets.of(me)[0];
    for _ in 0..3590 {
        h.step_by(1.0);
    }
    assert_eq!(mercs(&h, me).len(), 1, "not yet");
    for _ in 0..20 {
        h.step_by(1.0);
    }
    assert!(mercs(&h, me).is_empty(), "its time is up");
    assert!(h.server.things.get(pet).is_none());
    let (mut h, b, me, world) = merc_shop(2000);
    assert!(!buy_contract(&mut h, b, me, 0));
    let pet = h.server.pets.of(me)[0];
    let clock = h.clock();
    yalain_world::death::player_dies(&mut h.server, &mut h.net, world, None, clock);
    h.run(2);
    assert!(mercs(&h, me).is_empty(), "its owner died");
    assert!(h.server.things.get(pet).is_none_or(|t| !t.alive()));
    let (mut h, b, me, _) = merc_shop(2000);
    assert!(!buy_contract(&mut h, b, me, 0));
    let pet = h.server.pets.of(me)[0];
    h.bots[b].send(
        false,
        queue::LOGON,
        &auberean_protocol::ClientMessage::LogOff(auberean_protocol::login::CharacterRef {
            character: me,
        }),
    );
    h.run(steps(30.0));
    assert!(mercs(&h, me).is_empty(), "its owner left");
    assert!(h.server.things.get(pet).is_none());
    h.enter_world(b, "one", me);
    h.run(10);
    assert!(mercs(&h, me).is_empty(), "nor is it back");
    for _ in 0..3610 {
        h.step_by(1.0);
    }
    assert!(h.server.things.get(pet).is_none());
    assert!(mercs(&h, me).is_empty());
}

/// A medic tends its owner when hurt, no sooner again than it may; a keeper tends its owner's other
/// pets. A mercenary doesn't take the place of a pet a skill summons.
/// Behaviour: server.pets.a-mercenary-tends
#[test]
fn a_medic_tends_its_owner_and_a_keeper_its_owners_pets() {
    use yalain_world::effect_defs::{EffectDef, Kind, Scale};
    use yalain_world::pets::mercenary;
    use yalain_world::player::quality;
    let (mut h, b, me, world) = merc_shop(2000);
    h.server.effect_table.effects.insert(
        mercenary::TEND_EFFECT,
        EffectDef {
            kind: Kind::InstantVital {
                flags: yalain_world::effect_defs::vital::HEALTH,
                change: Scale(vec![(0.0, 100.0)]),
            },
            ..yalain_testkit::plain_effect(mercenary::TEND_EFFECT)
        },
    );
    let set = |h: &mut Harness, n: i32, of: i32| {
        let p = h.server.players.get_mut(&world).expect("in the world");
        yalain_world::quals::set_int(&mut p.qualities, quality::HEALTH_MAX, of);
        yalain_world::quals::set_int(&mut p.qualities, quality::HEALTH, n);
    };
    let health = |h: &Harness| {
        yalain_world::quals::int(&h.server.players[&world].qualities, quality::HEALTH).unwrap_or(0)
    };
    assert!(!buy_contract(&mut h, b, me, 0));
    set(&mut h, 900, 1000);
    h.run(steps(2.0));
    assert_eq!(health(&h), 900, "hurt too little to tend");
    set(&mut h, 500, 1000);
    h.run(steps(2.0));
    assert_eq!(health(&h), 600, "tended");
    h.run(steps(5.0));
    assert_eq!(health(&h), 600, "not yet again");
    set(&mut h, 1000, 1000);
    h.advance(mercenary::TENDS_EVERY);
    h.run(1);
    set(&mut h, 500, 1000);
    h.run(steps(2.0));
    assert_eq!(health(&h), 600, "again");
    // The keeper tends another pet of its owner's that is hurt.
    assert!(!buy_contract(&mut h, b, me, 1));
    assert_eq!(mercs(&h, me), [0x4700_3455]);
    let keeper = h.server.pets.of(me)[0];
    let waits = h.server.pets.by_id[&keeper].next_tend;
    h.server.things.get_mut(keeper).expect("hired").health = 10;
    h.run(steps(2.0));
    assert_eq!(
        h.server.pets.by_id[&keeper].next_tend, waits,
        "a keeper doesn't tend itself"
    );
    let mut other = h.server.pets.by_id[&keeper].clone();
    other.id = 0x8130_0000_0000_0777;
    other.class = 16;
    other.effect = 0x7000_0805;
    let mut body = h.server.things.get(keeper).expect("in the world").clone();
    body.id = other.id;
    body.max_health = 1000;
    body.health = 500;
    h.server.things.insert(body);
    h.server.pets.by_id.insert(other.id, other);
    h.run(steps(2.0));
    let healed = h.server.things.get(0x8130_0000_0000_0777).map(|t| t.health);
    assert!(healed >= Some(600), "tended: {healed:?}");
    assert_eq!(mercs(&h, me).len(), 1, "the other is no mercenary");
}
