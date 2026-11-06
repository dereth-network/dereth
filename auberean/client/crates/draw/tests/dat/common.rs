//! The target install's records, opened once per test binary.

use std::sync::OnceLock;

use auberean_assets::Context;
use auberean_dat::locate::{install_dats, DatName};
use auberean_dat::testing::require_target_install;
use auberean_dat::DatFile;
use auberean_world::records::WorldRecords;

/// The records of the target install and the ids its portal file holds; the test fails when the
/// install is not found.
pub struct Install {
    pub records: WorldRecords,
    pub ids: Vec<u32>,
}

pub fn install() -> &'static Install {
    static INSTALL: OnceLock<Install> = OnceLock::new();
    INSTALL.get_or_init(|| {
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
        let language = take(&DatName::Local("English".to_owned()));
        let ids = portal.entries().map(|e| e.id).collect();
        let ctx = Context::from_portal(&portal).expect("the property list");
        let records = WorldRecords::new(portal, cell, highres, ctx);
        if let Some(file) = language {
            records.use_language_file(file);
        }
        Install { records, ids }
    })
}

/// The ids of one record kind (the top byte of its ids).
pub fn ids_of(kind: u8) -> Vec<u32> {
    install()
        .ids
        .iter()
        .copied()
        .filter(|id| id >> 24 == u32::from(kind))
        .collect()
}
