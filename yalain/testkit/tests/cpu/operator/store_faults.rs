//! What the world does when its database fails. A character whose state cannot be read, whether the
//! database will not answer, the state is damaged or a newer build kept it, is refused its entry
//! with the client's own words and never saved over: "could not read" is never "new". A save that
//! fails is held and tried again, and everyone is told. All a pass of the world did is kept whole, a
//! trade's two sides among it, whatever instant the server dies at. A panic stops the world: what
//! whole events kept is written, and nothing half done or since.

use auberean_protocol::login::{CharacterRef, CharacterRefused, PlayCharacter};
use auberean_protocol::text::NarrowString;
use auberean_protocol::{queue, ClientMessage, ServerMessage};
use auberean_script::args::Arg;
use yalain_store::{SqliteStore, Store};
use yalain_testkit::faults::FaultStore;
use yalain_testkit::{test_rules, Harness};
use yalain_world::market::{Consignment, Market, RECORD};
use yalain_world::state::PlayState;
use yalain_world::ServerConfig;
use yalain_world::{guard, saves, trade};

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

/// The world's own records are never started empty by a read that failed: nothing is done with them
/// meanwhile, and what they held is there once the store answers.
/// Behaviour: server.operator.a-record-that-will-not-read-is-not-written-over
#[test]
fn a_record_that_will_not_read_is_not_written_over() {
    let market = Market {
        next_sale: 7,
        list: vec![Consignment {
            sale: 7,
            owner: 1,
            keeper: 2,
            catalog: 3,
            store: 4,
            item: 5,
            template: 6,
            price: 8.0,
            offered: 9,
            sold: 0,
            entered: 10.0,
        }],
        ..Market::default()
    }
    .to_bytes();
    let allegiances = yalain_world::allegiance::Allegiances {
        loaded: true,
        nodes: [(11, yalain_world::allegiance::Node::default())].into(),
        ..Default::default()
    }
    .to_bytes();
    let mut h = Harness::with(ServerConfig::default(), FaultStore::default(), test_rules());
    let store = &mut h.server.store;
    store.inner.set_world_record(RECORD, &market).expect("kept");
    let names = yalain_world::allegiance::RECORD;
    store
        .inner
        .set_world_record(names, &allegiances)
        .expect("kept");
    store.records_read_fail = true;
    // A player enters, which reads them both, and the world saves what it may.
    let (_, id) = h.player_in_world("one", "Elara");
    assert!(!h.server.market.loaded && !h.server.allegiances.loaded);
    assert!(!yalain_world::market::ensure(&mut h.server));
    yalain_world::allegiance::keep(&mut h.server);
    let world = world_of(&h, id).expect("in the world");
    let mine = h.server.players[&world].character;
    yalain_world::allegiance::earned(&mut h.server, &mut h.net, mine, 100);
    h.run(5);
    let store = &mut h.server.store;
    assert_eq!(
        store.inner.world_record(RECORD).expect("reads"),
        Some(market)
    );
    assert_eq!(
        store.inner.world_record(names).expect("reads"),
        Some(allegiances)
    );
    store.records_read_fail = false;
    assert!(yalain_world::market::ensure(&mut h.server));
    assert_eq!(h.server.market.list.len(), 1, "what they held");
    assert!(yalain_world::allegiance::ensure(&mut h.server));
    assert!(h.server.allegiances.nodes.contains_key(&11));
}

/// A record that does not read is copied aside for the operator before the world starts it empty,
/// and a second one does not take the first one's place.
/// Behaviour: server.operator.a-record-that-will-not-read-is-not-written-over
#[test]
fn a_damaged_record_is_copied_aside_before_the_world_writes_a_new_one() {
    let mut h = Harness::new();
    for rubbish in [[0xFFu8; 3], [0xFEu8; 3]] {
        h.server.market.loaded = false;
        h.server
            .store
            .set_world_record(RECORD, &rubbish)
            .expect("kept");
        assert!(yalain_world::market::ensure(&mut h.server));
        assert!(h.server.market.list.is_empty());
    }
    for (name, rubbish) in [
        ("consignments.unreadable", [0xFFu8; 3]),
        ("consignments.unreadable.2", [0xFEu8; 3]),
    ] {
        assert_eq!(
            h.server.store.world_record(name).expect("reads").as_deref(),
            Some(&rubbish[..])
        );
    }
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

/// A character that leaves while saves fail is not lost: what it did reaches the store once they
/// work.
/// Behaviour: server.operator.a-save-that-fails-is-held-and-tried-again
#[test]
fn a_character_leaving_while_saves_fail_is_kept_once_they_work() {
    let mut h = Harness::with(ServerConfig::default(), FaultStore::default(), test_rules());
    let (b, id, coins) = logged_off_with_coins(&mut h);
    assert_eq!(asks_to_enter(&mut h, b, id), None);
    let world = world_of(&h, id).expect("in the world");
    h.server.store.writes_fail = true;
    let more =
        yalain_world::inventory::give(&mut h.server, &mut h.net, world, COINS, 7).expect("given");
    h.run(2);
    yalain_world::world::leave(&mut h.server, &mut h.net, world);
    h.run(2);
    assert_eq!(h.server.saves.unsaved(), vec![id]);
    let st = |h: &mut Harness<FaultStore>| {
        let st = h.server.store.inner.world_state(id).expect("reads");
        let items = PlayState::from_bytes(&st.expect("kept").play)
            .expect("reads")
            .items;
        items.iter().map(|x| x.id).collect::<Vec<u64>>()
    };
    assert!(!st(&mut h).contains(&more), "not in the store yet");
    h.server.store.writes_fail = false;
    h.run_to(yalain_net::LocalTime(h.now.0 + saves::RETRY_SECONDS + 1.0));
    assert!(st(&mut h).contains(&more) && st(&mut h).contains(&coins));
    assert!(!h.server.saves.failing());
}

/// A character back in the world while saves fail comes back with what it had when it left, not
/// what the store last took.
/// Behaviour: server.operator.a-save-that-fails-is-held-and-tried-again
#[test]
fn a_character_back_in_the_world_while_saves_fail_has_what_it_did() {
    let mut h = Harness::with(ServerConfig::default(), FaultStore::default(), test_rules());
    let (b, id, _) = logged_off_with_coins(&mut h);
    assert_eq!(asks_to_enter(&mut h, b, id), None);
    let world = world_of(&h, id).expect("in the world");
    h.server.store.writes_fail = true;
    let more =
        yalain_world::inventory::give(&mut h.server, &mut h.net, world, COINS, 7).expect("given");
    h.run(2);
    h.bots[b].send(
        false,
        queue::LOGON,
        &ClientMessage::LogOff(CharacterRef { character: id }),
    );
    h.run_to(yalain_net::LocalTime(h.now.0 + 25.0));
    assert!(world_of(&h, id).is_none(), "out of the world");
    assert_eq!(asks_to_enter(&mut h, b, id), None);
    let world = world_of(&h, id).expect("back");
    assert!(h.server.players[&world]
        .play
        .items
        .iter()
        .any(|x| x.id == more));
}

fn notices<S: Store>(h: &Harness<S>, b: usize, from: usize) -> Vec<String> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::Notice(n) => n
                .text
                .literal
                .as_ref()
                .map(|t| String::from_utf16_lossy(&t.0).trim_end().to_owned()),
            _ => None,
        })
        .collect()
}

/// Saves failing are told once to each player in the world and to one entering, tried again only
/// every few seconds, and their working again is told too.
/// Behaviour: server.operator.a-save-that-fails-is-told-to-everyone
#[test]
fn saves_failing_are_told_to_everyone_and_so_is_their_working_again() {
    let mut h = Harness::with(ServerConfig::default(), FaultStore::default(), test_rules());
    let (a, ida) = h.player_in_world("one", "Elara");
    let from = h.bots[a].inbox.len();
    h.server.store.writes_fail = true;
    let world = world_of(&h, ida).expect("in the world");
    yalain_world::world::keep_player(&mut h.server, world);
    h.run(4);
    assert!(h.server.saves.failing());
    assert_eq!(notices(&h, a, from), vec![saves::CANNOT_SAVE]);
    let tried = h.server.store.failed_writes;
    // One who was not there is told as it enters. (Its account and character are made first.)
    h.server.store.writes_fail = false;
    let b = h.at_character_list("two");
    let idb = h.create_character(b, "two", "Torvald");
    h.server.store.writes_fail = true;
    yalain_world::world::keep_player(&mut h.server, world);
    h.run_to(yalain_net::LocalTime(h.now.0 + 0.2));
    let fromb = h.bots[b].inbox.len();
    h.enter_world(b, "two", idb);
    assert!(notices(&h, b, fromb).contains(&saves::CANNOT_SAVE.to_owned()));
    // Tried again every few seconds, not every pass.
    let at = h.now.0;
    let before = h.server.store.failed_writes;
    h.run_to(yalain_net::LocalTime(at + 3.0 * saves::RETRY_SECONDS));
    let again = h.server.store.failed_writes - before;
    assert!((2..=4).contains(&again), "{again} tries, after {tried}");
    let (from, fromb) = (h.bots[a].inbox.len(), h.bots[b].inbox.len());
    h.server.store.writes_fail = false;
    h.run_to(yalain_net::LocalTime(h.now.0 + saves::RETRY_SECONDS + 1.0));
    assert!(!h.server.saves.failing());
    assert_eq!(notices(&h, a, from), vec![saves::SAVING_AGAIN]);
    assert_eq!(notices(&h, b, fromb), vec![saves::SAVING_AGAIN]);
}

/// A server stopping while saves fail still holds every character, for its last tries.
/// Behaviour: server.operator.a-save-that-fails-is-held-and-tried-again
#[test]
fn a_world_stopped_while_saves_fail_holds_everyone_for_its_last_tries() {
    let mut h = Harness::with(ServerConfig::default(), FaultStore::default(), test_rules());
    let (_, id) = h.player_in_world("one", "Elara");
    h.server.store.writes_fail = true;
    yalain_world::shutdown::save_everyone(&mut h.server, &mut h.net);
    assert!(h.server.players.is_empty());
    assert_eq!(h.server.saves.unsaved(), vec![id]);
    assert!(saves::write_out(&mut h.server).is_err());
    assert_eq!(h.server.saves.unsaved(), vec![id], "held still");
    h.server.store.writes_fail = false;
    assert_eq!(saves::write_out(&mut h.server), Ok(()));
    assert!(h.server.saves.unsaved().is_empty());
}

/// Two players in a trade on the server's real store; the trade open, each offering an item; each
/// character and its offer.
fn trading(h: &mut Harness<SqliteStore>) -> ((usize, u64, u64), (usize, u64, u64)) {
    h.server.catalog = yalain_testkit::test_catalog();
    h.server.catalog.insert(yalain_testkit::test_template(
        COINS,
        0x1067,
        yalain_content::Props::default(),
    ));
    let (a, ida) = h.player_in_world("one", "Elara");
    let (b, idb) = h.player_in_world("two", "Torvald");
    let at = h.server.players[&world_of(h, ida).expect("in the world")].position;
    let wb = world_of(h, idb).expect("in the world");
    h.server
        .players
        .get_mut(&wb)
        .expect("in the world")
        .position = at;
    let wa = world_of(h, ida).expect("in the world");
    let bow = yalain_world::inventory::give(&mut h.server, &mut h.net, wa, COINS, 1).expect("made");
    let gold =
        yalain_world::inventory::give(&mut h.server, &mut h.net, wb, COINS, 9).expect("made");
    use trade::function as f;
    h.ask(a, f::OPEN, &[Arg::LongInt(idb)], 10);
    h.ask(b, f::OPEN, &[Arg::LongInt(ida)], 10);
    h.ask(a, f::OFFER, &[Arg::Int(1), Arg::LongInt(bow)], 5);
    h.ask(b, f::OFFER, &[Arg::Int(9), Arg::LongInt(gold)], 5);
    ((a, ida, bow), (b, idb, gold))
}

/// The items a kept character holds, in a store.
fn held_in(store: &mut SqliteStore, id: u64) -> Vec<u64> {
    let st = store.world_state(id).expect("reads").expect("kept");
    let play = PlayState::from_bytes(&st.play).expect("reads");
    play.items.iter().map(|x| x.id).collect()
}

/// A trade is one transaction. The server's log cut off at every one of the pages the trade wrote
/// (what a crash or a power cut can leave) opens cleanly and holds the trade done on both sides or
/// on neither: never one item with two holders, never one with none.
/// Behaviour: server.trade.kept-whole-across-a-crash
#[test]
fn a_server_dying_at_any_point_of_a_trades_save_keeps_it_whole() {
    let dir = yalain_testkit::faults::TempDir::new("trade");
    let path = dir.path().join("world.db");
    let store = SqliteStore::open(&path).expect("opens");
    let mut h = Harness::with(ServerConfig::default(), store, test_rules());
    let ((a, ida, bow), (b, idb, gold)) = trading(&mut h);
    h.ask(a, trade::function::ACCEPT, &[], 5);
    // Only what the trade writes is in the log from here.
    let other = rusqlite::Connection::open(&path).expect("opens");
    other
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
        .expect("all in the file");
    drop(other);
    h.ask(b, trade::function::ACCEPT, &[], 5);
    assert!(held_in(&mut h.server.store, idb).contains(&bow));
    let wal = std::fs::read(dir.path().join("world.db-wal")).expect("the log");
    let main = std::fs::read(&path).expect("the file");
    const HEADER: usize = 32;
    const FRAME: usize = 24 + 4096;
    let frames = wal.len().saturating_sub(HEADER) / FRAME;
    assert!(frames >= 1, "the trade wrote {frames} pages");
    let cut = yalain_testkit::faults::TempDir::new("trade-cut");
    let mut seen = (false, false);
    for n in 0..=frames {
        // Whole pages, and a page torn in half.
        for torn in [0, FRAME / 2] {
            let at = (HEADER + n * FRAME + torn).min(wal.len());
            let copy = cut.path().join(format!("cut-{n}-{torn}.db"));
            std::fs::write(&copy, &main).expect("copied");
            let log = cut.path().join(format!("cut-{n}-{torn}.db-wal"));
            std::fs::write(&log, &wal[..at]).expect("copied");
            let mut store = SqliteStore::open(&copy).expect("a crash's file opens");
            let (ha, hb) = (held_in(&mut store, ida), held_in(&mut store, idb));
            let done = hb.contains(&bow);
            assert_eq!(
                ha.contains(&bow),
                !done,
                "the bow has one holder, cut at {at}"
            );
            assert_eq!(
                ha.contains(&gold),
                done,
                "the gold goes with it, cut at {at}"
            );
            assert_eq!(
                hb.contains(&gold),
                !done,
                "the gold has one holder, cut at {at}"
            );
            seen = (seen.0 || !done, seen.1 || done);
        }
    }
    assert_eq!(seen, (true, true), "the cuts fall before and after it");
}

/// Saves failing as a trade ends: both sides are held, and both reach the store together.
/// Behaviour: server.trade.kept-whole-across-a-crash
#[test]
fn a_trade_ending_while_saves_fail_reaches_the_store_whole() {
    let dir = yalain_testkit::faults::TempDir::new("trade-fail");
    let path = dir.path().join("world.db");
    let store = SqliteStore::open(&path).expect("opens");
    let mut h = Harness::with(ServerConfig::default(), store, test_rules());
    let ((a, ida, bow), (b, idb, gold)) = trading(&mut h);
    // Another program has the write lock as the trade ends.
    let other = rusqlite::Connection::open(&path).expect("opens");
    other.execute_batch("BEGIN IMMEDIATE").expect("holds it");
    h.ask(a, trade::function::ACCEPT, &[], 2);
    h.ask(b, trade::function::ACCEPT, &[], 2);
    assert!(h.server.saves.failing());
    other.execute_batch("COMMIT").expect("lets go");
    let before = (
        held_in(&mut h.server.store, ida),
        held_in(&mut h.server.store, idb),
    );
    assert!(
        before.0.contains(&bow) && before.1.contains(&gold),
        "neither side yet"
    );
    h.run_to(yalain_net::LocalTime(h.now.0 + saves::RETRY_SECONDS + 1.0));
    assert!(!h.server.saves.failing());
    let (ha, hb) = (
        held_in(&mut h.server.store, ida),
        held_in(&mut h.server.store, idb),
    );
    assert!(ha.contains(&gold) && !ha.contains(&bow));
    assert!(hb.contains(&bow) && !hb.contains(&gold));
}

/// One client's character in the world with coins, kept, and the connection it's in the world on.
fn in_world_with_coins<S: Store>(h: &mut Harness<S>) -> (usize, u64, u64, yalain_net::ConnId) {
    let (b, id, coins) = logged_off_with_coins(h);
    assert_eq!(asks_to_enter(h, b, id), None);
    let world = world_of(h, id).expect("in the world");
    h.run(2);
    (b, id, coins, world)
}

fn items_kept(h: &mut Harness<FaultStore>, id: u64) -> Vec<u64> {
    let st = h.server.store.inner.world_state(id).expect("reads");
    let play = PlayState::from_bytes(&st.expect("kept").play).expect("reads");
    play.items.iter().map(|x| x.id).collect()
}

/// A character's log-off runs into a bug as its leaving ends, after it is kept; the world's stop
/// follows, as the server's loop does it.
fn a_log_off_panics(h: &mut Harness<FaultStore>, b: usize, id: u64) {
    h.server.store.last_played_panics = Some(id);
    h.bots[b].send(
        false,
        queue::LOGON,
        &ClientMessage::LogOff(CharacterRef { character: id }),
    );
    let got = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        for _ in 0..4000 {
            h.step();
        }
    }));
    let panic = got.expect_err("the bug was reached");
    h.server.store.last_played_panics = None;
    let clock = h.clock();
    let what = guard::words(panic.as_ref());
    guard::stopped(&mut h.server, &mut h.net, clock, &what);
}

/// A panic takes nothing that whole events kept: what saves failing were holding is still held, and
/// reaches the store when the stop writes it out. Without the guard the process dies with it.
/// Behaviour: server.operator.a-panic-keeps-what-whole-events-kept
#[test]
fn a_panic_while_saves_fail_still_writes_what_was_held() {
    let mut h = Harness::with(ServerConfig::default(), FaultStore::default(), test_rules());
    let (b, id, _, world) = in_world_with_coins(&mut h);
    h.server.store.writes_fail = true;
    let more =
        yalain_world::inventory::give(&mut h.server, &mut h.net, world, COINS, 7).expect("given");
    h.run(2);
    assert!(h.server.saves.failing());
    a_log_off_panics(&mut h, b, id);
    assert!(!items_kept(&mut h, id).contains(&more), "not yet written");
    h.server.store.writes_fail = false;
    saves::write_out(&mut h.server).expect("writes");
    assert!(
        items_kept(&mut h, id).contains(&more),
        "written by the stop"
    );
    assert!(h.server.saves.unsaved().is_empty());
}

/// What the piece of work that panicked had kept is dropped, whole pieces before it are not, and
/// no one's memory is saved after it: each character goes back to its last save.
/// Behaviour: server.operator.a-panic-saves-nothing-half-done
#[test]
fn what_a_panicking_piece_kept_never_reaches_the_store() {
    let mut h = Harness::with(ServerConfig::default(), FaultStore::default(), test_rules());
    let (b, id, _, world) = in_world_with_coins(&mut h);
    let (_, other) = h.player_in_world("two", "Bram");
    let other_world = world_of(&h, other).expect("in the world");
    h.run(2);
    let at = h
        .server
        .store
        .inner
        .world_state(id)
        .expect("reads")
        .expect("kept");
    let other_at = h
        .server
        .store
        .inner
        .world_state(other)
        .expect("reads")
        .expect("kept");
    // Memory no save has taken: the one whose log-off panics, and someone beside it.
    h.server
        .players
        .get_mut(&world)
        .expect("in the world")
        .position
        .origin[0] += 5.0;
    h.server
        .players
        .get_mut(&other_world)
        .expect("in the world")
        .position
        .origin[0] += 5.0;
    a_log_off_panics(&mut h, b, id);
    assert!(
        h.server.saves.unsaved().is_empty(),
        "the leaving's own keep is taken back: {:?}",
        h.server.saves.unsaved()
    );
    assert_eq!(
        yalain_world::shutdown::save_everyone(&mut h.server, &mut h.net),
        0,
        "no one is saved from memory"
    );
    saves::write_out(&mut h.server).expect("writes");
    let kept = h
        .server
        .store
        .inner
        .world_state(id)
        .expect("reads")
        .expect("kept");
    assert_eq!(kept.position, at.position, "as it was last kept");
    let kept = h
        .server
        .store
        .inner
        .world_state(other)
        .expect("reads")
        .expect("kept");
    assert_eq!(kept.position, other_at.position, "as it was last kept");
}

/// Everyone in the world reads why, and every connection ends rather than timing out.
/// Behaviour: server.operator.a-panic-is-told-and-the-world-stops
#[test]
fn a_panic_tells_everyone_and_ends_their_connections() {
    let mut h = Harness::with(ServerConfig::default(), FaultStore::default(), test_rules());
    let (b, id, _, _) = in_world_with_coins(&mut h);
    let (c, _) = h.player_in_world("two", "Bram");
    h.run(2);
    let from = h.bots[c].inbox.len();
    let open = h.net.connection_count();
    assert!(open >= 2, "{open} connections");
    a_log_off_panics(&mut h, b, id);
    assert_eq!(h.net.connection_count(), 0, "every connection is ended");
    assert!(h.server.panicked.is_some());
    h.run(4);
    assert_eq!(notices(&h, c, from), vec![guard::STOPPING.to_owned()]);
    assert!(
        h.bots[c].inbox[from..].iter().any(|r| matches!(
            r.message(),
            Some(ServerMessage::CharacterRefused(x)) if x.reason == CharacterRefused::SERVER_CRASH
        )),
        "its client goes to its disconnected screen"
    );
}

/// An event that was whole is kept though the next in its pass panics.
/// Behaviour: server.operator.a-panic-keeps-what-whole-events-kept
#[test]
fn a_whole_event_in_the_pass_of_a_panic_is_still_kept() {
    let mut h = Harness::with(ServerConfig::default(), FaultStore::default(), test_rules());
    let (a, id, coins, _) = in_world_with_coins(&mut h);
    let c = h.at_character_list("two");
    h.run(2);
    assert!(h.server.saves.unsaved().is_empty());
    let call = yalain_testkit::play::server_call(
        yalain_world::pack::function::TO_MONEY_BAG,
        &[
            Arg::LongInt(id),
            Arg::Int(0),
            Arg::Int(100),
            Arg::LongInt(coins),
        ],
    );
    h.bots[a].send(true, queue::WEENIE, &call);
    h.server.store.name_taken_panics = true;
    let got = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        h.create_character(c, "two", "Bram");
    }));
    let panic = got.expect_err("the bug was reached");
    let clock = h.clock();
    let what = guard::words(panic.as_ref());
    guard::stopped(&mut h.server, &mut h.net, clock, &what);
    assert_eq!(h.server.saves.unsaved(), vec![id], "the first event's keep");
}

/// A timer's work that was whole is kept though the next timer in its pass panics.
/// Behaviour: server.operator.a-panic-keeps-what-whole-events-kept
#[test]
fn a_whole_timer_in_the_pass_of_a_panic_is_still_kept() {
    let mut h = Harness::with(ServerConfig::default(), FaultStore::default(), test_rules());
    let (a, first, _, _) = in_world_with_coins(&mut h);
    let (b, second) = h.player_in_world("two", "Bram");
    h.run(2);
    assert!(h.server.saves.unsaved().is_empty());
    let off = |id| ClientMessage::LogOff(CharacterRef { character: id });
    h.bots[a].send(false, queue::LOGON, &off(first));
    a_log_off_panics(&mut h, b, second);
    assert_eq!(
        h.server.saves.unsaved(),
        vec![first],
        "the first leaving's keep"
    );
}

/// A character the world has no start place for (a row a client's creation can't make) is refused
/// at the door and stays at its list; it doesn't hang its client.
/// Behaviour: server.operator.a-character-with-no-start-place-is-refused
#[test]
fn a_character_with_no_start_place_is_refused_and_its_client_can_go_on() {
    let mut h = Harness::with(ServerConfig::default(), FaultStore::default(), test_rules());
    let (b, id, _) = logged_off_with_coins(&mut h);
    let mut c = h
        .server
        .store
        .inner
        .character(id)
        .expect("reads")
        .expect("made");
    c.id = id + 1;
    c.name = "Raceless".to_owned();
    c.race = 0;
    h.server.store.inner.insert_character(&c).expect("written");
    assert_eq!(
        asks_to_enter(&mut h, b, c.id),
        Some(CharacterRefused::COULD_NOT_PLACE)
    );
    assert!(world_of(&h, c.id).is_none(), "it did not enter");
    assert!(h
        .server
        .store
        .inner
        .world_state(c.id)
        .expect("reads")
        .is_none_or(|st| st.position.is_none()));
    assert_eq!(asks_to_enter(&mut h, b, id), None, "another enters");
    assert!(world_of(&h, id).is_some());
}
