//! The shop windows on the retail layouts: a shop's sales bought, a catalog's shops entered, the
//! player's consignments collected and a new one put up.

use auberean_text::StringInfo;
use auberean_ui::types::button_id;
use auberean_ui::Token;
use auberean_ui_game::gameplay::{Record, Request};
use auberean_ui_game::shop::catalog::Catalog;
use auberean_ui_game::shop::consignment::Consignments;
use auberean_ui_game::shop::{notice, sale_state, Sale, Shop, ShopRequest, Store};

use super::stage::{answer, dialog, said, World, ICON, ORE, ORE_RECORD};

const AXE: u64 = 0x5000_0000_0000_0201;
const BREAD: u64 = 0x5000_0000_0000_0202;
const GEM: u64 = 0x5000_0000_0000_0203;

fn sale(item: u64, ordinal: u32, what: &str, stock: i32, state: u32) -> Sale {
    Sale {
        item,
        item_did: ORE_RECORD,
        icon: ICON,
        description: StringInfo::literal(what),
        cost_description: StringInfo::literal("Coins"),
        cost_icon: ICON,
        cost_quantity: 12.0,
        stock,
        ordinal,
        state,
        ..Sale::default()
    }
}

fn store() -> Store {
    Store {
        id: 0x7100_0001,
        name: StringInfo::literal("Smithy"),
        description: StringInfo::literal("Iron and steel"),
        sales: vec![
            sale(AXE, 1, "Axe", -1, 0),
            sale(BREAD, 2, "Bread", 0, 0),
            sale(GEM, 3, "Gem", 4, sale_state::RESTRICTED),
        ],
        position: 0,
        size: 3,
        ..Store::default()
    }
}

fn shops(out: &[Request]) -> Vec<ShopRequest> {
    out.iter()
        .filter_map(|r| match r {
            Request::Shop(s) => Some(s.clone()),
            _ => None,
        })
        .collect()
}

fn rows(w: &World) -> Vec<auberean_ui::Handle> {
    let shop = w.el("Shop");
    w.ui.list_items(w.under(shop, "ItemList"))
}

/// A shop the scripts show: its name and description, a row per sale with what is left (no end
/// of them, sold out, restricted); the page buttons unavailable on the only page. A sale pressed
/// is selected and its item selected for the player; purchase buys one when only one may be
/// bought, asks how many when more may (and buys that many), and tells the player when none may.
/// The scripts' purchase question is put up and its answer given back. The page buttons ask for
/// the next page and wait; close leaves the shop and hides the window.
///
/// Behaviour: ui.shop.a-shop-shows-its-sales-and-buys-them
#[test]
fn a_shop_shows_its_sales_and_buys_them() {
    let mut w = World::new();
    w.tick(&[said(
        notice::VIEW_STORE,
        vec![0],
        vec![Some(Record::Store(store()))],
    )]);
    let field = w.el("Shop");
    assert!(w.shown(field), "shown");
    assert_eq!(w.text(w.under(field, "ShopName")), "Smithy");
    assert_eq!(w.text(w.under(field, "ShopDesc")), "Iron and steel");
    let r = rows(&w);
    assert_eq!(r.len(), 3);
    let stock: Vec<String> = r
        .iter()
        .map(|row| w.text(w.under(*row, "StockQuantity")))
        .collect();
    assert!(stock[1] != stock[0] && stock[2] != stock[0], "{stock:?}");
    assert_eq!(
        w.state(w.under(r[1], "StockQuantity")),
        Token::of("unavailable")
    );
    assert!(w.ghosted(w.under(field, "ShopNextButton")));
    assert!(w.ghosted(w.under(field, "ShopPrevButton")));
    let purchase = w.under(field, "PurchaseButton");
    assert!(w.ghosted(purchase), "nothing selected");

    // The axe selected and bought: one may be bought.
    let out = w.press(r[0], button_id::LEFT);
    assert!(out.contains(&Request::SetSelection(AXE)), "{out:?}");
    assert!(!w.ghosted(purchase));
    w.shop.version = 1;
    w.shop.max_purchase.insert(AXE, 1);
    let out = w.click(purchase);
    assert_eq!(
        shops(&out),
        vec![ShopRequest::Purchase {
            item: AXE,
            quantity: 1,
            slot: 0
        }]
    );
    // Five may be bought: how many asked, then that many bought.
    w.shop.version = 2;
    w.shop.max_purchase.insert(AXE, 5);
    w.click(purchase);
    let (_, out) = w.split(3);
    assert_eq!(
        shops(&out),
        vec![ShopRequest::Purchase {
            item: AXE,
            quantity: 3,
            slot: 0
        }]
    );
    // None may be bought: the player told so.
    w.shop.version = 3;
    w.shop.max_purchase.insert(AXE, 0);
    let out = w.click(purchase);
    assert!(shops(&out).is_empty());
    assert!(dialog(&out).is_some(), "the player told: {out:?}");
    assert!(
        out.iter()
            .any(|r| matches!(r, Request::Dialog(d) if d.modal)),
        "at once, in place of any dialog up: {out:?}"
    );

    // The scripts' question, answered yes.
    let product = Some(Record::Text(StringInfo::literal("Axe")));
    let coins = Some(Record::Text(StringInfo::literal("Coins")));
    let out = w.tick(&[said(
        notice::CONFIRM_PURCHASE,
        vec![0, 1, 0, 12],
        vec![product, None, coins, None],
    )]);
    let (q, text) = dialog(&out).expect("the question");
    assert!(text.contains("Axe"), "{text}");
    let out = w.tick(&[answer(q, true)]);
    assert_eq!(shops(&out), vec![ShopRequest::Confirm(true)]);

    // Close: the shop left, the window hidden.
    let out = w.click(w.under(field, "ShopCloseButton"));
    assert_eq!(shops(&out), vec![ShopRequest::LeaveStore]);
    assert!(!w.shown(field));
    let shop = w.play.child::<Shop>().expect("the shop window");
    assert_eq!(shop.dialog(), 0);
}

/// A catalog the scripts show: its header, a row per shop with its name and how many things it
/// sells; a shop pressed twice is entered; the consignments button asks for the player's
/// consignments; close leaves the catalog.
///
/// Behaviour: ui.shop.a-catalog-lists-its-shops-and-enters-one
#[test]
fn a_catalog_lists_its_shops_and_enters_one() {
    let mut w = World::new();
    let mut other = store();
    other.id = 0x7100_0002;
    other.name = StringInfo::literal("Bakery");
    other.size = 12;
    let header = Some(Record::Text(StringInfo::literal("Market")));
    let list = Some(Record::Stores(vec![store(), other]));
    w.tick(&[said(notice::VIEW_CATALOG, vec![0, 0], vec![header, list])]);
    let field = w.el("Catalog");
    assert!(w.shown(field));
    assert_eq!(w.text(w.under(field, "StoreHeader")), "Market");
    let catalog = w.play.child::<Catalog>().expect("the catalog");
    assert_eq!(catalog.stores(&w.ui).0, vec![0x7100_0001, 0x7100_0002]);
    let enter = w.under(field, "EnterStoreButton");
    assert!(w.ghosted(enter), "no shop selected");
    let rows = w.ui.list_items(w.under(field, "StoreList"));
    assert_eq!(w.text(w.under(rows[1], "StoreName")), "Bakery");
    assert_eq!(w.text(w.under(rows[1], "StoreSize")), "12");
    let pick = w.under(rows[1], "SelectionField");
    w.press(pick, button_id::LEFT);
    assert!(!w.ghosted(enter));
    let out = w.press(pick, button_id::DOUBLE_LEFT);
    assert!(
        shops(&out).contains(&ShopRequest::EnterShop(0x7100_0002)),
        "{out:?}"
    );
    let out = w.click(w.under(field, "ConsignmentButton"));
    assert_eq!(shops(&out), vec![ShopRequest::EnterConsignment]);
    let out = w.click(w.under(field, "CatalogCloseButton"));
    assert_eq!(shops(&out), vec![ShopRequest::LeaveCatalog]);
    assert!(!w.shown(field), "left");
}

/// The player's consignments the scripts show: a row per consignment (active or sold) and an
/// empty row; one pressed and collected asks for its coins when sold. New opens the new-sale
/// window; ore dropped on it asks how many, and with a price typed, put up asks what it costs
/// and yes puts that many up for that price. Ore dropped on an active consignment of ore asks how
/// many, then whether to restock it at its cost, and yes restocks it.
///
/// Behaviour: ui.shop.consignments-are-collected-put-up-and-restocked
#[test]
fn consignments_are_collected_put_up_and_restocked() {
    let mut w = World::new();
    w.shop.version = 1;
    w.shop.consignment_charge = 0.05;
    let mut mine = store();
    mine.name = StringInfo::literal("My stall");
    mine.sales = vec![
        sale(ORE, 1, "Ore", 5, 0),
        sale(AXE, 2, "Axe", 0, sale_state::SOLD),
    ];
    w.tick(&[said(
        notice::VIEW_CONSIGNMENT,
        vec![0],
        vec![Some(Record::Store(mine))],
    )]);
    let field = w.el("Consignment");
    assert!(w.shown(field));
    assert_eq!(w.text(w.under(field, "ConsignmentName")), "My stall");
    let list = w.under(field, "ItemList");
    let rows = w.ui.list_items(list);
    assert_eq!(rows.len(), 3, "two and an empty row");
    assert_eq!(
        w.state(w.under(rows[1], "StockQuantity")),
        Token::of("sold")
    );
    let c = w.play.child::<Consignments>().expect("the consignments");
    assert_eq!(c.sales(&w.ui).0, vec![1, 2]);

    // The sold axe collected.
    w.press(rows[1], button_id::LEFT);
    let out = w.click(w.under(field, "CollectConsignmentButton"));
    assert_eq!(
        shops(&out),
        vec![ShopRequest::Collect {
            sale: 2,
            sold: true
        }]
    );

    // Ore dropped on the ore's consignment: how many, the question, restocked.
    let ore = w.pack_icon(ORE);
    w.drag(ore, rows[0]);
    let (_, out) = w.split(10);
    assert!(dialog(&out).is_none(), "it waits for the name: {out:?}");
    let out = answer_name(&mut w, 10, "10 Ore");
    let (q, text) = dialog(&out).expect("the restock question");
    assert!(text.contains("10 Ore"), "{text}");
    assert!(
        text.contains('7'),
        "ten at twelve with a five-hundredths charge just over its decimal value: {text}"
    );
    let out = w.tick(&[answer(q, true)]);
    assert_eq!(
        shops(&out),
        vec![ShopRequest::Restock {
            item: ORE,
            quantity: 10,
            sale: 1
        }]
    );

    // A new consignment: the window, the ore, a price, put up.
    w.click(w.under(field, "NewConsignmentButton"));
    let window = w.el("NewSaleField");
    assert!(w.shown(window), "the new-sale window");
    let ore = w.pack_icon(ORE);
    w.drag(ore, w.under(window, "NewSale"));
    w.split(12);
    assert_eq!(w.text(w.under(window, "NewSaleDesc")), "Ore");
    let price = w.under(window, "NewSalePrice");
    w.ui.set_text(price, "7");
    let out = w.click(w.under(window, "AddSaleButton"));
    let (q, _) = dialog(&out).expect("the sale question");
    let out = w.tick(&[answer(q, true)]);
    assert_eq!(
        shops(&out),
        vec![ShopRequest::Sell {
            item: ORE,
            quantity: 12,
            price: 7
        }]
    );
    assert!(!w.shown(window), "put up, hidden");
}

/// Part of a stack is called by its name for that many, which the scripts answer: the new-sale
/// window shows it once it comes and its question says it; a restock's question waits for it.
///
/// Behaviour: ui.shop.part-of-a-stack-is-named-for-its-amount
#[test]
fn part_of_a_stack_is_named_for_its_amount() {
    let name_asked = |out: &[Request], quantity| {
        out.contains(&Request::Inventory(
            auberean_ui_game::inventory::Request::NameAmount {
                item: ORE,
                quantity,
            },
        ))
    };
    let mut w = World::new();
    w.shop.version = 1;
    w.shop.consignment_charge = 0.05;
    let mut mine = store();
    mine.name = StringInfo::literal("My stall");
    mine.sales = vec![sale(ORE, 1, "Ore", 5, 0)];
    w.tick(&[said(
        notice::VIEW_CONSIGNMENT,
        vec![0],
        vec![Some(Record::Store(mine))],
    )]);
    let field = w.el("Consignment");

    // A new sale of three of them.
    w.click(w.under(field, "NewConsignmentButton"));
    let window = w.el("NewSaleField");
    let ore = w.pack_icon(ORE);
    w.drag(ore, w.under(window, "NewSale"));
    let (_, out) = w.split(3);
    assert!(name_asked(&out, 3), "{out:?}");
    assert_eq!(w.text(w.under(window, "NewSaleDesc")), "", "not known yet");
    answer_name(&mut w, 3, "3 Ore");
    assert_eq!(w.text(w.under(window, "NewSaleDesc")), "3 Ore");
    let price = w.under(window, "NewSalePrice");
    w.ui.set_text(price, "7");
    let out = w.click(w.under(window, "AddSaleButton"));
    let (q, text) = dialog(&out).expect("the sale question");
    assert!(text.contains("3 Ore"), "{text}");
    w.tick(&[answer(q, false)]);
    w.click(w.under(window, "NewSaleCloseButton"));

    // All of them need no asking.
    w.click(w.under(field, "NewConsignmentButton"));
    let ore = w.pack_icon(ORE);
    w.drag(ore, w.under(window, "NewSale"));
    let (_, out) = w.split(12);
    assert!(!name_asked(&out, 12), "{out:?}");
    assert_eq!(w.text(w.under(window, "NewSaleDesc")), "Ore");
}

/// The scripts answer what the ore is called for a number of it.
fn answer_name(w: &mut World, quantity: u32, name: &str) -> Vec<Request> {
    w.inventory.amount_name = Some((ORE, quantity, name.to_owned()));
    w.inventory.version += 1;
    w.tick(&[])
}
