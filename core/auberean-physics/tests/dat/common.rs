//! The target install's world, opened once per test binary (or anew, for a test that changes it).

use std::sync::OnceLock;

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
    WORLD.get_or_init(|| Some(open_world())).as_ref()
}

/// The target install's world opened anew, for a test that changes what its records keep; the
/// test fails when the install is not found.
pub fn open_world() -> World {
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
    World { records, region }
}

/// The world: a test that needs it fails when the install is not found.
macro_rules! need_world {
    () => {
        crate::common::world().expect("the install was required when it was opened")
    };
}
pub(crate) use need_world;
