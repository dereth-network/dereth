//! ACE: Source/ACE.Database/ShardDatabaseWithCaching.cs::ShardDatabaseWithCaching
//! ShardDatabaseWithCaching fills by guid class, updates cached rows, maintenance drops past
//! retention each minute, zero retention caches nothing.
//! Fixture: synthetic account and shard records on the memory and SQLite backends.

use std::sync::Arc;
use std::time::Duration;

use empyrean_common::clock::VirtualClock;
use empyrean_common::dotnet::datetime::TimeSpan;
use empyrean_common::dotnet::DotNetDict;
use empyrean_entity::enums::PropertyInt;
use empyrean_entity::Biota;
use empyrean_store::{MemShard, ShardDatabase, ShardDatabaseWithCaching};

fn biota(id: u32, v: i32) -> Biota {
    let mut d = DotNetDict::new();
    d.insert(PropertyInt(1), v);
    Biota {
        id,
        weenie_class_id: 1,
        properties_int: Some(d),
        ..Default::default()
    }
}

fn caching(
    player_min: f64,
    other_min: f64,
) -> (Arc<VirtualClock>, ShardDatabaseWithCaching<MemShard>) {
    let clock = Arc::new(VirtualClock::default());
    let db = ShardDatabaseWithCaching::new(
        MemShard::new(),
        clock.clone(),
        TimeSpan::from_minutes(player_min),
        TimeSpan::from_minutes(other_min),
    );
    (clock, db)
}

#[test]
fn saves_and_reads_fill_the_cache_by_guid_class() {
    let (_clock, mut db) = caching(31.0, 11.0);
    assert!(db.save_biota(&mut biota(0x5000_0001, 1), false));
    assert!(db.save_biota(&mut biota(0x8000_0001, 1), false));
    assert!(
        db.save_biota(&mut biota(0x8000_0002, 1), true),
        "doNotAddToCache"
    );
    let mut keys = db.get_biota_cache_keys();
    keys.sort_unstable();
    assert_eq!(keys, vec![0x5000_0001, 0x8000_0001]);

    assert!(db.get_biota(0x8000_0002, true).is_some());
    assert_eq!(db.get_biota_cache_keys().len(), 2);
    assert!(db.get_biota(0x8000_0002, false).is_some());
    assert_eq!(db.get_biota_cache_keys().len(), 3);
}

#[test]
fn a_cached_save_updates_the_cached_rows_and_the_store() {
    let (_clock, mut db) = caching(31.0, 11.0);
    assert!(db.save_biota(&mut biota(0x8000_0001, 1), false));
    assert!(db.save_biota(&mut biota(0x8000_0001, 2), false));
    assert_eq!(
        db.get_biota(0x8000_0001, false)
            .unwrap()
            .get_property_int(PropertyInt(1)),
        Some(2)
    );
    assert_eq!(
        db.base()
            .get_biota(0x8000_0001, false)
            .unwrap()
            .get_property_int(PropertyInt(1)),
        Some(2)
    );

    assert!(db.remove_biota(0x8000_0001));
    assert!(db.get_biota_cache_keys().is_empty());
    assert!(db.get_biota(0x8000_0001, false).is_none());
}

#[test]
fn maintenance_drops_entries_past_their_retention_once_a_minute() {
    let (clock, mut db) = caching(31.0, 11.0);
    assert!(db.save_biota(&mut biota(0x5000_0001, 1), false));
    assert!(db.save_biota(&mut biota(0x8000_0001, 1), false));
    let _ = db.get_biota(0x8000_0009, false); // runs maintenance once: next run a minute later

    clock.advance(Duration::from_secs(12 * 60));
    let _ = db.get_biota(0x8000_0009, false);
    assert_eq!(
        db.get_biota_cache_keys(),
        vec![0x5000_0001],
        "the non-player entry expired after 11 minutes"
    );

    clock.advance(Duration::from_secs(20 * 60));
    let _ = db.get_biota(0x8000_0009, false);
    assert!(
        db.get_biota_cache_keys().is_empty(),
        "the player entry after 31"
    );
}

#[test]
fn zero_retention_caches_nothing() {
    let (_clock, mut db) = caching(0.0, 0.0);
    assert!(db.save_biota(&mut biota(0x5000_0001, 1), false));
    assert!(db.get_biota(0x5000_0001, false).is_some());
    assert!(db.get_biota_cache_keys().is_empty());
}
