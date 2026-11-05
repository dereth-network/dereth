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

/// Everything on but what `off` takes away.
fn without(off: fn(&mut EraFeatures)) -> GameState {
    let mut state = in_world();
    state.world_services.features = EraFeatures::ALL;
    state.notebook = true;
    off(&mut state.world_services.features);
    state
}

/// Behaviour: presentation.era.every-interface-refuses-what-the-world-lacks
/// A window of a system the world lacks stays shut through the one place that opens windows:
/// directly, by its key, and from the game's own notice. One open as the system goes closes.
#[test]
fn a_window_of_a_system_the_world_lacks_stays_shut_whatever_asks() {
    let mut h = Harness::new(Default::default());
    let mut state = without(|f| {
        f.housing = false;
        f.apartments = false;
    });
    h.frame(&state);
    h.ui.windows.open(WindowId::House, 0.0);
    assert!(!h.ui.windows.is_open(WindowId::House), "asked directly");
    // The house key.
    h.input.actions.push(0x1000_0018);
    h.frame(&state);
    assert!(!h.ui.windows.is_open(WindowId::House), "by its key");
    // A deed's profile from the game.
    let mut deed = at_a_deed();
    deed.world_services.features.housing = false;
    deed.world_services.features.apartments = false;
    h.frame(&deed);
    assert!(!h.ui.windows.is_open(WindowId::Maintenance), "by the game");

    state.world_services.features = EraFeatures::ALL;
    h.frame(&state);
    h.ui.windows.open(WindowId::House, 0.0);
    assert!(h.ui.windows.is_open(WindowId::House));
    state.world_services.features.housing = false;
    h.frame(&state);
    assert!(!h.ui.windows.is_open(WindowId::House), "closes as it goes");
}

/// Behaviour: presentation.era.every-interface-refuses-what-the-world-lacks
/// The trade the game holds open on a world without trade opens no window and sends no close,
/// frame after frame; the same for the salvage list.
#[test]
fn a_refused_trade_or_salvage_window_does_not_answer_the_game_every_frame() {
    use dereth_client_contract::panels::salvage::SalvageAction;
    let mut h = Harness::new(Default::default());
    let mut state = without(|f| {
        f.trade = false;
        f.tinkering = false;
    });
    state.trade = Some(dereth_client_contract::view::TradeView {
        open: true,
        ..Default::default()
    });
    state.salvage = Some(Default::default());
    for _ in 0..4 {
        let out = h.frame(&state);
        assert!(!out.requests.contains(&UiRequest::TradeClose));
        assert!(!out
            .requests
            .contains(&UiRequest::SalvageList(SalvageAction::Close)));
        assert!(!h.ui.windows.is_open(WindowId::Trade));
        assert!(!h.ui.windows.is_open(WindowId::Salvage));
    }
}

/// Behaviour: presentation.era.every-interface-refuses-what-the-world-lacks
/// The journal's keys follow the journal and the contract key the contracts, though one window
/// holds both.
#[test]
fn the_journal_and_contract_keys_each_follow_their_own_system() {
    const JOURNAL: u32 = 0x1000_0128;
    const CONTRACTS: u32 = 0x1000_012E;
    for (state, key, opens) in [
        (without(|f| f.journal = false), JOURNAL, false),
        (without(|f| f.journal = false), CONTRACTS, true),
        (without(|f| f.contracts = false), CONTRACTS, false),
        (without(|f| f.contracts = false), JOURNAL, true),
    ] {
        let mut h = Harness::new(Default::default());
        h.frame(&state);
        h.input.actions.push(key);
        h.frame(&state);
        assert_eq!(h.ui.windows.is_open(WindowId::Journal), opens, "{key:x}");
    }
}

/// Behaviour: presentation.era.every-interface-refuses-what-the-world-lacks
/// With one page the Journal draws it and has nothing to turn to.
#[test]
fn the_journal_window_shows_only_the_pages_the_world_has() {
    use dereth_client_contract::journal::JournalAction;
    let shows_notebook = |state: &GameState| {
        let mut h = Harness::new(Default::default());
        h.frame(state);
        h.ui.windows.open(WindowId::Journal, 0.0);
        h.frame(state)
            .requests
            .contains(&UiRequest::Journal(JournalAction::Visibility(true)))
    };
    let mut no_notebook = without(|_| {});
    no_notebook.notebook = false;
    assert!(!shows_notebook(&no_notebook));
    assert!(
        shows_notebook(&without(|f| f.contracts = false)),
        "the notebook is the one page there is"
    );
    assert!(
        !shows_notebook(&without(|_| {})),
        "both pages: the contracts first"
    );
}

/// The Allegiance window's Swear button, at a layout scale of one.
fn swear_button() -> (f32, f32) {
    let ((w, h), (x, y)) = WindowId::Allegiance.geometry();
    let (bx, bw) = (x + 12.0, w - 24.0);
    let foot = y + h - 10.0 - 30.0;
    (bx + (bw - 12.0) / 6.0, foot + 14.0)
}

/// Behaviour: presentation.era.every-interface-refuses-what-the-world-lacks
/// The Swear button follows the shared rule the other two interfaces read, not only the selection:
/// a player the rule refuses (short of the experience an oath costs) is no one to swear to.
#[test]
fn the_swear_button_follows_the_shared_rule_for_whom_the_character_may_swear_to() {
    use dereth_horizon::ui::game::{Relation, Target};
    let someone = ObjectId(0x5000_0002);
    let at = |swear_target| {
        let mut state = in_world();
        state.target = Some(Target {
            id: someone,
            name: "Someone".into(),
            relation: Relation::Player,
            health: None,
            level: None,
            icon: None,
            look: None,
        });
        state.swear_target = swear_target;
        let mut h = Harness::new(Default::default());
        h.frame(&state);
        h.ui.windows.open(WindowId::Allegiance, 0.0);
        h.frame(&state);
        let (x, y) = swear_button();
        h.move_to(x, y);
        h.press();
        let out = h.frame(&state);
        h.release();
        out.allegiance_question
    };
    assert_eq!(at(None), None, "short of experience, the button is off");
    let asked = at(Some(someone)).expect("asked");
    assert_eq!(asked.1, someone);
}

/// Behaviour: presentation.era.fellowship-options-follow-the-world
/// A world without open fellowships has no Open button: the buttons after it close up, and its
/// place is Quit's.
#[test]
fn the_fellowship_window_has_an_open_button_only_with_open_fellowships() {
    use dereth_client_contract::view::{FellowEntry, FellowshipView};
    let me = ObjectId(0x5000_0001);
    let press_fourth_button = |fellowship_open: bool| {
        let mut state = without(|_| {});
        state.era = Some(EraFeatures {
            fellowship_open,
            ..EraFeatures::ALL
        });
        state.player_id = Some(me);
        state.fellowship_view = Some(FellowshipView {
            leader: me,
            members: vec![FellowEntry {
                id: me,
                ..FellowEntry::default()
            }],
            ..FellowshipView::default()
        });
        let mut h = Harness::new(Default::default());
        h.frame(&state);
        h.ui.windows.open(WindowId::Social, 0.0);
        h.frame(&state);
        let ((w, height), (x, y)) = WindowId::Social.geometry();
        let bw = (w - 24.0 - 30.0) / 6.0;
        let foot = y + height - 10.0 - 30.0;
        h.move_to(x + 12.0 + 3.0 * (bw + 6.0) + bw / 2.0, foot + 14.0);
        h.press();
        let mut asked = h.frame(&state).requests;
        h.release();
        asked.extend(h.frame(&state).requests);
        asked
    };
    assert!(press_fourth_button(true).contains(&UiRequest::FellowshipToggleOpenness));
    let without_it = press_fourth_button(false);
    assert!(!without_it.contains(&UiRequest::FellowshipToggleOpenness));
    assert!(without_it.contains(&UiRequest::FellowshipQuit { disband: false }));
}
