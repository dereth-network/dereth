// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Chest.cs
//! Port of `Source/ACE.Server/WorldObjects/Chest.cs`.
//!
//! A chest is a `Container` with a lock (`lock.rs`), an open/close motion, a reset timer that
//! closes it, relocks it and regenerates its generated contents, and the unlocker's window of
//! first access. The typed properties (`ChestRegenOnClose`, `ChestClearedWhenClosed`,
//! `LockCode`) are generated in `props/chest.rs`.

use empyrean_entity::enums::{MotionCommand, MotionStance, PropertyBool, Sound};
use empyrean_entity::ObjectGuid;

use crate::entity::actions::action_chain::ActionChain;
use crate::entity::actions::i_actor::Actor;
use crate::entity::activation_result::ActivationResult;
use crate::managers::property_manager;
use crate::managers::quest_manager::{self, QuestOwner};
use crate::network::game_messages::messages::game_message_public_update_property_bool::game_message_public_update_property_bool;
use crate::network::game_messages::messages::game_message_sound::game_message_sound;
use crate::network::motion::movement_data::Motion;
use crate::world_objects::lock::{self, UnlockResults};
use crate::world_objects::managers::emote_manager;
use crate::world_objects::world_object::WorldObject;
use crate::world_objects::{
    container, player_networking, world_object, world_object_generators, world_object_networking,
    world_object_use,
};
use crate::{dispatch, World};

/// Non-property fields declared in `Chest.cs`.
#[derive(Debug, Default)]
pub struct ChestFields {}

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

/// `protected static readonly Motion motionOpen = new Motion(MotionStance.NonCombat, MotionCommand.On)`.
// ACE: Chest.motionOpen
fn motion_open() -> Motion {
    Motion::new(MotionStance::NonCombat, MotionCommand::On, 1.0)
}

/// `protected static readonly Motion motionClosed = new Motion(MotionStance.NonCombat, MotionCommand.Off)`.
// ACE: Chest.motionClosed
fn motion_closed() -> Motion {
    Motion::new(MotionStance::NonCombat, MotionCommand::Off, 1.0)
}

// ================================================================================ Chest.cs

/// This is the default setup for resetting chests: `ResetInterval ?? Default_ChestResetInterval`,
/// the default again below 15 s.
// ACE: Chest.ChestResetInterval
#[must_use]
pub fn chest_reset_interval(w: &World, this: ObjectGuid) -> f64 {
    let mut chest_reset_interval = match obj(w, this).reset_interval() {
        Some(r) => r,
        None => dispatch::default_chest_reset_interval::default_chest_reset_interval(w, this),
    };

    if chest_reset_interval < 15.0 {
        chest_reset_interval =
            dispatch::default_chest_reset_interval::default_chest_reset_interval(w, this);
    }

    chest_reset_interval
}

/// `ChestRegenOnClose` (the generated property, given this chest's `ChestResetInterval`).
fn chest_regen_on_close(w: &World, this: ObjectGuid) -> bool {
    let interval = chest_reset_interval(w, this);
    obj(w, this).chest_regen_on_close(interval)
}

// ACE: Chest.Default_ChestResetInterval
#[allow(unused_variables)]
pub fn chest_default_chest_reset_interval(
    w: &crate::World,
    this: empyrean_entity::ObjectGuid,
) -> f64 {
    120.0
}

/// The base requirements, then: players only; a locked chest refuses (with the locked sound);
/// the unlocker's window; an open chest closes for its viewer (or a vanished viewer) and refuses
/// anyone else; the quest requirement.
// ACE: Chest.CheckUseRequirements
#[allow(clippy::if_same_then_else)] // ACE's two quest branches, kept apart
pub fn chest_check_use_requirements(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    activator: empyrean_entity::ObjectGuid,
) -> crate::entity::activation_result::ActivationResult {
    let base_requirements =
        world_object_use::world_object_check_use_requirements(w, this, activator);
    if !base_requirements.success {
        return base_requirements;
    }

    if !w.objects.get(activator).is_some_and(WorldObject::is_player) {
        return ActivationResult::new(false);
    }
    let player = activator;

    if obj(w, this).is_locked() {
        if property_manager::get_bool(w, "fix_chest_missing_inventory_window", false, true).item {
            let msg = format!(
                "The {} is locked",
                dispatch::name::name(w, this).unwrap_or_default()
            );
            player_networking::send_transient_error(w, player, &msg);
        }

        let sound = game_message_sound(this, Sound::OpenFailDueToLock, 1.0);
        world_object_networking::enqueue_broadcast(w, this, true, &[sound]);
        return ActivationResult::new(false);
    }

    let o = obj(w, this);
    // `activator.Guid.Full != LastUnlocker`: lifted, so a null LastUnlocker is never equal
    if let Some(use_lock_timestamp) = o.use_lock_timestamp() {
        if o.last_unlocker() != Some(activator.full()) {
            let current_time = w.now.unix_time;

            // prevent ninja looting
            if use_lock_timestamp
                + property_manager::get_double(w, "unlocker_window", 0.0, true).item
                > current_time
            {
                let msg = container::in_use_message(w, this);
                player_networking::send_transient_error(w, player, &msg);
                return ActivationResult::new(false);
            }
        }
    }

    if obj(w, this).is_open() {
        let viewer = obj(w, this).viewer();
        if viewer == player.full() {
            // current player has this chest open, close it
            dispatch::close::close(w, this, player);
        } else {
            // another player has this chest open -- ensure they are within range
            let current_landblock = obj(w, this)
                .current_landblock
                .expect("System.NullReferenceException: CurrentLandblock");
            let current_viewer = crate::entity::landblock::get_object(
                w,
                current_landblock,
                ObjectGuid::new(viewer),
                true,
            )
            .filter(|&g| obj(w, g).is_player());

            if current_viewer.is_none() {
                dispatch::close::close(w, this, ObjectGuid::INVALID); // current viewer not found, close it
            } else {
                let msg = container::in_use_message(w, this);
                player_networking::send_transient_error(w, player, &msg);
            }
        }

        return ActivationResult::new(false);
    }

    // handle quest requirements
    if let Some(quest) = obj(w, this).quest() {
        let owner = QuestOwner::Creature(player);
        if !quest_manager::has_quest(w, &owner, &quest) {
            emote_manager::on_quest(w, this, player);
        } else if quest_manager::can_solve(w, &owner, &quest) {
            emote_manager::on_quest(w, this, player);
        } else {
            quest_manager::handle_solve_error(w, &owner, &quest);
            return ActivationResult::new(false);
        }
    }

    ActivationResult::new(true)
}

/// This is raised by Player.HandleActionUseItem. The item does not exist in the players
/// possession. If the item was outside of range, the player will have been commanded to move
/// using DoMoveTo before ActOnUse is called. When this is called, it should be assumed that the
/// player is within range.
// ACE: Chest.ActOnUse
pub fn chest_act_on_use(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    activator: empyrean_entity::ObjectGuid,
) {
    if !w.objects.get(activator).is_some_and(WorldObject::is_player) {
        return;
    }

    // open chest
    dispatch::open::open(w, this, activator);
}

/// `Container.Open`, then the reset timer (once per reset cycle), and the unlocker's window ends.
// ACE: Chest.Open
pub fn chest_open(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    player: empyrean_entity::ObjectGuid,
) {
    container::container_open(w, this, player);

    let chest_reset_interval = chest_reset_interval(w, this);
    if !obj(w, this).reset_message_pending() && chest_reset_interval != f64::INFINITY {
        let reset_timestamp = obj(w, this).reset_timestamp();

        let mut action_chain = ActionChain::new();
        action_chain.add_delay_seconds(w, chest_reset_interval);
        action_chain.add_action(Actor::Object(this), move |w: &mut World| {
            reset(w, this, reset_timestamp)
        });
        action_chain.enqueue_chain(w);

        obj_mut(w, this).set_reset_message_pending(true);
    }

    obj_mut(w, this).set_use_lock_timestamp(None);
}

/// The virtual `Close(Player)`: `Close(player, tryReset: true)` (C# overload resolution prefers
/// the non-override `Close(Player, bool = true)` declared in `Chest`).
// ACE: Chest.Close
pub fn chest_close(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    player: empyrean_entity::ObjectGuid,
) {
    close(w, this, player, true);
}

/// Called when a chest is closed, or walked away from. `player` is `ObjectGuid::INVALID` for
/// ACE's `null`.
// ACE: Chest.Close
pub fn close(w: &mut World, this: ObjectGuid, player: ObjectGuid, try_reset: bool) {
    container::container_close(w, this, player);

    if chest_regen_on_close(w, this) && try_reset {
        let reset_timestamp = obj(w, this).reset_timestamp();
        reset(w, this, reset_timestamp);
    }
}

/// `Container.FinishClose`; a chest cleared when closed fades out once its generated contents
/// are all gone.
// ACE: Chest.FinishClose
pub fn chest_finish_close(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    player: empyrean_entity::ObjectGuid,
) {
    container::container_finish_close(w, this, player);

    let o = obj(w, this);
    if o.chest_cleared_when_closed() && o.init_generated_objects() > 0 && o.current_create() == 0 {
        world_object::fade_out_and_destroy(w, this, true); // Chest's complete generated inventory count has been wiped out
                                                           //Destroy(); // Chest's complete generated inventory count has been wiped out
    }
}

/// The reset timer: unless an earlier reset already ran (`resetTimestamp` no longer current),
/// closes the chest, relocks it, clears what players left in it and regenerates its contents.
// ACE: Chest.Reset
pub fn reset(w: &mut World, this: ObjectGuid, reset_timestamp: Option<f64>) {
    let Some(o) = w.objects.get(this) else { return };
    #[allow(clippy::float_cmp)] // C#'s lifted != on double?
    if reset_timestamp != o.reset_timestamp() {
        return; // already cleared by previous reset
    }

    // TODO: if 'ResetInterval' style, do we want to ensure a minimum amount of time for the last viewer?

    // should only be an edge case with reload-landblock
    let Some(current_landblock) = o.current_landblock else {
        return;
    };

    let player = crate::entity::landblock::get_object(
        w,
        current_landblock,
        ObjectGuid::new(o.viewer()),
        true,
    )
    .filter(|&g| obj(w, g).is_player())
    .unwrap_or(ObjectGuid::INVALID);

    if obj(w, this).is_open() {
        close(w, this, player, false);
    }

    let o = obj(w, this);
    if o.default_locked() && !o.is_locked() {
        let o = obj_mut(w, this);
        o.set_is_locked(true);
        if !property_manager::get_bool(w, "fix_chest_missing_inventory_window", false, true).item {
            let o = obj_mut(w, this);
            let is_locked = o.is_locked();
            let m = game_message_public_update_property_bool(o, PropertyBool::Locked, is_locked);
            world_object_networking::enqueue_broadcast(w, this, true, &[m]);
        }
    }

    container::clear_unmanaged_inventory(w, this, false);

    if obj(w, this).is_generator() {
        dispatch::reset_generator::reset_generator(w, this);
        obj_mut(w, this).set_currently_powering_up(true);
        if obj(w, this).init_generated_objects() > 0 {
            world_object_generators::generator_generate(w, this);
        }
    }

    let now = w.now.unix_time;
    let o = obj_mut(w, this);
    o.set_reset_timestamp(Some(now));
    o.set_reset_message_pending(false);
}

// ACE: Chest.DoOnOpenMotionChanges
pub fn chest_do_on_open_motion_changes(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
) -> f32 {
    if obj(w, this).motion_table_id() != 0 {
        world_object::execute_motion(w, this, motion_open(), true, None, false)
    } else {
        0.0
    }
}

// ACE: Chest.DoOnCloseMotionChanges
pub fn chest_do_on_close_motion_changes(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
) -> f32 {
    if obj(w, this).motion_table_id() != 0 {
        world_object::execute_motion(w, this, motion_closed(), true, None, false)
    } else {
        0.0
    }
}

/// Used for unlocking a chest via lockpick, so contains a skill check. On success the unlocker
/// gets the chest's window of first access.
// ACE: Chest.Unlock
pub fn unlock_lockpick(
    w: &mut World,
    this: ObjectGuid,
    unlocker_guid: u32,
    player_lockpick_skill_lvl: u32,
    difficulty: &mut i32,
) -> UnlockResults {
    let result = lock::unlock_lockpick(w, this, player_lockpick_skill_lvl, difficulty);

    if result == UnlockResults::UnlockSuccess {
        let now = w.now.unix_time;
        let o = obj_mut(w, this);
        o.set_last_unlocker(Some(unlocker_guid));
        o.set_use_lock_timestamp(Some(now));
    }
    result
}

/// Used for unlocking a chest via a key.
// ACE: Chest.Unlock
pub fn unlock_key(
    w: &mut World,
    this: ObjectGuid,
    unlocker_guid: u32,
    key: Option<ObjectGuid>,
    key_code: Option<&str>,
) -> UnlockResults {
    let result = lock::unlock_key(w, this, key, key_code);

    if result == UnlockResults::UnlockSuccess {
        let now = w.now.unix_time;
        let o = obj_mut(w, this);
        o.set_last_unlocker(Some(unlocker_guid));
        o.set_use_lock_timestamp(Some(now));
    }
    result
}

// ---- constructors and SetEphemeralValues ----

/// `new Chest(weenie, guid)` / `new Chest(biota)`: the `Container` constructor, then
/// Chest's `SetEphemeralValues`.
// ACE: Chest.Chest
pub fn chest_ctor(
    o: &mut crate::world_objects::world_object::WorldObject,
    env: &crate::world_objects::world_object::CtorEnv<'_>,
    src: crate::world_objects::world_object::CtorSource,
) {
    crate::world_objects::container::container_ctor(o, env, src);
    chest_set_ephemeral_values(o, env);
}

// ACE: Chest.SetEphemeralValues
fn chest_set_ephemeral_values(
    o: &mut crate::world_objects::world_object::WorldObject,
    _env: &crate::world_objects::world_object::CtorEnv<'_>,
) {
    o.set_container_capacity(Some(o.container_capacity().unwrap_or(10)));
    o.set_item_capacity(Some(o.item_capacity().unwrap_or(120)));

    // todo: fix broken data
    o.set_activation_response(
        o.activation_response() | empyrean_entity::enums::ActivationResponse::Use,
    );

    // do any chests default to open?
    o.wo.world_object_properties.current_motion_state = Some(motion_closed());

    if o.is_locked() {
        o.set_default_locked(true);
    }

    if o.default_locked() {
        // ignore regen interval, only regen on relock
        o.wo.world_object_tick.next_generator_regeneration_time = f64::MAX;
    }
}
