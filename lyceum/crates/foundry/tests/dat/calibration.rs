//! Behaviour: none (tooling: the record checks, calibrated on the retail files)

use lyceum_foundry::dats::DatSet;
use lyceum_foundry::validate::{validate, Scope};

/// Every landblock, statics record, surface, image texture and picture of the end-of-retail files
/// passes every check: a check that failed here would be wrong, not the data.
#[test]
fn the_checks_pass_on_every_retail_record() {
    let set = DatSet::open(&dereth_dat::testing::dat_dir()).unwrap();
    let report = validate(&set, &Scope::All);
    assert!(
        report.problems.is_empty(),
        "{} fault(s), the first {:?}",
        report.problems.len(),
        &report.problems[..report.problems.len().min(8)]
    );
    // 65,025 landblocks, 5,346 statics records, 6,152 surfaces, 7,221 image textures and the
    // portal's and high-res file's 22,978 pictures.
    assert_eq!(report.checked, 106_722);
}
