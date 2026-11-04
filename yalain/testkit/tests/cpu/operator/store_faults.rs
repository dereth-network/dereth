//! What the world does when its database fails. A character whose state cannot be read, whether the
//! database will not answer, the state is damaged or a newer build kept it, is refused its entry
//! with the client's own words and never saved over: "could not read" is never "new".

use auberean_protocol::login::{CharacterRef, CharacterRefused, PlayCharacter};
use auberean_protocol::text::NarrowString;
use auberean_protocol::{queue, ClientMessage, ServerMessage};
use yalain_store::{SqliteStore, Store};
use yalain_testkit::faults::FaultStore;
use yalain_testkit::{test_rules, Harness};
use yalain_world::ServerConfig;

const COINS: u32 = 0x4700_0538;

fn world_of<S: Store>(h: &Harness<S>, id: u64) -> Option<yalain_net::ConnId> {
    h.server
        .players
        .iter()
        .find(|(_, p)| p.character == id)
        .map(|(c, _)| *c)
}

/// A player with coins, logged off, at its character list; the client, the character and the coins.
fn logged_off_with_coins<S: Store>(h: &mut Harness<S>) -> (usize, u64, u64) {
    h.server.catalog = yalain_testkit::test_catalog();
    h.server.catalog.insert(yalain_testkit::test_template(
        COINS,
        0x1067,
        yalain_content::Props::default(),
    ));
    let (b, id) = h.player_in_world("one", "Elara");
    let world = world_of(h, id).expect("in the world");
    let coins =
        yalain_world::inventory::give(&mut h.server, &mut h.net, world, COINS, 500).expect("given");
    h.run(10);
    h.bots[b].send(
        false,
        queue::LOGON,
        &ClientMessage::LogOff(CharacterRef { character: id }),
    );
    let t = h.now.0 + 25.0;
    h.run_to(yalain_net::LocalTime(t));
    assert!(world_of(h, id).is_none(), "out of the world");
    (b, id, coins)
}

/// Ask to enter; the reason it is refused with, if it is.
fn asks_to_enter<S: Store>(h: &mut Harness<S>, b: usize, id: u64) -> Option<u32> {
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        false,
        queue::LOGON,
        &ClientMessage::PlayCharacter(PlayCharacter {
            character: id,
            account: NarrowString::from("one"),
        }),
    );
    h.run(40);
    h.bots[b].inbox[from..]
        .iter()
        .find_map(|r| match r.message() {
            Some(ServerMessage::CharacterRefused(c)) => Some(c.reason),
            _ => None,
        })
}

/// While the database will not read the character's state it is refused ("There has been an
/// error."), stays at its list, and enters with all it had once the database reads again.
/// Behaviour: server.operator.a-state-that-will-not-read-refuses-entry
#[test]
fn a_database_that_will_not_read_refuses_the_entry_and_loses_nothing() {
    let mut h = Harness::with(ServerConfig::default(), FaultStore::default(), test_rules());
    let (b, id, coins) = logged_off_with_coins(&mut h);
    let kept = h.server.store.inner.world_state(id).expect("reads");
    h.server.store.state_reads_fail = true;
    assert_eq!(asks_to_enter(&mut h, b, id), Some(CharacterRefused::ERROR));
    assert!(world_of(&h, id).is_none(), "it did not enter");
    assert_eq!(
        h.server.store.inner.world_state(id).expect("reads"),
        kept,
        "nothing written"
    );
    h.server.store.state_reads_fail = false;
    assert_eq!(asks_to_enter(&mut h, b, id), None, "its entry goes on");
    let world = world_of(&h, id).expect("in the world again");
    let holds = &h.server.players[&world].play.items;
    assert!(holds.iter().any(|x| x.id == coins), "with what it had");
}

/// A database that fails even to find the character says so, not that the character is someone
/// else's.
/// Behaviour: server.operator.a-state-that-will-not-read-refuses-entry
#[test]
fn a_database_that_cannot_find_the_character_refuses_with_an_error() {
    let mut h = Harness::with(ServerConfig::default(), FaultStore::default(), test_rules());
    let (b, id, _) = logged_off_with_coins(&mut h);
    h.server.store.reads_fail = true;
    assert_eq!(asks_to_enter(&mut h, b, id), Some(CharacterRefused::ERROR));
}

/// A kept part changed by `damage`; the state as it now stands in the store.
fn damaged<S: Store>(h: &mut Harness<S>, id: u64, damage: fn(&mut yalain_store::WorldState)) {
    let mut st = h
        .server
        .store
        .world_state(id)
        .expect("reads")
        .expect("kept");
    damage(&mut st);
    h.server.store.set_world_state(id, &st).expect("kept");
}

/// A kept state that does not read is refused ("This character is corrupt..."), and its bytes stay,
/// through the refusal and the world's saves after it, whichever part is damaged.
/// Behaviour: server.operator.a-state-that-will-not-read-refuses-entry
#[test]
fn a_damaged_state_refuses_the_entry_and_its_bytes_stay() {
    let parts: [fn(&mut yalain_store::WorldState); 3] = [
        |st| st.play.truncate(st.play.len() / 2),
        |st| st.qualities.truncate(3),
        |st| st.options.truncate(3),
    ];
    for damage in parts {
        let mut h = Harness::new();
        let (b, id, _) = logged_off_with_coins(&mut h);
        damaged(&mut h, id, damage);
        let kept = h.server.store.world_state(id).expect("reads");
        assert_eq!(
            asks_to_enter(&mut h, b, id),
            Some(CharacterRefused::CORRUPT_CHARACTER)
        );
        let t = h.now.0 + 400.0;
        h.run_to(yalain_net::LocalTime(t));
        assert!(world_of(&h, id).is_none(), "it did not enter");
        assert_eq!(h.server.store.world_state(id).expect("reads"), kept);
    }
}

/// A state a newer build kept (the world went back to an older build) is refused as an old
/// character is, and its bytes stay for the newer build to read.
/// Behaviour: server.operator.a-state-that-will-not-read-refuses-entry
#[test]
fn a_state_a_newer_build_kept_refuses_the_entry_and_its_bytes_stay() {
    let mut h = Harness::new();
    let (b, id, _) = logged_off_with_coins(&mut h);
    damaged(&mut h, id, |st| {
        st.play[..4].copy_from_slice(&u32::MAX.to_le_bytes());
    });
    let kept = h.server.store.world_state(id).expect("reads");
    assert_eq!(
        asks_to_enter(&mut h, b, id),
        Some(CharacterRefused::OLD_CHARACTER)
    );
    assert!(world_of(&h, id).is_none(), "it did not enter");
    assert_eq!(h.server.store.world_state(id).expect("reads"), kept);
}

/// An entry refused at the world connection itself (the state went bad after the referral): back
/// at the character list, where it may ask again.
/// Behaviour: server.operator.a-state-that-will-not-read-refuses-entry
#[test]
fn a_state_gone_bad_after_the_referral_goes_back_to_the_list() {
    let mut h = Harness::with(ServerConfig::default(), FaultStore::default(), test_rules());
    let (b, id, coins) = logged_off_with_coins(&mut h);
    // The referral goes out, and the state fails as the world connection comes up.
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        false,
        queue::LOGON,
        &ClientMessage::PlayCharacter(PlayCharacter {
            character: id,
            account: NarrowString::from("one"),
        }),
    );
    h.server.store.state_reads_after = Some(1);
    h.run(40);
    assert!(h.bots[b].inbox[from..].iter().any(|r| matches!(
        r.message(),
        Some(ServerMessage::CharacterRefused(c)) if c.reason == CharacterRefused::ERROR
    )));
    assert!(world_of(&h, id).is_none());
    h.server.store.state_reads_after = None;
    h.server.store.state_reads_fail = false;
    assert_eq!(asks_to_enter(&mut h, b, id), None);
    let world = world_of(&h, id).expect("in the world again");
    assert!(h.server.players[&world]
        .play
        .items
        .iter()
        .any(|x| x.id == coins));
}

/// On the store the server runs, another program holding the file's write lock does not stop a read
/// (its log keeps readers going): the character enters whole, never new, though the store waits out
/// its busy time on the entry's own save.
/// Behaviour: server.operator.a-state-that-will-not-read-refuses-entry
#[test]
fn a_database_file_locked_by_another_program_still_gives_the_character_whole() {
    let dir = yalain_testkit::faults::TempDir::new("locked");
    let path = dir.path().join("world.db");
    let store = SqliteStore::open(&path).expect("opens");
    let mut h = Harness::with(ServerConfig::default(), store, test_rules());
    let (b, id, coins) = logged_off_with_coins(&mut h);
    let other = rusqlite::Connection::open(&path).expect("opens");
    other.execute_batch("BEGIN EXCLUSIVE").expect("holds it");
    assert_eq!(asks_to_enter(&mut h, b, id), None, "its entry goes on");
    let world = world_of(&h, id).expect("in the world");
    let holds = &h.server.players[&world].play.items;
    assert!(holds.iter().any(|x| x.id == coins), "with what it had");
    drop(other);
}

/// On the store the server runs, a file damaged on disk gives an error, never an empty answer: the
/// entry is refused and the file is not written.
/// Behaviour: server.operator.a-state-that-will-not-read-refuses-entry
#[test]
fn a_database_file_damaged_on_disk_refuses_the_entry_and_is_not_written() {
    let dir = yalain_testkit::faults::TempDir::new("damaged");
    let path = dir.path().join("world.db");
    let store = SqliteStore::open(&path).expect("opens");
    let mut h = Harness::with(ServerConfig::default(), store, test_rules());
    let (b, id, _) = logged_off_with_coins(&mut h);
    let other = rusqlite::Connection::open(&path).expect("opens");
    other
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
        .expect("all in the file");
    drop(other);
    // The first page, which says what the file is, stays; every other is rubbish.
    let mut file = std::fs::read(&path).expect("reads");
    let page = 4096;
    assert!(file.len() > page, "more than its first page");
    file[page..].fill(0xA5);
    std::fs::write(&path, &file).expect("writes");
    h.server.store = SqliteStore::open(&path).expect("its first page reads");
    assert_eq!(asks_to_enter(&mut h, b, id), Some(CharacterRefused::ERROR));
    assert!(world_of(&h, id).is_none(), "it did not enter");
    drop(h);
    assert_eq!(std::fs::read(&path).expect("reads"), file, "not written");
}
