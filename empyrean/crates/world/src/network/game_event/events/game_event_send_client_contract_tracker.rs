// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventSendClientContractTracker.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventSendClientContractTracker.cs`.

use empyrean_net::GameMessageGroup;
use empyrean_net::SessionId;

use crate::network::game_event::game_event_message::game_event_message_with_capacity;
use crate::network::game_event::game_event_message::session_data;
use crate::network::game_event::game_event_message::session_player;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::BinaryWriter;
use crate::network::game_messages::game_message::GameMessage;
use crate::network::structure::contract_tracker::{self, CharacterPropertiesContractRegistry};
use crate::World;

// ACE: GameEventSendClientContractTracker.GameEventSendClientContractTracker
/// `new ContractTracker(session.Player, contract)` and its writer.
#[must_use]
pub fn game_event_send_client_contract_tracker(
    w: &mut World,
    session: SessionId,
    contract: &CharacterPropertiesContractRegistry,
) -> GameMessage {
    let mut msg = game_event_message_with_capacity(
        GameEventType::SendClientContractTracker,
        GameMessageGroup::UIQueue,
        session_data(w, session),
        52,
    );
    let player = session_player(w, session);
    let mut contract_tracker = contract_tracker::contract_tracker_new(w, player, contract);
    contract_tracker::write(&mut msg.data, w, &mut contract_tracker);
    msg.data
        .write_u32(u32::from(contract_tracker.delete_contract));
    msg.data
        .write_u32(u32::from(contract_tracker.set_as_display_contract));
    // Not ACE's (retail, V294; the retail captures): the retail server's update is
    // 48 bytes, twelve more after the flags that the client never reads. We send that length
    // with the twelve as zeros; ACE sent 36.
    msg.data
        .write_bytes(&[0; dereth_protocol::social::CONTRACT_TRACKER_TAIL]);
    msg
}
