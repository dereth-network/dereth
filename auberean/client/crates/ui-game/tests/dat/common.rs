//! The target install's interface records, opened once per test binary.

use std::sync::{Arc, OnceLock};

use auberean_assets::Context;
use auberean_dat::locate::DatName;
use auberean_dat::testing::require_target_install;
use auberean_dat::DatFile;
use auberean_ui::DatAssets;

/// The target install's records (its English file, then its portal and high-resolution files);
/// the test fails when the install is not found.
pub fn records() -> Arc<DatAssets> {
    static RECORDS: OnceLock<Arc<DatAssets>> = OnceLock::new();
    RECORDS
        .get_or_init(|| {
            let dir = require_target_install();
            let open = |n: &DatName| DatFile::open(&n.in_dir(&dir)).expect("open a data file");
            let portal = open(&DatName::Portal);
            let ctx = Context::from_portal(&portal).expect("the portal's property list");
            let mut dats = vec![open(&DatName::Local("English".into())), portal];
            if DatName::HighRes.in_dir(&dir).exists() {
                dats.push(open(&DatName::HighRes));
            }
            Arc::new(DatAssets::new(dats, ctx))
        })
        .clone()
}
