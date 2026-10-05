//! The book's page replies: an add-page response (`0x00B6`) inserts a page and rebuilds the page
//! strip; an add-page refusal and a page mismatch both refetch instead of inserting; and a
//! delete-page response (`0x00B7`) is counted and changes nothing.
//! Fixture: `net::late_receivers`' socket-free replay App with `long-solo-play`'s recorded `0x00B4`
//! book open.

#![cfg(gpu)]

use crate::common::gpu_lock;
use crate::net::late_receivers::{gameplay, recorded, settle, setup, Peer, PLAYER};

use dereth_client::app::App;
use dereth_client_runtime::dropped;
use dereth_primitives::ObjectId;
use dereth_protocol::{Message, Opcode};

/// Open `long-solo-play`'s book — the one `0x00B4 Writing_BookOpen` in the whole corpus, the Academy
/// Wordsmith's twelve-page chat primer — and return its id.
fn open_the_recorded_book(app: &mut App, peer: &mut Peer) -> ObjectId {
    let blob = recorded("long-solo-play", Opcode::WRITING_BOOK_OPEN)
        .pop()
        .expect("long-solo-play carries exactly one 0x00B4");
    let mut r = dereth_protocol::archive::Reader::new(&blob[16..]);
    let m =
        dereth_protocol::trade::WritingBookOpen::read(&mut r).expect("the recorded body decodes");
    peer.replay(app, blob);
    settle(app);
    assert_eq!(
        app.objects().world.book.opening,
        1,
        "the recorded 0x00B4 must open a book, or the 0x00B6 stations below measure the id guard"
    );
    assert_eq!(
        gameplay(app).book.cur_page,
        0,
        "opening the recorded book selects page 0"
    );
    m.book_id
}

/// Behaviour: panels.book.next-adds-a-page-and-the-reply-makes-it-editable
///
/// **`0x00B6 Writing_BookAddPageResponse` puts a page in the open book's list.**
///
/// The response notice reaches the book panel. The panel inserts a
/// `PageData` at `page`, authors it with the local player's ID and name, rebuilds the page menu,
/// and draws the result.
///
/// **Falsified by** making the arm's opcode unreachable: `pages_added` stays 0 and the strip keeps
/// its twelve rows.
#[test]
fn an_add_page_response_inserts_a_page_and_rebuilds_the_strip() {
    let _gpu = gpu_lock();
    let (mut app, mut peer) = setup("book-add");
    let book = open_the_recorded_book(&mut app, &mut peer);

    let (pages_before, rows_before) = {
        let b = &gameplay(&mut app).book;
        (b.pages.len(), b.menu_labels.len())
    };
    assert!(
        pages_before >= 2,
        "the recorded book has pages to insert into: {pages_before}"
    );

    peer.event(
        &mut app,
        &dereth_protocol::trade::WritingBookAddPageResponse {
            book_id: book,
            page_number: 0,
            success: 1,
        },
    );
    settle(&mut app);

    let s = &app.interaction().stats;
    assert_eq!(s.book_add_page_responses, 1, "one message consumed");
    assert_eq!(
        s.book_add_pages_relayed, 1,
        "and it got past the two id refusals"
    );
    assert_eq!(app.objects().world.book.add_page_ignored, 0);

    let b = &gameplay(&mut app).book;
    assert_eq!(
        b.pages_added, 1,
        "the successful add-page response inserts one page"
    );
    assert_eq!(
        b.pages.len(),
        pages_before + 1,
        "the panel's own page list grew"
    );
    assert_eq!(
        b.menu_labels.len(),
        rows_before + 1,
        "insertion rebuilds the panel's menu strip"
    );
    assert_eq!(
        b.pages[0].author_id, PLAYER,
        "the inserted page uses the current player ID as author"
    );
    assert_eq!(
        b.pages[0].author_name, "Larktest",
        "the inserted page resolves the current player's NAME quality as author name"
    );
    assert!(
        !b.request_pending,
        "the response clears request-pending state as its final update"
    );
    assert_eq!(
        b.book_data_refetches, 0,
        "the insert arm, not either refetch arm"
    );
    assert!(!dropped::unreceived(Opcode::WRITING_BOOK_ADD_PAGE_RESPONSE));
}

/// **The two arms that do not insert, and they are the reading a plainer transcription loses.**
///
/// Both `success == 0` and a page other than the current page ask for the *whole book* again rather
/// than inserting at the page in the response. The page-mismatch arm **moves the current page
/// first**; the refusal arm
/// leaves it unchanged.
///
/// **Falsified by** dropping the `page != self.cur_page` test from
/// `BookPanel::add_page_response`: page 5 is then inserted and `pages_added` reads 2.
#[test]
fn an_add_page_refusal_and_a_page_mismatch_both_refetch_instead_of_inserting() {
    let _gpu = gpu_lock();
    let (mut app, mut peer) = setup("book-refetch");
    let book = open_the_recorded_book(&mut app, &mut peer);
    let pages_before = gameplay(&mut app).book.pages.len();

    // `success == 0`.
    peer.event(
        &mut app,
        &dereth_protocol::trade::WritingBookAddPageResponse {
            book_id: book,
            page_number: 0,
            success: 0,
        },
    );
    settle(&mut app);
    {
        let b = &gameplay(&mut app).book;
        assert_eq!(b.pages_added, 0, "the server refused the page");
        assert_eq!(
            b.book_data_refetches, 1,
            "a refused add-page response requests one whole-book refetch"
        );
        assert_eq!(
            b.cur_page, 0,
            "and the refusal arm does NOT move the current page"
        );
        assert_eq!(b.pages.len(), pages_before);
    }

    // A page other than the current page.
    peer.event(
        &mut app,
        &dereth_protocol::trade::WritingBookAddPageResponse {
            book_id: book,
            page_number: 5,
            success: 1,
        },
    );
    settle(&mut app);
    {
        let b = &gameplay(&mut app).book;
        assert_eq!(b.pages_added, 0, "still nothing inserted");
        assert_eq!(b.book_data_refetches, 2);
        assert_eq!(
            b.cur_page, 5,
            "a page mismatch moves the current page to 5 before refetch"
        );
        assert_eq!(b.pages.len(), pages_before);
    }

    // The model's own two refusals: another object's book, and a book that is not open.
    peer.event(
        &mut app,
        &dereth_protocol::trade::WritingBookAddPageResponse {
            book_id: ObjectId(0xDEAD_BEEF),
            page_number: 5,
            success: 1,
        },
    );
    settle(&mut app);
    assert_eq!(
        app.objects().world.book.add_page_ignored,
        1,
        "a response for another book ID is ignored without inserting or refetching"
    );
    assert_eq!(
        app.interaction().stats.book_add_pages_relayed,
        2,
        "the two before it did get past the id guard -- this one did not"
    );
    assert_eq!(
        gameplay(&mut app).book.book_data_refetches,
        2,
        "and raises no refetch either"
    );
}

/// **`0x00B7 Writing_BookDeletePageResponse` is received and does nothing, and that is retail.**
///
/// The delete response notice goes to every game-side notice receiver, and every one of them has
/// only the empty default handler for it. The add-page notice beside it is live in exactly one
/// receiver, the book panel.
///
/// **Falsified by** removing the arm: `dropped::unreceived` goes true again.
#[test]
fn a_delete_page_response_is_counted_and_changes_nothing() {
    let _gpu = gpu_lock();
    let (mut app, mut peer) = setup("book-delete");
    let book = open_the_recorded_book(&mut app, &mut peer);
    let before = {
        let b = &gameplay(&mut app).book;
        (
            b.pages.len(),
            b.menu_labels.len(),
            b.cur_page,
            b.pages_added,
            b.book_data_refetches,
        )
    };

    peer.event(
        &mut app,
        &dereth_protocol::trade::WritingBookDeletePageResponse {
            book_id: book,
            page_number: 0,
            success: 1,
        },
    );
    settle(&mut app);

    assert_eq!(
        app.interaction().stats.book_delete_page_responses,
        1,
        "consumed"
    );
    assert_eq!(app.objects().world.book.delete_page_responses, 1);
    let after = {
        let b = &gameplay(&mut app).book;
        (
            b.pages.len(),
            b.menu_labels.len(),
            b.cur_page,
            b.pages_added,
            b.book_data_refetches,
        )
    };
    assert_eq!(
        after, before,
        "the delete response's no-op receiver leaves the panel tuple unchanged"
    );
    assert!(!dropped::unreceived(
        Opcode::WRITING_BOOK_DELETE_PAGE_RESPONSE
    ));
}
