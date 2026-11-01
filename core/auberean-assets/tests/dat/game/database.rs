//! The object database over the end-of-retail files: records found through the master id map's
//! enumerations, and enumerations' names.

use std::sync::Arc;

use auberean_assets::property::{
    PropertyList, ValueKind, GROUP_NAMES, MASTER_PROPERTY_MAPPING, MASTER_PROPERTY_VALUE,
    VALUE_TYPE_NAMES,
};
use auberean_assets::strings::StringState;
use auberean_assets::verify::type_in;
use auberean_assets::Database;
use auberean_dat::{DatFile, RecordType};

use crate::common::installs;

/// The database over a checked set's files, in the order the client opens them (the portal
/// first, whose header names the master id map).
pub(crate) fn database() -> Database {
    let i = &installs()[0];
    let mut files: Vec<Arc<DatFile>> = Vec::new();
    for (name, path) in
        auberean_dat::locate::install_dats(&auberean_dat::testing::require_install(i.set))
            .expect("list the install")
    {
        let f = Arc::new(DatFile::open(&path).expect("open"));
        if name == auberean_dat::locate::DatName::Portal {
            files.insert(0, f);
        } else {
            files.push(f);
        }
    }
    Database::new(files)
}

/// Behaviour: data.database.property-list-by-enumeration
#[test]
fn the_property_list_is_the_record_its_enumeration_names() {
    let db = database();
    let i = &installs()[0];
    assert_eq!(
        db.data_id_by_enum(MASTER_PROPERTY_VALUE, MASTER_PROPERTY_MAPPING),
        0x3400_0000
    );
    let list: &PropertyList = db.property_list().expect("the property list");
    assert_eq!(Some(list), i.ctx.properties());
    // The language's rules are named the same way, through the string tables' mapping.
    let state = db.get_by_enum::<StringState>(0x4000_0001, 0x4000_0004);
    assert!(state.is_some());
}

/// Behaviour: data.database.property-names-type-and-group-names-tools-only
#[test]
fn property_names_resolve_but_value_type_and_group_names_are_not_in_the_files() {
    let db = database();
    let list = db.property_list().expect("the property list");
    for (id, _) in list.iter() {
        let name = list.name_of(id).expect("a property name");
        assert_eq!(list.id_of(name), Some(id), "{name}");
        assert_eq!(list.id_of(&name.to_ascii_uppercase()), Some(id), "{name}");
    }
    // The enumerations naming the value types and the groups map to records in the enum maps'
    // tools half, which no client file holds: the game cannot name a type or a group.
    for e in [VALUE_TYPE_NAMES, GROUP_NAMES] {
        let did = db.data_id_by_enum(e, auberean_assets::database::ENUM_MAP_MAPPING);
        assert_ne!(did, 0, "{e:#x}");
        assert!(db.read(did).is_none(), "{e:#x} names {did:08X}");
    }
    for (_, d) in list.iter() {
        assert_eq!(db.enumeration_name(VALUE_TYPE_NAMES, d.value_kind), None);
        assert_eq!(db.enumeration_name(GROUP_NAMES, d.group), None);
    }
    for k in ValueKind::ALL {
        assert_eq!(db.enumeration_name(VALUE_TYPE_NAMES, k.number()), None);
    }
}

/// Behaviour: data.database.enum-map-extends-its-base
#[test]
fn an_enum_map_holds_its_own_names_then_its_base_maps() {
    let db = database();
    let i = &installs()[0];
    let mut based = 0;
    for (_, dat) in &i.dats {
        for e in dat.entries() {
            if type_in(dat, e.id) != Some(RecordType::ENUM_MAP) {
                continue;
            }
            let m = db.enum_map(e.id).expect("the enum map");
            let entries = db.enum_map_entries(e.id);
            for (v, n) in &entries {
                assert!(db.enum_map_name(e.id, *v).is_some(), "{:08X} {v:#x}", e.id);
                assert!(db.enum_map_value(e.id, n).is_some(), "{:08X} {n}", e.id);
            }
            if m.base != 0 {
                based += 1;
                let base = db.enum_map_entries(m.base);
                assert_eq!(entries.len(), m.names.len() + base.len(), "{:08X}", e.id);
                assert_eq!(&entries[m.names.len()..], &base[..], "{:08X}", e.id);
            }
        }
    }
    assert!(based > 0, "no enum map with a base");
}

/// Behaviour: data.database.no-word-filter-at-end-of-retail
#[test]
fn the_end_of_retail_files_name_no_word_filter() {
    let db = database();
    // The word filter is found by value 0x40000005 of the property list's mapping.
    assert_eq!(db.data_id_by_enum(0x4000_0005, MASTER_PROPERTY_MAPPING), 0);
    assert!(db
        .get::<auberean_assets::script::WordFilter>(0x0EBA_DA55)
        .is_none());
}
