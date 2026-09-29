// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Key.cs
//! Port of `Source/ACE.Server/WorldObjects/Key.cs`.
//!
//! A key used on a lock (`UseWithTarget`): the use requirements, a broken key's removal, then
//! `UnlockerHelper.UseUnlocker` (`lock.rs`). `KeyCode` and `OpensAnyLock` are generated in
//! `props/key.rs`.

use empyrean_entity::enums::WeenieError;
use empyrean_entity::ObjectGuid;

use crate::managers::player_manager::player_session;
use crate::network::game_messages::game_message::enqueue_send;
use crate::world_objects::player_inventory::{self, SearchLocations};
use crate::world_objects::{lock, player_use};
use crate::{dispatch, World};

/// Non-property fields declared in `Key.cs`.
#[derive(Debug, Default)]
pub struct KeyFields {}

/// Verifies the use requirements, removes a key whose `Structure` is 0 or above its
/// `MaxStructure`, and otherwise uses it on `target`.
// ACE: Key.HandleActionUseOnTarget
pub fn key_handle_action_use_on_target(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    player: empyrean_entity::ObjectGuid,
    target: empyrean_entity::ObjectGuid,
) {
    // verify use requirements
    let result = dispatch::check_use_requirements::check_use_requirements(w, this, player);

    if !result.success {
        if let Some(message) = result.message {
            let session =
                player_session(w, player).expect("System.NullReferenceException: Player.Session");
            enqueue_send(w, session, message);
        }

        player_use::send_use_done_event(w, player, WeenieError::None);
        return;
    }

    let o = w
        .objects
        .get(this)
        .expect("System.NullReferenceException: this");
    let (structure, max_structure) = (o.structure(), o.max_structure());
    // `Structure == 0 || Structure > MaxStructure`: lifted, false for a null operand
    let broken =
        structure == Some(0) || matches!((structure, max_structure), (Some(s), Some(m)) if s > m);
    if broken {
        let name = |g: ObjectGuid| dispatch::name::name(w, g).unwrap_or_default();
        log::warn!(
            "Key.HandleActionUseOnTarget: Structure / MaxStructure is {} / {} for {} (0x{}:{}), used on {} (0x{}:{}) and used by {} (0x{})",
            structure.map(|s| s.to_string()).unwrap_or_default(),
            max_structure.map(|s| s.to_string()).unwrap_or_default(),
            name(this),
            this,
            o.biota.weenie_class_id,
            name(target),
            target,
            w.objects.get(target).map_or(0, |t| t.biota.weenie_class_id),
            name(player),
            player
        );

        let found = player_inventory::find_object(w, player, this, SearchLocations::Everywhere);
        world_object_delete_object(w, this, found.root_owner);

        player_use::send_use_done_event(w, player, WeenieError::YouCannotUseThatItem);
        return;
    }

    lock::use_unlocker(w, player, this, target);
}

// ================================================================================ pointers

/// `WorldObject.DeleteObject(rootOwner)` (`WorldObject_Decay.cs`).
fn world_object_delete_object(w: &mut World, this: ObjectGuid, root_owner: Option<ObjectGuid>) {
    crate::world_objects::world_object_decay::delete_object(w, this, root_owner);
}

// ---- constructors and SetEphemeralValues ----

/// `new Key(weenie, guid)` / `new Key(biota)`: the `WorldObject` constructor, then
/// Key's `SetEphemeralValues`.
// ACE: Key.Key
pub fn key_ctor(
    o: &mut crate::world_objects::world_object::WorldObject,
    env: &crate::world_objects::world_object::CtorEnv<'_>,
    src: crate::world_objects::world_object::CtorSource,
) {
    crate::world_objects::world_object::world_object_ctor(o, env, src);
    key_set_ephemeral_values(o, env);
}

// ACE: Key.SetEphemeralValues
fn key_set_ephemeral_values(
    o: &mut crate::world_objects::world_object::WorldObject,
    _env: &crate::world_objects::world_object::CtorEnv<'_>,
) {
    // These shoudl come from the weenie. After confirmation, remove these
    //KeyCode = AceObject.KeyCode ?? "";
    //Structure = AceObject.Structure ?? AceObject.MaxStructure;

    // `Structure > MaxStructure` is false when either is null.
    if let (Some(structure), Some(max_structure)) = (o.structure(), o.max_structure()) {
        if structure > max_structure {
            o.set_structure(o.max_structure());
        }
    }
}
