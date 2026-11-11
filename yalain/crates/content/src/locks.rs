//! The world's locks: which placed objects (doors) are locked, and the key id that opens each.

use std::collections::BTreeMap;

/// A placed object's lock: the id a key must carry to open it (none known: no key opens it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Lock {
    pub key: Option<u32>,
}

/// The locked objects: those with world ids of their own by their ids, and those the server
/// numbers (a world's authored objects) by their cell and template.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LockTable {
    pub objects: BTreeMap<u64, Lock>,
    pub placed: BTreeMap<(u32, u32), Lock>,
}

impl LockTable {
    /// The lock of a placed object with a world id of its own, if it is locked.
    #[must_use]
    pub fn of(&self, object: u64) -> Option<&Lock> {
        self.objects.get(&object)
    }

    /// The lock of a placed object the server numbers, by its cell and template, if it is locked.
    #[must_use]
    pub fn of_placed(&self, cell: u32, template: u32) -> Option<&Lock> {
        self.placed.get(&(cell, template))
    }
}
