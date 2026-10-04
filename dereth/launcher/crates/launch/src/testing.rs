//! Synthetic dats and installs, for tests in this crate and the ones above it.

use std::path::Path;

use crate::datset::{DatRole, Iterations};

pub use crate::dat::testdat::{build as dat_bytes, eor as eor_dat_bytes};

/// Write a Classic pair, `portal.dat` and `cell.dat`, with the given iterations.
pub fn write_classic_set(dir: &Path, portal: u32, cell: u32) {
    std::fs::create_dir_all(dir).expect("create the test folder");
    std::fs::write(
        dir.join("portal.dat"),
        crate::dat::testdat::pre_tod(false, portal),
    )
    .expect("write a test dat");
    std::fs::write(
        dir.join("cell.dat"),
        crate::dat::testdat::pre_tod(true, cell),
    )
    .expect("write a test dat");
}

/// Write a folder of the four dats with the given iteration counts. Missing counts are missing files.
pub fn write_dat_set(dir: &Path, it: Iterations) {
    std::fs::create_dir_all(dir).expect("create the test folder");
    for (role, set, sub) in [
        (DatRole::Portal, 1, 0),
        (DatRole::Cell, 2, 1),
        (DatRole::Local, 3, 1),
        (DatRole::Highres, 1, 0x6946_6948),
    ] {
        if let Some(n) = it.get(role) {
            std::fs::write(
                dir.join(role.file_name()),
                crate::dat::testdat::eor(set, sub, n),
            )
            .expect("write a test dat");
        }
    }
}
