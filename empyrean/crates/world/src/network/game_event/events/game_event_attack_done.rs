// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventAttackDone.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventAttackDone.cs`.

use dereth_protocol::combat as proto;
use empyrean_common::dotnet::CsCast;
use empyrean_entity::enums::WeenieError;
use empyrean_net::GameMessageGroup;

use crate::network::game_event::game_event_message::game_event_from_proto;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::GameMessage;
use crate::sessions::SessionData;

// ACE: GameEventAttackDone.GameEventAttackDone
/// `errorType` defaults to `WeenieError.None` in ACE.
#[must_use]
pub fn game_event_attack_done(session: &mut SessionData, error_type: WeenieError) -> GameMessage {
    game_event_from_proto(
        GameEventType::AttackDone,
        GameMessageGroup::UIQueue,
        session,
        &proto::CombatHandleAttackDoneEvent {
            error: error_type.0.cs_cast(),
        },
    )
}
