// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/WorldObject_Database.cs
//! Port of `Source/ACE.Server/WorldObjects/WorldObject_Database.cs`.
//!
//! ACE's `BiotaDatabaseLock` is not needed: the shard gets an owned snapshot of the biota, taken
//! on the world thread, and its callback runs on the world thread.

use empyrean_common::dotnet::datetime::DotNetDateTime;
use empyrean_entity::enums::{PhysicsState, PropertyString, WeenieType};
use empyrean_entity::ObjectGuid;

use crate::world_objects::kinds::KindData;
use crate::world_objects::world_object::WorldObject;
use crate::World;

/// Non-property fields declared in `WorldObject_Database.cs`.
#[derive(Debug, Default)]
pub struct WorldObjectDatabaseFields {
    /// True for an object built by the `(Biota)` constructor (restored from the shard).
    // ACE: WorldObject.biotaOriginatedFromDatabase
    pub biota_originated_from_database: bool,
    /// `DateTime.MinValue` (the default) until a save is requested.
    // ACE: WorldObject.LastRequestedDatabaseSave
    pub last_requested_database_save: DotNetDateTime,
    /// Set when a property or position change reaches the biota; cleared before a save is
    /// requested. The trigger for saving on add, modify or remove of properties.
    // ACE: WorldObject.ChangesDetected
    pub changes_detected: bool,
}

impl WorldObject {
    // ACE: WorldObject.BiotaOriginatedFromOrHasBeenSavedToDatabase
    #[must_use]
    pub fn biota_originated_from_or_has_been_saved_to_database(&self) -> bool {
        let db = &self.wo.world_object_database;
        db.biota_originated_from_database
            || db.last_requested_database_save != DotNetDateTime::MIN_VALUE
    }
}

/// This will set the LastRequestedDatabaseSave to MinValue and ChangesDetected to true.
/// If `enqueue_remove` is set to true, `DatabaseManager.Shard.RemoveBiota()` will be called for
/// the biota. Set it to false to perform all the normal routines for a remove but not the actual
/// removal (for collecting biotas to remove in bulk).
// ACE: WorldObject.RemoveBiotaFromDatabase
pub fn remove_biota_from_database(w: &mut World, this: ObjectGuid, enqueue_remove: bool) {
    let Some(o) = w.objects.get_mut(this) else {
        return;
    };

    // If this entity doesn't exist in the database, let's not queue up work unnecessary database work.
    if !o.biota_originated_from_or_has_been_saved_to_database() {
        o.wo.world_object_database.changes_detected = true;
        return;
    }

    o.wo.world_object_database.last_requested_database_save = DotNetDateTime::MIN_VALUE;
    o.wo.world_object_database.changes_detected = true;

    if enqueue_remove {
        let id = o.biota.id;
        w.shard.remove_biota(id, None);
    }
}

/// This will set the LastRequestedDatabaseSave to UtcNow and ChangesDetected to false.
/// If `enqueue_save` is set to true, `DatabaseManager.Shard.SaveBiota()` will be called for the
/// biota. Set it to false to perform all the normal routines for a save but not the actual save
/// (for collecting biotas in bulk for bulk saving: the caller then snapshots the biota).
///
/// The shard gets an owned snapshot of the biota taken here; the callback runs on the world
/// thread. A guid missing from the store is a destroyed object: nothing is saved.
// ACE: WorldObject.SaveBiotaToDatabase
// Not ACE's (a fix, V343): an enqueued save stamps `CheckpointTimestamp`
// before it clears the changed flag, so a saved object is left unchanged and is not saved again
// until something else changes. ACE stamped it after, and the stamp marked the object changed
// again, so every save was followed by another at the next landblock or player save.
pub fn world_object_save_biota_to_database(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    enqueue_save: bool,
) {
    let utc_now = w.now.utc;
    let current_unix_time = w.now.unix_time;
    let Some(o) = w.objects.get_mut(this) else {
        return;
    };

    // Make sure all of our positions in the biota are up to date with our current cached values.
    let cached: Vec<_> =
        o.wo.world_object_properties
            .position_cache
            .iter()
            .filter_map(|(k, v)| v.map(|position| (*k, position)))
            .collect();
    for (position_type, position) in cached {
        o.biota.set_position(position_type, &position);
    }

    if enqueue_save {
        o.set_checkpoint_timestamp(Some(current_unix_time));
    }

    o.wo.world_object_database.last_requested_database_save = utc_now;
    o.wo.world_object_database.changes_detected = false;

    if enqueue_save {
        //DatabaseManager.Shard.SaveBiota(Biota, BiotaDatabaseLock, null);
        let snapshot = o.biota.clone();
        w.shard.save_biota(
            snapshot,
            Some(Box::new(move |w: &mut crate::World, result: bool| {
                if !result {
                    if let Some(player) = w.objects.get_mut(this).and_then(|o| o.player.as_mut()) {
                        // This will trigger a boot on next player tick
                        player.player_database.biota_save_failed = true;
                    }
                }
            })),
        );
    }
}

/// `slumlord.House` (`SlumLord.cs`: `ParentLink as House`).
fn slum_lord_house(w: &World, slumlord: ObjectGuid) -> Option<&WorldObject> {
    let parent = w.objects.get(slumlord)?.wo.world_object_links.parent_link?;
    w.objects.get(parent).filter(|p| p.is_house())
}

/// `container.Inventory.Count`.
fn container_inventory_count(o: &WorldObject) -> usize {
    o.container
        .as_ref()
        .map_or(0, |c| c.container.inventory.len())
}

/// A static that should persist to the shard may be a hook with an item, or a house that's been
/// purchased, or a housing chest that isn't empty, etc. If the world object originated from the
/// database or has been saved to the database, this will also return true. A guid missing from
/// the store answers false.
// ACE: WorldObject.IsStaticThatShouldPersistToShard
#[must_use]
pub fn is_static_that_should_persist_to_shard(w: &World, this: ObjectGuid) -> bool {
    let Some(o) = w.objects.get(this) else {
        return false;
    };

    if !o.guid.is_static() {
        return false;
    }

    if o.biota_originated_from_or_has_been_saved_to_database() {
        return true;
    }

    if o.biota.weenie_type == WeenieType::SlumLord && o.is_slum_lord() {
        if let Some(house) = slum_lord_house(w, this) {
            if house.house_owner().is_some_and(|owner| owner != 0) {
                return true;
            }
        }
    }

    if o.biota.weenie_type == WeenieType::House
        && o.is_house()
        && o.house_owner().is_some_and(|owner| owner != 0)
    {
        return true;
    }

    if (o.biota.weenie_type == WeenieType::Hook || o.biota.weenie_type == WeenieType::Storage)
        && o.is_container()
        && container_inventory_count(o) > 0
    {
        return true;
    }

    false
}

/// This will filter out the following: Ammunition and Spell projectiles; monster corpses;
/// missiles that haven't been saved to the shard yet. If the world object originated from the
/// database or has been saved to the database, this will also return true. A guid missing from
/// the store answers false.
// ACE: WorldObject.IsDynamicThatShouldPersistToShard
#[must_use]
pub fn is_dynamic_that_should_persist_to_shard(w: &World, this: ObjectGuid) -> bool {
    let Some(o) = w.objects.get(this) else {
        return false;
    };

    if !o.guid.is_dynamic() {
        return false;
    }

    if o.biota_originated_from_or_has_been_saved_to_database() {
        return true;
    }

    // Don't save generators, and items that were generated by a generator
    // If the item was generated by a generator and then picked up by a player, the wo.Generator property would be set to null.
    if o.is_generator() || o.wo.world_object_generators.generator.is_some() {
        return false;
    }

    let weenie_type = o.biota.weenie_type;
    if weenie_type == WeenieType::Missile
        || weenie_type == WeenieType::Ammunition
        || weenie_type == WeenieType::ProjectileSpell
        || weenie_type == WeenieType::GamePiece
        || weenie_type == WeenieType::Pet
        || weenie_type == WeenieType::CombatPet
    {
        return false;
    }

    if weenie_type == WeenieType::Corpse {
        if let KindData::Corpse(corpse) = &o.kind {
            if corpse.corpse.is_monster {
                return false;
            }
        }
    }

    // `this is Portal portal && portal.IsGateway` (Portal.cs: `IsGateway => WeenieClassId == 1955`).
    if weenie_type == WeenieType::Portal && o.is_portal() && o.biota.weenie_class_id == 1955 {
        return false;
    }

    // Missiles are unique. The only missiles that are persistable are ones that already exist in the database.
    // TODO: See if we can remove this check by catching the WeenieType above.
    // `Missile` reads `GetPhysicsState(PhysicsState.Missile)`: never null, false without a body.
    let missile = crate::physics::phys_ext::get_physics_state(w, this, PhysicsState::Missile);
    if missile {
        log::warn!(
            "Missile: WeenieClassId: {}, Name: {}, WeenieType: {:?}, detected in IsDynamicThatShouldPersistToShard() that wasn't caught by prior check.",
            o.biota.weenie_class_id,
            o.get_property(PropertyString::Name).unwrap_or_default(),
            weenie_type
        );
        return false;
    }

    true
}
