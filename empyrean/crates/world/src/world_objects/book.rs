// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Book.cs
//! Port of `Source/ACE.Server/WorldObjects/Book.cs`: books, notes and parchments with pages.

use empyrean_entity::enums::{PropertyInt, PropertyString};
use empyrean_entity::models::properties_book_page_data::PropertiesBookPageData;
use empyrean_entity::models::properties_book_page_data_extensions as pages;
use empyrean_entity::ObjectGuid;

use crate::world_objects::world_object::WorldObject;
use crate::World;

/// Non-property fields declared in `Book.cs`.
#[derive(Debug, Default)]
pub struct BookFields {}

/// `Biota.PropertiesBook` (created by the constructor, so never null for a Book).
fn properties_book(o: &WorldObject) -> &empyrean_entity::models::properties_book::PropertiesBook {
    o.biota
        .properties_book
        .as_ref()
        .expect("ACE: Biota.PropertiesBook is null (NullReferenceException)")
}

// ACE: Book.SetProperties
/// Sets each non-empty value.
pub fn set_properties(
    o: &mut WorldObject,
    name: &str,
    short_desc: &str,
    inscription: &str,
    scribe_name: &str,
    scribe_account: &str,
) {
    if !name.is_empty() {
        o.set_property(PropertyString::Name, name.to_owned());
    }
    if !short_desc.is_empty() {
        o.set_property(PropertyString::ShortDesc, short_desc.to_owned());
    }
    if !inscription.is_empty() {
        o.set_property(PropertyString::Inscription, inscription.to_owned());
    }
    if !scribe_name.is_empty() {
        o.set_property(PropertyString::ScribeName, scribe_name.to_owned());
    }
    if !scribe_account.is_empty() {
        o.set_property(PropertyString::ScribeAccount, scribe_account.to_owned());
    }
}

// ---- virtual-dispatch targets ----

/// This is raised by Player.HandleActionUseItem. The item does not exist in the players
/// possession. If the item was outside of range, the player will have been commanded to move
/// using DoMoveTo before ActOnUse is called. When this is called, it should be assumed that the
/// player is within range.
// ACE: Book.ActOnUse
pub fn book_act_on_use(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    activator: empyrean_entity::ObjectGuid,
) {
    if !w.objects.get(activator).is_some_and(WorldObject::is_player) {
        return;
    }
    let player = activator;

    send_book_data_response(w, this, player);
}

/// `new GameEventBookDataResponse(player.Session, Guid.Full, maxChars, maxPages, pages, inscription,
/// authorID, authorName, ignoreAuthor)`, sent: the body `Book.ActOnUse` and `Player.ReadBook` share.
pub(crate) fn send_book_data_response(w: &mut World, book: ObjectGuid, player: ObjectGuid) {
    let o = w.objects.get(book).expect("ACE: book is null");
    let max_chars = properties_book(o).max_num_chars_per_page;
    let max_pages = properties_book(o).max_num_pages;

    let author_name = o.scribe_name().unwrap_or_default();

    //uint authorID = ScribeIID ?? 0xFFFFFFFF;
    let author_id = o.scribe_iid().unwrap_or(0xFFFF_FFFF);

    let pages = pages::clone(o.biota.properties_book_page_data.as_ref()).unwrap_or_default();

    let ignore_author = o.ignore_author().unwrap_or(false);

    let inscription = o.inscription().unwrap_or_default();

    let session = crate::managers::player_manager::player_session(w, player)
        .expect("ACE: player.Session is null (NullReferenceException)");
    let book_data_response = crate::network::game_event::events::game_event_book_data_response::game_event_book_data_response(
        w,
        session,
        book.full(),
        max_chars,
        max_pages,
        &pages,
        Some(&inscription),
        author_id,
        Some(&author_name),
        ignore_author,
    );
    crate::network::game_messages::game_message::enqueue_send(w, session, book_data_response);
}

// ACE: Book.AddPage
/// Appends a page; `None` and index -1 (ACE's `out int index`) when the book is full.
pub fn add_page(
    o: &mut WorldObject,
    author_id: u32,
    author_name: &str,
    author_account: Option<&str>,
    ignore_author: bool,
    page_text: &str,
) -> (Option<PropertiesBookPageData>, i32) {
    if pages::get_page_count(o.biota.properties_book_page_data.as_ref())
        >= properties_book(o).max_num_pages
    {
        return (None, -1);
    }

    let page = PropertiesBookPageData {
        author_id,
        author_name: Some(author_name.to_owned()),
        author_account: author_account.map(str::to_owned),
        ignore_author,
        page_text: Some(page_text.to_owned()),
    };

    let list = o
        .biota
        .properties_book_page_data
        .as_mut()
        .expect("ACE: Biota.PropertiesBookPageData is null (NullReferenceException)");
    let index = pages::add_page(list, page.clone());
    o.wo.world_object_database.changes_detected = true;

    let new_page_count = pages::get_page_count(o.biota.properties_book_page_data.as_ref());
    o.set_property(PropertyInt::AppraisalPages, new_page_count);

    (Some(page), index)
}

// ACE: Book.ModifyPage
/// Rewrites a page the player may edit (its author, or anyone for an `IgnoreAuthor` page, or
/// staff); false for a missing page or unchanged text.
pub fn modify_page(
    w: &mut World,
    this: ObjectGuid,
    index: i32,
    page_text: &str,
    player: ObjectGuid,
) -> bool {
    let o = w.objects.get(this).expect("ACE: this is null");
    let Some(page) = pages::get_page(o.biota.properties_book_page_data.as_ref(), index) else {
        return false;
    };
    if page.page_text.as_deref() == Some(page_text) {
        return false;
    }
    // `page.PageText.Equals(pageText)` on a null PageText throws
    assert!(
        page.page_text.is_some(),
        "ACE: page.PageText is null (NullReferenceException)"
    );

    let player_name = crate::dispatch::name::name(w, player).unwrap_or_default();
    let account_name =
        crate::world_objects::world_object_networking::shims::player_session_account(w, player);
    let p = w.objects.get(player).expect("ACE: player is null");
    let can_edit = page.ignore_author
        || (player.full() == page.author_id
            && page.author_name.as_deref() == Some(player_name.as_str())
            && page.author_account == account_name)
        || p.is_sentinel()
        || p.is_admin();

    if !can_edit {
        return false;
    }

    let o = w.objects.get_mut(this).expect("ACE: this is null");
    let list = o
        .biota
        .properties_book_page_data
        .as_mut()
        .expect("the page exists");
    let page = &mut list[usize::try_from(index).expect("a found page has a non-negative index")];
    page.author_account = account_name;
    page.author_id = player.full();
    page.author_name = Some(player_name);
    page.page_text = Some(page_text.to_owned());
    o.wo.world_object_database.changes_detected = true;

    true
}

// ACE: Book.DeletePage
/// Removes a page the player may delete (its author by guid, anyone for an `IgnoreAuthor` page,
/// or staff).
pub fn delete_page(w: &mut World, this: ObjectGuid, index: i32, player: ObjectGuid) -> bool {
    let o = w.objects.get(this).expect("ACE: this is null");
    let p = w.objects.get(player).expect("ACE: player is null");
    let Some(page) = pages::get_page(o.biota.properties_book_page_data.as_ref(), index) else {
        return false;
    };
    if !page.ignore_author && player.full() != page.author_id && !p.is_sentinel() && !p.is_admin() {
        return false;
    }

    let o = w.objects.get_mut(this).expect("ACE: this is null");
    pages::remove_page(o.biota.properties_book_page_data.as_mut(), index);
    o.wo.world_object_database.changes_detected = true;

    let new_page_count = pages::get_page_count(o.biota.properties_book_page_data.as_ref());
    o.set_property(PropertyInt::AppraisalPages, new_page_count);

    true
}

// ACE: Book.GetPage
#[must_use]
pub fn get_page(o: &WorldObject, index: i32) -> Option<&PropertiesBookPageData> {
    pages::get_page(o.biota.properties_book_page_data.as_ref(), index)
}

// ---- constructors and SetEphemeralValues ----

/// `new Book(weenie, guid)` / `new Book(biota)`: the `WorldObject` constructor, then Book's
/// `InitializePropertyDictionaries` and `SetEphemeralValues`.
// ACE: Book.Book
pub fn book_ctor(
    o: &mut crate::world_objects::world_object::WorldObject,
    env: &crate::world_objects::world_object::CtorEnv<'_>,
    src: crate::world_objects::world_object::CtorSource,
) {
    crate::world_objects::world_object::world_object_ctor(o, env, src);
    book_initialize_property_dictionaries(o);
    book_set_ephemeral_values(o);
}

// ACE: Book.InitializePropertyDictionaries
fn book_initialize_property_dictionaries(o: &mut crate::world_objects::world_object::WorldObject) {
    if o.biota.properties_book.is_none() {
        o.biota.properties_book =
            Some(empyrean_entity::models::properties_book::PropertiesBook::default());
    }
    if o.biota.properties_book_page_data.is_none() {
        o.biota.properties_book_page_data = Some(Vec::new());
    }
}

// ACE: Book.SetEphemeralValues
fn book_set_ephemeral_values(o: &mut crate::world_objects::world_object::WorldObject) {
    o.wo.world_object.object_description_flags |=
        empyrean_entity::enums::ObjectDescriptionFlag::Book;

    // Ensure a book can always be "read"
    o.set_activation_response(
        o.activation_response() | empyrean_entity::enums::ActivationResponse::Use,
    );

    let pages = o
        .biota
        .properties_book_page_data
        .as_ref()
        .map_or(0, Vec::len);
    o.set_property(
        empyrean_entity::enums::PropertyInt::AppraisalPages,
        i32::try_from(pages).unwrap_or(i32::MAX),
    );

    let max_num_pages = o
        .biota
        .properties_book
        .as_ref()
        .map_or(0, |b| b.max_num_pages);
    o.set_property(
        empyrean_entity::enums::PropertyInt::AppraisalMaxPages,
        max_num_pages,
    );
}
