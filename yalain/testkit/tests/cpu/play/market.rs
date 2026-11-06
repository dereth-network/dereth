//! The market: a broker's catalog, an item put up for sale, the sale shown in its store, bought by
//! another player, and its price collected.

use auberean_protocol::ServerMessage;
use auberean_protocol::ServerMessage as M;
use auberean_script::args::Arg;
use yalain_content::{ContentObject, ContentPack, Description, Facts, Kind, Props};
use yalain_store::Store as _;
use yalain_testkit::play::use_of;
use yalain_testkit::{test_template, Harness};
use yalain_world::graph::{Builder, Slot};
use yalain_world::market::{function, Broker};
use yalain_world::places::function::USE_DONE;
use yalain_world::store::function::{ENTER_STORE, PURCHASE};

const BROKER: u64 = 0x40FE_FA00_0000_0003;
const BROKER_TEMPLATE: u32 = 0x4700_2E8B;
const CATALOG: u32 = 0x6F00_1503;
const STORE: u32 = 0x6F00_150B;
const GEM: u32 = 0x4700_0AFD;

fn broker() -> ContentObject {
    ContentObject {
        object: Some(BROKER),
        template: BROKER_TEMPLATE,
        description: Description {
            name_text: None,
            package: 0x13A3,
            look: 0x1F00_0100,
            enter_fx: 0,
            scale: None,
            name: None,
            numbers: [0; 6],
            source: "authored for a test".into(),
        },
        cell: 0x0299_0109,
        origin: [61.0, 240.2, -44.6],
        rotation: [1.0, 0.0, 0.0, 0.0],
        kind: Kind::Static,
        label: String::new(),
        placed: "authored for a test".into(),
        destination: None,
        facts: Facts::default(),
        walks: None,
        gate: None,
    }
}

fn world() -> Harness {
    let mut h = Harness::new();
    h.server.content = ContentPack {
        objects: vec![broker()],
        ..ContentPack::default()
    };
    h.server.brokers.insert(
        BROKER_TEMPLATE,
        Broker {
            catalog: CATALOG,
            location: (0x0D3C_90C4, 0x2500_0CE6),
        },
    );
    h.server
        .catalog
        .insert(test_template(GEM, 0x1067, Props::default()));
    h
}

fn at_the_broker(h: &mut Harness, id: u64) {
    let p = h
        .server
        .players
        .values_mut()
        .find(|p| p.character == id)
        .expect("in the world");
    p.position.cell = 0x0299_0109;
    p.position.origin = [59.06, 240.2, -44.6];
}

fn money(h: &mut Harness, id: u64, amount: i32) {
    let p = h
        .server
        .players
        .values_mut()
        .find(|p| p.character == id)
        .expect("in the world");
    yalain_world::quals::set_int(&mut p.qualities, 3100, amount);
}

fn last_money(h: &Harness, b: usize, from: usize) -> Option<i32> {
    h.world_messages(b, from)
        .iter()
        .rev()
        .find_map(|m| match m {
            M::UpdateInt(q) if q.quality == 3100 => Some(q.value),
            _ => None,
        })
}

/// Using a broker answers the use, then shows its catalog; an item put up for sale takes a
/// hundredth of what it would fetch, moves to the broker and shows the consignment; another player
/// sees the sale in its store and buys it (the price taken, the item made in its pack, the sale's
/// stock told as gone); the seller collects the price.
/// Behaviour: server.market.catalog-consignments-and-player-sales
#[test]
fn an_item_put_up_with_a_broker_is_bought_by_another_player_and_its_price_collected() {
    let mut h = world();
    let (a, ida) = h.player_in_world("one", "Seller");
    let (b, idb) = h.player_in_world("two", "Buyer");
    at_the_broker(&mut h, ida);
    at_the_broker(&mut h, idb);
    money(&mut h, ida, 1_000);
    money(&mut h, idb, 5_000);
    // The catalog.
    let from = h.bots[a].inbox.len();
    h.ask(
        a,
        yalain_world::requests::function::USE,
        &[use_of(BROKER, ida)],
        10,
    );
    let f = h.functions(a, from);
    let done = f.iter().position(|x| *x == USE_DONE).expect("answered");
    let cat = f
        .iter()
        .position(|x| *x == function::ENTER_CATALOG)
        .expect("the catalog");
    assert!(done < cat);
    // An item put up: 32 for 100 each, a charge of 32.
    let gem = yalain_world::inventory::make_item(&mut h.server, GEM, 32, 0).expect("made");
    let p = h
        .server
        .players
        .values_mut()
        .find(|p| p.character == ida)
        .expect("in the world");
    p.play.items.push(yalain_world::state::Held {
        id: gem,
        template: GEM,
        slot: 0,
        location: 0,
        quantity: 32,
    });
    let from = h.bots[a].inbox.len();
    h.ask(
        a,
        function::SELL,
        &[
            Arg::Int(STORE),
            Arg::Int(100),
            Arg::Int(32),
            Arg::LongInt(gem),
            Arg::LongInt(BROKER),
        ],
        10,
    );
    assert_eq!(last_money(&h, a, from), Some(968));
    let f = h.functions(a, from);
    assert!(f.contains(&0x820E_005E) && f.contains(&function::UPDATE_CONSIGNMENT));
    assert_eq!(h.server.market.list.len(), 1);
    // The other player finds it in the store and buys it.
    let from = h.bots[b].inbox.len();
    h.ask(
        b,
        function::ENTER_STORE,
        &[Arg::Int(STORE), Arg::LongInt(BROKER)],
        10,
    );
    assert!(h.functions(b, from).contains(&ENTER_STORE));
    let mut g = Builder::new();
    g.object(
        0x1B76,
        &[
            Slot::Long(gem),
            Slot::Ref(None),
            Slot::Long(idb),
            Slot::Int(32),
            Slot::Long(BROKER),
            Slot::Int(0),
            Slot::Int(0),
        ],
    );
    let from = h.bots[b].inbox.len();
    h.ask(b, PURCHASE, &[Arg::Object(g.graph())], 10);
    assert_eq!(last_money(&h, b, from), Some(1_800));
    assert!(h.functions(b, from).contains(&function::UPDATE_SALE));
    assert!(h
        .world_messages(b, from)
        .iter()
        .any(|m| matches!(m, ServerMessage::CreateObject(o) if o.id == gem)));
    // The seller collects the price.
    let sale = h.server.market.list[0].sale;
    let from = h.bots[a].inbox.len();
    h.ask(
        a,
        function::COLLECT,
        &[Arg::Int(1), Arg::Int(sale), Arg::LongInt(BROKER)],
        10,
    );
    assert_eq!(last_money(&h, a, from), Some(968 + 3_200));
    assert!(h.server.market.list.is_empty());
}

/// A sword put up with a broker: its seller and what its level gave it.
fn put_up_a_made_item(h: &mut Harness, a: usize, ida: u64) -> u64 {
    let sword = yalain_world::inventory::make_item(&mut h.server, GEM, 1, 41).expect("made");
    {
        let t = h.server.things.get_mut(sword).expect("made");
        t.own.min_level = Some(20);
        t.own.made = Some(yalain_world::made::Made {
            value: 41,
            armor: Some(33),
            ..yalain_world::made::Made::default()
        });
        t.crafter = Some("Seller".to_owned());
    }
    let p = h
        .server
        .players
        .values_mut()
        .find(|p| p.character == ida)
        .expect("in the world");
    p.play.items.push(yalain_world::state::Held {
        id: sword,
        template: GEM,
        slot: 0,
        location: 0,
        quantity: 1,
    });
    h.ask(
        a,
        function::SELL,
        &[
            Arg::Int(STORE),
            Arg::Int(100),
            Arg::Int(1),
            Arg::LongInt(sword),
            Arg::LongInt(BROKER),
        ],
        10,
    );
    assert_eq!(h.server.market.list.len(), 1);
    sword
}

/// A consignment taken back unsold is the item that went up: its value, what its level gave it
/// and its maker stay with it.
/// Behaviour: server.market.a-consigned-item-keeps-what-it-holds
#[test]
fn an_item_taken_back_from_a_broker_keeps_what_it_holds_of_its_own() {
    let mut h = world();
    let (a, ida) = h.player_in_world("one", "Seller");
    at_the_broker(&mut h, ida);
    money(&mut h, ida, 1_000);
    let sword = put_up_a_made_item(&mut h, a, ida);
    let sale = h.server.market.list[0].sale;
    h.ask(
        a,
        function::COLLECT,
        &[Arg::Int(0), Arg::Int(sale), Arg::LongInt(BROKER)],
        10,
    );
    assert!(h.server.market.list.is_empty());
    let p = h.server.players.values().next().expect("in the world");
    let back = p
        .play
        .items
        .iter()
        .find(|i| i.template == GEM)
        .expect("back in the pack");
    let t = h.server.things.get(back.id).expect("in the world");
    assert_eq!(t.value, 41, "its value (the item was {sword:#x})");
    assert_eq!(t.own.min_level, Some(20));
    assert_eq!(t.own.made.and_then(|m| m.armor), Some(33));
    assert_eq!(t.crafter.as_deref(), Some("Seller"));
}

/// Part of a stack put up holds what the stack held of its own.
/// Behaviour: server.inventory.a-part-split-off-holds-the-stacks-numbers
#[test]
fn part_of_a_stack_put_up_with_a_broker_keeps_the_stacks_numbers() {
    let mut h = world();
    let (a, ida) = h.player_in_world("one", "Seller");
    at_the_broker(&mut h, ida);
    money(&mut h, ida, 1_000);
    let stack = yalain_world::inventory::make_item(&mut h.server, GEM, 5, 41).expect("made");
    h.server.things.get_mut(stack).expect("made").crafter = Some("Seller".to_owned());
    let p = h
        .server
        .players
        .values_mut()
        .find(|p| p.character == ida)
        .expect("in the world");
    p.play.items.push(yalain_world::state::Held {
        id: stack,
        template: GEM,
        slot: 0,
        location: 0,
        quantity: 5,
    });
    h.ask(
        a,
        function::SELL,
        &[
            Arg::Int(STORE),
            Arg::Int(100),
            Arg::Int(2),
            Arg::LongInt(stack),
            Arg::LongInt(BROKER),
        ],
        10,
    );
    let sale = h.server.market.list[0];
    assert_ne!(sale.item, stack, "a new item");
    let t = h.server.things.get(sale.item).expect("with the broker");
    assert_eq!(
        (t.quantity, t.value, t.crafter.as_deref()),
        (2, 41, Some("Seller"))
    );
    assert_eq!(h.server.things.get(stack).expect("kept").quantity, 3);
}

/// A consignment is taken back only in reach of its broker: from afar nothing moves and it stays up.
/// Behaviour: server.market.dealings-in-reach-and-with-room
#[test]
fn a_consignment_is_collected_only_in_reach_of_its_broker() {
    let mut h = world();
    let (a, ida) = h.player_in_world("one", "Seller");
    at_the_broker(&mut h, ida);
    money(&mut h, ida, 1_000);
    put_up_a_made_item(&mut h, a, ida);
    let sale = h.server.market.list[0].sale;
    let collect = [Arg::Int(0), Arg::Int(sale), Arg::LongInt(BROKER)];
    let world = *h.server.players.keys().next().expect("in the world");
    let at = h.server.players[&world].position;
    let held = h.server.players[&world].play.items.len();
    h.server
        .players
        .get_mut(&world)
        .expect("in")
        .position
        .origin[0] += 100.0;
    h.ask(a, function::COLLECT, &collect, 10);
    assert_eq!(h.server.market.list.len(), 1, "not from afar");
    assert_eq!(h.server.players[&world].play.items.len(), held);
    h.server.players.get_mut(&world).expect("in").position = at;
    h.ask(a, function::COLLECT, &collect, 10);
    assert!(h.server.market.list.is_empty(), "in reach");
    assert_eq!(h.server.players[&world].play.items.len(), held + 1);
}

/// A seller collecting the price of what another player bought leaves the buyer's item alone.
/// Behaviour: server.market.a-consigned-item-keeps-what-it-holds
#[test]
fn a_seller_collecting_its_price_leaves_the_buyers_item_in_the_world() {
    let mut h = world();
    let (a, ida) = h.player_in_world("one", "Seller");
    let (b, idb) = h.player_in_world("two", "Buyer");
    at_the_broker(&mut h, ida);
    at_the_broker(&mut h, idb);
    money(&mut h, ida, 1_000);
    money(&mut h, idb, 5_000);
    let sword = put_up_a_made_item(&mut h, a, ida);
    h.ask(
        b,
        function::ENTER_STORE,
        &[Arg::Int(STORE), Arg::LongInt(BROKER)],
        10,
    );
    let mut g = Builder::new();
    g.object(
        0x1B76,
        &[
            Slot::Long(sword),
            Slot::Ref(None),
            Slot::Long(idb),
            Slot::Int(1),
            Slot::Long(BROKER),
            Slot::Int(0),
            Slot::Int(0),
        ],
    );
    h.ask(b, PURCHASE, &[Arg::Object(g.graph())], 10);
    let sale = h.server.market.list[0].sale;
    assert_eq!(
        h.server.things.get(sword).map(|t| t.value),
        Some(41),
        "bought"
    );
    h.ask(
        a,
        function::COLLECT,
        &[Arg::Int(1), Arg::Int(sale), Arg::LongInt(BROKER)],
        10,
    );
    assert!(h.server.market.list.is_empty());
    let t = h.server.things.get(sword).expect("still the buyer's");
    assert_eq!(t.value, 41);
    assert_eq!(t.own.min_level, Some(20));
    assert_eq!(t.own.made.and_then(|m| m.armor), Some(33));
}

/// A player buys the one sale put up with a broker.
fn buy(h: &mut Harness, b: usize, idb: u64, item: u64) {
    let mut g = Builder::new();
    g.object(
        0x1B76,
        &[
            Slot::Long(item),
            Slot::Ref(None),
            Slot::Long(idb),
            Slot::Int(1),
            Slot::Long(BROKER),
            Slot::Int(0),
            Slot::Int(0),
        ],
    );
    h.ask(b, PURCHASE, &[Arg::Object(g.graph())], 10);
}

/// A sale bought while its seller is in the world: the seller is told which sale and how many have
/// sold, at that broker, and the buyer isn't.
/// Behaviour: server.market.a-seller-is-told-of-a-sale
#[test]
fn a_seller_in_the_world_is_told_of_a_sale_as_it_is_bought() {
    use yalain_world::calls::{Node, Value};
    let mut h = world();
    let (a, ida) = h.player_in_world("one", "Seller");
    let (b, idb) = h.player_in_world("two", "Buyer");
    at_the_broker(&mut h, ida);
    at_the_broker(&mut h, idb);
    money(&mut h, ida, 1_000);
    money(&mut h, idb, 5_000);
    let sword = put_up_a_made_item(&mut h, a, ida);
    let sale = h.server.market.list[0].sale;
    let (from_a, from_b) = (h.bots[a].inbox.len(), h.bots[b].inbox.len());
    buy(&mut h, b, idb, sword);
    assert_eq!(h.server.market.list[0].sold, 1, "bought");
    assert!(!h
        .functions(b, from_b)
        .contains(&function::UPDATE_SALE_REMINDERS));
    let told: Vec<Vec<Value>> = h
        .calls(a, from_a)
        .into_iter()
        .filter(|(f, _)| *f == function::UPDATE_SALE_REMINDERS)
        .map(|(_, v)| v)
        .collect();
    assert_eq!(told.len(), 1, "told once");
    assert!(told[0]
        .iter()
        .any(|v| matches!(v, Value::Long(k) if *k == BROKER)));
    let desc: Vec<u32> = told[0]
        .iter()
        .find_map(|v| match v {
            Value::Graph(g) => g.iter().find_map(|(_, n)| match n {
                Node::Script { package, slots }
                    if *package == yalain_world::market::CONSIGNMENT_DESC.0 =>
                {
                    Some(slots.iter().map(|(_, word)| *word).collect())
                }
                _ => None,
            }),
            _ => None,
        })
        .expect("the sale's words");
    assert_eq!(desc, [sale, 0, 1], "the sale, not expired, one sold");
}

/// A consignment tells its seller how many are left and how many have sold, in the consignments'
/// page and the reminder alike: all left before a sale, none after it.
/// Behaviour: server.market.a-consignment-says-how-many-are-left
#[test]
fn a_consignment_tells_its_seller_how_many_are_left_and_how_many_sold() {
    use yalain_world::calls::{Node, Value};
    let mut h = world();
    let (a, ida) = h.player_in_world("one", "Seller");
    let (b, idb) = h.player_in_world("two", "Buyer");
    at_the_broker(&mut h, ida);
    at_the_broker(&mut h, idb);
    money(&mut h, ida, 1_000);
    money(&mut h, idb, 5_000);
    let sword = put_up_a_made_item(&mut h, a, ida);
    // Each consignment's left and sold, in the page and the reminder the seller is sent.
    let shown = |h: &mut Harness| -> Vec<(u32, u32)> {
        let from = h.bots[a].inbox.len();
        h.ask(a, function::ENTER_CONSIGNMENT, &[Arg::LongInt(BROKER)], 10);
        let mut found = Vec::new();
        for (_, values) in h.calls(a, from).into_iter().filter(|(f, _)| {
            *f == function::ENTER_CONSIGNMENT_DONE || *f == function::SALE_REMINDERS
        }) {
            for v in &values {
                let Value::Graph(g) = v else { continue };
                for (_, n) in g {
                    if let Node::Script { package, slots } = n {
                        if *package == yalain_world::market::CONSIGNMENT.0 {
                            found.push((slots[4].1, slots[5].1));
                        }
                    }
                }
            }
        }
        found
    };
    assert_eq!(shown(&mut h), [(1, 0), (1, 0)], "the page and the reminder");
    buy(&mut h, b, idb, sword);
    assert_eq!(shown(&mut h), [(0, 1), (0, 1)], "none left, one sold");
}

/// A sale whose seller is away goes as any other, and no one is told.
/// Behaviour: server.market.a-seller-is-told-of-a-sale
#[test]
fn a_sale_bought_while_its_seller_is_away_tells_no_one() {
    let mut h = world();
    let (a, ida) = h.player_in_world("one", "Seller");
    let (b, idb) = h.player_in_world("two", "Buyer");
    at_the_broker(&mut h, ida);
    at_the_broker(&mut h, idb);
    money(&mut h, ida, 1_000);
    money(&mut h, idb, 5_000);
    let sword = put_up_a_made_item(&mut h, a, ida);
    h.server.market.list[0].owner = 0x2130_0000_0000_0FFF;
    let (from_a, from_b) = (h.bots[a].inbox.len(), h.bots[b].inbox.len());
    buy(&mut h, b, idb, sword);
    assert_eq!(h.server.market.list[0].sold, 1, "bought");
    for f in [h.functions(a, from_a), h.functions(b, from_b)] {
        assert!(!f.contains(&function::UPDATE_SALE_REMINDERS));
    }
}

/// The world forgets its market, as a restart does, and reads it back from the store.
fn restart_the_market(h: &mut Harness, item: u64) {
    h.run(1);
    h.server.market = yalain_world::market::Market::default();
    h.server.things.remove(item);
    assert!(yalain_world::market::ensure(&mut h.server), "reads");
}

/// A restart leaves an item on sale as it was: its value, what its level gave it and its maker
/// come back with it from the world's record.
/// Behaviour: server.market.a-consigned-item-keeps-what-it-holds
#[test]
fn an_item_on_sale_keeps_what_it_holds_across_a_restart() {
    let mut h = world();
    let (a, ida) = h.player_in_world("one", "Seller");
    at_the_broker(&mut h, ida);
    money(&mut h, ida, 1_000);
    let sword = put_up_a_made_item(&mut h, a, ida);
    restart_the_market(&mut h, sword);
    assert_eq!(h.server.market.list.len(), 1, "still on sale");
    let t = h.server.things.get(sword).expect("back with its broker");
    assert_eq!(t.value, 41);
    assert_eq!(t.own.min_level, Some(20));
    assert_eq!(t.own.made.and_then(|m| m.armor), Some(33));
    assert_eq!(t.crafter.as_deref(), Some("Seller"));
}

/// The one consignment as a record kept before the record held an item's own numbers: the version,
/// the next sale, the count, then its fifteen words.
fn record_of_the_first_form(item: u64, sold: u32) -> Vec<u8> {
    let mut out = vec![1u8];
    out.extend(8u32.to_le_bytes());
    out.extend(1u32.to_le_bytes());
    out.extend(7u32.to_le_bytes());
    out.extend(0x5000_0001u64.to_le_bytes());
    out.extend(BROKER.to_le_bytes());
    out.extend(CATALOG.to_le_bytes());
    out.extend(STORE.to_le_bytes());
    out.extend(item.to_le_bytes());
    out.extend(GEM.to_le_bytes());
    out.extend(100f32.to_bits().to_le_bytes());
    out.extend(3u32.to_le_bytes());
    out.extend(sold.to_le_bytes());
    out.extend(0f64.to_bits().to_le_bytes());
    out
}

/// A record kept before the record held an item's own numbers still reads: its sale stands, and its
/// item comes back as its template and count, all that record knows of it.
/// Behaviour: server.market.a-consigned-item-keeps-what-it-holds
#[test]
fn a_consignments_record_of_the_first_form_still_reads() {
    let mut h = world();
    let item = 0x8130_ED00_B200_0F01;
    h.server
        .store
        .set_world_record(
            yalain_world::market::RECORD,
            &record_of_the_first_form(item, 0),
        )
        .expect("kept");
    assert!(yalain_world::market::ensure(&mut h.server), "reads");
    assert_eq!(h.server.market.list.len(), 1);
    assert_eq!(h.server.market.next_sale, 8);
    let t = h.server.things.get(item).expect("with its broker");
    assert_eq!((t.template, t.quantity, t.value), (GEM, 3, 0));
}

/// A sale sold out and not yet collected makes no item at a restart: the item is its buyer's, and
/// comes back with its buyer.
/// Behaviour: server.market.a-consigned-item-keeps-what-it-holds
#[test]
fn a_sold_item_is_not_made_again_for_its_broker_at_a_restart() {
    let mut h = world();
    let item = 0x8130_ED00_B200_0F01;
    h.server
        .store
        .set_world_record(
            yalain_world::market::RECORD,
            &record_of_the_first_form(item, 3),
        )
        .expect("kept");
    assert!(yalain_world::market::ensure(&mut h.server), "reads");
    assert_eq!(h.server.market.list.len(), 1, "its price still to collect");
    assert!(h.server.things.get(item).is_none());
}

/// A broker's dealings from beyond its reach do nothing; a sale bought into a full pack is refused
/// for want of room and neither paid nor moved.
/// Behaviour: server.market.dealings-in-reach-and-with-room
#[test]
fn a_brokers_dealings_need_reach_and_room() {
    let mut h = world();
    let (a, ida) = h.player_in_world("one", "Seller");
    let (b, idb) = h.player_in_world("two", "Buyer");
    at_the_broker(&mut h, ida);
    at_the_broker(&mut h, idb);
    money(&mut h, ida, 1_000);
    money(&mut h, idb, 5_000);
    let wa = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == ida)
        .expect("in")
        .0;
    let wb = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == idb)
        .expect("in")
        .0;
    let gem = yalain_world::inventory::make_item(&mut h.server, GEM, 8, 0).expect("made");
    h.server
        .players
        .get_mut(&wa)
        .expect("in")
        .play
        .items
        .push(yalain_world::state::Held {
            id: gem,
            template: GEM,
            slot: 0,
            location: 0,
            quantity: 8,
        });
    let sell = [
        Arg::Int(STORE),
        Arg::Int(100),
        Arg::Int(8),
        Arg::LongInt(gem),
        Arg::LongInt(BROKER),
    ];
    let at = h.server.players[&wa].position;
    h.server.players.get_mut(&wa).expect("in").position.origin[0] += 100.0;
    h.ask(a, function::SELL, &sell, 10);
    assert!(h.server.market.list.is_empty(), "not from afar");
    h.server.players.get_mut(&wa).expect("in").position = at;
    h.ask(a, function::SELL, &sell, 10);
    assert_eq!(h.server.market.list.len(), 1, "in reach");
    // A full pack buys nothing.
    let room = yalain_world::inventory::PACK_ROOM as usize;
    while h.server.players[&wb].play.items.len() < room {
        yalain_world::inventory::give(&mut h.server, &mut h.net, wb, GEM, 1);
    }
    let mut g = Builder::new();
    g.object(
        0x1B76,
        &[
            Slot::Long(gem),
            Slot::Ref(None),
            Slot::Long(idb),
            Slot::Int(8),
            Slot::Long(BROKER),
            Slot::Int(0),
            Slot::Int(0),
        ],
    );
    h.ask(b, PURCHASE, &[Arg::Object(g.graph())], 10);
    let p = &h.server.players[&wb];
    assert_eq!(p.play.items.len(), room);
    assert_eq!(yalain_world::store::money(&p.qualities), 5_000, "not paid");
    assert_eq!(h.server.market.list[0].sold, 0, "not sold");
}

fn world_of(h: &Harness, id: u64) -> yalain_net::ConnId {
    *h.server
        .players
        .iter()
        .find(|(_, p)| p.character == id)
        .expect("in the world")
        .0
}

/// The error of the last store answer a player has had since `from`.
fn last_error(h: &Harness, b: usize, from: usize) -> Option<u32> {
    h.calls(b, from)
        .iter()
        .rev()
        .find(|(f, _)| *f == yalain_world::store::function::REQUEST_DONE)
        .and_then(|(_, args)| match args.first() {
            Some(yalain_world::calls::Value::Int(e)) => Some(*e),
            _ => None,
        })
}

/// A sold consignment stays to be collected: it comes back to no one, its buyer's item stays
/// whole, and its price is paid when it is asked for.
/// Behaviour: server.market.a-consignment-comes-back-only-unsold-with-room-and-its-template
#[test]
fn a_sold_consignment_stays_up_with_its_price_and_its_buyers_item_stays_whole() {
    let mut h = world();
    let (a, ida) = h.player_in_world("one", "Seller");
    let (b, idb) = h.player_in_world("two", "Buyer");
    at_the_broker(&mut h, ida);
    at_the_broker(&mut h, idb);
    money(&mut h, ida, 1_000);
    money(&mut h, idb, 5_000);
    let sword = put_up_a_made_item(&mut h, a, ida);
    let mut g = Builder::new();
    g.object(
        0x1B76,
        &[
            Slot::Long(sword),
            Slot::Ref(None),
            Slot::Long(idb),
            Slot::Int(1),
            Slot::Long(BROKER),
            Slot::Int(0),
            Slot::Int(0),
        ],
    );
    h.ask(b, PURCHASE, &[Arg::Object(g.graph())], 10);
    let sale = h.server.market.list[0].sale;
    let wa = world_of(&h, ida);
    let held = h.server.players[&wa].play.items.len();
    let from = h.bots[a].inbox.len();
    h.ask(
        a,
        function::COLLECT,
        &[Arg::Int(0), Arg::Int(sale), Arg::LongInt(BROKER)],
        10,
    );
    assert_eq!(
        last_error(&h, a, from),
        Some(yalain_world::store::error::COLLECT_FAILED)
    );
    assert!(h.functions(a, from).contains(&function::UPDATE_CONSIGNMENT));
    assert_eq!(h.server.market.list.len(), 1, "stays up");
    assert_eq!(h.server.players[&wa].play.items.len(), held);
    let t = h.server.things.get(sword).expect("still the buyer's");
    assert_eq!(t.value, 41);
    assert_eq!(t.own.made.and_then(|m| m.armor), Some(33));
    let wb = world_of(&h, idb);
    yalain_world::world::keep_player(&mut h.server, wb);
    let st = yalain_world::saves::state_of(&mut h.server, idb)
        .expect("reads")
        .expect("kept with its buyer");
    let play = yalain_world::state::PlayState::from_bytes(&st.play).expect("reads");
    assert!(play.item_states.iter().any(|k| k.id == sword));
    // Its price is still there.
    h.ask(
        a,
        function::COLLECT,
        &[Arg::Int(1), Arg::Int(sale), Arg::LongInt(BROKER)],
        10,
    );
    assert!(h.server.market.list.is_empty());
    assert!(h.server.things.get(sword).is_some());
}

/// An unsold consignment stays up while its seller's pack has no place for it, and comes back when
/// it has.
/// Behaviour: server.market.a-consignment-comes-back-only-unsold-with-room-and-its-template
#[test]
fn an_unsold_consignment_stays_up_until_its_sellers_pack_has_room() {
    let mut h = world();
    let (a, ida) = h.player_in_world("one", "Seller");
    at_the_broker(&mut h, ida);
    money(&mut h, ida, 1_000);
    put_up_a_made_item(&mut h, a, ida);
    let w = world_of(&h, ida);
    let room = yalain_world::inventory::PACK_ROOM as usize;
    while h.server.players[&w].play.items.len() < room {
        yalain_world::inventory::give(&mut h.server, &mut h.net, w, GEM, 1);
    }
    let sale = h.server.market.list[0].sale;
    let collect = [Arg::Int(0), Arg::Int(sale), Arg::LongInt(BROKER)];
    let from = h.bots[a].inbox.len();
    h.ask(a, function::COLLECT, &collect, 10);
    assert_eq!(
        last_error(&h, a, from),
        Some(yalain_world::store::error::NO_ROOM)
    );
    assert_eq!(h.server.market.list.len(), 1, "stays up");
    assert_eq!(h.server.players[&w].play.items.len(), room);
    // A place made, it comes back whole.
    let one = h.server.players[&w].play.items[0].id;
    yalain_world::inventory::take_item(&mut h.server, &mut h.net, w, one, GEM);
    h.ask(a, function::COLLECT, &collect, 10);
    assert!(h.server.market.list.is_empty());
    let p = &h.server.players[&w];
    assert_eq!(p.play.items.len(), room);
    assert!(p
        .play
        .items
        .iter()
        .all(|i| i.slot < yalain_world::inventory::PACK_ROOM));
    assert!(p
        .play
        .items
        .iter()
        .any(|i| h.server.things.get(i.id).is_some_and(|t| t.value == 41)));
}

/// A sale of a template the world no longer holds stays up, kept whole: it comes back to its seller
/// once the template does, what it held of its own with it, whatever else was kept meanwhile.
/// Behaviour: server.market.a-consignment-comes-back-only-unsold-with-room-and-its-template
#[test]
fn a_consignment_whose_template_has_gone_stays_up_and_comes_back_whole_when_it_returns() {
    let mut h = world();
    let (a, ida) = h.player_in_world("one", "Seller");
    at_the_broker(&mut h, ida);
    money(&mut h, ida, 1_000);
    let sword = put_up_a_made_item(&mut h, a, ida);
    let sale = h.server.market.list[0].sale;
    let w = world_of(&h, ida);
    let collect = [Arg::Int(0), Arg::Int(sale), Arg::LongInt(BROKER)];
    // In the same running world.
    let whole = std::mem::take(&mut h.server.catalog);
    let from = h.bots[a].inbox.len();
    h.ask(a, function::COLLECT, &collect, 10);
    assert_eq!(
        last_error(&h, a, from),
        Some(yalain_world::store::error::COLLECT_FAILED)
    );
    assert_eq!(h.server.market.list.len(), 1, "stays up");
    assert!(
        h.server.things.get(sword).is_some(),
        "still with its broker"
    );
    // Across a restart with it gone: not in the world, and a save of the market keeps it.
    restart_the_market(&mut h, sword);
    assert!(h.server.things.get(sword).is_none());
    let from = h.bots[a].inbox.len();
    h.ask(a, function::COLLECT, &collect, 10);
    assert_eq!(
        last_error(&h, a, from),
        Some(yalain_world::store::error::COLLECT_FAILED)
    );
    h.ask(
        a,
        function::COLLECT,
        &[Arg::Int(1), Arg::Int(sale), Arg::LongInt(BROKER)],
        10,
    );
    assert_eq!(h.server.market.list.len(), 1, "still up");
    // The template back.
    h.server.catalog = whole;
    restart_the_market(&mut h, sword);
    h.ask(a, function::COLLECT, &collect, 10);
    assert!(h.server.market.list.is_empty());
    let back = h.server.players[&w]
        .play
        .items
        .iter()
        .find(|i| i.template == GEM)
        .expect("back in the pack");
    let t = h.server.things.get(back.id).expect("in the world");
    assert_eq!(t.value, 41);
    assert_eq!(t.own.made.and_then(|m| m.armor), Some(33));
    assert_eq!(t.crafter.as_deref(), Some("Seller"));
}

/// A sale of a template the world no longer holds is not sold: nothing is paid and nothing moves.
/// Behaviour: server.market.a-consignment-comes-back-only-unsold-with-room-and-its-template
#[test]
fn a_sale_whose_template_has_gone_is_not_bought_or_paid_for() {
    let mut h = world();
    let (a, ida) = h.player_in_world("one", "Seller");
    let (b, idb) = h.player_in_world("two", "Buyer");
    at_the_broker(&mut h, ida);
    at_the_broker(&mut h, idb);
    money(&mut h, ida, 1_000);
    money(&mut h, idb, 5_000);
    let sword = put_up_a_made_item(&mut h, a, ida);
    h.server.catalog = yalain_content::Catalog::default();
    restart_the_market(&mut h, sword);
    let wb = world_of(&h, idb);
    let held = h.server.players[&wb].play.items.len();
    let mut g = Builder::new();
    g.object(
        0x1B76,
        &[
            Slot::Long(sword),
            Slot::Ref(None),
            Slot::Long(idb),
            Slot::Int(1),
            Slot::Long(BROKER),
            Slot::Int(0),
            Slot::Int(0),
        ],
    );
    let from = h.bots[b].inbox.len();
    h.ask(b, PURCHASE, &[Arg::Object(g.graph())], 10);
    assert_eq!(
        last_error(&h, b, from),
        Some(yalain_world::store::error::NOT_FOUND)
    );
    assert_eq!(last_money(&h, b, from), None, "nothing paid");
    assert_eq!(h.server.players[&wb].play.items.len(), held);
    assert_eq!(h.server.market.list[0].sold, 0);
}

/// Part of a stack of a template the world no longer holds is not put up: nothing is charged and
/// the stack stays whole.
/// Behaviour: server.market.a-consignment-comes-back-only-unsold-with-room-and-its-template
#[test]
fn part_of_a_stack_whose_template_has_gone_is_not_put_up() {
    let mut h = world();
    let (a, ida) = h.player_in_world("one", "Seller");
    at_the_broker(&mut h, ida);
    money(&mut h, ida, 1_000);
    let stack = yalain_world::inventory::make_item(&mut h.server, GEM, 5, 0).expect("made");
    let w = world_of(&h, ida);
    h.server
        .players
        .get_mut(&w)
        .expect("in")
        .play
        .items
        .push(yalain_world::state::Held {
            id: stack,
            template: GEM,
            slot: 0,
            location: 0,
            quantity: 5,
        });
    h.server.catalog = yalain_content::Catalog::default();
    let from = h.bots[a].inbox.len();
    h.ask(
        a,
        function::SELL,
        &[
            Arg::Int(STORE),
            Arg::Int(100),
            Arg::Int(2),
            Arg::LongInt(stack),
            Arg::LongInt(BROKER),
        ],
        10,
    );
    assert!(h.server.market.list.is_empty());
    assert_eq!(last_money(&h, a, from), None, "nothing charged");
    assert_eq!(h.server.things.get(stack).expect("kept").quantity, 5);
    assert_eq!(h.server.players[&w].play.items[0].quantity, 5);
}

/// A price collected past what money can count leaves the seller at the most it can.
/// Behaviour: server.market.catalog-consignments-and-player-sales
#[test]
fn a_price_collected_past_what_money_counts_stops_at_the_most() {
    let mut h = world();
    let (a, ida) = h.player_in_world("one", "Seller");
    let (b, idb) = h.player_in_world("two", "Buyer");
    at_the_broker(&mut h, ida);
    at_the_broker(&mut h, idb);
    money(&mut h, ida, 1_000);
    money(&mut h, idb, 5_000);
    let sword = put_up_a_made_item(&mut h, a, ida);
    let mut g = Builder::new();
    g.object(
        0x1B76,
        &[
            Slot::Long(sword),
            Slot::Ref(None),
            Slot::Long(idb),
            Slot::Int(1),
            Slot::Long(BROKER),
            Slot::Int(0),
            Slot::Int(0),
        ],
    );
    h.ask(b, PURCHASE, &[Arg::Object(g.graph())], 10);
    money(&mut h, ida, -1);
    let sale = h.server.market.list[0].sale;
    let from = h.bots[a].inbox.len();
    h.ask(
        a,
        function::COLLECT,
        &[Arg::Int(1), Arg::Int(sale), Arg::LongInt(BROKER)],
        10,
    );
    assert_eq!(last_money(&h, a, from), Some(-1));
    assert!(h.server.market.list.is_empty());
}

/// The sale of one item, with all it should leave behind told: the money it took, what the store
/// answered, and whether it went up.
fn sold(h: &mut Harness, a: usize, ida: u64, item: u64) -> (i32, Option<u32>, bool) {
    let p = h
        .server
        .players
        .values_mut()
        .find(|p| p.character == ida)
        .expect("in the world");
    let template = h.server.things.get(item).expect("made").template;
    if !p.play.items.iter().any(|i| i.id == item) {
        p.play.items.push(yalain_world::state::Held {
            id: item,
            template,
            slot: 0,
            location: 0,
            quantity: 1,
        });
    }
    let from = h.bots[a].inbox.len();
    h.ask(
        a,
        function::SELL,
        &[
            Arg::Int(STORE),
            Arg::Int(100),
            Arg::Int(1),
            Arg::LongInt(item),
            Arg::LongInt(BROKER),
        ],
        10,
    );
    let up = h.server.market.list.iter().any(|c| c.item == item);
    let money = h
        .server
        .players
        .values()
        .find(|p| p.character == ida)
        .map_or(0, |p| {
            yalain_world::quals::int(&p.qualities, 3100).unwrap_or(0)
        });
    (money, last_error(h, a, from), up)
}

/// An item bound by use in combat is refused its sale: nothing taken, the item kept, and marked to
/// bind but not yet bound it goes up.
/// Behaviour: server.market.a-bound-item-is-not-sold
#[test]
fn an_item_bound_by_use_is_refused_its_sale_and_one_still_to_bind_goes_up() {
    let mut h = world();
    let (a, ida) = h.player_in_world("one", "Seller");
    at_the_broker(&mut h, ida);
    money(&mut h, ida, 1_000);
    let bound = yalain_world::inventory::make_item(&mut h.server, GEM, 1, 41).expect("made");
    h.server.things.get_mut(bound).expect("made").own.bound_to = ida;
    let refused = yalain_world::store::error::SALE_REFUSED;
    assert_eq!(
        sold(&mut h, a, ida, bound),
        (1_000, Some(refused), false),
        "nothing taken"
    );
    let marked = yalain_world::inventory::make_item(&mut h.server, GEM, 1, 41).expect("made");
    h.server.things.get_mut(marked).expect("made").binds_on_use = true;
    let (left, error, up) = sold(&mut h, a, ida, marked);
    assert!(left < 1_000, "its charge taken");
    assert_eq!((error, up), (None, true));
}

/// An item under an effect that attunes it is refused its sale, nothing taken, and may go up when the
/// effect has run out.
/// Behaviour: server.market.an-item-under-an-attuning-effect-is-not-sold
#[test]
fn an_item_under_an_attuning_effect_is_refused_its_sale_until_it_ends() {
    const STATE: u32 = 0x7000_7A02;
    let mut h = world();
    let (a, ida) = h.player_in_world("one", "Seller");
    at_the_broker(&mut h, ida);
    money(&mut h, ida, 1_000);
    h.server
        .effect_table
        .effects
        .insert(STATE, yalain_testkit::attuning_effect(STATE));
    let item = yalain_world::inventory::make_item(&mut h.server, GEM, 1, 41).expect("made");
    let until = h.server.game_now + 100.0;
    h.server
        .things
        .get_mut(item)
        .expect("made")
        .timed
        .push(yalain_world::things::TimedEffect {
            state: STATE,
            spellcraft: 1.0,
            until,
        });
    let refused = yalain_world::store::error::SALE_REFUSED;
    assert_eq!(
        sold(&mut h, a, ida, item),
        (1_000, Some(refused), false),
        "nothing taken"
    );
    h.advance(101.0);
    h.run(1);
    let (left, error, up) = sold(&mut h, a, ida, item);
    assert!(left < 1_000, "its charge taken");
    assert_eq!((error, up), (None, true));
}

/// An item attuned to its holder, by its template or by a recipe, is refused its sale: nothing
/// taken, the item kept.
/// Behaviour: server.market.an-attuned-item-is-not-sold
#[test]
fn an_attuned_item_is_refused_its_sale_by_its_template_or_a_recipe() {
    const ATTUNED: u32 = 0x4700_7A41;
    let mut h = world();
    let (a, ida) = h.player_in_world("one", "Seller");
    at_the_broker(&mut h, ida);
    money(&mut h, ida, 1_000);
    let mut props = yalain_content::Props::default();
    props.set("Item_IsAttunable", yalain_content::Prop::Bool(true));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(ATTUNED, 0x200, props));
    let refused = Some(yalain_world::store::error::SALE_REFUSED);
    let by_template =
        yalain_world::inventory::make_item(&mut h.server, ATTUNED, 1, 41).expect("made");
    assert_eq!(sold(&mut h, a, ida, by_template), (1_000, refused, false));
    let by_recipe = yalain_world::inventory::make_item(&mut h.server, GEM, 1, 41).expect("made");
    h.server
        .things
        .get_mut(by_recipe)
        .expect("made")
        .own
        .attuned = true;
    assert_eq!(sold(&mut h, a, ida, by_recipe), (1_000, refused, false));
}

/// A character at the most consignments it may hold is told so and nothing is taken; one collected
/// makes room again.
/// Behaviour: server.market.a-character-holds-so-many-consignments
#[test]
fn a_character_at_its_most_consignments_is_refused_and_told() {
    use yalain_world::market::{AT_THE_MOST, MOST_CONSIGNMENTS};
    use yalain_world::store::{self, error};
    let mut h = world();
    let (a, ida) = h.player_in_world("one", "Seller");
    at_the_broker(&mut h, ida);
    money(&mut h, ida, 100_000);
    let mut stacks = Props::default();
    stacks.set("Item_MaxStackSize", yalain_content::Prop::Int(1000));
    h.server.catalog.insert(test_template(GEM, 0x1067, stacks));
    let gem = yalain_world::inventory::make_item(&mut h.server, GEM, 1000, 0).expect("made");
    let w = world_of(&h, ida);
    let p = h.server.players.get_mut(&w).expect("in the world");
    p.play.items.push(yalain_world::state::Held {
        id: gem,
        template: GEM,
        slot: 0,
        location: 0,
        quantity: 1000,
    });
    let sell = [
        Arg::Int(STORE),
        Arg::Int(1),
        Arg::Int(1),
        Arg::LongInt(gem),
        Arg::LongInt(BROKER),
    ];
    for _ in 0..MOST_CONSIGNMENTS {
        h.ask(a, function::SELL, &sell, 2);
    }
    assert_eq!(h.server.market.list.len(), MOST_CONSIGNMENTS);
    let left = yalain_world::store::money(&h.server.players[&w].qualities);
    let from = h.bots[a].inbox.len();
    h.ask(a, function::SELL, &sell, 5);
    assert_eq!(h.server.market.list.len(), MOST_CONSIGNMENTS, "no more");
    assert_eq!(
        store::money(&h.server.players[&w].qualities),
        left,
        "nothing taken"
    );
    assert_eq!(last_error(&h, a, from), Some(error::SALE_REFUSED));
    assert!(h
        .texts(a, from)
        .iter()
        .any(|(k, id)| (*k, *id) == (1, AT_THE_MOST.0)));
    let sale = h.server.market.list[0].sale;
    let collect = [Arg::Int(0), Arg::Int(sale), Arg::LongInt(BROKER)];
    h.ask(a, function::COLLECT, &collect, 5);
    assert_eq!(h.server.market.list.len(), MOST_CONSIGNMENTS - 1);
    h.ask(a, function::SELL, &sell, 2);
    assert_eq!(h.server.market.list.len(), MOST_CONSIGNMENTS, "room again");
}

/// A store's page comes out of a connection's allowance, by the sales it shows and the listing read
/// for them: an ordinary player's are all answered, one past the allowance is not, and time brings
/// them back.
/// Behaviour: server.market.pages-drawn-from-an-allowance
#[test]
fn market_pages_come_out_of_an_allowance_of_rows() {
    use yalain_world::market::{LISTED_A_ROW, PAGE_ROWS_A_SECOND, PAGE_ROWS_HELD};
    let mut h = world();
    let (a, ida) = h.player_in_world("one", "Shopper");
    at_the_broker(&mut h, ida);
    h.server.market.loaded = true;
    h.server.market.list = (1..=10).map(sale_row).collect();
    let page = [Arg::Int(STORE), Arg::LongInt(BROKER)];
    let pages = |h: &Harness, from: usize| {
        h.functions(a, from)
            .iter()
            .filter(|f| **f == ENTER_STORE)
            .count()
    };
    let from = h.bots[a].inbox.len();
    for _ in 0..50 {
        h.ask(a, function::ENTER_STORE, &page, 1);
    }
    h.run(5);
    assert_eq!(pages(&h, from), 50, "a player's pages are all answered");
    let full = PAGE_ROWS_HELD / PAGE_ROWS_A_SECOND + 1.0;
    h.step_by(full);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let huge = (PAGE_ROWS_HELD as u32) * u32::try_from(LISTED_A_ROW).expect("few");
    h.server.market.list = (1..=huge).map(sale_row).collect();
    // Two at once: the first takes all there is, and the second is not answered however long it waits.
    let from = h.bots[a].inbox.len();
    let ask = yalain_testkit::play::server_call(function::ENTER_STORE, &page);
    h.bots[a].send(true, auberean_protocol::queue::WEENIE, &ask);
    h.bots[a].send(true, auberean_protocol::queue::WEENIE, &ask);
    assert!(
        h.run_until(5_000, |h| pages(h, from) >= 1),
        "a page of a long listing comes"
    );
    assert!(
        !h.run_until(5_000, |h| pages(h, from) >= 2),
        "the next is not answered"
    );
    h.step_by(full);
    let from = h.bots[a].inbox.len();
    h.ask(a, function::ENTER_STORE, &page, 1);
    assert!(h.run_until(5_000, |h| pages(h, from) >= 1), "back in time");
}

/// A broker's store shows a page's sales at a time however many it lists, says how many it lists, and
/// its pages turn through them all, to its end and back; no page holds a connection's other messages.
/// Behaviour: server.market.a-brokers-store-goes-by-pages
#[test]
fn a_brokers_long_store_goes_by_pages_that_turn_through_it_all() {
    use yalain_world::store::function::{NEXT_SALES, PREV_SALES, UPDATE_STORE};
    let mut h = world();
    let (a, ida) = h.player_in_world("one", "Shopper");
    at_the_broker(&mut h, ida);
    h.server.market.loaded = true;
    // Long enough that a page of them all would go by the pace; sold-out sales aren't listed.
    let mut list: Vec<_> = (1..=2001).map(sale_row).collect();
    list[0].sold = 1;
    h.server.market.list = list;
    let page = [Arg::Int(STORE), Arg::LongInt(BROKER)];
    // Each view's sales' ids, its size and its place.
    let views = |h: &Harness, from: usize, f: u32| -> Vec<(Vec<u64>, u32, u32, usize)> {
        h.world_messages(a, from)
            .iter()
            .filter_map(|m| {
                let ServerMessage::PlayerCall(c) = m else {
                    return None;
                };
                if c.function.0 != f {
                    return None;
                }
                let len = m.encode().len();
                let args = c.args.as_ref()?;
                let v = yalain_world::calls::values(&args.0).ok()?;
                let graph = v.iter().find_map(|x| match x {
                    yalain_world::calls::Value::Graph(g) => Some(g),
                    _ => None,
                })?;
                let (mut size, mut pos, mut sales) = (0, 0, Vec::new());
                for (_, n) in graph {
                    if let yalain_world::calls::Node::Script { package, slots } = n {
                        if *package == yalain_world::store::package::STORE_VIEW {
                            size = slots[2].1;
                            pos = slots[3].1;
                        } else {
                            sales.push(u64::from(slots[3].1) << 32 | u64::from(slots[2].1));
                        }
                    }
                }
                Some((sales, size, pos, len))
            })
            .collect()
    };
    let from = h.bots[a].inbox.len();
    h.ask(a, function::ENTER_STORE, &page, 3);
    let first = views(&h, from, ENTER_STORE);
    assert_eq!(first.len(), 1);
    let (sales, size, pos, len) = &first[0];
    assert_eq!((*size, *pos), (2000, 0), "the whole listing's size");
    assert_eq!(sales.len(), yalain_world::store::PAGE as usize);
    assert_eq!(sales[0], sale_row(2).item, "in the listing's order");
    assert!(*len < 32 * 1024, "a page is short: {len}");
    let mut seen: Vec<u64> = sales.clone();
    let mut last_pos = 0;
    for _ in 0..300 {
        let from = h.bots[a].inbox.len();
        h.ask(a, NEXT_SALES, &[], 3);
        let got = views(&h, from, UPDATE_STORE);
        assert_eq!(got.len(), 1, "a turn is answered");
        let (sales, size, pos, _) = &got[0];
        assert_eq!(*size, 2000);
        if *pos == last_pos {
            break;
        }
        last_pos = *pos;
        seen.extend(sales);
        // As slowly as the allowance asks of so long a listing.
        h.step_by(0.25);
    }
    assert_eq!(seen.len(), 2000, "every sale listed, once");
    let want: Vec<u64> = (2..=2001).map(|i| sale_row(i).item).collect();
    assert_eq!(seen, want);
    let from = h.bots[a].inbox.len();
    h.ask(a, PREV_SALES, &[], 3);
    let got = views(&h, from, UPDATE_STORE);
    assert_eq!(got[0].2, last_pos - yalain_world::store::PAGE);
}

/// A row of the broker's store.
fn sale_row(sale: u32) -> yalain_world::market::Consignment {
    yalain_world::market::Consignment {
        sale,
        owner: 1,
        keeper: BROKER,
        catalog: CATALOG,
        store: STORE,
        item: 0x8131_0000_0000_0000 + u64::from(sale),
        template: GEM,
        price: 1.0,
        offered: 1,
        sold: 0,
        entered: 0.0,
    }
}

/// An item is not put up for sale while a use under way counts on the pack.
/// Behaviour: server.market.waits-for-a-use-under-way
#[test]
fn nothing_is_put_up_for_sale_while_a_trip_counts_on_the_pack() {
    let mut h = world();
    let (a, ida) = h.player_in_world("one", "Elara");
    h.run(10);
    at_the_broker(&mut h, ida);
    money(&mut h, ida, 1000);
    let world = *h.server.players.keys().next().expect("in");
    yalain_world::inventory::give(&mut h.server, &mut h.net, world, GEM, 1);
    h.run(3);
    let gem = h.server.players[&world]
        .play
        .items
        .iter()
        .find(|i| i.template == GEM)
        .expect("held")
        .id;
    let to = h.server.players[&world].position;
    h.server.players.get_mut(&world).expect("in").trip = Some(yalain_world::places::Trip {
        to,
        used: None,
        step: None,
        resurrect: false,
        sent: false,
        said: None,
    });
    h.ask(
        a,
        function::SELL,
        &[
            Arg::Int(STORE),
            Arg::Int(100),
            Arg::Int(1),
            Arg::LongInt(gem),
            Arg::LongInt(BROKER),
        ],
        10,
    );
    assert!(h.server.market.list.is_empty(), "not put up");
    assert!(
        h.server.players[&world]
            .play
            .items
            .iter()
            .any(|i| i.id == gem),
        "still held"
    );
}
