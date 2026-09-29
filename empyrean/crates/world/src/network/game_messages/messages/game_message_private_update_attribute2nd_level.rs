// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameMessages/Messages/GameMessagePrivateUpdateAttribute2ndLevel.cs
//! Port of `Source/ACE.Server/Network/GameMessages/Messages/GameMessagePrivateUpdateAttribute2ndLevel.cs`.

use dereth_protocol::qualities::{self as proto, PrivateUpdate};
use empyrean_entity::enums::Vital;
use empyrean_net::GameMessageGroup;

use crate::network::game_messages::game_message::{byte_sequence, GameMessage};
use crate::network::game_messages::game_message_opcode::GameMessageOpcode;
use crate::network::sequence::sequence_manager::HasSequences;
use crate::network::sequence::sequence_type::SequenceType;

// ACE: GameMessagePrivateUpdateAttribute2ndLevel.GameMessagePrivateUpdateAttribute2ndLevel
#[must_use]
pub fn game_message_private_update_attribute2nd_level(
    world_object: &mut impl HasSequences,
    vital: Vital,
    current: u32,
) -> GameMessage {
    let sequence = byte_sequence(
        &world_object
            .sequences()
            .get_next_sequence_of(SequenceType::UpdateAttribute2ndLevel, vital),
    );
    GameMessage::from_proto(
        GameMessageOpcode::PrivateUpdateAttribute2ndLevel,
        GameMessageGroup::UIQueue,
        &proto::QualitiesPrivateUpdateAttribute2ndLevel(PrivateUpdate {
            sequence,
            property_id: vital.0,
            value: current,
        }),
    )
}
