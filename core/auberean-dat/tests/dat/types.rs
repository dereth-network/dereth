//! Every record of the target install has a type, and lives in the file its type says.

use auberean_dat::locate::{install_dats, DatName};
use auberean_dat::testing::require_install;
use auberean_dat::testing::TARGET;
use auberean_dat::{cell_type, type_of_id, DatFile, DatKind, RecordType};

/// Behaviour: data.types.every-record-has-a-type
#[test]
fn every_record_of_the_target_install_has_a_type_in_the_file_it_lives_in() {
    let d = require_install(TARGET);
    for (name, path) in install_dats(&d).unwrap() {
        let dat = DatFile::open(&path).unwrap();
        for e in dat.entries() {
            let ty = if matches!(name, DatName::Cell(_)) {
                cell_type(e.id)
            } else {
                type_of_id(e.id)
            };
            let ty = ty.unwrap_or_else(|| panic!("{}: {:08X} has no type", name.file_name(), e.id));
            if ty == RecordType::ITERATION_LIST || matches!(name, DatName::Cell(_)) {
                continue;
            }
            let want = match name {
                DatName::Local(_) => DatKind::Local,
                _ => DatKind::Portal,
            };
            assert_eq!(
                ty.info().unwrap().dat,
                want,
                "{}: {:08X} is {ty}",
                name.file_name(),
                e.id
            );
        }
    }
}
