//! A spell aims at what its formula's target component names: war bolts at creatures, item
//! enchantments at enchantable items, portal ties at portals.
//! Fixture: the shipped spell table from the retail DATs.

use dereth_assets::tables::SpellTable;
use dereth_assets::Decode;
use dereth_client_model::magic::spell_target_type;
use dereth_primitives::{AssetSource, DataId};

/// `Frost Blast III`, a war spell aimed at creatures.
const FROST_BLAST_III: u32 = 107;
/// `Blade Bane I`, an item enchantment aimed at enchantable items.
const BLADE_BANE_I: u32 = 37;
/// `Primary Portal Tie`, aimed at portals.
const PRIMARY_PORTAL_TIE: u32 = 47;

fn spell_table() -> SpellTable {
    let dir = dereth_dat::testing::dat_dir();
    assert!(
        dereth_dat::testing::have_dats(),
        "the spell table lives in client_portal.dat: none at {} -- set $DERETH_TEST_DAT_DIR",
        dir.display()
    );
    let store = dereth_dat::RetailDatStore::open_dir(&dir).expect("the retail dat store opens");
    let sid = DataId(0x0E00_000E);
    SpellTable::decode_payload(sid, &store.read(sid).expect("the spell table"))
        .expect("the spell table decodes")
}

/// Behaviour: magic.targeting.a-spell-aims-at-what-its-formulas-target-component-names
#[test]
fn a_spells_target_type_comes_from_its_formulas_target_component() {
    let table = spell_table();
    for (id, name, want) in [
        (FROST_BLAST_III, "Frost Blast III", 0x10),
        (BLADE_BANE_I, "Blade Bane I", 0x0008_8B8F),
        (PRIMARY_PORTAL_TIE, "Primary Portal Tie", 0x1001_0000),
    ] {
        let base = table.spells.get(&id).expect("the spell is shipped");
        assert_eq!(base.name, name, "spell {id}");
        assert_eq!(
            spell_target_type(base),
            want,
            "{name} aims at {want:#x}, whatever its non-component target field says ({:#x})",
            base.non_component_target_type
        );
    }
}
