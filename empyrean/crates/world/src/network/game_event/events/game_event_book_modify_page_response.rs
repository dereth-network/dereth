// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventBookModifyPageResponse.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventBookModifyPageResponse.cs`.

use dereth_primitives::ObjectId;
use dereth_protocol::trade as proto;
use empyrean_common::dotnet::CsCast;
use empyrean_net::GameMessageGroup;

use crate::network::game_event::game_event_message::game_event_from_proto;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::GameMessage;
use crate::sessions::SessionData;

// ACE: GameEventBookModifyPageResponse.GameEventBookModifyPageResponse
#[must_use]
pub fn game_event_book_modify_page_response(
    session: &mut SessionData,
    book_guid: u32,
    page: i32,
    success: bool,
) -> GameMessage {
    game_event_from_proto(
        GameEventType::BookModifyPageResponse,
        GameMessageGroup::UIQueue,
        session,
        &proto::WritingBookModifyPageResponse {
            book_id: ObjectId(book_guid),
            page_number: page.cs_cast(), // 0-based
            success: i32::from(success),
        },
    )
}
