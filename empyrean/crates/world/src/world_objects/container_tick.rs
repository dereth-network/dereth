// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Container_Tick.cs
//! Port of `Source/ACE.Server/WorldObjects/Container_Tick.cs`.

use empyrean_entity::enums::ChatMessageType;
use empyrean_entity::models::properties_enchantment_registry_extensions::has_enchantments;
use empyrean_entity::ObjectGuid;

use crate::dispatch;
use crate::managers::player_manager::player_session;
use crate::network::game_messages::game_message::enqueue_send;
use crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use crate::world_objects::container::inventory_values;
use crate::world_objects::world_object::WorldObject;
use crate::World;

/// Non-property fields declared in `Container_Tick.cs`.
#[derive(Debug, Default)]
pub struct ContainerTickFields {}

/// Ticks the enchantments and lifespans of the items directly in `this` (`root_owner` is the
/// container whose heartbeat runs), and deletes the items whose lifespan is spent.
// ACE: Container.Inventory_Tick
pub fn inventory_tick(w: &mut World, this: ObjectGuid, root_owner: ObjectGuid) {
    let utc_now = w.now.utc;
    let mut expire_items = Vec::new();

    // added where clause
    for wo in inventory_values(w, this) {
        let o = w
            .objects
            .get(wo)
            .expect("an inventory item is a live object");
        let wo_has_enchantments =
            has_enchantments(o.biota.properties_enchantment_registry.as_ref());
        if !(wo_has_enchantments || o.lifespan().is_some()) {
            continue;
        }

        // FIXME: wo.NextHeartbeatTime is double.MaxValue here
        //if (wo.NextHeartbeatTime <= currentUnixTime)
        //wo.Heartbeat(currentUnixTime);

        // just go by parent heartbeats, only for enchantments?
        if wo_has_enchantments {
            // `wo.EnchantmentManager.HeartBeat(CachedHeartbeatInterval);` (this container's interval)
            // Not ACE's (retail captures, V280): in a player's packs, the
            // seconds since the player's previous heartbeat, which comes every 4 to 6 s.
            let interval = if w
                .objects
                .get(root_owner)
                .is_some_and(WorldObject::is_player)
            {
                crate::world_objects::player_tick::heartbeat_credit(w, root_owner)
            } else {
                w.objects
                    .get(this)
                    .expect("ACE: this is null")
                    .wo
                    .world_object_tick
                    .cached_heartbeat_interval
            };
            crate::world_objects::managers::enchantment_manager::heart_beat(w, wo, interval);
        }

        if w.objects
            .get(wo)
            .is_some_and(|o| o.is_lifespan_spent(utc_now))
        {
            expire_items.push(wo);
        }
    }

    // delete items when RemainingLifespan <= 0
    for expire_item in expire_items {
        // (the name is read first: `expireItem.Name` outlives the Destroy in C#)
        let name = dispatch::name::name(w, expire_item).unwrap_or_default();
        crate::world_objects::world_object_decay::delete_object(w, expire_item, Some(root_owner));

        if w.objects
            .get(root_owner)
            .is_some_and(WorldObject::is_player)
        {
            let message = game_message_system_chat(
                &format!("Its lifespan finished, your {name} crumbles to dust."),
                ChatMessageType::Broadcast,
            );
            if let Some(session) = player_session(w, root_owner) {
                enqueue_send(w, session, message);
            }
        }
    }
}

// ---- virtual-dispatch targets: each `not_ported!` until it is ported ----

// ACE: Container.Heartbeat
pub fn container_heartbeat(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    current_unix_time: f64,
) {
    // TODO: fix bug for landblock containers w/ no heartbeat
    inventory_tick(w, this, this);

    let subcontainers: Vec<ObjectGuid> = inventory_values(w, this)
        .into_iter()
        .filter(|&g| w.objects.get(g).is_some_and(WorldObject::is_container))
        .collect();
    for subcontainer in subcontainers {
        inventory_tick(w, subcontainer, this);
    }

    // for landblock containers
    let Some(o) = w.objects.get(this) else { return };
    if let (true, Some(current_landblock)) = (o.is_open(), o.current_landblock) {
        let viewer = crate::entity::landblock::get_object(
            w,
            current_landblock,
            ObjectGuid::new(o.viewer()),
            true,
        )
        .filter(|&g| w.objects.get(g).is_some_and(WorldObject::is_player));
        let Some(viewer) = viewer else {
            dispatch::close::close(w, this, ObjectGuid::INVALID);
            return;
        };
        let (within_use_radius, _target_valid) =
            crate::entity::landblock::within_use_radius(w, current_landblock, viewer, this, None);
        if !within_use_radius {
            dispatch::close::close(w, this, viewer);
            return;
        }
    }
    crate::world_objects::world_object_tick::world_object_heartbeat(w, this, current_unix_time);
}
