//! An opposite-parity pair of set-panel-visibility notices for one page settles (no live-lock),
//! with the page left up when the show is last.
//! Fixture: shipped layouts, strings and keymaps loaded from the retail DATs.

use crate::common::layout::RegistrationOrder;

use dereth_ui::{Delivery, ElemHandle, ElementId, Screen, UiSystem};
use dereth_ui_screens::screens::gameplay::GamePlayScreen;

/// `<BOOK>`, the Book panel's page of the toolbar panel stack.
const BOOK: ElementId = ElementId(0x1000_0182);

/// How many `UiFlow::deliver` passes a single visibility edge may cost before it is a live-lock.
const DRAINS: usize = 16;

fn env() -> UiSystem {
    let (ui, _flow, _store) =
        crate::common::layout::load((800, 600), RegistrationOrder::BeforeResolver);
    ui
}

/// `UiFlow::deliver`, bounded. Returns the number of passes it took to empty the outbox, or
/// `None` when [`DRAINS`] passes were not enough — which is the live-lock, reported rather than
/// entered.
fn pump(ui: &mut UiSystem, s: &mut GamePlayScreen) -> Option<usize> {
    for pass in 0..DRAINS {
        let batch = ui.drain_outbox();
        if batch.is_empty() {
            return Some(pass);
        }
        for d in batch {
            if let Delivery::Element { msg, .. } = d {
                s.on_element_message(&mut dereth_ui::framework::ScreenCx::new(ui), &msg);
            }
        }
    }
    ui.drain_outbox().is_empty().then_some(DRAINS)
}

/// The shipped tree, pumped to quiescence — `setup_children`'s hide loop included.
fn screen() -> (UiSystem, GamePlayScreen) {
    let mut ui = env();
    let mut s = GamePlayScreen::default();
    s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("the gameplay screen builds from the shipped layout");
    assert!(
        pump(&mut ui, &mut s).is_some(),
        "screen construction itself must settle"
    );
    ui.requests.clear();
    (ui, s)
}

fn page(ui: &UiSystem, s: &GamePlayScreen) -> ElemHandle {
    let root = s.root().expect("the gameplay root");
    ui.get_child_recursive(root, BOOK)
        .expect("<BOOK> 0x10000182 is in the shipped layout")
}

fn visible(ui: &UiSystem, h: ElemHandle) -> bool {
    ui.node(h).expect("alive").region.flags.visible
}

/// Behaviour: panels.stack.opposite-visibility-notices-for-one-page-settle-on-the-last
/// Two opposite-parity `0x18`s for one page, queued together, are what the client actually
/// produces when a page is hidden by `setup_children` and opened in the same frame. The handler
/// must reach a fixed point, and the fixed point must agree with the page's real visibility.
#[test]
fn an_opposite_parity_pair_of_visibility_notices_for_one_page_settles() {
    let (mut ui, mut s) = screen();
    let book = page(&ui, &s);
    assert!(
        !visible(&ui, book),
        "setup_children leaves every registered page hidden"
    );
    assert!(
        s.panels
            .pages
            .iter()
            .any(|p| p.element == BOOK && p.panel_id != 0),
        "<BOOK> is one of the panel controller's registered pages and carries a nonzero 0x10000029"
    );

    // Both writes before a single drain, so both notices are in the outbox at once. This is
    // child initialization's hide followed by opening the book's unconditional show, in the frame
    // order the client produces them.
    ui.set_visible(book, true);
    ui.set_visible(book, false);

    let passes = pump(&mut ui, &mut s).unwrap_or_else(|| {
        panic!(
            "the panel controller's 0x18 arm never reached a fixed point: still delivering after {DRAINS} \
             drains. The handler is reading the stamped message parameter instead of \
             the child's own visibility, so a stale notice writes the page's visibility back and queues its \
             own replacement -- UiShell::deliver_pending would spin here for ever."
        )
    });
    assert!(passes <= DRAINS, "settled in {passes} drains");

    // The fixed point is the truth, not merely quiet: the stack's current page and the page's own
    // flag are the two halves of "the controller is visible iff a page is currently shown".
    assert!(
        !visible(&ui, book),
        "the last write wins; the notice pair does not resurrect the page"
    );
    assert_ne!(
        s.panels.current,
        Some(BOOK),
        "a page that is down cannot be the currently shown panel"
    );
}

/// The same pair the other way round — hidden last is not the only order a frame can produce, and
/// the fixed point must follow the final write rather than the final notice.
#[test]
fn the_pair_settles_with_the_page_left_up_when_the_show_is_last() {
    let (mut ui, mut s) = screen();
    let book = page(&ui, &s);
    ui.set_visible(book, true);
    ui.set_visible(book, false);
    ui.set_visible(book, true);

    let passes = pump(&mut ui, &mut s).unwrap_or_else(|| {
        panic!("the panel controller's 0x18 arm never reached a fixed point after {DRAINS} drains")
    });
    assert!(passes <= DRAINS, "settled in {passes} drains");
    assert!(visible(&ui, book), "the last write wins");
    assert_eq!(
        s.panels.current,
        Some(BOOK),
        "the page that is up is the currently shown panel"
    );
}
