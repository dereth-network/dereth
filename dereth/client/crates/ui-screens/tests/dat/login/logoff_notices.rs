//! The gameplay screen is a registered handler for both logout notices; the log-off notice raises
//! the logoff confirmation and arms the quit; the end-character-session notice carries its int and
//! its two arms differ.
//! Fixture: shipped layouts, strings and keymaps loaded from the retail DATs.

use crate::common::layout::RegistrationOrder;

use dereth_ui::framework::Screen;
use dereth_ui::{Delivery, NoticeId, NoticePayload, UiSystem};

use dereth_ui_screens::screens::gameplay::{logout, GamePlayScreen, NOTICES};

const LOGOFF: NoticeId = NoticeId::Logoff;
const END_CHARACTER_SESSION: NoticeId = NoticeId::EndCharacterSession;

fn env() -> UiSystem {
    let (ui, _flow, _store) =
        crate::common::layout::load((800, 600), RegistrationOrder::BeforeResolver);
    ui
}

fn screen() -> (UiSystem, GamePlayScreen) {
    let mut ui = env();
    let mut s = GamePlayScreen::default();
    s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("the gameplay screen builds");
    let _ = ui.drain_outbox();
    (ui, s)
}

/// The global event fan-out followed by the UI flow's notice delivery — the production route, so
/// a screen that is not registered receives nothing and every assertion below fails.
///
/// Returns how many notice deliveries were addressed to a listener, which is the *"is anyone
/// registered"* half stated separately from *"did the handler do anything"*.
fn send(ui: &mut UiSystem, s: &mut GamePlayScreen, id: NoticeId, a: u32) -> usize {
    ui.send_notice(
        id,
        &NoticePayload {
            a,
            ..NoticePayload::default()
        },
    );
    let mut n = 0;
    for d in ui.drain_outbox() {
        match d {
            Delivery::Notice { id, payload, .. } => {
                n += 1;
                s.on_notice(&mut dereth_ui::framework::ScreenCx::new(ui), id, &payload);
            }
            Delivery::Element { msg, .. } => {
                s.on_element_message(&mut dereth_ui::framework::ScreenCx::new(ui), &msg)
            }
            Delivery::Global { id, param, .. } => {
                s.on_global_message(&mut dereth_ui::framework::ScreenCx::new(ui), id, param)
            }
        }
    }
    n
}

// ---------------------------------------------------------------------------------------------

/// The screen is a registered handler for both logout notices.
#[test]
fn the_screen_is_a_registered_handler_for_both_logout_notices() {
    let (mut ui, mut s) = screen();
    assert_eq!(NOTICES, [END_CHARACTER_SESSION, LOGOFF]);
    // A notice the class does not register reaches nobody.
    assert_eq!(send(&mut ui, &mut s, NoticeId::DidTagClicked, 0), 0);
    for id in NOTICES {
        assert_eq!(
            send(&mut ui, &mut s, id, 0),
            1,
            "notice {id:?} reached no listener"
        );
    }
}

/// Behaviour: logout.notices.the-logoff-notice-raises-the-confirmation-and-arms-the-quit
/// The log-off notice raises the logoff confirmation and arms the quit.
#[test]
fn the_logoff_notice_raises_the_logoff_confirmation_and_arms_the_quit() {
    let (mut ui, mut s) = screen();
    assert!(!s.do_end_session);
    assert!(!s.should_quit_on_logout);
    assert!(s.logout_dialog().is_none());

    assert_eq!(send(&mut ui, &mut s, LOGOFF, 0), 1);

    assert!(s.do_end_session, "do_end_session");
    assert!(
        s.should_quit_on_logout,
        "should_quit_on_logout — *Yes* quits, it does not go to select"
    );
    let d = s
        .logout_dialog()
        .expect("the logout confirmation dialog was not raised");
    assert!(ui.node(d).is_some(), "and the dialog element is live");
    // The confirmation dialog is only made when none is open, so a second notice raises no second one.
    assert_eq!(send(&mut ui, &mut s, LOGOFF, 0), 1);
    assert_eq!(s.logout_dialog(), Some(d), "one dialog, never two");
    // The two prompts are different strings and this arm takes the log-off one.
    assert_ne!(logout::LOGOFF_CONFIRM, logout::END_SESSION_CONFIRM);
}

/// The end-character-session notice carries its int and the two arms differ.
#[test]
fn the_end_character_session_notice_carries_its_int_and_the_two_arms_differ() {
    // param != 0 — ask, then go to character select.
    let (mut ui, mut s) = screen();
    assert_eq!(send(&mut ui, &mut s, END_CHARACTER_SESSION, 1), 1);
    assert!(s.do_end_session);
    assert!(
        !s.should_quit_on_logout,
        "the asking branch does not arm the quit"
    );
    assert!(s.logout_dialog().is_some());

    // param == 0 — quit now, no dialog at all.
    let (mut ui, mut s) = screen();
    assert_eq!(send(&mut ui, &mut s, END_CHARACTER_SESSION, 0), 1);
    assert!(s.do_end_session);
    assert!(s.should_quit_on_logout);
    assert!(
        s.logout_dialog().is_none(),
        "the quit-now branch asks nothing"
    );
}
