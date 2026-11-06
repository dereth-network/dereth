//! Opening an install's data files.

use auberean_client_sdk::dat::locate::{install_dats, DatName};
use auberean_client_sdk::dat::DatFile;
use auberean_interface::data::{open_data, Install, NO_CELL, NO_PORTAL};

/// Where a data file's header says its game version.
const GAME_VERSION_AT: usize = 0x12C + 0x38;

/// A data file of the install, its header saying `version`.
fn at_version(path: &std::path::Path, version: u32) -> DatFile {
    let mut bytes = std::fs::read(path).expect("the file reads");
    bytes[GAME_VERSION_AT..GAME_VERSION_AT + 4].copy_from_slice(&version.to_le_bytes());
    DatFile::from_bytes(bytes).expect("the file opens")
}

/// A portal file not at game version 3, or a cell file not at 0, counts as not there, and nothing
/// opens.
///
/// Behaviour: data.disk.a-file-of-another-game-version-is-not-opened
#[test]
fn a_file_of_another_game_version_is_not_opened() {
    let dir = auberean_dat::testing::require_target_install();
    let found = install_dats(&dir).expect("list the install");
    let path = |want: &DatName| {
        found
            .iter()
            .find(|(n, _)| n == want)
            .map(|(_, p)| p.clone())
            .expect("the file")
    };
    let (portal, small) = (
        path(&DatName::Portal),
        path(&DatName::Local("English".to_owned())),
    );
    let names = vec![DatName::Portal, DatName::Cell(1)];
    let error = |open: &dyn Fn(&DatName) -> Option<DatFile>| {
        let install = Install {
            found: names.clone(),
            open,
        };
        open_data(&install, None, "").err()
    };
    assert_eq!(
        error(&|_| Some(at_version(&small, 2))),
        Some(NO_PORTAL.to_owned())
    );
    assert_eq!(
        error(&|name| match name {
            DatName::Portal => DatFile::open(&portal).ok(),
            _ => Some(at_version(&small, 3)),
        }),
        Some(NO_CELL.to_owned())
    );
}
