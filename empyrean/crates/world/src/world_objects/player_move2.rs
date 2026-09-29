// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Player_Move2.cs, Source/ACE.Server/Entity/MoveToParams.cs
//! Port of `Source/ACE.Server/WorldObjects/Player_Move2.cs`.
//!
//! The full-physics (`FastTick`) move-to and turn-to chains: the server runs the MoveTo in the
//! player's own physics body (3.6's `MoveToManager`) and the callback fires from
//! `OnMoveComplete`, or from `Player_Tick`'s `HandleMoveToCallback` once the body is at rest.
//! Also hosts `Entity/MoveToParams.cs` (its three fields and constructor), which nothing else
//! uses.

use std::fmt;

use dereth_animation::motion::{flags, MovementParameters};
use empyrean_entity::enums::{WeenieError, WeenieType};
use empyrean_entity::ObjectGuid;

use crate::network::motion::move_to_parameters::RetailMoveTo;
use crate::physics::phys_ext;
use crate::world_objects::player_move::{self, MoveToCallback};
use crate::World;

// ACE: MoveToParams
/// The pending callback of a MoveTo2 or TurnTo2 chain, its target and use radius.
pub struct MoveToParams {
    // ACE: MoveToParams.Callback
    pub callback: Option<MoveToCallback>,
    // ACE: MoveToParams.Target
    pub target: ObjectGuid,
    // ACE: MoveToParams.UseRadius
    pub use_radius: Option<f32>,
}

impl MoveToParams {
    // ACE: MoveToParams.MoveToParams
    #[must_use]
    pub fn new(callback: MoveToCallback, target: ObjectGuid, use_radius: Option<f32>) -> Self {
        MoveToParams {
            callback: Some(callback),
            target,
            use_radius,
        }
    }
}

impl fmt::Debug for MoveToParams {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MoveToParams")
            .field("callback", &self.callback.is_some())
            .field("target", &self.target)
            .field("use_radius", &self.use_radius)
            .finish()
    }
}

/// Non-property fields declared in `Player_Move2.cs`.
#[derive(Debug, Default)]
pub struct PlayerMove2Fields {
    // ACE: Player.IsPlayerMovingTo2
    pub is_player_moving_to2: bool,
    // ACE: Player.MoveToParams
    pub move_to_params: Option<MoveToParams>,
}

/// The player's `Player_Move2` fields.
///
/// # Panics
/// When `this` is not a live player.
#[must_use]
pub fn fields(w: &World, this: ObjectGuid) -> &PlayerMove2Fields {
    &w.objects
        .get(this)
        .and_then(|o| o.player.as_ref())
        .expect("ACE: this is a Player")
        .player_move2
}

/// The player's `Player_Move2` fields, mutably.
///
/// # Panics
/// When `this` is not a live player.
pub fn fields_mut(w: &mut World, this: ObjectGuid) -> &mut PlayerMove2Fields {
    &mut w
        .objects
        .get_mut(this)
        .and_then(|o| o.player.as_mut())
        .expect("ACE: this is a Player")
        .player_move2
}

/// `MoveToParams?.Callback != null`.
#[must_use]
pub fn move_to_params_has_callback(w: &World, this: ObjectGuid) -> bool {
    fields(w, this)
        .move_to_params
        .as_ref()
        .is_some_and(|p| p.callback.is_some())
}

// ACE: Player.CreateMoveToChain2
/// Moves to `target` in the player's own body, then calls `callback`. An earlier chain is
/// stopped, and a pending callback fails first. The MoveTo's flag word is `kind`'s, or a
/// portal's (V257).
pub fn create_move_to_chain2(
    w: &mut World,
    this: ObjectGuid,
    target: ObjectGuid,
    callback: MoveToCallback,
    use_radius: Option<f32>,
    rotate: bool,
    kind: RetailMoveTo,
) {
    if fields(w, this).is_player_moving_to2 {
        stop_existing_move_to_chains2(w, this);
    }

    if fields(w, this).move_to_params.is_some() {
        check_move_to_params(w, this);
    }

    let target_obj = w
        .objects
        .get(target)
        .expect("ACE: target is null (NullReferenceException)");
    if target_obj.location().is_none() {
        log::error!("{this:?}.MoveTo({target:?}): target.Location is null");
        callback(w, false);
        return;
    }

    let use_radius = Some(use_radius.unwrap_or_else(|| target_obj.use_radius().unwrap_or(0.6)));

    let current_landblock = w
        .objects
        .get(this)
        .and_then(|o| o.current_landblock)
        .expect("ACE: CurrentLandblock is null");
    let (within_use_radius, _target_valid) =
        crate::entity::landblock::within_use_radius(w, current_landblock, this, target, use_radius);

    if within_use_radius {
        if rotate {
            create_turn_to_chain2(w, this, target, callback, use_radius, false, false);
        } else {
            callback(w, true);
        }

        return;
    }

    // Not ACE's (retail captures, V257): a portal's use-move carried 0x1EA4F.
    let kind = if w
        .objects
        .get(target)
        .is_some_and(|t| t.biota.weenie_type == WeenieType::Portal)
    {
        RetailMoveTo::Portal
    } else {
        kind
    };

    // send command to client
    player_move::creature_move_to_object(w, this, target, None, kind);

    // start on server
    // forward this to PhysicsObj.MoveManager.MoveToManager
    let mvp = get_move_to_params(w, this, target, use_radius, kind);

    let h = physics_obj(w, this);
    phys_ext::restart_clock_if_idle(w, h);

    fields_mut(w, this).is_player_moving_to2 = true;

    fields_mut(w, this).move_to_params = Some(MoveToParams::new(callback, target, use_radius));

    if let Some(target_h) = phys_ext::physics_obj(w, target) {
        phys_ext::move_to_object(w, h, target_h, &mvp);
    }
    //PhysicsObj.LastMoveWasAutonomous = false;

    phys_ext::update_object(w, h);
}

// ACE: Player.CreateTurnToChain2
/// Turns to `target` (or its wielder) in the player's own body, then calls `callback`.
pub fn create_turn_to_chain2(
    w: &mut World,
    this: ObjectGuid,
    target: ObjectGuid,
    callback: MoveToCallback,
    use_radius: Option<f32>,
    stop_completely: bool,
    always_turn: bool,
) {
    if fields(w, this).is_player_moving_to2 {
        stop_existing_move_to_chains2(w, this);
    }

    if fields(w, this).move_to_params.is_some() {
        check_move_to_params(w, this);
    }

    let target_obj = w
        .objects
        .get(target)
        .expect("ACE: target is null (NullReferenceException)");
    let rotate_target = target_obj.wielder.unwrap_or(target);

    if w.objects
        .get(rotate_target)
        .and_then(crate::world_objects::world_object::WorldObject::location)
        .is_none()
    {
        log::error!("{this:?}.TurnTo({target:?}): target.Location is null");
        callback(w, false);
        return;
    }

    // send command to client
    creature_turn_to_object(w, this, rotate_target, stop_completely);

    // start on server
    // forward this to PhysicsObj.MoveManager.MoveToManager
    let mvp = get_turn_to_params(stop_completely);

    let h = physics_obj(w, this);
    phys_ext::restart_clock_if_idle(w, h);

    fields_mut(w, this).is_player_moving_to2 = true;

    fields_mut(w, this).move_to_params = Some(MoveToParams::new(callback, target, use_radius));

    phys_ext::set_move_to_always_turn(w, h, always_turn);

    let rotate_target_h = phys_ext::physics_obj(w, rotate_target);
    phys_ext::turn_to_object(w, h, rotate_target_h, &mvp);
    //PhysicsObj.LastMoveWasAutonomous = false;

    phys_ext::update_object(w, h);

    phys_ext::set_move_to_always_turn(w, h, false);
}

// ACE: Player.StopExistingMoveToChains2
pub fn stop_existing_move_to_chains2(w: &mut World, this: ObjectGuid) {
    //Console.WriteLine($"CancelMoveTo");
    let h = physics_obj(w, this);
    phys_ext::cancel_moveto(w, h);
}

// ACE: Player.GetMoveToParams
/// The body's parameters for the move the clients are sent: `kind`'s flag word and threshold 15
/// (V257), with the use radius as the distance.
#[must_use]
pub fn get_move_to_params(
    w: &World,
    _this: ObjectGuid,
    target: ObjectGuid,
    use_radius: Option<f32>,
    kind: RetailMoveTo,
) -> MovementParameters {
    // Not ACE's (retail captures, V257): ACE starts from its own defaults (CanCharge, threshold
    // 1.0) and clears CanRun at half the threshold or farther, so the body always charges; the
    // body now moves as the clients are told to, with the flag word of the kind of move.
    let mut mvp = kind.movement_parameters();
    let target_obj = w
        .objects
        .get(target)
        .expect("ACE: target is null (NullReferenceException)");
    mvp.distance_to_object = use_radius
        .or_else(|| target_obj.use_radius())
        .unwrap_or(0.6);

    // move directly to portal origin
    //if (target is Portal)
    //mvp.UseSpheres = false;

    mvp
}

// ACE: Player.GetTurnToParams
#[must_use]
pub fn get_turn_to_params(stop_completely: bool) -> MovementParameters {
    let mut mvp = phys_ext::ace_movement_parameters();

    //mvp.HoldKeyToApply = HoldKey.Run;
    if stop_completely {
        mvp.flags |= flags::STOP_COMPLETELY;
    } else {
        mvp.flags &= !flags::STOP_COMPLETELY;
    }
    //mvp.ModifyInterpretedState = false;

    mvp
}

// ACE: Player.OnMoveComplete_MoveTo2
/// The chain ended: success calls back at once; a cancel is checked again once the player is at
/// rest (`Player_Tick` → `HandleMoveToCallback`).
pub fn on_move_complete_move_to2(w: &mut World, this: ObjectGuid, status: WeenieError) {
    if crate::world_objects::player_tick::DEBUG_PLAYER_MOVE_TO_STATE_PHYSICS {
        log::debug!("{this:?}.OnMoveComplete_MoveTo({status:?})");
    }

    fields_mut(w, this).is_player_moving_to2 = false;

    let params = fields_mut(w, this)
        .move_to_params
        .as_mut()
        .expect("ACE: MoveToParams is null (NullReferenceException)");
    if params.callback.is_none() {
        // nothing to do -- we are done here
        fields_mut(w, this).move_to_params = None;
        return;
    }

    let success = status == WeenieError::None;

    if success {
        let callback = params.callback.take().expect("checked above");
        callback(w, true);
        fields_mut(w, this).move_to_params = None;
    }

    // if action cancelled, check again when player is stationary
    // through Player_Tick -> HandleMoveToCallback
}

// ACE: Player.CheckMoveToParams
/// Fails a pending action (queued actions must still run, to keep the client out of its busy
/// state), then clears it.
pub fn check_move_to_params(w: &mut World, this: ObjectGuid) {
    // because of the additional gap, it is now possible to queue up actions
    // we don't want to queue up multiple actions, but we still need to process the queue,
    // to prevent busy state on client

    // fail pending action
    let callback = fields_mut(w, this)
        .move_to_params
        .as_mut()
        .expect("ACE: MoveToParams is null (NullReferenceException)")
        .callback
        .take();
    if let Some(callback) = callback {
        callback(w, false);
    }

    fields_mut(w, this).move_to_params = None;
}

// ACE: Player.HandleMoveToCallback
/// At rest after a MoveTo2: succeeds when facing the target and within its use radius.
pub fn handle_move_to_callback(w: &mut World, this: ObjectGuid) {
    let (target, use_radius) = {
        let p = fields(w, this)
            .move_to_params
            .as_ref()
            .expect("ACE: MoveToParams is null (NullReferenceException)");
        (p.target, p.use_radius)
    };

    let is_facing = creature_is_facing(w, this, target);

    let within_use_radius = use_radius.is_none() || {
        let current_landblock = w
            .objects
            .get(this)
            .and_then(|o| o.current_landblock)
            .expect("ACE: CurrentLandblock is null");
        crate::entity::landblock::within_use_radius(w, current_landblock, this, target, use_radius)
            .0
    };

    let success = is_facing && within_use_radius;

    let callback = fields_mut(w, this)
        .move_to_params
        .as_mut()
        .and_then(|p| p.callback.take())
        .expect("ACE: MoveToParams.Callback is null (NullReferenceException)");
    callback(w, success);

    fields_mut(w, this).move_to_params = None;
}

fn physics_obj(w: &World, this: ObjectGuid) -> dereth_physics::PhysHandle {
    phys_ext::physics_obj(w, this).expect("ACE: PhysicsObj is null (NullReferenceException)")
}

// ---- pointers to members ported in other files --------------------------------------------------

/// `Creature.TurnToObject(target, stopCompletely)` (`Creature_Navigation.cs`): the client-side
/// TurnToObject.
fn creature_turn_to_object(
    w: &mut World,
    this: ObjectGuid,
    target: ObjectGuid,
    stop_completely: bool,
) {
    crate::world_objects::creature_navigation::turn_to_object(w, this, target, stop_completely);
}

/// `Monster.IsFacing(target)` (`Monster_Navigation.cs`).
fn creature_is_facing(w: &World, this: ObjectGuid, target: ObjectGuid) -> bool {
    crate::world_objects::monster_navigation::is_facing(w, this, Some(target))
}
