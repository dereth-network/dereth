// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventAttackerNotification.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventAttackerNotification.cs`.

use dereth_protocol::combat as proto;
use empyrean_common::dotnet::CsCast;
use empyrean_entity::enums::AttackConditions;
use empyrean_entity::enums::DamageType;
use empyrean_net::GameMessageGroup;

use crate::network::game_event::game_event_message::game_event_message;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::ace_str;
use crate::network::game_messages::game_message::GameMessage;
use crate::sessions::SessionData;

// ACE: GameEventAttackerNotification.GameEventAttackerNotification
#[must_use]
pub fn game_event_attacker_notification(
    session: &mut SessionData,
    defender_name: &str,
    damage_type: DamageType,
    percent: f32,
    damage: u32,
    critical_hit: bool,
    attack_conditions: AttackConditions,
) -> GameMessage {
    // AttackConditions is written as a `ulong`, following ACE and the retail server
    // (V293); the client reads only the low dword.
    let wide: u64 = attack_conditions.0.cs_cast();
    let body = proto::AttackerNotification {
        defender_name: ace_str(defender_name),
        damage_type: damage_type.0.cs_cast(),
        percent: f64::from(percent),
        damage,
        critical: u32::from(critical_hit),
        attack_conditions: wide.cs_cast(),
        attack_conditions_high: (wide >> 32).cs_cast(),
    };
    let mut msg = game_event_message(
        GameEventType::AttackerNotification,
        GameMessageGroup::UIQueue,
        session,
    );
    msg.write_proto_strings(&body, &[defender_name]);
    msg
}
