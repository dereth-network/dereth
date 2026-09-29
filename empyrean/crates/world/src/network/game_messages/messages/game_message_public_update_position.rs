// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameMessages/Messages/GameMessagePublicUpdatePosition.cs
//! Port of `Source/ACE.Server/Network/GameMessages/Messages/GameMessagePublicUpdatePosition.cs`.

use dereth_protocol::qualities::{self as proto, PublicUpdate};
use empyrean_entity::enums::PositionType;
use empyrean_entity::Position;
use empyrean_net::GameMessageGroup;

use crate::network::game_messages::game_message::{byte_sequence, GameMessage};
use crate::network::game_messages::game_message_opcode::GameMessageOpcode;
use crate::network::sequence::sequence_manager::HasSequences;
use crate::network::sequence::sequence_type::SequenceType;

// ACE: GameMessagePublicUpdatePosition.GameMessagePublicUpdatePosition
#[must_use]
pub fn game_message_public_update_position(
    world_object: &mut impl HasSequences,
    position_type: PositionType,
    pos: &Position,
) -> GameMessage {
    let sequence = byte_sequence(
        &world_object
            .sequences()
            .get_next_sequence_of(SequenceType::UpdatePosition, position_type),
    );
    // `pos.Serialize(Writer)`: writeQuaternion and writeLandblock default to true.
    GameMessage::from_proto(
        GameMessageOpcode::PublicUpdatePosition,
        GameMessageGroup::UIQueue,
        &proto::QualitiesUpdatePosition(PublicUpdate {
            sequence,
            object: world_object.world_object().guid.into(),
            property_id: position_type.into(),
            value: pos.into(),
        }),
    )
}
