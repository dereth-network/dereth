//! The pointer over buttons and list rows: states, presses, clicks.

use auberean_assets::ui::AttributeValue as V;
use auberean_ui::token::{attr, msg, state as st};
use auberean_ui::{Token, Value};

use crate::fixture::{element, image, state, ui, LAYOUT};

fn button() -> auberean_assets::ui::Element {
    let mut b = element(
        "Button",
        "button",
        [10, 10, 59, 29],
        0,
        vec![
            state("normal", 1, vec![], vec![image(1, 0)], vec![]),
            state(
                "stateless",
                1,
                vec![("rollover", V::Bool(true))],
                vec![],
                vec![],
            ),
            state("normal_rollover", 1, vec![], vec![image(2, 0)], vec![]),
            state("normal_pressed", 1, vec![], vec![image(3, 0)], vec![]),
            state("ghosted", 1, vec![], vec![image(4, 0)], vec![]),
        ],
    );
    b.default_state = Token::of("normal").0;
    b
}

/// A button rolls over under the pointer, shows pressed while held, and clicks when released on
/// it; released off it, it sends no click.
///
/// Behaviour: ui.button.a-release-on-the-button-clicks
#[test]
fn a_release_on_the_button_clicks() {
    let mut u = ui(vec![button()], vec![]);
    let l = u.load_layout(LAYOUT).unwrap();
    let b = u.create_root(l, Token::of("Button")).unwrap();
    assert_eq!(u.node(b).unwrap().state, st::NORMAL);
    assert!(u.pointer_move(20, 20));
    assert_eq!(u.node(b).unwrap().state, st::NORMAL_ROLLOVER);
    assert!(u.pointer_press());
    assert_eq!(u.node(b).unwrap().state, st::NORMAL_PRESSED);
    assert!(u.pointer_release());
    let what: Vec<Token> = u.take_messages().iter().map(|m| m.what).collect();
    assert!(what.contains(&msg::BUTTON_PRESS));
    assert!(what.contains(&msg::BUTTON_RELEASE_ON));
    assert_eq!(u.node(b).unwrap().state, st::NORMAL_ROLLOVER);

    u.pointer_press();
    u.pointer_move(500, 500);
    u.pointer_release();
    let what: Vec<Token> = u.take_messages().iter().map(|m| m.what).collect();
    assert!(what.contains(&msg::BUTTON_RELEASE_OFF));
    assert!(!what.contains(&msg::BUTTON_RELEASE_ON));
    assert_eq!(u.node(b).unwrap().state, st::NORMAL);
}

/// A ghosted button shows its ghosted state whatever the pointer does, and never clicks.
///
/// Behaviour: ui.button.a-ghosted-button-never-clicks
#[test]
fn a_ghosted_button_never_clicks() {
    let mut u = ui(vec![button()], vec![]);
    let l = u.load_layout(LAYOUT).unwrap();
    let b = u.create_root(l, Token::of("Button")).unwrap();
    u.set_attr(b, attr::GHOSTED, Value::Bool(true));
    assert_eq!(u.node(b).unwrap().state, st::GHOSTED);
    u.pointer_move(20, 20);
    u.pointer_press();
    u.pointer_release();
    assert_eq!(u.node(b).unwrap().state, st::GHOSTED);
    assert!(u
        .take_messages()
        .iter()
        .all(|m| m.what != msg::BUTTON_RELEASE_ON));
}

/// An element marked invisible to the mouse lets the pointer through to what is beneath it.
///
/// Behaviour: ui.pointer.mouse-invisible-elements-let-the-pointer-through
#[test]
fn mouse_invisible_elements_let_the_pointer_through() {
    let cover = element(
        "Cover",
        "field",
        [0, 0, 99, 99],
        0,
        vec![state(
            "stateless",
            0,
            vec![("mouse_invisible", V::Bool(true))],
            vec![],
            vec![],
        )],
    );
    let under = element("Under", "field", [0, 0, 99, 99], 5, vec![]);
    let mut u = ui(vec![cover, under], vec![]);
    let l = u.load_layout(LAYOUT).unwrap();
    u.create_root(l, Token::of("Cover")).unwrap();
    let under = u.create_root(l, Token::of("Under")).unwrap();
    assert_eq!(u.hit(50, 50), Some(under));
}

/// A list's rows stack from its top; pressing one selects it, which puts the row in the list's
/// selected state and the row selected before in its normal state. (The list stands in a window, as
/// every list of the layouts does: a press on a window's own child stops at that child.)
///
/// Behaviour: ui.list.a-pressed-row-is-selected
#[test]
fn a_pressed_row_is_selected() {
    let row = element(
        "Row",
        "text",
        [0, 0, 99, 19],
        0,
        vec![
            state("stateless", 0, vec![], vec![], vec![]),
            state("normal", 0, vec![], vec![image(1, 2)], vec![]),
            state("highlight", 0, vec![], vec![image(2, 2)], vec![]),
        ],
    );
    let list = element(
        "List",
        "listbox",
        [10, 10, 109, 109],
        0,
        vec![state(
            "stateless",
            0,
            vec![
                ("item_normal_state", V::Id(Token::of("normal").0)),
                ("item_selected_state", V::Id(Token::of("highlight").0)),
                ("selected_item_state_change", V::Bool(true)),
                ("click_select", V::Bool(true)),
            ],
            vec![],
            vec![],
        )],
    );
    let window = element(
        "Window",
        "field",
        [0, 0, 199, 199],
        0,
        vec![state("stateless", 0, vec![], vec![], vec![list])],
    );
    let mut u = ui(vec![row, window], vec![]);
    let l = u.load_layout(LAYOUT).unwrap();
    let window = u.create_root(l, Token::of("Window")).unwrap();
    let list = u.child(window, Token::of("List")).unwrap();
    let rows: Vec<_> = (0..3)
        .map(|_| {
            let r = u.create(l, Token::of("Row")).unwrap();
            u.list_add(list, r);
            r
        })
        .collect();
    assert_eq!(u.screen_rect(rows[2]).unwrap().y0, 50, "rows stack");
    assert_eq!(
        u.node(rows[1]).unwrap().state,
        st::STATELESS,
        "a row keeps its own state"
    );
    u.list_select(list, 0);
    u.pointer_move(20, 35);
    u.pointer_press();
    u.pointer_release();
    assert_eq!(u.node(list).unwrap().list.selected, Some(1));
    assert_eq!(u.node(rows[1]).unwrap().state, Token::of("highlight"));
    assert_eq!(u.node(rows[0]).unwrap().state, Token::of("normal"));
    assert!(u
        .take_messages()
        .iter()
        .any(|m| m.what == msg::NEW_SELECTION && m.param == 1));
}

/// A press taken by a window in front never reaches a window behind it: the one behind does not
/// hold the button afterwards, so the next press on it alone (once the first is let go) is its
/// own, heard as pressed on the topmost.
///
/// Behaviour: ui.pointer.a-press-taken-in-front-is-not-held-behind
#[test]
fn a_press_taken_in_front_is_not_held_behind() {
    let back = element(
        "Back",
        "field",
        [0, 0, 199, 199],
        10,
        vec![state("stateless", 0, vec![], vec![], vec![])],
    );
    let mut u = ui(vec![button(), back], vec![]);
    let l = u.load_layout(LAYOUT).unwrap();
    let behind = u.create_root(l, Token::of("Back")).unwrap();
    let b = u.create_root(l, Token::of("Button")).unwrap();
    assert!(
        u.node(b).unwrap().z < u.node(behind).unwrap().z,
        "the button in front"
    );
    // Clicked on the button, which lies over the field.
    u.pointer_move(20, 20);
    u.take_messages();
    u.pointer_press();
    u.pointer_release();
    let what: Vec<(Token, Token)> = u
        .take_messages()
        .iter()
        .map(|m| (m.element, m.what))
        .collect();
    assert!(what.contains(&(Token::of("Button"), msg::BUTTON_RELEASE_ON)));
    let pressed = [
        msg::MOUSE_PRESS,
        msg::MOUSE_PRESS_TOP,
        msg::MOUSE_RELEASE,
        msg::MOUSE_RELEASE_TOP,
    ];
    assert!(
        !what
            .iter()
            .any(|(e, m)| *e == Token::of("Back") && pressed.contains(m)),
        "the field behind heard no press: {what:?}"
    );
    assert!(!u
        .node(behind)
        .unwrap()
        .holds(auberean_ui::types::button_id::LEFT));
    // Pressed on the field alone: its own press.
    u.pointer_move(150, 150);
    u.take_messages();
    u.pointer_press();
    let what: Vec<(Token, Token)> = u
        .take_messages()
        .iter()
        .map(|m| (m.element, m.what))
        .collect();
    assert!(
        what.contains(&(Token::of("Back"), msg::MOUSE_PRESS_TOP)),
        "the field pressed: {what:?}"
    );
    u.pointer_release();
}

/// A turned top-level element is drawn turned, but the pointer finds it by its unturned rectangle:
/// the interface's walks test each region's rectangle and take nothing of the turn.
///
/// Behaviour: ui.pointer.a-turned-element-is-found-by-its-unturned-rectangle
#[test]
fn a_turned_element_is_found_by_its_unturned_rectangle() {
    let mut u = ui(vec![button()], vec![]);
    let l = u.load_layout(LAYOUT).unwrap();
    let b = u.create_root(l, Token::of("Button")).unwrap();
    u.set_rotation(b, 45);
    assert!(
        u.pointer_move(11, 11),
        "the rectangle's corner, off the turned picture"
    );
    assert_eq!(u.hit(11, 11), Some(b));
    assert!(
        !u.pointer_move(35, 40),
        "below the rectangle, on the turned picture"
    );
    assert_eq!(u.hit(35, 40), None);
}
