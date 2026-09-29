// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Physics/Managers/ServerObjectManager.cs
//! Port of `Source/ACE.Server/Physics/Managers/ServerObjectManager.cs`.
//!
//! ACE's static class is `World.server_object_manager`. It maps a physics object id (the object's
//! guid) to its body in `World.physics`, which is how ACE turns an id held in a collision record
//! back into a `PhysicsObj`.

use dereth_physics::PhysHandle;
use empyrean_common::dotnet::DotNetDict;

use crate::physics::phys_ext;
use crate::World;

/// The state of ACE's static `ServerObjectManager`.
#[derive(Debug, Default)]
pub struct ServerObjectManagerState {
    /// Custom lookup table of PhysicsObjs for the server. ACE's `ConcurrentDictionary`; nothing
    /// enumerates it, so its order is not observable.
    // ACE: ServerObjectManager.ServerObjects
    pub server_objects: DotNetDict<u32, PhysHandle>,
}

/// Adds a PhysicsObj to the static list of server-wide objects. A handle that names no live body
/// stands for ACE's `null` argument.
// ACE: ServerObjectManager.AddServerObject
pub fn add_server_object(w: &mut World, obj: PhysHandle) {
    if let Some(id) = phys_ext::id(w, obj) {
        w.server_object_manager.server_objects.insert(id, obj);
    }
}

/// Removes a PhysicsObj from the static list of server-wide objects.
// ACE: ServerObjectManager.RemoveServerObject
pub fn remove_server_object(w: &mut World, obj: PhysHandle) {
    if let Some(id) = phys_ext::id(w, obj) {
        w.server_object_manager.server_objects.remove(&id);
    }
}

/// Returns a PhysicsObj for an object ID.
// ACE: ServerObjectManager.GetObjectA
pub fn get_object_a(w: &World, object_id: u32) -> Option<PhysHandle> {
    w.server_object_manager
        .server_objects
        .get(&object_id)
        .copied()
}
