// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventViewContents.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventViewContents.cs`.

use dereth_protocol::objects::ItemOnViewContents;
use dereth_protocol::types::ContentProfile;
use empyrean_entity::ObjectGuid;
use empyrean_net::GameMessageGroup;
use empyrean_net::SessionId;

use crate::network::game_event::game_event_message::game_event_message;
use crate::network::game_event::game_event_message::session_data;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::GameMessage;
use crate::World;

// ACE: GameEventViewContents.GameEventViewContents
/// The container and its items ordered by `PlacementPosition` (a stable sort, null first), each
/// with its container type; `Container.Inventory` is the inventory
/// dictionary.
#[must_use]
pub fn game_event_view_contents(
    w: &mut World,
    session: SessionId,
    container: ObjectGuid,
) -> GameMessage {
    use empyrean_entity::enums::{ContainerType, PropertyBool, WeenieType};

    let mut msg = game_event_message(
        GameEventType::ViewContents,
        GameMessageGroup::UIQueue,
        session_data(w, session),
    );

    // The container, `(uint)container.Inventory.Count`, then each item and its container type.
    let inventory = crate::world_objects::container::inventory_values(w, container);
    let mut items: Vec<_> = inventory.iter().filter_map(|g| w.objects.get(*g)).collect();
    // `Inventory` holds the objects themselves, so its count is the number written.
    debug_assert_eq!(
        items.len(),
        inventory.len(),
        "an inventory entry is not in the world"
    );
    items.sort_by_key(|o| o.placement_position());
    let contents = items
        .into_iter()
        .map(|inv| {
            let container_type = if inv.biota.weenie_type == WeenieType::Container {
                ContainerType::Container
            } else if inv
                .get_property(PropertyBool::RequiresBackpackSlot)
                .unwrap_or(false)
            {
                ContainerType::Foci
            } else {
                ContainerType::NonContainer
            };
            ContentProfile {
                iid: inv.guid.into(),
                container_properties: container_type.0.cast_unsigned(),
            }
        })
        .collect();
    msg.write_proto(&ItemOnViewContents {
        container: container.into(),
        contents,
    });
    msg
}
