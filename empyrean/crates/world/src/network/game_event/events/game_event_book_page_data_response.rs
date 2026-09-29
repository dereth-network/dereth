// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventBookPageDataResponse.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventBookPageDataResponse.cs`.

use dereth_primitives::ObjectId;
use dereth_protocol::trade::{self as proto, PageData};
use empyrean_entity::models::PropertiesBookPageData;
use empyrean_net::GameMessageGroup;
use empyrean_net::SessionId;

use crate::network::game_event::game_event_message::game_event_message;
use crate::network::game_event::game_event_message::session_data;
use crate::network::game_event::game_event_message::session_player;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::ace_str;
use crate::network::game_messages::game_message::GameMessage;
use crate::World;

// ACE: GameEventBookPageDataResponse.GameEventBookPageDataResponse
#[must_use]
pub fn game_event_book_page_data_response(
    w: &mut World,
    session: SessionId,
    book_id: u32,
    page_index: i32,
    page_data: &PropertiesBookPageData,
) -> GameMessage {
    // 465 is the average seen in retail pcaps, 1,084 is the max seen in retail pcaps
    let mut msg = game_event_message(
        GameEventType::BookPageDataResponse,
        GameMessageGroup::UIQueue,
        session_data(w, session),
    );
    let player = session_player(w, session);
    let player = w
        .objects
        .get(player)
        .expect("ACE: session.Player is null (NullReferenceException)");

    let author_name = page_data.author_name.as_deref();
    // Check if player is admin and hide AuthorAccount if not. Potential security hole if we are sending out account usernames.
    let author_account = if player.is_admin_prop() {
        page_data.author_account.as_deref()
    } else {
        Some("Password is cheese")
    };
    let page_text = page_data.page_text.as_deref();
    // `book_id` unused?
    let body = proto::BookPageDataResponse {
        object_id: ObjectId(book_id),
        page: page_index.cast_unsigned(),
        data: PageData {
            author_id: ObjectId(page_data.author_id),
            author_name: ace_str(author_name),
            author_account: ace_str(author_account),
            version: Some(PageData::VERSION_WITH_FLAGS), // flags
            text_included: 1, // textIncluded - Will also be the case, even if we are sending an empty string.
            ignore_author: i32::from(page_data.ignore_author), // ignoreAuthor
            page_text: Some(ace_str(page_text)),
        },
    };
    msg.write_proto_strings(
        &body,
        &[
            author_name.unwrap_or(""),
            author_account.unwrap_or(""),
            page_text.unwrap_or(""),
        ],
    );
    msg
}
