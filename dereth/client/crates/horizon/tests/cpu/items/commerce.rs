//! The shop and trade windows follow the game: each opens with the game's vendor or trade, and
//! closing the window closes it in the game.
//!
//! Behaviour: none (this client's own interface, not a behaviour of the retail client)

use dereth_client_contract::view::{ShopView, TradeView};
use dereth_client_contract::UiRequest;
use dereth_horizon::ui::game::GameState;
use dereth_horizon::ui::panels::WindowId;

use crate::Harness;

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
