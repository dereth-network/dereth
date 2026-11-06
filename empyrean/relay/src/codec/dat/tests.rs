//! The dat download messages, built from made-up records.

use super::*;
use dereth_protocol::{read_body, write_blob};

fn packed(len: u32, bytes: &[u8]) -> Packed {
    Packed {
        inflated_len: len,
        bytes: bytes.to_vec(),
    }
}

#[test]
fn a_begin_carries_both_iterations_counts_and_totals() {
    let begin = DatBegin {
        portal_iteration: 105,
        portal_records: 3,
        cell_iteration: 67,
        cell_records: 0,
        portal_bytes: 900,
        cell_bytes: 0,
    };
    let blob = write_blob(&begin).unwrap();
    assert_eq!(&blob[..4], &0xf7b8u32.to_le_bytes());
    assert_eq!(blob.len(), 28);
    assert_eq!(read_body::<DatBegin>(&blob[4..]).unwrap(), begin);
}

#[test]
fn a_record_names_its_cache_and_lands_in_the_file_that_keeps_it() {
    let record = DatRecord {
        id: 0x0E00_0123,
        cache: 0x27,
        data: packed(40, &[1, 2, 3, 4, 5]),
    };
    let blob = write_blob(&record).unwrap();
    // Id, cache, two lengths, five bytes padded to the word.
    assert_eq!(blob.len(), 4 + 16 + 8);
    let back = read_body::<DatRecord>(&blob[4..]).unwrap();
    assert_eq!(back, record);
    assert_eq!(back.file(), DatFileKind::Portal);
    for (cache, file) in [
        (0, DatFileKind::Cell),
        (2, DatFileKind::Cell),
        (3, DatFileKind::Portal),
    ] {
        assert_eq!(DatFileKind::of_cache(cache), file);
    }
}

#[test]
fn a_record_too_large_to_inflate_or_longer_than_its_message_is_refused() {
    for len in [0, MAX_RECORD] {
        let mut w = Writer::body();
        for v in [1u32, 0x27, 1, len] {
            w.u32(v);
        }
        w.bytes(&[0, 0, 0, 0]);
        assert!(read_body::<DatRecord>(w.as_slice()).is_err(), "{len}");
    }
    let mut w = Writer::body();
    for v in [1u32, 0x27, 9, 4] {
        w.u32(v);
    }
    w.bytes(&[0; 4]);
    assert!(read_body::<DatRecord>(w.as_slice()).is_err());
    // More than the alignment after the bytes is not a record.
    let mut w = Writer::body();
    for v in [1u32, 0x27, 1, 4] {
        w.u32(v);
    }
    w.bytes(&[7, 0, 0, 0, 0]);
    assert!(read_body::<DatRecord>(w.as_slice()).is_err());
}

#[test]
fn cell_records_and_pairs_round_trip() {
    let one = CellRecord {
        id: 0x1234_FFFF,
        data: packed(10, &[9; 7]),
    };
    let blob = write_blob(&one).unwrap();
    assert_eq!(read_body::<CellRecord>(&blob[4..]).unwrap(), one);
    for first in [None, Some(one.clone())] {
        let pair = CellPair {
            first,
            second: CellRecord {
                id: 0x1234_FFFE,
                data: packed(3, &[1, 2]),
            },
        };
        let blob = write_blob(&pair).unwrap();
        assert_eq!(read_body::<CellPair>(&blob[4..]).unwrap(), pair);
    }
}

#[test]
fn a_touch_list_and_the_cell_request_round_trip() {
    let touch = DatTouch {
        ids: vec![0x0100_0001, 0, 7],
    };
    let blob = write_blob(&touch).unwrap();
    assert_eq!(read_body::<DatTouch>(&blob[4..]).unwrap(), touch);
    let mut w = Writer::body();
    w.u32(1_000_000);
    assert!(read_body::<DatTouch>(w.as_slice()).is_err());
    let request = CellRequest {
        id: 0xA9B4_0100,
        cache: 2,
    };
    let blob = write_blob(&request).unwrap();
    assert_eq!(blob, [0xa9, 0xf7, 0, 0, 0, 1, 0xb4, 0xa9, 2, 0, 0, 0]);
}
