//! AC1's own content taken off a converted block: the converted land holds no AC1 retail content,
//! so a block's landblock info is written without AC1's buildings, its cells, its static objects
//! and its house restrictions. AC2's statics, its buildings with rooms and its dungeons are added
//! to what is left.
//!
//! **Cells:** every one of the block's cells goes, the rooms a building's doorways lead into (a
//! dungeon under an entrance among them) and the cells no building reaches (an AC1 dungeon under
//! the land) alike; the converter deletes all of AC1's records of the block through the overlay
//! with one deletion, and writes the block's own over it, its cell count starting again from
//! none.

use std::collections::{BTreeSet, VecDeque};

use dereth_assets::world::LandblockInfo;
use dereth_assets::EnvCell;

/// What one block's landblock info lost.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Removed {
    pub buildings: usize,
    /// The cells the buildings reach.
    pub rooms: usize,
    /// The cells no building reaches: AC1's dungeons.
    pub cells: usize,
    /// The block's static objects (AC1's statics standing on the land).
    pub objects: usize,
    /// Entries of the house restriction table (cell to owning house).
    pub restrictions: usize,
}

impl Removed {
    pub fn add(&mut self, o: &Self) {
        self.buildings += o.buildings;
        self.rooms += o.rooms;
        self.cells += o.cells;
        self.objects += o.objects;
        self.restrictions += o.restrictions;
    }

    #[must_use]
    pub fn any(&self) -> bool {
        *self != Self::default()
    }
}

/// A block's landblock info with AC1's content taken off.
#[derive(Clone, Debug, PartialEq)]
pub struct Cleared {
    pub info: LandblockInfo,
    pub removed: Removed,
    /// The numbers of the cells taken off (`0x0100` up): every one the block had.
    pub cell_numbers: Vec<u32>,
}

/// `info` without AC1's content; `cell` reads one of the block's cells by its full id.
#[must_use]
pub fn clear(info: &LandblockInfo, cell: &dyn Fn(u32) -> Option<EnvCell>) -> Cleared {
    let block = info.id.raw() & 0xFFFF_0000;
    let count = info.num_cells;
    let in_block = |k: u32| (0x0100..0x0100 + count).contains(&k);
    let mut rooms = BTreeSet::new();
    let mut queue: VecDeque<u32> = info
        .buildings
        .iter()
        .flat_map(|b| &b.portals)
        .flat_map(|p| std::iter::once(p.other_cell_id).chain(p.stab_list.iter().copied()))
        .map(u32::from)
        .collect();
    while let Some(k) = queue.pop_front() {
        if !in_block(k) || !rooms.insert(k) {
            continue;
        }
        if let Some(c) = cell(block | k) {
            for p in c.portals.iter().filter(|p| p.flags & 4 == 0) {
                queue.push_back(p.other_cell_id & 0xFFFF);
            }
        }
    }
    Cleared {
        info: LandblockInfo {
            id: info.id,
            num_cells: 0,
            objects: Vec::new(),
            pack_mask: info.pack_mask & !1,
            buildings: Vec::new(),
            restrictions: None,
        },
        removed: Removed {
            buildings: info.buildings.len(),
            rooms: rooms.len(),
            cells: (count as usize).saturating_sub(rooms.len()),
            objects: info.objects.len(),
            restrictions: info.restrictions.as_ref().map_or(0, |r| r.entries.len()),
        },
        cell_numbers: (0x0100..0x0100 + count).collect(),
    }
}

/// An empty landblock info for a block AC1 has none for.
#[must_use]
pub fn empty(id: dereth_primitives::DataId) -> LandblockInfo {
    LandblockInfo {
        id,
        num_cells: 0,
        objects: Vec::new(),
        pack_mask: 0,
        buildings: Vec::new(),
        restrictions: None,
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the AC2 world's cleared blocks)
    use super::*;
    use dereth_assets::world::{
        BuildInfo, BuildingPortal, CellPortal, ObjectEntry, RestrictionTable,
    };
    use dereth_primitives::{DataId, Frame, Quat, Vec3};

    fn at(x: f32) -> Frame {
        Frame::new(Vec3::new(x, 10.0, 20.0), Quat::new(1.0, 0.0, 0.0, 0.0))
    }

    /// A cell of block `A9B4` whose ways lead to `to` (`None`: outside).
    fn room(k: u32, to: &[Option<u32>]) -> EnvCell {
        EnvCell {
            id: DataId(0xA9B4_0000 | k),
            flags: 0,
            cell_id_repeat: 0xA9B4_0000 | k,
            surfaces: vec![DataId(0x0800_0001)],
            environment: DataId(0x0D00_0010),
            cell_struct: 0,
            frame: at(1.0),
            portals: to
                .iter()
                .map(|t| CellPortal {
                    flags: if t.is_some() { 0 } else { 4 },
                    polygon_id: 0,
                    other_cell_id: t.unwrap_or(0xFFFF_FFFF),
                    other_portal_id: -1,
                })
                .collect(),
            visible_cells: Vec::new(),
            static_objects: Vec::new(),
            restriction_obj: None,
        }
    }

    /// AC1's block: an AC1 static, a cottage whose doorway leads into `0x0100`, which leads to
    /// `0x0101`; the house restriction for both; and a dungeon cell `0x0102` no building reaches.
    fn ac1_block(dungeon: bool) -> (LandblockInfo, Vec<EnvCell>) {
        let mut cells = vec![
            room(0x0100, &[None, Some(0x0101)]),
            room(0x0101, &[Some(0x0100)]),
        ];
        if dungeon {
            cells.push(room(0x0102, &[]));
        }
        let info = LandblockInfo {
            id: DataId(0xA9B4_FFFE),
            num_cells: u32::try_from(cells.len()).unwrap(),
            objects: vec![ObjectEntry {
                id: DataId(0x0100_1234),
                frame: at(5.0),
            }],
            pack_mask: 1,
            buildings: vec![BuildInfo {
                id: DataId(0x0100_2232),
                frame: at(1.0),
                num_leaves: 2,
                portals: vec![BuildingPortal {
                    flags: 1,
                    other_cell_id: 0x0100,
                    other_portal_id: 0,
                    stab_list: vec![0x0100],
                }],
            }],
            restrictions: Some(RestrictionTable {
                buckets: 8,
                entries: vec![(0xA9B4_0100, 0x7000_0001), (0xA9B4_0101, 0x7000_0001)],
            }),
        };
        (info, cells)
    }

    fn reader(cells: &[EnvCell]) -> impl Fn(u32) -> Option<EnvCell> + '_ {
        move |id| cells.iter().find(|c| c.id.raw() == id).cloned()
    }

    #[test]
    fn a_converted_block_keeps_no_ac1_building_room_or_static_but_keeps_ac2s_statics() {
        let (info, cells) = ac1_block(false);
        let c = clear(&info, &reader(&cells));
        assert!(c.info.buildings.is_empty() && c.info.objects.is_empty());
        assert_eq!(c.info.num_cells, 0, "both rooms gone");
        assert!(c.info.restrictions.is_none() && c.info.pack_mask & 1 == 0);
        assert_eq!(
            c.removed,
            Removed {
                buildings: 1,
                rooms: 2,
                cells: 0,
                objects: 1,
                restrictions: 2
            }
        );
        assert_eq!(c.cell_numbers, [0x0100, 0x0101]);
        // AC2's static added to what is left, and an AC2 building numbered from the first cell.
        let mut info = c.info;
        let ac2 = ObjectEntry {
            id: DataId(0x0200_C001),
            frame: at(7.0),
        };
        info.objects.push(ac2);
        use dereth_assets::Decode as _;
        let back = LandblockInfo::decode_payload(info.id, &info.encode()).unwrap();
        assert_eq!(back.objects, vec![ac2], "AC2's static, and only it");
        assert!(back.buildings.is_empty() && back.num_cells == 0);
    }

    #[test]
    fn a_dungeon_no_building_reaches_goes_with_the_rooms() {
        let (info, cells) = ac1_block(true);
        let c = clear(&info, &reader(&cells));
        assert_eq!((c.removed.rooms, c.removed.cells), (2, 1));
        assert_eq!(c.info.num_cells, 0, "no cell of AC1's left in the count");
        assert_eq!(c.cell_numbers, [0x0100, 0x0101, 0x0102]);
        // An AC1 dungeon alone: its cells go all the same.
        let mut alone = info.clone();
        alone.buildings.clear();
        alone.restrictions = None;
        alone.pack_mask = 0;
        alone.objects.clear();
        let c = clear(&alone, &reader(&cells));
        assert_eq!(
            (c.info.num_cells, c.removed.rooms, c.removed.cells),
            (0, 0, 3)
        );
        assert!(
            c.removed.any(),
            "a block with nothing but a dungeon is written"
        );
    }
}
