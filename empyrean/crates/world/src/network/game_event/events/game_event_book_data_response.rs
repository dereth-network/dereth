// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventBookDataResponse.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventBookDataResponse.cs`.

use dereth_primitives::ObjectId;
use dereth_protocol::trade::{self as proto, PageData, PageDataList};
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

// ACE: GameEventBookDataResponse.GameEventBookDataResponse
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn game_event_book_data_response(
    w: &mut World,
    session: SessionId,
    book_id: u32,
    max_chars: i32,
    max_pages: i32,
    page_data: &[PropertiesBookPageData],
    inscription: Option<&str>,
    author_id: u32,
    author_name: Option<&str>,
    ignore_author: bool,
) -> GameMessage {
    let mut msg = game_event_message(
        GameEventType::BookDataResponse,
        GameMessageGroup::UIQueue,
        session_data(w, session),
    );
    let player = session_player(w, session);
    let player = w
        .objects
        .get(player)
        .expect("ACE: session.Player is null (NullReferenceException)");

    let mut strings: Vec<&str> = Vec::new();
    let mut pages = Vec::with_capacity(page_data.len());
    for page in page_data {
        let author_name = page.author_name.as_deref();
        // Check if player is admin and hide AuthorAccount if not. Potential security hole if we are sending out account usernames.
        let author_account = if player.is_admin_prop()
            || player.is_sentinel_prop()
            || player.is_envoy()
            || player.is_arch()
            || player.is_psr()
        {
            page.author_account.as_deref()
        } else {
            Some("beer good")
        };
        strings.extend([author_name.unwrap_or(""), author_account.unwrap_or("")]);

        // With this flag set, it tells the client to always read the next two items.
        // Might result in more data than retail in some instances, but easier to manage and control for us.
        let version = Some(PageData::VERSION_WITH_FLAGS); // 0xFFFF0002

        // This will always be null for this event.
        let (text_included, page_text) = match page.page_text.as_deref() {
            Some(page_text) => {
                strings.push(page_text);
                (1, Some(ace_str(page_text))) // Text Included
            }
            None => (0, None), // Text Included
        };
        pages.push(PageData {
            author_id: ObjectId(page.author_id),
            author_name: ace_str(author_name),
            author_account: ace_str(author_account),
            version,
            text_included,
            ignore_author: i32::from(ignore_author), // Ignore Author
            page_text,
        });
    }
    strings.extend([inscription.unwrap_or(""), author_name.unwrap_or("")]);

    // PCAPs show these page numbers were always the same regardless of if the pages were filled or blank.
    let body = proto::WritingBookOpen {
        book_id: ObjectId(book_id),
        max_num_pages: max_pages.cast_unsigned(), // maxNumPages
        pages: PageDataList {
            max_num_pages: max_pages,          // numPages
            max_num_chars_per_page: max_chars, // maxNumCharsPerPage
            pages,
        },
        inscription: ace_str(inscription),
        scribe_id: ObjectId(if author_id != 0xFFFF_FFFF {
            author_id
        } else {
            0x0
        }),
        scribe_name: ace_str(author_name),
    };
    msg.write_proto_strings(&body, &strings);
    msg
}
