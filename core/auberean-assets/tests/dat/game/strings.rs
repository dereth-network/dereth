//! String tables and string states in every language file, and enum maps.

use auberean_assets::decode;
use auberean_assets::strings::StringTable;
use auberean_assets::verify::type_in;
use auberean_dat::RecordType;

use crate::common::{assert_every_record_decodes, installs};

/// Behaviour: data.string-table.every-record-decodes
#[test]
fn every_string_table_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::STRING_TABLE]);
}

/// Behaviour: data.enum-map.every-record-decodes
#[test]
fn every_enum_map_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::ENUM_MAP]);
}

/// Behaviour: data.string-state.every-record-decodes
#[test]
fn every_string_state_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::STRING_STATE]);
}

/// Every string table of every language packs to as many bytes as it holds, and every entry
/// that stores its variables' names is one whose names are worth storing (an item name's), so a
/// table written back keeps them.
///
/// Behaviour: data.string-table.every-record-packs-to-its-size
#[test]
fn every_string_table_packs_to_its_own_size_and_stores_names_only_for_item_names() {
    let (mut tables, mut named) = (0, 0);
    for i in installs() {
        for (_, dat) in &i.dats {
            for e in dat.entries() {
                if type_in(dat, e.id) != Some(RecordType::STRING_TABLE) {
                    continue;
                }
                let bytes = dat.read_entry(e).expect("read the record");
                let t: StringTable = decode(e.id, &bytes, &i.ctx).expect("decodes");
                assert_eq!(t.pack_size(), bytes.len(), "{:08X}", e.id);
                for s in t.entries.iter().filter(|s| !s.variable_names.is_empty()) {
                    assert!(s.names_worth_packing(), "{:08X} {:08X}", e.id, s.token);
                    named += 1;
                }
                tables += 1;
            }
        }
    }
    assert!(
        tables > 10_000 && named > 0,
        "{tables} tables, {named} named entries"
    );
}
