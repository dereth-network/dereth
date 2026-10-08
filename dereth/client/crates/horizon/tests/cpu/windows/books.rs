//! The book window writes as the game's book session lets it: the page it is turned to takes what
//! is typed, the buttons turn the book and add a page, a page of the player's own can be deleted,
//! and closing the window closes the book in the game.
//!
//! Behaviour: none (experimental Horizon interface)

use dereth_client_contract::book::{BookAction, BookSessionView};
use dereth_client_contract::view::{BookPageView, BookView};
use dereth_client_contract::UiRequest;
use dereth_horizon::draw::Rect;
use dereth_horizon::ui::game::GameState;
use dereth_horizon::ui::input::vk;
use dereth_horizon::ui::panels::WindowId;
use dereth_primitives::ObjectId;

use crate::Harness;

const BOOK: ObjectId = ObjectId(0x8000_0B00);
const ME: ObjectId = ObjectId(0x5000_0001);

fn reading(editable: bool) -> GameState {
    GameState {
        in_world: true,
        name: "Tester".into(),
        book: Some(BookView {
            book_id: BOOK,
            player_id: ME,
            max_num_pages: 10,
            pages: vec![BookPageView {
                author_id: ME,
                author_name: "Tester".into(),
                text: Some("Hail".into()),
                ..BookPageView::default()
            }],
            title: "A Journal".into(),
            opening: 1,
            ..BookView::default()
        }),
        book_session: BookSessionView {
            current_page: 0,
            draft: "Hail".into(),
            editable,
            ..BookSessionView::default()
        },
        ..GameState::default()
    }
}

/// The book window's body, as it opens at a layout scale of one.
fn body() -> Rect {
    let ((w, h), (x, y)) = WindowId::Book.geometry();
    Rect::new(x + 12.0, y + 46.0, w - 24.0, h - 56.0)
}

fn click(h: &mut Harness, state: &GameState, at: (f32, f32)) -> Vec<UiRequest> {
    h.move_to(at.0, at.1);
    h.press();
    let mut out = h.frame(state).requests;
    h.release();
    out.extend(h.frame(state).requests);
    out
}

fn opened(state: &GameState) -> Harness {
    let mut h = Harness::new(Default::default());
    h.frame(state);
    assert!(h.ui.windows.is_open(WindowId::Book));
    h
}

#[test]
fn a_click_on_a_writable_page_takes_the_typing_and_each_edit_goes_to_the_game() {
    let state = reading(true);
    let mut h = opened(&state);
    let b = body();
    click(&mut h, &state, (b.x + 100.0, b.y + 100.0));
    h.input.chars = vec!['!'];
    let out = h.frame(&state).requests;
    assert_eq!(
        out,
        [UiRequest::Book(BookAction::Edit {
            book: BOOK,
            page: 0,
            text: "Hail!".into(),
        })]
    );
}

#[test]
fn a_page_the_game_does_not_let_the_player_write_on_takes_no_typing() {
    let state = reading(false);
    let mut h = opened(&state);
    let b = body();
    click(&mut h, &state, (b.x + 100.0, b.y + 100.0));
    h.input.chars = vec!['!'];
    h.input.keys = vec![vk::BACK];
    let out = h.frame(&state).requests;
    assert!(out.is_empty(), "{out:?}");
}

#[test]
fn next_turns_the_page_and_new_page_turns_past_the_last_which_adds_one() {
    let state = reading(true);
    let mut h = opened(&state);
    let b = body();
    let foot = b.bottom() - 32.0 + 14.0;
    let next = click(&mut h, &state, (b.right() - 8.0 - 46.0, foot));
    assert!(next.contains(&UiRequest::Book(BookAction::Turn {
        book: BOOK,
        page: 1
    })));
    let new = click(&mut h, &state, (b.x + 16.0 + 92.0 + 46.0, foot));
    assert!(new.contains(&UiRequest::Book(BookAction::Turn {
        book: BOOK,
        page: 1
    })));
}

#[test]
fn deleting_a_page_of_the_player_s_own_blanks_it_and_saves_it() {
    let state = reading(true);
    let mut h = opened(&state);
    let b = body();
    let foot = b.bottom() - 32.0 + 14.0;
    let out = click(&mut h, &state, (b.right() - 24.0 - 2.0 * 92.0 + 46.0, foot));
    assert_eq!(
        out,
        [
            UiRequest::Book(BookAction::Edit {
                book: BOOK,
                page: 0,
                text: String::new(),
            }),
            UiRequest::Book(BookAction::Flush { book: BOOK }),
        ]
    );
}

#[test]
fn closing_the_window_closes_the_book_in_the_game() {
    let state = reading(true);
    let mut h = opened(&state);
    h.ui.windows.close(WindowId::Book);
    let out = h.frame(&state);
    assert!(out
        .requests
        .contains(&UiRequest::Book(BookAction::Close { book: BOOK })));
    assert!(out.book_closed);
}
