//! Tables of ids and versions.

use auberean_assets::names::NameTable;
use auberean_assets::tables::{FileIdTable, PairMap};
use auberean_dat::locate::DatName;
use auberean_dat::types::type_of_file_name;
use auberean_dat::{Reader, RecordType};

use crate::common::{assert_every_record_decodes, installs};

/// Behaviour: data.iteration-list.every-record-decodes
#[test]
fn every_iteration_list_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::ITERATION_LIST]);
}

/// Behaviour: data.id-map.every-record-decodes
#[test]
fn every_id_map_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::ID_MAP]);
}

/// Behaviour: data.file-id-table.every-record-decodes
#[test]
fn the_file_id_table_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::FILE_ID_TABLE]);
}

/// Read as the client reads it, the file-id table names every one of its music and script library
/// files: each name finds its id, whatever its folders and letter case, and each id names its file
/// (no two names share an id, and every name's extension is its type's).
///
/// Behaviour: data.file-id-table.names-find-their-ids
#[test]
fn every_name_in_the_file_id_table_finds_its_id() {
    for i in installs() {
        let portal = &i
            .dats
            .iter()
            .find(|(n, _)| *n == DatName::Portal)
            .unwrap()
            .1;
        let bytes = portal.read(0x0E00_0003).expect("the file-id table");
        let record = auberean_assets::decode::<FileIdTable>(0x0E00_0003, &bytes, &i.ctx).unwrap();
        let mut t = NameTable::from_file_ids(&record);
        let mut n = 0;
        for ty in &record.types {
            for e in &ty.entries {
                assert_eq!(
                    type_of_file_name(&e.name).map(|t| t.0),
                    Some(ty.record_type)
                );
                assert_eq!(t.id_by_name(&e.name, ty.record_type), e.id.0, "{}", e.name);
                let upper = format!("Some\\Folder/{}", e.name.to_ascii_uppercase());
                assert_eq!(t.id_by_name(&upper, ty.record_type), e.id.0, "{upper}");
                let back = t.name_by_id(e.id.0, ty.record_type).unwrap();
                assert_eq!(back, e.name.to_ascii_lowercase());
                n += 1;
            }
        }
        assert_eq!(t.len(), n, "{}", i.set);
        assert!(n > 1000, "{}: {n} names", i.set);
        assert_eq!(
            t.id_by_name("melodicband.sgt", RecordType::MUSIC.0),
            0x2400_0007
        );
        assert_eq!(
            t.id_by_name("server.wlib", RecordType::SCRIPT_LIBRARY.0),
            0x5600_0000
        );
    }
}

/// The client's own reader for the iteration list's type wants a count of pairs; every file's
/// iteration list counts versions instead, so none of them reads that way.
///
/// Behaviour: data.iteration-list.not-pairs
#[test]
fn no_iteration_list_reads_as_the_clients_pairs() {
    let mut n = 0;
    for i in installs() {
        for (name, dat) in &i.dats {
            let bytes = dat
                .read(auberean_dat::container::ITERATION_LIST)
                .expect("every file holds its iteration list");
            assert!(PairMap::read(&mut Reader::new(&bytes)).is_err(), "{name:?}");
            n += 1;
        }
    }
    assert!(n >= 6, "{n} files");
}
