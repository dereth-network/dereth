// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/WorldObjectInfo.cs
//! Port of `Source/ACE.Server/Entity/WorldObjectInfo.cs`.
//!
//! ACE holds a `WeakReference<WorldObject>`; here the reference is the object's guid, resolved
//! against `World.objects` on use. An object gone from the store is ACE's
//! collected target.

use empyrean_entity::enums::WeenieType;
use empyrean_entity::ObjectGuid;

use crate::World;

/// This is a light weight object that holds a weak reference to a WorldObject. In addition, it also
/// caches values from that WorldObject incase the reference is released. This way, we can still
/// access values that we may have needed from the WorldObject after it is gone.
// ACE: WorldObjectInfo
#[derive(Debug, Clone, PartialEq)]
pub struct WorldObjectInfo {
    // ACE: WorldObjectInfo.WorldObjectRef
    pub world_object_ref: ObjectGuid,
    // ACE: WorldObjectInfo.Guid
    pub guid: ObjectGuid,
    // ACE: WorldObjectInfo.Name
    pub name: Option<String>,
    // ACE: WorldObjectInfo.WeenieClassId
    pub weenie_class_id: u32,
    // ACE: WorldObjectInfo.WeenieType
    pub weenie_type: WeenieType,
}

impl WorldObjectInfo {
    /// `new WorldObjectInfo(worldObject)`. The virtual `Name` is read through its dispatcher.
    ///
    /// # Panics
    /// When `world_object` is not in the store (ACE's `NullReferenceException`).
    // ACE: WorldObjectInfo.WorldObjectInfo
    #[must_use]
    pub fn new(w: &World, world_object: ObjectGuid) -> Self {
        let o = w.objects.get(world_object).unwrap_or_else(|| {
            panic!("System.NullReferenceException: WorldObjectInfo({world_object:?})")
        });
        let (weenie_class_id, weenie_type) = (o.biota.weenie_class_id, o.biota.weenie_type);

        WorldObjectInfo {
            world_object_ref: world_object,
            guid: world_object,
            name: crate::dispatch::name::name(w, world_object),
            weenie_class_id,
            weenie_type,
        }
    }

    /// If you use this function, you should cache and re-use the result in a local code space. You
    /// should not hold on to the result, nor should you use this function too aggressively.
    // ACE: WorldObjectInfo.TryGetWorldObject
    #[must_use]
    pub fn try_get_world_object(&self, w: &World) -> Option<ObjectGuid> {
        w.objects
            .contains(self.world_object_ref)
            .then_some(self.world_object_ref)
    }
}

/// `WorldObjectInfo<T>`: the same, carrying a value (`default(T)` is `None`).
#[derive(Debug, Clone, PartialEq)]
pub struct WorldObjectInfoOf<T> {
    pub base: WorldObjectInfo,
    // ACE: WorldObjectInfo.Value
    pub value: Option<T>,
}

impl<T> WorldObjectInfoOf<T> {
    /// `new WorldObjectInfo<T>(worldObject, value)`.
    // ACE: WorldObjectInfo.WorldObjectInfo
    #[must_use]
    pub fn with_value(w: &World, world_object: ObjectGuid, value: T) -> Self {
        WorldObjectInfoOf {
            base: WorldObjectInfo::new(w, world_object),
            value: Some(value),
        }
    }

    /// `new WorldObjectInfo<T>(worldObject)`.
    // ACE: WorldObjectInfo.WorldObjectInfo
    #[must_use]
    pub fn new(w: &World, world_object: ObjectGuid) -> Self {
        WorldObjectInfoOf {
            base: WorldObjectInfo::new(w, world_object),
            value: None,
        }
    }
}
