//! A book read on the retail layouts: its pages and headings, turned, closed after the last.

use auberean_text::StringInfo;
use auberean_ui_game::book::{split_pages, Book, USE_BOOK};
use auberean_ui_game::gameplay::Record;

use super::stage::{said, World};

/// A book's text splits at each `<BR>` into a page; a heading between `<H>` and `</H>` goes beside
/// its page (the line after it dropped), a page without one has a lone line there; what follows
/// the last `<BR>` is no page.
///
/// Behaviour: ui.book.a-books-text-splits-into-pages-and-headings
#[test]
fn a_books_text_splits_into_pages_and_headings() {
    let (headings, pages) = split_pages("<H>One</H>\nFirst page.  <BR>Second page.<BR>tail");
    assert_eq!(headings, ["One\n", "\n"]);
    assert_eq!(pages, ["First page.\n", "Second page.\n"]);
    let (h, p) = split_pages("</H>backwards<H><BR>");
    assert_eq!(
        (h, p),
        (vec!["\n".to_owned()], vec!["</H>backwards<H>\n".to_owned()])
    );
}

/// A book the scripts open shows its title and its first page, each page a whole number of the
/// text's pages, its count at most a thousand; the slider's right arrow turns a page; a press on
/// the picture turns to the next page and, after the last, closes the book.
///
/// Behaviour: ui.book.a-book-shows-its-pages-and-turns-them
#[test]
fn a_book_shows_its_pages_and_turns_them() {
    let mut w = World::new();
    let text =
        "<H>Prologue</H>\nIt was a dark night.<BR><H>Act One</H>\nThe sun rose.<BR>The end.<BR>";
    let title = Some(Record::Text(StringInfo::literal("A Tale")));
    let body = Some(Record::Text(StringInfo::literal(text)));
    let mode = auberean_ui::Token::of("Dialog").raw();
    w.tick(&[said(
        USE_BOOK,
        vec![1, mode, 0, 0, 0],
        vec![None, None, title, body, None],
    )]);
    let book = w.el("Book");
    assert!(w.shown(book), "open");
    let help = w.el("BookHelpButton");
    assert!(!w.shown(help), "no help button on a panel made as it opens");
    assert_eq!(w.text(w.el("Panel_Title_Book")), "A Tale");
    let b = w.play.child::<Book>().expect("the book");
    let (page, last) = b.page();
    assert_eq!(page, 1);
    assert!((3..=1000).contains(&last), "{last} pages");
    w.click(w.el("BookSliderRight"));
    assert_eq!(w.play.child::<Book>().map(|b| b.page().0), Some(2));
    let text = w.el("BookClientAreaText");
    assert_eq!(
        w.ui.text_state(text).unwrap().view_y,
        w.ui.node(text).unwrap().rect.height(),
        "the second page a whole page down"
    );
    // Each press a second apart (two at once would be a double press).
    for _ in 2..last {
        w.wait(1.0);
        w.click(w.el("BookForeground"));
    }
    assert_eq!(w.play.child::<Book>().map(|b| b.page().0), Some(last));
    w.wait(1.0);
    w.click(w.el("BookForeground"));
    w.tick(&[]);
    assert!(
        w.ui.element(auberean_ui::Token::of("Book"))
            .is_none_or(|h| !w.shown(h)),
        "closed"
    );
}
