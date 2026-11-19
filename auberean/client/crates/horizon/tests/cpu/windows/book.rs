//! The book window: a book the game opens, split into its pages, turned and closed, and copied.
//!
//! Behaviour: none (this client's own interface, not a behaviour of the retail client)

use auberean_client_sdk::contract::notice::{BookNotice, Notice, Said};
use auberean_client_sdk::contract::text::TextRef;
use auberean_horizon::journal::Book;
use auberean_horizon::state::GameState;
use auberean_horizon::ui::book::{copied, split_pages, BookWindow};

use crate::centre;
use crate::windows::journal::{follow, view};
use crate::Harness;

fn opened(text: &str, controls: bool) -> Notice {
    Notice {
        about: None,
        said: Said::Book(BookNotice::Open {
            controls,
            look: 0,
            title: Some(TextRef::literal("A Tale")),
            text: Some(TextRef::literal(text)),
            picture: 0,
        }),
    }
}

#[test]
fn a_books_text_splits_at_its_page_breaks_each_page_with_its_heading() {
    let pages = split_pages("<H>One</H>\nFirst page.\n<BR>Second page.  \n<BR>left over");
    assert_eq!(
        pages,
        vec![
            (Some("One".to_owned()), "First page.".to_owned()),
            (None, "Second page.".to_owned()),
        ],
        "what follows the last break is not a page"
    );
    assert_eq!(
        split_pages("A sign."),
        vec![(None, "A sign.".to_owned())],
        "no break: one page"
    );
    assert!(split_pages("  ").is_empty());
    let book = Book {
        title: "A Tale".into(),
        text: "<H>One</H>\nFirst.<BR>Second.<BR>".into(),
        ..Book::default()
    };
    assert_eq!(copied(&book), "A Tale\n\nOne\nFirst.\n\n\nSecond.");
}

#[test]
fn a_book_the_game_opens_shows_its_first_page_turns_with_a_press_and_closes_after_the_last() {
    let mut s = GameState::default();
    let vw = view(Vec::new(), 1);
    follow(&mut s, &vw, &[opened("Page one.<BR>Page two.<BR>", true)]);
    let mut h = Harness::with_built_in_art();
    h.frame(&s);
    assert!(h.ui.book.is_open(), "opened by the game");
    assert_eq!(h.ui.book.leaves.len(), 2);
    assert_eq!(h.ui.book.leaves[0].lines, vec!["Page one.".to_owned()]);
    let parts = BookWindow::parts(h.ui.book.body().expect("drawn"), h.k());
    follow(&mut s, &vw, &[]);
    h.click(&s, centre(parts.on));
    assert_eq!(h.ui.book.page, 1, "the arrow turns on");
    h.click(&s, centre(parts.back));
    assert_eq!(h.ui.book.page, 0, "and back");
    h.click(&s, centre(parts.page));
    assert_eq!(h.ui.book.page, 1, "a press on the page turns it");
    h.click(&s, centre(parts.copy));
    assert_eq!(
        h.input.copied.as_deref(),
        Some("A Tale\n\nPage one.\n\n\nPage two.")
    );
    h.click(&s, centre(parts.page));
    assert!(!h.ui.book.is_open(), "closed after the last page");
    h.frame(&s);
    assert!(!h.ui.book.is_open(), "the same book stays closed");

    // Opened again, it opens at its first page; without controls no arrows.
    follow(&mut s, &vw, &[opened("Only page.<BR>", false)]);
    h.frame(&s);
    assert!(h.ui.book.is_open());
    assert_eq!(h.ui.book.page, 0);
    follow(&mut s, &vw, &[]);
    h.input.copied = None;
    h.click(&s, centre(parts.copy));
    assert!(h.input.copied.is_none(), "no Copy without the controls");
}

#[test]
fn a_long_page_runs_over_as_many_of_the_windows_pages_as_it_needs() {
    let mut s = GameState::default();
    let vw = view(Vec::new(), 1);
    let long: String = (0..200).map(|i| format!("Line {i}\n")).collect();
    follow(
        &mut s,
        &vw,
        &[opened(&format!("<H>Long</H>\n{long}<BR>"), true)],
    );
    let mut h = Harness::with_built_in_art();
    h.frame(&s);
    let leaves = &h.ui.book.leaves;
    assert!(leaves.len() > 1, "{} pages", leaves.len());
    assert_eq!(leaves[0].heading.as_deref(), Some("Long"));
    assert!(
        leaves[1..].iter().all(|l| l.heading.is_none()),
        "the heading on its first"
    );
    let lines: usize = leaves.iter().map(|l| l.lines.len()).sum();
    assert!(lines >= 200, "every line kept: {lines}");
}

/// A book the game opens while the book window is open behind another window brings it to the
/// front with the new book.
#[test]
fn a_book_opened_while_the_book_window_is_behind_another_comes_to_the_front() {
    use auberean_horizon::keys::Keyed;
    use auberean_horizon::ui::stack::Window;
    let mut s = crate::in_world();
    let vw = view(Vec::new(), 1);
    follow(&mut s, &vw, &[opened("A first page.", true)]);
    let mut h = Harness::with_built_in_art();
    h.frame(&s);
    h.frame(&s);
    h.ui.keyed(Keyed::Skills, &s);
    h.frame(&s);
    h.frame(&s);
    let front = |h: &Harness| h.ui.stack.order().last().copied();
    assert_eq!(
        front(&h),
        Some(Window::Skills),
        "the skill window opened last"
    );
    follow(&mut s, &vw, &[opened("Another book.", true)]);
    h.frame(&s);
    assert!(h.ui.book.is_open());
    assert_eq!(front(&h), Some(Window::Book), "the new book in front");
}

#[test]
fn the_copy_button_fits_its_words() {
    let h = Harness::with_built_in_art();
    let k = h.k();
    let body = auberean_horizon::draw::Rect::new(0.0, 0.0, 600.0 * k, 500.0 * k);
    let copy = BookWindow::parts(body, k).copy;
    crate::measured(&h, |p| {
        assert!(dereth_ui_kit::widgets::button_label_width(p, "Copy Text") <= copy.w - 12.0 * k);
    });
}
