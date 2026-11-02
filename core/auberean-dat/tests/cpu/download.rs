//! The data download as the client runs it: the server's question answered from the files, a
//! download's drops made, its records stored and checked off, each file given its new version,
//! and the end said and heard.

use std::sync::Arc;

use auberean_dat::disk::controller::OPEN_CREATE;
use auberean_dat::disk::download::{
    event, stream_ids, DddState, Download, MissingIteration, Output, SAVE_FAILED_NOTICE,
};
use auberean_dat::disk::{Opened, SharedStore, StoreSet, VersionSet};
use auberean_dat::{type_of_id, QualifiedId, RecordType};

use crate::disk::support::{spec, OneFile};

const PORTAL: u64 = 0x1_0000_0000;
const LOCAL: u64 = 0x3_8000_0001;
const CELL: u64 = 0x2_0000_0001;
const HIGH: u64 = 0x1_6946_6948;

/// A new file of this id holding data versions 1 and 2.
fn file(id: u64) -> Arc<SharedStore> {
    let mut files = OneFile::new(&[]);
    let s = SharedStore::new(Box::new(|| 0x5000_0000));
    let mut sp = spec(OPEN_CREATE, 0x400 + 256 * 0x100, 0x100);
    sp.data_set = u32::try_from(id >> 32).unwrap();
    sp.data_subset = u32::try_from(id & 0xFFFF_FFFF).unwrap();
    assert_eq!(s.init_file(&mut files, &mut sp), Opened::Ready);
    let mut v = VersionSet::new();
    v.add(1);
    v.add(2);
    assert!(s.lock().save_iteration_list(&mut v));
    *s.held.lock().unwrap() = v;
    Arc::new(s)
}

fn four() -> StoreSet {
    let files = [file(PORTAL), file(LOCAL), file(CELL), file(HIGH)];
    StoreSet {
        by_id: files.iter().map(|f| (f.dat_file_id(), f.clone())).collect(),
        by_slot: files.map(Some),
    }
}

fn q(kind: RecordType, id: u32) -> QualifiedId {
    QualifiedId { id, kind: kind.0 }
}

fn versions(s: &Arc<SharedStore>) -> Vec<i32> {
    s.lock().load_iteration_list().unwrap().values
}

/// Behaviour: data.download.asked-which-data-is-held
#[test]
fn asked_which_data_it_holds_the_client_answers_each_files_versions_in_order() {
    let set = four();
    let mut d = Download::new();
    let lists = d.on_server_interrogation(&set).unwrap();
    let ids: Vec<u64> = lists.iter().map(|(id, _)| *id).collect();
    assert_eq!(ids, vec![PORTAL, LOCAL, CELL, HIGH]);
    assert!(lists.iter().all(|(_, v)| v.values == vec![1, 2]));
    assert_eq!(d.state, DddState::InterrogationReceived);
    // A file whose list does not read: nothing is sent.
    assert!(set.by_slot[2].as_ref().unwrap().delete_data(0xFFFF_0001));
    assert!(d.on_server_interrogation(&set).is_none());
}

/// Behaviour: data.download.records-stored-and-checked-off
#[test]
fn a_download_stores_each_record_checks_it_off_and_gives_each_file_its_version() {
    let set = four();
    let (portal, local) = (
        set.by_slot[0].clone().unwrap(),
        set.by_slot[1].clone().unwrap(),
    );
    let mut d = Download::new();
    d.on_server_interrogation(&set).unwrap();
    // A record before the download begins: stored, and counted early. (The server names its
    // type as the download's lists type it.)
    let early = stream_ids(&[0x0600_0001], 1)[0];
    d.on_data(&set, PORTAL, early, 7, Some(b"early"));
    assert_eq!(d.early_saves, vec![early]);
    assert!(portal.load_data(0x0600_0001).is_some());
    // The download: the portal waits for two (one came early), the local file for none.
    let parts = vec![
        MissingIteration::from_wire(PORTAL, 3, &[0x0600_0001, 0x0600_0002], &[]),
        MissingIteration::from_wire(LOCAL, 9, &[], &[]),
    ];
    d.out.clear();
    d.on_begin_ddd(&set, 100, parts);
    assert_eq!(d.state, DddState::Patching);
    assert_eq!(
        d.out,
        vec![Output::Notice {
            event: event::PATCHING,
            value: 93
        }]
    );
    assert_eq!(
        versions(&local),
        vec![1, 2, 9],
        "nothing to fetch: the version at once"
    );
    assert_eq!(d.pending.len(), 1, "the early record is checked off");
    assert_eq!(versions(&portal), vec![1, 2]);
    // The last record: stored, the portal's version taken, and the end said.
    d.out.clear();
    let id2 = d.missing[0].download[0];
    d.on_data(&set, PORTAL, id2, 12, Some(b"second"));
    assert_eq!(
        d.out,
        vec![
            Output::Notice {
                event: event::PROGRESS,
                value: 12
            },
            Output::SendEnd
        ]
    );
    assert_eq!(versions(&portal), vec![1, 2, 3]);
    assert_eq!(portal.held.lock().unwrap().values, vec![1, 2, 3]);
    // The server's end.
    d.out.clear();
    d.on_end_ddd();
    assert_eq!(d.state, DddState::RunTime);
    assert_eq!(
        d.out,
        vec![Output::Notice {
            event: event::DONE,
            value: 0
        }]
    );
    assert!(d.missing.is_empty() && d.pending.is_empty() && d.early_saves.is_empty());
}

/// Behaviour: data.download.drops-a-landblock-with-its-block
#[test]
fn a_dropped_landblock_takes_every_record_of_its_block_and_other_drops_go_alone() {
    let set = four();
    let (portal, cell) = (
        set.by_slot[0].clone().unwrap(),
        set.by_slot[2].clone().unwrap(),
    );
    for id in [0x0101_FFFF, 0x0101_FFFE, 0x0101_0100, 0x0102_FFFF] {
        assert!(cell.save_data(id, b"cell"));
    }
    for id in [0x0600_0005, 0x0600_0006] {
        assert!(portal.save_data(id, b"pic"));
    }
    let mut d = Download::new();
    let parts = vec![
        MissingIteration::from_wire(CELL, 4, &[], &[0x0101_FFFF]),
        MissingIteration::from_wire(PORTAL, 4, &[], &[0x0600_0005]),
    ];
    d.on_begin_ddd(&set, 0, parts);
    for (id, there) in [
        (0x0101_FFFF, false),
        (0x0101_FFFE, false),
        (0x0101_0100, false),
        (0x0102_FFFF, true),
    ] {
        assert_eq!(cell.is_member(id).is_some(), there, "{id:08X}");
    }
    assert!(portal.is_member(0x0600_0005).is_none());
    assert!(portal.is_member(0x0600_0006).is_some());
    // Nothing to fetch: the end is said at once, and no bytes to come means no notice.
    assert_eq!(d.out, vec![Output::SendEnd]);
}

/// Behaviour: data.download.a-bad-record-is-asked-again
#[test]
fn a_record_that_does_not_unpack_is_asked_again_and_one_that_does_not_store_stops_the_client() {
    let set = four();
    let mut d = Download::new();
    let r = q(RecordType::SCRIPT_LIBRARY, 0x0600_0009);
    d.on_data(&set, PORTAL, r, 4, None);
    d.on_refused(r);
    d.on_waiting(17);
    assert_eq!(
        d.out,
        vec![
            Output::Request(r),
            Output::Refused(r),
            Output::Notice {
                event: event::WAITING,
                value: 17
            },
        ]
    );
    // With no file to store it in, the store fails.
    let closed = StoreSet::default();
    d.out.clear();
    d.on_data(&closed, PORTAL, r, 4, Some(b"x"));
    assert_eq!(
        d.out,
        vec![
            Output::Notice {
                event: event::PROGRESS,
                value: 4
            },
            Output::Fatal(SAVE_FAILED_NOTICE)
        ]
    );
}

/// Behaviour: data.download.ids-typed-by-their-file
#[test]
fn a_downloads_ids_are_typed_by_their_file() {
    let kinds =
        |ids: &[u32], set| -> Vec<u32> { stream_ids(ids, set).iter().map(|q| q.kind).collect() };
    assert_eq!(
        kinds(&[0x0101_FFFF, 0x0101_FFFE, 0x0101_0100, 0x0101_0005], 2),
        vec![
            RecordType::LANDBLOCK.0,
            RecordType::LANDBLOCK_INFO.0,
            RecordType::CELL.0,
            RecordType::LANDBLOCK.0
        ]
    );
    assert_eq!(
        kinds(&[0x0600_0001], 1),
        vec![type_of_id(0x0600_0001).unwrap().0]
    );
    assert_eq!(kinds(&[0x0000_0001], 1), vec![RecordType::SCRIPT_LIBRARY.0]);
    assert_eq!(kinds(&[0x0000_0001], 3), vec![RecordType::STRING_TABLE.0]);
    assert_eq!(kinds(&[0x0000_0001], 9), vec![0]);
}

/// Behaviour: data.download.records-stored-and-checked-off
#[test]
fn the_caches_disk_calls_go_to_the_file_a_record_belongs_to() {
    let set = four();
    let pic = q(RecordType(0), 0x0600_0042);
    assert!(!set.is_on_disk(pic));
    assert!(set.save_data(pic, b"abc"));
    assert!(set.is_on_disk(pic) && set.are_on_disk(&[pic]));
    assert_eq!(set.load_data(pic).as_deref(), Some(&b"abc"[..]));
    assert!(set.by_slot[0]
        .as_ref()
        .unwrap()
        .is_member(0x0600_0042)
        .is_some());
    assert!(set.worker_execute_purge_request(pic));
    assert!(!set.purge_data(pic));
    assert!(set.worker_execute_save_request(pic, CELL, b"in the cell file"));
    assert!(set.by_slot[2]
        .as_ref()
        .unwrap()
        .is_member(0x0600_0042)
        .is_some());
}
