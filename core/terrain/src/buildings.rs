//! The per-cell static object lists of a landblock: the building each land cell owns and the
//! static objects registered in it.
//!
//! The sort cell's building add, its building test and its part add; a cell's part add and the
//! shadow-part list.
//!
//! Static objects and buildings are registered in cells in their original insertion order.

use std::collections::BTreeMap;

use dereth_primitives::{DataId, Frame};

/// Shadow-part-list cap: a cell holds at most 100 shadow
/// parts, and the cell counts them.
pub const MAX_SHADOW_PARTS: usize = 100;

/// One building registered in a land cell. The client records **at most
/// one** building per land cell, and that presence is what suppresses scenery in
/// that cell (the third scenery filter).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Building {
    pub id: DataId,
    pub frame: Frame,
}

/// A landblock's per-cell registration: the building each cell owns, and the static objects
/// registered in it.
#[derive(Debug, Default, Clone)]
pub struct SortCells {
    // ORDER-OK: keyed by the outdoor cell index 1..=64 and iterated in ascending index order, which
    // is a total order independent of insertion, so a BTreeMap is the container that makes that
    // explicit.
    buildings: BTreeMap<u16, Building>,
    statics: BTreeMap<u16, Vec<DataId>>,
}

impl SortCells {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add at most one building per cell. A second building offered to the
    /// same cell is **ignored**, not stacked, which is why a cell with two overlapping buildings
    /// suppresses scenery once rather than twice.
    pub fn add_building(&mut self, cell_index: u16, b: Building) -> bool {
        if self.buildings.contains_key(&cell_index) {
            return false;
        }
        self.buildings.insert(cell_index, b);
        true
    }

    /// Third scenery filter: whether the cell already has a building.
    #[must_use]
    pub fn has_building(&self, cell_index: u16) -> bool {
        self.buildings.contains_key(&cell_index)
    }

    #[must_use]
    pub fn building(&self, cell_index: u16) -> Option<&Building> {
        self.buildings.get(&cell_index)
    }

    /// Register a static object in a cell, up to the 100-part
    /// cap. Returns false when the cell is full, which is what the client's bounded array does.
    pub fn add_static(&mut self, cell_index: u16, id: DataId) -> bool {
        let v = self.statics.entry(cell_index).or_default();
        if v.len() >= MAX_SHADOW_PARTS {
            return false;
        }
        v.push(id);
        true
    }

    /// The statics of one cell, in **registration order**, which is the stable tie order used by
    /// the shadow-part sort.
    #[must_use]
    pub fn statics(&self, cell_index: u16) -> &[DataId] {
        self.statics.get(&cell_index).map_or(&[], Vec::as_slice)
    }

    /// Every cell that owns a building, ascending.
    #[must_use]
    pub fn building_cells(&self) -> Vec<u16> {
        self.buildings.keys().copied().collect()
    }
}

#[cfg(test)]
mod tests {
    // Index arithmetic in test fixtures, bounded by the loops that build them.
    #![allow(clippy::cast_possible_truncation)]

    use super::*;
    use dereth_primitives::{Quat, Vec3};

    fn building(id: u32) -> Building {
        Building {
            id: DataId(id),
            frame: Frame::new(Vec3::ZERO, Quat::IDENTITY),
        }
    }

    /// Oracle: each land cell records at most **one** building per
    /// land cell, and that presence is the scenery filter.
    #[test]
    fn a_cell_holds_at_most_one_building_and_that_suppresses_its_scenery() {
        let mut s = SortCells::new();
        assert!(!s.has_building(5));
        assert!(s.add_building(5, building(1)));
        assert!(s.has_building(5));
        assert!(
            !s.add_building(5, building(2)),
            "a second building is ignored, not stacked"
        );
        assert_eq!(s.building(5).map(|b| b.id), Some(DataId(1)));
        assert!(
            !s.has_building(6),
            "the neighbouring cell still grows scenery"
        );
        assert_eq!(s.building_cells(), vec![5]);
    }

    /// Oracle: a cell's shadow-part list is
    /// bounded at 100, and registration order is what the shadow-part sort's stability preserves.
    #[test]
    fn static_registration_is_ordered_and_capped() {
        let mut s = SortCells::new();
        for i in 0..MAX_SHADOW_PARTS {
            // LINT-OK: index arithmetic over the 100-entry cap.
            assert!(s.add_static(3, DataId(i as u32)), "entry {i}");
        }
        assert!(!s.add_static(3, DataId(999)), "the 101st is refused");
        assert_eq!(s.statics(3).len(), MAX_SHADOW_PARTS);
        assert_eq!(
            s.statics(3)[0],
            DataId(0),
            "registration order is preserved"
        );
        assert_eq!(s.statics(3)[99], DataId(99));
        assert!(s.statics(4).is_empty());
    }
}
