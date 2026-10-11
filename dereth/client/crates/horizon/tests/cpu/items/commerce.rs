//! The shop and trade windows follow the game: each opens with the game's vendor or trade, and
//! closing the window closes it in the game. A right-click on a row of the shopping list or of
//! the items to sell takes it back off.
//!
//! Behaviour: none (this client's own interface, not a behaviour of the retail client)

use dereth_client_contract::view::{ShopRow, ShopView, TradeView};
use dereth_client_contract::UiRequest;
use dereth_horizon::draw::Rect;
use dereth_horizon::ui::game::GameState;
use dereth_horizon::ui::panels::WindowId;
use dereth_primitives::ObjectId;

use crate::Harness;

const BREAD: ObjectId = ObjectId(0x8000_0101);
const ARROWS: ObjectId = ObjectId(0x8000_0102);
const SWORD: ObjectId = ObjectId(0x8000_0103);

fn in_world() -> GameState {
    GameState {
        in_world: true,
        name: "Tester".into(),
        ..GameState::default()
    }
}

#[test]
fn a_vendor_opens_the_shop_and_closing_the_shop_closes_the_vendor() {
    let mut h = Harness::new(Default::default());
    let mut state = in_world();
    state.shop = Some(ShopView {
        open: true,
        ..ShopView::default()
    });
    h.frame(&state);
    assert!(h.ui.windows.is_open(WindowId::Vendor));
    h.ui.windows.close(WindowId::Vendor);
    let out = h.frame(&state);
    assert!(
        out.requests.contains(&UiRequest::VendorClose),
        "{:?}",
        out.requests
    );
}

#[test]
fn the_vendor_closing_closes_the_shop() {
    let mut h = Harness::new(Default::default());
    let mut state = in_world();
    state.shop = Some(ShopView {
        open: true,
        ..ShopView::default()
    });
    h.frame(&state);
    state.shop = None;
    let out = h.frame(&state);
    assert!(!h.ui.windows.is_open(WindowId::Vendor));
    assert!(!out.requests.contains(&UiRequest::VendorClose));
}

#[test]
fn a_trade_opens_the_trade_window_and_closing_it_closes_the_trade() {
    let mut h = Harness::new(Default::default());
    let mut state = in_world();
    state.trade = Some(TradeView {
        open: true,
        ..TradeView::default()
    });
    h.frame(&state);
    assert!(h.ui.windows.is_open(WindowId::Trade));
    h.ui.windows.close(WindowId::Trade);
    let out = h.frame(&state);
    assert!(
        out.requests.contains(&UiRequest::TradeClose),
        "{:?}",
        out.requests
    );
}

fn row(item: ObjectId, name: &str) -> ShopRow {
    ShopRow {
        item,
        name: name.into(),
        amount: 1,
        obj_type: 0x80,
        price: 10,
        ..ShopRow::default()
    }
}

/// A vendor open on its Buy tab (or on its Sell tab), with the shopping list and the items to
/// sell given.
fn shopping(sell_tab: bool, buy_list: Vec<ShopRow>, sell_list: Vec<ShopRow>) -> GameState {
    let mut state = in_world();
    state.shop = Some(ShopView {
        open: true,
        sell_mode: sell_tab,
        buy_list,
        sell_list,
        ..ShopView::default()
    });
    state
}

/// The rows of the vendor window's lists, top to bottom: what the pad can stop on in the window
/// that is a row's height (34 at this scale), where its buttons are shorter.
fn rows(h: &Harness) -> Vec<Rect> {
    let window =
        h.ui.windows
            .rects(1.0)
            .into_iter()
            .find(|(w, _)| *w == WindowId::Vendor)
            .map(|(_, r)| r)
            .expect("the vendor window is open");
    let mut rows: Vec<Rect> = h
        .input
        .nav
        .last
        .targets
        .iter()
        .map(|t| t.rect)
        .filter(|r| window.contains(r.x + 1.0, r.y + 1.0) && (r.h - 34.0).abs() < 0.5)
        .collect();
    rows.sort_by(|a, b| (a.y, a.x).partial_cmp(&(b.y, b.x)).unwrap());
    rows
}

fn right_click(h: &mut Harness, state: &GameState, r: Rect) -> Vec<UiRequest> {
    h.move_to(r.x + r.w / 2.0, r.y + r.h / 2.0);
    h.input.down[1] = true;
    h.input.pressed[1] = true;
    let out = h.frame(state);
    h.input.down[1] = false;
    h.input.released[1] = true;
    h.frame(state);
    out.requests
}

#[test]
fn a_right_click_on_a_row_of_the_shopping_list_takes_it_back_off() {
    let state = shopping(
        false,
        vec![row(BREAD, "Bread"), row(ARROWS, "Arrows")],
        Vec::new(),
    );
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    h.frame(&state);
    let rows = rows(&h);
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert_eq!(
        right_click(&mut h, &state, rows[1]),
        vec![UiRequest::VendorClearList {
            sell: false,
            item: Some(ARROWS),
        }]
    );
}

#[test]
fn a_right_click_on_a_row_of_the_items_to_sell_takes_it_back_off() {
    let state = shopping(true, Vec::new(), vec![row(SWORD, "Sword")]);
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    h.frame(&state);
    let rows = rows(&h);
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(
        right_click(&mut h, &state, rows[0]),
        vec![UiRequest::VendorClearList {
            sell: true,
            item: Some(SWORD),
        }]
    );
}
