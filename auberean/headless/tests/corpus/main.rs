//! Tests that replay the recorded sessions through the client. Without the install or the capture
//! index, a test fails, naming the variable to set.

mod download;
#[path = "../replay/mod.rs"]
mod replay;
mod sessions;

use std::sync::{Arc, OnceLock};

use auberean_client_sdk::runtime::data::{DataFiles, ENGLISH};
use auberean_client_sdk::runtime::link_scripts;
use auberean_client_sdk::ClientData;
use auberean_dat::locate::{install_dats, DatName};
use auberean_dat::testing::require_target_install;
use auberean_dat::DatFile;

/// The September 2013 install's data files in the order the client opens them, and its script
/// library linked against the client's natives.
pub fn client_data() -> Option<&'static ClientData> {
    static D: OnceLock<Option<ClientData>> = OnceLock::new();
    D.get_or_init(|| {
        let dir = require_target_install();
        let found = install_dats(&dir).ok()?;
        let open = |want: &dyn Fn(&DatName) -> bool| -> Option<Arc<DatFile>> {
            let (_, p) = found.iter().find(|(n, _)| want(n))?;
            DatFile::open(p).ok().map(Arc::new)
        };
        let portal = open(&|n| *n == DatName::Portal)?;
        let mut files = vec![Arc::clone(&portal)];
        files.extend(open(&|n| *n == DatName::Local("English".into())));
        files.extend(open(&|n| *n == DatName::Cell(1)));
        files.extend(open(&|n| *n == DatName::HighRes));
        let program =
            link_scripts(&portal).expect("the library links against the client's natives");
        Some(ClientData {
            files: DataFiles {
                files,
                language: ENGLISH,
            },
            scripts: Some((portal, Arc::new(program))),
        })
    })
    .as_ref()
}
