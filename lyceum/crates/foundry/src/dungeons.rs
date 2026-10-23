//! The world's dungeons: the landblocks whose indoor cells are a dungeon's rather than a town's
//! rooms, and what the portals that lead into each call it.

use std::collections::BTreeMap;

use dereth_assets::world::{EnvCell, LandblockInfo};
use dereth_assets::Decode;
use dereth_primitives::{DataId, EraId};

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

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the world editor's dungeon list)
    use super::*;

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
}
