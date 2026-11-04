//! Characters and accounts kept and read back. Each test uses its own file in the temporary
//! folder, removed at the end.

use yalain_store::{
    backups, restore, snapshot, Ban, Character, Listed, MemoryStore, NetworkList, SqliteStore,
    Store, StoredPosition, WorldState, KEEP_BACKUPS, SCHEMA_VERSION,
};

fn character(id: u64, account: u64, name: &str) -> Character {
    Character {
        id,
        account,
        name: name.to_string(),
        race: 4,
        sex: 0x2000,
        class: 4,
        implement: 8,
        start_area: 0x4000_0005,
        entity_did: 0x4700_110F,
        physique: vec![(1, 0.94), (2, 1.01)],
        look: vec![0x22, 1, 0, 0, 0x7A, 0, 0, 0x1F],
        created_at: 1_484_464_962,
        deleted_at: None,
        last_played: 1_484_464_962,
    }
}

struct TempFile(std::path::PathBuf);

impl Drop for TempFile {
    // The file, its write-ahead log and index, and its backups.
    fn drop(&mut self) {
        for b in backups(&self.0) {
            let _ = std::fs::remove_file(b);
        }
        for suffix in ["-wal", "-shm", ""] {
            let mut name = self.0.as_os_str().to_owned();
            name.push(suffix);
            let _ = std::fs::remove_file(std::path::PathBuf::from(name));
        }
    }
}

/// An account's characters, with their choices, looks and deletions, are the same after the
/// server is stopped and started again, and character numbers are never handed out twice.
/// Behaviour: server.characters.outlive-a-restart
#[test]
fn characters_and_their_deletions_outlive_reopening_the_database() {
    let file = TempFile(
        std::env::temp_dir().join(format!("yalain-store-test-{}.sqlite", std::process::id())),
    );
    let _ = std::fs::remove_file(&file.0);
    let (account, numbers) = {
        let mut s = SqliteStore::open(&file.0).expect("opens");
        let a = s
            .create_account("Tester", Some(vec![1, 2, 3]), 100)
            .expect("made");
        let n1 = s.next_character_number().expect("number");
        let n2 = s.next_character_number().expect("number");
        s.insert_character(&character(n1, a.id, "Babyfare"))
            .expect("kept");
        s.insert_character(&character(n2, a.id, "Lugscrew"))
            .expect("kept");
        s.set_deleted(n2, Some(200)).expect("deleted");
        (a, (n1, n2))
    };
    let mut s = SqliteStore::open(&file.0).expect("opens again");
    assert_eq!(
        s.account_by_name("tester").expect("reads"),
        Some(account.clone())
    );
    let chars = s.characters(account.id).expect("reads");
    assert_eq!(chars.len(), 2);
    assert_eq!(chars[0], character(numbers.0, account.id, "Babyfare"));
    assert_eq!(chars[1].deleted_at, Some(200));
    assert!(s.name_taken("BABYFARE").expect("reads"));
    assert!(s.next_character_number().expect("number") > numbers.1);
    s.purge_character(numbers.1).expect("purged");
    assert_eq!(s.characters(account.id).expect("reads").len(), 1);
    assert!(
        s.insert_character(&character(numbers.0, account.id, "Other"))
            .is_err(),
        "an id is kept once"
    );
}

/// Behaviour: server.characters.outlive-a-restart
#[test]
fn the_memory_store_keeps_the_same_rules() {
    let mut s = MemoryStore::default();
    let a = s.create_account("Tester", None, 1).expect("made");
    assert!(s.create_account("TESTER", None, 1).is_err());
    s.insert_character(&character(1, a.id, "Babyfare"))
        .expect("kept");
    assert!(
        s.insert_character(&character(2, a.id, "babyfare")).is_err(),
        "names are unique in any case"
    );
    assert_eq!(s.characters(a.id).expect("reads").len(), 1);
}

fn world() -> WorldState {
    WorldState {
        position: Some(StoredPosition {
            cell: 0x0298_0109,
            origin: [59.06, 240.2, -44.6],
            rotation: [
                std::f32::consts::FRAC_1_SQRT_2,
                0.0,
                0.0,
                std::f32::consts::FRAC_1_SQRT_2,
            ],
        }),
        instances: 3,
        qualities: vec![0x2B, 0, 0, 0, 0x30, 5, 0, 0x81],
        options: vec![0xEA, 0x27, 0, 0],
        places: vec![(
            0x4000_0001,
            StoredPosition {
                cell: 0x76B8_0025,
                origin: [93.8, 87.5, 182.5],
                rotation: [1.0, 0.0, 0.0, 0.0],
            },
        )],
        play: vec![1, 0, 0, 0, 9, 9, 9, 9],
    }
}

/// Where a character stands, what it is in the world, the places it keeps and what it holds
/// and has done in play are kept when it leaves and read back at its next entry, across the server stopping and starting again; purging the character
/// forgets them.
/// Behaviour: server.world.state-kept-between-sessions
#[test]
fn a_characters_world_state_outlives_reopening_the_database_and_goes_with_it() {
    let file = TempFile(std::env::temp_dir().join(format!(
        "yalain-store-world-test-{}.sqlite",
        std::process::id()
    )));
    let _ = std::fs::remove_file(&file.0);
    let id = {
        let mut s = SqliteStore::open(&file.0).expect("opens");
        let a = s.create_account("Tester", None, 100).expect("made");
        let n = s.next_character_number().expect("number");
        s.insert_character(&character(n, a.id, "Babyfare"))
            .expect("kept");
        assert_eq!(
            s.world_state(n).expect("reads"),
            None,
            "nothing before the first entry"
        );
        s.set_world_state(
            n,
            &WorldState {
                instances: 1,
                ..WorldState::default()
            },
        )
        .expect("kept");
        s.set_world_state(n, &world()).expect("replaced");
        n
    };
    let mut s = SqliteStore::open(&file.0).expect("opens again");
    assert_eq!(s.world_state(id).expect("reads"), Some(world()));
    s.purge_character(id).expect("purged");
    assert_eq!(s.world_state(id).expect("reads"), None);

    let mut m = MemoryStore::default();
    m.set_world_state(7, &world()).expect("kept");
    assert_eq!(m.world_state(7).expect("reads"), Some(world()));
    m.purge_character(7).expect("purged");
    assert_eq!(m.world_state(7).expect("reads"), None);
}

/// A record the whole world keeps (the allegiances, the consignments) reads back the same after
/// the database is opened again, in both stores, and a second keeping replaces the first.
/// Behaviour: server.world.state-kept-between-sessions
#[test]
fn a_world_record_outlives_reopening_the_database() {
    let file = TempFile(std::env::temp_dir().join(format!(
        "yalain-store-test-records-{}.sqlite",
        std::process::id()
    )));
    let _ = std::fs::remove_file(&file.0);
    {
        let mut s = SqliteStore::open(&file.0).expect("opens");
        assert_eq!(s.world_record("allegiances").expect("reads"), None);
        s.set_world_record("allegiances", &[1, 2, 3]).expect("kept");
        s.set_world_record("allegiances", &[4, 5])
            .expect("kept again");
    }
    let mut s = SqliteStore::open(&file.0).expect("opens again");
    assert_eq!(
        s.world_record("allegiances").expect("reads"),
        Some(vec![4, 5])
    );
    assert_eq!(s.world_record("consignments").expect("reads"), None);
    let mut m = MemoryStore::default();
    m.set_world_record("allegiances", &[4, 5]).expect("kept");
    assert_eq!(
        m.world_record("allegiances").expect("reads"),
        Some(vec![4, 5])
    );
}

/// A store file kept before the Dereth Network was added: its earlier tables alone, holding one
/// account and one character.
fn earlier_store(path: &std::path::Path) {
    let db = rusqlite::Connection::open(path).expect("made");
    db.execute_batch(
        "CREATE TABLE account (id INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE COLLATE NOCASE,
             secret BLOB, created_at INTEGER NOT NULL);
         CREATE TABLE character (id INTEGER PRIMARY KEY, account INTEGER NOT NULL REFERENCES account (id),
             name TEXT NOT NULL UNIQUE COLLATE NOCASE, race INTEGER NOT NULL, sex INTEGER NOT NULL,
             class INTEGER NOT NULL, implement INTEGER NOT NULL, start_area INTEGER NOT NULL,
             entity_did INTEGER NOT NULL, physique BLOB NOT NULL, look BLOB NOT NULL,
             created_at INTEGER NOT NULL, deleted_at INTEGER, last_played INTEGER NOT NULL);
         CREATE TABLE counter (name TEXT PRIMARY KEY, value INTEGER NOT NULL);
         CREATE TABLE world_state (id INTEGER PRIMARY KEY REFERENCES character (id), position BLOB,
             instances INTEGER NOT NULL, qualities BLOB NOT NULL, options BLOB NOT NULL);
         CREATE TABLE play_state (id INTEGER PRIMARY KEY REFERENCES character (id), play BLOB NOT NULL);
         CREATE TABLE kept_place (id INTEGER NOT NULL REFERENCES character (id), kind INTEGER NOT NULL,
             position BLOB NOT NULL, PRIMARY KEY (id, kind));
         CREATE TABLE world_record (name TEXT PRIMARY KEY, value BLOB NOT NULL);
         INSERT INTO account VALUES (1, 'Tester', NULL, 100);
         INSERT INTO character VALUES (7, 1, 'Babyfare', 4, 8192, 4, 8, 1073741829, 1191186703,
             x'', x'', 100, NULL, 100);",
    )
    .expect("the earlier tables");
}

/// A database kept before the Dereth Network was added opens with every character off the
/// network and nothing blocked; a character put on the network and the operator's blocks are kept
/// across a restart, and forgetting the character takes it off.
/// Behaviour: server.network.setting-and-blocks-kept
#[test]
fn an_earlier_database_gains_the_networks_setting_and_blocks_and_keeps_them() {
    let file = TempFile(std::env::temp_dir().join(format!(
        "yalain-store-network-test-{}.sqlite",
        std::process::id()
    )));
    let _ = std::fs::remove_file(&file.0);
    earlier_store(&file.0);
    {
        let mut s = SqliteStore::open(&file.0).expect("an earlier file opens");
        assert_eq!(
            s.character(7).expect("reads").map(|c| c.name),
            Some("Babyfare".to_string()),
            "what it held is still there"
        );
        assert!(s.network_characters().expect("reads").is_empty());
        assert!(s.network_blocks().expect("reads").is_empty());
        s.set_network_on(7, true).expect("on");
        s.set_network_on(7, true).expect("on twice");
        s.set_network_block("coldeve", true).expect("blocked");
        s.set_network_block("mal@dreamweave", true)
            .expect("blocked");
        s.set_network_block("frost", true).expect("blocked");
        s.set_network_block("frost", false).expect("lifted");
    }
    let mut s = SqliteStore::open(&file.0).expect("opens again");
    assert_eq!(s.network_characters().expect("reads"), vec![7]);
    assert_eq!(
        s.network_blocks().expect("reads"),
        vec!["coldeve".to_string(), "mal@dreamweave".to_string()]
    );
    s.set_network_on(7, false).expect("off");
    assert!(s.network_characters().expect("reads").is_empty());
    s.set_network_on(7, true).expect("on again");
    s.purge_character(7).expect("purged");
    assert!(
        s.network_characters().expect("reads").is_empty(),
        "a character forgotten is off the network"
    );
}

/// Behaviour: server.network.setting-and-blocks-kept
#[test]
fn the_memory_store_keeps_the_networks_setting_and_blocks_alike() {
    let mut s = MemoryStore::default();
    assert!(s.network_characters().expect("reads").is_empty());
    s.set_network_on(3, true).expect("on");
    s.set_network_on(9, true).expect("on");
    s.set_network_on(9, false).expect("off");
    s.set_network_block("coldeve", true).expect("blocked");
    assert_eq!(s.network_characters().expect("reads"), vec![3]);
    assert_eq!(
        s.network_blocks().expect("reads"),
        vec!["coldeve".to_string()]
    );
    s.purge_character(3).expect("purged");
    assert!(s.network_characters().expect("reads").is_empty());
}

/// A database kept before the friends service (the Dereth Network's tables already in it) opens
/// with every list empty; a character's friends and ignored names are kept across a restart, a
/// name once in any case, and forgetting the character forgets its lists.
/// Behaviour: server.friends.lists-kept
#[test]
fn an_earlier_database_gains_the_friends_lists_and_keeps_them() {
    let file = TempFile(std::env::temp_dir().join(format!(
        "yalain-store-friends-test-{}.sqlite",
        std::process::id()
    )));
    let _ = std::fs::remove_file(&file.0);
    earlier_store(&file.0);
    rusqlite::Connection::open(&file.0)
        .expect("opens")
        .execute_batch(
            "CREATE TABLE network_character (id INTEGER PRIMARY KEY REFERENCES character (id));
             CREATE TABLE network_block (block TEXT PRIMARY KEY);
             INSERT INTO network_character VALUES (7);",
        )
        .expect("the network's tables");
    let listed = |list, entry: &str| Listed {
        character: 7,
        list,
        entry: entry.to_owned(),
    };
    {
        let mut s = SqliteStore::open(&file.0).expect("an earlier file opens");
        assert_eq!(s.network_characters().expect("reads"), vec![7]);
        assert!(s.network_lists().expect("reads").is_empty());
        assert_eq!(
            s.character_named("BABYFARE").expect("reads").map(|c| c.id),
            Some(7)
        );
        assert_eq!(s.character_named("Nobody").expect("reads"), None);
        s.set_listed(7, NetworkList::Friends, "Cara", true)
            .expect("kept");
        s.set_listed(7, NetworkList::Friends, "cara", true)
            .expect("kept once");
        s.set_listed(7, NetworkList::Friends, "Bob@coldeve", true)
            .expect("kept");
        s.set_listed(7, NetworkList::Ignored, "Mal@dreamweave", true)
            .expect("kept");
        s.set_listed(7, NetworkList::Ignored, "Dan", true)
            .expect("kept");
        s.set_listed(7, NetworkList::Ignored, "DAN", false)
            .expect("taken off in any case");
    }
    let mut s = SqliteStore::open(&file.0).expect("opens again");
    assert_eq!(
        s.network_lists().expect("reads"),
        vec![
            listed(NetworkList::Friends, "Bob@coldeve"),
            listed(NetworkList::Friends, "Cara"),
            listed(NetworkList::Ignored, "Mal@dreamweave"),
        ]
    );
    s.purge_character(7).expect("purged");
    assert!(
        s.network_lists().expect("reads").is_empty(),
        "a character forgotten forgets its lists"
    );
}

/// A rename changes every character's lists that hold the character, in any case, and no other: not
/// another world's character of that name, nor a list that holds the new name already but for
/// holding it once. Run on each store, and the file's survives reopening it.
fn a_rename_reaches_the_lists(s: &mut impl Store) {
    const OTHER: u64 = 8;
    let listed = |character, list, entry: &str| Listed {
        character,
        list,
        entry: entry.to_owned(),
    };
    s.insert_character(&character(7, 1, "Elara")).expect("made");
    s.insert_character(&character(OTHER, 1, "Cara"))
        .expect("made");
    for (who, list, entry) in [
        (OTHER, NetworkList::Friends, "elara"),
        (OTHER, NetworkList::Ignored, "Elara"),
        (OTHER, NetworkList::Ignored, "Renamed"),
        (OTHER, NetworkList::Friends, "Elara@coldeve"),
        (OTHER, NetworkList::Friends, "Elaran"),
        (7, NetworkList::Friends, "Cara"),
    ] {
        s.set_listed(who, list, entry, true).expect("kept");
    }
    s.rename_character(7, "Renamed").expect("renamed");
    assert_eq!(
        s.network_lists().expect("reads"),
        vec![
            listed(7, NetworkList::Friends, "Cara"),
            listed(OTHER, NetworkList::Friends, "Elara@coldeve"),
            listed(OTHER, NetworkList::Friends, "Elaran"),
            listed(OTHER, NetworkList::Friends, "Renamed"),
            listed(OTHER, NetworkList::Ignored, "Renamed"),
        ]
    );
    assert!(s.rename_character(99, "Nobody").is_err(), "no character");
    assert_eq!(s.network_lists().expect("reads").len(), 5, "nothing else");
}

/// Behaviour: server.friends.lists-follow-a-rename
#[test]
fn a_rename_changes_the_name_in_every_characters_lists_on_the_memory_store() {
    a_rename_reaches_the_lists(&mut MemoryStore::default());
}

/// Behaviour: server.friends.lists-follow-a-rename
#[test]
fn a_rename_changes_the_name_in_every_characters_lists_in_the_file() {
    let file = TempFile(std::env::temp_dir().join(format!(
        "yalain-store-test-rename-{}.sqlite",
        std::process::id()
    )));
    let _ = std::fs::remove_file(&file.0);
    {
        let mut s = SqliteStore::open(&file.0).expect("opens");
        s.create_account("Tester", Some(vec![1, 2, 3]), 100)
            .expect("made");
        a_rename_reaches_the_lists(&mut s);
    }
    let mut s = SqliteStore::open(&file.0).expect("opens again");
    let lists = s.network_lists().expect("reads");
    assert_eq!(lists.len(), 5);
    assert!(lists.iter().all(|l| l.entry != "Elara"), "{lists:?}");
}

/// Behaviour: server.friends.lists-kept
#[test]
fn the_memory_store_keeps_the_friends_lists_alike() {
    let mut s = MemoryStore::default();
    s.set_listed(3, NetworkList::Friends, "Cara", true)
        .expect("kept");
    s.set_listed(3, NetworkList::Friends, "CARA", true)
        .expect("kept once");
    s.set_listed(3, NetworkList::Ignored, "Bob@coldeve", true)
        .expect("kept");
    s.set_listed(4, NetworkList::Friends, "Ann", true)
        .expect("kept");
    s.set_listed(4, NetworkList::Friends, "ann", false)
        .expect("taken off");
    assert_eq!(
        s.network_lists().expect("reads"),
        vec![
            Listed {
                character: 3,
                list: NetworkList::Friends,
                entry: "Cara".into()
            },
            Listed {
                character: 3,
                list: NetworkList::Ignored,
                entry: "Bob@coldeve".into()
            },
        ]
    );
    s.purge_character(3).expect("purged");
    assert!(s.network_lists().expect("reads").is_empty());
}

/// An account made with no secret takes one, which reads back after the database is opened again,
/// in both stores; an account that does not exist takes none.
/// Behaviour: server.accounts.open-made-at-first-login-then-need-their-ticket
#[test]
fn an_accounts_secret_given_later_outlives_reopening_the_database() {
    let file = TempFile(std::env::temp_dir().join(format!(
        "yalain-store-test-secret-{}.sqlite",
        std::process::id()
    )));
    let _ = std::fs::remove_file(&file.0);
    let id = {
        let mut s = SqliteStore::open(&file.0).expect("opens");
        let a = s.create_account("Tester", None, 1).expect("made");
        s.set_account_secret(a.id, vec![7, 8]).expect("kept");
        assert!(
            s.set_account_secret(a.id + 1, vec![1]).is_err(),
            "no such account"
        );
        a.id
    };
    let mut s = SqliteStore::open(&file.0).expect("opens again");
    let a = s.account_by_name("tester").expect("reads").expect("there");
    assert_eq!((a.id, a.secret), (id, Some(vec![7, 8])));
    let mut m = MemoryStore::default();
    let a = m.create_account("Tester", None, 1).expect("made");
    m.set_account_secret(a.id, vec![7, 8]).expect("kept");
    assert!(
        m.set_account_secret(a.id + 1, vec![1]).is_err(),
        "no such account"
    );
    let a = m.account_by_name("TESTER").expect("reads").expect("there");
    assert_eq!(a.secret, Some(vec![7, 8]));
}

/// A character's save that fails part way (here the database refuses its kept places) keeps
/// nothing of itself: the save before it reads back whole, its places included. A purge that fails
/// part way likewise leaves the character and everything it kept.
/// Behaviour: server.world.a-save-is-whole-or-not-at-all
#[test]
fn a_save_or_purge_that_fails_part_way_leaves_the_one_before_it_whole() {
    let file = TempFile(std::env::temp_dir().join(format!(
        "yalain-store-whole-test-{}.sqlite",
        std::process::id()
    )));
    let _ = std::fs::remove_file(&file.0);
    let mut s = SqliteStore::open(&file.0).expect("opens");
    let a = s.create_account("Tester", None, 100).expect("made");
    let n = s.next_character_number().expect("number");
    s.insert_character(&character(n, a.id, "Babyfare"))
        .expect("kept");
    s.set_world_state(n, &world()).expect("kept");

    let other = rusqlite::Connection::open(&file.0).expect("a second connection");
    other
        .execute_batch(
            "CREATE TRIGGER refuse_places BEFORE INSERT ON kept_place \
             BEGIN SELECT RAISE(ABORT, 'refused'); END;
             CREATE TRIGGER refuse_purge BEFORE DELETE ON character \
             BEGIN SELECT RAISE(ABORT, 'refused'); END;",
        )
        .expect("triggers");
    let mut later = world();
    later.instances = 9;
    later.play = vec![2, 2, 2, 2];
    assert!(s.set_world_state(n, &later).is_err(), "the save fails");
    assert_eq!(
        s.world_state(n).expect("reads"),
        Some(world()),
        "the save before"
    );
    assert!(s.purge_character(n).is_err(), "the purge fails");
    assert_eq!(
        s.world_state(n).expect("reads"),
        Some(world()),
        "still kept"
    );
    assert!(s.character(n).expect("reads").is_some(), "still there");
}

/// A character the operator makes with a build, or brings from another world, is kept with its
/// state in one transaction: when the state fails part way, no character is left without it; when
/// both go, both read back.
/// Behaviour: server.operator.characters-exported-and-imported
#[test]
fn a_character_kept_with_its_state_is_kept_whole_or_not_at_all() {
    let file = temp("with-state");
    let mut s = SqliteStore::open(&file.0).expect("opens");
    let a = s.create_account("Tester", None, 100).expect("made");
    let n = s.next_character_number().expect("number");
    s.insert_character_with_state(&character(n, a.id, "Babyfare"), &world())
        .expect("kept");
    assert_eq!(s.world_state(n).expect("reads"), Some(world()));
    assert!(s.character(n).expect("reads").is_some());

    let other = rusqlite::Connection::open(&file.0).expect("a second connection");
    other
        .execute_batch(
            "CREATE TRIGGER refuse_places BEFORE INSERT ON kept_place \
             BEGIN SELECT RAISE(ABORT, 'refused'); END;",
        )
        .expect("a trigger");
    let m = s.next_character_number().expect("number");
    assert!(
        s.insert_character_with_state(&character(m, a.id, "Halfmade"), &world())
            .is_err(),
        "its places refused"
    );
    assert_eq!(
        s.character(m).expect("reads"),
        None,
        "no character without its state"
    );
    assert!(
        !s.name_taken("Halfmade").expect("reads"),
        "its name still free"
    );

    let mut mem = MemoryStore::default();
    let a = mem.create_account("Tester", None, 100).expect("made");
    mem.insert_character_with_state(&character(7, a.id, "Babyfare"), &world())
        .expect("kept");
    assert_eq!(mem.world_state(7).expect("reads"), Some(world()));
    assert!(mem
        .insert_character_with_state(&character(8, a.id, "babyfare"), &world())
        .is_err());
    assert_eq!(
        mem.world_state(8).expect("reads"),
        None,
        "nothing of a refused one"
    );
}

/// A temporary file of its own, with nothing of a run before at its place.
fn temp(name: &str) -> TempFile {
    let file = TempFile(
        std::env::temp_dir().join(format!("yalain-store-{name}-{}.sqlite", std::process::id())),
    );
    drop(TempFile(file.0.clone()));
    file
}

/// What a database file holds, read with a connection of its own: its schema version, whether it
/// has the friends lists' table, and its characters' ids.
fn holds(path: &std::path::Path) -> (i64, bool, Vec<i64>) {
    let db = rusqlite::Connection::open(path).expect("opens");
    let version = db
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .expect("version");
    let lists = db
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE name = 'network_list'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .expect("reads")
        > 0;
    let mut st = db
        .prepare("SELECT id FROM character ORDER BY id")
        .expect("reads");
    let ids = st
        .query_map([], |r| r.get(0))
        .expect("reads")
        .map(|r| r.expect("an id"))
        .collect();
    (version, lists, ids)
}

/// A database kept before the schema had versions is copied beside itself, whole and as it was,
/// before its schema is upgraded, as Empyrean backs up a database before a migration; one already
/// at this version, and a new one, are not copied.
/// Behaviour: server.world.database-backed-up-before-a-schema-upgrade
#[test]
fn a_database_is_backed_up_as_it_was_before_its_schema_is_upgraded() {
    let file = temp("backup");
    earlier_store(&file.0);
    drop(SqliteStore::open(&file.0).expect("an earlier file opens"));
    let kept = backups(&file.0);
    assert_eq!(kept.len(), 1, "one backup: {kept:?}");
    let name = kept[0]
        .file_name()
        .expect("named")
        .to_string_lossy()
        .into_owned();
    let stem = file
        .0
        .file_name()
        .expect("named")
        .to_string_lossy()
        .into_owned();
    assert!(
        name.starts_with(&format!("{stem}.backup-v0-v{SCHEMA_VERSION}-")),
        "named for the file and the versions: {name}"
    );
    assert_eq!(
        holds(&kept[0]),
        (0, false, vec![7]),
        "the backup is the file as it was: no version, no friends lists, its character"
    );
    assert_eq!(
        holds(&file.0),
        (SCHEMA_VERSION, true, vec![7]),
        "the file upgraded"
    );
    drop(SqliteStore::open(&file.0).expect("opens again"));
    assert_eq!(
        backups(&file.0).len(),
        1,
        "nothing to upgrade, nothing copied"
    );

    let new = temp("backup-new");
    drop(SqliteStore::open(&new.0).expect("a new file opens"));
    assert!(backups(&new.0).is_empty(), "a new file has nothing to keep");
    assert_eq!(holds(&new.0).0, SCHEMA_VERSION);
}

/// Only the newest three backups of a file are kept: a fourth upgrade removes the oldest.
/// Behaviour: server.world.database-backed-up-before-a-schema-upgrade
#[test]
fn only_the_newest_three_backups_of_a_database_are_kept() {
    let file = temp("backup-three");
    earlier_store(&file.0);
    let mut made = Vec::new();
    for _ in 0..4 {
        drop(SqliteStore::open(&file.0).expect("opens"));
        let all = backups(&file.0);
        made.push(all.last().expect("a backup").clone());
        rusqlite::Connection::open(&file.0)
            .expect("opens")
            .pragma_update(None, "user_version", 0)
            .expect("as before versions");
    }
    assert_eq!(KEEP_BACKUPS, 3);
    assert_eq!(
        backups(&file.0),
        made[1..],
        "the newest three, the oldest removed"
    );
}

/// The backup before an update: a sibling of the file named for the update, whatever the file's
/// schema, the newest three kept; putting it back removes what the file became, its write-ahead log
/// with it; and a file that is not there has no backup.
/// Behaviour: server.update.database-backup-and-restore
#[test]
fn an_updates_backup_is_named_for_it_and_puts_the_database_back() {
    let file = temp("update-backup");
    drop(SqliteStore::open(&file.0).expect("opens"));
    let backup = snapshot(&file.0, "update-v0.1.0-v0.2.0").expect("backed up");
    let name = backup
        .file_name()
        .expect("a name")
        .to_string_lossy()
        .into_owned();
    assert!(
        name.contains(".sqlite.backup-update-v0.1.0-v0.2.0-"),
        "{name}"
    );
    assert_eq!(backups(&file.0), std::slice::from_ref(&backup));
    assert_eq!(holds(&backup).0, SCHEMA_VERSION);

    rusqlite::Connection::open(&file.0)
        .expect("opens")
        .pragma_update(None, "user_version", 99)
        .expect("as a new program leaves it");
    let mut wal = file.0.as_os_str().to_owned();
    wal.push("-wal");
    std::fs::write(&wal, b"what the new program left").expect("a write-ahead log");
    restore(&backup, &file.0).expect("put back");
    assert!(!std::path::Path::new(&wal).exists(), "its log goes");
    assert_eq!(holds(&file.0).0, SCHEMA_VERSION);

    let mut made = vec![backup];
    for i in 0..3 {
        made.push(snapshot(&file.0, &format!("update-{i}")).expect("backed up"));
    }
    assert_eq!(backups(&file.0).len(), KEEP_BACKUPS, "the newest three");
    assert_eq!(
        made.iter().filter(|b| b.exists()).count(),
        KEEP_BACKUPS,
        "and no more on disk"
    );

    assert!(snapshot(&file.0.with_extension("gone"), "update").is_err());
}

/// A database a newer Yalain kept (a schema version above this build's) is refused, naming its
/// version, and left as it was, with no backup made.
/// Behaviour: server.world.database-backed-up-before-a-schema-upgrade
#[test]
fn a_database_a_newer_yalain_kept_is_refused_and_left_as_it_was() {
    let file = temp("newer");
    drop(SqliteStore::open(&file.0).expect("opens"));
    let newer = SCHEMA_VERSION + 1;
    rusqlite::Connection::open(&file.0)
        .expect("opens")
        .pragma_update(None, "user_version", newer)
        .expect("as a newer Yalain keeps it");
    let before = std::fs::read(&file.0).expect("reads");
    let e = SqliteStore::open(&file.0).expect_err("refused");
    assert!(e.0.contains(&format!("version {newer}")), "{e}");
    assert_eq!(std::fs::read(&file.0).expect("reads"), before, "untouched");
    assert!(backups(&file.0).is_empty());
}

/// The foreign keys hold, as Empyrean has them: no character without its account, no saved state
/// or list without its character; a purge goes, its rows first.
/// Behaviour: server.world.database-foreign-keys-hold
#[test]
fn no_character_is_kept_without_its_account_nor_its_state_without_it() {
    let file = temp("keys");
    let mut s = SqliteStore::open(&file.0).expect("opens");
    let a = s.create_account("Tester", None, 100).expect("made");
    assert!(
        s.insert_character(&character(8, a.id + 1, "Nobody"))
            .is_err(),
        "no character without its account"
    );
    assert!(
        s.set_world_state(98, &world()).is_err(),
        "no state without its character"
    );
    assert!(
        s.set_listed(98, NetworkList::Friends, "Cara", true)
            .is_err(),
        "no list without its character"
    );
    assert!(s.set_network_on(98, true).is_err());
    s.insert_character(&character(7, a.id, "Babyfare"))
        .expect("kept");
    s.set_world_state(7, &world()).expect("saved");
    s.set_listed(7, NetworkList::Friends, "Cara", true)
        .expect("listed");
    s.set_network_on(7, true).expect("on");
    s.purge_character(7).expect("a purge goes, its rows first");
    assert_eq!(s.character(7).expect("reads"), None);
}

/// The database is kept with a write-ahead log, as Empyrean keeps its own: a save is in the file
/// once made, read by another connection while the server holds it open, and is there after the
/// server stops without closing it (a crash).
/// Behaviour: server.world.database-write-ahead-log
#[test]
fn the_database_is_kept_with_a_write_ahead_log_and_a_save_outlives_a_crash() {
    let file = temp("wal");
    let mut s = SqliteStore::open(&file.0).expect("opens");
    let a = s.create_account("Tester", None, 100).expect("made");
    s.insert_character(&character(5, a.id, "Babyfare"))
        .expect("kept");
    s.set_world_state(5, &world()).expect("saved");
    let other = rusqlite::Connection::open(&file.0).expect("a second connection");
    let mode: String = other
        .pragma_query_value(None, "journal_mode", |r| r.get(0))
        .expect("reads");
    assert_eq!(mode, "wal");
    drop(other);
    // The server stops without closing the database: nothing is checkpointed.
    std::mem::forget(s);
    let mut again = SqliteStore::open(&file.0).expect("opens after the crash");
    assert_eq!(again.world_state(5).expect("reads"), Some(world()));
}

/// A file kept with access levels takes the admin flag in their place: the highest level (5) with a
/// secret is an admin; every level below it, and the highest with no secret, a player; and the levels
/// themselves are gone from the file; the
/// backup taken before the upgrade keeps them. A character's template is kept with it.
/// Behaviour: server.operator.access-levels-become-the-admin-flag
#[test]
fn a_file_with_access_levels_takes_the_admin_flag_in_their_place() {
    let file = temp("levels");
    earlier_store(&file.0);
    {
        let mut sql = SqliteStore::open(&file.0).expect("to the current version");
        sql.create_account("sentinel", None, 1).expect("made");
        sql.create_account("admin", Some(vec![1]), 1).expect("made");
        sql.create_account("above", Some(vec![1]), 1).expect("made");
        sql.create_account("unclaimed", None, 1).expect("made");
    }
    {
        // Put the file back as version 2 kept it, levels and all.
        let db = rusqlite::Connection::open(&file.0).expect("opens");
        db.execute_batch(
            "ALTER TABLE account DROP COLUMN admin;
             ALTER TABLE account DROP COLUMN admin_groups;
             ALTER TABLE account ADD COLUMN access INTEGER NOT NULL DEFAULT 0;
             UPDATE account SET access = 2 WHERE name = 'sentinel';
             UPDATE account SET access = 5 WHERE name = 'admin';
             UPDATE account SET access = 9 WHERE name = 'above';
             UPDATE account SET access = 5 WHERE name = 'unclaimed';
             PRAGMA user_version = 2;",
        )
        .expect("the earlier schema");
    }
    let mut sql = SqliteStore::open(&file.0).expect("an earlier file opens");
    let admin = |sql: &mut SqliteStore, name: &str| {
        sql.account_by_name(name)
            .expect("reads")
            .expect("kept")
            .admin
    };
    assert!(!admin(&mut sql, "tester"), "a player stays one");
    assert!(!admin(&mut sql, "sentinel"), "a level between is a player");
    assert!(admin(&mut sql, "admin"), "the highest level is an admin");
    assert!(admin(&mut sql, "above"), "a level above it too");
    assert!(
        !admin(&mut sql, "unclaimed"),
        "the highest with no secret is a player"
    );
    sql.set_character_entity(7, 0x4700_0531).expect("kept");
    assert!(
        sql.set_character_entity(99, 1).is_err(),
        "no such character"
    );
    assert_eq!(
        sql.character(7).expect("reads").expect("kept").entity_did,
        0x4700_0531
    );
    drop(sql);
    assert_eq!(holds(&file.0).0, SCHEMA_VERSION);
    let db = rusqlite::Connection::open(&file.0).expect("opens");
    let levels: i64 = db
        .query_row(
            "SELECT count(*) FROM pragma_table_info('account') WHERE name = 'access'",
            [],
            |r| r.get(0),
        )
        .expect("reads");
    assert_eq!(levels, 0, "the levels are gone");
    let backup = backups(&file.0).pop().expect("a backup");
    let old = rusqlite::Connection::open(&backup).expect("opens");
    let kept: i64 = old
        .query_row(
            "SELECT access FROM account WHERE name = 'sentinel'",
            [],
            |r| r.get(0),
        )
        .expect("the backup keeps the levels");
    assert_eq!(kept, 2);
    drop(old);
    std::fs::remove_file(&backup).expect("removed");
}

/// The operator's moderation is kept, in SQLite and in memory alike: an account's admin flag
/// and ban (with who made it, when it ends and why), each change at once, a ban lifted; a
/// character's gag with the seconds it has still to run, lifted, and forgotten with the character.
/// The accounts list by name without regard to case. A file kept before them gains them: every
/// account a player's, none banned, nobody gagged.
/// Behaviour: server.operator.moderation-kept
#[test]
fn the_operators_moderation_is_kept_and_an_earlier_file_gains_it() {
    let file = temp("moderation");
    earlier_store(&file.0);
    let mut sql = SqliteStore::open(&file.0).expect("an earlier file opens");
    let tester = sql
        .account_by_name("tester")
        .expect("reads")
        .expect("the earlier account");
    assert_eq!((tester.admin, tester.ban.clone()), (false, None));
    assert_eq!(sql.gag(7).expect("reads"), None);
    let ban = Ban {
        at: 1_000,
        until: 2_000,
        by: "CONSOLE".into(),
        reason: Some("spam".into()),
    };
    let mut memory = MemoryStore::default();
    memory.create_account("Tester", None, 100).expect("made");
    memory
        .insert_character(&character(7, 1, "Babyfare"))
        .expect("kept");
    for store in [&mut sql as &mut dyn Store, &mut memory] {
        store.create_account("alma", None, 5).expect("made");
        store.set_account_admin(1, true).expect("set");
        store
            .set_account_groups(1, &["leads".to_owned(), "support".to_owned()])
            .expect("set");
        store.set_account_ban(1, Some(&ban)).expect("banned");
        store.set_gag(7, Some(250.5)).expect("gagged");
        assert!(
            store.set_account_admin(99, true).is_err(),
            "no such account"
        );
        assert!(store.set_account_ban(99, None).is_err(), "no such account");
        let names: Vec<String> = store
            .accounts()
            .expect("reads")
            .into_iter()
            .map(|a| a.name)
            .collect();
        assert_eq!(names, ["alma", "Tester"], "by name, without regard to case");
    }
    drop(sql);
    let mut sql = SqliteStore::open(&file.0).expect("opens again");
    for store in [&mut sql as &mut dyn Store, &mut memory] {
        let a = store
            .account_by_name("TESTER")
            .expect("reads")
            .expect("kept");
        assert_eq!(a.groups, ["leads", "support"], "kept");
        assert_eq!((a.admin, a.ban), (true, Some(ban.clone())), "kept");
        assert_eq!(store.gag(7).expect("reads"), Some(250.5));
        store.set_account_ban(1, None).expect("lifted");
        store.set_gag(7, None).expect("ungagged");
        assert_eq!(
            store
                .account_by_name("tester")
                .expect("reads")
                .expect("kept")
                .ban,
            None
        );
        assert_eq!(store.gag(7).expect("reads"), None);
        store.set_gag(7, Some(10.0)).expect("gagged");
        store.purge_character(7).expect("purged");
        assert_eq!(store.gag(7).expect("reads"), None, "forgotten with it");
    }
}

/// Keeping several characters' states and records is all or nothing: a failure at the last of them
/// leaves the first as it was. One the store no longer has is left out, and does not stop the rest.
/// Behaviour: server.operator.a-save-that-fails-is-held-and-tried-again
#[test]
fn keeping_several_at_once_is_all_or_nothing() {
    let file = TempFile(
        std::env::temp_dir().join(format!("yalain-store-save-all-{}.db", std::process::id())),
    );
    let mut s = SqliteStore::open(&file.0).expect("opens");
    let account = s.create_account("one", None, 0).expect("made");
    for id in [7, 8] {
        s.insert_character(&character(id, account.id, &format!("c{id}")))
            .expect("kept");
    }
    let state = |n: u8| WorldState {
        qualities: vec![n],
        ..WorldState::default()
    };
    let (one, two) = (state(1), state(2));
    s.save_all(&[(7, &one), (8, &one), (99, &one)], &[("r", &[1])])
        .expect("kept, and the one it has not left out");
    // The file refuses the last write of the next.
    let other = rusqlite::Connection::open(&file.0).expect("opens");
    other
        .execute_batch(
            "CREATE TRIGGER full BEFORE UPDATE ON world_record \
             BEGIN SELECT RAISE(ABORT, 'database or disk is full'); END",
        )
        .expect("made");
    drop(other);
    assert!(s.save_all(&[(7, &two), (8, &two)], &[("r", &[2])]).is_err());
    for id in [7, 8] {
        assert_eq!(
            s.world_state(id).expect("reads").expect("kept").qualities,
            [1]
        );
    }
    assert_eq!(s.world_record("r").expect("reads"), Some(vec![1]));
    assert!(s.world_state(99).expect("reads").is_none());
}

/// Numbers set aside for the world's things are never set aside again, the file reopened or not.
/// Behaviour: server.inventory.item-ids-never-given-twice
#[test]
fn numbers_set_aside_for_things_outlive_reopening_the_database() {
    let file = TempFile(std::env::temp_dir().join(format!(
        "yalain-store-thing-numbers-{}.db",
        std::process::id()
    )));
    let mut s = SqliteStore::open(&file.0).expect("opens");
    assert_eq!(s.reserve_thing_numbers(100), Ok(1));
    assert_eq!(s.reserve_thing_numbers(10), Ok(101));
    drop(s);
    let mut s = SqliteStore::open(&file.0).expect("opens");
    assert_eq!(s.reserve_thing_numbers(1), Ok(111));
    let mut m = MemoryStore::default();
    assert_eq!(m.reserve_thing_numbers(100), Ok(1));
    assert_eq!(m.reserve_thing_numbers(1), Ok(101));
}
