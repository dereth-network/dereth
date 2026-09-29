// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Player_Vitals.cs
//! Port of `Source/ACE.Server/WorldObjects/Player_Vitals.cs`.

use empyrean_common::dotnet::CsCast;
use empyrean_entity::enums::{
    ChatMessageType, PlayScript, PropertyAttribute2nd, PropertyInt, Sound, Vital,
};
use empyrean_entity::ObjectGuid;

use crate::network::game_messages::game_message::GameMessage;
use crate::network::game_messages::messages::game_message_private_update_attribute2nd_level::game_message_private_update_attribute2nd_level;
use crate::network::game_messages::messages::game_message_private_update_property_int::game_message_private_update_property_int;
use crate::network::game_messages::messages::game_message_private_update_vital::game_message_private_update_vital;
use crate::network::game_messages::messages::game_message_sound::game_message_sound;
use crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use crate::world_objects::creature_vitals;
use crate::world_objects::entity::creature_attribute::StatCtx;
use crate::world_objects::entity::creature_vital::{get_gear_max_health, CreatureVital};
use crate::world_objects::player_skills::{
    exceeds_available_experience, handle_run_rate_update, name, property_manager_get_bool, send,
    spend_xp,
};
use crate::world_objects::world_object::{play_particle_effect, WorldObject};
use crate::World;

/// Non-property fields declared in `Player_Vitals.cs`.
#[derive(Debug, Default)]
pub struct PlayerVitalsFields {}

fn obj(w: &World, this: ObjectGuid) -> &WorldObject {
    w.objects
        .get(this)
        .unwrap_or_else(|| panic!("Player {this:?}: missing object"))
}

fn obj_mut(w: &mut World, this: ObjectGuid) -> &mut WorldObject {
    w.objects
        .get_mut(this)
        .unwrap_or_else(|| panic!("Player {this:?}: missing object"))
}

/// `new GameMessagePrivateUpdateVital(this, creatureVital)`.
pub(crate) fn update_vital_message(
    w: &mut World,
    this: ObjectGuid,
    creature_vital: CreatureVital,
) -> GameMessage {
    game_message_private_update_vital(obj_mut(w, this), creature_vital)
}

/// `new GameMessagePrivateUpdateAttribute2ndLevel(this, vital, current)`.
fn update_level_message(
    w: &mut World,
    this: ObjectGuid,
    vital: Vital,
    current: u32,
) -> GameMessage {
    game_message_private_update_attribute2nd_level(obj_mut(w, this), vital, current)
}

// ACE: Player.HandleActionRaiseVital
/// GameAction 0x44 RaiseVital: spends `amount` XP on a vital.
pub fn handle_action_raise_vital(
    w: &mut World,
    this: ObjectGuid,
    vital: PropertyAttribute2nd,
    amount: u32,
) -> bool {
    let Some(creature_vital) = obj(w, this).vitals().get(&vital).copied() else {
        log::warn!(
            "{}.HandleActionRaiseVital({}, {amount}) - invalid vital",
            name(w, this),
            vital.to_dotnet_string()
        );
        return false;
    };

    if exceeds_available_experience(w, this, i64::from(amount)) {
        // there is a client bug for vitals only,

        // where the client will enable the button to raise a vital by 10
        // if the player only has enough AvailableExperience to raise it by 1

        let session = crate::world_objects::player_skills::session(w, this);
        crate::network::chat_packet::send_server_message(
            w,
            Some(session),
            &format!("Your attempt to raise {} has failed.", vital.to_sentence()),
            ChatMessageType::Broadcast,
        );

        log::warn!(
            "{}.HandleActionRaiseVital({}, {amount}) - amount > AvailableExperience",
            name(w, this),
            vital.to_dotnet_string()
        );
        return false;
    }

    let prev_rank = creature_vital.ranks(obj(w, this));

    if !spend_vital_xp(w, this, creature_vital, amount, true) {
        return false;
    }

    let update = update_vital_message(w, this, creature_vital);
    send(w, this, [update]);

    if prev_rank != creature_vital.ranks(obj(w, this)) {
        // checks if max rank is achieved and plays fireworks w/ special text
        let mut suffix = "";
        if creature_vital.is_max_rank(w, obj(w, this)) {
            // fireworks
            play_particle_effect(w, this, PlayScript::WeddingBliss, this, 1.0f32);
            suffix = " and has reached its upper limit";
        }

        let sound = game_message_sound(this, Sound::RaiseTrait, 1.0f32);
        let base = creature_vital.base(w, obj(w, this));
        let msg = game_message_system_chat(
            &format!("Your base {} is now {base}{suffix}!", vital.to_sentence()),
            ChatMessageType::Advancement,
        );

        send(w, this, [sound, msg]);
    }
    true
}

// ACE: Player.SpendVitalXp
/// Spends `amount` on the vital if it is below max rank and the amount fits; recomputes ranks.
pub(crate) fn spend_vital_xp(
    w: &mut World,
    this: ObjectGuid,
    creature_vital: CreatureVital,
    amount: u32,
    send_network_update: bool,
) -> bool {
    // ensure vital is not already max rank
    if creature_vital.is_max_rank(w, obj(w, this)) {
        log::warn!(
            "{}.SpendVitalXp({}, {amount}) - player tried to raise vital beyond max rank",
            name(w, this),
            creature_vital.vital.to_dotnet_string()
        );
        return false;
    }

    // the client should already handle this naturally,
    // but ensure player can't spent xp beyond the max rank
    let amount_to_end = creature_vital.experience_left(w, obj(w, this));

    if amount > amount_to_end {
        log::warn!(
            "{}.SpendVitalXp({}, {amount}) - player tried to raise vital beyond {amount_to_end} experience",
            name(w, this),
            creature_vital.vital.to_dotnet_string()
        );
        return false; // returning error here, instead of setting amount to amountToEnd
    }

    // everything looks good at this point,
    // spend xp on vital
    if !spend_xp(w, this, i64::from(amount), send_network_update) {
        log::warn!(
            "{}.SpendVitalXp({}, {amount}) - SpendXP failed",
            name(w, this),
            creature_vital.vital.to_dotnet_string()
        );
        return false;
    }

    let o = obj_mut(w, this);
    let spent = creature_vital.experience_spent(o).wrapping_add(amount);
    creature_vital.set_experience_spent(o, spent);

    // calculate new rank
    let rank: u16 = calc_vital_rank(w, spent).cs_cast();
    creature_vital.set_ranks(obj_mut(w, this), u32::from(rank));

    true
}

// ACE: Player.SpendAllAvailableVitalXp
/// Spends as much available XP on the vital as it can take.
pub fn spend_all_available_vital_xp(
    w: &mut World,
    this: ObjectGuid,
    creature_vital: CreatureVital,
    send_network_update: bool,
) {
    let mut amount_remaining = creature_vital.experience_left(w, obj(w, this));

    if exceeds_available_experience(w, this, i64::from(amount_remaining)) {
        let available = obj(w, this).available_experience().unwrap_or(0);
        amount_remaining = available.cs_cast();
    }

    spend_vital_xp(
        w,
        this,
        creature_vital,
        amount_remaining,
        send_network_update,
    );
}

// ACE: Player.SetMaxVitals
/// The Creature fill, then the three current-level updates to the client.
pub fn player_set_max_vitals(w: &mut World, this: ObjectGuid) {
    creature_vitals::creature_set_max_vitals(w, this);

    let (h, s, m) = {
        let o = obj(w, this);
        (
            o.health().current(o),
            o.stamina().current(o),
            o.mana().current(o),
        )
    };
    let health = update_level_message(w, this, Vital::Health, h);
    let stamina = update_level_message(w, this, Vital::Stamina, s);
    let mana = update_level_message(w, this, Vital::Mana, m);

    send(w, this, [health, stamina, mana]);

    if fellowship_is_set(w, this) {
        set_fellow_vital_update(w, this);
    }
}

// ACE: Player.UpdateVital
/// The Creature update, then (on a change) the client update, the fellowship flag, and the
/// exhaustion hooks for stamina.
pub fn player_update_vital(
    w: &mut World,
    this: ObjectGuid,
    vital: CreatureVital,
    new_val: i32,
) -> i32 {
    let prev_val = vital.current(obj(w, this));

    let change = creature_vitals::creature_update_vital(w, this, vital, new_val);

    if change == 0 {
        return 0;
    }

    let current = vital.current(obj(w, this));
    let msg = update_level_message(w, this, vital.to_enum(), current);
    send(w, this, [msg]);

    if fellowship_is_set(w, this) {
        set_fellow_vital_update(w, this);
    }

    // check for exhaustion
    if vital.vital == PropertyAttribute2nd::Stamina
        || vital.vital == PropertyAttribute2nd::MaxStamina
    {
        // when the stamina actually reached 0 (a request below 0 is clamped to it)
        if current == 0 {
            on_exhausted(w, this);
        }
        // retail was missing the 'exhausted done' automatic hook here
        else if prev_val == 0 && property_manager_get_bool(w, "runrate_add_hooks") {
            handle_run_rate_update(w, this);
        }
    }
    change
}

// ACE: Player.HandleTargetVitals
/// On the player heartbeat: sends the selected target's health percentage.
pub fn handle_target_vitals(w: &mut World, this: ObjectGuid) {
    // `selectedTarget?.TryGetWorldObject() as Creature`
    let Some(target) = selected_target(w, this) else {
        return;
    };
    let Some(t) = w.objects.get(target).filter(|t| t.is_creature()) else {
        return;
    };

    let health = t.health();
    if health.current(t) == 0 {
        return;
    }

    let current = health.current(t);
    #[allow(clippy::cast_precision_loss)]
    let health_percent =
        current as f32 / health.max_value(&mut StatCtx::in_world(w, target)) as f32;

    let session = crate::world_objects::player_skills::session(w, this);
    let session_data = w
        .sessions
        .get_mut(session)
        .expect("NullReferenceException: Player.Session");
    let msg =
        crate::network::game_event::events::game_event_update_health::game_event_update_health(
            session_data,
            target.full(),
            health_percent,
        );
    crate::network::game_messages::game_message::enqueue_send(w, session, msg);
}

// ACE: Player.CalcVitalRank
/// The highest vital rank `xp_amount` buys (the shared rules' search; see
/// [`crate::world_objects::player_attributes::calc_attribute_rank`]).
#[must_use]
pub fn calc_vital_rank(w: &World, xp_amount: u32) -> i32 {
    let xp_table = w.dats.portal_dat().xp_table();
    let rank = dereth_rules::advancement::attribute_2nd_level_from_experience(xp_table, xp_amount);
    i32::try_from(rank).expect("a List index fits an int")
}

// ACE: Player.VitalHeartBeat
/// The Creature regeneration tick, flagging the fellowship on a change.
pub fn player_vital_heart_beat(w: &mut World, this: ObjectGuid) -> bool {
    let vital_update = creature_vitals::creature_vital_heart_beat(w, this);

    if vital_update && fellowship_is_set(w, this) {
        set_fellow_vital_update(w, this);
    }

    vital_update
}

// ACE: Player.HandleMaxHealthUpdate
/// When gear with GearMaxHealth is equipped or removed: updates the property, clamps health,
/// and tells the client.
pub fn handle_max_health_update(w: &mut World, this: ObjectGuid) {
    let gear_max_health = get_gear_max_health(w, obj(w, this));

    {
        let o = obj_mut(w, this);
        if gear_max_health == 0 {
            o.set_gear_max_health(None);
        } else {
            o.set_gear_max_health(Some(gear_max_health));
        }
    }

    let msg = game_message_private_update_property_int(
        obj_mut(w, this),
        PropertyInt::GearMaxHealth,
        gear_max_health,
    );
    send(w, this, [msg]);

    let health = obj(w, this).health();
    let current = health.current(obj(w, this));
    let max = health.max_value(&mut StatCtx::in_world(w, this));
    if current > max {
        health.set_current(obj_mut(w, this), max);
    }

    let current = health.current(obj(w, this));
    let msg = update_level_message(w, this, Vital::Health, current);
    send(w, this, [msg]);
}

// ---------------------------------------------------------------------------------------------
// Not ACE: pointers to members ported in other files, named after them.
// ---------------------------------------------------------------------------------------------

/// `Fellowship != null` (`Player_Fellowship.cs`).
fn fellowship_is_set(w: &World, this: ObjectGuid) -> bool {
    crate::world_objects::player_fellowship::fellowship(w, this).is_some()
}

/// `FellowVitalUpdate = true` (`Player_Fellowship.cs` field).
fn set_fellow_vital_update(w: &mut World, this: ObjectGuid) {
    crate::world_objects::player_fellowship::set_fellow_vital_update(w, this, true);
}

/// `Player.OnExhausted()` (`Player.cs`).
fn on_exhausted(w: &mut World, this: ObjectGuid) {
    crate::world_objects::player::on_exhausted(w, this);
}

/// `selectedTarget?.TryGetWorldObject()` (`Player_Tracking.cs`).
fn selected_target(w: &World, this: ObjectGuid) -> Option<ObjectGuid> {
    crate::world_objects::player_tracking::selected_target(w, this)
}
