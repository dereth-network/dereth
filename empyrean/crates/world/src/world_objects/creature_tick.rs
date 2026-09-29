// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Creature_Tick.cs
//! Port of `Source/ACE.Server/WorldObjects/Creature_Tick.cs`.

use empyrean_entity::enums::ChatMessageType;
use empyrean_entity::models::properties_enchantment_registry_extensions::has_enchantments;
use empyrean_entity::ObjectGuid;

use crate::dispatch;
use crate::managers::player_manager::player_session;
use crate::network::game_messages::game_message::enqueue_send;
use crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use crate::world_objects::creature_equipment::equipped_objects_values;
use crate::world_objects::managers::{emote_manager, enchantment_manager};
use crate::world_objects::world_object::WorldObject;
use crate::World;

/// Non-property fields declared in `Creature_Tick.cs`.
#[derive(Debug, Default)]
pub struct CreatureTickFields {}

// ---- virtual-dispatch targets ----

// ACE: Creature.Heartbeat
/// Called every ~5 seconds for Creatures.
pub fn creature_heartbeat(w: &mut World, this: ObjectGuid, current_unix_time: f64) {
    let utc_now = w.now.utc;
    let mut expire_items = Vec::new();

    // added where clause
    for wo in equipped_objects_values(w, this) {
        let o = w
            .objects
            .get(wo)
            .expect("an equipped item is a live object");
        // `i.EnchantmentManager.HasEnchantments`: the caching manager's value is the registry's.
        if !(has_enchantments(o.biota.properties_enchantment_registry.as_ref())
            || o.lifespan().is_some())
        {
            continue;
        }

        // FIXME: wo.NextHeartbeatTime is double.MaxValue here
        //if (wo.NextHeartbeatTime <= currentUnixTime)
        //wo.Heartbeat(currentUnixTime);

        // just go by parent heartbeats, only for enchantments?
        // TODO: handle players dropping / picking up items
        // Not ACE's (the retail captures, V280): a player's items are credited the
        // seconds since its previous heartbeat (4 to 6 s); others ACE's interval.
        let interval = crate::world_objects::player_tick::heartbeat_credit(w, this);
        enchantment_manager::heart_beat(w, wo, interval);

        if w.objects
            .get(wo)
            .is_some_and(|o| o.is_lifespan_spent(utc_now))
        {
            expire_items.push(wo);
        }
    }

    dispatch::vital_heart_beat::vital_heart_beat(w, this);

    emote_manager::heart_beat(w, this);

    if let Some(c) = w.objects.get_mut(this).and_then(|o| o.creature.as_mut()) {
        c.creature_death.damage_history.try_prune(utc_now);
    }

    // delete items when RemainingLifespan <= 0
    for expire_item in expire_items {
        let name = dispatch::name::name(w, expire_item).unwrap_or_default();
        crate::world_objects::world_object_decay::delete_object(w, expire_item, Some(this));

        if w.objects.get(this).is_some_and(WorldObject::is_player) {
            let message = game_message_system_chat(
                &format!("Its lifespan finished, your {name} crumbles to dust."),
                ChatMessageType::Broadcast,
            );
            if let Some(session) = player_session(w, this) {
                enqueue_send(w, session, message);
            }
        }
    }

    crate::world_objects::container_tick::container_heartbeat(w, this, current_unix_time);
}
