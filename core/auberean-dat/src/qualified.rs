//! Record ids with their types, and the ordered sets of them a record's references are gathered
//! into: the records one record needs (its pictures, its string tables, the entities it places),
//! which the client loads, checks and downloads together.

use std::collections::BTreeSet;

use crate::types::{type_of_id, RecordType};

/// A record id and its type. Ordered by id, then type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct QualifiedId {
    pub id: u32,
    /// The type number (`0x400000NN`); 0 when no type is known for the id.
    pub kind: u32,
}

impl QualifiedId {
    /// An id with a type; a type of 0 is divined from the id ([`divine_type`]).
    // D2-12832
    #[must_use]
    pub fn new(id: u32, kind: u32) -> Self {
        let kind = if id != 0 && kind == 0 {
            divine_type(id)
        } else {
            kind
        };
        Self { id, kind }
    }
}

/// The type number of a record id (0 for none): by the id chain of the portal, local and
/// high-resolution files.
// D2-12731, D2-12009
#[must_use]
pub fn divine_type(id: u32) -> u32 {
    type_of_id(id).map_or(0, |t| t.0)
}

/// The type number of a cell-file record from its id's low word (0 for none).
// D2-12730
#[must_use]
pub fn divine_cell_type(low: u16) -> u32 {
    match low {
        0x0100..=0xEFFF => RecordType::CELL.0,
        0xFFFC => RecordType::LIGHT_INFO.0,
        0xFFFD => RecordType::PATH_MAP.0,
        0xFFFE => RecordType::LANDBLOCK_INFO.0,
        0xFFFF => RecordType::LANDBLOCK.0,
        _ => 0,
    }
}

/// An ordered set of qualified ids, without repeats and without id 0.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct QualifiedIds {
    set: BTreeSet<QualifiedId>,
}

impl QualifiedIds {
    // D2-02013
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an id, its type divined from it; `false` for id 0 or one already held.
    // D2-12838
    pub fn add(&mut self, id: u32) -> bool {
        self.add_typed(id, 0)
    }

    /// Add an id with a type (0: divined); `false` for id 0 or one already held.
    // D2-12838, D2-12836
    pub fn add_typed(&mut self, id: u32, kind: u32) -> bool {
        if id == 0 {
            return false;
        }
        self.set.insert(QualifiedId::new(id, kind))
    }

    /// Add each id of a list that is not 0, its type divined.
    // D2-12839
    pub fn add_ids(&mut self, ids: &[u32]) {
        for &id in ids {
            self.add(id);
        }
    }

    /// The `n`th id in order.
    // D2-12833
    #[must_use]
    pub fn get(&self, n: usize) -> Option<QualifiedId> {
        self.set.iter().nth(n).copied()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.set.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.set.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = QualifiedId> + '_ {
        self.set.iter().copied()
    }

    #[must_use]
    pub fn contains(&self, id: u32) -> bool {
        self.set.iter().any(|q| q.id == id)
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the ordered set's own rules)
    use super::*;

    #[test]
    fn ids_are_kept_in_id_order_once_each_and_never_zero() {
        let mut s = QualifiedIds::new();
        assert!(s.add(0x2500_0010));
        assert!(s.add(0x0600_0001));
        assert!(!s.add(0x0600_0001));
        assert!(!s.add(0));
        assert!(s.add_typed(0x0600_0001, 0x4000_0026));
        let ids: Vec<_> = s.iter().map(|q| (q.id, q.kind)).collect();
        assert_eq!(
            ids,
            [
                (0x0600_0001, 0x4000_000A),
                (0x0600_0001, 0x4000_0026),
                (0x2500_0010, 0x4000_0026)
            ]
        );
        assert_eq!(s.get(2).map(|q| q.id), Some(0x2500_0010));
    }
}
