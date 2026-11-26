//! AC2's creatures in the land: every family a creature table names (`tools/art/ac2-creatures`),
//! made as Asheron's Call's own records ([`crate::ac2_creature`]), each in a block of ids of its own
//! that the land's other records never reach, so a world's weenies name the same ids whatever else
//! the land holds. `creatures.tsv` in the overlay folder says which ids each look's weenie names.

use std::fmt::Write as _;

use dereth_primitives::DataId;

use crate::ac2_creature::make::{self, Made};
use crate::ac2_creature::table::Table;
use crate::ac2_look::source::Ac2;
use crate::ac2_world::art::Records;

/// The table written into the overlay folder.
pub const TABLE: &str = "creatures.tsv";

/// The first family's first low id. The land numbers its other records from `0xC000` up.
const FIRST: u16 = 0xB000;

/// Low ids a family may take of each kind.
const BLOCK: u16 = 0x100;

/// Each family's table, in the order its blocks of ids follow.
const FAMILIES: [(&str, &str); 12] = [
    ("tyrant", include_str!("../../ac2-creatures/tyrant.toml")),
    ("beetle", include_str!("../../ac2-creatures/beetle.toml")),
    ("crone", include_str!("../../ac2-creatures/crone.toml")),
    (
        "darkenfowl",
        include_str!("../../ac2-creatures/darkenfowl.toml"),
    ),
    ("flayer", include_str!("../../ac2-creatures/flayer.toml")),
    (
        "gigurath",
        include_str!("../../ac2-creatures/gigurath.toml"),
    ),
    ("grutt", include_str!("../../ac2-creatures/grutt.toml")),
    ("mimbu", include_str!("../../ac2-creatures/mimbu.toml")),
    ("mucor", include_str!("../../ac2-creatures/mucor.toml")),
    ("scraven", include_str!("../../ac2-creatures/scraven.toml")),
    (
        "thornling",
        include_str!("../../ac2-creatures/thornling.toml"),
    ),
    ("vermin", include_str!("../../ac2-creatures/vermin.toml")),
];

/// A family made: its table and what was made of it.
pub struct Family {
    pub table: Table,
    pub made: Made,
}

/// The first low id of the `k`th family's block.
fn first_of(k: usize) -> u16 {
    // LINT-OK: twelve families.
    #[allow(clippy::cast_possible_truncation)]
    let k = k as u16;
    FIRST + k * BLOCK
}

/// Whether `id` lies in the block of ids starting at low id `first`.
fn in_block(id: DataId, first: u16) -> bool {
    let low = id.raw() & 0xFFFF;
    (u32::from(first)..u32::from(first) + u32::from(BLOCK)).contains(&low)
}

/// Make every family into `out`.
///
/// # Errors
/// A table that does not read, a creature that does not make, a family that outgrows its block
/// of ids, or a record whose id `out` holds already.
pub fn make(ac2: &Ac2, out: &mut Records) -> Result<Vec<Family>, String> {
    let mut families = Vec::new();
    for (k, (name, text)) in FAMILIES.iter().enumerate() {
        let table = Table::parse(text).map_err(|e| format!("creature {name}: {e}"))?;
        let first = first_of(k);
        let mut ids = make::Ids::from(first);
        let mut records = Records::default();
        let made = make::make(ac2, &table, &mut ids, &mut records)
            .map_err(|e| format!("creature {name}: {e}"))?;
        for (id, bytes) in records.0 {
            if !in_block(id, first) {
                return Err(format!(
                    "creature {name}: {id:?} lies beyond its ids ({first:04X} and {BLOCK:#X} on)"
                ));
            }
            if out.0.contains_key(&id) {
                return Err(format!(
                    "creature {name}: {id:?} is a record the land made already"
                ));
            }
            out.add(id, bytes);
        }
        families.push(Family { table, made });
    }
    Ok(families)
}

/// `creatures.tsv`: a line per look, the family's own first, with the ids its weenie names and the
/// texture swaps it lays on the body's parts (`part:old>new`, space separated; none for a
/// family's own look).
#[must_use]
pub fn table(families: &[Family]) -> String {
    let mut s = String::from(
        "look\tfamily\tlabel\tsetup\tmotion_table\tcombat_table\ttranslucency\ttexture_swaps\n",
    );
    for f in families {
        let m = &f.made;
        let mut line = |look: &str, translucency: Option<f64>, swaps: &[(u16, DataId, DataId)]| {
            let swaps: Vec<String> = swaps
                .iter()
                .map(|(part, old, new)| format!("{part}:{:08X}>{:08X}", old.raw(), new.raw()))
                .collect();
            let _ = writeln!(
                s,
                "{look}\t{}\t{}\t{:08X}\t{:08X}\t{:08X}\t{}\t{}",
                f.table.name,
                f.table.label,
                m.setup.raw(),
                m.motion_table.raw(),
                m.combat_table.raw(),
                translucency.map_or(String::new(), |t| format!("{t:.2}")),
                swaps.join(" ")
            );
        };
        line(&f.table.name, None, &[]);
        for l in &m.looks {
            line(&l.name, l.translucency, &l.parts);
        }
    }
    s
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the AC2 world's creatures)

    use super::*;

    /// Each family's ids start a block on from the last, and all of them end below the land's own.
    #[test]
    fn every_familys_block_lies_below_the_lands_own_ids_and_apart_from_the_others() {
        let blocks: Vec<u16> = (0..FAMILIES.len()).map(first_of).collect();
        assert_eq!(blocks[0], 0xB000);
        assert!(blocks.windows(2).all(|w| w[1] - w[0] == BLOCK));
        let last = u32::from(*blocks.last().unwrap()) + u32::from(BLOCK);
        assert!(last <= 0xC000, "the last block ends at {last:#X}");
    }

    /// An id is its family's when its low half lies in the family's block.
    #[test]
    fn an_id_is_in_its_familys_block_and_not_the_next() {
        assert!(in_block(DataId(0x0200_B1FF), 0xB100));
        assert!(!in_block(DataId(0x0200_B200), 0xB100));
        assert!(!in_block(DataId(0x0200_B0FF), 0xB100));
    }

    /// Each table the land carries reads, and is the family it is listed as.
    #[test]
    fn every_familys_table_reads_and_names_itself() {
        for (name, text) in FAMILIES {
            let t = Table::parse(text).unwrap();
            assert_eq!(t.name, name);
        }
    }
}
