//! The shop window and the market's catalog: buying one, or as many as the player says; a sale
//! that can't be had, or can't be afforded; a sale carried into the pack; the pages; the game's
//! purchase question; a catalog's shop entered.
//!
//! Behaviour: none (this client's own interface, not a behaviour of the retail client)

use auberean_client_sdk::contract::notice::{Sale, Store};
use auberean_client_sdk::contract::StoreAction;
use auberean_client_sdk::Action;
use auberean_horizon::keys::Keyed;
use auberean_horizon::state::goods::{sale_state, Catalog, Purchase, SaleShown, ShopShown};
use auberean_horizon::state::GameState;
use auberean_horizon::ui::shop::{stock_line, ShopLayout};

use crate::things::{carry, holding, item, mid, pack, without_selecting};
use crate::Harness;

const SWORD: u64 = 0x50;
const AXE: u64 = 0x51;
const CAP: u64 = 0x7000_0000_0000_0002;

fn sale(item: u64, stock: i32, state: u32) -> SaleShown {
    SaleShown {
        sale: Sale {
            item,
            stock,
            state,
            cost_quantity: 30.0,
            ..Sale::default()
        },
        description: format!("Sale {item:#x}"),
        cost: "coins".to_owned(),
    }
}

fn shopping(sales: Vec<SaleShown>, most: &[(u64, u32)]) -> GameState {
    let mut s = holding(pack(
        [&[CAP], &[], &[], &[]],
        &[(CAP, item("Cap"))],
        &[],
        100,
    ));
    s.goods.shop = Some(ShopShown {
        store: Store {
            id: 4,
            position: 0,
            size: 12,
            ..Store::default()
        },
        name: "Smithy".to_owned(),
        description: String::new(),
        sales,
    });
    s.goods.max_purchase = most.iter().copied().collect();
    s
}

/// The shop drawn once; where its parts are.
fn shown(h: &mut Harness, s: &GameState) -> ShopLayout {
    h.frame(s);
    let (_, body) = h.ui.goods.shops.shop_drawn().expect("shown");
    ShopLayout::of(body, h.k())
}

#[test]
fn a_sale_pressed_twice_buys_one_when_one_is_all_the_player_may_buy() {
    let mut h = Harness::new();
    let s = shopping(vec![sale(SWORD, 3, 0)], &[(SWORD, 1)]);
    let l = shown(&mut h, &s);
    let (_, body) = h.ui.goods.shops.shop_drawn().unwrap();
    let row = l.row(0, body.w, h.k());
    h.click(&s, mid(row));
    let out = h.click(&s, mid(row));
    assert_eq!(
        without_selecting(out.acts),
        [Action::Store(StoreAction::Purchase {
            item: SWORD,
            quantity: 1,
            slot: 0
        })]
    );
}

#[test]
fn a_sale_with_no_end_to_its_stock_reads_unlimited_and_is_bought() {
    let mut h = Harness::new();
    let s = shopping(vec![sale(SWORD, -1, 0)], &[(SWORD, 1)]);
    assert_eq!(stock_line(&sale(SWORD, -1, 0)), "Unlimited");
    let l = shown(&mut h, &s);
    let (_, body) = h.ui.goods.shops.shop_drawn().unwrap();
    h.click(&s, mid(l.row(0, body.w, h.k())));
    let out = h.click(&s, mid(l.buy));
    assert_eq!(
        without_selecting(out.acts),
        [Action::Store(StoreAction::Purchase {
            item: SWORD,
            quantity: 1,
            slot: 0
        })]
    );
}

#[test]
fn buy_asks_how_many_when_the_player_may_buy_more_than_one() {
    let mut h = Harness::new();
    let s = shopping(vec![sale(SWORD, 3, 0)], &[(SWORD, 3)]);
    let l = shown(&mut h, &s);
    let (_, body) = h.ui.goods.shops.shop_drawn().unwrap();
    h.click(&s, mid(l.row(0, body.w, h.k())));
    let out = h.click(&s, mid(l.buy));
    assert!(without_selecting(out.acts).is_empty());
    let a = h.ui.goods.amount.as_ref().expect("asked");
    assert_eq!((a.value(), a.most), (1, 3));
}

#[test]
fn a_sale_none_left_of_cannot_be_bought_and_one_unaffordable_says_so() {
    let mut h = Harness::new();
    let s = shopping(
        vec![sale(SWORD, 0, 0), sale(AXE, 2, 0)],
        &[(SWORD, 5), (AXE, 0)],
    );
    let l = shown(&mut h, &s);
    let (_, body) = h.ui.goods.shops.shop_drawn().unwrap();
    let k = h.k();
    for row in [0, 1] {
        h.click(&s, mid(l.row(row, body.w, k)));
        let out = h.click(&s, mid(l.buy));
        assert!(without_selecting(out.acts).is_empty());
    }
    assert_eq!(
        h.ui.goods.asks.told,
        ["That can't be bought.", "You can't afford that."]
    );
    assert_eq!(stock_line(&sale(SWORD, 0, 0)), "Sold Out");
    assert_eq!(
        stock_line(&sale(SWORD, 4, sale_state::RESTRICTED)),
        "Restricted"
    );
}

#[test]
fn a_sale_carried_into_the_pack_is_bought_into_that_slot() {
    let mut h = Harness::new();
    let s = shopping(vec![sale(SWORD, 3, 0)], &[(SWORD, 1)]);
    h.ui.keyed(Keyed::Inventory, &s);
    let l = shown(&mut h, &s);
    let (_, body) = h.ui.goods.shops.shop_drawn().unwrap();
    let k = h.k();
    let inv = h.ui.goods.inventory.layout(k).expect("open");
    let row = l.row(0, body.w, k);
    let tile = (row.x + 20.0 * k, row.y + 20.0 * k);
    let acts = without_selecting(carry(&mut h, &s, tile, mid(inv.cell(0, k))));
    assert_eq!(
        acts,
        [Action::Store(StoreAction::Purchase {
            item: SWORD,
            quantity: 1,
            slot: 0
        })]
    );
}

#[test]
fn the_page_arrows_step_through_the_shops_pages() {
    let mut h = Harness::new();
    let s = shopping(vec![sale(SWORD, 3, 0)], &[]);
    let l = shown(&mut h, &s);
    assert!(
        h.click(&s, mid(l.previous)).acts.is_empty(),
        "the first page"
    );
    assert_eq!(
        h.click(&s, mid(l.next)).acts,
        [Action::Store(StoreAction::NextSales)]
    );
}

#[test]
fn the_games_purchase_question_is_asked_and_its_answer_sent() {
    let mut h = Harness::new();
    let mut s = shopping(vec![sale(SWORD, 3, 0)], &[]);
    s.goods.purchase = Some(Purchase {
        product: "Iron sword".to_owned(),
        quantity: 2,
        cost: "coins".to_owned(),
        cost_quantity: 60,
    });
    h.frame(&s);
    let q = h.ui.goods.question.as_ref().expect("asked");
    assert_eq!(q.text, "Buy 2 Iron sword for 60 coins?");
    let out = h.key(&s, auberean_horizon::ui::vk::ENTER);
    assert_eq!(out.acts, [Action::Store(StoreAction::Confirm(true))]);
    assert!(h.ui.goods.asks.purchase_answered);
}

#[test]
fn a_catalogs_shop_pressed_twice_is_entered() {
    let mut h = Harness::new();
    let mut s = holding(pack([&[], &[], &[], &[]], &[], &[], 0));
    s.goods.catalog = Some(Catalog {
        header: "Market".to_owned(),
        shops: vec![ShopShown {
            store: Store {
                id: 9,
                ..Store::default()
            },
            name: "Tailor".to_owned(),
            ..ShopShown::default()
        }],
    });
    h.frame(&s);
    let (_, body) = h.ui.goods.shops.catalog_drawn().expect("shown");
    let k = h.k();
    let row = (body.x + 100.0 * k, body.y + 60.0 * k);
    assert!(h.click(&s, row).acts.is_empty());
    assert_eq!(
        h.click(&s, row).acts,
        [Action::Store(StoreAction::EnterShop(9))]
    );
}
