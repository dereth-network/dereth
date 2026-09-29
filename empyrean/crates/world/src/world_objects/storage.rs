// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Storage.cs
//! Port of `Source/ACE.Server/WorldObjects/Storage.cs`.

use empyrean_entity::enums::Sound;
use empyrean_entity::ObjectGuid;

use crate::entity::activation_result::ActivationResult;
use crate::network::game_messages::messages::game_message_sound::game_message_sound;
use crate::world_objects::world_object::{CtorEnv, CtorSource, WorldObject};
use crate::world_objects::{container, house, player_networking, world_object_networking};
use crate::{dispatch, World};

/// Non-property fields declared in `Storage.cs`.
#[derive(Debug, Default)]
pub struct StorageFields {}

// ACE: Storage.House
#[must_use]
pub fn house(w: &World, this: ObjectGuid) -> Option<ObjectGuid> {
    let parent = w.objects.get(this)?.wo.world_object_links.parent_link?;
    w.objects
        .get(parent)
        .is_some_and(WorldObject::is_house)
        .then_some(parent)
}

// ACE: Storage.Default_ChestResetInterval
#[must_use]
pub fn storage_default_chest_reset_interval(_w: &World, _this: ObjectGuid) -> f64 {
    f64::INFINITY
}

// ---- constructors and SetEphemeralValues ----

/// `new Storage(weenie, guid)` / `new Storage(biota)`: the `Chest` constructor, then
/// Storage's `SetEphemeralValues`.
// ACE: Storage.Storage
pub fn storage_ctor(o: &mut WorldObject, env: &CtorEnv<'_>, src: CtorSource) {
    crate::world_objects::chest::chest_ctor(o, env, src);
    storage_set_ephemeral_values(o, env);
}

// ACE: Storage.SetEphemeralValues
fn storage_set_ephemeral_values(o: &mut WorldObject, _env: &CtorEnv<'_>) {
    o.set_is_locked(false);
    o.set_is_open(false);

    // unanimated objects will float in the air, and not be affected by gravity
    // unless we give it a bit of velocity to start
    // fixes floating storage chests
    //Velocity = new Vector3(0, 0, 0.5f);
    o.wo.world_object.bump_velocity = true;
}

// ACE: Storage.CheckUseRequirements
pub fn storage_check_use_requirements(
    w: &mut World,
    this: ObjectGuid,
    activator: ObjectGuid,
) -> ActivationResult {
    let base_requirements =
        crate::world_objects::chest::chest_check_use_requirements(w, this, activator);
    if !base_requirements.success {
        return base_requirements;
    }

    if !w.objects.get(activator).is_some_and(WorldObject::is_player) {
        return ActivationResult::new(false);
    }
    let player = activator;

    if w.objects
        .get(player)
        .is_some_and(WorldObject::ignore_house_barriers)
    {
        return ActivationResult::new(true);
    }

    let root_house = house(w, this).and_then(|h| house::root_house(w, h));

    let Some(root_house) = root_house else {
        log::error!(
            "[HOUSE] {} tried to use Storage chest @ {}, couldn't find RootHouse (this shouldn't happen)",
            dispatch::name::name(w, player).unwrap_or_default(),
            w.objects.get(this).and_then(WorldObject::location).map_or_else(String::new, |l| l.to_string())
        );
        return ActivationResult::new(false);
    };

    if !house::has_permission(w, root_house, player, true) {
        let name = dispatch::name::name(w, this).unwrap_or_default();
        player_networking::send_transient_error(
            w,
            player,
            &format!("You do not have permission to access {name}"),
        );
        let m = game_message_sound(this, Sound::OpenFailDueToLock, 1.0);
        world_object_networking::enqueue_broadcast(w, this, true, &[m]);
        return ActivationResult::new(false);
    }
    ActivationResult::new(true)
}

/// This event is raised when player adds item to storage
// ACE: Storage.OnAddItem
pub fn storage_on_add_item(w: &mut World, this: ObjectGuid) {
    //Console.WriteLine("Storage.OnAddItem()");

    if !container::inventory_values(w, this).is_empty() {
        // Here we explicitly save the storage to the database to prevent item loss.
        // If the player adds an item to the storage, and the server crashes before the storage has been saved, the item will be lost.
        dispatch::save_biota_to_database::save_biota_to_database(w, this, true);
    }
}

/// This event is raised when player removes item from storage
// ACE: Storage.OnRemoveItem
pub fn storage_on_remove_item(w: &mut World, this: ObjectGuid, _removed_item: ObjectGuid) {
    //Console.WriteLine("Storage.OnRemoveItem()");

    // Here we explicitly save the storage to the database to prevent property desync.
    dispatch::save_biota_to_database::save_biota_to_database(w, this, true);
}
