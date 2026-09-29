// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameMessages/Messages/GameMessageAutonomousPosition.cs
//! Port of `Source/ACE.Server/Network/GameMessages/Messages/GameMessageAutonomousPosition.cs`.

use empyrean_net::GameMessageGroup;

use crate::network::game_messages::game_message::BinaryWriter;
use crate::network::game_messages::game_message::GameMessage;
use crate::network::game_messages::game_message_opcode::GameMessageOpcode;
use crate::network::sequence::sequence_manager::HasSequences;
use crate::network::sequence::sequence_type::SequenceType;

// ACE: GameMessageAutonomousPosition.GameMessageAutonomousPosition
/// Writes nothing after the opcode unless `worldObject is Player`.
///
/// # Panics
/// When the player has no `Location` (ACE: `NullReferenceException`).
#[must_use]
pub fn game_message_autonomous_position(world_object: &mut impl HasSequences) -> GameMessage {
    let mut msg = GameMessage::new(
        GameMessageOpcode::AutonomousPosition,
        GameMessageGroup::SecureWeenieQueue,
    );
    let wo = world_object.world_object();
    if wo.is_player() {
        let guid = wo.guid;
        let location = wo
            .location()
            .expect("ACE: Player.Location is null (NullReferenceException)");
        msg.data.write_guid(guid);
        location.serialize(&mut msg.data, true, false);
        let seq = world_object.sequences();
        msg.data
            .write_bytes(&seq.get_current_sequence(SequenceType::ObjectInstance)); // instance_timestamp - always 1 in my pcaps
        msg.data
            .write_bytes(&seq.get_current_sequence(SequenceType::ObjectServerControl)); // server_control_timestamp - always 0 in my pcaps
        msg.data
            .write_bytes(&seq.get_current_sequence(SequenceType::ObjectTeleport)); // teleport_timestamp - always 0 in my pcaps
        msg.data
            .write_bytes(&seq.get_current_sequence(SequenceType::ObjectForcePosition)); // force_position_timestamp - always 0 in my pcaps
        msg.data.write_u32(1); // contact - always "true" / 1 in my pcaps
    }
    msg
}
