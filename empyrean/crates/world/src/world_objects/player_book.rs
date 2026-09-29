// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Player_Book.cs
//! Port of `Source/ACE.Server/WorldObjects/Player_Book.cs`: reading and writing books in the
//! player's inventory (or on the last used hook).

use empyrean_entity::ObjectGuid;

use crate::managers::player_manager::player_session;
use crate::network::game_event::events::game_event_book_add_page_response::game_event_book_add_page_response;
use crate::network::game_event::events::game_event_book_delete_page_response::game_event_book_delete_page_response;
use crate::network::game_event::events::game_event_book_modify_page_response::game_event_book_modify_page_response;
use crate::network::game_event::events::game_event_book_page_data_response::game_event_book_page_data_response;
use crate::network::game_messages::game_message::enqueue_send;
use crate::world_objects::book;
use crate::world_objects::player_inventory::{find_object, SearchLocations};
use crate::World;

/// Non-property fields declared in `Player_Book.cs`.
#[derive(Debug, Default)]
pub struct PlayerBookFields {}

/// `FindObject(new ObjectGuid(bookGuid), MyInventory | LastUsedHook, ...) as Book`.
fn find_book(w: &World, this: ObjectGuid, book_guid: u32) -> Option<ObjectGuid> {
    let found = find_object(
        w,
        this,
        ObjectGuid::new(book_guid),
        SearchLocations::MyInventory | SearchLocations::LastUsedHook,
    );
    found.result.filter(|&g| {
        w.objects
            .get(g)
            .is_some_and(crate::world_objects::world_object::WorldObject::is_book)
    })
}

fn session(w: &World, this: ObjectGuid) -> empyrean_net::SessionId {
    player_session(w, this).expect("ACE: Player.Session is null (NullReferenceException)")
}

// ACE: Player.ReadBook
pub fn read_book(w: &mut crate::World, this: empyrean_entity::ObjectGuid, book_guid: u32) {
    // This appears to have been sent in response 0x00AA (not pcapped)
    // When a book is blank, client automatically adds a "blank" page after opening, gets a GameEventBookAddPageResponse success and then sends 0x00AA, expecting this response or the page/book is not writable or usable, unless it is closed and opened again

    // TODO: Do we want to throttle this request, like appraisals?

    let Some(book) = find_book(w, this, book_guid) else {
        return;
    };

    // found book
    book::send_book_data_response(w, book, this);
}

// ACE: Player.ReadBookPage
pub fn read_book_page(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    book_guid: u32,
    page_num: i32,
) {
    // This is completely unused.

    // TODO: Do we want to throttle this request, like appraisals?

    let Some(book) = find_book(w, this, book_guid) else {
        return;
    };

    // found book
    let page = book::get_page(w.objects.get(book).expect("a found book"), page_num).cloned();

    if let Some(page) = page {
        let s = session(w, this);
        let pdr = game_event_book_page_data_response(w, s, book.full(), page_num, &page);

        enqueue_send(w, s, pdr);
    }
}

// ACE: Player.HandleActionBookAddPage
pub fn handle_action_book_add_page(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    book_guid: u32,
) {
    // find inventory book
    let Some(book) = find_book(w, this, book_guid) else {
        return;
    };

    let name = crate::dispatch::name::name(w, this).unwrap_or_default();
    let account =
        crate::world_objects::world_object_networking::shims::player_session_account(w, this);
    let o = w.objects.get_mut(book).expect("a found book");
    let ignore_author = o.ignore_author().unwrap_or(false);
    let (page, index) =
        book::add_page(o, this.full(), &name, account.as_deref(), ignore_author, "");

    if page.is_some() {
        let s = session(w, this);
        let data = w.sessions.get_mut(s).expect("the player's session");
        let msg = game_event_book_add_page_response(data, book_guid, index, true);
        enqueue_send(w, s, msg);
    }
}

// ACE: Player.HandleActionBookModifyPage
pub fn handle_action_book_modify_page(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    book_guid: u32,
    page_id: i32,
    page_text: &str,
) {
    // find inventory book
    let Some(book) = find_book(w, this, book_guid) else {
        return;
    };

    // Not ACE's (fix, V342): the response reports whether the page was
    // changed, so a refused edit (not the page's author, or the text unchanged) is answered as
    // refused. ACE discarded the result and always reported success.
    let success = book::modify_page(w, book, page_id, page_text, this);

    let s = session(w, this);
    let data = w.sessions.get_mut(s).expect("the player's session");
    let msg = game_event_book_modify_page_response(data, book_guid, page_id, success);
    enqueue_send(w, s, msg);
}

// ACE: Player.HandleActionBookDeletePage
pub fn handle_action_book_delete_page(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    book_guid: u32,
    page_id: i32,
) {
    // find inventory book
    let Some(book) = find_book(w, this, book_guid) else {
        return;
    };

    let success = book::delete_page(w, book, page_id, this);

    let s = session(w, this);
    let data = w.sessions.get_mut(s).expect("the player's session");
    let msg = game_event_book_delete_page_response(data, book_guid, page_id, success);
    enqueue_send(w, s, msg);
}
