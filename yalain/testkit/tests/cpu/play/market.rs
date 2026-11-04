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
