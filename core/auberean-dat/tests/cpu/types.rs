//! The type table's own state and the fields the authoring tools read: its folders, which types'
//! records are files of their own, and the pack version.

use auberean_dat::{MasterMap, RecordType};

/// The table is set up once (a second set-up answers `false`) and let go; the folders a type's
/// source files are kept in are the game's or the engine's folder followed by the type's folder
/// name, and empty for a type with no folder name or not in the table.
///
/// Behaviour: data.master-map.data-folders
#[test]
fn a_types_folders_join_the_games_or_engines_folder_and_its_folder_name() {
    let mut m = MasterMap::default();
    assert!(!m.is_set_up());
    assert!(m.init());
    assert!(!m.init(), "set up already");
    m.set_game_data_dir(MasterMap::PLAYER_FOLDER);
    m.set_engine_data_dir("E:/engine/");
    assert_eq!(m.game_data_dir(RecordType::MESH), "/mesh");
    assert_eq!(
        m.engine_data_dir(RecordType::SCRIPT_LIBRARY),
        "E:/engine/weenie/wsl"
    );
    assert_eq!(m.game_data_dir(RecordType::LANDBLOCK), "", "no folder name");
    assert_eq!(
        m.game_data_dir(RecordType(0x4000_0099)),
        "",
        "not in the table"
    );
    assert!(m.cleanup());
    assert!(!m.is_set_up());
    assert_eq!(m.game_folder(), "/", "the folders stay");
    assert!(m.init());
}

/// Every type's pack version is 1; the authoring tools keep each record as a file of its own for
/// every type but five: the landblock, the cell, the file-id table, the qualities and the
/// iteration list. A type not in the table has neither.
///
/// Behaviour: data.master-map.has-file-and-pack-version
#[test]
fn every_type_but_five_keeps_its_records_as_files_and_every_pack_version_is_one() {
    let without: Vec<u32> = auberean_dat::types::TYPES
        .iter()
        .filter(|t| !t.ty.has_file())
        .map(|t| t.ty.0)
        .collect();
    assert_eq!(
        without,
        [
            0x4000_0019,
            0x4000_0046,
            0x4000_0050,
            0x4000_0052,
            0x4000_0053
        ]
    );
    assert!(auberean_dat::types::TYPES
        .iter()
        .all(|t| t.ty.pack_version() == 1));
    assert!(!RecordType(0x4000_0099).has_file());
    assert_eq!(RecordType(0x4000_0099).pack_version(), 0);
}
