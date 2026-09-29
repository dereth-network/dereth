// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionBookPageData.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionBookPageData.cs`.

use dereth_protocol as proto;
use empyrean_common::dotnet::CsCast;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_book;
use crate::World;

// ACE: GameActionBookPageData.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode::<proto::trade::WritingBookPageData>()?;
    let book_guid = m.book_id.0;
    let page_num: i32 = m.page.cs_cast(); // 0-based
    log::info!("0xAE - BookPageData({book_guid:08X}, {page_num}) - unused?");

    let player = session_player(w, session);
    player_book::read_book_page(w, player, book_guid, page_num);
    Ok(())
}
