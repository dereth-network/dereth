// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Food.cs
//! Port of `Source/ACE.Server/WorldObjects/Food.cs`.
//!
//! Food and drink: the eat/drink motion (`Player.ApplyConsumable`), then the vital boost, the
//! spell, the use sound, and one of the stack consumed.

use empyrean_common::dotnet::{math, CsCast};
use empyrean_entity::enums::{
    ChatMessageType, DamageType, MotionCommand, PropertyAttribute2nd, Sound, SpellType, WeenieError,
};
use empyrean_entity::ObjectGuid;

use crate::entity::damage_history;
use crate::entity::spell::Spell;
use crate::managers::player_manager::player_session;
use crate::network::game_messages::game_message::enqueue_send;
use crate::network::game_messages::messages::game_message_sound::game_message_sound;
use crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use crate::world_objects::player_inventory::{self, SearchLocations};
use crate::world_objects::world_object::WorldObject;
use crate::world_objects::{
    creature_death, creature_vitals, healer, monster_combat, player, player_networking, player_use,
    world_object_magic, world_object_networking,
};
use crate::{dispatch, World};

/// Non-property fields declared in `Food.cs`.
#[derive(Debug, Default)]
pub struct FoodFields {}

// ================================================================================ helpers

fn obj(w: &World, g: ObjectGuid) -> &WorldObject {
    w.objects.get(g).unwrap_or_else(|| {
        panic!(
            "System.NullReferenceException: object 0x{:08X} is not in World.objects",
            g.full()
        )
    })
}

fn name(w: &World, g: ObjectGuid) -> String {
    dispatch::name::name(w, g).unwrap_or_default()
}

fn chat(w: &mut World, player: ObjectGuid, msg: &str, chat_message_type: ChatMessageType) {
    let m = game_message_system_chat(msg, chat_message_type);
    let session = player_session(w, player).expect("System.NullReferenceException: Player.Session");
    enqueue_send(w, session, m);
}

/// `player.IsBusy || player.Teleporting || player.suicideInProgress`.
pub(crate) fn player_is_busy_teleporting_or_suiciding(w: &World, player: ObjectGuid) -> bool {
    let o = obj(w, player);
    o.wo.world_object.is_busy
        || o.wo.world_object.teleporting
        || o.player
            .as_ref()
            .is_some_and(|p| p.player_death.suicide_in_progress)
}

// ================================================================================ Food.cs

/// This is raised by Player.HandleActionUseItem. The item should be in the players possession.
// ACE: Food.ActOnUse
pub fn food_act_on_use(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    activator: empyrean_entity::ObjectGuid,
) {
    if !w.objects.get(activator).is_some_and(WorldObject::is_player) {
        return;
    }
    let player = activator;

    if player_is_busy_teleporting_or_suiciding(w, player) {
        player_networking::send_weenie_error(w, player, WeenieError::YoureTooBusy);
        return;
    }

    if player::is_jumping(w, player) {
        player_networking::send_weenie_error(w, player, WeenieError::YouCantDoThatWhileInTheAir);
        return;
    }

    let motion_command = if get_use_sound(obj(w, this)) == Sound::Eat1 {
        MotionCommand::Eat
    } else {
        MotionCommand::Drink
    };

    player_use::apply_consumable(
        w,
        player,
        motion_command,
        Box::new(move |w: &mut World| apply_consumable(w, this, player)),
        1.0,
    );
}

/// Applies the boost from the consumable, broadcasts the sound, sends message to player, and
/// consumes from inventory
// ACE: Food.ApplyConsumable
pub fn apply_consumable(w: &mut World, this: ObjectGuid, player: ObjectGuid) {
    if monster_combat::is_dead(obj(w, player)) {
        return;
    }

    // verify item is still valid
    if player_inventory::find_object(w, player, this, SearchLocations::MyInventory)
        .result
        .is_none()
    {
        //player.SendWeenieError(WeenieError.ObjectGone);   // results in 'Unable to move object!' transient error
        let msg = format!("Cannot find the {}", name(w, this)); // custom message
        player_networking::send_transient_error(w, player, &msg);
        return;
    }

    // trying to use a dispel potion while pk timer is active
    // send error message and cancel - do not consume item
    if let Some(spell_did) = obj(w, this).spell_did() {
        let spell = Spell::new(w, spell_did, true);

        if spell.meta_spell_type() == SpellType::Dispel
            && !world_object_magic::verify_dispel_pk_status(w, Some(this), Some(player))
        {
            return;
        }
    }

    if obj(w, this).booster_enum() != PropertyAttribute2nd::Undef {
        boost_vital(w, this, player);
    }

    if obj(w, this).spell_did().is_some() {
        cast_spell(w, this, player);
    }

    let sound_event = game_message_sound(player, get_use_sound(obj(w, this)), 1.0);
    world_object_networking::enqueue_broadcast(w, player, true, &[sound_event]);

    if !obj(w, this).unlimited_use() {
        player_inventory::try_consume_from_inventory_with_networking(w, player, this, 1);
    }
}

/// The vital change: `BoostValue` (a restoring boost scaled by the player's healing rating),
/// the damage history, the message, and death when a negative boost kills.
// ACE: Food.BoostVital
pub fn boost_vital(w: &mut World, this: ObjectGuid, player: ObjectGuid) {
    let booster_enum = obj(w, this).booster_enum();
    let vital = obj(w, player).get_creature_vital(booster_enum);

    let Some(vital) = vital else {
        let msg = format!(
            "{} ({}) contains invalid vital {}",
            name(w, this),
            this,
            booster_enum.to_dotnet_string()
        );
        chat(w, player, &msg, ChatMessageType::Broadcast);
        return;
    };

    let boost_value = obj(w, this).boost_value();

    // only apply to restoration food?
    let rating_mod = if boost_value > 0 {
        healer::creature_get_healing_rating_mod(w, player)
    } else {
        1.0
    };

    let boost_value_scaled = boost_vital_amount(boost_value, rating_mod);

    let vital_change =
        creature_vitals::update_vital_delta(w, player, vital, boost_value_scaled).unsigned_abs();

    if booster_enum == PropertyAttribute2nd::Health {
        if boost_value >= 0 {
            damage_history::on_heal(w, player, vital_change);
        } else {
            damage_history::add(w, player, this, DamageType::Health, vital_change);
        }
    }

    let verb = if boost_value >= 0 {
        "restores"
    } else {
        "takes"
    };

    let msg = format!(
        "The {} {verb} {vital_change} points of your {}.",
        name(w, this),
        booster_enum.to_dotnet_string()
    );
    chat(w, player, &msg, ChatMessageType::Broadcast);

    if monster_combat::is_dead(obj(w, player)) {
        let last_damager = damage_history::of(w, player).last_damager();
        dispatch::on_death::on_death(w, player, last_damager, DamageType::Health, false);
        creature_death::die(w, player);
    }
}

/// `(int)Math.Round(BoostValue * ratingMod)`: the `int` times the `float` rating, rounded half
/// to even.
#[must_use]
pub fn boost_vital_amount(boost_value: i32, rating_mod: f32) -> i32 {
    #[allow(clippy::cast_precision_loss)] // C#'s int * float
    let product = boost_value as f32 * rating_mod;
    math::round(f64::from(product)).cs_cast()
}

/// Casts the food's spell on the player (as the player, not the item).
// ACE: Food.CastSpell
pub fn cast_spell(w: &mut World, this: ObjectGuid, player: ObjectGuid) {
    let spell_did = obj(w, this).spell_did();
    let spell = Spell::new(
        w,
        spell_did.expect("InvalidOperationException: SpellDID"),
        true,
    );

    if spell.not_found() {
        if spell.spell_base.is_some() {
            let msg = format!("{} spell not implemented, yet!", spell.name());
            chat(w, player, &msg, ChatMessageType::System);
        } else {
            let msg = format!("Invalid spell id {}", spell_did.unwrap_or(0));
            chat(w, player, &msg, ChatMessageType::System);
        }

        return;
    }

    // should be 'You cast', instead of 'Item cast'
    // omitting the item caster here, so player is also used for enchantment registry caster,
    // which could prevent some scenarios with spamming enchantments from multiple food sources to protect against dispels
    world_object_magic::try_cast_spell(
        w,
        player,
        &spell,
        Some(player),
        Some(this),
        None,
        false,
        false,
        false,
    );
}

/// `UseSound`, or `Eat1` when it is `Invalid`.
// ACE: Food.GetUseSound
#[must_use]
pub fn get_use_sound(o: &WorldObject) -> Sound {
    let mut use_sound = o.use_sound();

    if use_sound == Sound::Invalid {
        use_sound = Sound::Eat1;
    }

    use_sound
}

// ---- constructors and SetEphemeralValues ----

/// `new Food(weenie, guid)` / `new Food(biota)`: the `Stackable` constructor, then
/// Food's `SetEphemeralValues`.
// ACE: Food.Food
pub fn food_ctor(
    o: &mut crate::world_objects::world_object::WorldObject,
    env: &crate::world_objects::world_object::CtorEnv<'_>,
    src: crate::world_objects::world_object::CtorSource,
) {
    crate::world_objects::stackable::stackable_ctor(o, env, src);
    food_set_ephemeral_values(o, env);
}

// ACE: Food.SetEphemeralValues
fn food_set_ephemeral_values(
    o: &mut crate::world_objects::world_object::WorldObject,
    _env: &crate::world_objects::world_object::CtorEnv<'_>,
) {
    o.wo.world_object.object_description_flags |=
        empyrean_entity::enums::ObjectDescriptionFlag::Food;
}
