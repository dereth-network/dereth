//! The data files written as the client writes them: records stored, replaced and removed, files
//! made and grown, crashed writes put right, and the data versions a file holds.
//!
//! The digests here are of whole files made by the client's own disk controller: the same
//! operations, from the same seeded generator, run on the client and folded with FNV-1a 64 (the
//! stamp's leading word, which the client fills from its own memory, set to 0).

// The files and records built here are small: every length and offset fits in 32 bits.
#![allow(clippy::cast_possible_truncation)]

use auberean_dat::disk::alloc::{header_block, StoreHeader};
use auberean_dat::disk::controller::{
    OPEN_CREATE, OPEN_CREATE_IF_BAD, OPEN_EXPANDABLE, OPEN_READ_ONLY,
};
use auberean_dat::disk::device::join;
use auberean_dat::disk::DirEntry;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use auberean_dat::disk::shared::NO_VERSIONS_NOTICE;
use auberean_dat::disk::{
    FileSearch, LookHost, Opened, SharedStore, Store, StoreSet, Transaction, VersionSet,
};
use auberean_dat::{DatFile, DatStorage, Reader, Writer};
use auberean_dat::{QualifiedId, RecordType};

#[path = "../disk_support.rs"]
pub(crate) mod support;
use support::{apply, digest, fnv, operations, spec, store, OneFile};

/// A new file (expandable) given a case's operations, closed: the file's digest and the answers'.
fn new_file_case(
    seed: u32,
    count: u32,
    space: u32,
    size: u32,
    block: u32,
    file: u32,
    shrink: bool,
) -> (u64, u64) {
    let mut files = OneFile::new(&[]);
    let mut s = store();
    assert!(s.init_file(
        &mut files,
        &mut spec(OPEN_CREATE | OPEN_EXPANDABLE, file, block)
    ));
    let answers = apply(&mut s, &operations(seed, count, space, size, shrink));
    s.close();
    let answers: Vec<u8> = answers.into_iter().map(u8::from).collect();
    let b = files.bytes();
    let dat = DatFile::from_bytes(b.clone()).expect("the file reads as a container");
    assert!(auberean_dat::container::sound(&dat.verify().unwrap()));
    (digest(b), fnv(&answers))
}

/// Behaviour: data.disk.writes-as-the-client-writes
#[test]
fn a_small_file_takes_records_and_grows_as_the_clients_does() {
    // 64 blocks of 64 bytes: the file grows a megabyte at the first record.
    assert_eq!(
        new_file_case(7, 200, 300, 200, 0x40, 0x400 + 64 * 0x40, false),
        (0x0d7e_7713_324e_ef1d, 0x53b3_84ce_f28c_4453)
    );
}

/// Behaviour: data.disk.writes-as-the-client-writes
#[test]
fn a_tree_two_levels_deep_splits_rotates_and_merges_as_the_clients_does() {
    assert_eq!(
        new_file_case(11, 6000, 4000, 40, 0x100, 0x400 + 256 * 0x100, false),
        (0x9c9a_8f51_7cc8_8e53, 0x854b_af50_90fd_ec6f)
    );
}

/// Behaviour: data.disk.writes-as-the-client-writes
#[test]
fn records_replaced_over_and_over_reuse_blocks_as_the_clients_do() {
    assert_eq!(
        new_file_case(23, 4000, 400, 600, 0x100, 0x400 + 32 * 0x100, false),
        (0x74a4_6a1f_6b39_7bf0, 0x53bb_c932_23a6_9b05)
    );
}

/// Behaviour: data.disk.writes-as-the-client-writes
#[test]
fn a_tree_three_levels_deep_grows_and_shrinks_as_the_clients_does() {
    assert_eq!(
        new_file_case(41, 40000, 30000, 24, 0x100, 0x400 + 256 * 0x100, false),
        (0x150a_9ca2_64c4_22f2, 0x8bf3_a3ae_d2ff_5705)
    );
    assert_eq!(
        new_file_case(43, 12000, 8000, 16, 0x100, 0x400 + 256 * 0x100, true),
        (0xd5ea_c0b8_81b3_b633, 0x768e_bed2_5519_5956)
    );
}

/// Behaviour: data.disk.a-removal-that-empties-the-root-fails
#[test]
fn a_removal_that_merges_away_the_root_fails_and_the_next_one_succeeds() {
    let mut files = OneFile::new(&[]);
    let mut s = store();
    assert!(s.init_file(
        &mut files,
        &mut spec(OPEN_CREATE | OPEN_EXPANDABLE, 0x10_0400, 0x100)
    ));
    // With the list of data versions, 62 records: the root splits into one entry over two leaves,
    // of 30 and 31.
    for id in 1..=61 {
        assert!(s.save_data(id, &[1, 2, 3]));
    }
    assert_eq!(s.tree.root.node.count(), 1);
    // One from the right leaf: both leaves hold 30, the fewest.
    assert!(s.delete_data(61));
    // Removing from a leaf of 30 merges the two leaves around the root's only entry: the root is
    // left empty and the merged leaf becomes the root, and the removal finds nothing after it.
    assert!(!s.delete_data(5));
    assert!(s.is_member(5).is_some(), "the record is still there");
    assert!(s.tree.root.node.is_leaf());
    assert_eq!(s.tree.root.node.count(), 61);
    // Asked again, it goes.
    assert!(s.delete_data(5));
    assert!(s.is_member(5).is_none());
    s.close();
    let dat = DatFile::from_bytes(files.bytes()).unwrap();
    assert!(auberean_dat::container::sound(&dat.verify().unwrap()));
    assert_eq!(
        dat.len(),
        60,
        "59 records and the empty list of data versions"
    );
}

/// The process dies: nothing more is written, not even on closing.
fn die(mut s: Store) {
    s.initialized = false;
}

/// The runs of `crashes` where the client itself faults opening the crashed file: a node
/// rewritten across several blocks and cut part way, so its count is not its entries'.
const CLIENT_FAULTS: &[(usize, std::ops::RangeInclusive<usize>)] = &[
    (9, 5..=9),
    (31, 10..=14),
    (31, 242..=246),
    (31, 272..=276),
    (31, 631..=635),
];

/// The entry count of the node whose chain starts at `off`.
fn node_count(b: &[u8], off: u32) -> i32 {
    let word = |at: usize| u32::from_le_bytes(b[at..at + 4].try_into().unwrap());
    let bs = word(300 + 4) as usize;
    let (mut out, mut cur) = (Vec::new(), off as usize);
    while out.len() < 0x4CC && cur != 0 {
        out.extend_from_slice(&b[cur + 4..cur + bs]);
        cur = (word(cur) & 0x7FFF_FFFF) as usize;
    }
    i32::from_le_bytes(out[248..252].try_into().unwrap())
}

/// A merge cut short after its left node was written: opening does it again, and the client then
/// writes the separator and the right node's entries past the left node's end, into its own
/// memory.
fn merge_done_again(b: &[u8]) -> bool {
    let t = Transaction::from_bytes(&b[0x100..0x12C].try_into().unwrap());
    t.kind == Transaction::MERGE && node_count(b, t.ul(2)) != 30
}

/// Behaviour: data.disk.a-crashed-write-is-put-right-on-opening
#[test]
fn a_write_cut_short_anywhere_is_finished_or_undone_on_opening_as_the_client_does() {
    let ops = operations(11, 6000, 4000, 40, false);
    let (base_ops, window) = (3000, 40);
    let mut files = OneFile::new(&[]);
    let mut s = store();
    assert!(s.init_file(
        &mut files,
        &mut spec(OPEN_CREATE | OPEN_EXPANDABLE, 0x400 + 256 * 0x100, 0x100)
    ));
    apply(&mut s, &ops[..base_ops]);
    s.close();
    let base = files.bytes();
    let reopen = |files: &mut OneFile| {
        let mut s = store();
        assert!(s.init_file(files, &mut spec(OPEN_EXPANDABLE, 0, 0)));
        s
    };
    let mut fold = 0xCBF2_9CE4_8422_2325u64;
    let mut mix = |v: u64| {
        fold = (fold ^ v).wrapping_mul(0x100_0000_01B3);
    };
    let (mut runs, mut failed) = (0, 0);
    for j in 0..window {
        let before = &ops[base_ops..base_ops + j];
        let op = &ops[base_ops + j..=base_ops + j];
        let mut files = OneFile::new(&base);
        let mut s = reopen(&mut files);
        apply(&mut s, before);
        let w0 = files.writes();
        apply(&mut s, op);
        let n = files.writes() - w0;
        die(s);
        for limit in 0..=n {
            let mut files = OneFile::new(&base);
            let mut s = reopen(&mut files);
            apply(&mut s, before);
            files.die_after(limit);
            apply(&mut s, op);
            die(s);
            let crashed = files.bytes();
            let mut after = OneFile::new(&crashed);
            let mut r = store();
            let ok = r.init_file(&mut after, &mut spec(OPEN_EXPANDABLE, 0, 0));
            r.close();
            runs += 1;
            if merge_done_again(&crashed) {
                // No answer to hold ours to; ours must only come through.
                mix(0);
                mix(3);
            } else if CLIENT_FAULTS
                .iter()
                .any(|(o, cut)| *o == j && cut.contains(&limit))
            {
                // No answer to hold ours to; ours must only come through.
                mix(0);
                mix(2);
            } else {
                failed += usize::from(!ok);
                mix(digest(after.bytes()));
                mix(u64::from(ok));
            }
            mix(n as u64);
        }
    }
    assert_eq!(runs, 1395);
    // A rotation cut short leaves its record in the slot, which opening cannot finish: the file
    // does not open.
    assert_eq!(failed, 96);
    assert_eq!(fold, 0xdc7f_81a6_3304_1d5a);
}

/// Behaviour: data.disk.writes-as-the-client-writes
#[test]
fn a_new_file_is_a_free_chain_an_empty_root_and_an_empty_list_of_data_versions() {
    let mut files = OneFile::new(&[]);
    let mut s = store();
    let mut sp = spec(OPEN_CREATE, 0x400 + 64 * 0x100, 0x100);
    assert!(s.init_file(&mut files, &mut sp));
    s.close();
    let b = files.bytes();
    let h = |i: usize| u32::from_le_bytes(b[300 + 4 * i..304 + 4 * i].try_into().unwrap());
    assert_eq!(h(0), 0x5442, "magic");
    assert_eq!((h(1), h(2), h(3), h(4)), (0x100, 0x400 + 64 * 0x100, 2, 7));
    // The root took 5 blocks and the empty list one: 58 of 64 are left, from 0xA00 to the last.
    assert_eq!((h(5), h(6), h(7), h(8)), (0xA00, 0x4300, 58, 0x400));
    assert_eq!(
        &b[0x100..0x12C],
        &[0u8; 0x2C],
        "the transaction slot is clear"
    );
    let mut s = store();
    let mut files = OneFile::new(&b);
    assert!(s.init_file(&mut files, &mut spec(0, 0, 0)));
    assert_eq!(s.load_iteration_list().unwrap().values, Vec::<i32>::new());
    assert_eq!(s.get_datestamp(0xFFFF_0001), 0x5000_0000);
    assert_eq!(s.howmuch_free_space(), 58 * 0x100);
}

/// Behaviour: data.disk.opening-checks-versions-and-access
#[test]
fn a_file_of_other_data_versions_is_refused_or_made_anew_and_a_read_only_one_takes_nothing() {
    let mut files = OneFile::new(&[]);
    let mut s = store();
    let mut sp = spec(OPEN_CREATE, 0x400 + 64 * 0x100, 0x100);
    sp.eng_pack_vnum = 104;
    sp.game_pack_vnum = 3;
    assert!(s.init_file(&mut files, &mut sp));
    assert!(s.save_data(9, b"nine"));
    s.close();
    let made = files.bytes();

    // Another game version: refused, the file untouched.
    let mut files = OneFile::new(&made);
    let mut s = store();
    let mut sp = spec(0, 0, 0);
    sp.game_pack_vnum = 4;
    assert!(!s.init_file(&mut files, &mut sp));
    assert_eq!(files.bytes(), made);
    // The engine's version alone is asked for: it opens, and gives the file's set and subset.
    let mut s = store();
    let mut sp = spec(OPEN_READ_ONLY, 0, 0);
    sp.data_set = 0;
    sp.eng_pack_vnum = 104;
    sp.game_pack_vnum = 4;
    sp.eng_only = true;
    assert!(s.init_file(&mut files, &mut sp));
    assert_eq!((sp.data_set, sp.data_subset), (2, 7));
    assert_eq!(s.load_data(9).as_deref(), Some(&b"nine"[..]));
    // Read only: nothing stored, nothing removed, nothing written on closing.
    assert!(!s.save_data(10, b"ten"));
    assert!(!s.delete_data(9));
    s.close();
    assert_eq!(files.bytes(), made);
    // Another version with the spec saying to make it anew: a new, empty file.
    let mut s = store();
    let mut sp = spec(OPEN_CREATE_IF_BAD, 0x400 + 64 * 0x100, 0x100);
    sp.eng_pack_vnum = 105;
    assert!(s.init_file(&mut files, &mut sp));
    assert!(s.load_data(9).is_none());
    assert_eq!(s.header().eng_pack_vnum, 105);
}

/// Behaviour: data.disk.writes-as-the-client-writes
#[test]
fn a_file_is_opened_by_its_folder_and_name_joined_with_a_slash() {
    assert_eq!(join("game.dat", "C:\\AC2"), "C:\\AC2/game.dat");
    assert_eq!(join("game.dat", "C:\\AC2\\"), "C:\\AC2\\game.dat");
    assert_eq!(join("game.dat", "data/"), "data/game.dat");
    assert_eq!(join("game.dat", ""), "game.dat");
    assert_eq!(join("", "data"), "data");
}

/// Behaviour: data.disk.writes-as-the-client-writes
#[test]
fn a_transaction_record_is_a_kind_and_ten_words() {
    let t = Transaction::new(Transaction::SPLIT, &[0x400, 61, 3, 0x9000, 77]);
    let b = t.to_bytes();
    assert_eq!(&b[..8], &[10, 0, 0, 0, 0, 4, 0, 0]);
    assert_eq!(Transaction::from_bytes(&b), t);
    assert_eq!(t.ul(5), 77);
}

/// Behaviour: data.version-set.packs-runs-of-three-or-more
#[test]
fn a_version_set_packs_runs_of_three_or_more_and_unpacks_negatives() {
    let mut s = VersionSet::new();
    for v in [9, 3, 4, 5, 6, 3, 20, 21, -2, 9, 1] {
        s.add(v);
    }
    let mut w = Writer::new();
    s.pack(&mut w);
    let words: Vec<i32> = w
        .finish()
        .chunks(4)
        .map(|c| i32::from_le_bytes(c.try_into().unwrap()))
        .collect();
    // Sorted, repeats dropped: -2 1 3 4 5 6 9 20 21. -2 alone loses its top bit.
    assert_eq!(words, vec![9, 0x7FFF_FFFE, 1, -4, 3, 9, 20, 21]);
    assert_eq!(s.pack_size(), 32);
    let raw: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
    let mut back = VersionSet::new();
    assert!(back.unpack(&mut Reader::new(&raw)));
    assert_eq!(back.values, vec![-2, 1, 3, 4, 5, 6, 9, 20, 21]);
    // A run longer than the count left, and a count over 100,000, are refused.
    let bad: Vec<u8> = [2i32, -3, 7].iter().flat_map(|w| w.to_le_bytes()).collect();
    assert!(!VersionSet::new().unpack(&mut Reader::new(&bad)));
    assert!(!VersionSet::new().unpack(&mut Reader::new(&100_001u32.to_le_bytes())));
}

/// A shared file opened over `bytes`, a new one (expandable) when empty.
fn shared(bytes: &[u8], flags: u32) -> (SharedStore, Opened, OneFile) {
    let mut files = OneFile::new(bytes);
    let s = SharedStore::new(Box::new(|| 0x5000_0000));
    let o = s.init_file(&mut files, &mut spec(flags, 0x400 + 64 * 0x100, 0x100));
    (s, o, files)
}

/// Behaviour: data.disk.a-file-without-a-version-list-is-closed
#[test]
fn a_shared_file_reads_its_data_versions_on_opening_and_one_without_them_is_closed() {
    let (s, o, files) = shared(&[], OPEN_CREATE);
    assert_eq!(o, Opened::Ready);
    let mut set = VersionSet::new();
    for v in [104, 105, 106, 3] {
        set.add(v);
    }
    assert!(s.lock().save_iteration_list(&mut set));
    assert!(s.save_data(0x0600_0001, b"one"));
    s.lock().close();
    let with = files.bytes();
    let (s, o, _) = shared(&with, 0);
    assert_eq!(o, Opened::Ready);
    assert_eq!(s.held.lock().unwrap().values, vec![3, 104, 105, 106]);
    assert_eq!(s.load_data(0x0600_0001).as_deref(), Some(&b"one"[..]));
    drop(s);
    // Without its list: it opens, then is closed again.
    let mut files = OneFile::new(&with);
    store_over(&mut files);
    let (s, o, _) = shared(&files.bytes(), 0);
    assert_eq!(o, Opened::NoVersions);
    assert!(!s.is_initialized());
    assert_eq!(NO_VERSIONS_NOTICE, 0xCE);
}

/// `files` opened and its list of data versions removed.
fn store_over(files: &mut OneFile) -> Store {
    let mut s = store();
    assert!(s.init_file(files, &mut spec(0, 0, 0)));
    assert!(s.delete_data(0xFFFF_0001));
    s
}

/// Behaviour: data.disk.a-record-is-looked-for-in-its-file
#[test]
fn a_record_is_looked_for_in_the_file_its_type_belongs_to() {
    let open = |id: u32| {
        let (s, o, _) = shared(&[], OPEN_CREATE);
        assert_eq!(o, Opened::Ready);
        if id != 0 {
            assert!(s.save_data(id, b"x"));
        }
        Arc::new(s)
    };
    let (portal, local, cell, high) = (open(0), open(0), open(0), open(0x0600_0002));
    let set = StoreSet {
        by_slot: [
            Some(portal.clone()),
            Some(local.clone()),
            Some(cell.clone()),
            Some(high.clone()),
        ],
        by_id: [(0x2_0000_0007, cell.clone())].into_iter().collect(),
    };
    let at =
        |kind: u32, id: u32, file: u64| set.get_disk_controller(QualifiedId { id, kind }, file);
    // A picture the high-resolution file holds comes from it; one it does not, from the portal.
    assert!(Arc::ptr_eq(&at(0, 0x0600_0002, 0).unwrap(), &high));
    assert!(Arc::ptr_eq(&at(0, 0x0600_0003, 0).unwrap(), &portal));
    // An interface layout (by its id) is local; a path map is in a cell file; a named file is
    // that file.
    assert!(Arc::ptr_eq(&at(0, 0x2200_0001, 0).unwrap(), &local));
    assert!(Arc::ptr_eq(
        &at(RecordType::PATH_MAP.0, 0x0101_FFFD, 0).unwrap(),
        &cell
    ));
    assert!(Arc::ptr_eq(
        &at(0, 0x0600_0003, 0x2_0000_0007).unwrap(),
        &cell
    ));
    assert!(set.is_dat_file_loaded(0x2_0000_0007));
    assert!(!set.is_dat_file_loaded(0x1_0000_0000));
    cell.lock().close();
    assert!(!set.is_dat_file_loaded(0x2_0000_0007));
}

/// Folders and the files in them, for a search.
struct Disk(Vec<&'static str>);

impl LookHost for Disk {
    fn exists(&self, name: &str) -> bool {
        self.0.contains(&name)
    }
    fn working_folder(&self) -> Option<String> {
        Some("C:\\game".into())
    }
    fn variable(&self, name: &str) -> Option<String> {
        (name == "PROJECT_DAT").then(|| "D:\\data".into())
    }
    fn registry(&self, _key: &str, _value: &str) -> Option<String> {
        Some("E:\\AC2\\".into())
    }
}

/// Behaviour: data.disk.a-file-is-looked-for-in-four-places
#[test]
fn a_file_is_looked_for_in_its_folder_the_working_folder_the_variable_and_the_registry() {
    let mut l = FileSearch::new("Software\\Turbine", "Path");
    let found = |l: &mut FileSearch, disk: &Disk, further| {
        l.look_for_file(disk, "game.dat", "F:/given/", further)
            .then(|| l.get_found_path())
    };
    assert_eq!(
        found(
            &mut l,
            &Disk(vec!["F:/given/game.dat", "C:\\game/game.dat"]),
            true
        )
        .as_deref(),
        Some("F:/given/")
    );
    assert_eq!(
        found(&mut l, &Disk(vec!["C:\\game/game.dat"]), true).as_deref(),
        Some("C:\\game/")
    );
    assert_eq!(found(&mut l, &Disk(vec!["C:\\game/game.dat"]), false), None);
    assert_eq!(
        found(&mut l, &Disk(vec!["D:\\data/game.dat"]), true).as_deref(),
        Some("D:\\data/")
    );
    assert_eq!(
        found(&mut l, &Disk(vec!["E:\\AC2\\game.dat"]), true).as_deref(),
        Some("E:\\AC2\\")
    );
    // No registry value to read: the search ends before it.
    let mut l = FileSearch::new("", "Path");
    assert!(!l.look_for_file(&Disk(vec!["E:\\AC2\\game.dat"]), "game.dat", "", true));
    // The folder already found is looked in again when no folder is given.
    let mut l = FileSearch::new("", "");
    assert!(l.look_for_file(&Disk(vec!["x/a.dat", "x/b.dat"]), "a.dat", "x/", false));
    assert!(l.is_file_here(&Disk(vec!["x/b.dat"]), "b.dat", ""));
    assert!(!l.is_file_here(&Disk(vec!["y/b.dat"]), "b.dat", "y/"));
}

/// Behaviour: data.disk.writes-as-the-client-writes
#[test]
fn the_whole_header_block_is_the_version_text_a_clear_slot_and_the_header() {
    let info = StoreHeader::new();
    let b = header_block(&info);
    assert!(b.starts_with(b"\nFile Header Structure Default Constructor v1.3\n\0"));
    assert_eq!(b[0xFF], 0x1A);
    assert_eq!(&b[0x100..0x12C], &[0u8; 0x2C]);
    assert_eq!(&b[300..304], &0x5442u32.to_le_bytes());
    assert_eq!(&b[300 + 12..300 + 16], &[0xFF; 4], "no data set");
    assert_eq!(
        DirEntry::default(),
        DirEntry {
            id: 0,
            offset: 0,
            size: 0,
            date: 0
        }
    );
}

/// Behaviour: data.disk.files-only-read-are-written-in-memory
#[test]
fn a_file_only_read_takes_records_in_memory_and_stays_as_it_was() {
    // A real file in memory, then read only.
    let mut files = OneFile::new(&[]);
    let mut s = store();
    assert!(s.init_file(
        &mut files,
        &mut spec(OPEN_CREATE, 0x400 + 64 * 0x100, 0x100)
    ));
    assert!(s.save_data(0x0600_0001, b"first"));
    s.close();
    let bytes = files.bytes();
    let dat = Arc::new(DatFile::from_bytes(bytes.clone()).unwrap());
    let clock: Arc<dyn Fn() -> u32 + Send + Sync> = Arc::new(|| 7);
    let set = StoreSet::over_files(std::slice::from_ref(&dat), &clock);
    let file = set.by_slot[2]
        .clone()
        .expect("the cell file, by its data set");
    // Records stored, replaced and removed, past the file's end too (it grows in memory).
    for i in 0..400 {
        assert!(file.save_data(0x0101_0100 + i, &[u8::try_from(i % 251).unwrap(); 300]));
    }
    assert!(file.save_data(0x0600_0001, b"second"));
    assert!(file.delete_data(0x0101_0105));
    assert_eq!(file.load_data(0x0600_0001).as_deref(), Some(&b"second"[..]));
    assert_eq!(file.load_data(0x0101_0106).map(|v| v.len()), Some(300));
    assert!(file.load_data(0x0101_0105).is_none());
    // The file underneath is as it was.
    assert_eq!(dat.read(0x0600_0001).unwrap(), b"first");
    assert!(dat.entry(0x0101_0106).is_none());
}

/// A file's bytes, counting how many times they are read.
#[derive(Debug)]
struct Counted {
    bytes: Vec<u8>,
    reads: Arc<AtomicUsize>,
}

impl DatStorage for Counted {
    fn read_exact_at(&self, offset: u64, buf: &mut [u8]) -> std::io::Result<()> {
        self.reads.fetch_add(1, Ordering::Relaxed);
        let at = offset as usize;
        let end = at + buf.len();
        if end > self.bytes.len() {
            return Err(std::io::ErrorKind::UnexpectedEof.into());
        }
        buf.copy_from_slice(&self.bytes[at..end]);
        Ok(())
    }
}

/// A record of several blocks in a file only read, and the directory's node that leads to it,
/// each come from one read of the file (or two, where its blocks cross from one chunk into the
/// next), not one read a block, and the same record read again reads the file no more; what is read is the record, and a record stored over the file
/// is read as stored.
///
/// Behaviour: data.disk.files-only-read-are-read-in-chunks
#[test]
fn a_file_only_read_is_read_in_chunks_not_a_block_at_a_time() {
    let mut files = OneFile::new(&[]);
    let mut s = store();
    assert!(s.init_file(
        &mut files,
        &mut spec(OPEN_CREATE, 0x400 + 1024 * 0x100, 0x100)
    ));
    // Small records around the large one, so it lies past the file's first chunk and the
    // directory's nodes lie past it.
    let big: Vec<u8> = (0..2000u32).map(|i| (i % 253) as u8).collect();
    for i in 0..400u32 {
        if i == 150 {
            assert!(s.save_data(0x0600_0001, &big));
        }
        assert!(s.save_data(0x0101_0100 + i, &[(i % 251) as u8; 200]));
    }
    s.close();
    let reads = Arc::new(AtomicUsize::new(0));
    let storage = Counted {
        bytes: files.bytes(),
        reads: reads.clone(),
    };
    let dat = Arc::new(DatFile::from_storage("f".into(), Box::new(storage)).unwrap());
    let clock: Arc<dyn Fn() -> u32 + Send + Sync> = Arc::new(|| 7);
    let set = StoreSet::over_files(std::slice::from_ref(&dat), &clock);
    let file = set.by_slot[2].clone().expect("the cell file");
    let first = dat.entry(0x0600_0001).expect("stored").offset;
    assert!(first as usize >= 0x8000, "past the first chunk: {first:#x}");
    // Its leaf of the directory (a node of five blocks of 0xFC bytes) and the record (eight
    // blocks): thirteen blocks, read a block at a time; here a chunk or two of each.
    let before = reads.load(Ordering::Relaxed);
    assert_eq!(file.load_data(0x0600_0001).as_deref(), Some(&big[..]));
    let once = reads.load(Ordering::Relaxed) - before;
    assert!((1..=4).contains(&once), "{once} reads for thirteen blocks");
    assert_eq!(file.load_data(0x0600_0001).as_deref(), Some(&big[..]));
    assert_eq!(
        reads.load(Ordering::Relaxed) - before,
        once,
        "read again: no reads"
    );
    // A record stored over it is read as stored, the file's own still as they were.
    assert!(file.save_data(0x0600_0001, b"second"));
    assert_eq!(file.load_data(0x0600_0001).as_deref(), Some(&b"second"[..]));
    assert_eq!(dat.read(0x0600_0001).unwrap(), big);
    assert_eq!(file.load_data(0x0101_0105).map(|v| v.len()), Some(200));
}
