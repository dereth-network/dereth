//! Behaviour: none (checks formats, geometry, numeric contracts or repository tools)
//! The four retail dats resolve under DERETH_TEST_DAT_DIR (the label for every other dat-tier failure).
//! Fixture: the shipped retail DAT records and recorded inputs.

use dereth_dat::{RetailDat, RetailDatStore};

/// The four files the tier-1 gates' retail-data check names (`xtask`'s `RETAIL_DATS`; the retired
/// `dere_fixtures::RETAIL_DATS` before it). `client_highres.dat` is in the list even though
/// [`RetailDatStore::open_dir`] treats it as optional, because that check requires all four — so an install missing it
/// answers *"no retail data"* to every tier-1 gate while the store itself opens happily. A partial
/// install produces a confusing half-skip rather than a clean one; that is the gates' own
/// stated reason and it is why this preflight is the stricter of the two.
const FOUR: [RetailDat; 4] = RetailDat::ALL;

/// The whole point of the file. Reads the directory; opens nothing.
#[test]
fn the_four_retail_dats_resolve_or_nothing_else_in_this_workspace_means_anything() {
    let set = std::env::var_os(dereth_dat::testing::DAT_DIR_VAR).is_some_and(|v| !v.is_empty());
    assert!(
        set,
        "PREFLIGHT: {} is unset, so the retail dats -- the oracle for ~1,600 tests in this \
         workspace -- are nowhere. Set it to the directory holding them.",
        dereth_dat::testing::DAT_DIR_VAR
    );
    let dir = dereth_dat::testing::dat_dir();
    // A relative directory resolves against the process's working directory, and cargo does not
    // promise which directory a test binary runs in -- so a relative value would make the oracle
    // mean different things in different suites. Assert it, rather than discovering it as a suite
    // that finds the dats and a neighbour that does not.
    assert!(
        dir.is_absolute(),
        "{} must be an absolute path; the dats resolved to {}",
        dereth_dat::testing::DAT_DIR_VAR,
        dir.display()
    );
    println!(
        "{} is set; the retail oracle resolves to {}",
        dereth_dat::testing::DAT_DIR_VAR,
        dir.display()
    );

    // ---------------------------------------------------------------------------------------
    // The negative control, first, because a check that cannot fail is not a check.
    //
    // A zero from an instrument you built is worth nothing until that instrument has produced a
    // non-zero on something you already know is non-zero. Here the known-negative is a directory
    // that certainly holds no dats, and the calibration goes through the same `shortfall_in` and
    // the same locator the tests use, not through a second copy of the logic.
    // ---------------------------------------------------------------------------------------
    let empty = std::env::temp_dir().join("dereth-o665-preflight-negative-control");
    std::fs::create_dir_all(&empty).expect("a temp directory for the negative control");
    let control = RetailDatStore::shortfall_in(&empty)
        .expect("a directory with no dats in it must report a shortfall");
    for name in RetailDatStore::REQUIRED_DATS {
        assert!(
            control.contains(name),
            "the shortfall message must name every missing file; it does not name {name}:\n{control}"
        );
    }
    assert!(
        control.contains(&empty.display().to_string()),
        "the shortfall message must name the directory it looked in:\n{control}"
    );
    let miss = dereth_dat::locate_retail_dats(std::slice::from_ref(&empty))
        .expect_err("an empty directory holds no dats");
    assert!(
        miss.to_string().contains(&empty.display().to_string()),
        "the locator's miss must name every directory it tried: {miss}"
    );

    // ---------------------------------------------------------------------------------------
    // The assertion itself.
    // ---------------------------------------------------------------------------------------
    let missing: Vec<&str> = FOUR
        .iter()
        .filter(|d| !d.in_dir(&dir).is_file())
        .map(|d| d.file_name())
        .collect();
    for d in FOUR {
        let name = d.file_name();
        match std::fs::metadata(d.in_dir(&dir)) {
            Ok(m) => println!("  ok       {name:<26} {:>13} bytes", m.len()),
            Err(e) => println!("  MISSING  {name:<26} {e}"),
        }
    }
    assert!(
        missing.is_empty(),
        "PREFLIGHT: the retail dats are the oracle for ~1,600 tests in this workspace and {} of \
         them are not under {}: {}.\n\
         Set {} to the directory holding them.\n\
         Every other red you are looking at in this run is probably this one.",
        missing.len(),
        dir.display(),
        missing.join(", "),
        dereth_dat::testing::DAT_DIR_VAR
    );

    // Two readings that could have disagreed: the list above, and the store's own gate. The
    // store's is the shorter list (`open_dir` treats the high-res dat as optional), so it can only
    // agree once the four-file assertion has passed.
    assert!(
        dereth_dat::testing::shortfall().is_none(),
        "the four files are all present and `dereth_dat::testing::shortfall` still reports a \
         shortfall, which means the two readings disagree: {:?}",
        dereth_dat::testing::shortfall()
    );
}
