//! The single owner of every live world object.
//!
//! Not an ACE port: ACE keeps objects alive through references held everywhere. Here every
//! reference is an [`ObjectGuid`] resolved through this store, and a missing entry stands for ACE's
//! `null` / `IsDestroyed`.
//!
//! There is deliberately **no iteration API**. Iteration order over this store is not something
//! ACE has, so nothing may depend on it. Ordered collections (a landblock's objects, a container's
//! inventory) keep their own `DotNetDict` of guids.

use std::collections::HashMap;

use empyrean_entity::ObjectGuid;

use crate::world_objects::world_object::WorldObject;

#[derive(Debug, Default)]
pub struct ObjectStore {
    // Boxed: a `WorldObject` is over a kilobyte, and boxes keep rehashing cheap.
    objects: HashMap<ObjectGuid, Box<WorldObject>>,
}

impl ObjectStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds an object under its own guid. Returns the object back if the guid is already taken
    /// (ACE would have two live objects with one guid; callers treat this as an error).
    pub fn insert(&mut self, obj: impl Into<Box<WorldObject>>) -> Result<(), Box<WorldObject>> {
        use std::collections::hash_map::Entry;
        let obj = obj.into();
        match self.objects.entry(obj.guid) {
            Entry::Occupied(_) => Err(obj),
            Entry::Vacant(v) => {
                v.insert(obj);
                Ok(())
            }
        }
    }

    pub fn remove(&mut self, guid: ObjectGuid) -> Option<Box<WorldObject>> {
        self.objects.remove(&guid)
    }

    pub fn contains(&self, guid: ObjectGuid) -> bool {
        self.objects.contains_key(&guid)
    }

    pub fn get(&self, guid: ObjectGuid) -> Option<&WorldObject> {
        self.objects.get(&guid).map(|b| &**b)
    }

    pub fn get_mut(&mut self, guid: ObjectGuid) -> Option<&mut WorldObject> {
        self.objects.get_mut(&guid).map(|b| &mut **b)
    }

    /// Two distinct objects mutably at once. `None` if either is missing or `a == b`.
    pub fn get2_mut(
        &mut self,
        a: ObjectGuid,
        b: ObjectGuid,
    ) -> Option<(&mut WorldObject, &mut WorldObject)> {
        if a == b {
            return None;
        }
        let [x, y] = self.objects.get_disjoint_mut([&a, &b]);
        Some((&mut **x?, &mut **y?))
    }

    pub fn len(&self) -> usize {
        self.objects.len()
    }

    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }
}
