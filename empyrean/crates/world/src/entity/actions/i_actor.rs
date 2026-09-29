// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/Actions/IActor.cs
//! Port of `Source/ACE.Server/Entity/Actions/IActor.cs`.
//!
//! ACE's `IActor` is anything with an `EnqueueAction(IAction)`: the world queue, the delay manager,
//! a landblock or a world object. Here an actor is a plain value ([`Actor`]) naming where an action
//! goes, and [`enqueue`] is the one routing function. Units that give
//! landblocks and world objects their queues plug in here, in [`enqueue`] and
//! [`action_queue_mut`], and nowhere else.
//!
//! # Landblock and object actors
//!
//! - `Landblock(id)` is the loaded landblock with that id: its `actionQueue`, run by
//!   `Landblock.TickMultiThreadedWork`. An action for a landblock that is no longer loaded is
//!   dropped: ACE's reference would point at the unloaded landblock, whose cleared queue never
//!   runs again.
//! - `Object(guid)` follows `WorldObject.EnqueueAction`, overridden by `Player.EnqueueAction` (the
//!   player's own queue). An object that is gone from the store is destroyed, so the action is
//!   dropped (ACE's `IsDestroyed` branch).

use empyrean_entity::enums::PropertyString;
use empyrean_entity::{LandblockId, ObjectGuid};

use crate::entity::actions::action_queue::ActionQueue;
use crate::entity::actions::delay_manager;
use crate::entity::actions::i_action::Action;
use crate::managers::world_manager;
use crate::world_objects::world_object::WorldObject;
use crate::World;

// ACE: IActor
/// Who runs an action: ACE's `IActor` instances, named by value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Actor {
    /// `WorldManager.ActionQueue`.
    World,
    /// `WorldManager.DelayManager`.
    Delay,
    /// `NetworkManager.InboundMessageQueue`: the queue inbound client messages are handled from,
    /// drained by `WorldManager.UpdateWorld` before the world queue.
    InboundMessageQueue,
    /// A `Landblock` (its own `actionQueue`).
    Landblock(LandblockId),
    /// A `WorldObject`: `WorldObject.EnqueueAction`, overridden by `Player.EnqueueAction`.
    Object(ObjectGuid),
}

/// `actor.EnqueueAction(action)`: routes one action to its actor.
pub fn enqueue(w: &mut World, actor: Actor, action: Action) {
    match actor {
        Actor::World => world_manager::enqueue_action(w, action),
        Actor::Delay => delay_manager::enqueue_action(w, action),
        Actor::InboundMessageQueue => w
            .sessions
            .inbound
            .inbound_message_queue
            .enqueue_action(action),
        Actor::Landblock(id) => {
            if let Some(landblock) = w.landblock_manager.landblocks.get_mut(id) {
                landblock.enqueue_action(action);
            }
        }
        Actor::Object(guid) => {
            // `Player.EnqueueAction` overrides: the player's own queue (Player_Tick.cs).
            if w.objects.get(guid).is_some_and(WorldObject::is_player) {
                crate::dispatch::enqueue_action::enqueue_action(w, guid, action);
            } else {
                world_object_enqueue_action(w, guid, action);
            }
        }
    }
}

// ACE: WorldObject.EnqueueAction
/// Enqueue work to be done on this object's landblock. If this is a detached creature, the work
/// is discarded. If this is a detached non-creature object, the work is enqueued onto
/// `WorldManager`.
///
/// The body of `WorldObject.EnqueueAction` (WorldObject_Tick.cs), kept here because it is the
/// routing rule; that file's dispatch stub should call it.
pub fn world_object_enqueue_action(w: &mut World, this: ObjectGuid, action: Action) {
    // A missing object is ACE's destroyed object: item is gone, no more work can be done to it.
    let Some(o) = w.objects.get(this) else {
        return;
    };

    let Some(current_landblock) = o.current_landblock else {
        if decay_completed(o) {
            // Item probably completed decay and started the fade-out process right before the landblock unloaded.
        } else if o.is_creature() {
            // If we've hit this point, something is asking to add work to a detached creature
            // It's likely a DelayManager processing a NextAct() action, and that action is likely an emote.
            // We don't need to emote Creatures.
            // If we find that there is a case where Creatures need to act after they've been detached from the landblock,
            // that work should be enqueued onto WorldManager
        } else if o.is_spell_projectile() {
            // Do no more work for detached spell projectiles
        } else if o.is_generator() {
            // This is a detached generator, we don't need to further the action chain
        } else {
            // Enqueue work for detached objects onto our thread-safe WorldManager

            // Here we filter out warnings for known cases where work may be queued onto an item that doesn't exist on a landblock
            if o.is_slum_lord() {
                // Slumlords (housing) can be loaded without its landblock
            } else if o.is_container() && !inventory_loaded(o) {
                // Containers enqueue the loading of their inventory from callbacks. It's possible the callback happened before the container was added to the landblock
            } else if !o.owner_id().is_some_and(|owner| owner > 0) {
                log::warn!(
                    "Item 0x{:08X}:{} has enqueued an action but is not attached to a landblock.",
                    this.full(),
                    o.get_property(PropertyString::Name).unwrap_or_default()
                );
            }

            world_manager::enqueue_action(w, action);
        }
        return;
    };

    enqueue(w, Actor::Landblock(current_landblock), action);
}

/// `decayCompleted` (a `WorldObject_Decay.cs` field, set by `Decay`).
fn decay_completed(o: &WorldObject) -> bool {
    crate::world_objects::world_object_decay::decay_completed(o)
}

/// `container.InventoryLoaded` (a `Container.cs` field, set by `SortBiotasIntoInventory` /
/// `SortWorldObjectsIntoInventory`).
fn inventory_loaded(o: &WorldObject) -> bool {
    o.container
        .as_ref()
        .expect("a Container")
        .container
        .inventory_loaded
}

/// The `ActionQueue` an actor owns, for
/// [`run_actions`](crate::entity::actions::action_queue::run_actions). `None` for the delay manager
/// (not an `ActionQueue`), for a landblock that is not loaded, and for objects other than players
/// (a player's is `Player.actionQueue`, run by `Player_Tick`; other objects have none).
pub fn action_queue_mut(w: &mut World, actor: Actor) -> Option<&mut ActionQueue> {
    match actor {
        Actor::World => Some(&mut w.world_manager.action_queue),
        Actor::InboundMessageQueue => Some(&mut w.sessions.inbound.inbound_message_queue),
        Actor::Landblock(id) => w
            .landblock_manager
            .landblocks
            .get_mut(id)
            .map(|l| l.action_queue_mut()),
        Actor::Object(guid) => crate::world_objects::player_tick::action_queue_mut(w, guid),
        Actor::Delay => None,
    }
}
