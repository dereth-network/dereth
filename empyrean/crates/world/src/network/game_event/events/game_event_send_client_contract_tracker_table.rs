// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventSendClientContractTrackerTable.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventSendClientContractTrackerTable.cs`.

use empyrean_net::GameMessageGroup;
use empyrean_net::SessionId;

use crate::network::game_event::game_event_message::game_event_message_with_capacity;
use crate::network::game_event::game_event_message::session_data;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::GameMessage;
use crate::World;

// ACE: GameEventSendClientContractTrackerTable.GameEventSendClientContractTrackerTable
/// `Writer.Write(session.Player.ContractManager)`: the contract tracker table.
#[must_use]
pub fn game_event_send_client_contract_tracker_table(
    w: &mut World,
    session: SessionId,
) -> GameMessage {
    let mut msg = game_event_message_with_capacity(
        GameEventType::SendClientContractTrackerTable,
        GameMessageGroup::UIQueue,
        session_data(w, session),
        512,
    );
    let player = crate::network::game_event::game_event_message::session_player(w, session);
    crate::world_objects::managers::contract_manager::write(&mut msg.data, w, player);
    msg
}
