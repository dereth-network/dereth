// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Door.cs
//! Port of `Source/ACE.Server/WorldObjects/Door.cs`.
//!
//! A door opens and closes with ACE's `On`/`Off` motions, is ethereal while open, closes (or
//! reopens) on its `ResetInterval` timer, relocks when it resets, and unlocks through
//! `LockHelper` (`lock.rs`).
//!
//! # `CurrentMotionState == motionOpen`
//!
//! ACE compares `CurrentMotionState` with Door's two static `Motion`s by reference. The port's
//! `CurrentMotionState` is a value, so [`DoorFields::current_motion`] records which static the
//! door last assigned, and the comparison also requires the current value to still be that
//! motion (anything else, such as an emote's `Motion`, replaced it). A `DIVERGE` marker on
//! [`current_motion_is`] records this.

use empyrean_entity::enums::{
    ChatMessageType, MotionCommand, MotionStance, PropertyBool, Quadrant, Sound,
};
use empyrean_entity::ObjectGuid;

use crate::entity::actions::action_chain::ActionChain;
use crate::entity::actions::i_actor::Actor;
use crate::managers::player_manager::player_session;
use crate::managers::property_manager;
use crate::network::game_event::events::game_event_communication_transient_string::game_event_communication_transient_string;
use crate::network::game_messages::game_message::enqueue_send;
use crate::network::game_messages::messages::game_message_public_update_property_bool::game_message_public_update_property_bool;
use crate::network::game_messages::messages::game_message_sound::game_message_sound;
use crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use crate::network::motion::movement_data::Motion;
use crate::physics::motion_table;
use crate::world_objects::kinds::KindData;
use crate::world_objects::lock::{self, UnlockResults};
use crate::world_objects::world_object::WorldObject;
use crate::world_objects::{world_object, world_object_networking};
use crate::{dispatch, World};

/// Which of Door's static motions `CurrentMotionState` was last set to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoorMotion {
    /// `motionOpen`: `new Motion(MotionStance.NonCombat, MotionCommand.On)`.
    Open,
    /// `motionClosed`: `new Motion(MotionStance.NonCombat, MotionCommand.Off)`.
    Closed,
}

impl DoorMotion {
    fn command(self) -> MotionCommand {
        match self {
            DoorMotion::Open => MotionCommand::On,
            DoorMotion::Closed => MotionCommand::Off,
        }
    }

    /// The static `Motion` itself.
    // ACE: Door.motionOpen, Door.motionClosed
    #[must_use]
    pub fn motion(self) -> Motion {
        Motion::new(MotionStance::NonCombat, self.command(), 1.0)
    }
}

/// Non-property fields declared in `Door.cs`.
#[derive(Debug, Default)]
pub struct DoorFields {
    // ACE: Door.CloseTimestamp
    pub close_timestamp: f64,
    /// Which static motion `CurrentMotionState` references (see the module docs).
    pub current_motion: Option<DoorMotion>,
}

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

fn door_fields(o: &WorldObject) -> &DoorFields {
    match &o.kind {
        KindData::Door(d) => &d.door,
        _ => panic!(
            "System.InvalidCastException: 0x{:08X} is not a Door",
            o.guid.full()
        ),
    }
}

fn door_fields_mut(o: &mut WorldObject) -> &mut DoorFields {
    let guid = o.guid;
    match &mut o.kind {
        KindData::Door(d) => &mut d.door,
        _ => panic!(
            "System.InvalidCastException: 0x{:08X} is not a Door",
            guid.full()
        ),
    }
}

/// This door's fields.
///
/// # Panics
/// When `this` is gone or is not a Door.
#[must_use]
pub fn fields(w: &World, this: ObjectGuid) -> &DoorFields {
    door_fields(obj(w, this))
}

/// `CurrentMotionState = motionOpen / motionClosed`.
fn set_current_motion(o: &mut WorldObject, which: DoorMotion) {
    o.wo.world_object_properties.current_motion_state = Some(which.motion());
    door_fields_mut(o).current_motion = Some(which);
}

/// `CurrentMotionState == motionOpen / motionClosed` (reference equality in ACE).
// DIVERGE: ACE compares CurrentMotionState with Door's static Motion by reference; the port records which static the door assigned and also requires the current value to still be that motion.
fn current_motion_is(w: &World, this: ObjectGuid, which: DoorMotion) -> bool {
    let o = obj(w, this);
    door_fields(o).current_motion == Some(which)
        && o.wo
            .world_object_properties
            .current_motion_state
            .as_ref()
            .is_some_and(|m| {
                m.stance == MotionStance::NonCombat
                    && m.movement_type == empyrean_entity::enums::MovementType::Invalid
                    && m.motion_state.forward_command == which.command()
            })
}

// ================================================================================ Door.cs

/// Sends the `ActivationTalk` to a player in front of an open door.
// ACE: Door.OnTalk
pub fn door_on_talk(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    activator: empyrean_entity::ObjectGuid,
) {
    if w.objects.get(activator).is_some_and(WorldObject::is_player) {
        let player = activator;
        let behind = world_object::get_relative_dir(w, player, this).contains(Quadrant::Back);

        if obj(w, this).is_open() && !behind {
            // not sure if retail made this distinction, but for the doors tested, it seemed more logical given the text shown
            let activation_talk = obj(w, this)
                .activation_talk()
                .expect("System.NullReferenceException: ActivationTalk");
            let m = game_message_system_chat(&activation_talk, ChatMessageType::Broadcast);
            let session =
                player_session(w, player).expect("System.NullReferenceException: Player.Session");
            enqueue_send(w, session, m);
        }
    }
}

/// Opens or closes the door (a locked door opens only from behind), then starts the auto-close
/// timer; a locked door tells a player so, with the locked sound.
// ACE: Door.ActOnUse
pub fn door_act_on_use(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    activator: empyrean_entity::ObjectGuid,
) {
    if obj(w, this).wo.world_object.is_busy {
        return;
    }

    let world_object = activator;
    let player = w
        .objects
        .get(activator)
        .is_some_and(WorldObject::is_player)
        .then_some(activator);
    let behind = player.is_some_and(|player| {
        world_object::get_relative_dir(w, player, this).contains(Quadrant::Back)
    });

    if !obj(w, this).is_locked() || behind {
        if !obj(w, this).is_open() {
            open(w, this, world_object);
        } else {
            let wo = obj(w, world_object);
            if !wo.is_switch() && !wo.is_pressure_plate() {
                close(w, this, world_object);
            }
        }

        // Create Door auto close timer
        let use_timestamp = obj(w, this).use_timestamp().unwrap_or(0.0);

        let mut auto_close_timer = ActionChain::new();
        auto_close_timer.add_delay_seconds(w, obj(w, this).reset_interval().unwrap_or(0.0));
        auto_close_timer.add_action(Actor::Object(this), move |w: &mut World| {
            reset(w, this, use_timestamp)
        });
        auto_close_timer.enqueue_chain(w);
    } else if let Some(player) = player {
        let session =
            player_session(w, player).expect("System.NullReferenceException: Player.Session");
        let door_is_locked = game_event_communication_transient_string(
            w.sessions
                .get_mut(session)
                .expect("the session's game half"),
            "The door is locked!",
        );
        enqueue_send(w, session, door_is_locked);
        let sound = game_message_sound(this, Sound::OpenFailDueToLock, 1.0);
        world_object_networking::enqueue_broadcast(w, this, true, &[sound]);
    }
}

/// Plays the open motion, makes the door ethereal and busy for the animation. `opener` is
/// `ObjectGuid.Invalid` (ACE's default `new ObjectGuid()`) for a reset.
// ACE: Door.Open
pub fn open(w: &mut World, this: ObjectGuid, opener: ObjectGuid) {
    if current_motion_is(w, this, DoorMotion::Open) {
        return;
    }

    world_object_networking::enqueue_broadcast_motion(
        w,
        this,
        &DoorMotion::Open.motion(),
        None,
        None,
    );
    set_current_motion(obj_mut(w, this), DoorMotion::Open);

    crate::world_objects::world_object_properties::set_ethereal(w, this, Some(true));
    obj_mut(w, this).set_is_open(true);

    world_object::enqueue_broadcast_physics_state(w, this);

    if opener.full() > 0 {
        let now = w.now.unix_time;
        obj_mut(w, this).set_use_timestamp(Some(now));
    }

    obj_mut(w, this).wo.world_object.is_busy = true;

    let motion_table_id = obj(w, this).motion_table_id();
    let anim_time = motion_table::get_animation_length_between(
        w,
        motion_table_id,
        MotionStance::NonCombat,
        MotionCommand::Off,
        MotionCommand::On,
        1.0,
    );

    let mut action_chain = ActionChain::new();
    action_chain.add_delay_seconds(w, f64::from(anim_time));
    action_chain.add_action(Actor::Object(this), move |w: &mut World| {
        obj_mut(w, this).wo.world_object.is_busy = false
    });
    action_chain.enqueue_chain(w);
}

/// Plays the close motion; the door turns solid when the animation ends (`FinalizeClose`).
/// `closer` is `ObjectGuid.Invalid` for a reset.
// ACE: Door.Close
pub fn close(w: &mut World, this: ObjectGuid, closer: ObjectGuid) {
    if current_motion_is(w, this, DoorMotion::Closed) {
        return;
    }

    world_object_networking::enqueue_broadcast_motion(
        w,
        this,
        &DoorMotion::Closed.motion(),
        None,
        None,
    );
    set_current_motion(obj_mut(w, this), DoorMotion::Closed);

    obj_mut(w, this).set_is_open(false);

    let motion_table_id = obj(w, this).motion_table_id();
    let anim_time = motion_table::get_animation_length_between(
        w,
        motion_table_id,
        MotionStance::NonCombat,
        MotionCommand::On,
        MotionCommand::Off,
        1.0,
    );

    //Console.WriteLine($"AnimTime: {animTime}");

    obj_mut(w, this).wo.world_object.is_busy = true;

    let now = w.now.unix_time;
    door_fields_mut(obj_mut(w, this)).close_timestamp = now;

    let close_timestamp = now;

    let mut action_chain = ActionChain::new();
    action_chain.add_delay_seconds(w, f64::from(anim_time));
    action_chain.add_action(Actor::Object(this), move |w: &mut World| {
        finalize_close(w, this, close_timestamp);
        obj_mut(w, this).wo.world_object.is_busy = false;
    });
    action_chain.enqueue_chain(w);

    if closer.full() > 0 {
        let now = w.now.unix_time;
        obj_mut(w, this).set_use_timestamp(Some(now));
    }
}

/// Turns the door solid, unless it reopened or closed again since; with `allow_door_hold`,
/// something standing in the doorway keeps it ethereal and it polls every second.
// ACE: Door.FinalizeClose
#[allow(clippy::float_cmp)] // C#'s != on the recorded timestamp
fn finalize_close(w: &mut World, this: ObjectGuid, close_timestamp: f64) {
    let o = obj(w, this);
    if o.is_open() || close_timestamp != door_fields(o).close_timestamp {
        return;
    }

    // ethereal must be set to false for ethereal_check_for_collisions
    crate::world_objects::world_object_properties::set_ethereal(w, this, Some(false));

    if property_manager::get_bool(w, "allow_door_hold", false, true).item
        && physics_obj_ethereal_check_for_collisions(w, this)
    {
        // the source of this bug is EtherealHook for the door
        // physics engine set_ethereal() -> ethereal_check_for_collisions() -> CheckEthereal state

        // if fix_door_holding == true, the player can still hold doors for other nearby players
        // who already know about the door / have not been far away from the door for > 25s

        // fix_door_holding == true only fixes 'long holding'
        //Console.WriteLine($"{Name} ({Guid}).FinalizeClose()");
        crate::world_objects::world_object_properties::set_ethereal(w, this, Some(true));

        let mut hold_chain = ActionChain::new();
        hold_chain.add_delay_seconds(w, 1.0); // poll every second
        hold_chain.add_action(Actor::Object(this), move |w: &mut World| {
            finalize_close(w, this, close_timestamp)
        });
        hold_chain.enqueue_chain(w);
        return;
    }

    world_object::enqueue_broadcast_physics_state(w, this);
}

/// The auto-close timer: unless the door was used again since, returns it to its default state
/// (closing and relocking, or opening).
// ACE: Door.Reset
fn reset(w: &mut World, this: ObjectGuid, use_timestamp: f64) {
    // `useTimestamp != UseTimestamp` (lifted: a null UseTimestamp is never equal)
    if Some(use_timestamp) != obj(w, this).use_timestamp() {
        return;
    }

    if !obj(w, this).default_open() {
        close(w, this, ObjectGuid::INVALID);
        if obj(w, this).default_locked() {
            let o = obj_mut(w, this);
            o.set_is_locked(true);
            let is_locked = o.is_locked();
            let update_property =
                game_message_public_update_property_bool(o, PropertyBool::Locked, is_locked);
            world_object_networking::enqueue_broadcast(w, this, true, &[update_property]);
        }
    } else {
        open(w, this, ObjectGuid::INVALID);
    }

    let now = w.now.unix_time;
    obj_mut(w, this).set_reset_timestamp(Some(now));
}

/// Used for unlocking a door via lockpick, so contains a skill check.
/// `player.Skills[Skill.Lockpick].Current` should be sent for the skill check. Returns the
/// result and the lock's difficulty (`ref int difficulty`).
// ACE: Door.Unlock
pub fn unlock_lockpick(
    w: &mut World,
    this: ObjectGuid,
    unlocker_guid: u32,
    player_lockpick_skill_lvl: u32,
    difficulty: &mut i32,
) -> UnlockResults {
    let _ = unlocker_guid;
    lock::unlock_lockpick(w, this, player_lockpick_skill_lvl, difficulty)
}

/// Used for unlocking a door via a key.
// ACE: Door.Unlock
pub fn unlock_key(
    w: &mut World,
    this: ObjectGuid,
    unlocker_guid: u32,
    key: Option<ObjectGuid>,
    key_code: Option<&str>,
) -> UnlockResults {
    let _ = unlocker_guid;
    lock::unlock_key(w, this, key, key_code)
}

/// The linked object activates this door.
// ACE: Door.SetLinkProperties
pub fn door_set_link_properties(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    wo: empyrean_entity::ObjectGuid,
) {
    obj_mut(w, wo).set_activation_target(this.full());
}

/// A creature that can open doors (`AiOptions` other than 0) opens a closed door it bumps.
// ACE: Door.OnCollideObject
pub fn door_on_collide_object(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    target: empyrean_entity::ObjectGuid,
) {
    if obj(w, this).is_open() {
        return;
    }

    // currently the only AI options appear to be 0 or 1,
    // 1 meaning able to open doors?
    let Some(creature) = w.objects.get(target).filter(|o| o.is_creature()) else {
        return;
    };
    if creature.ai_options() == 0 {
        return;
    }

    dispatch::act_on_use::act_on_use(w, this, target);
}

// ================================================================================ pointers

/// `PhysicsObj.ethereal_check_for_collisions()`: the shared physics world's check; false when
/// the door has no body (ACE: `NullReferenceException`, unreachable for a door on a landblock).
fn physics_obj_ethereal_check_for_collisions(w: &mut World, this: ObjectGuid) -> bool {
    let Some(h) = obj(w, this).phys else {
        return false;
    };
    w.physics.ethereal_check_for_collisions(h)
}

// ---- constructors and SetEphemeralValues ----

/// `new Door(weenie, guid)` / `new Door(biota)`: the `WorldObject` constructor, then
/// Door's `SetEphemeralValues`.
// ACE: Door.Door
pub fn door_ctor(
    o: &mut crate::world_objects::world_object::WorldObject,
    env: &crate::world_objects::world_object::CtorEnv<'_>,
    src: crate::world_objects::world_object::CtorSource,
) {
    crate::world_objects::world_object::world_object_ctor(o, env, src);
    door_set_ephemeral_values(o, env);
}

// ACE: Door.SetEphemeralValues
fn door_set_ephemeral_values(
    o: &mut crate::world_objects::world_object::WorldObject,
    _env: &crate::world_objects::world_object::CtorEnv<'_>,
) {
    o.wo.world_object.object_description_flags |=
        empyrean_entity::enums::ObjectDescriptionFlag::Door;

    if !o.default_open() {
        set_current_motion(o, DoorMotion::Closed);
        o.set_is_open(false);
        //Ethereal = false;
    } else {
        set_current_motion(o, DoorMotion::Open);
        o.set_is_open(true);
        o.set_ethereal(Some(true));
    }

    // `ResetInterval ?? 30.0f`: the float widens exactly.
    o.set_reset_interval(Some(o.reset_interval().unwrap_or(f64::from(30.0f32))));
    o.set_lock_code(Some(o.lock_code().unwrap_or_default()));

    // Account for possible missing property from recreated weenies
    if o.is_locked() && !o.default_locked() {
        o.set_default_locked(true);
    }

    if o.default_locked() {
        o.set_is_locked(true);
    } else {
        o.set_is_locked(false);
    }

    o.set_activation_response(
        o.activation_response() | empyrean_entity::enums::ActivationResponse::Use,
    );
}
