// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Player_Tracking.cs
//! Port of `Source/ACE.Server/WorldObjects/Player_Tracking.cs`.
//!
//! `TrackObject` (the create-object a player is sent for each object its `ObjectMaint` newly
//! sees) and `TrackEquippedObject`; `AddTrackedObject` (reached from `WorldObject.NotifyPlayers`);
//! and the removal side (`RemoveTrackedObject`, `RemoveTrackedEquippedObject`, reached from the
//! landblock's removal broadcast and from dequipping), `GetKnownObjects`, the cloak, and the
//! pre-teleport visibility fix.
//!
//! # Objects gone before a queued removal runs
//!
//! ACE queues `p => p.RemoveTrackedObject(wo, fromPickup)` on each player; the lambda's reference
//! keeps a destroyed `wo` (and its `EquippedObjects`) readable until the action runs. Here a
//! destroyed object leaves `World.objects`, so the broadcast takes a [`TrackedObjectSnapshot`] of
//! what the removal reads (the messages it builds from `wo`) when it is queued, and the action
//! uses the live object when it is still there, else the snapshot. A `DeleteObject` only reads the
//! guid and the `ObjectInstance` sequence, which never changes after the object is created (only a
//! player's login sets it), so the bytes are the same either way.

use dereth_physics::PhysHandle;
use empyrean_common::dotnet::DotNetDict;
use empyrean_entity::enums::{CloakStatus, EquipMask, PhysicsState, PropertyBool};
use empyrean_entity::ObjectGuid;

use crate::entity::actions::action_chain::ActionChain;
use crate::entity::actions::i_actor::Actor;
use crate::entity::world_object_info::WorldObjectInfo;
use crate::network::game_messages::game_message::{self, GameMessage};
use crate::network::game_messages::messages::{
    game_message_create_object, game_message_delete_object, game_message_parent_event,
    game_message_pickup_event,
};
use crate::physics::{object_maint, phys_ext};
use crate::world_objects::world_object::WorldObject;
use crate::world_objects::world_object_networking::{self, shims};
use crate::World;

/// Non-property fields declared in `Player_Tracking.cs`.
#[derive(Debug, Default)]
pub struct PlayerTrackingFields {
    /// ObjectId of the currently selected Target (only players and creatures). Set by
    /// `Player.UpdateSelectedTarget` (`Player.cs`).
    // ACE: Player.selectedTarget
    pub selected_target: Option<WorldObjectInfo>,
    /// The last use of each shared-cooldown key this session. ACE declares it and never reads or
    /// writes it (null).
    // ACE: Player.LastUseTracker
    pub last_use_tracker:
        Option<DotNetDict<i32, empyrean_common::dotnet::datetime::DotNetDateTime>>,
}

#[must_use]
pub fn fields(w: &World, this: ObjectGuid) -> &PlayerTrackingFields {
    &w.objects
        .get(this)
        .and_then(|o| o.player.as_ref())
        .expect("ACE: this is a Player")
        .player_tracking
}

pub fn fields_mut(w: &mut World, this: ObjectGuid) -> &mut PlayerTrackingFields {
    &mut w
        .objects
        .get_mut(this)
        .and_then(|o| o.player.as_mut())
        .expect("ACE: this is a Player")
        .player_tracking
}

/// `selectedTarget?.TryGetWorldObject()`.
#[must_use]
pub fn selected_target(w: &World, this: ObjectGuid) -> Option<ObjectGuid> {
    fields(w, this)
        .selected_target
        .as_ref()
        .and_then(|t| t.try_get_world_object(w))
}

/// The `selectedTarget` setter, for `Player.UpdateSelectedTarget` (`Player.cs`).
pub fn set_selected_target(w: &mut World, this: ObjectGuid, target: Option<WorldObjectInfo>) {
    fields_mut(w, this).selected_target = target;
}

// ACE: Player.ObjMaint
/// The link to this player's Object Maintenance: `PhysicsObj.ObjMaint`, the body's.
///
/// # Panics
/// When the player has no body (ACE: `NullReferenceException`).
#[must_use]
pub fn obj_maint(w: &World, this: ObjectGuid) -> PhysHandle {
    phys_ext::physics_obj(w, this).expect("ACE: Player.PhysicsObj is null (NullReferenceException)")
}

// ACE: Player.GetKnownObjects
/// Returns the list of WorldObjects this player currently knows about: the `KnownObjects` values
/// whose `WeenieObj.WorldObject` is set (bodies of objects in the store), in `KnownObjects` order.
// DIVERGE: a player without a body knows nothing (ACE's `PhysicsObj.ObjMaint` throws; only a
// bodiless test player gets here).
#[must_use]
pub fn get_known_objects(w: &World, this: ObjectGuid) -> Vec<ObjectGuid> {
    let Some(me) = phys_ext::physics_obj(w, this) else {
        return Vec::new();
    };
    object_maint::get_known_objects_values_where(w, me, |i| world_object_of(w, i).is_some())
        .into_iter()
        .filter_map(|i| world_object_of(w, i))
        .collect()
}

/// `physicsObj.WeenieObj.WorldObject`: the store object whose body this is.
fn world_object_of(w: &World, h: PhysHandle) -> Option<ObjectGuid> {
    let guid = ObjectGuid::new(phys_ext::id(w, h)?);
    w.objects
        .get(guid)
        .filter(|o| o.phys == Some(h))
        .map(|_| guid)
}

// ACE: Player.TrackObject
/// Sends a network message to player for CreateObject, if applicable. `delay` is unused by ACE's
/// body (it defaults to false).
pub fn track_object(w: &mut World, this: ObjectGuid, world_object: ObjectGuid, delay: bool) {
    let _ = delay;
    //Console.WriteLine($"TrackObject({worldObject.Name}, {delay})");

    // `worldObject == null`: an object gone from the store.
    let Some(wo) = w.objects.get(world_object) else {
        return;
    };
    if world_object == this {
        return;
    }

    let adminvision = shims::player_adminvision(w, this);

    // If Visibility is true, do not send object to client, object is meant for server side only, unless Adminvision is true.
    if wo.visibility() && !adminvision {
        return;
    }
    let is_creature = wo.is_creature();

    let session = shims::player_session(w, this)
        .expect("ACE: Player.Session is null (NullReferenceException)");
    let msg = game_message_create_object::game_message_create_object(
        w,
        world_object,
        adminvision,
        adminvision,
    );
    game_message::enqueue_send(w, session, msg);

    //Console.WriteLine($"Player {Name} - TrackObject({worldObject.Name})");

    // add creature equipped objects / wielded items
    if is_creature {
        for wielded_item in
            crate::world_objects::creature_equipment::equipped_objects_values(w, world_object)
        {
            if crate::world_objects::creature_equipment::is_in_child_location(w, this, wielded_item)
            {
                track_equipped_object(w, this, world_object, wielded_item);
            }
        }

        if crate::world_objects::monster_navigation::fields(w, world_object).is_moving {
            crate::dispatch::broadcast_move_to::broadcast_move_to(w, world_object, this);
        }
    }
}

// ACE: Player.AddTrackedObject
/// Adds `world_object` to this player's known and visible objects and sends it (`TrackObject`);
/// false when the player already knows it.
pub fn add_tracked_object(w: &mut World, this: ObjectGuid, world_object: ObjectGuid) -> bool {
    let me = w
        .objects
        .get(this)
        .and_then(|o| o.phys)
        .expect("ACE: Player.PhysicsObj is null (NullReferenceException)");
    let it = w.objects.get(world_object).and_then(|o| o.phys);

    // does this work for equipped objects?
    if it.is_some_and(|it| object_maint::known_objects_contains_value(w, me, it)) {
        //Console.WriteLine($"Player {Name} - AddTrackedObject({worldObject.Name}) skipped, already tracked");
        return false;
    }

    let it = it.expect("ACE: worldObject.PhysicsObj is null (NullReferenceException)");
    object_maint::add_known_object(w, me, it);
    object_maint::add_visible_object(w, me, it);

    track_object(w, this, world_object, false);
    true
}

// ACE: Player.TrackEquippedObject
pub fn track_equipped_object(
    w: &mut World,
    this: ObjectGuid,
    wielder: ObjectGuid,
    wielded_item: ObjectGuid,
) {
    //Console.WriteLine($"Player {Name} - TrackEquippedObject({wieldedItem.Name}) on Wielder {wielder.Name}");

    // We make sure the item is actually wielded and selectable
    let location = w
        .objects
        .get(wielded_item)
        .and_then(|o| o.current_wielded_location())
        .unwrap_or(EquipMask::None);
    if (location & EquipMask::SelectablePlusAmmo) == EquipMask::None {
        return;
    }

    // The wielder already knows about this object
    if wielder == this {
        return;
    }

    let session = shims::player_session(w, this)
        .expect("ACE: Player.Session is null (NullReferenceException)");
    let msg = game_message_create_object::game_message_create_object(w, wielded_item, false, false);
    game_message::enqueue_send(w, session, msg);
}

// ACE: Player.RemoveTrackedObject
/// Tells the client that `wo` has left its view: a `PickupEvent` (and, for a wielded item, the
/// `ParentEvent` that puts it back on its wielder) when it was picked up, else a `DeleteObject`;
/// then the same for each item a creature wields (`RemoveTrackedEquippedObject`).
pub fn remove_tracked_object(w: &mut World, this: ObjectGuid, wo: ObjectGuid, from_pickup: bool) {
    let snapshot = TrackedObjectSnapshot::take(w, wo);
    remove_tracked_object_snapshot(w, this, wo, from_pickup, &snapshot);
}

/// What [`remove_tracked_object`] reads of `wo`, taken when a removal is queued for later (see the
/// module docs).
// DIVERGE: ACE's queued lambda holds `wo` itself; a destroyed object leaves the store here, so
// what the removal reads is taken when it is queued (the same DeleteObject bytes).
#[derive(Debug, Clone)]
pub struct TrackedObjectSnapshot {
    /// `new GameMessageDeleteObject(wo)`.
    delete_object: GameMessage,
    /// `wo is Creature`, with `creature.EquippedObjects.Values` as (item, its snapshot).
    equipped: Option<Vec<(ObjectGuid, EquippedObjectSnapshot)>>,
}

/// What [`remove_tracked_equipped_object`] reads of an item.
#[derive(Debug, Clone)]
pub struct EquippedObjectSnapshot {
    /// `worldObject.ValidLocations`.
    valid_locations: Option<EquipMask>,
    /// `new GameMessageDeleteObject(worldObject)`.
    delete_object: GameMessage,
}

impl TrackedObjectSnapshot {
    /// Reads `wo` now.
    ///
    /// # Panics
    /// When `wo` is not in the store (ACE: `NullReferenceException`).
    #[must_use]
    pub fn take(w: &mut World, wo: ObjectGuid) -> Self {
        let delete_object = delete_object_message(w, wo)
            .expect("ACE: RemoveTrackedObject(wo): wo is null (NullReferenceException)");
        let equipped = w
            .objects
            .get(wo)
            .is_some_and(WorldObject::is_creature)
            .then(|| {
                crate::world_objects::creature_equipment::equipped_objects_values(w, wo)
                    .into_iter()
                    .filter_map(|item| EquippedObjectSnapshot::take(w, item).map(|s| (item, s)))
                    .collect()
            });
        Self {
            delete_object,
            equipped,
        }
    }
}

impl EquippedObjectSnapshot {
    /// Reads `item` now; `None` when it is not in the store.
    #[must_use]
    pub fn take(w: &mut World, item: ObjectGuid) -> Option<Self> {
        let valid_locations = w.objects.get(item)?.valid_locations();
        Some(Self {
            valid_locations,
            delete_object: delete_object_message(w, item)?,
        })
    }
}

/// `new GameMessageDeleteObject(wo)`, when `wo` is in the store.
fn delete_object_message(w: &mut World, wo: ObjectGuid) -> Option<GameMessage> {
    w.objects
        .get_mut(wo)
        .map(game_message_delete_object::game_message_delete_object)
}

/// [`remove_tracked_object`], reading `wo` live when it is still in the store, else from
/// `snapshot` (see the module docs).
pub fn remove_tracked_object_snapshot(
    w: &mut World,
    this: ObjectGuid,
    wo: ObjectGuid,
    from_pickup: bool,
    snapshot: &TrackedObjectSnapshot,
) {
    //log.Info($"{Name}.RemoveTrackedObject({wo.Name} ({wo.Guid}), {fromPickup})");

    let session = shims::player_session(w, this)
        .expect("ACE: Player.Session is null (NullReferenceException)");
    if from_pickup {
        // DIVERGE: a picked-up object destroyed before this action runs sends nothing: its
        // PickupEvent advances the object's own position sequence, which left with it (ACE sends it
        // from the orphaned object).
        let Some(o) = w.objects.get_mut(wo) else {
            log::warn!(
                "{this:?}.RemoveTrackedObject({wo:?}, fromPickup: true): the object is gone"
            );
            return;
        };
        let msg = game_message_pickup_event::game_message_pickup_event(o);
        game_message::enqueue_send(w, session, msg);

        let o = w.objects.get(wo).expect("the object");
        if o.wielder_id().is_some() && o.parent_location().map_or(0, |l| l.0) != 0 {
            let wielder = o
                .wielder
                .expect("ACE: wo.Wielder is null (NullReferenceException)");
            let (wielder_obj, item) = w
                .objects
                .get2_mut(wielder, wo)
                .expect("ACE: wo.Wielder is null (NullReferenceException)");
            let msg =
                game_message_parent_event::game_message_parent_event(wielder_obj, item, None, None);
            game_message::enqueue_send(w, session, msg);
        }
    } else {
        let msg = delete_object_message(w, wo).unwrap_or_else(|| snapshot.delete_object.clone());
        game_message::enqueue_send(w, session, msg);
    }

    let is_creature = match w.objects.get(wo) {
        Some(o) => o.is_creature(),
        None => snapshot.equipped.is_some(),
    };
    if is_creature {
        let equipped: Vec<(ObjectGuid, Option<EquippedObjectSnapshot>)> = if w.objects.contains(wo)
        {
            crate::world_objects::creature_equipment::equipped_objects_values(w, wo)
                .into_iter()
                .map(|i| (i, None))
                .collect()
        } else {
            snapshot
                .equipped
                .iter()
                .flatten()
                .map(|(i, s)| (*i, Some(s.clone())))
                .collect()
        };
        for (wielded_item, item_snapshot) in equipped {
            let item_snapshot = match item_snapshot {
                Some(s) => s,
                None => EquippedObjectSnapshot::take(w, wielded_item)
                    .expect("an equipped item in the store"),
            };
            remove_tracked_equipped_object_snapshot(w, this, wo, wielded_item, &item_snapshot);
        }
    }
}

// ACE: Player.RemoveTrackedEquippedObject
/// Tells the client an item `former_wielder` wielded is gone (a `DeleteObject`), unless it could
/// never have been tracked or the player is the former wielder.
pub fn remove_tracked_equipped_object(
    w: &mut World,
    this: ObjectGuid,
    former_wielder: ObjectGuid,
    world_object: ObjectGuid,
) {
    let snapshot = EquippedObjectSnapshot::take(w, world_object)
        .expect("ACE: worldObject is null (NullReferenceException)");
    remove_tracked_equipped_object_snapshot(w, this, former_wielder, world_object, &snapshot);
}

/// [`remove_tracked_equipped_object`], reading the item live when it is still in the store, else
/// from `snapshot`.
pub fn remove_tracked_equipped_object_snapshot(
    w: &mut World,
    this: ObjectGuid,
    former_wielder: ObjectGuid,
    world_object: ObjectGuid,
    snapshot: &EquippedObjectSnapshot,
) {
    //Console.WriteLine($"Player {Name} - RemoveTrackedEquippedObject({worldObject.Name}) on Former Wielder {formerWielder.Name}");

    let valid_locations = match w.objects.get(world_object) {
        Some(o) => o.valid_locations(),
        None => snapshot.valid_locations,
    };

    // We don't need to remove objects that couldn't have been tracked in the first place
    if (valid_locations.unwrap_or(EquipMask::None) & EquipMask::SelectablePlusAmmo)
        == EquipMask::None
    {
        return;
    }

    // The former wielder already knows about this object was removed
    if former_wielder == this {
        return;
    }

    // intended for cloaked objects, as DO's should not be sent for them
    // but this breaks regular players, as the state of worldObject has already changed, and is never in a ChildLocation
    //if (!IsInChildLocation(worldObject))
    //return;

    // todo: Until we can fix the tracking system better, sending the PickupEvent like retail causes weapon dissapearing bugs on relog
    //Session.Network.EnqueueSend(new GameMessagePickupEvent(worldObject));

    let session = shims::player_session(w, this)
        .expect("ACE: Player.Session is null (NullReferenceException)");
    let msg =
        delete_object_message(w, world_object).unwrap_or_else(|| snapshot.delete_object.clone());
    game_message::enqueue_send(w, session, msg);
}

/// `wo.EnqueueActionBroadcast(p => p.RemoveTrackedObject(wo, fromPickup))` (Landblock.cs,
/// `RemoveWorldObjectInternal`): `wo` is read now (the snapshot) for players whose action runs
/// after it has left the store.
pub fn enqueue_action_broadcast_remove_tracked_object(
    w: &mut World,
    wo: ObjectGuid,
    from_pickup: bool,
) {
    // `if (PhysicsObj == null) return;` (EnqueueActionBroadcast), before anything is read.
    if w.objects.get(wo).is_none_or(|o| o.phys.is_none()) {
        return;
    }
    let snapshot = TrackedObjectSnapshot::take(w, wo);
    // Not ACE's (retail captures, V286): the delete (or pickup) goes to
    // every player in the object's 3×3 landblocks, whether or not each knows it (retail's corpse
    // decay deletes reached players who had forgotten the corpse); ACE told the known players only.
    world_object_networking::enqueue_action_broadcast_to_reach(
        w,
        wo,
        move |w: &mut World, p: ObjectGuid| {
            remove_tracked_object_snapshot(w, p, wo, from_pickup, &snapshot)
        },
        false,
    );
}

/// `EnqueueActionBroadcast(p => p.RemoveTrackedEquippedObject(this, wo))` (Creature_Equipment.cs),
/// the item read now as for [`enqueue_action_broadcast_remove_tracked_object`].
pub fn enqueue_action_broadcast_remove_tracked_equipped_object(
    w: &mut World,
    former_wielder: ObjectGuid,
    world_object: ObjectGuid,
) {
    if w.objects
        .get(former_wielder)
        .is_none_or(|o| o.phys.is_none())
    {
        return;
    }
    let snapshot = EquippedObjectSnapshot::take(w, world_object)
        .expect("ACE: worldObject is null (NullReferenceException)");
    // Not ACE's (retail captures, V286): the item's delete goes to the
    // former wielder's 3×3 landblock reach, as other deletes do.
    world_object_networking::enqueue_action_broadcast_to_reach(
        w,
        former_wielder,
        move |w: &mut World, p: ObjectGuid| {
            remove_tracked_equipped_object_snapshot(w, p, former_wielder, world_object, &snapshot)
        },
        false,
    );
}

/// `EnqueueActionBroadcast(p => p.TrackEquippedObject(this, wo))` (Creature_Equipment.cs). An item
/// gone from the store when the action runs reads as not wielded, so nothing is sent (ACE would
/// send the orphaned item's CreateObject).
// DIVERGE: an item destroyed before the queued TrackEquippedObject runs sends nothing.
pub fn enqueue_action_broadcast_track_equipped_object(
    w: &mut World,
    wielder: ObjectGuid,
    wielded_item: ObjectGuid,
) {
    world_object_networking::enqueue_action_broadcast(
        w,
        wielder,
        move |w: &mut World, p: ObjectGuid| track_equipped_object(w, p, wielder, wielded_item),
        false,
    );
}

/// `EnqueueBroadcast(false, new GameMessageDeleteObject(this))`.
fn broadcast_delete_object(w: &mut World, this: ObjectGuid) {
    let Some(msg) = delete_object_message(w, this) else {
        return;
    };
    world_object_networking::enqueue_broadcast(w, this, false, &[msg]);
}

/// `EnqueueBroadcast(false, new GameMessageCreateObject(this, adminvision, adminnodraw))`.
fn broadcast_create_object(w: &mut World, this: ObjectGuid, adminvision: bool, adminnodraw: bool) {
    if !w.objects.contains(this) {
        return;
    }
    let msg =
        game_message_create_object::game_message_create_object(w, this, adminvision, adminnodraw);
    world_object_networking::enqueue_broadcast(w, this, false, &[msg]);
}

// ACE: Player.DeCloak
/// Leaves the cloak: a DeleteObject to everyone who knows the player, a CreateObject half a second
/// later, and the physics state restored half a second after that.
pub fn de_cloak(w: &mut World, this: ObjectGuid) {
    if w.objects.get(this).map(WorldObject::cloak_status) == Some(CloakStatus::Off) {
        return;
    }

    let mut action_chain = ActionChain::new();

    action_chain.add_action(Actor::Object(this), move |w: &mut World| {
        broadcast_delete_object(w, this)
    });
    action_chain.add_action(Actor::Object(this), move |w: &mut World| {
        // `NoDraw = true`
        phys_ext::set_physics_property_state(
            w,
            this,
            PropertyBool::NoDraw,
            PhysicsState::NoDraw,
            Some(true),
        );
        crate::world_objects::world_object::enqueue_broadcast_physics_state(w, this);
        if let Some(o) = w.objects.get_mut(this) {
            o.set_visibility(false);
        }
    });
    action_chain.add_delay_seconds(w, 0.5);
    action_chain.add_action(Actor::Object(this), move |w: &mut World| {
        broadcast_create_object(w, this, false, false)
    });
    action_chain.add_delay_seconds(w, 0.5);
    action_chain.add_action(Actor::Object(this), move |w: &mut World| {
        // `Cloaked = false; Ethereal = false; NoDraw = false; ReportCollisions = true;`
        phys_ext::set_physics_state(w, this, PhysicsState::Cloaked, Some(false));
        phys_ext::set_physics_property_state(
            w,
            this,
            PropertyBool::Ethereal,
            PhysicsState::Ethereal,
            Some(false),
        );
        phys_ext::set_physics_property_state(
            w,
            this,
            PropertyBool::NoDraw,
            PhysicsState::NoDraw,
            Some(false),
        );
        phys_ext::set_physics_property_state(
            w,
            this,
            PropertyBool::ReportCollisions,
            PhysicsState::ReportCollisions,
            Some(true),
        );
        crate::world_objects::world_object::enqueue_broadcast_physics_state(w, this);
    });

    action_chain.enqueue_chain(w);
}

// ACE: Player.HandleCloak
/// Enters the cloak: cloaked, ethereal and undrawn, a DeleteObject to everyone who knows the
/// player, then (a second later) a CreateObject with admin vision and no-draw.
pub fn handle_cloak(w: &mut World, this: ObjectGuid) {
    if w.objects.get(this).map(WorldObject::cloak_status) == Some(CloakStatus::On) {
        return;
    }

    let mut action_chain = ActionChain::new();

    action_chain.add_action(Actor::Object(this), move |w: &mut World| {
        // `Cloaked = true; Ethereal = true; NoDraw = true; ReportCollisions = false;`
        phys_ext::set_physics_state(w, this, PhysicsState::Cloaked, Some(true));
        phys_ext::set_physics_property_state(
            w,
            this,
            PropertyBool::Ethereal,
            PhysicsState::Ethereal,
            Some(true),
        );
        phys_ext::set_physics_property_state(
            w,
            this,
            PropertyBool::NoDraw,
            PhysicsState::NoDraw,
            Some(true),
        );
        phys_ext::set_physics_property_state(
            w,
            this,
            PropertyBool::ReportCollisions,
            PhysicsState::ReportCollisions,
            Some(false),
        );
        crate::world_objects::world_object::enqueue_broadcast_physics_state(w, this);
    });
    action_chain.add_action(Actor::Object(this), move |w: &mut World| {
        broadcast_delete_object(w, this)
    });
    action_chain.add_delay_seconds(w, 0.5);
    action_chain.add_action(Actor::Object(this), move |w: &mut World| {
        if let Some(o) = w.objects.get_mut(this) {
            o.set_visibility(true);
        }
    });
    action_chain.add_delay_seconds(w, 0.5);
    action_chain.add_action(Actor::Object(this), move |w: &mut World| {
        broadcast_create_object(w, this, true, true)
    });

    action_chain.enqueue_chain(w);
}

// ACE: Player.HandlePreTeleportVisibility
/// With `teleport_visibility_fix` on (1 players, 2 creatures, 3 every object), a teleport to
/// another cell first makes this player and each known object forget each other (a DeleteObject
/// both ways), so the destination sends fresh CreateObjects. ACE: "a DO and then a CO is the only
/// thing that fixes this issue ... this part probably deviates from retail a bit, but is the
/// equivalent automated fix".
pub fn handle_pre_teleport_visibility(
    w: &mut World,
    this: ObjectGuid,
    new_position: &empyrean_entity::Position,
) {
    let fix_level =
        crate::managers::property_manager::get_long(w, "teleport_visibility_fix", 0, true).item;

    // disabled by default
    if fix_level < 1 {
        return;
    }

    let location = w
        .objects
        .get(this)
        .and_then(WorldObject::location)
        .expect("ACE: Location is null (NullReferenceException)");
    if location.cell() == new_position.cell() {
        return;
    }

    let mut known_objs = get_known_objects(w, this);

    if fix_level == 1 {
        // filter to players only
        known_objs.retain(|&i| w.objects.get(i).is_some_and(WorldObject::is_player));
    } else if fix_level == 2 {
        // filter to creatures only
        known_objs.retain(|&i| w.objects.get(i).is_some_and(WorldObject::is_creature));
    }

    let me = obj_maint(w, this);
    for known_obj in known_objs {
        let known_phys = phys_ext::physics_obj(w, known_obj).expect("a known object's body");
        object_maint::remove_object(w, known_phys, me, true);

        if w.objects.get(known_obj).is_some_and(WorldObject::is_player) {
            remove_tracked_object(w, known_obj, this, false);
        }

        object_maint::remove_object(w, me, known_phys, true);
        remove_tracked_object(w, this, known_obj, false);
    }
}
