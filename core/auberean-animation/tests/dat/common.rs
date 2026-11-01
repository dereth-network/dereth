//! The target install's records, opened once per test binary.

use std::sync::OnceLock;

use auberean_assets::Context;
use auberean_dat::locate::{install_dats, DatName};
use auberean_dat::testing::require_target_install;
use auberean_dat::DatFile;
use auberean_world::records::WorldRecords;

/// The records of the target install, opened once; the test fails when the install is not found.
pub fn records() -> Option<&'static WorldRecords> {
    static WORLD: OnceLock<Option<WorldRecords>> = OnceLock::new();
    WORLD
        .get_or_init(|| {
            let dir = require_target_install();
            let mut files: Vec<(DatName, DatFile)> = install_dats(&dir)
                .expect("list the install")
                .into_iter()
                .map(|(n, p)| {
                    (
                        n,
                        DatFile::open(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display())),
                    )
                })
                .collect();
            let mut take = |name: &DatName| {
                let k = files.iter().position(|(n, _)| n == name)?;
                Some(files.swap_remove(k).1)
            };
            let portal = take(&DatName::Portal).expect("a portal file");
            let cell = take(&DatName::Cell(1)).expect("a cell file");
            let highres = take(&DatName::HighRes);
            let ctx = Context::from_portal(&portal).expect("the property list");
            Some(WorldRecords::new(portal, cell, highres, ctx))
        })
        .as_ref()
}

/// The records: a test that needs them fails when the install is not found.
macro_rules! need_records {
    () => {
        crate::common::records().expect("the install was required when it was opened")
    };
}
pub(crate) use need_records;

/// The target install's portal file, opened once (to list its records).
pub fn portal() -> &'static DatFile {
    static PORTAL: OnceLock<DatFile> = OnceLock::new();
    PORTAL.get_or_init(|| {
        let dir = require_target_install();
        let (_, p) = install_dats(&dir)
            .expect("list the install")
            .into_iter()
            .find(|(n, _)| *n == DatName::Portal)
            .expect("a portal file");
        DatFile::open(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
    })
}

/// The ids of the portal records in `lo..=hi`.
pub fn ids(lo: u32, hi: u32) -> Vec<u32> {
    portal().entries_in(lo, hi).map(|e| e.id).collect()
}
