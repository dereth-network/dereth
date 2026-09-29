// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Gem.cs
//! Port of `Source/ACE.Server/WorldObjects/Gem.cs`.
//!
//! Gems (and the other `Gem` weenies: potions with a use animation, contracts, rares): the
//! refusals, the signal a gem may send, the rare-gem confirmation and timer, then the spell, the
//! contract, the created item, the sound and one of the stack consumed. `RareId`,
//! `RareUsesTimer` and `UseSendsSignal` are generated in `props/gem.rs`.

use empyrean_common::dotnet::CsCast;
use empyrean_entity::enums::{
    CharacterOption, ChatMessageType, MotionCommand, PropertyBool, SpellType, Usable, WeenieError,
};
use empyrean_entity::ObjectGuid;

use crate::entity::confirmation::Confirmation;
use crate::entity::items_to_receive::ItemsToReceive;
use crate::entity::spell::Spell;
use crate::entity::tailoring;
use crate::managers::player_manager::player_session;
use crate::network::game_event::events::game_event_communication_transient_string::game_event_communication_transient_string;
use crate::network::game_event::events::game_event_weenie_error::game_event_weenie_error;
use crate::network::game_messages::game_message::enqueue_send;
use crate::network::game_messages::messages::game_message_sound::game_message_sound;
use crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use crate::world_objects::managers::{
    confirmation_manager, contract_manager, enchantment_manager_with_caching,
};
use crate::world_objects::player_inventory::{self, SearchLocations};
use crate::world_objects::world_object::WorldObject;
use crate::world_objects::{
    food, monster_combat, player, player_character, player_networking, player_use,
    world_object_equipment, world_object_magic, world_object_networking, world_object_use,
};
use crate::{dispatch, World};

/// Non-property fields declared in `Gem.cs`.
#[derive(Debug, Default)]
pub struct GemFields {}

/// For Rares that use cooldown timers (RareUsesTimer), any other rares with RareUsesTimer may not
/// be used for 3 minutes. Note that if the player logs out, this cooldown timer continues to
/// tick/expire (unlike enchantments). (A mutable `public static int` in ACE that nothing assigns.)
// ACE: Gem.RareTimer
pub const RARE_TIMER: i32 = 180;

// ================================================================================ helpers

fn obj(w: &World, g: ObjectGuid) -> &WorldObject {
    w.objects.get(g).unwrap_or_else(|| {
        panic!(
            "System.NullReferenceException: object 0x{:08X} is not in World.objects",
            g.full()
        )
    })
}

fn obj_mut(w: &mut World, g: ObjectGuid) -> &mut WorldObject {
    w.objects.get_mut(g).unwrap_or_else(|| {
        panic!(
            "System.NullReferenceException: object 0x{:08X} is not in World.objects",
            g.full()
        )
    })
}

fn name(w: &World, g: ObjectGuid) -> String {
    dispatch::name::name(w, g).unwrap_or_default()
}

fn chat(w: &mut World, player: ObjectGuid, msg: &str) {
    let m = game_message_system_chat(msg, ChatMessageType::Broadcast);
    let session = player_session(w, player).expect("System.NullReferenceException: Player.Session");
    enqueue_send(w, session, m);
}

fn transient(w: &mut World, player: ObjectGuid, msg: &str) {
    let session = player_session(w, player).expect("System.NullReferenceException: Player.Session");
    let m = game_event_communication_transient_string(
        w.sessions
            .get_mut(session)
            .expect("the session's game half"),
        msg,
    );
    enqueue_send(w, session, m);
}

// ================================================================================ Gem.cs

/// This is raised by Player.HandleActionUseItem. The item should be in the players possession.
// ACE: Gem.ActOnUse
pub fn gem_act_on_use(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    activator: empyrean_entity::ObjectGuid,
) {
    act_on_use(w, this, activator, false);
}

/// `ActOnUse(activator, confirmed)`: `confirmed` is set when a rare gem's confirmation was
/// accepted.
// ACE: Gem.ActOnUse
pub fn act_on_use(w: &mut World, this: ObjectGuid, activator: ObjectGuid, confirmed: bool) {
    if !w.objects.get(activator).is_some_and(WorldObject::is_player) {
        return;
    }
    let player = activator;

    if food::player_is_busy_teleporting_or_suiciding(w, player) {
        player_networking::send_weenie_error(w, player, WeenieError::YoureTooBusy);
        return;
    }

    if player::is_jumping(w, player) {
        player_networking::send_weenie_error(w, player, WeenieError::YouCantDoThatWhileInTheAir);
        return;
    }

    // `!string.IsNullOrWhiteSpace(UseSendsSignal)`
    if let Some(use_sends_signal) = obj(w, this)
        .use_sends_signal()
        .filter(|s| !s.chars().all(char::is_whitespace))
    {
        if let Some(lb) = obj(w, player).current_landblock {
            crate::entity::landblock::emit_signal(w, lb, player, &use_sends_signal);
        }
        return;
    }

    // handle rare gems
    if obj(w, this).rare_id().is_some()
        && player_character::get_character_option(w, player, CharacterOption::ConfirmUseOfRareGems)
        && !confirmed
    {
        let msg = format!("Are you sure you want to use {}?", name(w, this));
        let confirm = Confirmation::custom(
            player,
            Box::new(move |w: &mut World| act_on_use(w, this, activator, true)),
        );
        if !confirmation_manager::enqueue_send(w, player, confirm, &msg) {
            player_networking::send_weenie_error(w, player, WeenieError::ConfirmationInProgress);
        }
        return;
    }

    if obj(w, this).rare_uses_timer() {
        let current_time = w.now.unix_time;

        let time_elapsed = current_time - obj(w, player).last_rare_used_timestamp();

        if time_elapsed < f64::from(RARE_TIMER) {
            // TODO: get retail message
            let remain_time: i32 = (f64::from(RARE_TIMER) - time_elapsed).ceil().cs_cast();
            chat(
                w,
                player,
                &format!("You may use another timed rare in {remain_time}s"),
            );
            return;
        }
    }

    let use_user_animation = obj(w, this).use_user_animation();
    if use_user_animation != MotionCommand::Invalid {
        // some gems have UseUserAnimation and UseSound, similar to food
        // eg. 7559 - Condensed Dispel Potion

        // the animation is also weird, and differs from food, in that it is the full animation
        // instead of stopping at the 'eat/drink' point... so we pass 0.5 here?

        let anim_mod = if use_user_animation == MotionCommand::MimeDrink
            || use_user_animation == MotionCommand::MimeEat
        {
            0.5
        } else {
            1.0
        };

        player_use::apply_consumable(
            w,
            player,
            use_user_animation,
            Box::new(move |w: &mut World| use_gem(w, this, player)),
            anim_mod,
        );
    } else {
        use_gem(w, this, player);
    }
}

/// The gem's effects: the rare-timer broadcast, the spell (portal summons cast by the gem,
/// item redirects, else by the player), the contract, the created item, the sound, and one of
/// the stack consumed.
// ACE: Gem.UseGem
pub fn use_gem(w: &mut World, this: ObjectGuid, player: ObjectGuid) {
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

    if obj(w, this).rare_uses_timer() {
        let current_time = w.now.unix_time;

        obj_mut(w, player).set_last_rare_used_timestamp(current_time);

        // local broadcast usage
        let m = game_message_system_chat(
            &format!("{} used the rare item {}", name(w, player), name(w, this)),
            ChatMessageType::Broadcast,
        );
        world_object_networking::enqueue_broadcast(w, player, true, &[m]);
    }

    if let Some(spell_did) = obj(w, this).spell_did() {
        let spell = Spell::new(w, spell_did, true);

        // should be 'You cast', instead of 'Item cast'
        // omitting the item caster here, so player is also used for enchantment registry caster,
        // which could prevent some scenarios with spamming enchantments from multiple gem sources to protect against dispels

        // TODO: figure this out better
        if spell.meta_spell_type() == SpellType::PortalSummon {
            world_object_magic::try_cast_spell(
                w,
                this,
                &spell,
                Some(player),
                Some(this),
                None,
                false,
                false,
                false,
            );
        } else if spell.is_impen_bane_type() || spell.is_item_redirectable_type() {
            world_object_magic::try_cast_item_enchantment_with_redirects(
                w,
                player,
                &spell,
                Some(player),
                Some(this),
            );
        } else {
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
    }

    if let Some(use_create_contract_id) = obj(w, this).use_create_contract_id().filter(|&id| id > 0)
    {
        if !contract_manager::add(w, player, use_create_contract_id.cs_cast()) {
            return;
        }

        // this wasn't in retail, but the lack of feedback when using a contract gem just seems jarring so...
        let msg = format!(
            "{} accepted. Click on the quill icon in the lower right corner to open your contract tab to view your active contracts.",
            name(w, this)
        );
        chat(w, player, &msg);
    }

    if obj(w, this).use_create_item().is_some_and(|i| i > 0)
        && !handle_use_create_item(w, this, player)
    {
        return;
    }

    let use_sound = obj(w, this).use_sound();
    if use_sound.0 > 0 {
        let m = game_message_sound(player, use_sound, 1.0);
        let session =
            player_session(w, player).expect("System.NullReferenceException: Player.Session");
        enqueue_send(w, session, m);
    }

    if !obj(w, this)
        .get_property(PropertyBool::UnlimitedUse)
        .unwrap_or(false)
    {
        player_inventory::try_consume_from_inventory_with_networking(w, player, this, 1);
    }
}

/// Creates `UseCreateQuantity` (default 1) of `UseCreateItem` in the player's inventory, in
/// stacks; refused (with ACE's messages) when the player lacks the burden or the room.
// ACE: Gem.HandleUseCreateItem
pub fn handle_use_create_item(w: &mut World, this: ObjectGuid, player: ObjectGuid) -> bool {
    let amount = obj(w, this).use_create_quantity().unwrap_or(1);
    let use_create_item = obj(w, this)
        .use_create_item()
        .expect("InvalidOperationException: UseCreateItem");

    let mut items_to_receive = ItemsToReceive::new(w, player);

    items_to_receive.add(w, use_create_item, amount);

    if items_to_receive.player_exceeds_limits() {
        if items_to_receive.player_exceeds_available_burden() {
            transient(w, player, "You are too encumbered to use that!");
        } else if items_to_receive.player_out_of_inventory_slots() {
            transient(w, player, "You do not have enough pack space to use that!");
        } else if items_to_receive.player_out_of_container_slots() {
            transient(
                w,
                player,
                "You do not have enough container slots to use that!",
            );
        }

        return false;
    }

    if items_to_receive.required_slots() > 0 {
        let mut remaining = amount;

        while remaining > 0 {
            let item = world_object_equipment::create_new_world_object_by_wcid(w, use_create_item)
                .expect("System.NullReferenceException: item");
            let item_guid = item.guid;
            w.objects.insert(item).unwrap_or_else(|_| {
                panic!(
                    "a new dynamic guid 0x{:08X} is already live",
                    item_guid.full()
                )
            });

            if obj(w, item_guid).is_stackable() {
                let stack_size =
                    remaining.min(obj(w, item_guid).max_stack_size().map_or(1, i32::from));

                obj_mut(w, item_guid).set_stack_size(Some(stack_size));
                remaining = remaining.wrapping_sub(stack_size);
            } else {
                remaining = remaining.wrapping_sub(1);
            }

            if player_inventory::try_create_in_inventory_with_networking(w, player, item_guid)
                .is_none()
            {
                // DIVERGE: ACE drops an item TryCreateInInventoryWithNetworking refused (the GC takes it); it leaves World.objects (4.5a's rule for unplaced new objects).
                w.objects.remove(item_guid);
            }
        }
    } else {
        let msg = format!("Unable to use {} at this time!", name(w, this));
        player_networking::send_transient_error(w, player, &msg);
        return false;
    }
    true
}

/// A tailoring kit goes to `Tailoring`; anything else falls back on the recipe manager.
// ACE: Gem.HandleActionUseOnTarget
pub fn gem_handle_action_use_on_target(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    player: empyrean_entity::ObjectGuid,
    target: empyrean_entity::ObjectGuid,
) {
    // should tailoring kit / aetheria be subtyped?
    if tailoring::is_tailoring_kit(obj(w, this).biota.weenie_class_id) {
        tailoring::use_object_on_target(w, player, this, target);
        return;
    }

    // fallback on recipe manager?
    world_object_use::world_object_handle_action_use_on_target(w, this, player, target);
}

/// A gem used from the pack (`ItemUseable == Contained`) refuses a busy or dead player, starting
/// the cooldown anyway; a gem not in the pack does nothing; then the base activation.
// ACE: Gem.OnActivate
pub fn gem_on_activate(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    activator: empyrean_entity::ObjectGuid,
) {
    if obj(w, this).item_useable() == Some(Usable::Contained)
        && w.objects.get(activator).is_some_and(WorldObject::is_player)
    {
        let player = activator;
        let contained_item = player_inventory::find_object(
            w,
            player,
            this,
            SearchLocations::MyInventory | SearchLocations::MyEquippedItems,
        )
        .result;
        if contained_item.is_some() {
            // item is contained by player
            if food::player_is_busy_teleporting_or_suiciding(w, player) {
                weenie_error(w, player, WeenieError::YoureTooBusy);
                enchantment_manager_with_caching::start_cooldown(w, player, this);
                return;
            }

            if monster_combat::is_dead(obj(w, player)) {
                weenie_error(w, player, WeenieError::Dead);
                enchantment_manager_with_caching::start_cooldown(w, player, this);
                return;
            }
        } else {
            return;
        }
    }

    world_object_use::world_object_on_activate(w, this, activator);
}

/// `player.Session.Network.EnqueueSend(new GameEventWeenieError(player.Session, error))`.
fn weenie_error(w: &mut World, player: ObjectGuid, error: WeenieError) {
    let session = player_session(w, player).expect("System.NullReferenceException: Player.Session");
    let m = game_event_weenie_error(
        w.sessions
            .get_mut(session)
            .expect("the session's game half"),
        error,
    );
    enqueue_send(w, session, m);
}

// ---- constructors and SetEphemeralValues ----

/// `new Gem(weenie, guid)` / `new Gem(biota)`: the `Stackable` constructor, then
/// Gem's `SetEphemeralValues`.
// ACE: Gem.Gem
pub fn gem_ctor(
    o: &mut crate::world_objects::world_object::WorldObject,
    env: &crate::world_objects::world_object::CtorEnv<'_>,
    src: crate::world_objects::world_object::CtorSource,
) {
    crate::world_objects::stackable::stackable_ctor(o, env, src);
    gem_set_ephemeral_values(o, env);
}

/// Empty in ACE.
// ACE: Gem.SetEphemeralValues
fn gem_set_ephemeral_values(
    _o: &mut crate::world_objects::world_object::WorldObject,
    _env: &crate::world_objects::world_object::CtorEnv<'_>,
) {
}
