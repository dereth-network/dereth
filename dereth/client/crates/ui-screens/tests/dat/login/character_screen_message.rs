//! The world's character screen message shows in a floating chat window right of the Create
//! Character button, titled Announcements, with no input row; its close button hides it; a world
//! that sends none gets no window.
//! Fixture: shipped layouts loaded from the retail DATs.

use crate::common::layout::RegistrationOrder;

use dereth_client_contract::persist::CharacterSet;
use dereth_client_contract::pregame::PregameView;
use dereth_ui::framework::{PregameCx, Screen, ScreenCx};
use dereth_ui::{Delivery, ElementId, UiSystem};
use dereth_ui_screens::screens::charmgmt::CharacterManagementScreen;
use dereth_ui_screens::screens::screen_message::{
    CLOSE_BUTTON, INPUT_CHILDREN, INPUT_ROW, PLACE, SCROLLBAR, TEXT, TEXT_FRAME, TITLE, TITLE_TEXT,
};

fn charmgmt() -> (UiSystem, CharacterManagementScreen) {
    let (mut ui, _flow, _store) =
        crate::common::layout::load((800, 600), RegistrationOrder::BeforeResolver);
    let mut s = CharacterManagementScreen::default();
    s.create(&mut ScreenCx::new(&mut ui))
        .expect("the character-management screen builds");
    let _ = ui.drain_outbox();
    (ui, s)
}

fn pregame(ui: &mut UiSystem, s: &mut CharacterManagementScreen, view: &PregameView) {
    let set = CharacterSet::default();
    let p = PregameCx {
        view,
        received_set: false,
        char_set: &set,
        char_set_changed: false,
        selected_avatar: dereth_primitives::ObjectId(0),
        chargen_slot: -1,
        chargen_response_changed: false,
        ui_strings: None,
        client_strings: None,
        tables: None,
    };
    s.on_pregame(&mut ScreenCx::new(ui), &p);
}

fn text_of(ui: &mut UiSystem, w: dereth_ui::ElemHandle, id: dereth_ui::ElementId) -> String {
    ui.get_child_recursive(w, id)
        .and_then(|c| ui.text_element_mut(c))
        .map(|t| t.glyphs.inq_text(false))
        .unwrap_or_default()
}

/// Behaviour: login.character-select.the-worlds-message-shows-in-a-window-of-its-own
#[test]
fn the_worlds_character_screen_message_shows_in_a_floating_window_that_closes() {
    let (mut ui, mut s) = charmgmt();
    let quiet = PregameView {
        world_name: Some("Frostfell".into()),
        ..PregameView::default()
    };
    pregame(&mut ui, &mut s, &quiet);
    assert!(s.message.window.is_none(), "no message, no window");

    // A world that ends its lines with CR LF, as a configuration file written on Windows does.
    let view = PregameView {
        world_name: Some("Frostfell".into()),
        character_screen_message: Some("Welcome to Frostfell.\r\nBe kind.\rPlay fair.".into()),
        ..PregameView::default()
    };
    pregame(&mut ui, &mut s, &view);
    let w = s.message.window.expect("the window is built");
    let n = ui.node(w).expect("alive");
    assert!(n.region.flags.visible);
    let b = n.region.box_;
    assert_eq!((b.x0, b.y0, b.width(), b.height()), PLACE);
    // Its bottom is the characters frame's.
    let frame = ui
        .get_child_recursive(s.roots()[0], ElementId(0x1000_039C))
        .and_then(|h| ui.node(h))
        .map(|n| n.region.box_)
        .expect("the characters frame");
    assert_eq!(
        b.y1, frame.y1,
        "the window ends where the characters frame does"
    );
    assert_eq!(
        text_of(&mut ui, w, TEXT),
        "Welcome to Frostfell.\nBe kind.\nPlay fair.",
        "carriage returns are line breaks"
    );
    assert_eq!(text_of(&mut ui, w, TITLE), TITLE_TEXT);
    assert_eq!(TITLE_TEXT, "Announcements");
    for id in INPUT_CHILDREN.into_iter().chain([INPUT_ROW]) {
        if let Some(h) = ui.get_child_recursive(w, id) {
            assert!(
                !ui.node(h).expect("alive").region.flags.visible,
                "{id:?} hidden"
            );
        }
    }
    // The text's frame and its scrollbar reach down to the window's bottom border, over the
    // hidden input row.
    let bottom = |ui: &UiSystem, id| {
        ui.get_child_recursive(w, id)
            .map(|h| ui.screen_box(h).y1 + 1)
            .expect("in the window")
    };
    let border = bottom(&ui, ElementId(0x1000_04D4));
    let border_top = ui
        .get_child_recursive(w, ElementId(0x1000_04D4))
        .map(|h| ui.screen_box(h).y0)
        .expect("the bottom border");
    assert_eq!(
        bottom(&ui, TEXT_FRAME),
        border_top,
        "the text fills the window"
    );
    assert_eq!(bottom(&ui, TEXT), border_top);
    assert_eq!(bottom(&ui, SCROLLBAR), border_top);
    assert!(border_top < border);

    // The close button.
    let close = ui
        .get_child_recursive(w, CLOSE_BUTTON)
        .expect("a close button");
    ui.broadcast_element_message(close, dereth_ui::msg::element::id::BUTTON_CLICKED, 7, 0);
    for d in ui.drain_outbox() {
        if let Delivery::Element { msg, .. } = d {
            s.on_element_message(&mut ScreenCx::new(&mut ui), &msg);
        }
    }
    assert!(!ui.node(w).expect("alive").region.flags.visible, "closed");
    pregame(&mut ui, &mut s, &view);
    assert!(
        !ui.node(w).expect("alive").region.flags.visible,
        "and stays closed"
    );
}
