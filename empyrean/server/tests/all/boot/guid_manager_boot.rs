//! ACE: Source/ACE.Server/Managers/GuidManager.cs::Initialize
//! Boot initialises GuidManager from the installed shard (and at range minimums over an empty
//! one) and a landblock load draws encounter guids.
//! Fixture: isolated configuration paths and synthetic server state.

use std::sync::Arc;

use empyrean_common::account_defaults::AccountDefaults;
use empyrean_common::clock::{Clock, ClockSnapshot, VirtualClock};
use empyrean_content::models::world::{Encounter, LandblockInstance, Weenie};
use empyrean_content::MemContent;
use empyrean_dat::FakeDats;
use empyrean_entity::enums::WeenieType;
use empyrean_entity::LandblockId;
use empyrean_server::database_manager::{self, InitializeOptions};
use empyrean_server::guid_manager_boot;
use empyrean_store::{MemAuth, MemShard, ShardDatabase};
use empyrean_world::managers::guid_manager as gm;
use empyrean_world::managers::landblock_manager as lm;
use empyrean_world::World;

fn content() -> MemContent {
    MemContent::new()
        .weenie(Weenie::new(1, "human", WeenieType::Creature))
        .weenie(Weenie::new(30, "drudge", WeenieType::Creature))
        .landblock_instance(LandblockInstance {
            guid: 0x7A9B_4000,
            weenie_class_id: 1,
            obj_cell_id: 0xA9B4_0001,
            ..LandblockInstance::default()
        })
        .encounter(Encounter {
            id: 1,
            landblock: 0xA9B4,
            weenie_class_id: 30,
            cell_x: 1,
            cell_y: 1,
            ..Encounter::default()
        })
        .encounter(Encounter {
            id: 2,
            landblock: 0xA9B4,
            weenie_class_id: 30,
            cell_x: 2,
            cell_y: 2,
            ..Encounter::default()
        })
}

fn stat_dats() -> Arc<empyrean_dat::DatManager> {
    let mut vital_table = empyrean_dat::file_id::SECONDARY_ATTRIBUTE_TABLE
        .to_le_bytes()
        .to_vec();
    vital_table.resize(4 + 3 * 6 * 4, 0); // three (w, x, y, z, attr1, attr2) formulas, X = 0
    FakeDats::new()
        .with_spell_table(empyrean_dat::fake::sample::spell_table())
        .with_raw(
            empyrean_dat::DatDatabaseType::Portal,
            empyrean_dat::file_id::SECONDARY_ATTRIBUTE_TABLE,
            vital_table,
        )
        .build()
        .expect("fake dats")
}

/// Boots as `Program.Main` does, up to `GuidManager.Initialize`, over a shard holding `ids`.
fn boot(ids: &[u32], threaded: bool) -> World {
    let clock: Arc<dyn Clock> = Arc::new(VirtualClock::default());
    let mut w = World::new(ClockSnapshot::take(&*clock, 0.0), stat_dats());
    w.content = Arc::new(content());
    let mut shard = MemShard::new();
    for &id in ids {
        let mut biota = empyrean_entity::Biota {
            id,
            weenie_class_id: 1,
            ..Default::default()
        };
        assert!(shard.save_biota(&mut biota, false));
    }
    let options = InitializeOptions {
        world_content_loaded: true,
        shard_player_biota_cache_time: 31,
        shard_non_player_biota_cache_time: 11,
        clock: Arc::clone(&clock),
        threaded,
    };
    let auth = Box::new(MemAuth::new(AccountDefaults::default(), clock));
    assert!(
        !database_manager::initialize(&mut w, auth, shard, &options),
        "InitializationFailure"
    );
    database_manager::start(&mut w);
    guid_manager_boot::initialize(&mut w);
    w
}

#[test]
fn boot_initializes_guid_manager_from_the_installed_shard() {
    let mut w = boot(
        &[
            0x5000_0005,
            0x5000_0002,
            0x8000_0003,
            0x8000_0010,
            0x7000_0001,
        ],
        true,
    );
    // PlayerGuidAllocator: the range's max + 1.
    assert_eq!(
        w.guid_manager
            .player_alloc
            .as_ref()
            .expect("initialized")
            .current(),
        0x5000_0006
    );
    // DynamicGuidAllocator: max + 1, and the unused runs between used ids as sequence gaps
    // (GetSequenceGaps starts after a used id, so 0x80000000..=02 is not one).
    let dynamic = w.guid_manager.dynamic_alloc.as_ref().expect("initialized");
    assert_eq!(dynamic.current(), 0x8000_0011);
    assert_eq!(
        dynamic.sequence_gap_total_available(),
        12,
        "0x80000004..=0F"
    );
    // The shard handle is back in the world.
    assert!(w
        .shard
        .base_database()
        .get_biota(0x5000_0005, false)
        .is_some());

    let id = LandblockId::new(0xA9B4_FFFF);
    lm::get_landblock(&mut w, id, false, false);
    assert_eq!(
        gm::new_dynamic_guid(&mut w).full(),
        0x8000_0006,
        "the encounters took 0x80000004 and 0x80000005"
    );
    assert_eq!(gm::new_player_guid(&mut w).full(), 0x5000_0006);
    database_manager::stop(&mut w);
}

#[test]
fn boot_over_an_empty_shard_starts_each_range_at_its_minimum() {
    let w = boot(&[], false);
    assert_eq!(
        w.guid_manager
            .player_alloc
            .as_ref()
            .expect("initialized")
            .current(),
        0x5000_0001
    );
    assert_eq!(
        w.guid_manager
            .dynamic_alloc
            .as_ref()
            .expect("initialized")
            .current(),
        0x8000_0000
    );
}
