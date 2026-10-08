//! The world's services follow the game: the housing window opens with a deed's house profile
//! and pays through the game's lists and question, the Game Center follows the chess game and
//! moves on a press, the barber opens with its start notice, and what the world's era lacks is
//! not offered.
//!
//! Behaviour: none (experimental Horizon interface)

use dereth_client_contract::panels::slumlord::{
    HouseOp, PaymentAction, PaymentItem, PaymentListsView,
};
use dereth_client_contract::view::{BarberView, MiniGameView, SlumlordPayment, SlumlordView};
use dereth_client_contract::UiRequest;
use dereth_horizon::ui::game::GameState;
use dereth_horizon::ui::panels::WindowId;
use dereth_primitives::{EraFeatures, ObjectId};

use crate::Harness;

fn in_world() -> GameState {
    GameState {
        in_world: true,
        name: "Tester".into(),
        ..GameState::default()
    }
}

/// The game's deed offering an unowned cottage, its price on the list and paid in full.
fn at_a_deed() -> GameState {
    let mut state = in_world();
    let w = &mut state.world_services;
    w.features = EraFeatures::ALL;
    w.payments = PaymentListsView {
        op: HouseOp::Buy,
        buy: vec![PaymentItem {
            id: ObjectId(0x5000_0010),
            wcid: 273,
            amount: 1,
            trade_note_value: None,
        }],
        buy_payment: SlumlordPayment {
            requirements: "1 Pyreal".into(),
            paid_in_full: true,
        },
        visible: true,
        ..PaymentListsView::default()
    };
    w.slumlord = Some(SlumlordView {
        slumlord: ObjectId(0x7000_0001),
        house_type: 1,
        ..SlumlordView::default()
    });
    w.slumlord_notices = 1;
    state
}

fn click(h: &mut Harness, x: f32, y: f32, state: &GameState) -> dereth_horizon::ui::Outcome {
    h.move_to(x, y);
    h.press();
    let out = h.frame(state);
    h.release();
    let rest = h.frame(state);
    let mut all = out;
    all.requests.extend(rest.requests);
    all
}

#[test]
fn a_deed_s_profile_opens_the_housing_window_and_a_purchase_is_asked_then_paid() {
    let mut h = Harness::new(Default::default());
    let state = at_a_deed();
    h.frame(&state);
    assert!(h.ui.windows.is_open(WindowId::Maintenance));
    // Buy: the body is the window's (700, 260) less its frame; the button sits at its lower right.
    let out = click(&mut h, 1093.0, 526.0, &state);
    assert!(
        out.requests
            .contains(&UiRequest::HousePaymentConfirmation { rent: false }),
        "{:?}",
        out.requests
    );
    // A second press while the question is up asks nothing more.
    let again = click(&mut h, 1093.0, 526.0, &state);
    assert!(!again
        .requests
        .contains(&UiRequest::HousePaymentConfirmation { rent: false }));
    h.ui.windows.world.answers.push((false, Some(true)));
    let out = h.frame(&state);
    assert!(
        out.requests
            .contains(&UiRequest::PaymentList(PaymentAction::Submit)),
        "{:?}",
        out.requests
    );
}

#[test]
fn a_no_to_the_purchase_question_closes_the_lists() {
    let mut h = Harness::new(Default::default());
    let state = at_a_deed();
    h.frame(&state);
    h.ui.windows.world.answers.push((false, Some(false)));
    let out = h.frame(&state);
    assert!(out
        .requests
        .contains(&UiRequest::PaymentList(PaymentAction::Close)));
    assert!(!out
        .requests
        .contains(&UiRequest::PaymentList(PaymentAction::Submit)));
}

#[test]
fn closing_the_housing_window_closes_the_lists_and_stops_watching_the_deed() {
    let mut h = Harness::new(Default::default());
    let mut state = at_a_deed();
    h.frame(&state);
    h.ui.windows.close(WindowId::Maintenance);
    let out = h.frame(&state);
    assert!(out
        .requests
        .contains(&UiRequest::PaymentList(PaymentAction::Close)));
    assert!(out.requests.contains(&UiRequest::UnregisterSlumlordRange));
    // The game takes the lists down, and the window stays shut.
    state.world_services.payments.visible = false;
    h.frame(&state);
    assert!(!h.ui.windows.is_open(WindowId::Maintenance));
}

#[test]
fn the_game_center_follows_the_game_and_a_press_on_a_square_moves() {
    let mut h = Harness::new(Default::default());
    let mut state = in_world();
    let mut slots = [None; 64];
    slots[0] = Some(3);
    state.world_services.minigame = Some(MiniGameView {
        visible: true,
        team: 0,
        game: ObjectId(9),
        piece_slots: slots,
        ..MiniGameView::default()
    });
    h.frame(&state);
    assert!(h.ui.windows.is_open(WindowId::GameCenter));
    // The board's first square, in the window at (720, 140) less its frame.
    let out = click(&mut h, 760.0, 213.0, &state);
    assert!(
        out.requests.contains(&UiRequest::MiniGameBoardPress(0)),
        "{:?}",
        out.requests
    );
    let out = click(&mut h, 800.0, 597.0, &state);
    assert!(
        out.requests
            .contains(&UiRequest::MiniGameButton(0x1000_0175)),
        "{:?}",
        out.requests
    );
    state.world_services.minigame = Some(MiniGameView::default());
    h.frame(&state);
    assert!(!h.ui.windows.is_open(WindowId::GameCenter));
}

#[test]
fn the_barber_opens_with_its_start_and_closing_it_sends_nothing() {
    let mut h = Harness::new(Default::default());
    let mut state = in_world();
    state.world_services.barber = Some(BarberView {
        generation: 1,
        heritage: 1,
        gender: 1,
        ..BarberView::default()
    });
    h.frame(&state);
    assert!(h.ui.windows.is_open(WindowId::Barber));
    h.ui.windows.close(WindowId::Barber);
    let out = h.frame(&state);
    assert!(
        !out.requests
            .iter()
            .any(|r| matches!(r, UiRequest::BarberFinish(_))),
        "{:?}",
        out.requests
    );
    assert!(!h.ui.windows.is_open(WindowId::Barber));
    // The same notice does not open it again; a new one does.
    h.frame(&state);
    assert!(!h.ui.windows.is_open(WindowId::Barber));
    state.world_services.barber = Some(BarberView {
        generation: 2,
        heritage: 1,
        gender: 1,
        ..BarberView::default()
    });
    h.frame(&state);
    assert!(h.ui.windows.is_open(WindowId::Barber));
}

#[test]
fn a_world_without_housing_or_chess_offers_neither_window() {
    let mut h = Harness::new(Default::default());
    let mut state = in_world();
    state.world_services.features = EraFeatures::ALL;
    h.frame(&state);
    assert!(h.ui.windows.offers(WindowId::House));
    state.world_services.features.housing = false;
    state.world_services.features.chess = false;
    h.frame(&state);
    assert!(!h.ui.windows.offers(WindowId::House));
    assert!(!h.ui.windows.offers(WindowId::GameCenter));
    assert!(h.ui.windows.offers(WindowId::Map));
}
