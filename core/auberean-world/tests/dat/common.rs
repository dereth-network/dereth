//! The target install's world, opened once per test binary, and its landblocks, loaded once.

use std::collections::HashMap;
use std::sync::OnceLock;

use auberean_assets::landblock::Landblock;
use auberean_assets::Context;
use auberean_dat::locate::{install_dats, DatName};
use auberean_dat::testing::require_target_install;
use auberean_dat::DatFile;
use auberean_world::records::{RegionSet, WorldRecords};

/// The world of the target install, and its region.
pub struct World {
    pub records: WorldRecords,
    pub region: RegionSet,
}

/// The target install's world, opened once; the test fails when the install is not found.
pub fn world() -> Option<&'static World> {
    static WORLD: OnceLock<Option<World>> = OnceLock::new();
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
            let records = WorldRecords::new(portal, cell, highres, ctx);
            let region = records.region().expect("the region");
            Some(World { records, region })
        })
        .as_ref()
}

/// Every landblock of the target install, by block coordinates.
pub fn landblocks() -> &'static HashMap<(i32, i32), Landblock> {
    static ALL: OnceLock<HashMap<(i32, i32), Landblock>> = OnceLock::new();
    ALL.get_or_init(|| {
        let Some(w) = world() else {
            return HashMap::new();
        };
        w.records
            .landblock_coordinates()
            .into_iter()
            .map(|(x, y)| {
                (
                    (x, y),
                    w.records.landblock(x, y).expect("a landblock decodes"),
                )
            })
            .collect()
    })
}

/// The world: a test that needs it fails when the install is not found.
macro_rules! need_world {
    () => {
        crate::common::world().expect("the install was required when it was opened")
    };
}
pub(crate) use need_world;
