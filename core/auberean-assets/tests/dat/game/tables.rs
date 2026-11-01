//! Tables of ids and versions.

use auberean_assets::tables::PairMap;
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
