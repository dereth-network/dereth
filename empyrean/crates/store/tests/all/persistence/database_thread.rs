//! ACE: Source/ACE.Database/SerializedShardDatabase.cs::SerializedShardDatabase
//! SerializedShardDatabase callbacks run on the world in order after commit; a failed commit
//! stops the writer; a failing save or panicking job does not; queued jobs merged in order;
//! MemShard rollback exact.
//! Fixture: synthetic account and shard records on the memory and SQLite backends.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use empyrean_common::clock::VirtualClock;
use empyrean_common::dotnet::DotNetDict;
use empyrean_entity::enums::PropertyInt;
use empyrean_entity::Biota;
use empyrean_store::models::shard::Biota as DbBiota;
use empyrean_store::models::shard::Character;
use empyrean_store::shard_database::{BiotaQuery, CharacterQuery, PopulatedCollectionFlags};
use empyrean_store::{MemShard, ShardDatabase, ShardHandle, SqliteShard, StoreError};

/// The stand-in for the world: records what the callbacks saw.
#[derive(Debug, Default)]
struct World {
    log: Vec<String>,
}

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

fn handle(db: Box<dyn ShardDatabase>) -> ShardHandle<World> {
    ShardHandle::synchronous(db, Arc::new(VirtualClock::default()))
}

#[test]
fn callbacks_run_on_the_world_in_order() {
    let h = handle(Box::new(SqliteShard::open_in_memory().unwrap()));
    let mut w = World::default();

    h.save_biota(
        biota(0x8000_0001, 5),
        Some(Box::new(|w: &mut World, ok| {
            w.log.push(format!("save {ok}"))
        })),
    );
    h.save_biotas_in_parallel(
        vec![biota(0x8000_0002, 1), biota(0x8000_0003, 1)],
        Some(Box::new(|w: &mut World, ok| {
            w.log.push(format!("many {ok}"))
        })),
        false,
    );
    h.save_character(
        Character {
            id: 0x5000_0001,
            account_id: 4,
            name: "A".into(),
            ..Default::default()
        },
        None,
    );
    h.get_max_guid_found_in_range(
        0x8000_0000,
        0x8FFF_FFFF,
        Some(Box::new(|w: &mut World, id| {
            w.log.push(format!("max {id:x}"))
        })),
    );
    h.get_characters(
        4,
        false,
        Some(Box::new(|w: &mut World, cs: Vec<Character>| {
            w.log.push(format!("chars {}", cs.len()))
        })),
    );
    h.is_character_name_available(
        "a".into(),
        Some(Box::new(|w: &mut World, free| {
            w.log.push(format!("free {free}"))
        })),
    );
    h.remove_biota(
        0x8000_0003,
        Some(Box::new(|w: &mut World, ok| {
            w.log.push(format!("remove {ok}"))
        })),
    );
    h.get_current_queue_wait_time(Some(Box::new(|w: &mut World, t| {
        w.log.push(format!("wait {}", t.ticks()))
    })));

    assert!(w.log.is_empty(), "nothing runs until the world drains");
    h.drain(&mut w);
    assert_eq!(
        w.log,
        vec![
            "save true",
            "many true",
            "max 80000003",
            "chars 1",
            "free false",
            "remove true",
            "wait 0"
        ]
    );
    assert_eq!(h.queue_count(), 0);
    assert!(h.base_database().get_biota(0x8000_0003, false).is_none());
}

#[test]
fn a_failed_commit_stops_the_writer_and_reports_false() {
    let mut mem = MemShard::new();
    mem.fail_next_commits(1);
    let h = handle(Box::new(mem));
    let mut w = World::default();

    h.save_biota(
        biota(0x8000_0001, 1),
        Some(Box::new(|w: &mut World, ok| {
            w.log.push(format!("first {ok}"))
        })),
    );
    h.drain(&mut w);
    assert_eq!(
        w.log,
        vec!["first false"],
        "the save ran but its commit failed"
    );
    assert!(h.is_writer_stopped());
    assert!(
        h.base_database().get_biota(0x8000_0001, false).is_none(),
        "rolled back"
    );

    // Later writes are refused without running; reads still work.
    h.save_biota(
        biota(0x8000_0002, 1),
        Some(Box::new(|w: &mut World, ok| {
            w.log.push(format!("second {ok}"))
        })),
    );
    h.get_max_guid_found_in_range(
        0,
        u32::MAX,
        Some(Box::new(|w: &mut World, id| {
            w.log.push(format!("max {id:x}"))
        })),
    );
    h.drain(&mut w);
    assert_eq!(&w.log[1..], ["second false", "max ffffffff"]);
    assert!(h.base_database().get_biota(0x8000_0002, false).is_none());
}

#[test]
fn a_failing_save_reports_false_without_stopping_the_writer() {
    let mut mem = MemShard::new();
    mem.fail_writes_of(0x8000_0001, true);
    let h = handle(Box::new(mem));
    let mut w = World::default();
    h.save_biota(
        biota(0x8000_0001, 1),
        Some(Box::new(|w: &mut World, ok| w.log.push(format!("{ok}")))),
    );
    h.save_biota(
        biota(0x8000_0002, 1),
        Some(Box::new(|w: &mut World, ok| w.log.push(format!("{ok}")))),
    );
    h.drain(&mut w);
    assert_eq!(w.log, vec!["false", "true"]);
    assert!(!h.is_writer_stopped());
}

#[test]
fn a_panicking_job_drops_its_callback_like_doworks_catch() {
    let mut mem = MemShard::new();
    let mut item = biota(0x8000_0002, 1);
    let mut iids = DotNetDict::new();
    iids.insert(
        empyrean_entity::enums::PropertyInstanceId::Container,
        0x5000_0001u32,
    );
    item.properties_iid = Some(iids);
    assert!(mem.save_biota(&mut item, false));
    mem.fail_reads_of(0x8000_0002, true);
    let h = handle(Box::new(mem));
    let mut w = World::default();

    // Reading the inventory hits the failing row: the job throws, is logged, and its callback never runs.
    h.get_inventory_in_parallel(
        0x5000_0001,
        true,
        Some(Box::new(|w: &mut World, items| {
            w.log.push(format!("inv {}", items.len()))
        })),
    );
    h.save_biota(
        biota(0x8000_0001, 1),
        Some(Box::new(|w: &mut World, ok| w.log.push(format!("{ok}")))),
    );
    h.drain(&mut w);
    assert_eq!(w.log, vec!["true"]);
    assert!(!h.is_writer_stopped());

    // SetCharacterAccessLevelByName is not implemented in ACE either: it throws on the caller.
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        h.set_character_access_level_by_name("x", empyrean_entity::enums::AccessLevel(1), None);
    }));
    assert!(r.is_err());
}

/// A MemShard that counts committed batches.
#[derive(Debug)]
struct Counting(MemShard, Arc<AtomicUsize>);

impl ShardDatabase for Counting {
    fn load_biota_row(&mut self, id: u32) -> Result<Option<DbBiota>, StoreError> {
        self.0.load_biota_row(id)
    }
    fn load_biota_collections(
        &mut self,
        b: &mut DbBiota,
        f: PopulatedCollectionFlags,
    ) -> Result<(), StoreError> {
        self.0.load_biota_collections(b, f)
    }
    fn write_biota(&mut self, b: &mut DbBiota) -> Result<(), StoreError> {
        self.0.write_biota(b)
    }
    fn delete_biota(&mut self, id: u32) -> Result<(), StoreError> {
        self.0.delete_biota(id)
    }
    fn query_biota_ids(&mut self, q: BiotaQuery) -> Result<Vec<u32>, StoreError> {
        self.0.query_biota_ids(q)
    }
    fn count_biotas(&mut self) -> Result<i64, StoreError> {
        self.0.count_biotas()
    }
    fn query_characters(&mut self, q: CharacterQuery<'_>) -> Result<Vec<Character>, StoreError> {
        self.0.query_characters(q)
    }
    fn load_character_properties(&mut self, c: &mut Character) -> Result<(), StoreError> {
        self.0.load_character_properties(c)
    }
    fn write_character(&mut self, c: &Character) -> Result<(), StoreError> {
        self.0.write_character(c)
    }
    fn begin_batch(&mut self) -> Result<(), StoreError> {
        self.0.begin_batch()
    }
    fn commit_batch(&mut self) -> Result<(), StoreError> {
        self.1.fetch_add(1, Ordering::SeqCst);
        self.0.commit_batch()
    }
    fn rollback_batch(&mut self) {
        self.0.rollback_batch();
    }
}

#[test]
fn the_thread_merges_queued_jobs_and_keeps_their_order() {
    let commits = Arc::new(AtomicUsize::new(0));
    let mut h: ShardHandle<World> = ShardHandle::new(
        Box::new(Counting(MemShard::new(), commits.clone())),
        Arc::new(VirtualClock::default()),
    );
    h.start();
    {
        // Hold the database while queueing, so the thread cannot run a batch until all are queued.
        let _held = h.base_database();
        for i in 0..200u32 {
            h.save_biota(
                biota(0x8000_0000 + i, i32::try_from(i).unwrap()),
                Some(Box::new(move |w: &mut World, ok| {
                    w.log.push(format!("{i} {ok}"))
                })),
            );
        }
        h.get_max_guid_found_in_range(
            0x8000_0000,
            0x8FFF_FFFF,
            Some(Box::new(|w: &mut World, id| {
                w.log.push(format!("max {id:x}"))
            })),
        );
    }
    h.stop(); // finishes the queue, then joins
    let mut w = World::default();
    h.drain(&mut w);
    let want: Vec<String> = (0..200)
        .map(|i| format!("{i} true"))
        .chain(std::iter::once("max 800000c7".to_string()))
        .collect();
    assert_eq!(w.log, want);
    assert_eq!(h.base_database().get_biota_count(), 200);
    // 201 jobs in batches of at most 64: the first batch is whatever was queued when the thread
    // woke (1..=64 jobs), then full batches.
    let n = commits.load(Ordering::SeqCst);
    assert!((4..=5).contains(&n), "{n} commits");
}

/// A mem shard rollback puts back exactly what the batch changed.
#[test]
fn a_mem_shard_rollback_puts_back_exactly_what_the_batch_changed() {
    use empyrean_store::models::shard::{
        BiotaPropertiesAllegiance, BiotaPropertiesInt, BiotaPropertiesPalette,
    };
    let row = |id: u32, v: i32| DbBiota {
        id,
        weenie_class_id: 1,
        biota_properties_int: vec![BiotaPropertiesInt {
            object_id: id,
            r#type: 1,
            value: v,
        }],
        biota_properties_palette: vec![BiotaPropertiesPalette {
            object_id: id,
            sub_palette_id: 7,
            ..Default::default()
        }],
        ..Default::default()
    };
    let with_allegiance = |mut b: DbBiota, character: u32| {
        b.biota_properties_allegiance
            .push(BiotaPropertiesAllegiance {
                allegiance_id: b.id,
                character_id: character,
                ..Default::default()
            });
        b
    };
    let character = |id: u32| Character {
        id,
        account_id: 1,
        name: format!("C{id:x}"),
        ..Default::default()
    };

    let mut mem = MemShard::new();
    mem.write_character(&character(0x5000_0001)).unwrap();
    mem.write_character(&character(0x5000_0002)).unwrap();
    mem.write_biota(&mut row(0x8000_0001, 1)).unwrap();
    mem.write_biota(&mut with_allegiance(row(0x8000_0002, 1), 0x5000_0001))
        .unwrap();
    mem.write_biota(&mut with_allegiance(row(0x8000_0003, 1), 0x5000_0002))
        .unwrap();
    let before = format!("{mem:?}");

    let change = |mem: &mut MemShard| {
        mem.begin_batch().unwrap();
        mem.write_biota(&mut row(0x8000_0001, 2)).unwrap();
        mem.delete_biota(0x8000_0003).unwrap();
        mem.write_biota(&mut row(0x8000_0004, 1)).unwrap();
        mem.write_character(&character(0x5000_0003)).unwrap();
        mem.delete_character(0x5000_0001).unwrap();
    };
    change(&mut mem);
    assert_ne!(format!("{mem:?}"), before);
    mem.rollback_batch();
    assert_eq!(
        format!("{mem:?}"),
        before,
        "the rollback put back every row, the cascade and the sequence"
    );

    change(&mut mem);
    mem.commit_batch().unwrap();
    let stored = mem.stored_biotas();
    let ids: Vec<u32> = stored.iter().map(|b| b.id).collect();
    assert_eq!(ids, [0x8000_0001, 0x8000_0002, 0x8000_0004]);
    assert_eq!(stored[0].biota_properties_int[0].value, 2);
    assert!(stored[1].biota_properties_allegiance.is_empty(), "cascaded");
    // the rolled-back batch's row ids are handed out again (the sequence was put back, as SQLite's is)
    assert_eq!(
        (
            stored[0].biota_properties_palette[0].id,
            stored[2].biota_properties_palette[0].id
        ),
        (4, 5)
    );
}
