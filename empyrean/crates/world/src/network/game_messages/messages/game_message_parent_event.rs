// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameMessages/Messages/GameMessageParentEvent.cs
//! Port of `Source/ACE.Server/Network/GameMessages/Messages/GameMessageParentEvent.cs`.

use dereth_protocol::objects as proto;
use dereth_protocol::types::PhysicsEventStamp;
use empyrean_common::dotnet::CsCast;
use empyrean_entity::enums::ParentLocation;
use empyrean_entity::enums::Placement;
use empyrean_net::GameMessageGroup;

use crate::network::game_messages::game_message::ushort_sequence;
use crate::network::game_messages::game_message::GameMessage;
use crate::network::game_messages::game_message_opcode::GameMessageOpcode;
use crate::network::sequence::sequence_manager::HasSequences;
use crate::network::sequence::sequence_type::SequenceType;

// ACE: GameMessageParentEvent.GameMessageParentEvent
/// `overridden_parent_location` and `overridden_placement` default to `null` in ACE.
#[must_use]
pub fn game_message_parent_event(
    creature: &mut impl HasSequences,
    wielded_selectable_item: &mut impl HasSequences,
    overridden_parent_location: Option<ParentLocation>,
    overridden_placement: Option<Placement>,
) -> GameMessage {
    let item = wielded_selectable_item.world_object();
    let (item_guid, item_parent_location, item_placement) =
        (item.guid, item.parent_location(), item.placement());

    let location = overridden_parent_location
        .or(item_parent_location)
        .unwrap_or(ParentLocation::None)
        .0;
    let placement: i32 = overridden_placement
        .or(item_placement)
        .unwrap_or(Placement::Default)
        .0
        .cs_cast();
    let creature_guid = creature.world_object().guid;
    let instance = ushort_sequence(
        &creature
            .sequences()
            .get_current_sequence(SequenceType::ObjectInstance),
    );
    let event = ushort_sequence(
        &wielded_selectable_item
            .sequences()
            .get_next_sequence(SequenceType::ObjectPosition),
    );
    GameMessage::from_proto(
        GameMessageOpcode::ParentEvent,
        GameMessageGroup::SmartboxQueue,
        &proto::ItemParentEvent {
            creature: creature_guid.into(),
            item: item_guid.into(),
            location: location.cast_unsigned(),
            placement_frame: placement.cast_unsigned(),
            timestamps: PhysicsEventStamp { instance, event },
        },
    )
}
