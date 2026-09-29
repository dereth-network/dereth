// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/UniqueTable.cs
//! Port of `Source/ACE.Server/Entity/UniqueTable.cs`.
//!
//! Helper counting the # of uniques in a container, and possibly its sub-containers.

use empyrean_common::dotnet::DotNetDict;
use empyrean_entity::ObjectGuid;

use crate::World;

// ACE: UniqueTable
#[derive(Debug, Clone, Default)]
pub struct UniqueTable {
    /// wcid => entry, in .NET insertion order.
    // ACE: UniqueTable.Entries
    pub entries: DotNetDict<u32, UniqueTableEntry>,
}

impl UniqueTable {
    /// `uniques` are objects in `World.objects`.
    ///
    /// # Panics
    /// When an object is gone (ACE's `NullReferenceException`).
    // ACE: UniqueTable.UniqueTable
    #[must_use]
    pub fn new(w: &World, uniques: &[ObjectGuid]) -> Self {
        let mut entries: DotNetDict<u32, UniqueTableEntry> = DotNetDict::new();

        for &unique in uniques {
            let o = w
                .objects
                .get(unique)
                .expect("System.NullReferenceException: unique");
            let wcid = o.biota.weenie_class_id;
            let stack_size = o.stack_size().unwrap_or(1);

            if let Some(entry) = entries.get_mut(&wcid) {
                entry.count = entry.count.wrapping_add(stack_size);
            } else {
                entries.add(
                    wcid,
                    UniqueTableEntry::new(stack_size, o.unique().unwrap_or(0)),
                );
            }
        }

        Self { entries }
    }
}

// ACE: UniqueTable.UniqueTableEntry
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UniqueTableEntry {
    // ACE: UniqueTable.UniqueTableEntry.Count
    pub count: i32,
    // ACE: UniqueTable.UniqueTableEntry.Max
    pub max: i32,
}

impl UniqueTableEntry {
    // ACE: UniqueTable.UniqueTableEntry.UniqueTableEntry
    #[must_use]
    pub fn new(count: i32, max: i32) -> Self {
        Self { count, max }
    }
}
