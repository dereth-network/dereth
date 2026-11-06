//! The data cache's requests over a portal file in memory: gets formed once per record and kept,
//! the records each names asked for in turn, the worker's queue run each time the cache is given
//! time, the server asked for what is in neither memory nor the files, and askers told.

use std::sync::Arc;

use auberean_assets::requests::{
    Batch, Event, Listener, OnFinished, Outcome, Requests, Sources, StateRecords, GATHER_NONE,
};
use auberean_assets::script::ScriptState;
use auberean_assets::tables::IdMap;
use auberean_assets::{encode, Database};
use auberean_dat::disk::controller::OPEN_CREATE;
use auberean_dat::disk::{
    DatStorageMut, Files, MemoryFile, OpenMode, OpenSpec, Opened, SharedStore, StoreSet,
};
use auberean_dat::{DatFile, QualifiedId, QualifiedIds, RecordType};
use auberean_primitives::DataId;

/// The gathering a record's needed records are asked for by.
const DEPS: u32 = 0x4000_0007;

const L: Listener = Listener { to: 7, user: 99 };

struct Mem(MemoryFile);

impl Files for Mem {
    fn open(&mut self, _name: &str, mode: OpenMode) -> Option<Box<dyn DatStorageMut>> {
        if mode == OpenMode::Create {
            self.0 = MemoryFile::default();
        }
        Some(Box::new(self.0.clone()))
    }
}

fn q(n: u32) -> QualifiedId {
    QualifiedId {
        id: RecordType::ID_MAP.base_id() + n,
        kind: RecordType::ID_MAP.0,
    }
}

/// An id map naming the maps `names`.
fn map(names: &[u32]) -> IdMap {
    IdMap {
        ids: names
            .iter()
            .enumerate()
            .map(|(k, n)| (u32::try_from(k).unwrap(), DataId(q(*n).id)))
            .collect(),
        ..IdMap::default()
    }
}

/// A portal file in memory holding maps 1 (naming 2 and 3), 2 (naming 4), 3 and 4, and the
/// file as a database reads it.
fn portal() -> (StoreSet, Arc<DatFile>) {
    portal_with(&[])
}

/// [`portal`], with these records too.
fn portal_with(extra: &[(u32, Vec<u8>)]) -> (StoreSet, Arc<DatFile>) {
    let s = SharedStore::new(Box::new(|| 0x5000_0000));
    let mut spec = OpenSpec::new(
        "f",
        "",
        1,
        0,
        OPEN_CREATE,
        0x400 + 256 * 0x100,
        0x100,
        -1,
        -1,
        false,
    );
    let mut mem = Mem(MemoryFile::default());
    assert_eq!(s.init_file(&mut mem, &mut spec), Opened::Ready);
    let s = Arc::new(s);
    for (n, names) in [(1, &[2, 3][..]), (2, &[4][..]), (3, &[][..]), (4, &[][..])] {
        assert!(s.save_data(q(n).id, &encode(q(n).id, &map(names))));
    }
    for (id, bytes) in extra {
        assert!(s.save_data(*id, bytes));
    }
    let dat = Arc::new(DatFile::from_bytes(mem.0.bytes()).expect("a container"));
    let set = StoreSet {
        by_id: [(s.dat_file_id(), s.clone())].into_iter().collect(),
        by_slot: [Some(s), None, None, None],
    };
    (set, dat)
}

struct Fixture {
    db: Database,
    files: StoreSet,
}

impl Fixture {
    fn new() -> Self {
        let (files, dat) = portal();
        Self {
            db: Database::new(vec![dat]),
            files,
        }
    }

    fn src(&self) -> Sources<'_> {
        Sources::new(&self.db, &self.files)
    }
}

fn finished(n: u32, outcome: Outcome, times: u32) -> Event {
    Event::Finished {
        to: L.to,
        record: q(n),
        outcome,
        user: L.user,
        times,
    }
}

fn done(batch: Batch, outcome: Outcome) -> Event {
    Event::BatchDone {
        to: L.to,
        batch,
        outcome,
        user: L.user,
    }
}

/// Behaviour: data.requests.a-get-waits-for-the-records-it-names
#[test]
fn a_get_reads_its_record_then_the_records_it_names_and_its_batch_is_told_when_all_are_in() {
    let f = Fixture::new();
    let mut r = Requests::new();
    let batch = r.async_get(f.src(), DEPS, q(1), Some(L));
    assert_eq!(batch, Batch(1));
    assert_eq!(r.queued(), 1, "on disk: read by the worker");
    assert!(r.out.is_empty());
    // Frame 1: map 1 read and decoded; the maps it names, 2 and 3, are formed and queued.
    r.use_time(f.src(), 1.0);
    assert!(r.out.is_empty());
    assert_eq!(r.pending(), vec![q(1), q(2), q(3)]);
    assert_eq!(r.get_state(q(1)), Some((Outcome::Success, 2)));
    // Frame 2: 2 and 3 read; 2 names 4.
    r.use_time(f.src(), 2.0);
    assert!(r.out.is_empty());
    assert_eq!(r.get_state(q(1)), Some((Outcome::Success, 1)));
    // Frame 3: 4 read, 2 done, so 1 is: its asker is told of it, then of the batch.
    r.use_time(f.src(), 3.0);
    assert_eq!(
        r.out,
        vec![
            finished(1, Outcome::Success, 1),
            done(batch, Outcome::Success)
        ]
    );
    assert_eq!(r.get_state(q(4)), Some((Outcome::Success, 0)));
    // Let go, every get goes, and the records the gets decoded alone with them.
    r.release_batch(batch);
    assert!(r.pending().is_empty());
    assert_eq!(r.live(), 0);
    assert_eq!(r.live_batches(), 0);
    assert!(f.db.get_if_loaded::<IdMap>(q(2).id).is_none());
}

/// Behaviour: data.requests.one-request-per-record
#[test]
fn a_second_asker_shares_the_kept_request_and_one_already_finished_answers_at_once() {
    let f = Fixture::new();
    let mut r = Requests::new();
    let a = r.async_get(f.src(), GATHER_NONE, q(3), Some(L));
    let b = r.async_get(f.src(), GATHER_NONE, q(3), Some(L));
    assert_ne!(a, b);
    assert_eq!(r.queued(), 1, "one read for both");
    r.use_time(f.src(), 1.0);
    assert_eq!(
        r.out,
        vec![
            finished(3, Outcome::Success, 1),
            finished(3, Outcome::Success, 1),
            done(a, Outcome::Success),
            done(b, Outcome::Success),
        ]
    );
    r.out.clear();
    // Finished and kept: a third asker is told at once, its batch the next time.
    let c = r.async_get(f.src(), GATHER_NONE, q(3), Some(L));
    assert_eq!(r.out, vec![finished(3, Outcome::Success, 1)]);
    assert_eq!(r.queued(), 0);
    r.use_time(f.src(), 2.0);
    assert_eq!(r.out.last(), Some(&done(c, Outcome::Success)));
    for x in [a, b, c] {
        r.release_batch(x);
    }
    assert_eq!((r.live(), r.live_batches()), (0, 0));
    // Without a listener, nobody is told, and the get stays kept.
    r.out.clear();
    assert_eq!(r.async_get(f.src(), GATHER_NONE, q(4), None), Batch::NONE);
    r.use_time(f.src(), 3.0);
    assert!(r.out.is_empty());
    assert_eq!(r.get_state(q(4)), Some((Outcome::Success, 0)));
}

/// Behaviour: data.requests.missing-records-are-asked-of-the-server
#[test]
fn a_record_in_neither_memory_nor_the_files_is_asked_of_the_server_and_its_answer_finishes_it() {
    let f = Fixture::new();
    let mut r = Requests::new();
    r.server = true;
    let got = r.async_get(f.src(), GATHER_NONE, q(5), Some(L));
    let refused = r.async_get(f.src(), GATHER_NONE, q(6), Some(L));
    assert_eq!(r.out, vec![Event::Ask(q(5)), Event::Ask(q(6))]);
    assert_eq!(r.get_state(q(5)), Some((Outcome::Unknown, 1)));
    r.out.clear();
    r.on_saved(f.src(), q(5), Some(encode(q(5).id, &map(&[]))));
    r.on_refused(f.src(), q(6));
    r.use_time(f.src(), 1.0);
    assert_eq!(
        r.out,
        vec![
            finished(5, Outcome::Success, 1),
            finished(6, Outcome::Failure, 1),
            done(got, Outcome::Success),
            done(refused, Outcome::Failure),
        ]
    );
    // Not connected: the server is not asked, and the get fails at once; with a record found
    // after it in the same batch, the batch is a partial success.
    let mut off = Requests::new();
    let both = off.async_get_many(f.src(), GATHER_NONE, &[q(7), q(3)], Some(L));
    assert_eq!(off.out, vec![finished(7, Outcome::Failure, 1)]);
    off.use_time(f.src(), 1.0);
    assert_eq!(
        &off.out[1..],
        &[
            finished(3, Outcome::Success, 1),
            done(both, Outcome::Partial)
        ]
    );
    // One asker letting go while another still waits leaves the get kept for the other.
    r.out.clear();
    let first = r.async_get(f.src(), GATHER_NONE, q(9), Some(L));
    let second = r.async_get(f.src(), GATHER_NONE, q(9), Some(L));
    r.release_batch(first);
    assert!(r.pending().contains(&q(9)));
    r.on_saved(f.src(), q(9), Some(encode(q(9).id, &map(&[]))));
    r.use_time(f.src(), 2.0);
    assert_eq!(
        &r.out[1..],
        &[
            finished(9, Outcome::Success, 1),
            done(second, Outcome::Success)
        ]
    );
}

/// Behaviour: data.requests.a-get-waits-for-the-records-it-names
#[test]
fn a_named_record_that_fails_makes_the_get_and_its_batch_a_partial_success() {
    let f = Fixture::new();
    assert!(f.files.purge_data(q(4)));
    let mut r = Requests::new();
    let b = r.async_get_many(f.src(), DEPS, &[q(2), q(3)], Some(L));
    for t in 1..=3 {
        r.use_time(f.src(), f64::from(t));
    }
    assert_eq!(
        r.out,
        vec![
            finished(2, Outcome::Partial, 1),
            finished(3, Outcome::Success, 1),
            done(b, Outcome::Partial),
        ]
    );
    // A get naming records whose gets are kept and finished counts them at once: map 1 names 2,
    // a partial success, so it is one too.
    r.out.clear();
    let one = r.async_get(f.src(), DEPS, q(1), Some(L));
    r.use_time(f.src(), 4.0);
    assert_eq!(
        r.out,
        vec![
            finished(1, Outcome::Partial, 1),
            done(one, Outcome::Partial)
        ]
    );
}

/// Behaviour: data.requests.records-in-memory-are-taken-from-their-cache
#[test]
fn a_record_in_memory_finishes_at_once_and_one_got_and_kept_asks_for_none_it_names() {
    let f = Fixture::new();
    let mut r = Requests::new();
    // Read through the requests' own blocking get: kept, its named records counted loaded.
    let held = r.blocking_get(f.src(), DEPS, q(1)).expect("on disk");
    assert_eq!(held.id(), q(1).id);
    let typed = held.downcast::<IdMap>().expect("an id map");
    assert_eq!(typed.ids.len(), 2);
    let b = r.async_get(f.src(), DEPS, q(1), Some(L));
    assert_eq!(r.out, vec![finished(1, Outcome::Success, 1)]);
    assert_eq!(
        r.pending(),
        vec![q(1)],
        "none of its named records asked for"
    );
    r.release_batch(b);
    // Got for no gathering, the record still asks for its named records the next time.
    let mut r = Requests::new();
    let f = Fixture::new();
    drop(r.blocking_get(f.src(), GATHER_NONE, q(1)).expect("on disk"));
    r.async_get(f.src(), DEPS, q(1), Some(L));
    assert_eq!(r.pending(), vec![q(1), q(2), q(3)]);
    // Got through the database, as every record the game reads is: none asked for.
    let mut r = Requests::new();
    let f = Fixture::new();
    let _held = f.db.get::<IdMap>(q(1).id).expect("in the file");
    r.async_get(f.src(), DEPS, q(1), Some(L));
    assert_eq!(r.pending(), vec![q(1)]);
}

/// Behaviour: data.requests.a-waiting-get-takes-a-kept-gets-record
#[test]
fn a_waiting_get_takes_the_record_a_kept_get_decoded_into_its_cache() {
    let f = Fixture::new();
    let mut r = Requests::new();
    let b = r.async_get(f.src(), GATHER_NONE, q(3), Some(L));
    r.use_time(f.src(), 1.0);
    assert!(
        f.db.get_if_loaded::<IdMap>(q(3).id).is_none(),
        "held by the get alone"
    );
    let o = r
        .blocking_get(f.src(), DEPS, q(3))
        .expect("the get's record");
    assert!(f.db.get_if_loaded::<IdMap>(q(3).id).is_some());
    r.release_batch(b);
    drop(o);
    assert_eq!(f.db.cached(RecordType::ID_MAP.0), (1, 1), "kept free");
    // Neither in memory nor in the files: none.
    assert!(r.blocking_get(f.src(), DEPS, q(9)).is_none());
}

/// Behaviour: data.requests.purges-and-saves-go-to-the-worker
#[test]
fn saves_and_purges_run_on_the_worker_and_a_get_waiting_for_a_saved_record_takes_it() {
    let f = Fixture::new();
    let mut r = Requests::new();
    r.server = true;
    let get = r.async_get(f.src(), GATHER_NONE, q(8), Some(L));
    r.out.clear();
    let bytes = encode(q(8).id, &map(&[]));
    let save = r.async_save(f.src(), q(8), bytes, 0, Some(L));
    let purge = r.async_purge_many(f.src(), &[q(3), q(4)], Some(L));
    assert!(!f.files.is_on_disk(q(8)), "not before the worker runs");
    r.use_time(f.src(), 1.0);
    assert!(f.files.is_on_disk(q(8)));
    assert!(!f.files.is_on_disk(q(3)) && !f.files.is_on_disk(q(4)));
    // A save or a purge tells its batch only: the get its record.
    assert_eq!(
        r.out,
        vec![
            finished(8, Outcome::Success, 1),
            done(get, Outcome::Success),
            done(save, Outcome::Success),
            done(purge, Outcome::Success),
        ]
    );
    // A save finishing while a get for its record is on the worker leaves that get to the
    // worker: map 2 names 4, whose get is queued as the save of 4 finishes.
    let f2 = Fixture::new();
    let mut r2 = Requests::new();
    let two = r2.async_get(f2.src(), DEPS, q(2), Some(L));
    let four = r2.async_save(f2.src(), q(4), encode(q(4).id, &map(&[])), 0, Some(L));
    r2.use_time(f2.src(), 1.0);
    assert_eq!(r2.out, vec![done(four, Outcome::Success)]);
    assert_eq!(r2.get_state(q(2)), Some((Outcome::Success, 1)));
    r2.use_time(f2.src(), 2.0);
    assert_eq!(
        &r2.out[1..],
        &[
            finished(2, Outcome::Success, 1),
            done(two, Outcome::Success)
        ]
    );
    // A purge of what is not there fails.
    r.out.clear();
    let gone = r.async_purge(f.src(), q(3), Some(L));
    r.use_time(f.src(), 2.0);
    assert_eq!(r.out, vec![done(gone, Outcome::Failure)]);
}

/// Behaviour: data.requests.the-worker-stops
#[test]
fn a_stopped_worker_leaves_its_queue_and_shutting_down_keeps_no_free_records() {
    let f = Fixture::new();
    let mut r = Requests::new();
    assert!(!r.set_shutdown(f.src(), true));
    r.async_get(f.src(), GATHER_NONE, q(3), Some(L));
    r.use_time(f.src(), 1.0);
    assert_eq!(r.queued(), 1);
    assert!(r.out.is_empty());
    // Every batch and kept get let go: only the queue's hold is left.
    r.flush_pending_requests();
    assert!(r.pending().is_empty());
    assert_eq!((r.live(), r.live_batches()), (1, 0));
}

/// Behaviour: data.requests.without-a-worker-each-runs-at-once
#[test]
fn without_a_worker_each_request_is_carried_out_as_it_is_made() {
    let f = Fixture::new();
    let mut r = Requests::in_place();
    let b = r.async_get(f.src(), GATHER_NONE, q(1), Some(L));
    assert_eq!(r.out, vec![finished(1, Outcome::Success, 1)]);
    let p = r.async_purge(f.src(), q(4), Some(L));
    assert!(!f.files.is_on_disk(q(4)));
    r.tell_done_batches();
    assert_eq!(
        &r.out[1..],
        &[done(b, Outcome::Success), done(p, Outcome::Success)]
    );
}

/// Behaviour: data.requests.the-worker-runs-at-the-end-of-a-frame
#[test]
fn the_worker_runs_its_whole_queue_each_time_the_cache_is_given_time_and_what_finishing_queues_waits_for_the_next(
) {
    let f = Fixture::new();
    let mut r = Requests::new();
    r.async_get_many(f.src(), GATHER_NONE, &[q(1), q(2), q(3), q(4)], Some(L));
    assert_eq!(r.queued(), 4, "nothing read as it is asked");
    r.use_time(f.src(), 1.0);
    assert_eq!(
        (r.queued(), r.out.len()),
        (0, 5),
        "all four read and finished"
    );
    // Named records found while finishing are read the next time.
    let mut r = Requests::new();
    r.async_get(f.src(), DEPS, q(2), Some(L));
    r.use_time(f.src(), 2.0);
    assert_eq!(r.queued(), 1);
    assert!(r.out.is_empty());
}

/// A database over the data cache's stores reads a record stored in them after it was made (a
/// record the server sent) as it reads the files' own; one over the files themselves never sees
/// it.
///
/// Behaviour: data.cache.reads-see-records-stored-for-the-session
#[test]
fn a_record_stored_for_the_session_is_read_by_a_database_over_the_stores() {
    let (files, dat) = portal();
    let stores = Arc::new(files);
    let over = Database::over_stores(vec![dat.clone()], stores.clone());
    let plain = Database::new(vec![dat]);
    assert!(over.get::<IdMap>(q(5).id).is_none());
    assert!(stores.save_data(q(5), &encode(q(5).id, &map(&[1]))));
    let got = over.get::<IdMap>(q(5).id).expect("the stored record");
    assert_eq!(got.ids.len(), 1);
    assert!(plain.get::<IdMap>(q(5).id).is_none());
    // The files' own records read as before.
    assert_eq!(over.read(q(1).id), plain.read(q(1).id));
}

/// What a growing asker heard: each record, its outcome, the asker's word and how many times.
type Heard = Arc<std::sync::Mutex<Vec<(QualifiedId, Outcome, u32, u32)>>>;

/// What a growing asker heard, and the records it adds the first time each record finishes.
struct Grow {
    heard: Heard,
    adds: Vec<(QualifiedId, Vec<QualifiedId>)>,
}

impl OnFinished for Grow {
    fn finished(
        &mut self,
        _db: &Database,
        record: QualifiedId,
        outcome: Outcome,
        user: u32,
        times: u32,
    ) -> Vec<(u32, Vec<QualifiedId>)> {
        self.heard
            .lock()
            .expect("the list")
            .push((record, outcome, user, times));
        if times != 1 {
            return Vec::new();
        }
        self.adds
            .iter()
            .filter(|(r, _)| *r == record)
            .map(|(_, ids)| (GATHER_NONE, ids.clone()))
            .collect()
    }
}

/// An asker that grows its requests hears a record finish before the request counts as done,
/// and what it adds to it is waited on as part of it: the request finishes again (the second
/// time for that asker) only once those are in, and only then is the batch done. Records it adds
/// that are in memory or finished already count at once, so its request can finish again within
/// the same call. An asker without a hook is only told, as before.
///
/// Behaviour: data.requests.an-asker-grows-its-request-as-it-finishes
#[test]
fn an_asker_that_grows_its_request_adds_records_to_it_before_it_counts_as_done() {
    let f = Fixture::new();
    let mut r = Requests::new();
    let heard = Arc::new(std::sync::Mutex::new(Vec::new()));
    r.set_hook(
        L.to,
        Some(Box::new(Grow {
            heard: heard.clone(),
            adds: vec![(q(3), vec![q(4)])],
        })),
    );
    let batch = r.async_get(f.src(), GATHER_NONE, q(3), Some(L));
    // Map 3 read: heard once, map 4 added to its request and queued; the batch waits.
    r.use_time(f.src(), 1.0);
    assert_eq!(r.out, vec![finished(3, Outcome::Success, 1)]);
    assert_eq!(r.queued(), 1, "map 4 read next");
    assert_eq!(r.pending(), vec![q(3), q(4)]);
    // Map 4 read: map 3's request finishes again, its second time, and the batch is done.
    r.use_time(f.src(), 2.0);
    assert_eq!(
        r.out,
        vec![
            finished(3, Outcome::Success, 1),
            finished(3, Outcome::Success, 2),
            done(batch, Outcome::Success),
        ]
    );
    assert_eq!(
        *heard.lock().expect("the list"),
        vec![
            (q(3), Outcome::Success, L.user, 1),
            (q(3), Outcome::Success, L.user, 2)
        ]
    );
    r.release_batch(batch);
    assert_eq!((r.live(), r.live_batches()), (0, 0));
    // Kept and finished already: the record it adds counts at once, and the request finishes
    // its second time in the same call.
    heard.lock().expect("the list").clear();
    r.out.clear();
    let kept = r.async_get(f.src(), GATHER_NONE, q(4), None);
    r.use_time(f.src(), 3.0);
    assert_eq!(kept, Batch::NONE);
    let again = r.async_get(f.src(), GATHER_NONE, q(3), Some(L));
    r.use_time(f.src(), 4.0);
    assert_eq!(
        r.out,
        vec![
            finished(3, Outcome::Success, 1),
            finished(3, Outcome::Success, 2),
            done(again, Outcome::Success),
        ]
    );
    // Without the hook: told once, nothing added.
    r.set_hook(L.to, None);
    r.release_batch(again);
    r.out.clear();
    let plain = r.async_get(f.src(), GATHER_NONE, q(2), Some(L));
    r.use_time(f.src(), 5.0);
    assert_eq!(
        r.out,
        vec![
            finished(2, Outcome::Success, 1),
            done(plain, Outcome::Success)
        ]
    );
    assert_eq!(heard.lock().expect("the list").len(), 2);
}

/// A host grows a get itself, found by its record while it is kept or by the batch that asked
/// for it: the get waits on the records added and finishes again when they are in (a batch done
/// already is told nothing more); a record no kept get is for, or a batch let go, grows nothing.
///
/// Behaviour: data.requests.a-host-grows-a-get
#[test]
fn a_host_grows_a_get_by_its_record_or_by_its_batch() {
    let f = Fixture::new();
    let mut r = Requests::new();
    let batch = r.async_get(f.src(), GATHER_NONE, q(3), Some(L));
    assert!(r.add_to_get(f.src(), GATHER_NONE, &[q(4)], q(3)));
    assert_eq!(r.get_state(q(3)), Some((Outcome::Unknown, 2)));
    r.use_time(f.src(), 1.0);
    assert_eq!(r.get_state(q(3)), Some((Outcome::Success, 0)));
    assert_eq!(
        r.out,
        vec![
            finished(3, Outcome::Success, 1),
            done(batch, Outcome::Success)
        ],
        "map 3 read, then map 4 with it"
    );
    // Finished and kept: grown by its batch, it waits on map 2 and finishes again; its batch was
    // done already, and is told nothing more.
    r.out.clear();
    assert!(r.add_to_batch_get(f.src(), GATHER_NONE, &[q(2)], batch, q(3)));
    assert_eq!(r.get_state(q(3)), Some((Outcome::Success, 1)));
    r.use_time(f.src(), 2.0);
    assert_eq!(r.get_state(q(3)), Some((Outcome::Success, 0)));
    assert!(r.out.is_empty());
    assert!(r.pending().contains(&q(2)));
    assert!(
        !r.add_to_get(f.src(), GATHER_NONE, &[q(2)], q(9)),
        "no get of map 9"
    );
    r.release_batch(batch);
    assert!(
        !r.add_to_batch_get(f.src(), GATHER_NONE, &[q(2)], batch, q(3)),
        "let go"
    );
}

/// A qualities record names, for any gathering, each data id it holds (not 0), the tables of its
/// string references and of the references inside them, then its description's icon and its name's
/// and plural name's tables, as the client gathers a qualities record's records.
///
/// Behaviour: data.qualities.a-record-names-its-data-ids-references-and-description
#[test]
fn a_qualities_record_names_its_data_ids_references_and_descriptions_records() {
    use auberean_assets::database::gather_sub_ids;
    use auberean_assets::property::StringInfo;
    use auberean_assets::script::{ObjectDescription, Qualities};
    use auberean_dat::QualifiedIds;
    use auberean_primitives::DataId;
    let named = |table: u32| StringInfo {
        table: DataId(table),
        ..StringInfo::default()
    };
    let q = Qualities {
        data_ids: vec![(1, DataId(0x3400_0007)), (2, DataId(0))],
        string_infos: vec![(0x100, named(0x2300_0010))],
        description: Some(ObjectDescription {
            icon: Some(DataId(0x0600_1234)),
            name: Some(named(0x2300_0001)),
            plural_name: Some(named(0x2300_0002)),
            ..ObjectDescription::default()
        }),
        ..Qualities::default()
    };
    let mut out = QualifiedIds::new();
    gather_sub_ids(&q, 0x4000_0009, &mut out);
    let mut ids: Vec<u32> = out.iter().map(|q| q.id).collect();
    ids.sort_unstable();
    assert_eq!(
        ids,
        vec![
            0x0600_1234,
            0x2300_0001,
            0x2300_0002,
            0x2300_0010,
            0x3400_0007
        ]
    );
    let mut none = QualifiedIds::new();
    gather_sub_ids(&Qualities::default(), 0x4000_0009, &mut none);
    assert_eq!(none.iter().count(), 0);
}

/// An enum map names its base map's record, and one with no base names none.
///
/// Behaviour: data.database.an-enum-map-names-its-base-record
#[test]
fn an_enum_map_names_its_base_record() {
    use auberean_assets::database::gather_sub_ids;
    use auberean_assets::strings::EnumMap;
    use auberean_dat::QualifiedIds;
    let map = |base| EnumMap {
        id: 0x2200_0002,
        base,
        names: Vec::new(),
    };
    let mut out = QualifiedIds::new();
    gather_sub_ids(&map(0x2200_0001), 0x4000_0030, &mut out);
    assert_eq!(
        out.iter().map(|q| q.id).collect::<Vec<u32>>(),
        [0x2200_0001]
    );
    let mut none = QualifiedIds::new();
    gather_sub_ids(&map(0), 0x4000_0030, &mut none);
    assert_eq!(none.iter().count(), 0);
}

/// A saved script state as the files hold one: its id, its graph's bytes counted, its package.
fn state(id: u32, graph: &[u8], package: u32) -> Vec<u8> {
    let mut b = id.to_le_bytes().to_vec();
    b.extend(u32::try_from(graph.len()).unwrap().to_le_bytes());
    b.extend(graph);
    b.extend(package.to_le_bytes());
    b
}

fn st(id: u32) -> QualifiedId {
    QualifiedId {
        id,
        kind: RecordType::SCRIPT_STATE.0,
    }
}

/// What scripts were asked: the state, its package and graph as loaded, and the gathering.
type Asked = (u32, u32, Vec<u8>, u32);

/// Scripts that name, for each state, the records listed for it, and remember what they were
/// asked.
struct Scripts {
    names: Vec<(u32, Vec<QualifiedId>)>,
    asked: std::sync::Mutex<Vec<Asked>>,
}

impl StateRecords for Scripts {
    fn state_records(&self, id: u32, state: &ScriptState, gather: u32, out: &mut QualifiedIds) {
        self.asked
            .lock()
            .unwrap()
            .push((id, state.package, state.graph.clone(), gather));
        for (s, named) in &self.names {
            if *s == id {
                for n in named {
                    out.add_typed(n.id, n.kind);
                }
            }
        }
    }
}

/// **A saved state's records are named by its scripts**: a state got for a gathering is asked,
/// as it was loaded, which records it needs, and those are got in turn, a state among them asked
/// the same way; the get waits on them all. Without scripts to ask, or for the gathering that
/// asks for no named records, a state names none and its scripts are never asked.
/// Fixture: a portal in memory with two states, the first naming a map and the second, the second
/// naming another map.
/// Behaviour: data.requests.a-saved-states-records-are-named-by-its-scripts
#[test]
fn a_saved_state_names_its_records_through_its_scripts_and_each_state_they_name_is_asked_in_turn() {
    const CREATION: u32 = 0x4000_0009;
    let (first, second) = (0x7000_0001, 0x7100_0002);
    let (files, dat) = portal_with(&[
        (first, state(first, &[1, 2, 3], 0x31)),
        (second, state(second, &[4], 0x32)),
    ]);
    let f = Fixture {
        db: Database::new(vec![dat]),
        files,
    };
    let scripts = Scripts {
        names: vec![(first, vec![q(4), st(second)]), (second, vec![q(3)])],
        asked: std::sync::Mutex::new(Vec::new()),
    };
    let src = Sources {
        states: Some(&scripts),
        ..f.src()
    };
    let mut r = Requests::new();
    let batch = r.async_get(src, CREATION, st(first), Some(L));
    for t in 1..=4 {
        r.use_time(src, f64::from(t));
    }
    assert_eq!(
        *scripts.asked.lock().unwrap(),
        vec![
            (first, 0x31, vec![1, 2, 3], CREATION),
            (second, 0x32, vec![4], CREATION)
        ],
        "each state asked once, as loaded, for the gathering"
    );
    let mut pending = r.pending();
    pending.sort_unstable_by_key(|q| q.id);
    assert_eq!(pending, vec![q(3), q(4), st(first), st(second)]);
    assert_eq!(
        r.out.last(),
        Some(&done(batch, Outcome::Success)),
        "the get waited on every record named"
    );

    // Without scripts, or for no named records, the state names none.
    for (with, gather) in [
        (None, CREATION),
        (Some(&scripts as &dyn StateRecords), GATHER_NONE),
    ] {
        let (files, dat) = portal_with(&[(first, state(first, &[1], 0x31))]);
        let f = Fixture {
            db: Database::new(vec![dat]),
            files,
        };
        let src = Sources {
            states: with,
            ..f.src()
        };
        scripts.asked.lock().unwrap().clear();
        let mut r = Requests::new();
        r.async_get(src, gather, st(first), Some(L));
        r.use_time(src, 1.0);
        r.use_time(src, 2.0);
        assert_eq!(r.pending(), vec![st(first)]);
        assert!(scripts.asked.lock().unwrap().is_empty());
    }
}
