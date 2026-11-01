//! Characters and accounts kept and read back. Each test uses its own file in the temporary
//! folder, removed at the end.

use yalain_store::{
    Character, Listed, MemoryStore, NetworkList, SqliteStore, Store, StoredPosition, WorldState,
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
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
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
