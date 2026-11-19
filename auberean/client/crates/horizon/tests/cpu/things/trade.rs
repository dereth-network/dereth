//! The trade window: when it shows, an item carried onto the player's side offered, an offer
//! carried out taken back, accept waiting for the game's answer, and a trade the partner closed.
//!
//! Behaviour: none (this client's own interface, not a behaviour of the retail client)

use auberean_client_sdk::contract::TradeAction;
use auberean_client_sdk::Action;
use auberean_horizon::keys::Keyed;
use auberean_horizon::state::goods::{Offer, Trade};
use auberean_horizon::state::GameState;

use crate::things::{carry, holding, item, mid, pack, stack, without_selecting};
use crate::Harness;

const PARTNER: u64 = 0x2130_0000_0000_0009;
const SWORD: u64 = 0x7000_0000_0000_0001;
const GEM: u64 = 0x7000_0000_0000_0004;

fn trading(t: Trade) -> GameState {
    let mut s = holding(pack(
        [&[SWORD, GEM], &[], &[], &[]],
        &[(SWORD, item("Sword")), (GEM, stack("Gems", 9, 20))],
        &[],
        0,
    ));
    s.goods.trade = Some(Trade {
        partner: PARTNER,
        partner_name: "Ardo".to_owned(),
        ..t
    });
    s
}

#[test]
fn the_window_shows_only_once_the_trade_opens_or_while_the_player_waits() {
    let mut h = Harness::new();
    // Asked by the partner: the game's question asks; no window yet.
    let s = trading(Trade::default());
    h.frame(&s);
    assert!(h.ui.goods.trade.rect().is_none());
    let s = trading(Trade {
        asked_by_player: true,
        ..Trade::default()
    });
    h.frame(&s);
    assert!(h.ui.goods.trade.rect().is_some(), "waiting for the partner");
    let s = trading(Trade {
        open: true,
        ..Trade::default()
    });
    h.frame(&s);
    assert!(h.ui.goods.trade.rect().is_some());
}

#[test]
fn an_item_carried_onto_the_players_side_is_offered_a_stack_asking_how_many() {
    let mut h = Harness::new();
    let s = trading(Trade {
        open: true,
        ..Trade::default()
    });
    h.ui.keyed(Keyed::Inventory, &s);
    h.frame(&s);
    let k = h.k();
    let inv = h.ui.goods.inventory.layout(k).expect("open");
    let t = h.ui.goods.trade.layout(k).expect("shown");
    let acts = without_selecting(carry(&mut h, &s, mid(inv.cell(0, k)), mid(t.offer)));
    assert_eq!(
        acts,
        [Action::Trade(TradeAction::Offer {
            item: SWORD,
            quantity: 1
        })]
    );
    let acts = without_selecting(carry(&mut h, &s, mid(inv.cell(1, k)), mid(t.offer)));
    assert!(acts.is_empty());
    assert_eq!(h.ui.goods.amount.as_ref().map(|a| a.most), Some(9));
}

#[test]
fn an_offer_carried_out_of_the_players_side_is_taken_back() {
    let mut h = Harness::new();
    let s = trading(Trade {
        open: true,
        mine: vec![Offer {
            item: SWORD,
            quantity: 1,
        }],
        ..Trade::default()
    });
    h.frame(&s);
    let k = h.k();
    let t = h.ui.goods.trade.layout(k).expect("shown");
    let first = auberean_horizon::ui::trade::Layout::cell(t.mine, 0, k);
    let acts = without_selecting(carry(&mut h, &s, mid(first), (300.0, 900.0)));
    assert_eq!(acts, [Action::Trade(TradeAction::Revoke(SWORD))]);
}

#[test]
fn accept_waits_for_the_games_answer_once_pressed() {
    let mut h = Harness::new();
    let s = trading(Trade {
        open: true,
        ..Trade::default()
    });
    h.frame(&s);
    let t = h.ui.goods.trade.layout(h.k()).expect("shown");
    let out = h.click(&s, mid(t.accept));
    assert_eq!(out.acts, [Action::Trade(TradeAction::Accept)]);
    assert_eq!(
        h.ui.goods.asks.trade,
        [auberean_horizon::ui::trade::Settle::Answering]
    );
    let waiting = trading(Trade {
        open: true,
        answering: true,
        ..Trade::default()
    });
    let out = h.click(&waiting, mid(t.accept));
    assert!(out.acts.is_empty(), "not again until the game answers");
}

#[test]
fn a_trade_the_partner_closed_closes_its_window_only() {
    let mut h = Harness::new();
    let s = trading(Trade {
        ended_by_partner: true,
        ..Trade::default()
    });
    h.frame(&s);
    assert!(h.ui.goods.trade.rect().is_some(), "it says so");
    let mut asked = Vec::new();
    assert!(h.ui.close_top(&mut asked));
    assert!(asked.is_empty(), "the trade is closed already");
    assert_eq!(
        h.ui.goods.asks.trade,
        [auberean_horizon::ui::trade::Settle::Dismissed]
    );
}
