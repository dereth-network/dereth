//! The character screen's five dialog contexts read the shared table and agree row for row; an
//! error raised under the exit confirmation waits; a second request for a queued context is
//! refused; destroying the screen unregisters every id.
//! Fixture: shipped layouts, strings and keymaps loaded from the retail DATs.

use crate::common::layout::RegistrationOrder;

use dereth_ui::dialog::DialogKind;
use dereth_ui::framework::Screen;
use dereth_ui::{ElemHandle, ElementId, ElementMessage, MessageId, UiSystem};

use dereth_ui_screens::screens::charmgmt::{
    CharacterManagementScreen, DialogContext, DIALOG_CONTEXTS,
};

/// The retail dat and the layout resolver. An `expect`, not a skip — the stated testability rule says *"a test
/// that skips is a test that passes"*.
fn ui_with_environment() -> UiSystem {
    let (ui, _flow, _store) =
        crate::common::layout::load((800, 600), RegistrationOrder::AfterResolver);
    ui
}

fn click(id: u32) -> ElementMessage {
    ElementMessage {
        source_id: ElementId(id),
        source: ElemHandle::for_test(1),
        id: dereth_ui::msg::element::id::BUTTON_CLICKED,
        p1: 0,
        p2: 0,
        point: dereth_ui::msg::MessagePoint::default(),
        serial: 1,
    }
}

/// **(c)** The screen's table *is* the shared table.
///
/// The five rows are quoted here in full so that a transposition in either copy is caught, and the
/// equality below is what makes them one copy rather than two. `DialogContext::answer_children`
/// is now `self.kind().answer_children()` and nothing else.
///
/// Falsified by: putting a literal back in `DialogContext::answer_children`; transposing any row
/// of `DialogKind::answer_children` (the accept/cancel column then disagrees with the table here);
/// changing `DialogContext::kind`.
#[test]
fn the_five_contexts_read_the_shared_table_and_agree_with_it_row_for_row() {
    // context, kind, accept, cancel — read off each subclass's own `listen_to_element_message`.
    let rows: [(DialogContext, DialogKind, Option<u32>, Option<u32>, &str); 5] = [
        (
            DialogContext::ConfirmExit,
            DialogKind::Confirmation,
            Some(0x17),
            Some(0x19),
            "the confirmation dialog",
        ),
        (
            DialogContext::DeleteCharacter,
            DialogKind::ConfirmationTextInput,
            Some(0x2E),
            Some(0x2F),
            "the confirmation-with-text-input dialog",
        ),
        (
            DialogContext::ErrorMessage,
            DialogKind::Message,
            Some(0x26),
            None,
            "the message dialog",
        ),
        (
            DialogContext::PleaseWait,
            DialogKind::Wait,
            None,
            None,
            "the wait dialog",
        ),
        (
            DialogContext::EnteringWorld,
            DialogKind::Wait,
            None,
            None,
            "the wait dialog",
        ),
    ];
    assert_eq!(
        rows.len(),
        DIALOG_CONTEXTS.len(),
        "all five contexts, no more and no fewer"
    );

    for (ctx, kind, accept, cancel, where_) in rows {
        assert_eq!(ctx.kind(), kind, "{ctx:?}: property 0x8E");
        let got = ctx.answer_children();
        assert_eq!(got.0.map(|e| e.0), accept, "{ctx:?} accept, per {where_}");
        assert_eq!(got.1.map(|e| e.0), cancel, "{ctx:?} cancel, per {where_}");
        // The point of the deletion: the screen's answer is the shared table's answer.
        assert_eq!(
            got,
            kind.answer_children(),
            "{ctx:?} reads dereth_ui's table"
        );
    }
}

/// Behaviour: ui.dialogs.a-message-raised-under-an-open-question-waits-and-a-duplicate-context-is-refused
/// An error message raised while the exit confirmation is up waits for it.
#[test]
fn an_error_message_raised_while_the_exit_confirmation_is_up_waits_for_it() {
    let mut ui = ui_with_environment();
    let mut s = CharacterManagementScreen::default();
    s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("the screen builds from its real layout");

    // EXIT — the exit confirmation is made with `d[0x8E] = 1`.
    s.on_element_message(
        &mut dereth_ui::framework::ScreenCx::new(&mut ui),
        &click(0x1000_03A4),
    );
    assert_eq!(s.open_dialog, Some(DialogContext::ConfirmExit));
    let exit = s
        .dialog_element(DialogContext::ConfirmExit)
        .expect("the confirmation is on screen");
    assert!(ui.is_alive(exit));
    assert_eq!(ui.dialogs.waiting_on(2), 0, "nothing behind it yet");

    // An error arrives while it is up — then `make_error_message_dialog`.
    s.set_error_msg("ID_NetErr_ConnectionLost".into());
    s.update(
        &mut dereth_ui::framework::ScreenCx::new(&mut ui),
        dereth_primitives::LocalTime(1.0),
    );
    assert!(
        s.dialog_element(DialogContext::ErrorMessage).is_none(),
        "queued behind the confirmation: it has a context and no element"
    );
    assert_eq!(ui.dialogs.waiting_on(2), 1, "make_dialog's push_back arm");
    assert_eq!(
        ui.dialogs
            .open_on(2)
            .and_then(|i| i.dialog.as_ref())
            .map(|d| d.pending_behind),
        Some(1),
        " — the child-0x33 banner's number"
    );
    assert!(
        ui.is_alive(exit),
        "and the confirmation is still the one on screen"
    );

    // *No* on the confirmation.: button 1 is yes, so `0x19` is no.
    s.on_element_message(
        &mut dereth_ui::framework::ScreenCx::new(&mut ui),
        &click(0x19),
    );
    assert!(
        ui.requests.take().is_empty(),
        "answering no queues no mode -- nothing was quit"
    );
    assert!(
        !ui.is_alive(exit),
        "CloseDialog deleted the confirmation's element"
    );
    assert!(s.dialog_element(DialogContext::ConfirmExit).is_none());

    // ...and the error box is now the current dialog on queue 2, with its element built.
    let error = s
        .dialog_element(DialogContext::ErrorMessage)
        .expect("open_next_dialog promoted it and the screen serviced it");
    assert!(ui.is_alive(error));
    assert_eq!(ui.dialogs.waiting_on(2), 0);
    assert_eq!(
        ui.node(error).expect("live").element_id(),
        DialogKind::Message.root_element_id(),
        "d[0x8E] = 3 asks for root 0x24 -- the error box, not a second confirmation"
    );

    // Its one button closes it, and the queue is empty.
    s.on_element_message(
        &mut dereth_ui::framework::ScreenCx::new(&mut ui),
        &click(0x26),
    );
    assert!(!ui.is_alive(error));
    assert!(!ui.dialogs.is_dialog_open(0), "nothing open on any queue");
}

/// The five dialog-context fields hold what the client's hold: **the context that making the
/// dialog returned**, so the screen's "context is non-zero" guard refuses a second request for a
/// dialog that is queued as well as one that is on screen.
///
/// Falsified by: guarding on the element instead of the context (a queued dialog would then be
/// raised twice and take two contexts).
#[test]
fn a_second_request_for_a_queued_context_is_refused_as_it_is_for_an_open_one() {
    let mut ui = ui_with_environment();
    let mut s = CharacterManagementScreen::default();
    s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("the screen builds");

    s.on_element_message(
        &mut dereth_ui::framework::ScreenCx::new(&mut ui),
        &click(0x1000_03A4),
    );
    assert!(s.dialog_element(DialogContext::ConfirmExit).is_some());
    assert!(!s.make_confirm_exit_dialog(&mut ui), "one is already up");

    s.set_error_msg("ID_NetErr_ConnectionLost".into());
    s.update(
        &mut dereth_ui::framework::ScreenCx::new(&mut ui),
        dereth_primitives::LocalTime(1.0),
    );
    assert!(
        s.dialog_element(DialogContext::ErrorMessage).is_none(),
        "queued"
    );
    assert_eq!(ui.dialogs.waiting_on(2), 1);
    assert!(
        !s.make_error_message_dialog(&mut ui),
        "queued still counts as taken"
    );
    assert_eq!(
        ui.dialogs.waiting_on(2),
        1,
        "and no second context was taken for it"
    );
}

/// Destroying character management unregisters every dialog button id it registered — and the
/// list it walks is now
/// [`DialogContext::answer_children`] rather than a third hand-written copy of the same five ids.
///
/// The registrations are keyed on an `ElementId` shared with every other dialog in the build, so
/// one left behind would answer a later screen's dialog.
#[test]
fn destroying_the_screen_unregisters_every_id_the_shared_table_names() {
    let mut ui = ui_with_environment();
    let mut s = CharacterManagementScreen::default();
    s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("the screen builds");
    s.on_element_message(
        &mut dereth_ui::framework::ScreenCx::new(&mut ui),
        &click(0x1000_03A4),
    );
    assert!(s.dialog_element(DialogContext::ConfirmExit).is_some());

    let ids: Vec<ElementId> = DIALOG_CONTEXTS
        .iter()
        .flat_map(|c| {
            let (a, b) = c.answer_children();
            [a, b]
        })
        .flatten()
        .collect();
    assert_eq!(ids.len(), 5, "0x17, 0x19, 0x2E, 0x2F, 0x26");
    assert!(
        ids.iter()
            .any(|id| !ui.element_message_listeners(*id, MessageId(1)).is_empty()),
        "the exit confirmation registered at least one of them"
    );

    for h in s.roots().to_vec() {
        ui.remove_and_delete_root(h);
    }
    s.destroy(&mut dereth_ui::framework::ScreenCx::new(&mut ui));
    for id in ids {
        assert!(
            ui.element_message_listeners(id, MessageId(1)).is_empty(),
            "{id:?} outlived the screen"
        );
    }
}
