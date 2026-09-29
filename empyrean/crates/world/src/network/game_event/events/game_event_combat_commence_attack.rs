// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventCombatCommenceAttack.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventCombatCommenceAttack.cs`.

use dereth_protocol::combat as proto;
use empyrean_net::GameMessageGroup;

use crate::network::game_event::game_event_message::game_event_from_proto;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::GameMessage;
use crate::sessions::SessionData;

// ACE: GameEventCombatCommenceAttack.GameEventCombatCommenceAttack
#[must_use]
pub fn game_event_combat_commence_attack(session: &mut SessionData) -> GameMessage {
    game_event_from_proto(
        GameEventType::CombatCommenceAttack,
        GameMessageGroup::UIQueue,
        session,
        &proto::CombatHandleCommenceAttackEvent,
    )
}
