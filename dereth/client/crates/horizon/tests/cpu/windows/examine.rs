//! The examine window follows the game: an examine opens it, a new selection while it is open is
//! examined in turn, and clearing the selection closes it.
//!
//! Behaviour: none (this client's own interface, not a behaviour of the retail client)

use dereth_client_contract::UiRequest;
use dereth_horizon::ui::game::{GameState, Relation, Target};
use dereth_horizon::ui::panels::WindowId;
use dereth_primitives::ObjectId;

use crate::Harness;

fn selecting(id: u32) -> GameState {
    GameState {
        in_world: true,
        name: "Tester".into(),
        target: Some(Target {
            id: ObjectId(id),
            name: "Drudge".into(),
            relation: Relation::Hostile,
            health: None,
            level: None,
            icon: None,
            look: None,
        }),
        ..GameState::default()
    }
}

#[test]
fn an_examine_opens_the_window_and_a_new_selection_is_examined_in_turn() {
    let mut h = Harness::new(Default::default());
    let mut state = selecting(1);
    state.examine_opened = true;
    h.frame(&state);
    assert!(h.ui.windows.is_open(WindowId::Examine));
    state.examine_opened = false;
    h.frame(&state);
    let next = selecting(2);
    let out = h.frame(&next);
    assert!(
        out.requests.contains(&UiRequest::Examine(ObjectId(2))),
        "{:?}",
        out.requests
    );
}

#[test]
fn clearing_the_selection_closes_the_window() {
    let mut h = Harness::new(Default::default());
    let mut state = selecting(1);
    state.examine_opened = true;
    h.frame(&state);
    let none = GameState {
        in_world: true,
        name: "Tester".into(),
        ..GameState::default()
    };
    h.frame(&none);
    assert!(!h.ui.windows.is_open(WindowId::Examine));
}

#[test]
fn the_examine_key_again_closes_the_window() {
    let mut h = Harness::new(Default::default());
    let mut state = selecting(1);
    state.examine_opened = true;
    h.frame(&state);
    state.examine_opened = false;
    state.examine_closed = true;
    h.frame(&state);
    assert!(!h.ui.windows.is_open(WindowId::Examine));
}

#[test]
fn an_inscription_takes_several_lines_as_typed_and_is_sent_when_the_box_lets_go() {
    use dereth_horizon::ui::game::{ExaminePane, Examined};
    use dereth_horizon::ui::input::vk;
    let mut h = Harness::new(Default::default());
    let mut state = selecting(0x8000_0001);
    state.examine_opened = true;
    state.examined = Some(Examined {
        id: ObjectId(0x8000_0001),
        pane: ExaminePane::Item,
        title: "Sword".into(),
        inscription_editable: true,
        ..Examined::default()
    });
    h.frame(&state);
    state.examine_opened = false;
    let w =
        h.ui.windows
            .rects(1.0)
            .into_iter()
            .find(|(id, _)| *id == WindowId::Examine)
            .map(|(_, r)| r)
            .expect("the examine window is open");
    // A press in the box gives it the keyboard.
    h.move_to(w.x + 60.0, w.bottom() - 80.0);
    h.press();
    h.frame(&state);
    h.release();
    h.frame(&state);
    // Two lines of a drawing, the second starting with spaces.
    h.input.chars = vec!['/', '\\'];
    h.frame(&state);
    h.input.keys.push(vk::ENTER);
    h.frame(&state);
    h.input.chars = vec![' ', '|'];
    let typing = h.frame(&state).requests;
    assert!(
        !typing
            .iter()
            .any(|r| matches!(r, UiRequest::SetInscription { .. })),
        "Enter starts a line and sends nothing"
    );
    // A press elsewhere lets the box go, and the lines are sent as typed.
    h.move_to(w.x + 60.0, w.y + 60.0);
    h.press();
    let sent = h.frame(&state).requests;
    assert!(
        sent.contains(&UiRequest::SetInscription {
            object: ObjectId(0x8000_0001),
            text: "/\\\n |".into()
        }),
        "{sent:?}"
    );
    // The box keeps what was committed, before the server answers: in and out again unchanged
    // sends nothing.
    h.release();
    h.frame(&state);
    h.move_to(w.x + 60.0, w.bottom() - 80.0);
    h.press();
    h.frame(&state);
    h.release();
    h.frame(&state);
    h.move_to(w.x + 60.0, w.y + 60.0);
    h.press();
    let again = h.frame(&state).requests;
    assert!(
        !again
            .iter()
            .any(|r| matches!(r, UiRequest::SetInscription { .. })),
        "{again:?}"
    );
}
