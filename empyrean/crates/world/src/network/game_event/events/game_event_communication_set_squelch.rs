// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventCommunicationSetSquelch.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventCommunicationSetSquelch.cs`.

use empyrean_net::GameMessageGroup;

use crate::network::game_event::game_event_message::game_event_message;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::GameMessage;
use crate::network::structure::squelch_db::{self, SquelchDB};
use crate::sessions::SessionData;

// ACE: GameEventSetSquelchDB.GameEventSetSquelchDB
#[must_use]
pub fn game_event_set_squelch_db(session: &mut SessionData, db: &SquelchDB) -> GameMessage {
    let mut msg = game_event_message(
        GameEventType::SetSquelchDB,
        GameMessageGroup::UIQueue,
        session,
    );
    squelch_db::write(&mut msg.data, db);
    msg
}
