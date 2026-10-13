//! What players are drawn with: the body setups of the character-creation table, the parts its
//! appearance choices swap in, and the parts the clothing tables swap onto bodies.

use dereth_assets::{CharGen, ClothingTable, Decode};
use dereth_dat::{DbType, RetailDatStore};
use dereth_primitives::DataId;

use crate::tally::{ModelId, Tally, Use};

/// The character-creation table's id.
pub const CHAR_GEN: DataId = DataId(0x0E00_0002);

/// What the character half found.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CharacterCounts {
    /// Heritage and sex pairs the creation table offers.
    pub bodies: u64,
    pub clothing_tables: u64,
    /// Clothing tables that would not decode.
    pub clothing_unread: u64,
}

/// Adds the bodies and part swaps of `char_gen` to `tally`: each heritage and sex's body setup is
/// one character instance, and each part an appearance choice swaps in (a hair style, a face) is
/// one instance per choice offering it.
pub fn fold_char_gen(tally: &mut Tally, char_gen: &CharGen) -> u64 {
    let mut bodies = 0;
    for group in char_gen.heritage_groups.values() {
        for sex in group.sexes.values() {
            bodies += 1;
            let who = format!("{} {}", group.name, sex.name);
            note(tally, sex.setup, Use::Character, &format!("{who} body"));
            let swaps = sex
                .base_objdesc
                .anim_part_changes
                .iter()
                .map(|&(part, id)| (part, id, "base"))
                .chain(sex.hair_styles.iter().flat_map(|h| {
                    h.objdesc
                        .anim_part_changes
                        .iter()
                        .map(|&(part, id)| (part, id, "hair style"))
                }));
            for (part, id, what) in swaps {
                note(
                    tally,
                    id,
                    Use::Character,
                    &format!("{who} {what}, part {part} of 0x{:08X}", sex.setup.0),
                );
            }
        }
    }
    bodies
}

/// Adds one clothing table's part swaps to `tally`: each graphics object it puts on a body part
/// is one clothing instance per table, noted with the body and part it replaces.
pub fn fold_clothing(tally: &mut Tally, table: &ClothingTable) {
    let mut seen = std::collections::BTreeSet::new();
    for (setup, effects) in &table.clothing_bases {
        for e in effects {
            if e.object_id.0 == 0 {
                continue;
            }
            let label = format!("part {} of 0x{:08X}", e.part_num, setup.0);
            let s = tally.stats_mut(ModelId::Object(e.object_id), Use::Clothing);
            *s.notes.entry(label).or_default() += 1;
            if seen.insert(e.object_id) {
                s.instances += 1;
            }
        }
    }
}

fn note(tally: &mut Tally, id: DataId, use_: Use, label: &str) {
    if id.0 == 0 {
        return;
    }
    let s = tally.stats_mut(ModelId::Object(id), use_);
    s.instances += 1;
    *s.notes.entry(label.to_owned()).or_default() += 1;
}

/// Reads the creation table and every clothing table from `store` into `tally`.
pub fn survey(store: &RetailDatStore, tally: &mut Tally) -> CharacterCounts {
    let mut counts = CharacterCounts::default();
    if let Ok(bytes) = store.read_typed(DbType::CharGen, CHAR_GEN) {
        if let Ok(cg) = CharGen::decode_payload_in(store.era_of(CHAR_GEN), CHAR_GEN, &bytes) {
            counts.bodies = fold_char_gen(tally, &cg);
        }
    }
    for id in store.ids_of(DbType::Clothing) {
        let table = store
            .read_typed(DbType::Clothing, id)
            .ok()
            .and_then(|b| ClothingTable::decode_payload_in(store.era_of(id), id, &b).ok());
        match table {
            Some(t) => {
                counts.clothing_tables += 1;
                fold_clothing(tally, &t);
            }
            None => counts.clothing_unread += 1,
        }
    }
    counts
}
