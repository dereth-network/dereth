//! The Journal's notebook is the game's shared notebook: showing it tells the game, which loads
//! it, and its buttons and fields change it through the game.
//!
//! Behaviour: none (experimental Horizon interface)

use dereth_client_contract::journal::{JournalAction, JournalPage, JournalView};
use dereth_client_contract::UiRequest;
use dereth_horizon::draw::Rect;
use dereth_horizon::ui::game::GameState;
use dereth_horizon::ui::panels::WindowId;

use crate::Harness;

fn with_notebook() -> GameState {
    let page = JournalPage {
        label: "Camp".into(),
        title: "Drudges".into(),
        page_number: 1,
        ..JournalPage::default()
    };
    GameState {
        in_world: true,
        name: "Tester".into(),
        notebook: true,
        journal: JournalView {
            pages: vec![page.clone()],
            draft: page,
            current_page: 1,
            loaded: true,
            ..JournalView::default()
        },
        ..GameState::default()
    }
}

/// The Journal window's body under its tabs, at a layout scale of one.
fn body() -> Rect {
    let ((w, h), (x, y)) = WindowId::Journal.geometry();
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

#[test]
fn the_notebook_tab_tells_the_game_it_is_shown_and_closing_the_window_that_it_is_not() {
    let state = with_notebook();
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    h.ui.windows.open(WindowId::Journal, 0.0);
    h.frame(&state);
    let b = body();
    // The second tab, at the tabs' least width with no fonts.
    let shown = click(&mut h, &state, (b.x + 105.0, b.y + 13.0));
    assert!(shown.contains(&UiRequest::Journal(JournalAction::Visibility(true))));
    // New Page, the fifth of the six buttons under the open page.
    let page = Rect::new(b.x, b.y + 34.0, b.w, b.h - 34.0);
    let x = page.x + page.w * 0.36 + 12.0;
    let bw = (page.right() - x - 20.0) / 6.0;
    let new = click(
        &mut h,
        &state,
        (x + (bw + 4.0) * 4.0 + bw / 2.0, page.bottom() - 16.0),
    );
    assert!(
        new.contains(&UiRequest::Journal(JournalAction::NewPage)),
        "{new:?}"
    );
    h.ui.windows.close(WindowId::Journal);
    let closed = h.frame(&state).requests;
    assert!(closed.contains(&UiRequest::Journal(JournalAction::Visibility(false))));
}
