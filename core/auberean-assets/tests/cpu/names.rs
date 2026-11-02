//! Records' file names: the names table's rules, the file-id table record read into one, and the
//! client's own table of file names, which starts empty.

use auberean_assets::names::{reformat_folder, strip_folders, EntryKind, FileNames, NameTable};
use auberean_assets::tables::{FileIdEntry, FileIdTable, FileIdType};
use auberean_dat::{MasterMap, RecordType};
use auberean_primitives::DataId;

const MUSIC: u32 = RecordType::MUSIC.0;
const WLIB: u32 = RecordType::SCRIPT_LIBRARY.0;
const STT: u32 = RecordType::STRING_TABLE.0;

/// A name is given an id only when its extension is the type's; it is kept without its folders
/// and in lower case, and a name that has an id keeps it.
///
/// Behaviour: data.name-table.assign-only-by-extension
#[test]
fn a_files_name_is_given_its_id_only_when_its_extension_is_the_types() {
    let mut t = NameTable::new();
    assert!(!t.assign_id("server.wlib", 0x5600_0000, MUSIC), "not music");
    assert!(t.is_empty(), "nothing is kept for a name refused");
    assert!(t.assign_id("Music\\Battle\\Drums.SGT", 0x2400_0001, MUSIC));
    let i = t
        .find_by_name("drums.sgt", EntryKind::OnlyExisting)
        .unwrap();
    let e = t.entry(i);
    assert_eq!(
        (e.name.as_str(), e.id, e.record_type),
        ("drums.sgt", 0x2400_0001, MUSIC)
    );
    assert_eq!(e.kind, EntryKind::WithId);
    assert!(
        t.assign_id("drums.sgt", 0x2400_0002, MUSIC),
        "a name with an id keeps it"
    );
    assert_eq!(t.id_by_name("drums.sgt", MUSIC), 0x2400_0001);
    assert_eq!(t.type_entry_if_any(MUSIC).unwrap().highest_id, 0x2400_0001);
}

/// A name is looked up without its folders and with letter case ignored; the type asked is not
/// consulted.
///
/// Behaviour: data.name-table.lookup-without-folders-or-case
#[test]
fn a_name_is_looked_up_without_its_folders_or_letter_case() {
    let mut t = NameTable::new();
    assert!(t.assign_id("server.wlib", 0x5600_0000, WLIB));
    assert_eq!(
        t.id_by_name("C:\\data/scripts\\SERVER.wlib", WLIB),
        0x5600_0000
    );
    assert_eq!(t.id_by_name("x:Server.WLIB", MUSIC), 0x5600_0000);
    assert_eq!(t.id_by_name("client.wlib", WLIB), 0);
    assert_eq!(strip_folders("a/b\\c:d.e"), "d.e");
    assert_eq!(strip_folders("plain"), "plain");
    assert_eq!(strip_folders("ends/"), "");
}

/// An id keeps the first name given it: a second name given the same id has it too, but the
/// type's entry still names the first, and the answer is `false`.
///
/// Behaviour: data.name-table.first-name-keeps-the-id
#[test]
fn an_id_keeps_the_first_name_given_it() {
    let mut t = NameTable::new();
    assert!(t.assign_id("one.sgt", 0x2400_0005, MUSIC));
    assert!(!t.assign_id("two.sgt", 0x2400_0005, MUSIC));
    assert_eq!(t.id_by_name("two.sgt", MUSIC), 0x2400_0005);
    assert_eq!(t.name_by_id(0x2400_0005, MUSIC).as_deref(), Some("one.sgt"));
    assert!(t.assign_id("low.sgt", 0x2400_0001, MUSIC));
    assert_eq!(t.type_entry_if_any(MUSIC).unwrap().highest_id, 0x2400_0005);
    assert_eq!(t.type_entry_if_any(MUSIC).unwrap().len(), 2);
    assert_eq!(t.name_by_id(0x2400_0009, MUSIC), None);
    assert_eq!(
        t.name_by_id(0x2400_0005, WLIB),
        None,
        "by id, the type is consulted"
    );
    t.clear();
    assert!(t.is_empty());
    assert_eq!(t.id_by_name("one.sgt", MUSIC), 0);
}

/// A type registers once, under its name; of its folders the table keeps only one that names a
/// drive (lowered, `/` turned to `\`, a doubled `/` and a trailing one dropped), as an entry of
/// its own, and the type is searched only when its folders' entries were.
///
/// Behaviour: data.name-table.register-keeps-drive-folders
#[test]
fn a_type_registers_once_and_keeps_only_folders_that_name_a_drive() {
    assert_eq!(
        reformat_folder("C:/Data//Music/"),
        ("c:\\data\\music".to_owned(), true)
    );
    assert_eq!(reformat_folder("/music"), ("\\music".to_owned(), false));
    assert_eq!(reformat_folder("a:b:c"), ("a:b:c".to_owned(), false));

    let mut t = NameTable::new();
    assert!(t.register_type(MUSIC, "MUSIC", "/music", "/music"));
    let m = t.type_entry_if_any(MUSIC).unwrap();
    assert!(m.registered && !m.searched);
    assert_eq!((m.name.as_str(), m.game_root.as_str()), ("MUSIC", ""));
    assert!(t.is_empty(), "no folder kept");
    assert!(!t.register_type(MUSIC, "OTHER", "", ""));
    assert_eq!(t.type_entry_if_any(MUSIC).unwrap().name, "MUSIC");

    assert!(t.register_type(STT, "STRING", "E:/Engine/stt", "G:/Game/STT/"));
    let s = t.type_entry_if_any(STT).unwrap();
    assert_eq!(s.game_root, "g:\\game\\stt");
    assert_eq!(s.engine_root, "e:\\engine\\stt");
    assert!(!s.searched);
    let i = t
        .find_by_name("G:\\GAME\\STT", EntryKind::OnlyExisting)
        .unwrap();
    assert_eq!(t.entry(i).kind, EntryKind::UnsearchedFolder);
    assert_eq!(t.entry(i).path, "g:\\game\\stt");
    assert!(t.is_type_registered(STT) && !t.is_type_registered(WLIB));
    assert!(!t.is_type_registered(0x4000_0099));
}

/// The file-id table record reads as the client reads it: each type's entry named, each name
/// given its id by the table's rules (a name whose extension is not its type's gets none).
///
/// Behaviour: data.file-id-table.read-as-the-client-reads-it
#[test]
fn the_file_id_table_record_reads_into_a_names_table_by_the_tables_rules() {
    let entry = |id: u32, name: &str| FileIdEntry {
        id: DataId(id),
        unused: String::new(),
        name: name.to_owned(),
    };
    let record = FileIdTable {
        types: vec![
            FileIdType {
                record_type: RecordType::QUALITIES.0,
                name: "QUALITIES".into(),
                entries: vec![],
            },
            FileIdType {
                record_type: WLIB,
                name: "WLIB".into(),
                entries: vec![entry(0x5600_0000, "server.wlib")],
            },
            FileIdType {
                record_type: MUSIC,
                name: "MUSIC".into(),
                entries: vec![
                    entry(0x2400_0007, "MelodicBand.sgt"),
                    entry(0x2400_0008, "stray.wlib"),
                ],
            },
        ],
    };
    let mut t = NameTable::from_file_ids(&record);
    assert_eq!(
        t.type_entry_if_any(RecordType::QUALITIES.0).unwrap().name,
        "QUALITIES"
    );
    assert!(!t.is_type_registered(MUSIC), "reading registers no type");
    assert_eq!(t.id_by_name("melodicband.sgt", MUSIC), 0x2400_0007);
    assert_eq!(t.id_by_name("server.wlib", WLIB), 0x5600_0000);
    assert_eq!(t.id_by_name("stray.wlib", MUSIC), 0);
    assert_eq!(
        t.name_by_id(0x2400_0007, MUSIC).as_deref(),
        Some("melodicband.sgt")
    );
    assert_eq!(t.len(), 2);
}

fn player_master() -> MasterMap {
    let mut m = MasterMap::default();
    assert!(m.init());
    m.set_game_data_dir(MasterMap::PLAYER_FOLDER);
    m.set_engine_data_dir(MasterMap::PLAYER_FOLDER);
    m
}

/// The client's table of file names, started as the client starts it (folders off, ids not to be
/// made, no file of names), holds nothing: every id asked is 0, every name and path empty. Types
/// asked about are registered once, `UNKNOWN` with no data cache; the last stop forgets them.
///
/// Behaviour: data.file-names.start-empty-and-answer-nothing
#[test]
fn the_clients_table_of_file_names_starts_empty_and_answers_nothing() {
    let m = player_master();
    let mut f = FileNames::new();
    assert!(!f.uses_folders() && f.ids_may_be_made() && !f.is_active());
    assert_eq!(f.id("drums.sgt", MUSIC, None, &m), 0, "not started");
    f.set_uses_folders(false);
    f.set_ids_may_be_made(false);
    assert!(f.init(4, 7, &m));
    assert!(f.is_active() && f.is_starting() && f.is_read_only() && f.is_new_name_file());
    assert!(!f.is_engine_only());
    assert_eq!((f.game_pack_stamp(), f.name_stores()), (7, 1));
    assert!(f.verify_types());
    assert!(!f.is_starting());
    assert!(f.init(5, 9, &m), "a second start only counts");
    assert_eq!((f.game_pack_stamp(), f.name_stores()), (7, 1));

    assert_eq!(f.id("  drums.sgt\t", MUSIC, None, &m), 0);
    assert_eq!(f.id("drums.wlib", MUSIC, None, &m), 0);
    assert_eq!(f.id("", MUSIC, None, &m), 0);
    assert_eq!(f.id_by_extension("server.wlib", None, &m), 0);
    let wide: Vec<u16> = "ui.stt".encode_utf16().collect();
    assert_eq!(f.id_wide(&wide, STT, None, &m), 0);
    assert_eq!(f.id_by_extension_wide(&wide, None, &m), 0);
    assert_eq!(f.file_name(0x2400_0007, MUSIC, None, &m), "");
    assert_eq!(f.file_name_of(0x2400_0007, None, &m), "");
    assert!(f.file_name_of_wide(0x2400_0007, None, &m).is_empty());
    assert_eq!(f.path_of(0x2400_0007), "");
    assert_eq!(f.path_by_extension("drums.sgt"), "");
    assert!(f.table().is_empty());

    let types = f.table();
    let music = types.type_entry_if_any(MUSIC).unwrap();
    assert!(music.registered);
    assert_eq!(music.name, "UNKNOWN");
    assert!(types.is_type_registered(WLIB) && types.is_type_registered(STT));

    f.cleanup();
    assert!(f.is_active(), "started twice, stopped once");
    assert!(
        f.table().is_type_registered(MUSIC),
        "the names stay until the last stop"
    );
    f.cleanup();
    assert!(!f.is_active());
    assert!(!f.table().is_type_registered(MUSIC));
}

/// Types are named `ENUM_MAPPER` and `DID_MAPPER` for those two; a type whose records are files
/// of their own registers its folders, which on a player's machine name no drive and are not
/// kept.
///
/// Behaviour: data.file-names.types-registered-with-their-names-and-folders
#[test]
fn types_register_with_their_own_names_and_no_folder_on_a_players_machine() {
    let m = player_master();
    let mut f = FileNames::new();
    f.set_uses_folders(false);
    f.set_ids_may_be_made(false);
    assert!(f.init(4, 0, &m));
    f.register_type(RecordType::ENUM_MAP.0, None, &m);
    f.register_type(RecordType::ID_MAP.0, None, &m);
    f.register_type(RecordType::LANDBLOCK.0, None, &m);
    f.register_type(MUSIC, None, &m);
    let t = f.table();
    assert_eq!(
        t.type_entry_if_any(RecordType::ENUM_MAP.0).unwrap().name,
        "ENUM_MAPPER"
    );
    assert_eq!(
        t.type_entry_if_any(RecordType::ID_MAP.0).unwrap().name,
        "DID_MAPPER"
    );
    let music = t.type_entry_if_any(MUSIC).unwrap();
    assert_eq!(
        (music.game_root.as_str(), music.engine_root.as_str()),
        ("", "")
    );
    assert!(t.is_empty());

    let mut dev = MasterMap::default();
    dev.set_game_data_dir("C:/proj/game/");
    dev.set_engine_data_dir("C:/proj/engine/");
    let mut g = FileNames::new();
    assert!(g.init(0, 0, &dev));
    g.register_type(MUSIC, None, &dev);
    g.register_type(RecordType::LANDBLOCK.0, None, &dev);
    let t = g.table();
    let music = t.type_entry_if_any(MUSIC).unwrap();
    assert_eq!(music.game_root, "c:\\proj\\game\\sound");
    assert_eq!(music.engine_root, "c:\\proj\\engine\\sound");
    let lb = t.type_entry_if_any(RecordType::LANDBLOCK.0).unwrap();
    assert_eq!(
        (lb.game_root.as_str(), lb.registered),
        ("", true),
        "no file of its own"
    );
}

/// With folders on, a start fails unless both folders are set.
///
/// Behaviour: data.file-names.start-empty-and-answer-nothing
#[test]
fn with_folders_on_a_start_needs_both_folders() {
    let mut f = FileNames::new();
    f.set_uses_folders(true);
    assert!(!f.init(0, 0, &MasterMap::default()));
    assert!(f.is_active(), "the start is counted all the same");
    assert!(!f.is_new_name_file());
}
