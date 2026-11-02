//! A retail data file written as the client writes one: the end-of-retail second cell file,
//! taken in memory, given the same seeded operations the client's own run was given.

use auberean_dat::disk::controller::OPEN_EXPANDABLE;
use auberean_dat::locate::DatName;
use auberean_dat::testing::{require_install, TARGET};
use auberean_dat::DatFile;

#[path = "../disk_support.rs"]
mod support;
use support::{apply, digest, fnv, operations, spec, store, OneFile};

/// Behaviour: data.disk.a-retail-file-is-written-as-the-client-writes-it
#[test]
fn the_retail_cell_file_takes_records_and_removals_as_the_clients_does() {
    let path = DatName::Cell(2).in_dir(&require_install(TARGET));
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let mut files = OneFile::new(&bytes);
    let mut s = store();
    assert!(s.init_file(&mut files, &mut spec(OPEN_EXPANDABLE, 0, 0)));
    let answers = apply(&mut s, &operations(31, 3000, 3000, 300, false));
    s.close();
    let answers: Vec<u8> = answers.into_iter().map(u8::from).collect();
    let b = files.bytes();
    let dat = DatFile::from_bytes(b.clone()).expect("the file still reads as a container");
    assert!(auberean_dat::container::sound(&dat.verify().unwrap()));
    assert_eq!(
        (digest(b), fnv(&answers)),
        (0x593f_49a6_3ad3_8784, 0x4a1e_fae6_d916_a148)
    );
}
