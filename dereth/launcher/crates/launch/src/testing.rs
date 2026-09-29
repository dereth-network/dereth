//! Synthetic dats and installs, for tests in this crate and the ones above it.

use std::path::Path;

use crate::datset::{DatRole, Iterations};

pub use crate::dat::testdat::{build as dat_bytes, eor as eor_dat_bytes};

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
