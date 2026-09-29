// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameMessages/Messages/GameMessagePublicUpdatePropertyInt64.cs
//! Port of `Source/ACE.Server/Network/GameMessages/Messages/GameMessagePublicUpdatePropertyInt64.cs`.

use dereth_protocol::qualities::{self as proto, PublicUpdate};
use empyrean_entity::enums::PropertyInt64;
use empyrean_net::GameMessageGroup;

use crate::network::game_messages::game_message::{byte_sequence, GameMessage};
use crate::network::game_messages::game_message_opcode::GameMessageOpcode;
use crate::network::sequence::sequence_manager::HasSequences;
use crate::network::sequence::sequence_type::SequenceType;

// ACE: GameMessagePublicUpdatePropertyInt64.GameMessagePublicUpdatePropertyInt64
#[must_use]
pub fn game_message_public_update_property_int64(
    world_object: &mut impl HasSequences,
    property: PropertyInt64,
    value: i64,
) -> GameMessage {
    let sequence = byte_sequence(
        &world_object
            .sequences()
            .get_next_sequence_of(SequenceType::UpdatePropertyInt64, property),
    );
    let object = world_object.world_object().guid.into();
    GameMessage::from_proto(
        GameMessageOpcode::PublicUpdatePropertyInt64,
        GameMessageGroup::UIQueue,
        &proto::QualitiesUpdateInt64(PublicUpdate {
            sequence,
            object,
            property_id: property.into(),
            value,
        }),
    )
}
