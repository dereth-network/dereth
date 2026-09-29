// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionBookAddPage.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionBookAddPage.cs`.

use dereth_protocol as proto;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_book;
use crate::World;

// ACE: GameActionBookAddPage.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode::<proto::trade::WritingBookAddPage>()?;
    let book_guid = m.book_id.0;

    let player = session_player(w, session);
    player_book::handle_action_book_add_page(w, player, book_guid);
    Ok(())
}
