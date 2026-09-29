// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Corpse.cs
//! Port of `Source/ACE.Server/WorldObjects/Corpse.cs`.
//!
//! Members are free functions over `(w, this)`, `this` being the corpse. The rot countdown itself
//! is `WorldObject_Decay.cs`'s; this file sets `TimeToRot`.

use empyrean_common::dotnet::math::{min as math_min, round_digits_mode, MidpointRounding};
use empyrean_common::dotnet::{CsCast, DotNetHashSet};
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_common::time::Time;
use empyrean_entity::enums::{ChatMessageType, PKLevel, Sound};
use empyrean_entity::ObjectGuid;

use crate::entity::actions::action_chain::ActionChain;
use crate::entity::actions::i_actor::Actor;
use crate::entity::damage_history_info::DamageHistoryInfo;
use crate::world_objects::creature_death::{container_inventory, name_of};
use crate::world_objects::kinds::KindData;
use crate::world_objects::world_object::WorldObject;
use crate::World;

/// Non-property fields declared in `Corpse.cs`.
#[derive(Debug, Default)]
pub struct CorpseFields {
    // ACE: Corpse.IsMonster
    /// Flag indicates if a corpse is from a monster or a player.
    pub is_monster: bool,

    // ACE: Corpse.permitteeOpened
    /// When a permittee opens a locked corpse of a permitter, the permitter is removed from the
    /// permittee's LootPermissions table by default (retail allows 1 locked corpse), but the
    /// permittee may still open/close this corpse again: the permittees that opened it.
    pub permittee_opened: Option<DotNetHashSet<u32>>,

    // ACE: Corpse.IsLooted
    pub is_looted: bool,

    // ACE: Corpse.rareGenerated
    pub rare_generated: Option<ObjectGuid>,

    // ACE: Corpse.killerName
    pub killer_name: Option<String>,
}

// ACE: Corpse.EmptyDecayTime
/// The maximum number of seconds for an empty corpse to stick around.
pub const EMPTY_DECAY_TIME: f64 = 15.0;

// ACE: Corpse.HalfLife
/// The number of seconds before all players can loot a monster corpse. A mutable `public static`
/// in ACE that nothing assigns, so a constant here.
pub const HALF_LIFE: i32 = 180;

fn obj(w: &World, this: ObjectGuid) -> &WorldObject {
    w.objects
        .get(this)
        .unwrap_or_else(|| panic!("Corpse {this:?}: missing object"))
}

fn obj_mut(w: &mut World, this: ObjectGuid) -> &mut WorldObject {
    w.objects
        .get_mut(this)
        .unwrap_or_else(|| panic!("Corpse {this:?}: missing object"))
}

/// This corpse's `CorpseFields`.
///
/// # Panics
/// When `this` is not a Corpse.
pub fn fields(w: &World, this: ObjectGuid) -> &CorpseFields {
    match &obj(w, this).kind {
        KindData::Corpse(d) => &d.corpse,
        _ => panic!("InvalidCastException: {this:?} is not a Corpse"),
    }
}

fn fields_mut(w: &mut World, this: ObjectGuid) -> &mut CorpseFields {
    match &mut obj_mut(w, this).kind {
        KindData::Corpse(d) => &mut d.corpse,
        _ => panic!("InvalidCastException: {this:?} is not a Corpse"),
    }
}

/// `corpse.IsMonster = value`.
pub fn set_is_monster(w: &mut World, this: ObjectGuid, value: bool) {
    fields_mut(w, this).is_monster = value;
}

/// `PkLevel` (`WorldObject_Properties.cs`: `(PKLevel)PkLevelModifier`).
fn pk_level(o: &WorldObject) -> PKLevel {
    PKLevel(o.pk_level_modifier().cs_cast())
}

// ACE: Corpse.OnInitialInventoryLoadCompleted
/// A player corpse reloaded from the database logs when it may decay.
pub fn corpse_on_initial_inventory_load_completed(w: &mut World, this: ObjectGuid) {
    let o = obj(w, this);
    if let Some(level) = o.level() {
        let time_to_rot = o.time_to_rot();
        let dt_time_to_rot = w.now.utc.add_seconds(time_to_rot.unwrap_or(0.0));
        let ts_decay = dt_time_to_rot - w.now.utc;
        let creation_timestamp = o.creation_timestamp();

        // DIVERGE: ACE prints the creation and decay times in server local time; here they are UTC.
        log::info!(
            "[CORPSE] {} (0x{this}) Reloaded from Database: Corpse Level: {level} | InventoryLoaded: {} | Inventory.Count: {} | TimeToRot: {} | CreationTimestamp: {} ({}) | Corpse should not decay before: {}, {} day(s), {} hours, {} minutes, and {} seconds from now.",
            name_of(w, this),
            container_inventory_loaded(w, this),
            container_inventory(w, this).len(),
            time_to_rot.map(|t| t.to_string()).unwrap_or_default(),
            creation_timestamp.map(|t| t.to_string()).unwrap_or_default(),
            Time::get_date_time_from_timestamp(f64::from(creation_timestamp.unwrap_or(0))).format("yyyy-MM-dd HH:mm:ss"),
            dt_time_to_rot.format("yyyy-MM-dd HH:mm:ss"),
            ts_decay.days(),
            ts_decay.hours(),
            ts_decay.minutes(),
            ts_decay.seconds()
        );
    }
}

// ACE: Corpse.CalculateObjDesc
/// Sets the object description for a corpse: the appearance saved at death, if any.
pub fn corpse_calculate_obj_desc(w: &mut World, this: ObjectGuid) -> empyrean_entity::ObjDesc {
    let (anim_parts, palettes, texture_maps) = {
        let b = &obj(w, this).biota;
        (
            b.properties_anim_part.clone().unwrap_or_default(),
            b.properties_palette.clone().unwrap_or_default(),
            b.properties_texture_map.clone().unwrap_or_default(),
        )
    };
    if anim_parts.is_empty() && palettes.is_empty() && texture_maps.is_empty() {
        return crate::world_objects::world_object_networking::world_object_calculate_obj_desc(
            w, this,
        ); // No Saved ObjDesc, let base handle it.
    }

    let mut obj_desc = empyrean_entity::ObjDesc::default();

    world_object_add_base_model_data(w, this, &mut obj_desc);

    obj_desc.anim_part_changes.extend(anim_parts);

    obj_desc.sub_palettes.extend(palettes);

    obj_desc.texture_changes.extend(texture_maps);

    obj_desc
}

// ACE: Corpse.RecalculateDecayTime
/// Sets the decay time for player corpse. This should be called AFTER the items (if any) have
/// been added to the corpse. Corpses that have no items will decay much faster.
pub fn recalculate_decay_time(w: &mut World, this: ObjectGuid, player: ObjectGuid) {
    let player_level = obj(w, player).level();

    // empty corpses decay faster
    let time_to_rot = if container_inventory(w, this).is_empty() {
        EMPTY_DECAY_TIME
    } else {
        // a player corpse decays after 5 mins * playerLevel with a minimum of 1 hour
        f64::from(3600.max(player_level.unwrap_or(1).wrapping_mul(300)))
    };
    obj_mut(w, this).set_time_to_rot(Some(time_to_rot));

    let dt_time_to_rot = w.now.utc.add_seconds(time_to_rot);
    let ts_decay = dt_time_to_rot - w.now.utc;

    obj_mut(w, this).set_level(Some(player_level.unwrap_or(1)));

    let creation_timestamp = obj(w, this).creation_timestamp();
    // DIVERGE: ACE prints the creation and decay times in server local time; here they are UTC.
    log::info!(
        "[CORPSE] {}.RecalculateDecayTime({}) 0x{this}: Player Level: {} | Inventory.Count: {} | TimeToRot: {time_to_rot} | CreationTimestamp: {} ({}) | Corpse should not decay before: {}, {} day(s), {} hours, {} minutes, and {} seconds from now.",
        name_of(w, this),
        name_of(w, player),
        player_level.map(|l| l.to_string()).unwrap_or_default(),
        container_inventory(w, this).len(),
        creation_timestamp.map(|t| t.to_string()).unwrap_or_default(),
        Time::get_date_time_from_timestamp(f64::from(creation_timestamp.unwrap_or(0))).format("yyyy-MM-dd HH:mm:ss"),
        dt_time_to_rot.format("yyyy-MM-dd HH:mm:ss"),
        ts_decay.days(),
        ts_decay.hours(),
        ts_decay.minutes(),
        ts_decay.seconds()
    );
}

// ACE: Corpse.Open
/// Called when a player attempts to loot a corpse: refused without looting rights.
pub fn corpse_open(w: &mut World, this: ObjectGuid, player: ObjectGuid) {
    // check for looting permission
    if !has_permission(w, this, player) {
        let name = name_of(w, this);
        let msg = if obj(w, this).corpse_generated_rare() {
            format!("You may not loot the {name} because the {name} has generated a rare item.")
        } else if pk_level(obj(w, this)) == PKLevel::PK {
            format!("You may not loot the {name} because the death was caused by a player killer.")
        } else {
            format!("You do not yet have the right to loot the {name}.")
        };
        crate::world_objects::player_skills::send_transient_error(w, player, &msg);
        return;
    }
    crate::world_objects::container::container_open(w, this, player);
}

// ACE: Corpse.HasPermission
/// Returns TRUE if `player` has permission to loot this corpse. A permittee opening a permitter's
/// locked corpse uses up the permission (retail), and is remembered for this corpse.
pub fn has_permission(w: &mut World, this: ObjectGuid, player: ObjectGuid) -> bool {
    let (victim_id, killer_id, is_looted) = {
        let o = obj(w, this);
        (o.victim_id(), o.killer_id(), fields(w, this).is_looted)
    };

    // players can loot their own corpses
    let Some(victim_id) = victim_id else {
        return true;
    };
    if player.full() == victim_id {
        return true;
    }

    // players can loot corpses of creatures they killed or corpses that have previously been looted by killer
    if killer_id.is_some_and(|k| player.full() == k) || is_looted {
        return true;
    }

    let victim_guid = ObjectGuid::new(victim_id);

    // players can /permit other players to loot their corpse if not killed by another player killer.
    if crate::world_objects::player_death::has_loot_permission(w, player, victim_guid)
        && pk_level(obj(w, this)) != PKLevel::PK
    {
        if !crate::managers::property_manager::get_bool(w, "permit_corpse_all", false, true).item {
            // this is the retail default. see the comments for 'permitteeOpened' for an explanation of why this table is needed
            // these are technically side effects, and HasPermission() is not the best place for this logic to mutate state,
            // however with the current lone calling pattern for corpse ActOnUse -> TryOpen -> HasPermission -> Open
            // if HasPermission returns true, the corpse is always opened, ie. there's no chance of 'the corpse is already in use' or any other failure cases,
            // as those pre-verifications have already happened before this function is called

            fields_mut(w, this)
                .permittee_opened
                .get_or_insert_with(DotNetHashSet::new)
                .insert(player.full());

            player_loot_permission_remove(w, player, victim_guid);
        }
        return true;
    }
    if fields(w, this)
        .permittee_opened
        .as_ref()
        .is_some_and(|p| p.contains(&player.full()))
    {
        return true;
    }

    // all players can loot monster corpses after 1/2 decay time except if corpse generates a rare
    let (time_to_rot, generated_rare) = {
        let o = obj(w, this);
        (o.time_to_rot(), o.corpse_generated_rare())
    };
    if time_to_rot.is_some_and(|t| t < f64::from(HALF_LIFE))
        && !victim_guid.is_player()
        && !generated_rare
    {
        return true;
    }

    // players in the same fellowship as the killer w/ loot sharing enabled except if corpse generates a rare
    if player_fellowship_share_loot(w, player) {
        let online_player =
            crate::managers::player_manager::get_online_player(w, killer_id.unwrap_or(0));
        if let Some(online_player) = online_player {
            if player_same_fellowship(w, player, online_player)
                && !generated_rare
                && pk_level(obj(w, this)) != PKLevel::PK
            {
                return true;
            }
        }
    }
    false
}

// ACE: Corpse.IsLooted
#[must_use]
pub fn is_looted(w: &World, this: ObjectGuid) -> bool {
    fields(w, this).is_looted
}

// ACE: Corpse.Close
/// Closing marks the corpse looted: after that anyone may loot it.
pub fn corpse_close(w: &mut World, this: ObjectGuid, player: ObjectGuid) {
    crate::world_objects::container::container_close(w, this, player);

    fields_mut(w, this).is_looted = true;
}

// ACE: Corpse.IsOnNoDropLandblock
#[must_use]
pub fn is_on_no_drop_landblock(w: &World, this: ObjectGuid) -> bool {
    match obj(w, this).location() {
        Some(location) => NO_DROP_LANDBLOCKS.contains(&(location.landblock_id().raw() >> 16)),
        None => false,
    }
}

// ACE: Corpse.EnterWorld
/// Enters the world; half a second later a corpse that generated a rare announces it.
pub fn corpse_enter_world(w: &mut World, this: ObjectGuid) -> bool {
    let mut action_chain = ActionChain::new();

    let success = crate::world_objects::world_object::world_object_enter_world(w, this);
    if !success {
        let location = w
            .objects
            .get(this)
            .and_then(WorldObject::location)
            .map(|l| l.to_loc_string())
            .unwrap_or_default();
        log::error!(
            "{} ({}) failed to spawn @ {location}",
            name_of(w, this),
            this
        );
        return false;
    }

    action_chain.add_delay_seconds(w, f64::from(0.5f32));
    action_chain.add_action(Actor::Object(this), move |w| {
        let Some(o) = w.objects.get(this) else { return };
        if o.location().is_some() && o.corpse_generated_rare() {
            let f = fields(w, this);
            let rare_name = f.rare_generated.map(|r| name_of(w, r)).unwrap_or_default();
            let msg = format!(
                "{} has discovered the {rare_name}!",
                f.killer_name.as_deref().unwrap_or("")
            );
            world_object_enqueue_broadcast_chat(w, this, &msg, ChatMessageType::System);
            crate::world_objects::world_object::apply_sound_effects(
                w,
                this,
                Sound::TriggerActivated,
                10.0,
            );
        }
    });
    action_chain.enqueue_chain(w);

    true
}

// ACE: Corpse.TryGenerateRare
/// Called to attempt to generate rare and add to corpse inventory, with the real-time rare timers
/// of the killer (`rares_real_time`, `rares_real_time_v2`).
pub fn try_generate_rare(w: &mut World, this: ObjectGuid, killer: &DamageHistoryInfo) {
    let killer_player = killer
        .try_get_attacker(w)
        .filter(|g| obj(w, *g).is_player());
    let timestamp: i32 = w.now.unix_time.cs_cast();
    let mut luck = 0;
    let mut second_chance_granted = false;

    let real_time_rares =
        crate::managers::property_manager::get_bool(w, "rares_real_time", false, true).item;
    let real_time_rares_alt =
        crate::managers::property_manager::get_bool(w, "rares_real_time_v2", false, true).item;
    if let Some(kp) = killer_player.filter(|_| real_time_rares) {
        if let Some(rares_login_timestamp) = obj(w, kp).rares_login_timestamp() {
            // Real Time Rares work the same as they always have. It rolls a number between 1 second and 2 months worth of seconds. When that number is up you get an additional chance of finding a rare on any valid rare kill.
            // That additional chance is very high. You can still only find one rare on any given kill but it's possible to find a normal rare when your Real Time Rare timer is up but you haven't found one yet.

            let now = Time::get_date_time_from_timestamp(f64::from(timestamp));
            let player_last_rare_found =
                Time::get_date_time_from_timestamp(f64::from(rares_login_timestamp));

            if now >= player_last_rare_found {
                second_chance_granted = true;
            }
        } else {
            let future = future_unix_time(w, this);
            obj_mut(w, kp).set_rares_login_timestamp(Some(future));
        }
    } else if let Some(kp) = killer_player.filter(|_| real_time_rares_alt) {
        if let Some(rares_login_timestamp) = obj(w, kp).rares_login_timestamp() {
            // This version of the system is based on interpretation of the following way the system was originally described.

            let now = Time::get_date_time_from_timestamp(f64::from(timestamp));
            let player_last_rare_found =
                Time::get_date_time_from_timestamp(f64::from(rares_login_timestamp));
            let time_between_rare_sighting = now - player_last_rare_found;
            let days_since_rare_sighting = time_between_rare_sighting.total_days();

            let max_days: i32 =
                crate::managers::property_manager::get_long(w, "rares_max_days_between", 0, true)
                    .item
                    .cs_cast(); // 30? 45? 60?
            let chances_modifier = round_digits_mode(
                days_since_rare_sighting / f64::from(max_days),
                2,
                MidpointRounding::ToZero,
            );
            let chances_modifier_adjusted = math_min(chances_modifier, f64::from(1.0f32));

            let t1_chance = 2500;
            luck = round_digits_mode(
                f64::from(t1_chance) * chances_modifier_adjusted,
                0,
                MidpointRounding::ToZero,
            )
            .cs_cast();
        } else {
            obj_mut(w, kp).set_rares_login_timestamp(Some(timestamp));
        }
    }

    let mut wo = loot_generation_factory_try_create_rare(w, luck);

    if second_chance_granted && wo.is_none() {
        luck = 2490;
        wo = loot_generation_factory_try_create_rare(w, luck);
    }

    let Some(wo) = wo else {
        return;
    };

    {
        let o = obj_mut(w, wo);
        if o.icon_underlay_id() != Some(0x0600_5B0C) {
            // ensure icon underlay exists for rare (loot profiles use this)
            o.set_icon_underlay_id(Some(0x0600_5B0C));
        }
    }

    let tier = loot_generation_factory_get_rare_tier(w, obj(w, wo).biota.weenie_class_id);
    let chance = loot_generation_factory_rare_chances(tier).unwrap_or(0);

    let killer_name_log = killer.name.clone().unwrap_or_default();
    log::info!(
        "[LOOT][RARE] {} ({}) generated rare {} ({}) for {} ({})",
        name_of(w, this),
        this,
        name_of(w, wo),
        wo,
        killer_name_log,
        killer.guid
    );
    log::info!(
        "[LOOT][RARE] Tier {tier} -- 1 / {} chance -- {} luck",
        empyrean_common::dotnet::format(chance, "N0"),
        empyrean_common::dotnet::format(luck, "N0")
    );

    if crate::world_objects::creature_death::container_try_add_to_inventory(w, this, wo) {
        {
            let f = fields_mut(w, this);
            f.rare_generated = Some(wo);
            f.killer_name = Some(
                killer
                    .name
                    .clone()
                    .unwrap_or_default()
                    .trim_start_matches('+')
                    .to_owned(),
            );
        }
        let o = obj_mut(w, this);
        o.set_corpse_generated_rare(true);
        let long_desc = o.long_desc().unwrap_or_default();
        o.set_long_desc(Some(long_desc + " This corpse generated a rare item!"));
        o.set_time_to_rot(Some(900.0)); // guesstimated 15 mins from hells

        if let Some(kp) = killer_player {
            if real_time_rares {
                let future = future_unix_time(w, this);
                obj_mut(w, kp).set_rares_login_timestamp(Some(future));
            } else {
                obj_mut(w, kp).set_rares_login_timestamp(Some(timestamp));
            }
            let o = obj_mut(w, kp);
            match tier {
                1 => {
                    o.set_rares_tier_one(o.rares_tier_one().wrapping_add(1));
                    o.set_rares_tier_one_login(Some(timestamp));
                }
                2 => {
                    o.set_rares_tier_two(o.rares_tier_two().wrapping_add(1));
                    o.set_rares_tier_two_login(Some(timestamp));
                }
                3 => {
                    o.set_rares_tier_three(o.rares_tier_three().wrapping_add(1));
                    o.set_rares_tier_three_login(Some(timestamp));
                }
                4 => {
                    o.set_rares_tier_four(o.rares_tier_four().wrapping_add(1));
                    o.set_rares_tier_four_login(Some(timestamp));
                }
                5 => {
                    o.set_rares_tier_five(o.rares_tier_five().wrapping_add(1));
                    o.set_rares_tier_five_login(Some(timestamp));
                }
                6 => {
                    o.set_rares_tier_six(o.rares_tier_six().wrapping_add(1));
                    o.set_rares_tier_six_login(Some(timestamp));
                }
                //case 7: RaresTierSeven
                _ => {}
            }
        }
    } else {
        log::error!("[RARE] failed to add to corpse inventory");
    }
}

/// `(int)Time.GetFutureUnixTime(ThreadSafeRandom.Next(1, (int)PropertyManager.GetLong("rares_max_seconds_between").Item))`:
/// one RNG draw.
fn future_unix_time(w: &World, _this: ObjectGuid) -> i32 {
    let max: i32 =
        crate::managers::property_manager::get_long(w, "rares_max_seconds_between", 0, true)
            .item
            .cs_cast();
    let seconds = ThreadSafeRandom::next(1, max);
    (w.now.unix_time + f64::from(seconds)).cs_cast()
}

// ACE: Corpse.NoDrop_Landblocks
/// A list of landblocks the player cannot drop items on corpse on death.
pub const NO_DROP_LANDBLOCKS: [u32; 19] = [
    0x005F, // Tanada House of Pancakes (Seasonal)
    0x00AF, // Colosseum Staging Area and Secret Mini-Bosses
    0x00B0, // Colosseum Arena One
    0x00B1, // Colosseum Arena Two
    0x00B2, // Colosseum Arena Three
    0x00B3, // Colosseum Arena Four
    0x00B4, // Colosseum Arena Five
    0x00B6, // Colosseum Arena Mini-Bosses
    0x00EA, // Mhoire Armory
    0x33F4, // Frozen Cave
    0x5960, // Gauntlet Arena One (Celestial Hand)
    0x5961, // Gauntlet Arena Two (Celestial Hand)
    0x5962, // Gauntlet Arena One (Eldritch Web)
    0x5963, // Gauntlet Arena Two (Eldritch Web)
    0x5964, // Gauntlet Arena One (Radiant Blood)
    0x5965, // Gauntlet Arena Two (Radiant Blood)
    0x596B, // Gauntlet Staging Area (All Societies)
    0x8A04, // Night Club (Seasonal Anniversary)
    0xB5F0, // Aerfalle's Sanctum
];

// ---- constructors and SetEphemeralValues ----

/// `new Corpse(weenie, guid)` / `new Corpse(biota)`: the `Container` constructor, then
/// Corpse's `SetEphemeralValues`.
// ACE: Corpse.Corpse
pub fn corpse_ctor(
    o: &mut crate::world_objects::world_object::WorldObject,
    env: &crate::world_objects::world_object::CtorEnv<'_>,
    src: crate::world_objects::world_object::CtorSource,
) {
    let from_biota = src.is_biota();
    crate::world_objects::container::container_ctor(o, env, src);
    corpse_set_ephemeral_values(o, env);

    if from_biota {
        // for player corpses restored from database,
        // ensure any floating corpses fall to the ground
        o.wo.world_object.bump_velocity = true;
    }
}

// ACE: Corpse.SetEphemeralValues
fn corpse_set_ephemeral_values(
    o: &mut crate::world_objects::world_object::WorldObject,
    _env: &crate::world_objects::world_object::CtorEnv<'_>,
) {
    o.wo.world_object.object_description_flags |=
        empyrean_entity::enums::ObjectDescriptionFlag::Corpse;

    o.wo.world_object_properties.current_motion_state =
        Some(crate::network::motion::movement_data::Motion::new(
            empyrean_entity::enums::MotionStance::NonCombat,
            empyrean_entity::enums::MotionCommand::Dead,
            1.0,
        ));

    o.set_container_capacity(Some(10));
    o.set_item_capacity(Some(120));

    o.set_suppress_generate_effect(Some(true));
}

// ---------------------------------------------------------------------------------------------
// Not ACE: pointers to members of other ACE files, named after them.
// ---------------------------------------------------------------------------------------------

/// `InventoryLoaded` (`Container.cs`).
fn container_inventory_loaded(w: &World, this: ObjectGuid) -> bool {
    obj(w, this)
        .container
        .as_ref()
        .is_some_and(|c| c.container.inventory_loaded)
}

/// `AddBaseModelData(objDesc)` (`WorldObject_Networking.cs`).
fn world_object_add_base_model_data(
    w: &mut World,
    this: ObjectGuid,
    obj_desc: &mut empyrean_entity::ObjDesc,
) {
    crate::world_objects::world_object_networking::add_base_model_data(w, this, obj_desc);
}

/// `player.LootPermission.Remove(victimGuid)` on another player (`Player_Death.cs` field).
fn player_loot_permission_remove(w: &mut World, player: ObjectGuid, victim: ObjectGuid) {
    if let Some(p) = w.objects.get_mut(player).and_then(|o| o.player.as_mut()) {
        p.player_death.loot_permission.remove(&victim);
    }
}

/// `player.Fellowship != null && player.Fellowship.ShareLoot`.
fn player_fellowship_share_loot(w: &World, player: ObjectGuid) -> bool {
    crate::world_objects::player_fellowship::fellowship(w, player)
        .is_some_and(|f| f.get(w).share_loot)
}

/// `onlinePlayer.Fellowship != null && player.Fellowship == onlinePlayer.Fellowship`.
fn player_same_fellowship(w: &World, player: ObjectGuid, other: ObjectGuid) -> bool {
    let other_fellowship = crate::world_objects::player_fellowship::fellowship(w, other);
    other_fellowship.is_some()
        && crate::world_objects::player_fellowship::fellowship(w, player) == other_fellowship
}

/// `EnqueueBroadcast(new GameMessageSystemChat(msg, type))` (`WorldObject_Networking.cs`).
fn world_object_enqueue_broadcast_chat(
    w: &mut World,
    this: ObjectGuid,
    msg: &str,
    chat_type: ChatMessageType,
) {
    let msg =
        crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat(
            msg, chat_type,
        );
    crate::world_objects::world_object_networking::enqueue_broadcast(w, this, true, &[msg]);
}

/// `LootGenerationFactory.TryCreateRare(luck)` (4.10); the rare joins `World.objects`.
fn loot_generation_factory_try_create_rare(w: &mut World, luck: i32) -> Option<ObjectGuid> {
    let wo = crate::factories::loot_generation_factory_rare::try_create_rare(w, luck)?;
    let guid = wo.guid;
    if let Err(dup) = w.objects.insert(wo) {
        panic!("two live objects with guid {:?}", dup.guid);
    }
    Some(guid)
}

/// `LootGenerationFactory.GetRareTier(wcid)` (4.10).
fn loot_generation_factory_get_rare_tier(_w: &World, wcid: u32) -> i32 {
    crate::factories::loot_generation_factory_rare::get_rare_tier(wcid)
}

/// `LootGenerationFactory.RareChances.TryGetValue(tier, out var chance)` (4.10).
fn loot_generation_factory_rare_chances(tier: i32) -> Option<i32> {
    crate::factories::loot_generation_factory_rare::RARE_CHANCES
        .iter()
        .find(|(t, _)| *t == tier)
        .map(|(_, c)| *c)
}
