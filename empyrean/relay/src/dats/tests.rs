//! Behaviour: none (the push as the later download, over made-up records in stored zlib blocks).

use dereth_protocol::admin::{MostlyConsecutiveIntSet, OverlayBase, TaggedIterationList};

use super::*;

/// A zlib stream holding `data` in one stored block.
fn stored(data: &[u8]) -> Packed {
    let len = u16::try_from(data.len()).unwrap();
    let mut out = vec![0x78, 0x01, 0x01];
    out.extend(len.to_le_bytes());
    out.extend((!len).to_le_bytes());
    out.extend(data);
    out.extend(dereth_dat::inflate::adler32(data).to_be_bytes());
    Packed {
        inflated_len: u32::try_from(data.len()).unwrap(),
        bytes: out,
    }
}

fn record(id: u32, cache: u32, data: &[u8]) -> DatRecord {
    DatRecord {
        id,
        cache,
        data: stored(data),
    }
}

fn client() -> ClientFiles {
    ClientFiles {
        portal: 102,
        cell: 67,
        portal_base: [1; 32],
        cell_base: [2; 32],
    }
}

fn begin(portal: u32, cell: u32) -> DatBegin {
    DatBegin {
        portal_iteration: portal,
        cell_iteration: cell,
        ..DatBegin::default()
    }
}

const PORTAL_ID: u32 = 0x0100_0001;
const CELL_ID: u32 = 0x0001_FFFF;

#[test]
fn what_the_client_holds_is_read_from_its_answer() {
    let list = |t, i, n| TaggedIterationList {
        dat_file_type: t,
        dat_file_id: i,
        iterations: MostlyConsecutiveIntSet {
            iterations: n,
            ints: vec![-n, 1],
        },
    };
    let answer = DddInterrogationResponse {
        iters_with_keys: vec![list(0, 1, 102), list(1, 2, 67), list(1, 3, 9)],
        overlay_bases: vec![OverlayBase {
            dat_file_type: 0,
            dat_file_id: 1,
            fingerprint: [7; 32],
        }],
        ..DddInterrogationResponse::default()
    };
    let c = ClientFiles::of(&answer);
    assert_eq!((c.portal, c.cell), (102, 67));
    assert_eq!(c.portal_base, [7; 32]);
    assert_eq!(c.cell_base, [0; 32]);
}

#[test]
fn the_newest_iteration_is_read_from_the_runs_not_the_lists_count() {
    let set = |iterations, ints: &[i32]| MostlyConsecutiveIntSet {
        iterations,
        ints: ints.to_vec(),
    };
    assert_eq!(newest_iteration(&set(102, &[-102, 1])), 102);
    assert_eq!(newest_iteration(&set(103, &[-102, 1, 105])), 105);
    assert_eq!(newest_iteration(&set(1, &[10_007])), 10_007);
    assert_eq!(newest_iteration(&set(5, &[1, 2, -3, 8])), 10);
    assert_eq!(newest_iteration(&set(0, &[])), 0);
}

#[test]
fn nothing_pushed_at_the_same_iterations_is_nothing_to_serve() {
    let mut push = Push::default();
    push.begin(begin(102, 67));
    let serve = push.serve("World", &client());
    assert!(serve.begin.is_none());
    assert!(serve.data.is_empty());
    assert_eq!(serve.manifest.world_key, "World");
    assert!(serve.manifest.files.is_empty());
}

#[test]
fn a_push_becomes_a_manifest_a_begin_and_each_record_for_the_right_file() {
    let mut push = Push::default();
    push.begin(begin(10_007, 10_007));
    // The server sends its begin more than once; the first says it all.
    push.begin(begin(1, 1));
    push.record(&record(PORTAL_ID, 0x27, b"portal bytes"));
    push.record(&record(CELL_ID, 0, b"cell bytes"));
    let serve = push.serve("World", &client());

    let begin = serve.begin.expect("something to do");
    assert_eq!(begin.data_expected, 22);
    assert_eq!(begin.revisions.len(), 2);
    let portal = &begin.revisions[0];
    assert_eq!((portal.dat_file_type, portal.dat_file_id), (0, 1));
    assert_eq!(
        (portal.iteration, portal.ids_to_download[0]),
        (10_007, PORTAL_ID)
    );
    let cell = &begin.revisions[1];
    assert_eq!((cell.dat_file_type, cell.dat_file_id), (1, 2));
    assert_eq!(cell.ids_to_download, [CELL_ID]);

    assert_eq!(serve.data.len(), 2);
    for d in &serve.data {
        assert_eq!(d.compressed, 0);
        assert_eq!(d.data_size as usize, d.data.len() + 4);
        assert_ne!(d.version, 0, "the client refuses version 0");
        assert_eq!(d.iteration, 10_007);
    }
    assert_eq!(serve.data[0].data, b"portal bytes");

    let m = &serve.manifest;
    assert_eq!(m.total_bytes, 22);
    assert_eq!(m.files.len(), 2);
    let f = &m.files[0];
    assert_eq!(
        f.base_fingerprint, [1; 32],
        "the client's own base, named back to it"
    );
    assert_eq!(f.base_iterations, 102);
    assert!(
        f.exact_iterations,
        "the server's numbers are the world's whole list"
    );
    assert_eq!(f.revisions, [10_007]);
    assert_eq!(f.records[0].sha256, record_hash(b"portal bytes"));
    assert_eq!(f.records[0].size, 12);
    assert_eq!(m.files[1].base_fingerprint, [2; 32]);
}

#[test]
fn a_file_the_push_leaves_alone_is_not_mentioned() {
    let mut push = Push::default();
    push.begin(begin(105, 67));
    push.record(&record(PORTAL_ID, 0x27, b"x"));
    let serve = push.serve("World", &client());
    assert_eq!(serve.manifest.files.len(), 1);
    assert_eq!(serve.begin.unwrap().revisions.len(), 1);
}

#[test]
fn an_iteration_that_moves_with_no_records_is_still_a_revision() {
    let mut push = Push::default();
    push.begin(begin(103, 67));
    let serve = push.serve("World", &client());
    let begin = serve.begin.expect("the iteration moved");
    assert_eq!(begin.revisions[0].iteration, 103);
    assert!(begin.revisions[0].ids_to_download.is_empty());
}

#[test]
fn a_record_that_does_not_inflate_is_counted_and_not_promised() {
    let mut push = Push::default();
    push.begin(begin(105, 67));
    let mut bad = record(PORTAL_ID, 0x27, b"x");
    bad.data.inflated_len = 99;
    push.record(&bad);
    assert_eq!(push.refused, 1);
    assert_eq!(push.held(), 0);
    assert!(push.serve("World", &client()).data.is_empty());
}

#[test]
fn a_record_dereths_download_would_refuse_is_held_back_rather_than_left_pending_for_ever() {
    assert!(dereth_takes(DatFileKind::Portal, PORTAL_ID));
    assert!(dereth_takes(DatFileKind::Cell, CELL_ID));
    assert!(
        dereth_takes(DatFileKind::Portal, 0x130F_0000),
        "the second set of regions, which the 1999 files hold"
    );
    assert!(!dereth_takes(DatFileKind::Portal, 0x0E00_0000));
    let mut push = Push::default();
    push.begin(begin(105, 67));
    push.record(&record(0x0E00_0000, 0x27, b"no such type"));
    push.record(&record(PORTAL_ID, 0x27, b"x"));
    assert_eq!(push.withheld, [0x0E00_0000]);
    let serve = push.serve("World", &client());
    let promised = &serve.begin.unwrap().revisions[0].ids_to_download;
    assert_eq!(promised, &[PORTAL_ID]);
    assert_eq!(serve.manifest.files[0].records.len(), 1);
}

#[test]
fn each_kind_of_cell_id_has_its_cache() {
    assert_eq!(cell_cache(0xA9B0_FFFF), 0);
    assert_eq!(cell_cache(0xA9B0_FFFE), 1);
    assert_eq!(cell_cache(0xA9B0_0100), 2);
}

#[test]
fn a_record_answering_a_request_keeps_the_type_asked_for() {
    let d = answer_cell(
        9,
        &CellRecord {
            id: CELL_ID,
            data: stored(b"cell"),
        },
        67,
    )
    .unwrap();
    assert_eq!(
        (d.resource_type, d.resource_id, d.iteration),
        (9, CELL_ID, 67)
    );
    assert_eq!((d.dat_file_type, d.dat_file_id), (1, 2));
}
