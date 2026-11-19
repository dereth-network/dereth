//! The player's consignments and a new sale: collecting one, restocking one with the same kind of
//! thing after a question, an item of another kind becoming a new sale, what putting one up
//! costs, and the sale reminder.
//!
//! Behaviour: none (this client's own interface, not a behaviour of the retail client)

use auberean_client_sdk::contract::notice::{Sale, Store};
use auberean_client_sdk::contract::StoreAction;
use auberean_client_sdk::Action;
use auberean_horizon::keys::Keyed;
use auberean_horizon::state::goods::{sale_state, SaleShown, ShopShown};
use auberean_horizon::state::GameState;
use auberean_horizon::ui::market::{cost_to_put_up, reminder_rect, Market};

use crate::things::{carry, holding, item, mid, pack, without_selecting};
use crate::Harness;

const SWORD: u64 = 0x7000_0000_0000_0001;
const CAP: u64 = 0x7000_0000_0000_0002;
const SWORDS: u32 = 0x4700_0100;

fn consignment(ordinal: u32, item_data: u32, state: u32) -> SaleShown {
    SaleShown {
        sale: Sale {
            ordinal,
            item_data,
            state,
            cost_quantity: 40.0,
            ..Sale::default()
        },
        description: format!("Consignment {ordinal}"),
        cost: "coins".to_owned(),
    }
}

fn selling() -> GameState {
    let mut sword = item("Sword");
    sword.data_id = SWORDS;
    let mut s = holding(pack(
        [&[SWORD, CAP], &[], &[], &[]],
        &[(SWORD, sword), (CAP, item("Cap"))],
        &[],
        500,
    ));
    s.goods.consignments = Some(ShopShown {
        store: Store::default(),
        name: "My consignments".to_owned(),
        description: String::new(),
        sales: vec![
            consignment(3, SWORDS, 0),
            consignment(5, 0x4700_0200, sale_state::SOLD),
        ],
    });
    s.goods.consignment_charge = 0.25;
    s
}

#[test]
fn collect_collects_the_selected_consignment_its_coins_once_sold() {
    let mut h = Harness::new();
    let s = selling();
    h.frame(&s);
    let (_, body) = h.ui.goods.market.drawn().expect("shown");
    let k = h.k();
    h.click(&s, mid(Market::row(body, 1, k)));
    let collect = (body.x + 60.0 * k, body.bottom() - 25.0 * k);
    assert_eq!(
        h.click(&s, collect).acts,
        [Action::Store(StoreAction::Collect {
            sale: 5,
            sold: true
        })]
    );
}

#[test]
fn an_item_of_a_consignments_kind_restocks_it_after_saying_what_it_costs() {
    let mut h = Harness::new();
    let s = selling();
    h.ui.keyed(Keyed::Inventory, &s);
    h.frame(&s);
    let k = h.k();
    let (_, body) = h.ui.goods.market.drawn().expect("shown");
    let inv = h.ui.goods.inventory.layout(k).expect("open");
    let acts = without_selecting(carry(
        &mut h,
        &s,
        mid(inv.cell(0, k)),
        mid(Market::row(body, 0, k)),
    ));
    assert!(acts.is_empty());
    let q = h.ui.goods.question.as_ref().expect("asked");
    // 40 coins each, one of them, at the market's quarter: ten coins.
    assert_eq!(q.text, "Restock with Sword? It costs 10 coins.");
    assert_eq!(
        h.key(&s, auberean_horizon::ui::vk::ENTER).acts,
        [Action::Store(StoreAction::Restock {
            item: SWORD,
            quantity: 1,
            sale: 3
        })]
    );
}

#[test]
fn an_item_of_another_kind_becomes_a_new_sale_put_up_after_a_question() {
    let mut h = Harness::new();
    let s = selling();
    h.ui.keyed(Keyed::Inventory, &s);
    h.frame(&s);
    let k = h.k();
    let (_, body) = h.ui.goods.market.drawn().expect("shown");
    let inv = h.ui.goods.inventory.layout(k).expect("open");
    let acts = without_selecting(carry(
        &mut h,
        &s,
        mid(inv.cell(1, k)),
        mid(Market::row(body, 0, k)),
    ));
    assert!(acts.is_empty());
    assert!(h.ui.goods.question.is_none(), "not a restock");
    assert_eq!(h.ui.goods.market.new_sale.item, Some((CAP, 1)));
    h.ui.goods.market.new_sale.price = "100".to_owned();
    h.frame(&s);
    let r = h.ui.goods.market.new_sale.body().expect("open");
    let put = (r.x + 60.0 * k, r.bottom() - 25.0 * k);
    h.click(&s, put);
    let q = h.ui.goods.question.as_ref().expect("asked");
    assert_eq!(q.text, "Sell Cap for 100 coins each? It costs 25 coins.");
    assert_eq!(
        h.key(&s, auberean_horizon::ui::vk::ENTER).acts,
        [Action::Store(StoreAction::Sell {
            item: CAP,
            quantity: 1,
            price: 100
        })]
    );
    assert!(
        !h.ui.goods.market.new_sale.window.open,
        "closed once put up"
    );
}

#[test]
fn putting_a_sale_up_costs_its_share_rounded_up_and_at_least_a_coin() {
    assert_eq!(cost_to_put_up(100, 3, 0.25), 75);
    // The charge is reckoned as the game holds it, in single precision: 5% is a little over.
    assert_eq!(cost_to_put_up(100, 3, 0.05), 16);
    assert_eq!(cost_to_put_up(1, 1, 0.05), 1);
    assert_eq!(cost_to_put_up(7, 1, 0.5), 4);
}

#[test]
fn the_sale_reminder_shows_while_a_sale_waits_and_shows_them_when_pressed() {
    let mut h = Harness::new();
    let mut s = holding(pack([&[], &[], &[], &[]], &[], &[], 0));
    let at = mid(reminder_rect(h.screen, h.k()));
    assert!(h.click(&s, at).acts.is_empty());
    s.goods.sale_waiting = true;
    assert_eq!(h.click(&s, at).acts, [Action::SaleReminders]);
}
