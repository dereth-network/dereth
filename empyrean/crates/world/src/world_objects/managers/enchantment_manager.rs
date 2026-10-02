// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Managers/EnchantmentManager.cs
//! Port of `Source/ACE.Server/WorldObjects/Managers/EnchantmentManager.cs`.
//!
//! # Shape
//!
//! ACE's `EnchantmentManager` holds `WorldObject` and `Player` (`obj as Player`) and reads the
//! object's biota. Here every member is a free function `(w, this, ..)`, `this` being the object
//! that owns the manager; `Player` is "`this` is a player".
//!
//! **Virtual members.** Every ACE `WorldObject.EnchantmentManager` is an
//! `EnchantmentManagerWithCaching` (`WorldObject.SetEphemeralValues`), so:
//! - callers use the functions in
//!   [`enchantment_manager_with_caching`](super::enchantment_manager_with_caching) for the members
//!   that class overrides (`HasEnchantments`, `Add`, `Remove`, `Dispel`, `UpdateVitae`, the
//!   aggregations ...), and the functions here for everything else;
//! - the functions here with the same name are the **base** implementations, and where the base
//!   body makes a virtual call (`HasVitae` in `UpdateVitae`, `Remove` in `HeartBeat` and
//!   `RemoveVitae`, `Dispel` in `DispelAllEnchantments`) it calls the caching override, as the
//!   .NET virtual call does.
//!
//! **Registry entries** returned by queries are references into the biota (read-only) or copies
//! (anything handed on to a function that takes `&mut World`). Updates in place go through
//! `get_enchantment_by_spell_mut` / `get_enchantments_by_category_indices`.
//!
//! **Pointers** to members of other ACE files are at the bottom of the file, each calling
//! `not_ported!` with ACE's name.

use empyrean_common::dotnet::math::round;
use empyrean_common::dotnet::CsCast;
use empyrean_common::extensions::float_extensions::epsilon_equals;
use empyrean_common::extensions::list_extensions::shuffle;
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_entity::enums::ext::skill_helper;
use empyrean_entity::enums::properties::{
    PropertyAttribute, PropertyAttribute2nd, PropertyFloat, PropertyInt,
};
use empyrean_entity::enums::{
    ChatMessageType, DamageType, DispelType, EnchantmentMask, EnchantmentTypeFlags, EquipmentSet,
    MagicSchool, Skill, Sound, SpellCategory, SpellId,
};
use empyrean_entity::models::properties_enchantment_registry_extensions as reg;
use empyrean_entity::models::PropertiesEnchantmentRegistry;
use empyrean_entity::ObjectGuid;
use empyrean_net::SessionId;

use crate::entity::add_enchantment_result::{
    caster_player_augmentation_increased_spell_duration, AddEnchantmentResult,
};
use crate::entity::spell::Spell;
use crate::entity::spell_enchantment::SpellEnchantment;
use crate::managers::player_manager::{self, player_session};
use crate::network::game_event::events::game_event_magic_dispel_enchantment::game_event_magic_dispel_enchantment;
use crate::network::game_event::events::game_event_magic_dispel_multiple_enchantments::game_event_magic_dispel_multiple_enchantments_registry;
use crate::network::game_event::events::game_event_magic_remove_enchantment::game_event_magic_remove_enchantment;
use crate::network::game_event::events::game_event_magic_update_enchantment::game_event_magic_update_enchantment;
use crate::network::game_event::game_event_message::session_data;
use crate::network::game_messages::game_message::enqueue_send;
use crate::network::game_messages::messages::game_message_sound::game_message_sound;
use crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use crate::network::structure::enchantment::Enchantment;
use crate::world_objects::managers::enchantment_manager_with_caching as caching;
use crate::world_objects::world_object::WorldObject;
use crate::World;

// ACE: StackType
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StackType {
    #[default]
    None,
    Initial,
    Surpass,
    Refresh,
    Surpassed,
}

/// The spell category of cooldown entries (ACE's `static ushort SpellCategory_Cooldown`, a
/// mutable static that nothing writes).
// ACE: EnchantmentManager.SpellCategory_Cooldown
pub const SPELL_CATEGORY_COOLDOWN: u16 = 0x8000;

// ------------------------------------------------------------------------------------ helpers

/// `WorldObject`: the manager's object. A missing object is ACE's `NullReferenceException`.
fn obj(w: &World, this: ObjectGuid) -> &WorldObject {
    w.objects
        .get(this)
        .expect("ACE: EnchantmentManager.WorldObject is null (NullReferenceException)")
}

fn obj_mut(w: &mut World, this: ObjectGuid) -> &mut WorldObject {
    w.objects
        .get_mut(this)
        .expect("ACE: EnchantmentManager.WorldObject is null (NullReferenceException)")
}

/// `WorldObject.Biota.PropertiesEnchantmentRegistry` for the queries that dereference the
/// result (`WorldObject.InitializePropertyDictionaries` always creates it).
fn registry(o: &WorldObject) -> &Vec<PropertiesEnchantmentRegistry> {
    o.biota
        .properties_enchantment_registry
        .as_ref()
        .expect("ACE: Biota.PropertiesEnchantmentRegistry is null (NullReferenceException)")
}

fn registry_mut(o: &mut WorldObject) -> &mut Vec<PropertiesEnchantmentRegistry> {
    o.biota
        .properties_enchantment_registry
        .as_mut()
        .expect("ACE: Biota.PropertiesEnchantmentRegistry is null (NullReferenceException)")
}

/// `WorldObject.ChangesDetected = true`.
fn set_changes_detected(w: &mut World, this: ObjectGuid) {
    obj_mut(w, this).wo.world_object_database.changes_detected = true;
}

/// `Player != null`.
fn is_player(w: &World, this: ObjectGuid) -> bool {
    w.objects.get(this).is_some_and(WorldObject::is_player)
}

/// `Player.Session`. ACE's `Player.Session.Network` throws `NullReferenceException` for a player
/// without a session.
fn session_of(w: &World, player: ObjectGuid) -> SessionId {
    player_session(w, player).expect("ACE: Player.Session is null (NullReferenceException)")
}

/// `Enum.HasFlag`.
fn has_flag(value: EnchantmentTypeFlags, flag: EnchantmentTypeFlags) -> bool {
    (value & flag) == flag
}

/// `(int)float` (net10: saturating, NaN to 0).
fn to_int(v: f32) -> i32 {
    v.cs_cast()
}

// --------------------------------------------------------------------------------- queries

/// Returns TRUE if this object has any active enchantments in the registry (base; callers use
/// [`caching::has_enchantments`]).
// ACE: EnchantmentManager.HasEnchantments
#[must_use]
pub fn has_enchantments(w: &World, this: ObjectGuid) -> bool {
    reg::has_enchantments(obj(w, this).biota.properties_enchantment_registry.as_ref())
}

/// Returns TRUE If this object has a vitae penalty (base; callers use [`caching::has_vitae`]).
// ACE: EnchantmentManager.HasVitae
#[must_use]
pub fn has_vitae(w: &World, this: ObjectGuid) -> bool {
    reg::has_enchantment(
        obj(w, this).biota.properties_enchantment_registry.as_ref(),
        SpellId::Vitae.0,
    )
}

/// Returns TRUE if registry contains spellId for this creature.
// ACE: EnchantmentManager.HasSpell
#[must_use]
pub fn has_spell(w: &World, this: ObjectGuid, spell_id: u32) -> bool {
    reg::has_enchantment(
        obj(w, this).biota.properties_enchantment_registry.as_ref(),
        spell_id,
    )
}

/// Returns the enchantments for a specific spell (`casterGuid` defaults to null).
// ACE: EnchantmentManager.GetEnchantment
#[must_use]
pub fn get_enchantment(
    w: &World,
    this: ObjectGuid,
    spell_id: u32,
    caster_guid: Option<u32>,
) -> Option<&PropertiesEnchantmentRegistry> {
    reg::get_enchantment_by_spell(
        obj(w, this).biota.properties_enchantment_registry.as_ref(),
        spell_id.cast_signed(),
        caster_guid,
    )
}

/// Returns the enchantments for a specific spell from an equipment set (the `EquipmentSet`
/// overload of `GetEnchantment`).
// ACE: EnchantmentManager.GetEnchantment
#[must_use]
pub fn get_enchantment_set(
    w: &World,
    this: ObjectGuid,
    spell_id: u32,
    equipment_set: EquipmentSet,
) -> Option<&PropertiesEnchantmentRegistry> {
    reg::get_enchantment_by_spell_set(
        obj(w, this).biota.properties_enchantment_registry.as_ref(),
        spell_id.cast_signed(),
        equipment_set,
    )
}

/// Returns a list of all the active enchantments for a magic school (the `MagicSchool` overload
/// of `GetEnchantments`).
// ACE: EnchantmentManager.GetEnchantments
#[must_use]
pub fn get_enchantments_school(
    w: &World,
    this: ObjectGuid,
    magic_school: MagicSchool,
) -> Vec<PropertiesEnchantmentRegistry> {
    let mut spells = Vec::new();

    let o = obj(w, this);
    let top_layer_enchantments =
        reg::get_enchantments_top_layer(Some(registry(o))).expect("the registry is not null");

    for enchantment in top_layer_enchantments {
        if enchantment.spell_id > i32::from(SPELL_CATEGORY_COOLDOWN) {
            continue;
        }

        let spell = Spell::from_int(w, enchantment.spell_id, true);

        if spell.not_found() {
            log::warn!(
                "EnchantmentManager.GetEnchantments({}): couldn't find spell {} for {}",
                magic_school.0,
                enchantment.spell_id,
                crate::dispatch::name::name(w, this).unwrap_or_default()
            );
            continue;
        }

        if spell.school() == magic_school {
            spells.push(enchantment.clone());
        }
    }

    spells
}

/// Returns all of the enchantments for a category (the `SpellCategory` overload of
/// `GetEnchantments`).
// ACE: EnchantmentManager.GetEnchantments
#[must_use]
pub fn get_enchantments_category(
    w: &World,
    this: ObjectGuid,
    spell_category: SpellCategory,
) -> Vec<&PropertiesEnchantmentRegistry> {
    reg::get_enchantments_by_category(Some(registry(obj(w, this))), spell_category)
        .expect("the registry is not null")
}

/// Returns the top layers in each spell category for a StatMod type.
// ACE: EnchantmentManager.GetEnchantments_TopLayer
#[must_use]
pub fn get_enchantments_top_layer(
    w: &World,
    this: ObjectGuid,
    stat_mod_type: EnchantmentTypeFlags,
) -> Vec<&PropertiesEnchantmentRegistry> {
    reg::get_enchantments_top_layer_by_stat_mod_type(Some(registry(obj(w, this))), stat_mod_type)
        .expect("the registry is not null")
}

/// Returns the top layers in each spell category for a StatMod type + key (`handleMultiple`
/// defaults to false).
// ACE: EnchantmentManager.GetEnchantments_TopLayer
#[must_use]
pub fn get_enchantments_top_layer_key(
    w: &World,
    this: ObjectGuid,
    stat_mod_type: EnchantmentTypeFlags,
    stat_mod_key: u32,
    handle_multiple: bool,
) -> Vec<&PropertiesEnchantmentRegistry> {
    reg::get_enchantments_top_layer_by_stat_mod_type_and_key(
        Some(registry(obj(w, this))),
        stat_mod_type,
        stat_mod_key,
        handle_multiple,
    )
    .expect("the registry is not null")
}

// ------------------------------------------------------------------------------ add / build

/// Add/update an enchantment in this object's registry (base; callers use [`caching::add`]).
/// `caster` and `weapon` are ACE's `WorldObject` arguments (`None` for `null`); `equip` and
/// `is_weapon_spell` default to false.
// ACE: EnchantmentManager.Add
pub fn add(
    w: &mut World,
    this: ObjectGuid,
    spell: &Spell,
    caster: Option<ObjectGuid>,
    weapon: Option<ObjectGuid>,
    equip: bool,
    is_weapon_spell: bool,
) -> AddEnchantmentResult {
    let mut result = AddEnchantmentResult::new();

    // check for existing spell in this category
    let indices =
        reg::get_enchantments_by_category_indices(Some(registry(obj(w, this))), spell.category())
            .expect("the registry is not null");

    // if none, add new record
    if indices.is_empty() {
        let mut new_entry = build_entry(w, this, spell, caster, weapon, equip, is_weapon_spell);
        new_entry.layer_id = 1;
        reg::add_enchantment(registry_mut(obj_mut(w, this)), new_entry.clone());
        set_changes_detected(w, this);

        result.enchantment = Some(new_entry);
        result.stack_type = StackType::Initial;
        return result;
    }

    let entries: Vec<PropertiesEnchantmentRegistry> = {
        let list = registry(obj(w, this));
        indices.iter().map(|&i| list[i].clone()).collect()
    };
    result.build_stack(w, &entries, spell, caster, equip, is_weapon_spell);

    // handle cases:
    // surpassing: new spell is written to next layer
    // refreshing: - key by caster guid
    // surpassed:  - underpowered spell is written to next layer?

    // note that these cases are not exclusive,
    // consider case: strength 3 -> strength 6 -> strength 3
    // for the 2nd cast of strength 3, it would have 1 refresh and 1 surpassed
    // would 2nd cast of strength 3 refresh the 1st, but still be surpassed by 6?

    let refresh_spell = if result.refresh.is_empty() {
        None
    } else {
        result.refresh_caster_entry.map(|e| indices[e])
    };

    if let Some(refresh_index) = refresh_spell {
        // for multiple void casters casting the same DoT,
        // we might want to sort by StatModValue in GetEnchantments_TopLayer()

        // for the same void caster re-casting the same DoT,
        // should be update the StatModVal here?

        let mut duration = spell.duration();
        if let Some(aug) = caster_player_augmentation_increased_spell_duration(w, caster) {
            if !is_weapon_spell && spell.dot_duration() == 0.0 {
                duration *= f64::from(1.0f32 + aug as f32 * 0.2f32);
            }
        }

        let refresh_entry = &mut registry_mut(obj_mut(w, this))[refresh_index];
        let time_remaining = refresh_entry.duration + refresh_entry.start_time;

        if duration > time_remaining {
            refresh_entry.start_time = 0.0;
            refresh_entry.duration = duration;
        }

        result.enchantment = Some(refresh_entry.clone());
    } else {
        // A new layer of a caster's built-in item spell withholds the
        // AugmentationIncreasedSpellDuration bonus, as the first layer and a refresh do (V252;
        // current ACE does the same).
        let mut new_entry = build_entry(w, this, spell, caster, weapon, equip, is_weapon_spell);
        new_entry.layer_id = result.next_layer_id();
        reg::add_enchantment(registry_mut(obj_mut(w, this)), new_entry.clone());

        result.enchantment = Some(new_entry);
    }
    set_changes_detected(w, this);

    // output message is from StackType,
    // which is the largest of the combined StackTypes

    result
}

/// Builds an enchantment registry entry from a spell ID. `caster`, `weapon`, `equip` and
/// `is_weapon_spell` default to null / null / false / false.
// ACE: EnchantmentManager.BuildEntry
#[allow(clippy::float_cmp)]
fn build_entry(
    w: &mut World,
    this: ObjectGuid,
    spell: &Spell,
    caster: Option<ObjectGuid>,
    weapon: Option<ObjectGuid>,
    equip: bool,
    is_weapon_spell: bool,
) -> PropertiesEnchantmentRegistry {
    let mut entry = PropertiesEnchantmentRegistry {
        enchantment_category: spell.meta_spell_type().0.cast_unsigned(),
        spell_id: spell.id().cast_signed(),
        spell_category: spell.category(),
        power_level: spell.power(),
        ..PropertiesEnchantmentRegistry::default()
    };

    let caster_is_creature = caster
        .and_then(|c| w.objects.get(c))
        .is_some_and(WorldObject::is_creature);

    if caster_is_creature {
        entry.duration = spell.duration();

        if let Some(aug) = caster_player_augmentation_increased_spell_duration(w, caster) {
            if !is_weapon_spell && spell.dot_duration() == 0.0 {
                entry.duration *= f64::from(1.0f32 + aug as f32 * 0.2f32);
            }
        }
    } else if !equip {
        entry.duration = spell.duration();
    } else {
        // enchantments from equipping items are active until the item is dequipped
        entry.duration = -1.0;
        entry.start_time = 0.0;
    }

    entry.caster_object_id = match caster {
        None => this.full(),
        Some(c) => c.full(),
    };

    entry.degrade_modifier = spell.degrade_modifier();
    entry.degrade_limit = spell.degrade_limit();
    entry.stat_mod_type = spell.stat_mod_type();
    entry.stat_mod_key = spell.stat_mod_key();
    entry.stat_mod_value = spell.stat_mod_val();

    if spell.is_beneficial() {
        // should "server" data be fixed or is this the better way to do this?
        entry.stat_mod_type |= EnchantmentTypeFlags::Beneficial;
    }

    if spell.is_damage_over_time() {
        let heartbeat_interval = obj(w, this).heartbeat_interval().unwrap_or(5.0);

        // scale StatModValue for HeartbeatIntervals other than the default 5s
        if heartbeat_interval != 5.0 {
            entry.stat_mod_value = spell.get_damage_per_tick(heartbeat_interval.cs_cast());
        }

        // calculate runtime StatModValue for enchantment
        if let Some(caster) = caster {
            entry.stat_mod_value = world_object_calculate_dot_enchantment_stat_mod_value(
                w,
                caster,
                spell,
                this,
                weapon,
                entry.stat_mod_value,
            );
        }
    }

    // handle equipment sets
    if let Some(caster) = caster {
        let equipment_set_id = w
            .objects
            .get(caster)
            .and_then(WorldObject::equipment_set_id);
        if let Some(equipment_set_id) = equipment_set_id {
            if world_object_item_set_contains(w, caster, spell.id()) {
                entry.has_spell_set_id = true;
                entry.spell_set_id = equipment_set_id;
            }
        }
    }

    entry
}

/// Adds a cooldown spell to the enchantment registry (base; callers use
/// [`caching::start_cooldown`]).
///
/// # Panics
/// When the manager's object is not a player with a session (ACE dereferences `Player.Session`).
// ACE: EnchantmentManager.StartCooldown
pub fn start_cooldown(w: &mut World, this: ObjectGuid, item: ObjectGuid) -> bool {
    let Some(item_obj) = w.objects.get(item) else {
        panic!("ACE: EnchantmentManager.StartCooldown: item is null (NullReferenceException)");
    };
    let Some(cooldown_id) = item_obj.cooldown_id() else {
        return false;
    };

    // TODO: BiotaPropertiesEnchantmentRegistry.SpellId should be uint
    let new_entry = PropertiesEnchantmentRegistry {
        spell_id: get_cooldown_spell_id(cooldown_id).cast_signed(),
        spell_category: SpellCategory(u32::from(SPELL_CATEGORY_COOLDOWN)),
        has_spell_set_id: true,
        duration: item_obj.cooldown_duration().unwrap_or(0.0),
        caster_object_id: item.full(),
        degrade_limit: -666.0,
        stat_mod_type: EnchantmentTypeFlags::Cooldown,
        enchantment_category: EnchantmentMask::Cooldown.0.cast_unsigned(),
        layer_id: 1, // cooldown at layer 1, any spells at layer 2?
        ..PropertiesEnchantmentRegistry::default()
    };

    reg::add_enchantment(registry_mut(obj_mut(w, this)), new_entry.clone());
    set_changes_detected(w, this);

    if !is_player(w, this) {
        panic!("ACE: EnchantmentManager.StartCooldown: Player is null (NullReferenceException)");
    }
    let session = session_of(w, this);
    let enchantment = new_enchantment(w, this, &new_entry);
    let msg = game_event_magic_update_enchantment(session_data(w, session), &enchantment);
    enqueue_send(w, session, msg);

    true
}

// ------------------------------------------------------------------------------ remove

/// Removes a spell from the enchantment registry, and sends the relevant network messages for
/// spell removal (base; callers use [`caching::remove`]). `sound` defaults to true.
// ACE: EnchantmentManager.Remove
pub fn remove(
    w: &mut World,
    this: ObjectGuid,
    entry: Option<&PropertiesEnchantmentRegistry>,
    sound: bool,
) {
    let Some(entry) = entry else { return };

    let spell_id = entry.spell_id;

    if reg::try_remove_enchantment(
        obj_mut(w, this)
            .biota
            .properties_enchantment_registry
            .as_mut(),
        entry.spell_id,
        entry.caster_object_id,
    ) {
        set_changes_detected(w, this);
    }

    if is_player(w, this) {
        // this line is to force vitae to be layer 0 to match retail pcaps. We save it as layer 1 to make EF Core happy.
        let layer = if i64::from(entry.spell_id) == i64::from(SpellId::Vitae.0) {
            0
        } else {
            entry.layer_id
        };
        let session = session_of(w, this);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // C#'s (ushort)int
        let msg = game_event_magic_remove_enchantment(
            session_data(w, session),
            entry.spell_id as u16,
            layer,
        );
        enqueue_send(w, session, msg);

        if sound && entry.spell_category != SpellCategory(u32::from(SPELL_CATEGORY_COOLDOWN)) {
            enqueue_send(
                w,
                session,
                game_message_sound(this, Sound::SpellExpire, 1.0),
            );
        }
    } else {
        let o = obj(w, this);
        let owner_id = o.owner_id().or_else(|| o.wielder_id());

        if let Some(owner_id) = owner_id {
            let owner = player_manager::get_online_player(w, owner_id);

            if let Some(owner) = owner {
                let spell = Spell::from_int(w, spell_id, true);

                let session = session_of(w, owner);
                let text = format!(
                    "The spell {} on {} has expired.",
                    spell.name(),
                    crate::dispatch::name::name(w, this).unwrap_or_default()
                );
                enqueue_send(
                    w,
                    session,
                    game_message_system_chat(&text, ChatMessageType::Magic),
                );

                if sound {
                    enqueue_send(
                        w,
                        session,
                        game_message_sound(owner, Sound::SpellExpire, 1.0),
                    );
                }
            }
        }
    }
}

/// Removes all enchantments except for vitae and item spells. Called on player death (base;
/// callers use [`caching::remove_all_enchantments`]).
// ACE: EnchantmentManager.RemoveAllEnchantments
pub fn remove_all_enchantments(w: &mut World, this: ObjectGuid) {
    // exclude cooldowns and enchantments from items
    let spells_to_exclude: Vec<i32> = registry(obj(w, this))
        .iter()
        .filter(|i| i.duration == -1.0 || i.spell_id > i32::from(i16::MAX))
        .map(|i| i.spell_id)
        .collect();

    reg::remove_all_enchantments(
        obj_mut(w, this)
            .biota
            .properties_enchantment_registry
            .as_mut(),
        &spells_to_exclude,
    );
    set_changes_detected(w, this);
}

/// Removes all enchantments except for beneficial enchantments, vitae and item spells. Called on
/// player death (base; callers use [`caching::remove_all_bad_enchantments`]).
// ACE: EnchantmentManager.RemoveAllBadEnchantments
pub fn remove_all_bad_enchantments(w: &mut World, this: ObjectGuid) {
    // exclude beneficial enchantments, cooldowns and enchantments from items
    let spells_to_exclude: Vec<i32> = registry(obj(w, this))
        .iter()
        .filter(|i| {
            has_flag(i.stat_mod_type, EnchantmentTypeFlags::Beneficial)
                || i.duration == -1.0
                || i.spell_id > i32::from(i16::MAX)
        })
        .map(|i| i.spell_id)
        .collect();

    reg::remove_all_enchantments(
        obj_mut(w, this)
            .biota
            .properties_enchantment_registry
            .as_mut(),
        &spells_to_exclude,
    );
    set_changes_detected(w, this);
}

// ------------------------------------------------------------------------------ vitae

/// Returns the vitae enchantment.
// ACE: EnchantmentManager.GetVitae
#[must_use]
pub fn get_vitae(w: &World, this: ObjectGuid) -> Option<&PropertiesEnchantmentRegistry> {
    get_enchantment(w, this, SpellId::Vitae.0, None)
}

/// Returns the minimum vitae for a player level.
// ACE: EnchantmentManager.GetMinVitae
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn get_min_vitae(w: &World, level: u32) -> f32 {
    let prop_vitae = 1.0 - property_manager_get_double(w, "vitae_penalty_max");

    let mut max_penalty = level.wrapping_sub(1).wrapping_mul(3);
    if max_penalty < 1 {
        max_penalty = 1;
    }

    let global_max = 100u32.wrapping_sub(round(prop_vitae * 100.0).cs_cast());
    if max_penalty > global_max {
        max_penalty = global_max;
    }

    let mut min_vitae = 100u32.wrapping_sub(max_penalty) as f32 / 100.0f32;
    if f64::from(min_vitae) < prop_vitae {
        min_vitae = prop_vitae.cs_cast();
    }

    min_vitae
}

/// Called on player death (base; callers use [`caching::update_vitae`]).
///
/// # Panics
/// When the player's `Level` is null (ACE: `InvalidOperationException` on `(uint)Player.Level`).
// ACE: EnchantmentManager.UpdateVitae
pub fn update_vitae(w: &mut World, this: ObjectGuid) -> f32 {
    if !is_player(w, this) {
        return 0.0;
    }

    // `!HasVitae` is a virtual call: the caching override answers.
    let vitae_index = if caching::has_vitae(w, this) {
        // update existing vitae
        let index = obj(w, this)
            .biota
            .properties_enchantment_registry
            .as_ref()
            .and_then(|l| {
                l.iter()
                    .position(|e| i64::from(e.spell_id) == i64::from(SpellId::Vitae.0))
            })
            .expect(
                "ACE: EnchantmentManager.UpdateVitae: GetVitae() is null (NullReferenceException)",
            );
        let penalty = property_manager_get_double(w, "vitae_penalty");
        registry_mut(obj_mut(w, this))[index].stat_mod_value -= CsCast::<f32>::cs_cast(penalty);
        set_changes_detected(w, this);
        index
    } else {
        // TODO refactor this so it uses the existing Add() method.

        // add entry for new vitae
        let spell = Spell::from_spell_id(w, SpellId::Vitae, true);

        let mut vitae = build_entry(w, this, &spell, None, None, false, false);
        vitae.enchantment_category = EnchantmentMask::Vitae.0.cast_unsigned();
        vitae.layer_id = 1; // This should be 0 but EF Core seems to be very unhappy with 0 as the layer id now that we're using layer as part of the composite key.
        vitae.stat_mod_value =
            1.0f32 - CsCast::<f32>::cs_cast(property_manager_get_double(w, "vitae_penalty"));
        let list = registry_mut(obj_mut(w, this));
        reg::add_enchantment(list, vitae);
        let index = list.len() - 1;
        set_changes_detected(w, this);
        index
    };

    let level = obj(w, this)
        .level()
        .expect("ACE: (uint)Player.Level on null (InvalidOperationException)");
    let min_vitae = get_min_vitae(w, level.cast_unsigned());

    let vitae = &mut registry_mut(obj_mut(w, this))[vitae_index];
    if vitae.stat_mod_value < min_vitae {
        vitae.stat_mod_value = min_vitae;
    }
    if vitae.stat_mod_value > 1.0 {
        vitae.stat_mod_value = 1.0;
    }

    vitae.stat_mod_value
}

/// Called when player crosses the VitaeCPPool threshold (base; callers use
/// [`caching::reduce_vitae`]).
///
/// # Panics
/// Without a vitae entry (ACE: `NullReferenceException`).
// ACE: EnchantmentManager.ReduceVitae
pub fn reduce_vitae(w: &mut World, this: ObjectGuid) -> f32 {
    let vitae = reg::get_enchantment_by_spell_mut(
        obj_mut(w, this)
            .biota
            .properties_enchantment_registry
            .as_mut(),
        SpellId::Vitae.0.cast_signed(),
        None,
    )
    .expect("ACE: EnchantmentManager.ReduceVitae: GetVitae() is null (NullReferenceException)");
    vitae.stat_mod_value += 0.01f32;

    if epsilon_equals(vitae.stat_mod_value, 1.0) || vitae.stat_mod_value > 1.0 {
        return 1.0;
    }

    vitae.stat_mod_value
}

/// Removes the vitae penalty for a player.
// ACE: EnchantmentManager.RemoveVitae
pub fn remove_vitae(w: &mut World, this: ObjectGuid) {
    if !is_player(w, this) {
        return;
    }

    let vitae = get_vitae(w, this).cloned();

    caching::remove(w, this, vitae.as_ref(), true);
}

// ------------------------------------------------------------------------------ dispel

/// Silently removes a spell from the enchantment registry, and sends the relevant network
/// message for dispel (base; callers use [`caching::dispel`]).
// ACE: EnchantmentManager.Dispel
pub fn dispel(w: &mut World, this: ObjectGuid, entry: Option<&PropertiesEnchantmentRegistry>) {
    let Some(entry) = entry else { return };

    if reg::try_remove_enchantment(
        obj_mut(w, this)
            .biota
            .properties_enchantment_registry
            .as_mut(),
        entry.spell_id,
        entry.caster_object_id,
    ) {
        set_changes_detected(w, this);
    }

    if is_player(w, this) {
        let session = session_of(w, this);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // C#'s (ushort)int
        let msg = game_event_magic_dispel_enchantment(
            session_data(w, session),
            entry.spell_id as u16,
            entry.layer_id,
        );
        enqueue_send(w, session, msg);
    }
}

/// Silently removes multiple spells from the enchantment registry, and sends the relevant
/// network messages for dispel (the `List` overload of `Dispel`; base, callers use
/// [`caching::dispel_list`]). `None` is ACE's `null`.
// ACE: EnchantmentManager.Dispel
pub fn dispel_list(
    w: &mut World,
    this: ObjectGuid,
    entries: Option<&[PropertiesEnchantmentRegistry]>,
) {
    let Some(entries) = entries else { return };
    if entries.is_empty() {
        return;
    }

    for entry in entries {
        if reg::try_remove_enchantment(
            obj_mut(w, this)
                .biota
                .properties_enchantment_registry
                .as_mut(),
            entry.spell_id,
            entry.caster_object_id,
        ) {
            set_changes_detected(w, this);
        }
    }
    if is_player(w, this) {
        let session = session_of(w, this);
        let msg = game_event_magic_dispel_multiple_enchantments_registry(
            session_data(w, session),
            entries,
        );
        enqueue_send(w, session, msg);
    }
}

/// Removes all enchantments from the player on server, and sends network messages to silently
/// dispel the enchantments.
// ACE: EnchantmentManager.DispelAllEnchantments
pub fn dispel_all_enchantments(w: &mut World, this: ObjectGuid) {
    let enchantments = reg::clone(obj(w, this).biota.properties_enchantment_registry.as_ref());

    caching::dispel_list(w, this, enchantments.as_deref());
}

/// Selects a list of spells to dispel. `spell` is the dispel spell.
// ACE: EnchantmentManager.SelectDispel
#[must_use]
pub fn select_dispel(w: &World, this: ObjectGuid, spell: &Spell) -> Vec<SpellEnchantment> {
    // NOTE: in the default 16PY db,
    // there are a lot of dispels where the actual #s do not match up with the spell descriptions...
    // ie. the description will say it dispels 3-6 spells, and it will only dispel 2-4 etc.

    // dispel factors:
    // min_power - the minimum power level of spell to dispel (unused?)
    // max_power - the maximum power level of spell to dispel
    // power_variance - rng for power level, unused?
    // dispel_school - the magic school to dispel, 0 if all
    // align - type of spells to dispel: positive, negative, or all
    // number - the maximum # of spells to dispel
    // number_variance - number * number_variance = the minimum # of spells to dispel
    let min_power = spell.min_power();
    let max_power = spell.max_power();
    let _power_variance = spell.power_variance();
    let dispel_school = spell.dispel_school();
    let align = spell.align();
    let number = spell.number();
    let number_variance = spell.number_variance();

    let enchantments = reg::clone(obj(w, this).biota.properties_enchantment_registry.as_ref())
        .expect("ACE: Enumerable.Where on a null list (ArgumentNullException)");

    // `uint PowerLevel` against `int` bounds compares as `long`.
    let filtered = enchantments.into_iter().filter(|e| {
        i64::from(e.power_level) >= i64::from(min_power)
            && i64::from(e.power_level) <= i64::from(max_power)
    });

    // no dispel for enchantments from item sources (and vitae)
    #[allow(clippy::float_cmp)]
    let filtered = filtered.filter(|e| e.duration != -1.0);

    // for dispelSchool and align,
    // we probably could do some calculations to figure out these values directly from the enchantments
    // but it would be far easier and more reliable to just do them through the spells
    // since dispels are not a time-critical function, this should still be fine
    let mut spells = Vec::new();
    for filter in filtered {
        let spell_enchantment = SpellEnchantment::new(w, filter);

        if !spell_enchantment.spell.not_found() {
            spells.push(spell_enchantment);
        }
    }

    let mut filter_spells = spells;
    if dispel_school != MagicSchool::None {
        filter_spells.retain(|s| s.spell.school() == dispel_school);
    }

    if align != DispelType::All {
        if align == DispelType::Positive {
            filter_spells.retain(|s| s.spell.is_beneficial());
        } else if align == DispelType::Negative {
            filter_spells.retain(|s| s.spell.is_harmful());
        }
    }

    // dispel all
    if number == -1 {
        return filter_spells;
    }

    // get number of spells to dispel
    let mut dispel_num = number;
    #[allow(clippy::float_cmp, clippy::cast_precision_loss)]
    if number_variance != 1.0 {
        let max_dispel_num = dispel_num;
        let min_dispel_num: i32 =
            round(f64::from(dispel_num as f32 * (1.0f32 - number_variance))).cs_cast();

        // factor in rng variance
        dispel_num = ThreadSafeRandom::next(min_dispel_num, max_dispel_num);
    }

    // randomize the filtered spell list
    shuffle(&mut filter_spells);

    // select the required # of spells
    filter_spells.truncate(usize::try_from(dispel_num).unwrap_or(0));
    filter_spells
}

// ------------------------------------------------------------------------------ keys

/// Gets the VitalRate key for a CreatureVital (`vital.Vital`).
// ACE: EnchantmentManager.GetVitalRateKey
#[must_use]
pub fn get_vital_rate_key(vital: PropertyAttribute2nd) -> PropertyFloat {
    match vital {
        PropertyAttribute2nd::MaxHealth => PropertyFloat::HealthRate,
        PropertyAttribute2nd::MaxStamina => PropertyFloat::StaminaRate,
        PropertyAttribute2nd::MaxMana => PropertyFloat::ManaRate,
        _ => PropertyFloat(0),
    }
}

/// Gets the ArmorModVsType key for a DamageType.
// ACE: EnchantmentManager.GetImpenBaneKey
#[must_use]
pub fn get_impen_bane_key(damage_type: DamageType) -> PropertyFloat {
    match damage_type {
        DamageType::Slash => PropertyFloat::ArmorModVsSlash,
        DamageType::Pierce => PropertyFloat::ArmorModVsPierce,
        DamageType::Bludgeon => PropertyFloat::ArmorModVsBludgeon,
        DamageType::Fire => PropertyFloat::ArmorModVsFire,
        DamageType::Cold => PropertyFloat::ArmorModVsCold,
        DamageType::Acid => PropertyFloat::ArmorModVsAcid,
        DamageType::Electric => PropertyFloat::ArmorModVsElectric,
        DamageType::Nether => PropertyFloat::ArmorModVsNether,
        _ => PropertyFloat(0),
    }
}

/// Gets the resistance PropertyFloat for a DamageType.
// ACE: EnchantmentManager.GetResistanceKey
#[must_use]
pub fn get_resistance_key(damage_type: DamageType) -> PropertyFloat {
    match damage_type {
        DamageType::Slash => PropertyFloat::ResistSlash,
        DamageType::Pierce => PropertyFloat::ResistPierce,
        DamageType::Bludgeon => PropertyFloat::ResistBludgeon,
        DamageType::Fire => PropertyFloat::ResistFire,
        DamageType::Cold => PropertyFloat::ResistCold,
        DamageType::Acid => PropertyFloat::ResistAcid,
        DamageType::Electric => PropertyFloat::ResistElectric,
        DamageType::Nether => PropertyFloat::ResistNether,
        _ => PropertyFloat(0),
    }
}

// ------------------------------------------------------------------------------ aggregations
// Base implementations of the virtual getters; callers use the caching overrides.

/// Returns the additive modifiers to an attribute from enchantments.
// ACE: EnchantmentManager.GetAttributeMod_Additive
#[must_use]
pub fn get_attribute_mod_additive(
    w: &World,
    this: ObjectGuid,
    attribute: PropertyAttribute,
) -> i32 {
    let enchantments = get_enchantments_top_layer_key(
        w,
        this,
        EnchantmentTypeFlags::Attribute | EnchantmentTypeFlags::Additive,
        u32::from(attribute.0),
        true,
    );

    let mut attribute_mod = 0i32;
    for enchantment in enchantments {
        attribute_mod = attribute_mod.wrapping_add(to_int(enchantment.stat_mod_value));
    }

    attribute_mod
}

/// Returns the multiplicative modifiers to an attribute from enchantments.
// ACE: EnchantmentManager.GetAttributeMod_Multiplier
#[must_use]
pub fn get_attribute_mod_multiplier(
    w: &World,
    this: ObjectGuid,
    attribute: PropertyAttribute,
) -> f32 {
    let enchantments = get_enchantments_top_layer_key(
        w,
        this,
        EnchantmentTypeFlags::Attribute | EnchantmentTypeFlags::Multiplicative,
        u32::from(attribute.0),
        true,
    );

    let mut multiplier = 1.0f32;
    for enchantment in enchantments {
        multiplier *= enchantment.stat_mod_value;
    }

    multiplier
}

/// Gets the additive modifiers to a vital / secondary attribute. `vital` is ACE's
/// `CreatureVital vital` as its `vital.Vital`, the only member read.
// ACE: EnchantmentManager.GetVitalMod_Additives
#[must_use]
pub fn get_vital_mod_additives(w: &World, this: ObjectGuid, vital: PropertyAttribute2nd) -> f32 {
    let enchantments = get_enchantments_top_layer_key(
        w,
        this,
        EnchantmentTypeFlags::SecondAtt | EnchantmentTypeFlags::Additive,
        u32::from(vital.0),
        true,
    );

    // additive
    let mut modifier = 0.0f32;
    for enchantment in enchantments {
        modifier += enchantment.stat_mod_value;
    }

    modifier
}

/// Gets the multiplicative modifiers to a vital / secondary attribute (`vital.Vital`).
// ACE: EnchantmentManager.GetVitalMod_Multiplier
#[must_use]
pub fn get_vital_mod_multiplier(w: &World, this: ObjectGuid, vital: PropertyAttribute2nd) -> f32 {
    // multiplicatives (asheron's lesser benediction)
    let enchantments = get_enchantments_top_layer_key(
        w,
        this,
        EnchantmentTypeFlags::SecondAtt | EnchantmentTypeFlags::Multiplicative,
        u32::from(vital.0),
        true,
    );

    let mut multiplier = 1.0f32;
    for enchantment in enchantments {
        multiplier *= enchantment.stat_mod_value;
    }

    multiplier
}

/// Returns the additive bonus from XP enchantments, such as Augmented Understanding.
// ACE: EnchantmentManager.GetXPBonus
#[must_use]
pub fn get_xp_bonus(w: &World, this: ObjectGuid) -> f32 {
    let enchantments = get_enchantments_category(w, this, SpellCategory::TrinketXPRaising);

    // TODO: temporary code to handle both additive and multiplicative mods
    // should be additive in database, update when everything is in sync
    let mut modifier = 0.0f32;

    // `OrderByDescending(i => i.PowerLevel).Take(1)`: the first of the highest power.
    let mut best: Option<&PropertiesEnchantmentRegistry> = None;
    for e in enchantments {
        if best.is_none_or(|b| e.power_level > b.power_level) {
            best = Some(e);
        }
    }
    if let Some(enchantment) = best {
        if has_flag(
            enchantment.stat_mod_type,
            EnchantmentTypeFlags::Multiplicative,
        ) {
            modifier += enchantment.stat_mod_value - 1.0;
        } else {
            modifier += enchantment.stat_mod_value;
        }
    }
    modifier
}

/// Returns the additive modifiers to a skill from enchantments.
// ACE: EnchantmentManager.GetSkillMod_Additives
#[must_use]
pub fn get_skill_mod_additives(w: &World, this: ObjectGuid, skill: Skill) -> i32 {
    let enchantments = get_enchantments_top_layer_key(
        w,
        this,
        EnchantmentTypeFlags::Skill | EnchantmentTypeFlags::Additive,
        skill.0.cast_unsigned(),
        true,
    );

    let mut skill_mod = 0i32;
    for enchantment in enchantments {
        skill_mod = skill_mod.wrapping_add(to_int(enchantment.stat_mod_value));
    }

    if skill_helper::DEFENSE_SKILLS.contains(&skill) {
        skill_mod = skill_mod.wrapping_add(get_defense_debuff_mod(w, this));
    }

    if skill_helper::ATTACK_SKILLS.contains(&skill) {
        skill_mod = skill_mod.wrapping_add(get_attack_debuff_mod(w, this));
    }

    skill_mod
}

/// Returns the multiplicative modifiers to a skill from enchantments.
// ACE: EnchantmentManager.GetSkillMod_Multiplier
#[must_use]
pub fn get_skill_mod_multiplier(w: &World, this: ObjectGuid, skill: Skill) -> f32 {
    // shroud spells
    let enchantments = get_enchantments_top_layer_key(
        w,
        this,
        EnchantmentTypeFlags::Skill | EnchantmentTypeFlags::Multiplicative,
        skill.0.cast_unsigned(),
        true,
    );

    let mut multiplier = 1.0f32;
    for enchantment in enchantments {
        multiplier *= enchantment.stat_mod_value;
    }

    multiplier
}

/// Returns the sum of the StatModValues for an EnchantmentTypeFlag (`positive` defaults to
/// null).
// ACE: EnchantmentManager.GetModifier
#[must_use]
pub fn get_modifier(
    w: &World,
    this: ObjectGuid,
    type_: EnchantmentTypeFlags,
    positive: Option<bool>,
) -> i32 {
    let enchantments = get_enchantments_top_layer(w, this, type_);

    let mut modifier = 0i32;
    for enchantment in enchantments {
        let stat_mod_val = to_int(enchantment.stat_mod_value);

        let take = match positive {
            None => true,
            Some(p) => p && stat_mod_val > 0 || !p && stat_mod_val < 0,
        };
        if take {
            modifier = modifier.wrapping_add(stat_mod_val);
        }
    }
    modifier
}

/// Returns the sum of the modifiers for a StatModKey (the `PropertyInt` overload of
/// `GetAdditiveMod`).
// ACE: EnchantmentManager.GetAdditiveMod
#[must_use]
pub fn get_additive_mod_int(w: &World, this: ObjectGuid, stat_mod_key: PropertyInt) -> i32 {
    let enchantments = get_enchantments_top_layer_key(
        w,
        this,
        EnchantmentTypeFlags::Additive,
        u32::from(stat_mod_key.0),
        false,
    );

    let mut modifier = 0i32;
    for enchantment in enchantments
        .into_iter()
        .filter(|e| (e.stat_mod_type & EnchantmentTypeFlags::Skill).0 == 0)
    {
        modifier = modifier.wrapping_add(to_int(enchantment.stat_mod_value));
    }

    modifier
}

/// Returns the sum of the enchantment statmod values (the list overload of `GetAdditiveMod`).
// ACE: EnchantmentManager.GetAdditiveMod
#[must_use]
pub fn get_additive_mod_list(enchantments: &[&PropertiesEnchantmentRegistry]) -> f32 {
    let mut modifier = 0.0f32;
    for enchantment in enchantments {
        modifier += enchantment.stat_mod_value;
    }

    modifier
}

/// The `PropertyFloat` overload of `GetAdditiveMod`.
// ACE: EnchantmentManager.GetAdditiveMod
#[must_use]
pub fn get_additive_mod_float(w: &World, this: ObjectGuid, stat_mod_key: PropertyFloat) -> f32 {
    let type_flags = EnchantmentTypeFlags::Float
        | EnchantmentTypeFlags::SingleStat
        | EnchantmentTypeFlags::Additive;

    let enchantments =
        get_enchantments_top_layer_key(w, this, type_flags, u32::from(stat_mod_key.0), false);

    let mut modifier = 0.0f32;
    for enchantment in enchantments {
        modifier += enchantment.stat_mod_value;
    }

    modifier
}

/// Returns the product of the modifiers for a StatModKey.
// ACE: EnchantmentManager.GetMultiplicativeMod
#[must_use]
pub fn get_multiplicative_mod(w: &World, this: ObjectGuid, stat_mod_key: PropertyFloat) -> f32 {
    let type_flags = EnchantmentTypeFlags::Float
        | EnchantmentTypeFlags::SingleStat
        | EnchantmentTypeFlags::Multiplicative;

    let enchantments =
        get_enchantments_top_layer_key(w, this, type_flags, u32::from(stat_mod_key.0), false);

    // multiplicative
    let mut modifier = 1.0f32;
    for enchantment in enchantments {
        modifier *= enchantment.stat_mod_value;
    }

    modifier
}

/// Returns the base armor modifier from enchantments.
// ACE: EnchantmentManager.GetBodyArmorMod
#[must_use]
pub fn get_body_armor_mod(w: &World, this: ObjectGuid) -> i32 {
    get_modifier(w, this, EnchantmentTypeFlags::BodyArmorValue, None)
}

/// Returns either the positive body armor from life spells (ie. Armor Self) or the negative
/// body armor (ie. Imperil): the `bool` overload of `GetBodyArmorMod`, which the caching class
/// does not override.
// ACE: EnchantmentManager.GetBodyArmorMod
#[must_use]
pub fn get_body_armor_mod_positive(w: &World, this: ObjectGuid, positive: bool) -> i32 {
    get_modifier(
        w,
        this,
        EnchantmentTypeFlags::BodyArmorValue,
        Some(positive),
    )
}

fn float_single_multiplicative(
    w: &World,
    this: ObjectGuid,
    key: PropertyFloat,
) -> Vec<&PropertiesEnchantmentRegistry> {
    let type_flags = EnchantmentTypeFlags::Float
        | EnchantmentTypeFlags::SingleStat
        | EnchantmentTypeFlags::Multiplicative;
    get_enchantments_top_layer_key(w, this, type_flags, u32::from(key.0), false)
}

/// Gets the resistance modifier for a damage type.
// ACE: EnchantmentManager.GetResistanceMod
#[must_use]
pub fn get_resistance_mod(w: &World, this: ObjectGuid, damage_type: DamageType) -> f32 {
    let resistance = get_resistance_key(damage_type);
    let enchantments = float_single_multiplicative(w, this, resistance);

    // multiplicative
    let mut modifier = 1.0f32;
    for enchantment in enchantments {
        modifier *= enchantment.stat_mod_value;
    }

    modifier
}

/// Gets the resistance modifier for a damage type (protections only).
// ACE: EnchantmentManager.GetProtectionResistanceMod
#[must_use]
pub fn get_protection_resistance_mod(w: &World, this: ObjectGuid, damage_type: DamageType) -> f32 {
    let resistance = get_resistance_key(damage_type);
    let enchantments = float_single_multiplicative(w, this, resistance);

    // multiplicative
    let mut modifier = 1.0f32;
    for enchantment in enchantments {
        if enchantment.stat_mod_value < 1.0 {
            modifier *= enchantment.stat_mod_value;
        }
    }

    modifier
}

/// Gets the resistance modifier for a damage type (vulnerabilities only).
// ACE: EnchantmentManager.GetVulnerabilityResistanceMod
#[must_use]
pub fn get_vulnerability_resistance_mod(
    w: &World,
    this: ObjectGuid,
    damage_type: DamageType,
) -> f32 {
    let resistance = get_resistance_key(damage_type);
    let enchantments = float_single_multiplicative(w, this, resistance);

    // multiplicative
    let mut modifier = 1.0f32;
    for enchantment in enchantments {
        if enchantment.stat_mod_value > 1.0 {
            modifier *= enchantment.stat_mod_value;
        }
    }

    modifier
}

/// Gets the regeneration modifier for a vital type (regeneration / rejuvenation / mana renewal);
/// `vital` is `vital.Vital`.
// ACE: EnchantmentManager.GetRegenerationMod
#[must_use]
pub fn get_regeneration_mod(w: &World, this: ObjectGuid, vital: PropertyAttribute2nd) -> f32 {
    let vital_key = get_vital_rate_key(vital);
    let enchantments = float_single_multiplicative(w, this, vital_key);

    // multiplicative
    let mut modifier = 1.0f32;
    for enchantment in enchantments {
        modifier *= enchantment.stat_mod_value;
    }

    modifier
}

/// Returns the weapon damage bonus, ie. Blood Drinker.
// ACE: EnchantmentManager.GetDamageBonus
#[must_use]
pub fn get_damage_bonus(w: &World, this: ObjectGuid) -> i32 {
    let damage_mod = get_additive_mod_int(w, this, PropertyInt::Damage);
    let aura_damage_mod = get_additive_mod_int(w, this, PropertyInt::WeaponAuraDamage);

    // there is an unfortunate situation in the spell db,
    // where blood drinker 1-7 are defined as PropertyInt.Damage
    // (possibly from also being cast as direct item spells elsewhere?)
    // and blood drinker 8 is properly defined as aura...

    aura_damage_mod.wrapping_add(damage_mod)
}

/// Returns the DamageMod for bow / crossbow.
// ACE: EnchantmentManager.GetDamageMod
#[must_use]
pub fn get_damage_mod(w: &World, this: ObjectGuid) -> f32 {
    get_additive_mod_float(w, this, PropertyFloat::DamageMod)
}

/// Returns the attack skill modifier, ie. Heart Seeker.
// ACE: EnchantmentManager.GetAttackMod
#[must_use]
pub fn get_attack_mod(w: &World, this: ObjectGuid) -> f32 {
    let offense_mod = get_additive_mod_float(w, this, PropertyFloat::WeaponOffense);
    let aura_offense_mod = get_additive_mod_float(w, this, PropertyFloat::WeaponAuraOffense);

    aura_offense_mod + offense_mod
}

/// Returns the weapon speed modifier, ie. Swift Killer.
// ACE: EnchantmentManager.GetWeaponSpeedMod
#[must_use]
pub fn get_weapon_speed_mod(w: &World, this: ObjectGuid) -> i32 {
    let speed_mod = get_additive_mod_int(w, this, PropertyInt::WeaponTime);
    let aura_speed_mod = get_additive_mod_int(w, this, PropertyInt::WeaponAuraSpeed);

    aura_speed_mod.wrapping_add(speed_mod)
}

/// Not ACE: the product of the multiplicative weapon-speed enchantments on this object
/// (Rockslide and its kin), 1 when there are none. ACE's weapon speed reads only the additive
/// ones, so a multiplicative one changes nothing there.
// Rules ported from ClassicACE (bDekaru), AGPL-3.0: Source/ACE.Server/WorldObjects/Managers/EnchantmentManager.cs
#[must_use]
pub fn get_weapon_multiplicative_speed_mod(w: &World, this: ObjectGuid) -> f32 {
    let enchantments = get_enchantments_top_layer_key(
        w,
        this,
        EnchantmentTypeFlags::Multiplicative,
        u32::from(PropertyInt::WeaponTime.0),
        false,
    );

    let mut modifier = 1.0f32;
    for enchantment in enchantments
        .into_iter()
        .filter(|e| (e.stat_mod_type & EnchantmentTypeFlags::Skill).0 == 0)
    {
        modifier *= enchantment.stat_mod_value;
    }

    modifier
}

/// Returns the defense skill modifier, ie. Defender.
// ACE: EnchantmentManager.GetDefenseMod
#[must_use]
pub fn get_defense_mod(w: &World, this: ObjectGuid) -> f32 {
    let defense_mod = get_additive_mod_float(w, this, PropertyFloat::WeaponDefense);
    let aura_defense_mod = get_additive_mod_float(w, this, PropertyFloat::WeaponAuraDefense);

    aura_defense_mod + defense_mod
}

/// Returns the mana conversion bonus modifier, ie. Hermetic Link / Void.
// ACE: EnchantmentManager.GetManaConvMod
#[must_use]
pub fn get_mana_conv_mod(w: &World, this: ObjectGuid) -> f32 {
    let mana_conv_mod = get_multiplicative_mod(w, this, PropertyFloat::ManaConversionMod);
    let mana_conv_aura_mod = get_multiplicative_mod(w, this, PropertyFloat::WeaponAuraManaConv);

    mana_conv_aura_mod * mana_conv_mod
}

/// Returns the elemental damage bonus modifier, ie. Spirit Drinker / Loather.
// ACE: EnchantmentManager.GetElementalDamageMod
#[must_use]
pub fn get_elemental_damage_mod(w: &World, this: ObjectGuid) -> f32 {
    let elemental_damage_mod = get_additive_mod_float(w, this, PropertyFloat::ElementalDamageMod);
    let elemental_damage_aura_mod =
        get_additive_mod_float(w, this, PropertyFloat::WeaponAuraElemental);

    elemental_damage_aura_mod + elemental_damage_mod
}

/// Returns the weapon damage variance modifier.
// ACE: EnchantmentManager.GetVarianceMod
#[must_use]
pub fn get_variance_mod(w: &World, this: ObjectGuid) -> f32 {
    get_multiplicative_mod(w, this, PropertyFloat::DamageVariance)
}

/// Returns the additive armor level modifier, ie. Impenetrability.
// ACE: EnchantmentManager.GetArmorMod
#[must_use]
pub fn get_armor_mod(w: &World, this: ObjectGuid) -> i32 {
    get_additive_mod_int(w, this, PropertyInt::ArmorLevel)
}

/// Gets the additive armor level vs type modifier, ie. banes.
// ACE: EnchantmentManager.GetArmorModVsType
#[must_use]
pub fn get_armor_mod_vs_type(w: &World, this: ObjectGuid, damage_type: DamageType) -> f32 {
    let type_flags = EnchantmentTypeFlags::Float
        | EnchantmentTypeFlags::SingleStat
        | EnchantmentTypeFlags::Additive;
    let key = get_impen_bane_key(damage_type);
    let enchantments = get_enchantments_top_layer_key(w, this, type_flags, u32::from(key.0), false);

    // additive
    let mut modifier = 0.0f32;
    for enchantment in enchantments {
        modifier += enchantment.stat_mod_value;
    }

    modifier
}

/// Returns the defense skill debuffs for Dirty Fighting.
// ACE: EnchantmentManager.GetDefenseDebuffMod
#[must_use]
pub fn get_defense_debuff_mod(w: &World, this: ObjectGuid) -> i32 {
    let type_flags = EnchantmentTypeFlags::Skill
        | EnchantmentTypeFlags::Additive
        | EnchantmentTypeFlags::DefenseSkills;
    let enchantments = get_enchantments_top_layer_key(w, this, type_flags, 0, false);

    // additive
    round(f64::from(get_additive_mod_list(&enchantments))).cs_cast()
}

/// Returns the attack skill debuffs for Dirty Fighting.
// ACE: EnchantmentManager.GetAttackDebuffMod
#[must_use]
pub fn get_attack_debuff_mod(w: &World, this: ObjectGuid) -> i32 {
    let type_flags = EnchantmentTypeFlags::Skill
        | EnchantmentTypeFlags::Additive
        | EnchantmentTypeFlags::AttackSkills;
    let enchantments = get_enchantments_top_layer_key(w, this, type_flags, 0, false);

    // additive
    round(f64::from(get_additive_mod_list(&enchantments))).cs_cast()
}

/// Returns the ResistLockpick enchantment additives, ie. Strengthen/Weaken Lock.
// ACE: EnchantmentManager.GetResistLockpick
#[must_use]
pub fn get_resist_lockpick(w: &World, this: ObjectGuid) -> i32 {
    get_additive_mod_int(w, this, PropertyInt::ResistLockpick)
}

/// Returns a rating enchantment modifier for `property`.
// ACE: EnchantmentManager.GetRating
#[must_use]
pub fn get_rating(w: &World, this: ObjectGuid, property: PropertyInt) -> i32 {
    let type_flags = EnchantmentTypeFlags::Int
        | EnchantmentTypeFlags::SingleStat
        | EnchantmentTypeFlags::Additive;
    let enchantments =
        get_enchantments_top_layer_key(w, this, type_flags, u32::from(property.0), false);

    round(f64::from(get_additive_mod_list(&enchantments))).cs_cast()
}

// ACE: EnchantmentManager.GetNetherDotDamageRating
#[must_use]
pub fn get_nether_dot_damage_rating(w: &World, this: ObjectGuid) -> i32 {
    let type_ = EnchantmentTypeFlags::Int
        | EnchantmentTypeFlags::SingleStat
        | EnchantmentTypeFlags::Additive;
    let nether_dots = get_enchantments_top_layer_key(
        w,
        this,
        type_,
        u32::from(PropertyInt::NetherOverTime.0),
        false,
    );

    // this function produces a similar value to the original ACE function,
    // but is using the actual retail calculation method
    let mut total_base_damage = 0.0f32;
    for nether_dot in nether_dots {
        // normally we could just use netherDot.StatModValue here,
        // but in case WorldObject has a non-default HeartbeatInterval,
        // we want this value to still be based on the damage per default heartbeat interval
        total_base_damage += get_damage_per_tick(w, this, nether_dot, Some(5.0));
    }
    // thanks to Xenocide for this formula!
    round(f64::from(total_base_damage / 8.0f32)).cs_cast()
}

/// Returns the damage over time (DoT) enchantment mod.
// ACE: EnchantmentManager.GetDamageOverTimeMod
#[must_use]
pub fn get_damage_over_time_mod(w: &World, this: ObjectGuid) -> f32 {
    let type_flags = EnchantmentTypeFlags::Int
        | EnchantmentTypeFlags::SingleStat
        | EnchantmentTypeFlags::Additive;
    let enchantments = get_enchantments_top_layer_key(
        w,
        this,
        type_flags,
        u32::from(PropertyInt::DamageOverTime.0),
        false,
    );

    // additive float
    get_additive_mod_list(&enchantments)
}

// ------------------------------------------------------------------------------ cooldowns

/// Adds 0x8000 to the sharedCooldownID.
// ACE: EnchantmentManager.GetCooldownSpellID
#[must_use]
pub fn get_cooldown_spell_id(shared_cooldown_id: i32) -> u32 {
    (i32::from(SPELL_CATEGORY_COOLDOWN) | shared_cooldown_id).cast_unsigned()
}

/// Returns the seconds until this item's cooldown expires.
// ACE: EnchantmentManager.GetCooldown
#[must_use]
pub fn get_cooldown(w: &World, this: ObjectGuid, shared_cooldown_id: i32) -> f32 {
    let cooldown_spell_id = get_cooldown_spell_id(shared_cooldown_id);

    let cooldown = get_enchantment(w, this, cooldown_spell_id, None);

    match cooldown {
        Some(cooldown) => (cooldown.duration - cooldown.start_time.abs()).cs_cast(),
        None => 0.0,
    }
}

/// Returns TRUE if this item can be activated at this time.
// ACE: EnchantmentManager.CheckCooldown
#[must_use]
#[allow(clippy::float_cmp)]
pub fn check_cooldown(w: &World, this: ObjectGuid, shared_cooldown_id: Option<i32>) -> bool {
    let Some(shared_cooldown_id) = shared_cooldown_id else {
        return true;
    };

    get_cooldown(w, this, shared_cooldown_id) == 0.0
}

// ------------------------------------------------------------------------------ heartbeat

/// Called every ~5 seconds for active object: ticks DoTs and HoTs, then counts every entry's
/// `StartTime` down by `heartbeat_interval` and removes the expired ones.
// ACE: EnchantmentManager.HeartBeat
pub fn heart_beat(w: &mut World, this: ObjectGuid, heartbeat_interval: f64) {
    let top_layer_enchantments: Vec<PropertiesEnchantmentRegistry> = {
        reg::get_enchantments_top_layer(obj(w, this).biota.properties_enchantment_registry.as_ref())
            .expect("ACE: foreach over a null list (NullReferenceException)")
            .into_iter()
            .cloned()
            .collect()
    };

    heart_beat_damage_over_time(w, this, &top_layer_enchantments);

    let expired = reg::heart_beat_enchantments_and_return_expired(
        obj_mut(w, this)
            .biota
            .properties_enchantment_registry
            .as_mut(),
        heartbeat_interval,
    )
    .expect("ACE: foreach over a null list (NullReferenceException)");

    for enchantment in &expired {
        // `Remove(enchantment)` is virtual: the caching override.
        caching::remove(w, this, Some(enchantment), true);
    }
}

/// Applies damage from DoTs every ~5 seconds. `enchantments` are the active enchantments at the
/// top layers.
// ACE: EnchantmentManager.HeartBeat_DamageOverTime
pub fn heart_beat_damage_over_time(
    w: &mut World,
    this: ObjectGuid,
    enchantments: &[PropertiesEnchantmentRegistry],
) {
    let mut dots = Vec::new();
    let mut nether_dots = Vec::new();
    let mut aetheria_dots = Vec::new();
    let mut heals = Vec::new();

    for enchantment in enchantments {
        // combine DoTs from multiple sources
        if enchantment.stat_mod_key == u32::from(PropertyInt::DamageOverTime.0) {
            if enchantment.spell_category == SpellCategory::AetheriaProcDamageOverTimeRaising {
                aetheria_dots.push(enchantment.clone());
            } else {
                dots.push(enchantment.clone());
            }
        } else if enchantment.stat_mod_key == u32::from(PropertyInt::NetherOverTime.0) {
            nether_dots.push(enchantment.clone());
        } else if enchantment.stat_mod_key == u32::from(PropertyInt::HealOverTime.0) {
            heals.push(enchantment.clone());
        }
    }

    // apply damage over time (DoTs)
    if !dots.is_empty() {
        apply_damage_tick(w, this, &dots, DamageType::Undef, false);
    }

    if !nether_dots.is_empty() {
        apply_damage_tick(w, this, &nether_dots, DamageType::Nether, false);
    }

    if !aetheria_dots.is_empty() {
        apply_damage_tick(w, this, &aetheria_dots, DamageType::Undef, true);
    }

    // apply healing over time (HoTs)
    if !heals.is_empty() {
        apply_healing_tick(w, this, &heals);
    }
}

/// `WorldObject is Creature creature && !creature.IsDead` (the creature, when alive).
fn living_creature(w: &World, this: ObjectGuid) -> Option<ObjectGuid> {
    let o = w.objects.get(this)?;
    (o.is_creature() && !crate::world_objects::monster_combat::is_dead(o)).then_some(this)
}

/// Applies 1 tick of healing from HoT spells.
// ACE: EnchantmentManager.ApplyHealingTick
pub fn apply_healing_tick(
    w: &mut World,
    this: ObjectGuid,
    enchantments: &[PropertiesEnchantmentRegistry],
) {
    use crate::entity::damage_history;

    let Some(creature) = living_creature(w, this) else {
        return;
    };

    // get the total tick amount
    let mut tick_amount_total = 0.0f32;
    for enchantment in enchantments {
        //var totalAmount = enchantment.StatModValue;
        //var totalTicks = GetNumTicks(enchantment);
        let tick_amount = enchantment.stat_mod_value;

        tick_amount_total += tick_amount;
    }

    // apply healing ratings?
    tick_amount_total *= crate::world_objects::creature_rating::get_healing_rating_mod(w, creature);

    // do healing
    let health = obj(w, creature).health();
    let delta: i32 = round(f64::from(tick_amount_total)).cs_cast();
    let heal_amount =
        crate::world_objects::creature_vitals::update_vital_delta(w, creature, health, delta);

    // account for negative HealOverTime spells, such as 5172 - Spectral Fountain Sip
    if heal_amount >= 0 {
        damage_history::on_heal(w, creature, heal_amount.cs_cast());
    } else {
        damage_history::add(
            w,
            creature,
            creature,
            DamageType::Health,
            heal_amount.wrapping_neg().cs_cast(),
        );
    }

    if is_player(w, creature) {
        let msg = format!(
            "You receive {} points of periodic {}.",
            heal_amount.wrapping_abs(),
            if heal_amount >= 0 { "healing" } else { "harm" }
        );
        let msg_type =
            if crate::managers::property_manager::get_bool(w, "aetheria_heal_color", false, true)
                .item
            {
                ChatMessageType::Broadcast
            } else {
                ChatMessageType::Combat
            };
        crate::world_objects::player::send_message(w, creature, &msg, msg_type);
    }

    if crate::world_objects::monster_combat::is_dead(obj(w, creature)) {
        let last_damager = damage_history::of(w, creature).last_damager();
        crate::dispatch::on_death::on_death(w, creature, last_damager, DamageType::Health, false);
        crate::world_objects::creature_death::die(w, creature);
    }
}

/// Applies 1 tick of damage from a DoT spell (`aetheria` defaults to false).
// ACE: EnchantmentManager.ApplyDamageTick
pub fn apply_damage_tick(
    w: &mut World,
    this: ObjectGuid,
    enchantments: &[PropertiesEnchantmentRegistry],
    damage_type: DamageType,
    aetheria: bool,
) {
    use crate::entity::damage_history;
    use crate::world_objects::{creature_properties, creature_rating};

    let Some(creature) = living_creature(w, this) else {
        return;
    };

    let mut is_dead = false;
    // `Dictionary<WorldObject, float>`: insertion order, nothing is removed
    let mut damagers: Vec<(ObjectGuid, f32)> = Vec::new();

    let target_player = is_player(w, this).then_some(this);

    // get the total tick amount
    let mut tick_amount_total = 0.0f32;
    for enchantment in enchantments {
        //var totalAmount = enchantment.StatModValue;
        //var totalTicks = GetNumTicks(enchantment);
        let mut tick_amount = enchantment.stat_mod_value;

        // run tick amount through damage calculation functions?
        // it appears retail might have done an initial damage calc,
        // and then applied that to the enchantment StatModVal beforehand
        // for each damage tick, this pre-calc would then be multiplied
        // against the realtime resistances

        let damager = obj(w, this).current_landblock.and_then(|lb| {
            crate::entity::landblock::get_object(
                w,
                lb,
                ObjectGuid::new(enchantment.caster_object_id),
                true,
            )
        });
        let Some(damager) = damager else {
            //Console.WriteLine($"{WorldObject.Name}.ApplyDamageTick() - couldn't find damager {enchantment.CasterObjectId:X8}");
            continue;
        };

        let mut resistance_mod = creature_properties::get_resistance_mod_damage(
            w,
            creature,
            damage_type,
            Some(damager),
            None,
            1.0,
        );

        let source_player = is_player(w, damager).then_some(damager);

        if let (Some(_), Some(target_player)) = (source_player, target_player) {
            // if a PKType with Enduring Enchantment has died, ensure they don't continue to take DoT from PK sources
            if !crate::world_objects::player_combat::is_pk_type(w, target_player) {
                continue;
            }

            // void spell projectile direct damage was modified to apply this pvp modifier *on top of* the player's natural resistance to nether,
            // which supposedly brings the direct damage from void spells in pvp closer to retail

            // however, dots were already supposedly on par, so we replace resistanceMod with void_pvp_modifier for dots,
            // instead of applying it on top like direct damage

            if damage_type == DamageType::Nether {
                resistance_mod = crate::managers::property_manager::get_double(
                    w,
                    "void_pvp_modifier",
                    0.0,
                    true,
                )
                .item
                .cs_cast();
            }
        }

        // with the halvening, this actually seems like the fairest balance currently..
        let use_nether_dot_damage_rating = target_player.is_some();

        let mut damage_resist_rating_mod = creature_rating::get_damage_resist_rating_mod(
            w,
            creature,
            Some(crate::world_objects::creature_combat::CombatType::Magic),
            use_nether_dot_damage_rating,
        ); // df?

        if let (Some(_), Some(target_player)) = (source_player, target_player) {
            let pk_damage_resist_rating_mod = creature_rating::get_negative_rating_mod(
                creature_rating::get_pk_damage_resist_rating(w, target_player),
                false,
            );

            damage_resist_rating_mod = creature_rating::additive_combine(&[
                damage_resist_rating_mod,
                pk_damage_resist_rating_mod,
            ]);
        }

        // should this be here, or somewhere else?
        // should this affect NetherDotDamageRating?
        let dot_resist_rating_mod = creature_rating::get_negative_rating_mod(
            creature_rating::get_dot_resistance_rating(w, creature),
            false,
        );

        //Console.WriteLine("DR: " + Creature.ModToRating(damageRatingMod));
        //Console.WriteLine("DRR: " + Creature.NegativeModToRating(damageResistRatingMod));
        //Console.WriteLine("NRR: " + Creature.NegativeModToRating(netherResistRatingMod));

        tick_amount *= resistance_mod * damage_resist_rating_mod * dot_resist_rating_mod;

        // make sure the target's current health is not exceeded
        let health_current: f32 = {
            let o = obj(w, creature);
            o.health().current(o).cs_cast()
        };
        if tick_amount_total + tick_amount >= health_current {
            tick_amount = health_current - tick_amount_total;
            is_dead = true;
        }

        match damagers.iter_mut().find(|(g, _)| *g == damager) {
            Some((_, v)) => *v += tick_amount,
            None => damagers.push((damager, tick_amount)),
        }

        let amount: u32 = round(f64::from(tick_amount)).cs_cast();
        damage_history::add(w, creature, damager, damage_type, amount);

        tick_amount_total += tick_amount;

        if is_dead {
            break;
        }
    }

    // No damage to an invincible creature.
    if !obj(w, creature).invincible() {
        crate::dispatch::take_damage_over_time::take_damage_over_time(
            w,
            creature,
            tick_amount_total,
            damage_type,
        );
    }

    if w.objects
        .get(creature)
        .is_none_or(crate::world_objects::monster_combat::is_dead)
    {
        return;
    }

    for (damager, amount) in damagers {
        let amount = if obj(w, creature).invincible() {
            0.0
        } else {
            amount
        };

        if is_player(w, damager) {
            let damage_source_player = damager;
            crate::world_objects::monster_combat::take_damage_over_time_notify_source(
                w,
                creature,
                damage_source_player,
                damage_type,
                amount,
                aetheria,
            );

            if w.objects
                .get(creature)
                .is_some_and(|o| !crate::world_objects::monster_combat::is_dead(o))
            {
                crate::world_objects::managers::emote_manager::on_damage(
                    w,
                    creature,
                    Some(damage_source_player),
                );
            }
        }
    }
}

// ------------------------------------------------------------------------------ network

/// Writes the EnchantmentRegistry to the network stream.
// ACE: EnchantmentManager.SendRegistry
#[allow(clippy::ptr_arg)]
pub fn send_registry(w: &mut World, this: ObjectGuid, writer: &mut Vec<u8>) {
    if !is_player(w, this) {
        return;
    }
    let registry =
        crate::network::structure::enchantment_registry::enchantment_registry_new(w, this);
    crate::network::structure::enchantment_registry::write(writer, &registry);
}

/// Writes UpdateEnchantment vitae to the network stream.
// ACE: EnchantmentManager.SendUpdateVitae
pub fn send_update_vitae(w: &mut World, this: ObjectGuid) {
    if !is_player(w, this) {
        return;
    }
    let vitae = get_vitae(w, this)
        .cloned()
        .expect("ACE: new Enchantment(Player, null) (NullReferenceException)");
    let vitae = new_enchantment(w, this, &vitae);
    let session = session_of(w, this);
    let msg = game_event_magic_update_enchantment(session_data(w, session), &vitae);
    enqueue_send(w, session, msg);
}

// ------------------------------------------------------------------------------ DoT maths

/// Returns the number of ticks for a DoT enchantment (`heartbeatInterval` defaults to null: the
/// object's `HeartbeatInterval ?? 5.0`).
// ACE: EnchantmentManager.GetNumTicks
#[must_use]
pub fn get_num_ticks(
    w: &World,
    this: ObjectGuid,
    enchantment: &PropertiesEnchantmentRegistry,
    heartbeat_interval: Option<f64>,
) -> i32 {
    // assumed to be DoT enchantment
    let heartbeat_interval =
        heartbeat_interval.unwrap_or_else(|| obj(w, this).heartbeat_interval().unwrap_or(5.0));

    // it's possible retail had a separate ticking mechanism for these,
    // that ensured ticks every 5s, instead of heartbeat intervals
    (enchantment.duration / heartbeat_interval).ceil().cs_cast()
}

/// Returns the total damage for a DoT enchantment.
// ACE: EnchantmentManager.GetTotalDamage
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn get_total_damage(
    w: &World,
    this: ObjectGuid,
    enchantment: &PropertiesEnchantmentRegistry,
) -> f32 {
    // assumed to be DoT enchantment
    enchantment.stat_mod_value * get_num_ticks(w, this, enchantment, None) as f32
}

// ACE: EnchantmentManager.GetDamagePerTick
#[must_use]
#[allow(clippy::float_cmp, clippy::cast_precision_loss)]
pub fn get_damage_per_tick(
    w: &World,
    this: ObjectGuid,
    enchantment: &PropertiesEnchantmentRegistry,
    heartbeat_interval: Option<f64>,
) -> f32 {
    // assumed to be DoT enchantment

    let creature_heartbeat_interval = obj(w, this).heartbeat_interval().unwrap_or(5.0);

    let heartbeat_interval = heartbeat_interval.unwrap_or(creature_heartbeat_interval);

    if heartbeat_interval == creature_heartbeat_interval {
        return enchantment.stat_mod_value;
    }

    // calculate the total damage w/ creature heartbeat interval
    let total_damage = get_total_damage(w, this, enchantment);

    // divide totalDamage by the requested tick interval
    let num_ticks = get_num_ticks(w, this, enchantment, Some(heartbeat_interval));

    total_damage / num_ticks as f32
}

// ---------------------------------------------------------------------------------------------
// Not ACE: pointers to members of other ACE files. Each calls `not_ported!` with ACE's name and
// answers the value noted until that member is ported.
// ---------------------------------------------------------------------------------------------

/// `PropertyManager.GetDouble(name).Item`.
fn property_manager_get_double(w: &World, name: &str) -> f64 {
    crate::managers::property_manager::get_double(w, name, 0.0, true).item
}

/// `new Enchantment(Player, entry)` (Network/Structure/Enchantment.cs).
pub(crate) fn new_enchantment(
    w: &World,
    player: ObjectGuid,
    entry: &PropertiesEnchantmentRegistry,
) -> Enchantment {
    crate::network::structure::enchantment::enchantment_from_registry(w, player, entry)
}

/// `caster.CalculateDotEnchantment_StatModValue(spell, target, weapon, statModVal)`
/// (WorldObject_Magic.cs).
fn world_object_calculate_dot_enchantment_stat_mod_value(
    w: &mut World,
    this: ObjectGuid,
    spell: &Spell,
    target: ObjectGuid,
    weapon: Option<ObjectGuid>,
    stat_mod_val: f32,
) -> f32 {
    crate::world_objects::world_object_magic::calculate_dot_enchantment_stat_mod_value(
        w,
        this,
        spell,
        Some(target),
        weapon,
        stat_mod_val,
    )
}

/// `caster.ItemSetContains(spellID)` (WorldObject_Set.cs).
fn world_object_item_set_contains(w: &World, this: ObjectGuid, spell_id: u32) -> bool {
    crate::world_objects::world_object_set::item_set_contains(w, this, spell_id)
}
