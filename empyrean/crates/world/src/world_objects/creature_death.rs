// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Creature_Death.cs
//! Port of `Source/ACE.Server/WorldObjects/Creature_Death.cs`.
//!
//! Members are free functions over `(w, this)`. `DamageHistoryInfo` arguments are values (ACE's
//! references are never mutated through these paths). Members of other ACE files are
//! called through the pointers at the end of the file, named after them.

use std::sync::Arc;

use empyrean_common::dotnet::math::round as math_round;
use empyrean_common::dotnet::{CsCast, DotNetDict};
use empyrean_entity::enums::{
    ChatMessageType, DamageType, DestinationType, MotionCommand, MotionStance, PKLevel,
    PlayerKillerStatus, PositionType, PropertyInstanceId, PropertyInt, PropertyString, ShareType,
    XpType,
};
use empyrean_entity::{ObjectGuid, Position};

use crate::dispatch;
use crate::entity::actions::action_chain::ActionChain;
use crate::entity::actions::i_actor::Actor;
use crate::entity::damage_history::{self, DamageHistory};
use crate::entity::damage_history_info::DamageHistoryInfo;
use crate::entity::death_message::{string_format, DeathMessage};
use crate::entity::strings;
use crate::network::game_event::events::game_event_killer_notification::game_event_killer_notification;
use crate::network::game_messages::game_message::enqueue_send;
use crate::network::game_messages::messages::game_message_pickup_event::game_message_pickup_event;
use crate::network::game_messages::messages::game_message_private_update_position::game_message_private_update_position;
use crate::network::game_messages::messages::game_message_public_update_instance_id::game_message_public_update_instance_id;
use crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use crate::network::motion::movement_data::Motion;
use crate::world_objects::player_skills::{self, send};
use crate::world_objects::world_object::{CtorEnv, WorldObject};
use crate::World;

/// Non-property fields declared in `Creature_Death.cs`.
#[derive(Debug, Default)]
pub struct CreatureDeathFields {
    // ACE: Creature.onDeathEntered
    pub on_death_entered: bool,

    // ACE: Creature.dieEntered
    pub die_entered: bool,

    // ACE: Creature.DamageHistory
    /// Declared in `Creature_Combat.cs` (`public DamageHistory DamageHistory { get; private set; }`);
    /// held here, with the death code that reads it, rather than beside `Creature_Combat.cs`'s other fields.
    /// `Creature.SetEphemeralValues` builds it; [`damage_history::of`] reads it.
    pub damage_history: DamageHistory,
}

fn obj(w: &World, this: ObjectGuid) -> &WorldObject {
    w.objects
        .get(this)
        .unwrap_or_else(|| panic!("Creature {this:?}: missing object"))
}

fn obj_mut(w: &mut World, this: ObjectGuid) -> &mut WorldObject {
    w.objects
        .get_mut(this)
        .unwrap_or_else(|| panic!("Creature {this:?}: missing object"))
}

fn fields(w: &World, this: ObjectGuid) -> &CreatureDeathFields {
    &obj(w, this)
        .creature
        .as_ref()
        .expect("Creature_Death on an object that is not a Creature")
        .creature_death
}

fn fields_mut(w: &mut World, this: ObjectGuid) -> &mut CreatureDeathFields {
    &mut obj_mut(w, this)
        .creature
        .as_mut()
        .expect("Creature_Death on an object that is not a Creature")
        .creature_death
}

fn is_player(w: &World, g: ObjectGuid) -> bool {
    w.objects.get(g).is_some_and(WorldObject::is_player)
}

/// The virtual `Name` (`Player.Name` prefixes `+`), as C# interpolation prints it (null: "").
pub(crate) fn name_of(w: &World, this: ObjectGuid) -> String {
    dispatch::name::name(w, this).unwrap_or_default()
}

fn chat(msg: &str) -> crate::network::game_messages::game_message::GameMessage {
    game_message_system_chat(msg, ChatMessageType::Broadcast)
}

// ACE: Creature.DeathTreasure
#[must_use]
pub fn death_treasure(
    w: &World,
    this: ObjectGuid,
) -> Option<Arc<empyrean_content::models::world::treasure_death::TreasureDeath>> {
    let death_treasure_type = obj(w, this).death_treasure_type()?;
    // DIVERGE: the era's profile for a creature (the Infiltration era scales its chances by tier).
    crate::factories::loot_generation_factory::era_death_treasure(w, death_treasure_type, this)
}

// ACE: Creature.OnDeath
/// Called when a monster or player dies, in conjunction with Die(). `last_damager` landed the
/// death blow; `damage_type` picks the death message; `critical_hit` a critical one.
pub fn creature_on_death(
    w: &mut World,
    this: ObjectGuid,
    last_damager: Option<DamageHistoryInfo>,
    damage_type: DamageType,
    critical_hit: bool,
) -> DeathMessage {
    if fields(w, this).on_death_entered {
        return get_death_message(w, this, last_damager.as_ref(), damage_type, critical_hit);
    }

    fields_mut(w, this).on_death_entered = true;

    creature_set_is_turning(w, this, false);
    creature_set_is_moving(w, this, false);

    //QuestManager.OnDeath(lastDamager?.TryGetAttacker());

    let (kill_quest, kill_quest2, kill_quest3) = {
        let o = obj(w, this);
        (o.kill_quest(), o.kill_quest2(), o.kill_quest3())
    };
    if let Some(q) = kill_quest {
        on_death_handle_kill_task(w, this, &q);
    }
    if let Some(q) = kill_quest2 {
        on_death_handle_kill_task(w, this, &q);
    }
    if let Some(q) = kill_quest3 {
        on_death_handle_kill_task(w, this, &q);
    }

    if !is_on_no_death_xp_landblock(w, this) {
        on_death_grant_xp(w, this);
    }

    get_death_message(w, this, last_damager.as_ref(), damage_type, critical_hit)
}

// ACE: Creature.GetDeathMessage
/// The death message for this death (one RNG draw via `Strings.GetDeathMessage` unless the death
/// was self-inflicted, environmental or by no one); a player killer gets a KillerNotification.
pub fn get_death_message(
    w: &mut World,
    this: ObjectGuid,
    last_damager_info: Option<&DamageHistoryInfo>,
    damage_type: DamageType,
    critical_hit: bool,
) -> DeathMessage {
    let last_damager = last_damager_info.and_then(|i| i.try_get_attacker(w));
    let last_damager_is =
        |f: fn(&WorldObject) -> bool| last_damager.and_then(|g| w.objects.get(g)).is_some_and(f);

    let Some(last_damager_info) = last_damager_info else {
        return strings::GENERAL[1];
    };
    // !(lastDamager is Creature)?
    if last_damager_info.guid == this
        || last_damager_is(WorldObject::is_hotspot)
        || last_damager_is(WorldObject::is_food)
    {
        return strings::GENERAL[1];
    }

    let mut death_message = strings::get_death_message(damage_type, critical_hit);

    // if killed by a player, send them a message
    if last_damager_info.is_player() {
        if critical_hit && obj(w, this).is_player() {
            death_message = strings::PK_CRITICAL[0];
        }

        let killer_msg = string_format(death_message.killer, &[&name_of(w, this)]);

        if let Some(player_killer) = last_damager.filter(|g| is_player(w, *g)) {
            let s = player_skills::session(w, player_killer);
            let session = w
                .sessions
                .get_mut(s)
                .expect("NullReferenceException: Player.Session");
            let msg = game_event_killer_notification(session, &killer_msg);
            enqueue_send(w, s, msg);
        }
    }
    death_message
}

// ACE: Creature.Die
/// Kills a player/creature and performs the full death sequence (the parameterless overload):
/// `Die(DamageHistory.LastDamager, DamageHistory.TopDamager)`, virtually.
pub fn die(w: &mut World, this: ObjectGuid) {
    let history = damage_history::of(w, this);
    let (last_damager, top_damager) = (history.last_damager(), history.top_damager());
    dispatch::die::die(w, this, last_damager, top_damager);
}

// ACE: Creature.Die
/// Performs the full death sequence for non-Player creatures.
pub fn creature_die(
    w: &mut World,
    this: ObjectGuid,
    last_damager: Option<DamageHistoryInfo>,
    top_damager: Option<DamageHistoryInfo>,
) {
    if fields(w, this).die_entered {
        return;
    }

    fields_mut(w, this).die_entered = true;

    let health = obj(w, this).health();
    dispatch::update_vital::update_vital(w, this, health, 0);

    if let Some(top_damager) = &top_damager {
        obj_mut(w, this).set_killer_id(Some(top_damager.guid.full()));

        if top_damager.is_player() {
            if let Some(top_damager_player) = top_damager.try_get_attacker(w) {
                let o = obj_mut(w, top_damager_player);
                let kills = o.creature_kills().unwrap_or(0).wrapping_add(1);
                o.set_creature_kills(Some(kills));
            }
        }
    }

    if let Some(o) = w.objects.get_mut(this) {
        o.wo.world_object_properties.current_motion_state = Some(Motion::new(
            MotionStance::NonCombat,
            MotionCommand::Ready,
            1.0,
        ));
    }
    //IsMonster = false;

    physics_obj_stop_completely(w, this);

    // broadcast death animation
    // `var motionDeath = new Motion(MotionStance.NonCombat, MotionCommand.Dead);`
    let death_anim_length = world_object_execute_motion_dead(w, this);

    emote_manager_on_death(w, this, last_damager.as_ref());

    let mut die_chain = ActionChain::new();

    // wait for death animation to finish
    //var deathAnimLength = DatManager.PortalDat.ReadFromDat<MotionTable>(MotionTableId).GetAnimationLength(MotionCommand.Dead);
    die_chain.add_delay_seconds(w, f64::from(death_anim_length));

    die_chain.add_action(Actor::Object(this), move |w| {
        create_corpse(w, this, top_damager.as_ref(), false);
        crate::world_objects::world_object::destroy(w, this, true, false);
    });

    die_chain.enqueue_chain(w);
}

// ACE: Creature.Smite
/// Called when an admin player uses the /smite command to instantly kill a creature.
pub fn smite(w: &mut World, this: ObjectGuid, smiter: ObjectGuid, use_take_damage: bool) {
    if use_take_damage {
        // deal remaining damage
        let current = {
            let o = obj(w, this);
            o.health().current(o)
        };
        creature_take_damage(w, this, smiter, DamageType::Bludgeon, current);
    } else {
        on_death_default(w, this);
        let smiter_info = DamageHistoryInfo::new(w, smiter, 0.0);
        dispatch::die::die(w, this, Some(smiter_info.clone()), Some(smiter_info));
    }
}

// ACE: Creature.OnDeath
/// The parameterless overload: `OnDeath(null, DamageType.Undef)`, virtually.
pub fn on_death_default(w: &mut World, this: ObjectGuid) {
    dispatch::on_death::on_death(w, this, None, DamageType::Undef, false);
}

// ACE: Creature.OnDeath_GrantXP
/// Grants XP to players in damage history, by their share of the damage.
pub fn on_death_grant_xp(w: &mut World, this: ObjectGuid) {
    {
        let o = obj(w, this);
        if o.is_player() && o.player_killer_status() == PlayerKillerStatus::PKLite {
            return;
        }
    }

    let total_health = damage_history::of(w, this).total_health();

    if total_health == 0.0 {
        return;
    }

    let damagers: Vec<DamageHistoryInfo> = damage_history::of(w, this)
        .total_damage
        .values()
        .cloned()
        .collect();
    for kvp in damagers {
        let damager = kvp.try_get_attacker(w);

        let mut player_damager = damager.filter(|g| is_player(w, *g));

        if player_damager.is_none() && kvp.pet_owner.is_some() {
            player_damager = kvp.try_get_pet_owner(w);
        }

        let Some(player_damager) = player_damager else {
            continue;
        };

        let total_damage = kvp.total_damage;

        let damage_percent = total_damage / total_health;

        let (xp_override, luminance_award) = {
            let o = obj(w, this);
            (o.xp_override(), o.luminance_award())
        };
        let xp: f32 = xp_override.unwrap_or(0).cs_cast();
        let total_xp = xp * damage_percent;

        let amount: i64 = math_round(f64::from(total_xp)).cs_cast();
        crate::world_objects::player_xp::earn_xp(
            w,
            player_damager,
            amount,
            XpType::Kill,
            ShareType::All,
        );

        // handle luminance
        if let Some(luminance_award) = luminance_award {
            let award: f32 = luminance_award.cs_cast();
            let total_luminance: i64 = math_round(f64::from(award * damage_percent)).cs_cast();
            player_earn_luminance(w, player_damager, total_luminance, XpType::Kill);
        }
    }
}

// ACE: Creature.OnDeath_HandleKillTask
/// Handles the KillTask for a killed creature, with full fellowship support and the per-player
/// and per-summon credit caps.
pub fn on_death_handle_kill_task(w: &mut World, this: ObjectGuid, kill_quest: &str) {
    // one caveat to do this, we need to keep track of player and summoning caps separately
    // this is to prevent ordering bugs, such as a player being processed after a summon,
    // and already being at the 1 cap for players

    let cap_setting = crate::managers::property_manager::get_long(
        w,
        "summoning_killtask_multicredit_cap",
        0,
        true,
    )
    .item;
    let cap_setting: i32 = cap_setting.cs_cast();
    let mut summon_credit_cap = cap_setting.wrapping_sub(1);

    let mut player_credits: DotNetDict<ObjectGuid, i32> = DotNetDict::new();
    let mut summon_credits: DotNetDict<ObjectGuid, i32> = DotNetDict::new();

    // this option isn't really needed anymore, but keeping it around for compatibility
    // it is now synonymous with summoning_killtask_multicredit_cap <= 1
    if !crate::managers::property_manager::get_bool(
        w,
        "allow_summoning_killtask_multicredit",
        false,
        true,
    )
    .item
    {
        summon_credit_cap = 0;
    }

    let damagers: Vec<DamageHistoryInfo> = damage_history::of(w, this)
        .total_damage
        .values()
        .cloned()
        .collect();
    for kvp in damagers {
        if kvp.total_damage <= 0.0 {
            continue;
        }

        let damager = kvp.try_get_attacker(w);

        let mut combat_pet = false;

        let mut player_damager = damager.filter(|g| is_player(w, *g));

        if player_damager.is_none() && kvp.pet_owner.is_some() {
            player_damager = kvp.try_get_pet_owner(w);
            combat_pet = true;
        }

        let Some(player_damager) = player_damager else {
            continue;
        };

        let mut use_summon_credits = combat_pet;

        let mut cap = if combat_pet { summon_credit_cap } else { 1 };

        if cap <= 0 {
            // handle special case: use playerCredits
            use_summon_credits = false;
            cap = 1;
        }

        let kill_task_credits = if use_summon_credits {
            &mut summon_credits
        } else {
            &mut player_credits
        };

        if quest_manager_has_quest(w, player_damager, kill_quest) {
            try_handle_kill_task(w, this, player_damager, kill_quest, kill_task_credits, cap);
        }
        // check option that requires killer to have killtask to pass to fellows
        // (on: a killer without the kill task passes no credit, as the option's description says
        // and as the combat-pet path below does)
        else if crate::managers::property_manager::get_bool(w, "fellow_kt_killer", false, true)
            .item
        {
            continue;
        }

        if !player_has_fellowship(w, player_damager) {
            continue;
        }

        // share with fellows in kill task range
        let fellows = fellowship_within_range(w, player_damager);

        for fellow in fellows {
            if quest_manager_has_quest(w, fellow, kill_quest) {
                try_handle_kill_task(w, this, fellow, kill_quest, kill_task_credits, cap);
            }
        }
    }
}

// ACE: Creature.TryHandleKillTask
/// Gives `player` one kill task credit unless they reached `cap` for this kill.
pub fn try_handle_kill_task(
    w: &mut World,
    this: ObjectGuid,
    player: ObjectGuid,
    kill_task: &str,
    kill_task_credits: &mut DotNetDict<ObjectGuid, i32>,
    cap: i32,
) -> bool {
    if let Some(current_credits) = kill_task_credits.get_mut(&player) {
        if *current_credits >= cap {
            return false;
        }

        *current_credits += 1;
    } else {
        kill_task_credits.insert(player, 1);
    }

    quest_manager_handle_kill_task(w, player, kill_task, this);

    true
}

// ACE: Creature.KillTask_GetEligibleReceivers
/// Returns a flattened structure of eligible Players, Fellows, and CombatPets (the old kill task
/// method, which ACE no longer calls).
pub fn kill_task_get_eligible_receivers(
    w: &mut World,
    this: ObjectGuid,
    kill_quest: &str,
) -> DotNetDict<ObjectGuid, DamageHistoryInfo> {
    // http://acpedia.org/wiki/Announcements_-_2012/12_-_A_Growing_Twilight#Release_Notes

    let quest_name = quest_manager_get_quest_name(kill_quest);

    // we are using DamageHistoryInfo here, instead of Creature or WorldObjectInfo
    // WeakReference<CombatPet> may be null for expired CombatPets, but we still need the WeakReference<PetOwner> references

    let mut receivers: DotNetDict<ObjectGuid, DamageHistoryInfo> = DotNetDict::new();

    let damagers: Vec<DamageHistoryInfo> = damage_history::of(w, this)
        .total_damage
        .values()
        .cloned()
        .collect();
    for kvp in damagers {
        if kvp.total_damage <= 0.0 {
            continue;
        }

        let damager = kvp.try_get_attacker(w);

        let mut player_damager = damager.filter(|g| is_player(w, *g));

        if player_damager.is_none() && kvp.pet_owner.is_some() {
            // handle combat pets
            player_damager = kvp.try_get_pet_owner(w);

            if let Some(owner) =
                player_damager.filter(|p| quest_manager_has_quest(w, *p, &quest_name))
            {
                // only add combat pet to eligible receivers if player has quest, and allow_summoning_killtask_multicredit = true (default, retail)
                if damage_history::of(w, this).has_damager(owner, true)
                    && crate::managers::property_manager::get_bool(
                        w,
                        "allow_summoning_killtask_multicredit",
                        false,
                        true,
                    )
                    .item
                {
                    receivers.insert(kvp.guid, kvp.clone()); // add CombatPet
                } else {
                    receivers.insert(owner, DamageHistoryInfo::new(w, owner, 0.0));
                    // add dummy profile for PetOwner
                }
            }

            // regardless if combat pet is eligible, we still want to continue traversing to the pet owner, and possibly fellows
        }

        let Some(player_damager) = player_damager else {
            continue;
        };

        if quest_manager_has_quest(w, player_damager, &quest_name) {
            // just add a fake DamageHistoryInfo for reference
            receivers.insert(
                player_damager,
                DamageHistoryInfo::new(w, player_damager, 0.0),
            );
        } else if crate::managers::property_manager::get_bool(w, "fellow_kt_killer", false, true)
            .item
        {
            // if this option is enabled (retail default), the killer is required to have kill task
            // for it to share with fellowship
            continue;
        }

        // we want to add fellowship members in a flattened structure
        // in this inner loop, instead of the outer loop

        if !player_has_fellowship(w, player_damager) {
            continue;
        }

        // share with fellows in kill task range
        let fellows = fellowship_within_range(w, player_damager);

        for fellow in fellows {
            if quest_manager_has_quest(w, fellow, &quest_name) {
                receivers.insert(fellow, DamageHistoryInfo::new(w, fellow, 0.0));
            }
        }
    }
    receivers
}

/// Not ACE: `WorldObjectFactory.CreateNewWorldObject(weenie)` as ACE runs it: a new dynamic guid,
/// the constructor, the guid recycled when it builds nothing; the new object joins
/// `World.objects`.
pub(crate) fn create_new_world_object(
    w: &mut World,
    weenie: Arc<empyrean_entity::Weenie>,
) -> Option<ObjectGuid> {
    let guid = crate::managers::guid_manager::new_dynamic_guid(w);
    let Some(wo) = CtorEnv::with_world(w, |env| {
        crate::factories::world_object_factory::create_world_object(env, Some(weenie), guid)
    }) else {
        crate::managers::guid_manager::recycle_dynamic_guid(w, guid);
        return None;
    };
    let guid = wo.guid;
    if let Err(dup) = w.objects.insert(wo) {
        panic!("two live objects with guid {:?}", dup.guid);
    }
    Some(guid)
}

/// Not ACE: `WorldObjectFactory.CreateNewWorldObject(uint weenieClassId)`: the cached weenie, then
/// [`create_new_world_object`] (a guid is taken only when the weenie exists, as in ACE).
pub(crate) fn create_new_world_object_by_wcid(
    w: &mut World,
    weenie_class_id: u32,
) -> Option<ObjectGuid> {
    let weenie = w.content.get_cached_weenie(weenie_class_id)?;
    create_new_world_object(w, weenie)
}

// ACE: Creature.CreateCorpse
/// Create a corpse for both creatures and players currently. `killer` is the top damager (looting
/// rights); `had_vitae` feeds the Olthoi slag rolls.
pub fn create_corpse(
    w: &mut World,
    this: ObjectGuid,
    killer: Option<&DamageHistoryInfo>,
    had_vitae: bool,
) {
    if obj(w, this).no_corpse() {
        if killer.is_some_and(|k| k.is_olthoi_player) {
            return;
        }

        let loot = generate_treasure(w, this, killer, None);

        let location = obj(w, this).location();
        for item in loot {
            let o = obj_mut(w, item);
            if !o.quest().unwrap_or_default().is_empty() {
                // if the item has a Quest string, make the creature a "generator" of the item so that the pickup action applies the quest.
                o.set_generator_id(Some(this.full()));
            }
            // `item.Location = new Position(Location);` (a null Location throws in ACE)
            o.set_location(Some(
                location.expect("NullReferenceException: new Position(Location)"),
            ));
            landblock_manager_add_object(w, item);
        }
        return;
    }

    let cached_weenie = w.content.get_cached_weenie_by_class_name("corpse");

    let corpse = cached_weenie
        .and_then(|weenie| create_new_world_object(w, weenie))
        .filter(|g| w.objects.get(*g).is_some_and(WorldObject::is_corpse))
        .expect("NullReferenceException: CreateNewWorldObject(\"corpse\") as Corpse");

    let mut prefix = "Corpse";

    if obj(w, this).treasure_corpse() {
        // Hardcoded values from PCAPs of Treasure Pile Corpses, everything else lines up exactly with existing corpse weenie
        let c = obj_mut(w, corpse);
        c.set_setup_table_id(0x0200_0EC4);
        c.set_motion_table_id(0x0900_019B);
        c.set_sound_table_id(0x2000_00C2);
        c.set_obj_scale(Some(0.4));

        prefix = "Treasure";
    } else {
        let (
            setup,
            motion,
            palette_base,
            clothing_base,
            physics_table,
            obj_scale,
            palette_template,
            shade,
        ) = {
            let o = obj(w, this);
            (
                o.setup_table_id(),
                o.motion_table_id(),
                o.palette_base_did(),
                o.clothing_base(),
                o.physics_table_id(),
                o.obj_scale(),
                o.palette_template(),
                o.shade(),
            )
        };
        {
            let c = obj_mut(w, corpse);
            c.set_setup_table_id(setup);
            c.set_motion_table_id(motion);
            //corpse.SoundTableId = SoundTableId; // Do not change sound table for corpses
            c.set_palette_base_did(palette_base);
            c.set_clothing_base(clothing_base);
            c.set_physics_table_id(physics_table);

            if obj_scale.is_some() {
                c.set_obj_scale(obj_scale);
            }
            if palette_template.is_some() {
                c.set_palette_template(palette_template);
            }
            if shade.is_some() {
                c.set_shade(shade);
            }
            //if (Translucency.HasValue) // Shadows have Translucency but their corpses do not, videographic evidence can be found on YouTube.
            //corpse.Translucency = Translucency;
        }

        // Pull and save objdesc for correct corpse apperance at time of death
        let obj_desc = dispatch::calculate_obj_desc::calculate_obj_desc(w, this);

        let c = obj_mut(w, corpse);
        c.biota.properties_anim_part = Some(obj_desc.anim_part_changes.clone());

        c.biota.properties_palette = Some(obj_desc.sub_palettes.clone());

        c.biota.properties_texture_map = Some(obj_desc.texture_changes.clone());
    }

    // use the physics location for accuracy,
    // especially while jumping
    let physics_position = physics_obj_position_ace_position(w, this);
    obj_mut(w, corpse).set_location(physics_position);

    let victim_name = name_of(w, this);
    {
        let c = obj_mut(w, corpse);
        c.set_victim_id(Some(this.full()));
        c.set_property(PropertyString::Name, format!("{prefix} of {victim_name}"));
    }

    // set 'killed by' for looting rights
    let mut killer_name = "misadventure".to_owned();
    if let Some(killer) = killer {
        let generator = obj(w, this).wo.world_object_generators.generator;
        if !generator.is_some_and(|g| g == killer.guid) && this != killer.guid {
            if let Some(n) = killer.name.as_deref().filter(|n| !n.trim().is_empty()) {
                killer_name = n.trim_start_matches('+').to_owned(); // vtank requires + to be stripped for regex matching.
            }

            obj_mut(w, corpse).set_killer_id(Some(killer.guid.full()));

            if killer.pet_owner.is_some() {
                if let Some(pet_owner) = killer.try_get_pet_owner(w) {
                    obj_mut(w, corpse).set_killer_id(Some(pet_owner.full()));
                }
            }
        }
    }

    obj_mut(w, corpse).set_long_desc(Some(format!("Killed by {killer_name}.")));

    let mut save_corpse = false;

    let player = obj(w, this).is_player();

    if player {
        let corpse_location = obj(w, corpse).location();
        obj_mut(w, corpse).set_position(PositionType::Location, corpse_location);

        let killer_is_olthoi_player = killer.is_some_and(|k| k.is_olthoi_player);
        let killer_is_pk_player = killer.is_some_and(|k| k.is_player() && k.guid != this);

        //var dropped = killer != null && killer.IsOlthoiPlayer ? player.CalculateDeathItems_Olthoi(corpse, hadVitae) : player.CalculateDeathItems(corpse);

        if killer_is_olthoi_player
            || crate::entity::damage_history_info::player_is_olthoi_player(w, this)
        {
            let dropped = crate::world_objects::player_death::calculate_death_items_olthoi(
                w,
                this,
                corpse,
                had_vitae,
                killer_is_olthoi_player,
                killer_is_pk_player,
            );

            for wo in &dropped {
                do_cantrip_logging(w, this, killer, *wo);
            }

            crate::world_objects::corpse::recalculate_decay_time(w, corpse, this);

            if !dropped.is_empty() {
                save_corpse = true;
            }

            obj_mut(w, corpse).set_pk_level_modifier(PKLevel::PK.0.cs_cast());
        } else {
            let dropped =
                crate::world_objects::player_death::calculate_death_items(w, this, corpse);

            crate::world_objects::corpse::recalculate_decay_time(w, corpse, this);

            if !dropped.is_empty() {
                save_corpse = true;
            }

            let player_cell = obj(w, this)
                .location()
                .expect("NullReferenceException: Player.Location")
                .cell();
            if (player_cell & 0xFFFF) < 0x100 {
                let corpse_location = obj(w, corpse)
                    .location()
                    .expect("NullReferenceException: corpse.Location");
                obj_mut(w, this)
                    .set_position(PositionType::LastOutsideDeath, Some(corpse_location));
                let msg = game_message_private_update_position(
                    obj_mut(w, this),
                    PositionType::LastOutsideDeath,
                    &corpse_location,
                );
                send(w, this, [msg]);

                if !dropped.is_empty() {
                    let coords =
                        crate::entity::position_extensions::get_map_coord_str(&corpse_location)
                            .unwrap_or_default();
                    send(
                        w,
                        this,
                        [chat(&format!("Your corpse is located at ({coords})."))],
                    );
                }
            }

            let killer_guid = killer.map(|k| k.guid.full());
            let is_pk_death =
                crate::world_objects::player_combat::is_pk_death(w, this, killer_guid);
            let is_pkl_death =
                crate::world_objects::player_combat::is_pk_lite_death(w, this, killer_guid);

            if is_pk_death {
                obj_mut(w, corpse).set_pk_level_modifier(PKLevel::PK.0.cs_cast());
            }

            if !is_pk_death && !is_pkl_death {
                let miser_aug = obj(w, this)
                    .augmentation_less_death_item_loss()
                    .wrapping_mul(5);
                if miser_aug > 0 {
                    send(w, this, [chat(&format!("Your augmentation has reduced the number of items you can lose by {miser_aug}!"))]);
                }
            }

            if dropped.is_empty() && !is_pkl_death {
                send(
                    w,
                    this,
                    [chat(
                        "You have retained all your items. You do not need to recover your corpse!",
                    )],
                );
            }
        }
    } else {
        crate::world_objects::corpse::set_is_monster(w, corpse, true);

        if killer.is_none_or(|k| !k.is_olthoi_player) {
            generate_treasure(w, this, killer, Some(corpse));
        } else {
            generate_treasure_olthoi(w, this, killer, corpse);
        }

        if let Some(k) = killer.filter(|k| k.is_player() && !k.is_olthoi_player) {
            let level = obj(w, this).level();
            if level.is_some_and(|l| l >= 100) {
                obj_mut(w, this).set_can_generate_rare(true);
            } else if let Some(killer_player) = k.try_get_attacker(w) {
                let killer_level = obj(w, killer_player).level();
                if let (Some(l), Some(kl)) = (level, killer_level) {
                    if l > kl {
                        obj_mut(w, this).set_can_generate_rare(true);
                    }
                }
            }
        } else {
            obj_mut(w, this).set_can_generate_rare(false);
        }
    }

    obj_mut(w, corpse).remove_property(PropertyInt::Value);

    // DIVERGE: an era without rares (`EraFeatures::pre_order_items_and_rares`) drops none
    // (ClassicACE's `Die` outside its end-of-retail ruleset).
    if let Some(killer) = killer
        .filter(|_| w.era.features.pre_order_items_and_rares && obj(w, this).can_generate_rare())
    {
        crate::world_objects::corpse::try_generate_rare(w, corpse, killer);
    }

    dispatch::init_physics_obj::init_physics_obj(w, corpse);

    // persist the original creature velocity (only used for falling) to corpse
    physics_obj_copy_velocity(w, this, corpse);

    dispatch::enter_world::enter_world(w, corpse);

    if player {
        let spawned = w.objects.get(corpse).is_some_and(|c| c.phys.is_some());
        if spawned {
            let at = obj(w, corpse)
                .location()
                .map(|l| l.to_string())
                .unwrap_or_default();
            log::info!("[CORPSE] {victim_name}'s corpse (0x{corpse}) is located at {at}");
        } else {
            let at = obj(w, this)
                .location()
                .map(|l| l.to_loc_string())
                .unwrap_or_default();
            log::info!(
                "[CORPSE] {victim_name}'s corpse (0x{corpse}) failed to spawn! Tried at {at}"
            );
        }
    }

    if save_corpse {
        dispatch::save_biota_to_database::save_biota_to_database(w, corpse, true);

        for item in container_inventory(w, corpse) {
            dispatch::save_biota_to_database::save_biota_to_database(w, item, true);
        }
    }

    // Not ACE (V203): a corpse whose `EnterWorld` failed is unreferenced once this returns (ACE's
    // garbage collector takes it, and a saved player corpse comes back from the shard with its
    // landblock); here it leaves `World.objects` with its contents.
    crate::world_objects::world_object::drop_unreferenced(w, corpse);
}

// ACE: Creature.GenerateTreasure
// Not ACE's (a fix, V291): ACE adds a treasure item in the creature's own Inventory (InventoryTreasureType loot, or an item EquipInventoryItems could not wield) to the corpse without taking it out of the creature's Inventory, so the creature's Destroy right after marks it destroyed and recycles its guid while the corpse still lists it (ACE's corpse still shows it and it can be looted, but as a destroyed object on a recycled guid); here it leaves the creature's Inventory first.
/// Transfers generated treasure from creature to corpse (or, with no corpse, returns it).
fn generate_treasure(
    w: &mut World,
    this: ObjectGuid,
    killer: Option<&DamageHistoryInfo>,
    corpse: Option<ObjectGuid>,
) -> Vec<ObjectGuid> {
    let mut dropped_items = Vec::new();

    // create death treasure from loot generation factory
    if let Some(death_treasure) = death_treasure(w, this) {
        let items = loot_generation_factory_create_random_loot_objects(w, &death_treasure);
        for wo in items {
            if let Some(corpse) = corpse {
                container_try_add_to_inventory(w, corpse, wo);
            } else {
                dropped_items.push(wo);
            }

            do_cantrip_logging(w, this, killer, wo);
        }
    }

    // move wielded treasure over, which also should include Wielded objects not marked for destroy on death.
    // allow server operators to configure this behavior due to errors in createlist post 16py data
    let drop_flags = if crate::managers::property_manager::get_bool(
        w,
        "creatures_drop_createlist_wield",
        false,
        true,
    )
    .item
    {
        DestinationType::WieldTreasure
    } else {
        DestinationType::Treasure
    };

    let wielded_treasure: Vec<ObjectGuid> = container_inventory(w, this)
        .into_iter()
        .chain(creature_equipped_objects(w, this))
        .filter(|i| {
            (obj(w, *i).wo.world_object.destination_type & drop_flags) != DestinationType::default()
        })
        .collect();
    for item in wielded_treasure {
        if obj(w, item).bonded() == Some(empyrean_entity::enums::BondedStatus::Destroy) {
            continue;
        }

        if creature_try_dequip_object_with_broadcasting(w, this, item) {
            let msg = game_message_public_update_instance_id(
                obj_mut(w, item),
                PropertyInstanceId::Wielder,
                ObjectGuid::INVALID,
            );
            world_object_enqueue_broadcast(w, this, &[msg]);
        }

        // Not ACE's (a fix, V291): an item carried in the creature's own
        // Inventory leaves it here, so the creature's Destroy that follows the death does not
        // destroy the item on the corpse (or on the ground); a wielded item is no longer in the
        // Inventory, so this does nothing for it.
        container_try_remove_from_inventory(w, this, item);

        if let Some(corpse) = corpse {
            container_try_add_to_inventory(w, corpse, item);
            let m1 = game_message_public_update_instance_id(
                obj_mut(w, item),
                PropertyInstanceId::Container,
                corpse,
            );
            let m2 = game_message_pickup_event(obj_mut(w, item));
            world_object_enqueue_broadcast(w, this, &[m1, m2]);
        } else {
            dropped_items.push(item);
        }
    }

    // contain and non-wielded treasure create
    let create_list = obj(w, this).biota.properties_create_list.clone();
    if let Some(create_list) = create_list {
        let create_list: Vec<_> = create_list
            .iter()
            .filter(|i| {
                (i.destination_type & DestinationType::Contain) != DestinationType::default()
                    || (i.destination_type & DestinationType::Treasure)
                        != DestinationType::default()
                        && (i.destination_type & DestinationType::Wield)
                            == DestinationType::default()
            })
            .cloned()
            .collect();

        let selected = creature_create_list_select(w, this, &create_list);

        for item in selected {
            if let Some(wo) = create_new_world_object_from_create_list(w, &item) {
                if let Some(corpse) = corpse {
                    container_try_add_to_inventory(w, corpse, wo);
                } else {
                    dropped_items.push(wo);
                }
            }
        }
    }

    dropped_items
}

// ACE: Creature.GenerateTreasure_Olthoi
/// Generates random amounts of slag on a corpse when an OlthoiPlayer is the killer.
fn generate_treasure_olthoi(
    w: &mut World,
    this: ObjectGuid,
    _killer: Option<&DamageHistoryInfo>,
    corpse: ObjectGuid,
) {
    let Some(death_treasure) = death_treasure(w, this) else {
        return;
    };

    let Some(slag) = loot_generation_factory_roll_slag(w, &death_treasure) else {
        return;
    };

    container_try_add_to_inventory(w, corpse, slag);
}

// ACE: Creature.DoCantripLogging
/// Debug log lines for epic and legendary cantrips on generated loot.
pub fn do_cantrip_logging(
    w: &World,
    this: ObjectGuid,
    killer: Option<&DamageHistoryInfo>,
    wo: ObjectGuid,
) {
    let (epic_cantrips, legendary_cantrips) = world_object_epic_and_legendary_cantrips(w, wo);
    let killer_name = killer.and_then(|k| k.name.clone()).unwrap_or_default();
    let killer_guid = killer.map(|k| k.guid.to_string()).unwrap_or_default();

    if !epic_cantrips.is_empty() && log::log_enabled!(log::Level::Debug) {
        log::debug!(
            "[LOOT][EPIC] {} ({}) generated item with {} epic{} - {} ({}) - {} - killed by {} ({})",
            name_of(w, this),
            this,
            epic_cantrips.len(),
            if epic_cantrips.len() > 1 { "s" } else { "" },
            name_of(w, wo),
            wo,
            get_spell_list(w, &epic_cantrips),
            killer_name,
            killer_guid
        );
    }

    if !legendary_cantrips.is_empty() && log::log_enabled!(log::Level::Debug) {
        log::debug!(
            "[LOOT][LEGENDARY] {} ({}) generated item with {} legendar{} - {} ({}) - {} - killed by {} ({})",
            name_of(w, this),
            this,
            legendary_cantrips.len(),
            if legendary_cantrips.len() > 1 { "ies" } else { "y" },
            name_of(w, wo),
            wo,
            get_spell_list(w, &legendary_cantrips),
            killer_name,
            killer_guid
        );
    }
}

// ACE: Creature.GetSpellList
/// The spell names of a spell table, comma separated.
#[must_use]
pub fn get_spell_list(w: &World, spell_table: &DotNetDict<i32, f32>) -> String {
    let mut spells = Vec::new();

    for (key, _) in spell_table.iter() {
        spells.push(crate::entity::spell::Spell::new(w, (*key).cs_cast(), false));
    }

    spells
        .iter()
        .map(|i| i.name().to_owned())
        .collect::<Vec<_>>()
        .join(", ")
}

// ACE: Creature.IsOnNoDeathXPLandblock
#[must_use]
pub fn is_on_no_death_xp_landblock(w: &World, this: ObjectGuid) -> bool {
    match obj(w, this).location() {
        Some(location) => NO_DEATH_XP_LANDBLOCKS.contains(&(location.landblock_id().raw() >> 16)),
        None => false,
    }
}

// ACE: Creature.NoDeathXP_Landblocks
/// A list of landblocks the player gains no xp from creature kills.
pub const NO_DEATH_XP_LANDBLOCKS: [u32; 12] = [
    0x00B0, // Colosseum Arena One
    0x00B1, // Colosseum Arena Two
    0x00B2, // Colosseum Arena Three
    0x00B3, // Colosseum Arena Four
    0x00B4, // Colosseum Arena Five
    0x5960, // Gauntlet Arena One (Celestial Hand)
    0x5961, // Gauntlet Arena Two (Celestial Hand)
    0x5962, // Gauntlet Arena One (Eldritch Web)
    0x5963, // Gauntlet Arena Two (Eldritch Web)
    0x5964, // Gauntlet Arena One (Radiant Blood)
    0x5965, // Gauntlet Arena Two (Radiant Blood)
    0x596B, // Gauntlet Staging Area (All Societies)
];

// ---------------------------------------------------------------------------------------------
// Not ACE: pointers to members of other ACE files, named after them.
// ---------------------------------------------------------------------------------------------

/// `IsTurning = value` (`Monster_Navigation.cs` field).
fn creature_set_is_turning(w: &mut World, this: ObjectGuid, value: bool) {
    crate::world_objects::monster_navigation::fields_mut(w, this).is_turning = value;
}

/// `IsMoving = value` (`Monster_Navigation.cs` field).
fn creature_set_is_moving(w: &mut World, this: ObjectGuid, value: bool) {
    crate::world_objects::monster_navigation::fields_mut(w, this).is_moving = value;
}

/// `TakeDamage(source, damageType, amount)` (`Monster_Combat.cs`, virtual: the dispatch).
fn creature_take_damage(
    w: &mut World,
    this: ObjectGuid,
    source: ObjectGuid,
    damage_type: DamageType,
    amount: u32,
) {
    #[allow(clippy::cast_precision_loss)] // C#'s `uint` to `float` argument
    let amount = amount as f32;
    let _ = crate::dispatch::take_damage::take_damage(w, this, source, damage_type, amount, false);
}

/// `player.QuestManager.HasQuest(questName)` (`Managers/QuestManager.cs`).
fn quest_manager_has_quest(w: &World, player: ObjectGuid, quest_name: &str) -> bool {
    crate::managers::quest_manager::has_quest(
        w,
        &crate::managers::quest_manager::QuestOwner::Creature(player),
        quest_name,
    )
}

/// `player.QuestManager.HandleKillTask(killTask, this)`.
fn quest_manager_handle_kill_task(
    w: &mut World,
    player: ObjectGuid,
    kill_task: &str,
    creature: ObjectGuid,
) {
    let mut owner = crate::managers::quest_manager::QuestOwner::Creature(player);
    crate::managers::quest_manager::handle_kill_task(w, &mut owner, kill_task, Some(creature));
}

/// `QuestManager.GetQuestName(questFormat)`.
fn quest_manager_get_quest_name(quest_format: &str) -> String {
    crate::managers::quest_manager::get_quest_name(quest_format).to_owned()
}

/// `player.Fellowship != null` (`Player_Fellowship.cs`).
pub(crate) fn player_has_fellowship(w: &World, player: ObjectGuid) -> bool {
    crate::world_objects::player_fellowship::fellowship(w, player).is_some()
}

/// `player.Fellowship.WithinRange(player)` (`Entity/Fellowship.cs`; `includeSelf` false).
fn fellowship_within_range(w: &mut World, player: ObjectGuid) -> Vec<ObjectGuid> {
    let fellowship = crate::world_objects::player_fellowship::fellowship(w, player)
        .expect("ACE: player.Fellowship is null (NullReferenceException)");
    crate::entity::fellowship::within_range(w, &fellowship, player, false)
}

/// `playerDamager.EarnLuminance(amount, xpType)` (`Player_Luminance.cs`).
fn player_earn_luminance(w: &mut World, player: ObjectGuid, amount: i64, xp_type: XpType) {
    crate::world_objects::player_luminance::earn_luminance(
        w,
        player,
        amount,
        xp_type,
        empyrean_entity::enums::ShareType::All,
    );
}

/// `PhysicsObj.StopCompletely(true)`.
fn physics_obj_stop_completely(w: &mut World, this: ObjectGuid) {
    if let Some(h) = obj(w, this).phys {
        crate::physics::motion::stop_completely(w, h, true);
    }
}

/// `ExecuteMotion(new Motion(MotionStance.NonCombat, MotionCommand.Dead))`: broadcasts the death
/// animation and returns its length.
fn world_object_execute_motion_dead(w: &mut World, this: ObjectGuid) -> f32 {
    let motion_death = Motion::new(MotionStance::NonCombat, MotionCommand::Dead, 1.0);
    crate::world_objects::world_object::execute_motion(w, this, motion_death, true, None, false)
}

/// `EmoteManager.OnDeath(lastDamager)`.
fn emote_manager_on_death(
    w: &mut World,
    this: ObjectGuid,
    last_damager: Option<&DamageHistoryInfo>,
) {
    crate::world_objects::managers::emote_manager::on_death(w, this, last_damager);
}

/// `PhysicsObj.Position.ACEPosition()`.
///
/// # Panics
/// When the creature has no physics body (ACE: `NullReferenceException`).
fn physics_obj_position_ace_position(w: &World, this: ObjectGuid) -> Option<Position> {
    let h = obj(w, this)
        .phys
        .expect("ACE: PhysicsObj is null (NullReferenceException)");
    let pos = crate::physics::phys_ext::position(w, h)
        .expect("ACE: PhysicsObj.Position is null (NullReferenceException)");
    Some(crate::entity::position_extensions::ace_position(&pos))
}

/// `corpse.PhysicsObj.Velocity = PhysicsObj.Velocity` (the fields).
///
/// # Panics
/// When either has no physics body (ACE: `NullReferenceException`).
fn physics_obj_copy_velocity(w: &mut World, this: ObjectGuid, corpse: ObjectGuid) {
    let h = obj(w, this)
        .phys
        .expect("ACE: PhysicsObj is null (NullReferenceException)");
    let corpse_h = obj(w, corpse)
        .phys
        .expect("ACE: corpse.PhysicsObj is null (NullReferenceException)");
    let velocity = crate::physics::phys_ext::velocity(w, h);
    crate::physics::phys_ext::set_velocity_field(w, corpse_h, velocity);
}

/// `LandblockManager.AddObject(item)`.
fn landblock_manager_add_object(w: &mut World, item: ObjectGuid) {
    crate::managers::landblock_manager::add_object(w, item, false);
}

/// `WorldObjectFactory.CreateNewWorldObject(createListItem)`: a new dynamic guid, the factory,
/// the guid recycled when it builds nothing; the new object joins `World.objects`.
fn create_new_world_object_from_create_list(
    w: &mut World,
    item: &empyrean_entity::models::properties_create_list::PropertiesCreateList,
) -> Option<ObjectGuid> {
    let wo =
        crate::factories::world_object_factory::create_new_world_object_from_create_list_in_world(
            w, item,
        )?;
    let guid = wo.guid;
    if let Err(dup) = w.objects.insert(wo) {
        panic!("two live objects with guid {:?}", dup.guid);
    }
    Some(guid)
}

/// Adds a newly created object (the factory returns owned objects) to `World.objects`.
fn insert_new_object(w: &mut World, wo: WorldObject) -> ObjectGuid {
    let guid = wo.guid;
    if let Err(dup) = w.objects.insert(wo) {
        panic!("two live objects with guid {:?}", dup.guid);
    }
    guid
}

/// `LootGenerationFactory.CreateRandomLootObjects(profile)` (4.10); the items join `World.objects`.
fn loot_generation_factory_create_random_loot_objects(
    w: &mut World,
    profile: &empyrean_content::models::world::treasure_death::TreasureDeath,
) -> Vec<ObjectGuid> {
    let items = crate::factories::loot_generation_factory::create_random_loot_objects(w, profile);
    items
        .into_iter()
        .map(|wo| insert_new_object(w, wo))
        .collect()
}

/// `LootGenerationFactory.RollSlag(profile)` (4.10); the slag joins `World.objects`.
fn loot_generation_factory_roll_slag(
    w: &mut World,
    profile: &empyrean_content::models::world::treasure_death::TreasureDeath,
) -> Option<ObjectGuid> {
    let slag = crate::factories::loot_generation_factory_olthoi_play::roll_slag(w, profile)?;
    Some(insert_new_object(w, slag))
}

/// `CreateListSelect(createList)` (`Creature_Equipment.cs`, a static member).
fn creature_create_list_select(
    w: &mut World,
    _this: ObjectGuid,
    create_list: &[empyrean_entity::models::properties_create_list::PropertiesCreateList],
) -> Vec<empyrean_entity::models::properties_create_list::PropertiesCreateList> {
    crate::world_objects::creature_equipment::create_list_select(w, create_list)
}

/// `EnqueueBroadcast(msgs)` (`WorldObject_Networking.cs`; `sendSelf` true).
fn world_object_enqueue_broadcast(
    w: &mut World,
    this: ObjectGuid,
    msgs: &[crate::network::game_messages::game_message::GameMessage],
) {
    crate::world_objects::world_object_networking::enqueue_broadcast(w, this, true, msgs);
}

/// `wo.EpicCantrips` / `wo.LegendaryCantrips` (`WorldObject_Magic.cs`):
/// `Biota.GetMatchingSpells(LootTables.EpicCantrips / LegendaryCantrips, BiotaDatabaseLock)`.
fn world_object_epic_and_legendary_cantrips(
    w: &World,
    wo: ObjectGuid,
) -> (DotNetDict<i32, f32>, DotNetDict<i32, f32>) {
    let sets = &*crate::factories::loot_tables::CANTRIP_SETS;
    let biota = &obj(w, wo).biota;
    (
        biota.get_matching_spells(&sets.epic_cantrips),
        biota.get_matching_spells(&sets.legendary_cantrips),
    )
}

/// `Container.Inventory.Values`, in .NET enumeration order.
pub fn container_inventory(w: &World, container: ObjectGuid) -> Vec<ObjectGuid> {
    crate::world_objects::container::inventory_values(w, container)
}

/// `Creature.EquippedObjects.Values`, in .NET enumeration order.
pub fn creature_equipped_objects(w: &World, creature: ObjectGuid) -> Vec<ObjectGuid> {
    crate::world_objects::creature_equipment::equipped_objects_values(w, creature)
}

/// `container.TryAddToInventory(item)`, with ACE's defaults (`placementPosition = 0`,
/// `limitToMainPackOnly = false`, `burdenCheck = true`).
pub fn container_try_add_to_inventory(
    w: &mut World,
    container: ObjectGuid,
    item: ObjectGuid,
) -> bool {
    crate::world_objects::container::try_add_to_inventory(w, container, item, 0, false, true)
}

/// `container.TryRemoveFromInventory(item.Guid)` (`forceSave = false`).
pub fn container_try_remove_from_inventory(
    w: &mut World,
    container: ObjectGuid,
    item: ObjectGuid,
) -> bool {
    crate::world_objects::container::try_remove_from_inventory(w, container, item, false)
}

/// `Creature.TryEquipObject(item, location)`.
pub fn creature_try_equip_object(
    w: &mut World,
    creature: ObjectGuid,
    item: ObjectGuid,
    location: empyrean_entity::enums::EquipMask,
) -> bool {
    crate::world_objects::creature_equipment::try_equip_object(w, creature, item, location)
}

/// `TryDequipObjectWithBroadcasting(item.Guid, out _, out _)` (`Creature_Equipment.cs`;
/// `droppingToLandscape = false`).
fn creature_try_dequip_object_with_broadcasting(
    w: &mut World,
    this: ObjectGuid,
    item: ObjectGuid,
) -> bool {
    crate::world_objects::creature_equipment::try_dequip_object_with_broadcasting(
        w, this, item, false,
    )
    .is_some()
}
