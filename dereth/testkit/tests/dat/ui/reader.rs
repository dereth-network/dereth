//! UI fixtures and scenarios for reader.

use super::*;
// =============================================================================================
// reader.book.*, notice.failure.a-refused-portal.*, hud.vitals.a-press.*
//
// Four rows, and every one of them is driven with the recorded message where a recording exists.
// =============================================================================================

/// The recorded opening of the one book the corpus carries: the twelve-page primer.
fn the_recorded_book() -> (
    dereth_client_net::client_session::SessionEvent,
    dereth_protocol::trade::WritingBookOpen,
) {
    use dereth_protocol::Message as _;
    let corpus = dereth_client_net::client_session::testing::Corpus::load("long-solo-play")
        .expect("the recordings are committed to the repository")
        .expect("long-solo-play is one of them");
    for row in &corpus.blobs {
        if row.dir != dereth_client_net::client_session::testing::Direction::ServerToClient
            || row.opcode != 0xF7B0
        {
            continue;
        }
        if row.payload.len() < 16 {
            continue;
        }
        let sub = u32::from_le_bytes(row.payload[12..16].try_into().expect("four bytes"));
        if sub == 0x00B4 {
            let e = dereth_testkit::inbound::event_of(row);
            let body = match &e {
                dereth_client_net::client_session::SessionEvent::UiEvent { blob, .. } => {
                    blob[4..].to_vec()
                }
                _ => panic!("the recorded opening is a queued event"),
            };
            let mut r = dereth_protocol::archive::Reader::new(&body);
            let decoded = dereth_protocol::trade::WritingBookOpen::read(&mut r)
                .expect("the recorded opening decodes");
            return (e, decoded);
        }
    }
    panic!("long-solo-play carries the corpus's one recorded book and it was not found");
}

/// Everything one text element holds, as a character and the colour it is drawn in.
fn drawn_glyphs(c: &mut HeadlessClient, h: ElemHandle) -> Vec<(char, u32)> {
    let ui = &mut c.app_mut().ui_mut().expect("the UI shell is up").ui;
    ui.text_element_mut(h).map_or_else(Vec::new, |t| {
        t.glyphs
            .glyphs
            .iter()
            .map(|g| (char::from_u32(u32::from(g.data)).unwrap_or('?'), g.color))
            .collect()
    })
}

pub(super) fn drawn_text(c: &mut HeadlessClient, h: ElemHandle) -> String {
    drawn_glyphs(c, h).into_iter().map(|(ch, _)| ch).collect()
}

/// The colour every letter of one phrase is drawn in; the phrase being there at all is half the
/// reading.
pub(super) fn drawn_colour(c: &mut HeadlessClient, h: ElemHandle, needle: &str) -> u32 {
    let gs = drawn_glyphs(c, h);
    let text: String = gs.iter().map(|(ch, _)| *ch).collect();
    let at = text
        .find(needle)
        .unwrap_or_else(|| panic!("{needle:?} is not in the log; the log holds {text:?}"));
    let n = needle.chars().count();
    let colours: Vec<u32> = gs[at..at + n].iter().map(|(_, col)| *col).collect();
    assert!(
        colours.windows(2).all(|w| w[0] == w[1]),
        "the phrase is drawn in one colour"
    );
    colours[0]
}

pub(super) fn hud_state(c: &HeadlessClient, h: ElemHandle) -> StateId {
    c.view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .node(h)
        .map_or(StateId(0), |n| n.state)
}

/// Whether an element is this one or lives under it.
pub(super) fn under(c: &HeadlessClient, mut h: ElemHandle, id: ElementId) -> bool {
    let ui = &c.view().expect_app().ui().expect("the UI shell is up").ui;
    loop {
        if ui.node(h).map(dereth_ui::ElementNode::element_id) == Some(id) {
            return true;
        }
        match ui.parent(h) {
            Some(p) => h = p,
            None => return false,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// reader.book.a-recorded-book-opens-the-reader-and-shows-its-first-page
// ---------------------------------------------------------------------------------------------

/// A scroll or a letter the shard sends really opens the reader and puts its page on the screen.
pub(super) fn a_recorded_book_opens_the_reader_and_shows_its_first_page() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));

    // The layout half: the reader's window is fully bound in the shipped tree.
    let bound = {
        let (_, s) = hud_gameplay(&mut c);
        s.book.fully_bound()
    };
    let win = hud_find(&c, dereth_ui_screens::panels::book::WINDOW);
    let page = hud_find(&c, dereth_ui_screens::panels::book::PAGE_TEXT);
    let title = hud_find(&c, dereth_ui_screens::panels::book::TITLE_TEXT);
    let starts_hidden = !hud_visible(&c, win);

    let (event, recorded) = the_recorded_book();
    let twelve_pages = recorded.pages.pages.len() == 12;
    let first = recorded.pages.pages[0]
        .page_text
        .clone()
        .expect("page one carries its own text");
    let the_recorded_words = first.starts_with("To chat with those around you");

    let before = c.view().expect_app().interaction().stats.books_opened;
    c.when(dereth_testkit::Inbound::event(event));
    let the_arm_ran = c.view().expect_app().interaction().stats.books_opened == before + 1;
    c.tick(1);

    let the_model_took_it = {
        let app = c.view().expect_app();
        let b = app
            .objects()
            .world
            .book
            .open
            .as_ref()
            .expect("the model took the message");
        // The number of pages is the message's own second field and not the list's length.
        b.book_id == recorded.book_id && b.max_num_pages == 12
    };
    let the_panel_opened_it = {
        let (_, s) = hud_gameplay(&mut c);
        s.book.opens == 1 && s.book.cur_page == 0 && s.book.menu_labels.len() == 12
    } && hud_visible(&c, win);

    let on_screen = drawn_text(&mut c, page);
    let the_page_is_there = on_screen == first && on_screen.contains("press the ENTER key");
    // An unsigned book takes the thing's own name, and this client has never seen the thing, so
    // the title is empty -- which is the client's own answer rather than a missing write.
    let untitled = recorded.scribe_id == dereth_primitives::ObjectId(0)
        && drawn_text(&mut c, title).is_empty();

    c.assert_behaviour(
        "reader.book.a-recorded-book-opens-the-reader-and-shows-its-first-page",
        move |_| {
            bound
                && starts_hidden
                && twelve_pages
                && the_recorded_words
                && the_arm_ran
                && the_model_took_it
                && the_panel_opened_it
                && the_page_is_there
                && untitled
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// reader.book.paging-moves-the-page-and-greys-the-control-that-cannot-be-pressed
// ---------------------------------------------------------------------------------------------

/// Turning the pages, and the two controls that say which way one can be turned.
pub(super) fn paging_a_book_moves_the_page_and_greys_what_cannot_be_pressed() {
    use dereth_ui_screens::panels::book::{STATE_NORMAL, STATE_UNAVAILABLE};

    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    let (event, recorded) = the_recorded_book();
    c.when(dereth_testkit::Inbound::event(event));
    c.tick(1);

    let prev = hud_find(&c, dereth_ui_screens::panels::book::PREV_BUTTON);
    let next = hud_find(&c, dereth_ui_screens::panels::book::NEXT_BUTTON);
    let page = hud_find(&c, dereth_ui_screens::panels::book::PAGE_TEXT);
    let on_the_first =
        hud_state(&c, prev) == STATE_UNAVAILABLE && hud_state(&c, next) == STATE_NORMAL;

    let mut hands = Hands::new();
    let at = hud_centre(&mut c, next);
    hands.click_at(&mut c, at.0, at.1);
    let want = recorded.pages.pages[1]
        .page_text
        .clone()
        .expect("page two's own text");
    let turned = {
        let (_, s) = hud_gameplay(&mut c);
        s.book.cur_page == 1
    } && drawn_text(&mut c, page) == want
        && hud_state(&c, prev) == STATE_NORMAL;

    // Off the end it stops, rather than wrapping or running past the last page.
    for _ in 0..20 {
        let at = hud_centre(&mut c, next);
        hands.click_at(&mut c, at.0, at.1);
    }
    let stops = {
        let (_, s) = hud_gameplay(&mut c);
        s.book.cur_page == 11
    } && hud_state(&c, next) == STATE_UNAVAILABLE;

    c.assert_behaviour(
        "reader.book.paging-moves-the-page-and-greys-the-control-that-cannot-be-pressed",
        move |_| on_the_first && turned && stops,
    );
    c.shutdown();
}
