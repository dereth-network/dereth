//! The world's dungeons: the landblocks whose indoor cells are a dungeon's rather than a town's
//! rooms, those under a building's entrance, and what the portals that lead into each call it,
//! or a converted land's own table of them.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use dereth_assets::world::{EnvCell, LandblockInfo};
use dereth_assets::Decode;
use dereth_primitives::{DataId, EraId, Vec3};

use crate::sql_ids::WeenieFacts;

/// One dungeon: its landblock, what it is called, and how many cells it has.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dungeon {
    pub landblock: u16,
    /// The name of a portal that leads into it; `None` when no portal the server knows does.
    pub name: Option<String>,
    pub cells: u32,
    /// Whether a pack places its cells, rather than the world's own files.
    pub pack: bool,
}

/// The weenie type of a portal.
const PORTAL: u32 = 7;

/// Every landblock of the cell file `read` reads (of the files of `era`) whose first indoor cell
/// is not seen from outside — a dungeon's, where a town's rooms are a building's and seen from
/// outside — with its number of cells.
#[must_use]
pub fn retail(read: &dyn Fn(DataId) -> Option<Vec<u8>>, era: EraId) -> BTreeMap<u16, u32> {
    let mut out = BTreeMap::new();
    for lb in 0..=u16::MAX {
        let first = u32::from(lb) << 16;
        let Some(bytes) = read(DataId(first | 0xFFFE)) else {
            continue;
        };
        let Ok(info) = LandblockInfo::decode_payload_for(era, DataId(first | 0xFFFE), &bytes)
        else {
            continue;
        };
        if info.num_cells == 0 {
            continue;
        }
        let id = DataId(first | 0x0100);
        let dungeon = read(id)
            .and_then(|b| EnvCell::decode_payload_for(era, id, &b).ok())
            .is_some_and(|c| c.flags & 1 == 0);
        if dungeon {
            out.insert(lb, info.num_cells);
        }
    }
    out
}

/// The most rooms a building of a town has: one whose doorways lead into more stands over a
/// dungeon, its entrance (a converted land's AC2 dungeon under its entrance).
pub const ENTRANCE_ROOMS: usize = 8;

/// Every landblock of the cell file `read` reads (of the files of `era`) where a building stands
/// over a dungeon: its doorways and the ways between its rooms reach more than
/// [`ENTRANCE_ROOMS`] rooms, or the landblock holds cells no building reaches (a dungeon beside
/// its entrance's hall, a portal between them). For each, the dungeon's rooms (those the largest
/// such building reaches, else those none does) and where its building stands (landblock meters).
#[must_use]
pub fn under_entrances(
    read: &dyn Fn(DataId) -> Option<Vec<u8>>,
    era: EraId,
) -> BTreeMap<u16, (u32, Vec3)> {
    let mut out = BTreeMap::new();
    for lb in 0..=u16::MAX {
        let first = u32::from(lb) << 16;
        let Some(bytes) = read(DataId(first | 0xFFFE)) else {
            continue;
        };
        let Ok(info) = LandblockInfo::decode_payload_for(era, DataId(first | 0xFFFE), &bytes)
        else {
            continue;
        };
        if info.num_cells == 0 || info.buildings.is_empty() {
            continue;
        }
        let cell = |k: u16| {
            let id = DataId(first | u32::from(k));
            read(id).and_then(|b| EnvCell::decode_payload_for(era, id, &b).ok())
        };
        let mut reached: BTreeSet<u16> = BTreeSet::new();
        for b in &info.buildings {
            let mut rooms: BTreeSet<u16> = BTreeSet::new();
            let mut todo: Vec<u16> = b.portals.iter().map(|p| p.other_cell_id).collect();
            while let Some(k) = todo.pop() {
                if k < 0x0100 || !rooms.insert(k) {
                    continue;
                }
                if let Some(c) = cell(k) {
                    todo.extend(
                        c.portals
                            .iter()
                            .filter(|p| p.other_cell_id <= 0xFFFF)
                            .filter_map(|p| u16::try_from(p.other_cell_id).ok()),
                    );
                }
            }
            let n = u32::try_from(rooms.len()).unwrap_or(u32::MAX);
            if rooms.len() > ENTRANCE_ROOMS && out.get(&lb).is_none_or(|(m, _)| *m < n) {
                out.insert(lb, (n, b.frame.origin));
            }
            reached.extend(rooms);
        }
        let apart = (info.num_cells as usize).saturating_sub(reached.len());
        if apart > 0 && !out.contains_key(&lb) {
            let n = u32::try_from(apart).unwrap_or(u32::MAX);
            out.insert(lb, (n, info.buildings[0].frame.origin));
        }
    }
    out
}

/// What the portals of `weenies` call the landblocks they lead into: for each landblock, the name
/// most of its portals have (the first by name among those as many).
#[must_use]
pub fn portal_names(weenies: &BTreeMap<u32, WeenieFacts>) -> BTreeMap<u16, String> {
    let mut seen: BTreeMap<u16, BTreeMap<String, u32>> = BTreeMap::new();
    for f in weenies.values() {
        let (Some(PORTAL), Some(cell), Some(name)) = (f.kind, f.destination, f.name.as_ref())
        else {
            continue;
        };
        if cell & 0xFFFF < 0x0100 {
            continue;
        }
        let Ok(lb) = u16::try_from(cell >> 16) else {
            continue;
        };
        *seen.entry(lb).or_default().entry(name.clone()).or_default() += 1;
    }
    seen.into_iter()
        .filter_map(|(lb, names)| {
            let most = names.values().copied().max()?;
            let name = names.into_iter().find(|(_, n)| *n == most)?.0;
            Some((lb, name))
        })
        .collect()
}

/// The file in a converted land's overlay folder naming its dungeons: a line per dungeon after a
/// header, `ac1_block ac2_block name`, tab-separated, the blocks in hex and the name empty where the
/// land gives none.
pub const LAND_NAMES: &str = "dungeon-names.tsv";

/// The names in a [`LAND_NAMES`] table, by the landblock each dungeon lies under in the world.
///
/// # Errors
/// A line that is not two blocks and a name.
pub fn parse_land_names(text: &str) -> Result<BTreeMap<u16, String>, String> {
    let mut out = BTreeMap::new();
    for (k, line) in text.lines().enumerate().skip(1) {
        if line.trim().is_empty() {
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        let [ac1, _ac2, name] = cols[..] else {
            return Err(format!("line {}: not three columns", k + 1));
        };
        let lb = u16::from_str_radix(ac1, 16)
            .map_err(|_| format!("line {}: {ac1:?} is not a landblock", k + 1))?;
        let name = name.trim();
        if !name.is_empty() {
            out.insert(lb, name.to_owned());
        }
    }
    Ok(out)
}

/// The names the world's land gives its dungeons ([`LAND_NAMES`]): the last of `imports` (overlay
/// folders) that holds the table. Empty when none does, as for a land not converted from AC2.
///
/// # Errors
/// A table that will not read or parse.
pub fn land_names(imports: &[PathBuf]) -> Result<BTreeMap<u16, String>, String> {
    for dir in imports.iter().rev() {
        let path = dir.join(LAND_NAMES);
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                return parse_land_names(&text).map_err(|e| format!("{}: {e}", path.display()))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("{}: {e}", path.display())),
        }
    }
    Ok(BTreeMap::new())
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the world editor's dungeon list)
    use super::*;

    #[test]
    fn a_converted_lands_table_names_its_dungeons_by_the_block_they_lie_under() {
        let text = "ac1_block\tac2_block\tname\nFE01\t00A7\tA Cave\nFE80\t0487\t\n";
        let names = parse_land_names(text).unwrap();
        // A dungeon the land gives no name has none here; the list calls it by its block.
        assert_eq!(names, BTreeMap::from([(0xFE01, "A Cave".to_owned())]));
        assert!(parse_land_names("ac1_block\tac2_block\tname\nFE01\tA Cave\n").is_err());
    }

    #[test]
    fn the_last_overlay_folder_with_a_table_names_the_dungeons() {
        let dir = std::env::temp_dir().join(format!("lyceum-land-names-{}", std::process::id()));
        let (first, second, none) = (dir.join("a"), dir.join("b"), dir.join("c"));
        for d in [&first, &second, &none] {
            std::fs::create_dir_all(d).unwrap();
        }
        let table = |name: &str| format!("ac1_block\tac2_block\tname\nFE01\t00A7\t{name}\n");
        std::fs::write(first.join(LAND_NAMES), table("Old")).unwrap();
        std::fs::write(second.join(LAND_NAMES), table("New")).unwrap();
        let got = land_names(&[first.clone(), second.clone(), none.clone()]).unwrap();
        assert_eq!(got.get(&0xFE01).map(String::as_str), Some("New"));
        assert!(land_names(std::slice::from_ref(&none)).unwrap().is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_dungeon_is_named_by_the_name_most_of_its_portals_have() {
        let portal = |name: &str, cell: u32| WeenieFacts {
            name: Some(name.into()),
            kind: Some(PORTAL),
            destination: Some(cell),
            ..WeenieFacts::default()
        };
        let mut w = BTreeMap::new();
        w.insert(1, portal("Glenden Wood Dungeon", 0x0163_0105));
        w.insert(2, portal("Glenden Wood Dungeon", 0x0163_0110));
        w.insert(3, portal("Old Portal", 0x0163_0100));
        // A portal to the outdoors names no dungeon; a creature names nothing.
        w.insert(4, portal("Town", 0xA9B4_0021));
        w.insert(
            5,
            WeenieFacts {
                name: Some("Drudge".into()),
                kind: Some(10),
                destination: Some(0x0200_0100),
                ..WeenieFacts::default()
            },
        );
        let names = portal_names(&w);
        assert_eq!(
            names.get(&0x0163).map(String::as_str),
            Some("Glenden Wood Dungeon")
        );
        assert_eq!(names.len(), 1);
    }

    /// A building whose doorway leads into more than a town's rooms stands over a dungeon: it is
    /// found with the rooms it reaches and where it stands; so are cells no building reaches,
    /// beside a hall; a cottage of two rooms is not.
    #[test]
    fn a_building_over_more_rooms_than_a_towns_is_a_dungeons_entrance() {
        use dereth_assets::world::{BuildInfo, BuildingPortal, CellPortal};
        use dereth_primitives::{Frame, Quat};
        let at = Frame::new(Vec3::new(40.0, 48.0, 29.0), Quat::IDENTITY);
        let room = |lb: u16, k: u16, to: &[u16]| EnvCell {
            id: DataId((u32::from(lb) << 16) | u32::from(k)),
            flags: 1,
            cell_id_repeat: (u32::from(lb) << 16) | u32::from(k),
            surfaces: vec![DataId(0x0800_0001)],
            environment: DataId(0x0D00_0010),
            cell_struct: 0,
            frame: at,
            portals: to
                .iter()
                .map(|t| CellPortal {
                    flags: 0,
                    polygon_id: 0,
                    other_cell_id: u32::from(*t),
                    other_portal_id: -1,
                })
                .collect(),
            visible_cells: Vec::new(),
            static_objects: Vec::new(),
            restriction_obj: None,
        };
        let block = |lb: u16, rooms: u16| {
            let mut records: BTreeMap<u32, Vec<u8>> = BTreeMap::new();
            for k in 0..rooms {
                let n = 0x0100 + k;
                let next: Vec<u16> = (k + 1 < rooms).then_some(n + 1).into_iter().collect();
                let c = room(lb, n, &next);
                records.insert(c.id.raw(), c.encode());
            }
            let info = LandblockInfo {
                id: DataId((u32::from(lb) << 16) | 0xFFFE),
                num_cells: u32::from(rooms),
                objects: Vec::new(),
                pack_mask: 0,
                buildings: vec![BuildInfo {
                    id: DataId(0x0100_2228),
                    frame: at,
                    num_leaves: 0,
                    portals: vec![BuildingPortal {
                        flags: 3,
                        other_cell_id: 0x0100,
                        other_portal_id: 0,
                        stab_list: vec![0x0100],
                    }],
                }],
                restrictions: None,
            };
            records.insert(info.id.raw(), info.encode());
            records
        };
        let mut all = block(0x558D, 42);
        all.extend(block(0xA9B4, 2));
        // A hall of one room, and a dungeon of three beside it no doorway reaches.
        let mut hall = block(0x6E8F, 1);
        for k in 0..3u16 {
            let c = room(0x6E8F, 0x0101 + k, &[]);
            hall.insert(c.id.raw(), c.encode());
        }
        let lid = 0x6E8F_FFFE;
        let mut info =
            LandblockInfo::decode_payload_for(EraId::Eor, DataId(lid), &hall[&lid]).unwrap();
        info.num_cells = 4;
        hall.insert(lid, info.encode());
        all.extend(hall);
        let found = under_entrances(&|id| all.get(&id.raw()).cloned(), EraId::Eor);
        assert_eq!(found.len(), 2, "the cottage is no entrance");
        assert_eq!(found[&0x558D], (42, at.origin));
        assert_eq!(
            found[&0x6E8F],
            (3, at.origin),
            "the rooms no building reaches"
        );
    }
}
