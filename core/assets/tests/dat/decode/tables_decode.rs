//! Behaviour: none (checks formats, geometry, numeric contracts or repository tools)
//! Every gameplay and UI table decodes completely; spell components and string encodings preserve their contracts.
//! Fixture: the shipped retail DAT records and recorded inputs.

use std::collections::BTreeMap;

use dereth_assets::tables::{spell_component_key, spell_hash};
use dereth_assets::ui::{LayoutDesc, PropertyAsset, PropertyTypes};
use dereth_assets::{decode_any, Decode, MasterProperty};
use dereth_dat::{divine_type, DbType, RetailDatStore};
use dereth_primitives::DataId;

fn store() -> RetailDatStore {
    dereth_dat::testing::open_store_or_fail()
}

/// Every gameplay table decodes with the cursor on the end.
#[test]
fn every_gameplay_table_decodes_with_the_cursor_on_the_end() {
    let s = store();
    // `docs/formats/02-file-ids-and-types.md` sections 5.2 and 6.
    let expect: &[DbType] = &[
        DbType::SpellTable,
        DbType::SpellComponentTable,
        DbType::SkillTable,
        DbType::XpTable,
        DbType::Attribute2ndTable,
        DbType::CharGen,
        DbType::ChatPoseTable,
        DbType::ContractTable,
        DbType::ObjectHierarchy,
        DbType::TabooTable,
        DbType::NameFilterTable,
        DbType::BadData,
        DbType::QualityFilter,
        DbType::CombatTable,
        DbType::EnumMapper,
        DbType::DidMapper,
        DbType::DualDidMapper,
    ];
    let mut total = 0usize;
    for kind in expect {
        let ids = s.ids_of(*kind);
        assert!(!ids.is_empty(), "the input exercises {kind:?}");
        for id in ids {
            let bytes = s.read_portal(id).unwrap();
            decode_any(*kind, id, &bytes)
                .unwrap_or_else(|e| panic!("{kind:?} {id} ({} bytes): {e}", bytes.len()));
            total += 1;
        }
    }
    assert_eq!(
        total,
        expect
            .iter()
            .map(|kind| s.ids_of(*kind).len())
            .sum::<usize>()
    );
}

/// Every singleton table resolves to its declared type and decodes.
#[test]
fn every_singleton_table_resolves_to_its_declared_type_and_decodes() {
    let s = store();
    let ids: Vec<DataId> = s
        .portal()
        .iter_ids()
        .filter(|i| i.raw() >> 24 == 0x0E)
        .collect();
    assert!(!ids.is_empty(), "singleton tables are exercised");
    let mut kinds = Vec::new();
    for id in ids {
        let kind = divine_type(id).unwrap_or_else(|| panic!("{id} divines to nothing"));
        let bytes = s.read_portal(id).unwrap();
        decode_any(kind, id, &bytes).unwrap_or_else(|e| panic!("{kind:?} {id}: {e}"));
        kinds.push(kind);
    }
    assert_eq!(
        kinds,
        vec![
            DbType::CharGen,
            DbType::Attribute2ndTable,
            DbType::SkillTable,
            DbType::ChatPoseTable,
            DbType::ObjectHierarchy,
            DbType::SpellTable,
            DbType::SpellComponentTable,
            DbType::XpTable,
            DbType::BadData,
            DbType::ContractTable,
            DbType::TabooTable,
            DbType::NameFilterTable,
            DbType::QualityFilter,
            DbType::QualityFilter,
        ]
    );
}

/// The spell table de obfuscates and decrypts.
#[test]
fn the_spell_table_de_obfuscates_and_decrypts() {
    let s = store();
    let id = DataId(0x0E00_000E);
    let table =
        dereth_assets::tables::SpellTable::decode_payload(id, &s.read_portal(id).unwrap()).unwrap();
    assert!(
        !table.spells.is_empty(),
        "only {} spells",
        table.spells.len()
    );

    let cid = DataId(0x0E00_000F);
    let comps = dereth_assets::tables::SpellComponentTable::decode_payload(
        cid,
        &s.read_portal(cid).unwrap(),
    )
    .unwrap();
    assert!(!comps.components.is_empty());

    // Every spell's name must be printable text after the swap, and every decrypted component id
    // must be a key of the component table. That is a far stronger check than "it parsed".
    let mut checked = 0usize;
    for sp in table.spells.values() {
        assert!(
            sp.name.chars().all(|ch| ch == ' ' || !ch.is_control()),
            "spell name is not text: {:?}",
            sp.name
        );
        assert!(!sp.name.is_empty());
        for comp in &sp.comps {
            assert!(
                comps.components.contains_key(comp),
                "spell {:?} decrypts to component {comp} which is not in the table",
                sp.name
            );
            checked += 1;
        }
    }
    assert!(checked > 0, "only {checked} component references checked");

    assert_eq!(
        checked,
        table
            .spells
            .values()
            .map(|spell| spell.comps.len())
            .sum::<usize>()
    );

    // And a spot check of the hash itself against a name that is in the table.
    let any = table.spells.values().next().unwrap();
    assert_eq!(
        any.comp_key,
        spell_component_key(any.name.as_bytes(), any.description.as_bytes())
    );
    assert_ne!(spell_hash(b"Strength Other I"), 0);
}

/// The shipped spell-set table: its header is one dword, 139 sets in the low 24 bits and a
/// bucket-size index of 1 in the high 8, and each set's tiers are a plain list under a dword count.
#[test]
fn the_spell_set_table_is_a_24_bit_hash_of_plain_tier_lists() {
    let s = store();
    let id = DataId(0x0E00_000E);
    let table =
        dereth_assets::tables::SpellTable::decode_payload(id, &s.read_portal(id).unwrap()).unwrap();
    assert_eq!(table.spellsets.len(), 139);
    assert_eq!(table.spellset_bucket_index, 1);
    assert_eq!(table.spellsets[&138].tiers.len(), 50);
    assert!(table.spellsets.values().all(|set| !set.tiers.is_empty()));
}

/// Every local dat file decodes.
#[test]
fn every_local_dat_file_decodes() {
    let s = store();
    let types = master_property_types(&s);
    let mut counts: BTreeMap<DbType, usize> = BTreeMap::new();
    for id in s.local().iter_ids() {
        if id == dereth_dat::ITERATION_LIST {
            continue;
        }
        let kind = divine_type(id).unwrap_or_else(|| panic!("{id} divines to nothing"));
        let bytes = s.local().read(id).unwrap();
        match kind {
            DbType::UiLayout => {
                LayoutDesc::decode_payload(id, &bytes, &types)
                    .unwrap_or_else(|e| panic!("layout {id} ({} bytes): {e}", bytes.len()));
            }
            _ => {
                decode_any(kind, id, &bytes)
                    .unwrap_or_else(|e| panic!("{kind:?} {id} ({} bytes): {e}", bytes.len()));
            }
        }
        *counts.entry(kind).or_insert(0) += 1;
    }
    assert!(
        counts[&DbType::UiLayout] > 0,
        "the input exercises UiLayout"
    );
    assert!(
        counts[&DbType::StringTable] > 0,
        "the input exercises StringTable"
    );
    assert!(
        counts[&DbType::StringState] > 0,
        "the input exercises StringState"
    );
    assert_eq!(
        counts.values().sum::<usize>(),
        s.local()
            .iter_ids()
            .filter(|id| *id != dereth_dat::ITERATION_LIST)
            .count()
    );
}

fn master_property_types(s: &RetailDatStore) -> PropertyTypes {
    let id = DataId(0x3900_0001);
    let mp = MasterProperty::decode_payload(id, &s.read_portal(id).unwrap()).unwrap();
    mp.property_types()
}

/// Every portal ui table decodes.
#[test]
fn every_portal_ui_table_decodes() {
    let s = store();
    let types = master_property_types(&s);
    for kind in [
        DbType::Font,
        DbType::StringType,
        DbType::ActionMap,
        DbType::Keymap,
        DbType::MasterProperty,
    ] {
        let ids = s.ids_of(kind);
        assert!(!ids.is_empty(), "the input exercises {kind:?}");
        for id in ids {
            let bytes = s.read_portal(id).unwrap();
            decode_any(kind, id, &bytes)
                .unwrap_or_else(|e| panic!("{kind:?} {id} ({} bytes): {e}", bytes.len()));
        }
    }
    let ids = s.ids_of(DbType::DbProperties);
    assert!(!ids.is_empty(), "database properties are exercised");
    for id in ids {
        let bytes = s.read_portal(id).unwrap();
        PropertyAsset::decode_payload(id, &bytes, &types)
            .unwrap_or_else(|e| panic!("dbproperty {id} ({} bytes): {e}", bytes.len()));
    }
}

/// The two string encodings are kept apart.
#[test]
fn the_two_string_encodings_are_kept_apart() {
    let s = store();
    let st_id = s.ids_of(DbType::StringTable)[0];
    let st =
        dereth_assets::StringTable::decode_payload(st_id, &s.local().read(st_id).unwrap()).unwrap();
    assert!(!st.strings.is_empty());
    for (_, e) in &st.strings {
        // `table` is INVALID_DID and `has_var_names` 0 in every retail entry.
        assert_eq!(e.table, DataId(0));
        assert_eq!(e.has_var_names, 0);
        assert!(!e.strings.is_empty());
    }

    let s31 = s.ids_of(DbType::StringType)[0];
    let ls =
        dereth_assets::LanguageString::decode_payload(s31, &s.read_portal(s31).unwrap()).unwrap();
    assert!(!ls.text.is_empty());
    // cp1252 is one byte per character, so the decoded length is the byte count.
    assert!(ls.text.chars().count() <= s.read_portal(s31).unwrap().len());

    // The language-info record has no DataID header and is 166 bytes.
    let li_id = DataId(0x4100_0000);
    let raw = s.local().read(li_id).unwrap();
    assert_eq!(raw.len(), 166);
    let li = dereth_assets::LanguageInfo::decode_payload(li_id, &raw).unwrap();
    assert_eq!(li.version, 1);
    assert_eq!(li.base, 10);
    // The English numeral list carries the ten digits plus seven further code points.
    assert!(
        li.numerals.chars().count() >= 10,
        "numerals: {:?}",
        li.numerals
    );
    assert!(li.numerals.starts_with('0'));
}
