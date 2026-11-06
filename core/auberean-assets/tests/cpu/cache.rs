//! How the database keeps records: holders, the free list, and time, over records put in memory.

use std::sync::Arc;

use auberean_assets::strings::EnumMap;
use auberean_assets::Database;
use auberean_dat::RecordType;

const KIND: u32 = RecordType::ENUM_MAP.0;

fn map(id: u32) -> EnumMap {
    EnumMap {
        id,
        base: 0,
        names: vec![(1, "one".into())],
    }
}

fn id(n: u32) -> u32 {
    0x2300_0100 + n
}

/// Behaviour: data.database.free-list-keeps-released-records
#[test]
fn a_record_nobody_holds_stays_loaded_until_the_free_list_overflows() {
    let db = Database::new(Vec::new());
    let a = db.put(id(0), map(id(0))).expect("not loaded yet");
    assert!(db.put(id(0), map(id(0))).is_none());
    let first = a.arc();
    let b = a.clone();
    drop(a);
    assert_eq!(db.cached(KIND), (1, 0), "still held");
    assert!(db.get_if_using::<EnumMap>(id(0)).is_some());
    drop(b);
    assert_eq!(db.cached(KIND), (1, 1), "free, still loaded");
    assert!(db.get_if_using::<EnumMap>(id(0)).is_none());
    let again = db.get_if_loaded::<EnumMap>(id(0)).expect("in memory");
    assert!(Arc::ptr_eq(&again.arc(), &first));
    assert_eq!(db.cached(KIND), (1, 0), "held again, off the free list");
    drop(again);
    // Past the greatest size, the oldest freed record goes first.
    let (_, max) = RecordType(KIND).cache_sizes();
    for n in 1..=max {
        drop(db.put(id(n), map(id(n))));
    }
    assert_eq!(db.cached(KIND), (max as usize, max as usize));
    assert!(
        db.get_if_loaded::<EnumMap>(id(0)).is_none(),
        "the oldest went"
    );
    assert!(db.get_if_loaded::<EnumMap>(id(1)).is_some());
}

/// Behaviour: data.database.free-records-go-after-thirty-seconds
#[test]
fn past_the_ideal_number_the_oldest_free_record_goes_thirty_seconds_after_it_was_freed() {
    let db = Database::new(Vec::new());
    drop(db.put(id(0), map(id(0))));
    db.set_free_list_ideal(KIND, 1);
    db.use_time(100.0);
    drop(db.put(id(1), map(id(1))));
    drop(db.put(id(2), map(id(2))));
    assert_eq!(db.cached(KIND).1, 3);
    // Record 0 was freed at time 0, the others at 100.
    db.use_time(129.0);
    assert_eq!(db.cached(KIND).1, 2, "one at a time, the oldest");
    db.use_time(129.0);
    assert_eq!(db.cached(KIND).1, 2, "the next is not 30 seconds old yet");
    db.use_time(130.5);
    assert_eq!(db.cached(KIND).1, 1, "down to the ideal number");
    db.use_time(1_000.0);
    assert_eq!(db.cached(KIND).1, 1);
}

/// Behaviour: data.database.not-keeping-free-records
#[test]
fn a_cache_told_not_to_keep_free_records_throws_them_away() {
    let db = Database::new(Vec::new());
    drop(db.put(id(0), map(id(0))));
    assert!(db.keep_free_objects(Some(KIND), false));
    assert_eq!(db.cached(KIND), (0, 0), "the free one went at once");
    drop(db.put(id(1), map(id(1))));
    assert_eq!(db.cached(KIND), (0, 0), "and a released one goes");
    db.keep_free_objects(None, true);
    let held = db.put(id(2), map(id(2))).expect("new");
    held.not_in_free_list();
    drop(held);
    assert_eq!(db.cached(KIND), (0, 0), "a record kept off the list goes");
    drop(db.put(id(3), map(id(3))));
    db.flush_free_objects(None);
    assert_eq!(db.cached(KIND), (0, 0));
    assert!(db.unload_cell_data());
}

/// **Every record type the client makes a cache for has a decoder**: the seventy-seven types its
/// cache start makes an object cache for.
///
/// Behaviour: data.database.every-cached-type-has-a-decoder
#[test]
fn every_record_type_the_client_makes_a_cache_for_has_a_decoder() {
    const CACHED: [u32; 77] = [
        0x4000_0004,
        0x4000_0005,
        0x4000_0006,
        0x4000_0008,
        0x4000_0009,
        0x4000_000A,
        0x4000_000B,
        0x4000_000C,
        0x4000_000D,
        0x4000_000E,
        0x4000_000F,
        0x4000_0010,
        0x4000_0011,
        0x4000_0012,
        0x4000_0013,
        0x4000_0014,
        0x4000_0015,
        0x4000_0016,
        0x4000_0018,
        0x4000_0019,
        0x4000_001D,
        0x4000_0021,
        0x4000_0022,
        0x4000_0025,
        0x4000_0026,
        0x4000_0028,
        0x4000_0029,
        0x4000_002A,
        0x4000_002B,
        0x4000_002C,
        0x4000_002D,
        0x4000_002E,
        0x4000_002F,
        0x4000_0030,
        0x4000_0031,
        0x4000_0032,
        0x4000_0033,
        0x4000_0034,
        0x4000_0035,
        0x4000_0036,
        0x4000_0039,
        0x4000_003C,
        0x4000_003D,
        0x4000_003E,
        0x4000_003F,
        0x4000_0040,
        0x4000_0042,
        0x4000_0043,
        0x4000_0044,
        0x4000_0046,
        0x4000_0047,
        0x4000_0048,
        0x4000_0049,
        0x4000_004A,
        0x4000_004B,
        0x4000_004C,
        0x4000_004D,
        0x4000_004E,
        0x4000_004F,
        0x4000_0050,
        0x4000_0051,
        0x4000_0052,
        0x4000_0053,
        0x4000_0054,
        0x4000_0055,
        0x4000_0056,
        0x4000_0057,
        0x4000_0059,
        0x4000_005A,
        0x4000_005B,
        0x4000_005C,
        0x4000_005D,
        0x4000_005E,
        0x4000_005F,
        0x4000_0060,
        0x4000_0061,
        0x4000_0062,
    ];
    for kind in CACHED {
        assert!(
            auberean_assets::Record::has_decoder(RecordType(kind)),
            "{kind:#x}"
        );
    }
}
