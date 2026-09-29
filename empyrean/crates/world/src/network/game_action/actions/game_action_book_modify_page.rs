// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionBookModifyPage.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionBookModifyPage.cs`.

use dereth_protocol as proto;
use empyrean_common::dotnet::CsCast;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_book;
use crate::World;

// ACE: GameActionBookModifyPage.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode_padded::<proto::trade::WritingBookModifyPage>()?;
    let book_guid = m.book_id.0;
    let page: i32 = m.page.cs_cast(); // 0-based
    let text = m.text;

    let player = session_player(w, session);
    player_book::handle_action_book_modify_page(w, player, book_guid, page, &text);
    Ok(())
}
