// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameMessages/Messages/GameMessageUpdateMotion.cs
//! Port of `Source/ACE.Server/Network/GameMessages/Messages/GameMessageUpdateMotion.cs`.

use dereth_world_data::command_numbering::CommandNumbering;
use empyrean_net::GameMessageGroup;

use crate::network::game_messages::game_message::BinaryWriter;
use crate::network::game_messages::game_message::GameMessage;
use crate::network::game_messages::game_message_opcode::GameMessageOpcode;
use crate::network::motion::movement_data::MovementData;
use crate::network::sequence::sequence_manager::HasSequences;
use crate::network::sequence::sequence_type::SequenceType;

// ACE: GameMessageUpdateMotion.GameMessageUpdateMotion
/// `GameMessageUpdateMotion(WorldObject wo, MovementData movementData)`. ACE's other overload,
/// `(WorldObject wo, Motion motion)`, is `new MovementData(wo, motion)` followed by the same
/// [`send`]; callers compose it. Not ACE: `numbering` is the world's files' command numbering,
/// which the motion goes out in.
#[must_use]
pub fn game_message_update_motion(
    wo: &mut impl HasSequences,
    movement_data: &MovementData,
    numbering: CommandNumbering,
) -> GameMessage {
    // 88 is the max seen in retail pcaps
    let mut msg = GameMessage::with_capacity(
        GameMessageOpcode::Motion,
        GameMessageGroup::SmartboxQueue,
        88,
    );
    send(&mut msg, wo, movement_data, numbering);
    msg
}

// ACE: GameMessageUpdateMotion.Send
pub fn send(
    msg: &mut GameMessage,
    wo: &mut impl HasSequences,
    movement_data: &MovementData,
    numbering: CommandNumbering,
) {
    msg.data.write_guid(wo.world_object().guid);
    msg.data.write_bytes(
        &wo.sequences()
            .get_current_sequence(SequenceType::ObjectInstance),
    );
    crate::network::motion::movement_data::write(
        &mut msg.data,
        movement_data,
        true,
        wo.sequences(),
        numbering,
    );
}
