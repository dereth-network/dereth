//! The data files' motion commands in the final numbering.
//!
//! Three kinds of record name motion commands: motion tables (their keys), combat manoeuvre
//! tables (a style and a motion per manoeuvre) and the spell component table (each component's
//! gesture). Files from before June 2015 number those commands in an older table
//! ([`CommandNumbering`](dereth_animation::command::CommandNumbering)); everything that reads
//! them -- the motion runtime, the server's attack and casting logic -- works in the final
//! numbering. So the numbering of a set of files is told
//! once, from its human motion table, and every such record is translated into the final
//! numbering as it is loaded. A command the final table has no row for is dropped with its entry.

pub use dereth_animation::command::CommandNumbering;
use dereth_assets::tables::{CombatManeuverTable, SpellComponentTable};
use dereth_assets::{Decode, MotionTable};
use dereth_dat::{DbType, RetailDatStore};
use dereth_primitives::DataId;

/// The human motion table, which every set of files has and which plays the commands each
/// numbering moved; the numbering is told from it.
pub const HUMAN_MOTION_TABLE: DataId = DataId(0x0900_0001);

/// The full command ids a motion table carries: its default style, the style defaults' styles and
/// motions, and the link destinations. The other keys are packed and carry no class byte.
pub fn full_ids(t: &MotionTable) -> impl Iterator<Item = u32> + Clone + '_ {
    std::iter::once(t.default_style)
        .chain(t.style_defaults.iter().flat_map(|(s, m)| [*s, *m]))
        .chain(t.links.values().flatten().map(|m| m.key))
}

/// The numbering the files holding `human` use. See [`CommandNumbering::infer`].
#[must_use]
pub fn of_human_table(human: &MotionTable) -> CommandNumbering {
    CommandNumbering::infer(full_ids(human))
}

/// The numbering of `store`'s world files: told from their human motion table, and the final
/// numbering when they have none.
#[must_use]
pub fn of_store(store: &RetailDatStore) -> CommandNumbering {
    store
        .read_typed(DbType::MTable, HUMAN_MOTION_TABLE)
        .ok()
        .and_then(|b| {
            MotionTable::decode_payload_in(store.era_of(HUMAN_MOTION_TABLE), HUMAN_MOTION_TABLE, &b)
                .ok()
        })
        .map_or(CommandNumbering::Final, |t| of_human_table(&t))
}

/// The numberings of a store's files: the world files', and beside an older world the later
/// files' (which answer the records the world files lack).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StoreNumbering {
    /// The world files', which is also the wire's: the client of the world's day numbered its
    /// messages as its files did.
    pub world: CommandNumbering,
    /// The later files' beside an older world; `None` without them.
    pub later: Option<CommandNumbering>,
}

impl StoreNumbering {
    /// Tell both numberings from the files' human motion tables.
    #[must_use]
    pub fn of(store: &RetailDatStore) -> Self {
        Self {
            world: of_store(store),
            later: store.later_files().map(|l| of_store(&l)),
        }
    }

    /// The numbering of the portal record `id` as `store` reads it: the later files' when it
    /// comes from them, the world's otherwise.
    #[must_use]
    pub fn of_record(&self, store: &RetailDatStore, id: DataId) -> CommandNumbering {
        match self.later {
            Some(later) if !store.portal().contains(id) => later,
            _ => self.world,
        }
    }
}

/// A packed `(style, motion)` key's two indices in the final numbering; `None` when either has
/// no final row. A zero half stays zero.
fn packed(n: CommandNumbering, key: u32) -> Option<u32> {
    let half = |i: u32| -> Option<u32> {
        if i == 0 {
            return Some(0);
        }
        n.to_final_index(u16::try_from(i).ok()?).map(u32::from)
    };
    Some(half(key >> 16)? << 16 | half(key & 0xFFFF)?)
}

/// A full id in the final numbering, or `None` when it has no final row.
fn full(n: CommandNumbering, id: u32) -> Option<u32> {
    if id == 0 {
        return Some(0);
    }
    n.to_final(id).map(|c| c.0)
}

/// `t` with every key in the final numbering. Nothing changes under [`CommandNumbering::Final`].
pub fn motion_table(t: &mut MotionTable, n: CommandNumbering) {
    if n == CommandNumbering::Final {
        return;
    }
    t.default_style = full(n, t.default_style).unwrap_or(0);
    t.style_defaults = std::mem::take(&mut t.style_defaults)
        .into_iter()
        .filter_map(|(s, m)| Some((full(n, s)?, full(n, m)?)))
        .collect();
    for list in [&mut t.cycles, &mut t.modifiers] {
        list.retain_mut(|m| match packed(n, m.key) {
            Some(k) => {
                m.key = k;
                true
            }
            None => false,
        });
    }
    t.links = std::mem::take(&mut t.links)
        .into_iter()
        .filter_map(|(outer, mut group)| {
            let outer = packed(n, outer)?;
            group.retain_mut(|m| match full(n, m.key) {
                Some(k) => {
                    m.key = k;
                    true
                }
                None => false,
            });
            Some((outer, group))
        })
        .collect();
}

/// `t` with every manoeuvre's style and motion in the final numbering.
pub fn combat_maneuver_table(t: &mut CombatManeuverTable, n: CommandNumbering) {
    if n == CommandNumbering::Final {
        return;
    }
    t.maneuvers
        .retain_mut(|m| match (full(n, m.style), full(n, m.motion)) {
            (Some(s), Some(c)) => {
                m.style = s;
                m.motion = c;
                true
            }
            _ => false,
        });
}

/// `t` with every component's gesture in the final numbering. A gesture with no final row
/// becomes 0, no gesture: the component itself stays.
pub fn spell_component_table(t: &mut SpellComponentTable, n: CommandNumbering) {
    if n == CommandNumbering::Final {
        return;
    }
    for c in t.components.values_mut() {
        c.gesture = full(n, c.gesture).unwrap_or(0);
    }
}

/// The wire index the numbering `n` gives the final command `id`; `None` when `n` has no such
/// command. Under the final numbering it is the id's low half, which is its index.
#[must_use]
#[allow(clippy::cast_possible_truncation)] // the low half of the id is the final index
pub fn wire_index(n: CommandNumbering, id: u32) -> Option<u16> {
    if n == CommandNumbering::Final {
        return Some(id as u16);
    }
    n.command_to_wire(dereth_animation::command::MotionCommand(id))
}

/// The final index of the wire index `i` in the numbering `n`; `i` itself when `n` has no final
/// counterpart for it (the final numbering always answers `i`).
#[must_use]
pub fn final_index_from_wire(n: CommandNumbering, i: u16) -> u16 {
    if n == CommandNumbering::Final {
        return i;
    }
    n.to_final_index(i).unwrap_or(i)
}

/// The final id of the full id `id` in the numbering `n`; `id` itself when `n` has no final
/// command for it.
#[must_use]
pub fn final_id(n: CommandNumbering, id: u32) -> u32 {
    if n == CommandNumbering::Final {
        return id;
    }
    n.to_final(id).map_or(id, |c| c.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_assets::motion::MotionData;
    use std::collections::BTreeMap;

    fn data(key: u32) -> MotionData {
        MotionData {
            key,
            bitfield: 0,
            flags: 0,
            anims: Vec::new(),
            velocity: None,
            omega: None,
        }
    }

    /// A table keyed in the older numbering comes out keyed in the final one: the packed keys'
    /// two halves, the full ids, and the entering-a-style group whose motion half is zero.
    #[test]
    fn an_older_table_is_keyed_in_the_final_numbering() {
        let mut t = MotionTable {
            id: HUMAN_MOTION_TABLE,
            default_style: 0x8000_003D,
            style_defaults: BTreeMap::from([(0x8000_0138, 0x4100_0003)]),
            cycles: vec![data(0x003D_013A), data(0x0138_0003)],
            modifiers: vec![data(0x0000_0115)],
            links: BTreeMap::from([
                (0x003D_0003, vec![data(0x1000_011B), data(0x4300_013A)]),
                (0x0138_0000, vec![data(0x4100_0003)]),
            ]),
        };
        assert_eq!(of_human_table(&t), CommandNumbering::Before2015);
        motion_table(&mut t, CommandNumbering::Before2015);
        assert_eq!(t.default_style, 0x8000_003D);
        assert_eq!(
            t.style_defaults,
            BTreeMap::from([(0x8000_013B, 0x4100_0003)])
        );
        let keys = |v: &[MotionData]| v.iter().map(|m| m.key).collect::<Vec<_>>();
        assert_eq!(keys(&t.cycles), [0x003D_013D, 0x013B_0003]);
        assert_eq!(keys(&t.modifiers), [0x0000_0118]);
        assert_eq!(keys(&t.links[&0x003D_0003]), [0x1000_011E, 0x4300_013D]);
        assert_eq!(keys(&t.links[&0x013B_0000]), [0x4100_0003]);
        assert_eq!(of_human_table(&t), CommandNumbering::Final);
    }

    /// A command the final table has no row for leaves with its entry, and the final numbering
    /// leaves a table alone.
    #[test]
    fn a_command_with_no_final_row_is_dropped() {
        let base = MotionTable {
            id: HUMAN_MOTION_TABLE,
            default_style: 0x8000_003D,
            style_defaults: BTreeMap::new(),
            cycles: vec![data(0x003D_0140), data(0x003D_0003)],
            modifiers: Vec::new(),
            links: BTreeMap::from([(0x003D_0003, vec![data(0x1300_0152), data(0x1000_011B)])]),
        };
        let mut t = base.clone();
        motion_table(&mut t, CommandNumbering::January2002);
        assert_eq!(t.cycles.len(), 1, "PointUpState has no later row");
        assert_eq!(t.links[&0x003D_0003].len(), 1, "nor has PointUp");
        assert_eq!(t.links[&0x003D_0003][0].key, 0x1000_011E);
        let mut same = base.clone();
        motion_table(&mut same, CommandNumbering::Final);
        assert_eq!(same, base);
    }
}
