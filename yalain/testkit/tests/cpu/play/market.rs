//! The market: a broker's catalog, an item put up for sale, the sale shown in its store, bought by
//! another player, and its price collected.

use auberean_protocol::ServerMessage;
use auberean_protocol::ServerMessage as M;
use auberean_script::args::Arg;
use yalain_content::{ContentObject, ContentPack, Description, Facts, Kind, Props};
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
