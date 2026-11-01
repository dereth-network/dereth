//! Keyed tables.

use auberean_protocol::table::{Table, SMALL};
use auberean_protocol::wire::{Field, Reader, Writer};

/// A character's appearance table as a January 2017 server sent it on creating the character:
/// five appearance keys, added in increasing order, laid out by bucket (47 buckets) with the
/// newest entry of each bucket first.
/// Behaviour: protocol.table.laid-out-bucket-by-bucket-newest-first
#[test]
fn a_table_lays_out_its_entries_bucket_by_bucket_newest_first() {
    let added = [
        0x2000_0009u32,
        0x2000_000A,
        0x2000_000B,
        0x2000_0038,
        0x2000_0039,
    ];
    let t = Table::from_inserted(SMALL, added.iter().map(|k| (*k, 0u32)));
    let order: Vec<u32> = t.entries.iter().map(|(k, _)| *k).collect();
    assert_eq!(
        order,
        vec![
            0x2000_0038,
            0x2000_0009,
            0x2000_0039,
            0x2000_000A,
            0x2000_000B
        ]
    );
}

/// Behaviour: protocol.table.laid-out-bucket-by-bucket-newest-first
#[test]
fn a_table_header_carries_its_size_class_above_the_count() {
    let t = Table::from_inserted(SMALL, [(3u32, 0.5f32), (11, 1.0)]);
    let mut w = Writer::default();
    t.write(&mut w);
    let bytes = w.finish();
    assert_eq!(&bytes[..4], &[2, 0, 0, 2]);
    let back = Table::<f32>::read(&mut Reader::new(&bytes)).expect("reads");
    assert_eq!(back, t);
}
