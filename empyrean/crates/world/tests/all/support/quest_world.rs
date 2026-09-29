//! Shared isolated-world fixture for the area tests.

#![allow(unused_imports)]

pub(crate) use empyrean_entity::enums::{
    CharacterTitle, DamageType, EmoteCategory, EmoteType, PropertyString, WeenieError,
};
pub(crate) use empyrean_entity::ObjectGuid;
pub(crate) use empyrean_world::entity::damage_history;
pub(crate) use empyrean_world::managers::landblock_manager as lm;
pub(crate) use empyrean_world::managers::quest_manager::{self as qm, QuestOwner};
pub(crate) use empyrean_world::network::game_messages::game_message::{start_capture, take_sent};
pub(crate) use empyrean_world::physics::{object_maint, phys_ext};
pub(crate) use empyrean_world::world_objects::managers::emote_manager as em;
pub(crate) use empyrean_world::world_objects::{
    creature_death, monster_awareness, player_tracking, portal,
};

pub(crate) use crate::content::quests::{
    chats, kinds, me, player, set_now, u32_at, we, world, RAT, T0,
};
pub(crate) use crate::monsters::monster_ai::{lb_id, H as Arena, MONSTER, PLAYER as ARENA_PLAYER};
pub(crate) use crate::movement::player_movement::spawn_portal;

pub(crate) const WEENIE_ERROR: u32 = 0x028A;

/// A `MaxHealth` vital of `health` and an empty enchantment registry, as a creature's constructor
/// leaves them (the fixture creature has neither; `DamageHistory.Add` reads the vital).
pub(crate) fn give_health(w: &mut empyrean_world::World, g: ObjectGuid, health: u32) {
    use empyrean_entity::enums::PropertyAttribute2nd;
    use empyrean_entity::models::properties_attribute_2nd::PropertiesAttribute2nd;
    use empyrean_world::world_objects::entity::creature_vital::CreatureVital;
    let o = w.objects.get_mut(g).unwrap();
    o.biota.properties_enchantment_registry = Some(Vec::new());
    let mut vitals = empyrean_common::dotnet::DotNetDict::new();
    vitals.insert(
        PropertyAttribute2nd::MaxHealth,
        PropertiesAttribute2nd {
            init_level: health,
            level_from_cp: 0,
            cp_spent: 0,
            current_level: health,
        },
    );
    o.biota.properties_attribute_2nd = Some(vitals);
    let cv = CreatureVital::new(o, PropertyAttribute2nd::MaxHealth);
    o.vitals_mut().insert(PropertyAttribute2nd::MaxHealth, cv);
}
pub(crate) const CREATE_OBJECT: u32 = 0xF745;
