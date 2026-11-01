//! The world's locks: which placed objects (doors) are locked, and the key id that opens each.

use std::collections::BTreeMap;

/// A placed object's lock: the id a key must carry to open it (none known: no key opens it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Lock {
    pub key: Option<u32>,
}

/// The locked objects, by their world ids.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LockTable {
    pub objects: BTreeMap<u64, Lock>,
}

impl LockTable {
    /// The lock of a placed object, if it is locked.
    #[must_use]
    pub fn of(&self, object: u64) -> Option<&Lock> {
        self.objects.get(&object)
    }
}
