//! The target install's portal file and script library, opened once per test binary.
//! Behaviour: none (test support)

use std::sync::{Arc, OnceLock};

use auberean_dat::locate::{install_dats, DatName};
use auberean_dat::testing::{require_install, TARGET};
use auberean_dat::DatFile;
use auberean_script::library::{Library, RECORD};

/// The target install's portal file and its library; the test fails when the install is not found.
pub fn target() -> Option<&'static (Arc<DatFile>, Library)> {
    static T: OnceLock<Option<(Arc<DatFile>, Library)>> = OnceLock::new();
    T.get_or_init(|| {
        let dir = require_install(TARGET);
        let (_, path) = install_dats(&dir)
            .ok()?
            .into_iter()
            .find(|(n, _)| *n == DatName::Portal)?;
        let dat = DatFile::open(&path).expect("open the portal file");
        let lib = dat
            .read_checked(RECORD, Library::from_record)
            .expect("the script library")
            .value;
        Some((Arc::new(dat), lib))
    })
    .as_ref()
}

/// The target install: a test that needs it fails when the install is not found.
macro_rules! need_target {
    () => {
        crate::common::target().expect("the install was required when it was opened")
    };
}
pub(crate) use need_target;
