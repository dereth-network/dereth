//! Behaviour: none (tooling: the record checks, against faults planted in a small set of files)

use dereth_assets::world::LandblockInfo;
use dereth_assets::SurfaceTexture;
use dereth_dat::{DatWriter, ModernDat};
use dereth_pack::dats::DatSet;
use dereth_pack::validate::{validate, Scope};
use dereth_primitives::DataId;

use super::common::{self, Fixture};

#[test]
fn the_checks_pass_on_the_files_as_they_are() {
    let f = Fixture::new("pack-checks-clean");
    let report = validate(&DatSet::open(&f.dats()).unwrap(), &Scope::All);
    assert!(report.problems.is_empty(), "{:?}", report.problems);
    // Four landblocks, one statics record, a surface, an image texture and a picture.
    assert_eq!(report.checked, 8);
}

#[test]
fn the_checks_find_each_fault_planted_in_the_files() {
    let f = Fixture::new("pack-checks-faults");
    let cell = ModernDat::Cell.in_dir(&f.dats());
    let mut w = DatWriter::open(&cell).unwrap();
    // A statics record that counts two cells its landblock does not hold.
    let mut info = common::home_statics();
    info.num_cells = 2;
    w.save(info.id, &info.encode(), 2, 1, 0).unwrap();
    // A landblock that says it has a statics record, and has none.
    w.save(
        DataId(0x0202_FFFF),
        &common::terrain(0x0202, true).encode(),
        2,
        1,
        0,
    )
    .unwrap();
    // A statics record whose terrain says it has none.
    let other = LandblockInfo {
        id: DataId(0x0201_FFFE),
        ..common::home_statics()
    };
    w.save(other.id, &other.encode(), 2, 1, 0).unwrap();
    drop(w);
    let mut w = DatWriter::open(&ModernDat::Portal.in_dir(&f.dats())).unwrap();
    // An image texture that names a picture the files do not hold.
    let texture = SurfaceTexture {
        id: DataId(common::TEXTURE),
        data_category: 0,
        unknown_byte: 2,
        source_levels: vec![DataId(0x0600_0099)],
    };
    w.save(texture.id, &texture.encode(), 2, 1, 0).unwrap();
    drop(w);
    let report = validate(&DatSet::open(&f.dats()).unwrap(), &Scope::All);
    let found = |s: &str| report.problems.iter().any(|p| p.contains(s));
    assert!(
        found("0x0101FFFE: says 2 cells, and cell 0x01010100"),
        "{:?}",
        report.problems
    );
    assert!(
        found("0x0202FFFF: says it has a statics record"),
        "{:?}",
        report.problems
    );
    assert!(
        found("0x0201FFFF: has a statics record"),
        "{:?}",
        report.problems
    );
    assert!(
        found("0x05000010: names image 0x06000099"),
        "{:?}",
        report.problems
    );
    assert_eq!(report.problems.len(), 4, "{:?}", report.problems);
}
