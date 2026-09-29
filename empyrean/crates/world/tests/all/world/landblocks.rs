//! ACE: Source/ACE.Server/Entity/Landblock.cs::TickMultiThreadedWork
//! GuidManager, Landblock, LandblockManager, groups/split, mesh and actors follow ACE.
//! Fixture: Synthetic landblocks and a mock shard store.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use dereth_primitives::DataId;
use empyrean_common::clock::{ClockSnapshot, VirtualClock};
use empyrean_common::dotnet::datetime::TimeSpan;
use empyrean_common::game_configuration::GameConfiguration;
use empyrean_common::not_ported;
use empyrean_common::preloaded_landblock::PreloadedLandblocks;
use empyrean_content::models::world::{Encounter, LandblockInstance, Weenie};
use empyrean_content::MemContent;
use empyrean_dat::file_types::{CellLandblock, LandblockInfo};
use empyrean_dat::{DatManager, FakeDats};
use empyrean_entity::enums::{PropertyDataId, WeenieType};
use empyrean_entity::numerics::Vector2;
use empyrean_entity::{LandblockId, ObjectGuid, Position};
use empyrean_testkit::land;
use empyrean_world::entity::actions::action_queue::run_actions;
use empyrean_world::entity::actions::i_action::Action;
use empyrean_world::entity::actions::i_actor::{action_queue_mut, enqueue, Actor};
use empyrean_world::entity::adjacency::Adjacency;
use empyrean_world::entity::landblock::{self, SortedByTime};
use empyrean_world::entity::landblock_group::LandblockGroup;
use empyrean_world::entity::landblock_mesh::LandblockMesh;
use empyrean_world::managers::guid_manager::{
    self as gm, DynamicGuidAllocator, PlayerGuidAllocator, ShardGuidQueries, INVALID_GUID,
};
use empyrean_world::managers::landblock_manager as lm;
use empyrean_world::world_objects::kinds::KindData;
use empyrean_world::world_objects::world_object::WorldObject;
use empyrean_world::World;

// ---------------------------------------------------------------------------------------------
// Harness

/// The shard reads `GuidManager` makes at start-up, answered from fixed values.
struct FakeShard {
    max: HashMap<(u32, u32), u32>,
    gaps: Vec<(u32, u32)>,
    gap_limit_seen: Option<u32>,
}

impl FakeShard {
    fn empty() -> Self {
        FakeShard {
            max: HashMap::new(),
            gaps: Vec::new(),
            gap_limit_seen: None,
        }
    }
}

impl ShardGuidQueries for FakeShard {
    fn get_max_guid_found_in_range(&mut self, min: u32, max: u32) -> u32 {
        self.max.get(&(min, max)).copied().unwrap_or(u32::MAX)
    }

    fn get_sequence_gaps(&mut self, _min: u32, limit: u32) -> Vec<(u32, u32)> {
        self.gap_limit_seen = Some(limit);
        self.gaps.clone()
    }
}

struct H {
    w: World,
    clock: VirtualClock,
}

const SETUP: u32 = land::TEST_SETUP;

/// The landblocks the tests place objects on: flat land at height 0.
const LAND: [u16; 6] = [0xA9B4, 0x3030, 0x3131, 0x3232, 0x3434, 0x3534];

impl H {
    fn new(content: MemContent, dats: Arc<DatManager>) -> Self {
        let clock = VirtualClock::default();
        let now = ClockSnapshot::take(&clock, 0.0);
        let mut w = World::new(now, dats);
        w.content = Arc::new(content);
        gm::initialize(&mut w, &mut FakeShard::empty());
        land::use_flat_land_with_test_setup(&mut w, &LAND, 0);
        H { w, clock }
    }

    fn plain() -> Self {
        Self::new(MemContent::new(), FakeDats::new().build().unwrap())
    }

    /// One `LandblockManager.Tick` at the clock's time.
    fn tick(&mut self) {
        self.w.now = ClockSnapshot::take(&self.clock, self.w.timers.portal_year_ticks);
        let pyt = self.w.timers.portal_year_ticks;
        lm::tick(&mut self.w, pyt);
    }

    fn advance(&mut self, secs: f64) {
        self.clock.advance(Duration::from_secs_f64(secs));
    }

    fn lb(&self, id: LandblockId) -> &landblock::Landblock {
        self.w.landblock_manager.landblocks.expect(id)
    }

    fn objects_of(&self, id: LandblockId) -> Vec<u32> {
        self.lb(id).world_object_guids().map(|g| g.full()).collect()
    }

    /// Puts an object in the store at `cell` and adds it to its landblock through `AddObject`.
    fn spawn(&mut self, wo: WorldObject, cell: u32) -> ObjectGuid {
        let g = wo.guid;
        self.w.objects.insert(wo).unwrap();
        self.w
            .objects
            .get_mut(g)
            .unwrap()
            .set_location(Some(Position::from_components(
                cell, 10.0, 10.0, 0.0, 0.0, 0.0, 0.0, 1.0, false,
            )));
        assert!(lm::add_object(&mut self.w, g, false));
        g
    }
}

fn lbid(x: u8, y: u8) -> LandblockId {
    LandblockId::from_xy(x, y)
}

fn plain_object(guid: u32, wcid: u32) -> WorldObject {
    let mut o = WorldObject {
        guid: ObjectGuid::new(guid),
        ..WorldObject::default()
    };
    o.biota.id = guid;
    o.biota.weenie_class_id = wcid;
    o.set_property(PropertyDataId::Setup, SETUP);
    // The constructor's InitializeHeartbeats (these objects skip the constructor): with a
    // HeartbeatInterval of 0 the object never heartbeats and is not a generator.
    o.set_heartbeat_interval(Some(0.0));
    empyrean_world::world_objects::world_object_tick::world_object_initialize_heartbeats(
        &mut o, 0.0,
    );
    o
}

fn creature(guid: u32) -> WorldObject {
    WorldObject {
        creature: Some(Box::default()),
        ..plain_object(guid, 1)
    }
}

fn player(guid: u32) -> WorldObject {
    WorldObject {
        creature: Some(Box::default()),
        container: Some(Box::default()),
        player: Some(Box::default()),
        ..plain_object(guid, 1)
    }
}

type Log = Arc<Mutex<Vec<String>>>;

fn note(l: &Log, s: &str) -> Action {
    let (l, s) = (Arc::clone(l), s.to_owned());
    Action::delegate(move |_w: &mut World| l.lock().unwrap().push(s))
}

fn entries(l: &Log) -> Vec<String> {
    l.lock().unwrap().clone()
}

// ---------------------------------------------------------------------------------------------
// GuidManager

#[test]
fn player_allocator_starts_at_min_or_after_the_shard_max_and_stops_at_max() {
    let mut shard = FakeShard::empty();
    let mut a = PlayerGuidAllocator::new(0x5000_0001, 0x5FFF_FFFF, "player", &mut shard);
    assert_eq!(a.current(), 0x5000_0001, "an empty range starts at min");
    assert_eq!(a.alloc(), 0x5000_0001);
    assert_eq!(a.alloc(), 0x5000_0002);
    assert_eq!(
        (a.min(), a.max(), a.current()),
        (0x5000_0001, 0x5FFF_FFFF, 0x5000_0003)
    );

    shard.max.insert((0x5000_0001, 0x5000_0010), 0x5000_000E);
    let mut a = PlayerGuidAllocator::new(0x5000_0001, 0x5000_0010, "player", &mut shard);
    assert_eq!(a.alloc(), 0x5000_000F, "the shard's max + 1");
    assert_eq!(a.alloc(), INVALID_GUID, "current == max: out of guids");
    assert_eq!(a.alloc(), INVALID_GUID);
    assert_eq!(a.current(), 0x5000_0010);

    // ACE-BUG: a full range starts at max + 1 and is never reported exhausted.
    shard.max.insert((0x5000_0001, 0x5000_0010), 0x5000_0010);
    let mut a = PlayerGuidAllocator::new(0x5000_0001, 0x5000_0010, "player", &mut shard);
    assert_eq!(a.alloc(), 0x5000_0011);
}

#[test]
fn dynamic_allocator_uses_old_recycled_guids_then_gaps_then_current() {
    let mut shard = FakeShard::empty();
    shard.max.insert((0x8000_0000, 0xFFFF_FFFE), 0x8000_0100);
    shard.gaps = vec![(0x8000_0010, 0x8000_0011), (0x8000_0020, 0x8000_0020)];
    let mut a = DynamicGuidAllocator::new(0x8000_0000, 0xFFFF_FFFE, "dynamic", false, &mut shard);
    assert_eq!(
        shard.gap_limit_seen,
        Some(10_000_000),
        "limitAvailableIDsReturnedInGetSequenceGaps"
    );
    assert_eq!(
        (
            a.sequence_gap_pairs_total(),
            a.sequence_gap_total_available()
        ),
        (2, 3)
    );

    let t0 = empyrean_common::dotnet::DotNetDateTime::new(2026, 1, 1);
    a.recycle(0x8000_0005, t0);
    assert_eq!(a.recycled_guids_total(), 1);

    // A recycled guid waits more than 360 minutes; gaps come first meanwhile, in order.
    assert_eq!(a.alloc(t0), 0x8000_0010);
    assert_eq!(
        a.alloc(t0 + TimeSpan::from_minutes(360.0)),
        0x8000_0011,
        "exactly 360 minutes is not enough"
    );
    assert_eq!(
        a.alloc(t0 + TimeSpan::from_minutes(360.0) + TimeSpan::from_ticks(1)),
        0x8000_0005
    );
    assert_eq!(a.alloc(t0), 0x8000_0020);
    assert_eq!(a.sequence_gap_pairs_total(), 0);
    assert_eq!(a.alloc(t0), 0x8000_0101, "then the shard's max + 1");
    assert_eq!(a.alloc(t0), 0x8000_0102);
    assert_eq!(
        a.to_string(),
        "DynamicGuidAllocator: dynamic, current: 0x80000103, max: 0xFFFFFFFE, sequence gap GUIDs available: 0, recycled GUIDs available: 0"
    );

    let mut shard = FakeShard::empty();
    DynamicGuidAllocator::new(0x8000_0000, 0xFFFF_FFFE, "dynamic", true, &mut shard);
    assert_eq!(
        shard.gap_limit_seen,
        Some(u32::MAX),
        "unlimited_sequence_gaps"
    );
}

#[test]
fn guid_manager_statics_and_id_list_output() {
    let mut w = World::new(
        ClockSnapshot::take(&VirtualClock::default(), 0.0),
        FakeDats::new().build().unwrap(),
    );
    assert_eq!(
        (
            gm::player_min(&w),
            gm::dynamic_current(&w),
            gm::recycled_guids_total(&w)
        ),
        (0, 0, 0),
        "before Initialize"
    );

    let mut shard = FakeShard::empty();
    shard.gaps = vec![(0x8000_0001, 0x8000_1000)];
    gm::initialize(&mut w, &mut shard);
    assert_eq!(gm::new_player_guid(&mut w), ObjectGuid::new(0x5000_0001));
    assert_eq!(gm::new_dynamic_guid(&mut w), ObjectGuid::new(0x8000_0001));
    assert_eq!(
        (gm::player_current(&w), gm::player_max(&w)),
        (0x5000_0002, 0x5FFF_FFFF)
    );
    assert_eq!(
        (
            gm::dynamic_min(&w),
            gm::dynamic_max(&w),
            gm::sequence_gap_pairs_total(&w)
        ),
        (0x8000_0000, 0xFFFF_FFFE, 1)
    );

    assert_eq!(
        gm::get_id_list_command_output(&mut w),
        "The next Player GUID to be allocated is expected to be: 0x50000002\nAfter 4,095 sequence gap ids have been consumed, and 0 recycled ids have been consumed, the next id will be 80000000"
    );

    gm::recycle_dynamic_guid(&mut w, ObjectGuid::new(0x8000_0001));
    w.now = ClockSnapshot::take(
        &VirtualClock::new(w.now.utc + TimeSpan::from_minutes(60.0)),
        0.0,
    );
    assert_eq!(
        gm::get_id_list_command_output(&mut w),
        "The next Player GUID to be allocated is expected to be: 0x50000002\nAfter 4,095 sequence gap ids have been consumed, and 1 recycled ids have been consumed, the next of which is available in 300.0 m, the next id will be: 0x80000000"
    );
}

// ---------------------------------------------------------------------------------------------
// Loading

const LB: u32 = 0xA9B4_0000;

fn weenie(wcid: u32) -> Weenie {
    Weenie::new(wcid, &format!("w{wcid}"), WeenieType::Generic)
        .with_did(PropertyDataId::Setup, SETUP)
}

#[test]
fn loading_a_landblock_creates_its_instances_in_ace_order_then_its_encounters() {
    let child = LandblockInstance {
        is_link_child: true,
        ..LandblockInstance::new(0x7A9B_4003, 11, LB | 0x0001, [1.0, 1.0, 0.0])
    };
    let content = MemContent::new()
        .weenie(weenie(10))
        .weenie(weenie(11))
        .weenie(weenie(12))
        // The landblock keeps the world database's row order (MemContent: the order rows were added).
        .landblock_instance(LandblockInstance::new(
            0x7A9B_4005,
            10,
            LB | 0x0002,
            [20.0, 30.0, 5.0],
        ))
        .landblock_instance(LandblockInstance::new(
            0x7A9B_4001,
            11,
            LB | 0x0001,
            [1.0, 2.0, 3.0],
        ))
        .landblock_instance(child)
        .landblock_instance(LandblockInstance::new(
            0x7A9B_4002,
            999,
            LB | 0x0001,
            [1.0, 2.0, 3.0],
        )) // no weenie: skipped
        .landblock_instance(LandblockInstance::new(
            0x7A9B_4004,
            12,
            LB | 0x0012,
            [4.0, 5.0, 6.0],
        ))
        .encounter(Encounter {
            id: 1,
            landblock: 0xA9B4,
            weenie_class_id: 12,
            cell_x: 2,
            cell_y: 3,
            ..Encounter::default()
        })
        .encounter(Encounter {
            id: 2,
            landblock: 0xA9B4,
            weenie_class_id: 999,
            cell_x: 0,
            cell_y: 0,
            ..Encounter::default()
        })
        .encounter(Encounter {
            id: 3,
            landblock: 0xA9B4,
            weenie_class_id: 10,
            cell_x: 9,
            cell_y: 0,
            ..Encounter::default()
        });
    let mut h = H::new(content, FakeDats::new().build().unwrap());
    let id = LandblockId::new(LB | 0xFFFF);

    not_ported::take_local();
    lm::get_landblock(&mut h.w, id, false, false);
    assert!(lm::is_loaded(&h.w, id));
    assert!(!h.lb(id).create_world_objects_completed());
    assert_eq!(
        h.lb(id).action_queue().len(),
        4,
        "CreateWorldObjects, SpawnDynamicShardObjects, two encounters"
    );
    assert_eq!(
        gm::dynamic_current(&h.w),
        0x8000_0002,
        "encounter guids are drawn at load, in encounter order"
    );

    let db_order: Vec<u32> =
        h.w.content
            .get_cached_instances_by_landblock(0xA9B4)
            .iter()
            .map(|i| i.guid)
            .collect();
    assert_eq!(
        db_order,
        [
            0x7A9B_4005,
            0x7A9B_4001,
            0x7A9B_4003,
            0x7A9B_4002,
            0x7A9B_4004
        ]
    );

    h.tick();
    assert!(h.lb(id).create_world_objects_completed());
    // Link children and instances without a weenie are skipped; encounters follow, in the
    // database's (cell_x, cell_y) order.
    assert_eq!(
        h.objects_of(id),
        [
            0x7A9B_4005,
            0x7A9B_4001,
            0x7A9B_4004,
            0x8000_0000,
            0x8000_0001
        ]
    );
    for g in h.objects_of(id) {
        assert_eq!(
            h.w.objects
                .get(ObjectGuid::new(g))
                .unwrap()
                .current_landblock,
            Some(id)
        );
    }
    let o = h.w.objects.get(ObjectGuid::new(0x7A9B_4005)).unwrap();
    let loc = o.location().unwrap();
    assert_eq!(
        (
            loc.landblock_id().raw(),
            loc.pos().x,
            loc.pos().y,
            loc.pos().z
        ),
        (LB | 0x0002, 20.0, 30.0, 5.0)
    );
    // Encounter at cell (9, 0): x clamped to 191.5; y 0 clamped to 0.5.
    let enc =
        h.w.objects
            .get(ObjectGuid::new(0x8000_0001))
            .unwrap()
            .location()
            .unwrap();
    assert_eq!(
        (enc.landblock_id().landblock(), enc.pos().x, enc.pos().y),
        (0xA9B4, 191.5, 0.5)
    );
    let enc =
        h.w.objects
            .get(ObjectGuid::new(0x8000_0000))
            .unwrap()
            .location()
            .unwrap();
    assert_eq!((enc.pos().x, enc.pos().y), (48.0, 72.0));

    let hits = not_ported::take_local();
    assert_eq!(
        hits.get("ACE: WorldObjectFactory.CreateNewWorldObjects"),
        None
    );
    assert_eq!(
        hits.get("ACE: WorldObjectFactory.CreateNewWorldObject"),
        None
    );
    assert_eq!(hits.get("ACE: WorldObject.ActivateLinks"), None);
}

// ---------------------------------------------------------------------------------------------
// Adjacency

#[test]
fn adjacent_ids_run_north_south_west_east_then_diagonals_and_stop_at_the_map_edge() {
    let id = lbid(0x10, 0x20);
    let ids: Vec<(u8, u8)> = lm::adjacent_ids_of(id, false)
        .iter()
        .map(|a| (a.landblock_x(), a.landblock_y()))
        .collect();
    assert_eq!(
        ids,
        [
            (0x10, 0x21),
            (0x10, 0x1F),
            (0x0F, 0x20),
            (0x11, 0x20),
            (0x0F, 0x21),
            (0x11, 0x21),
            (0x0F, 0x1F),
            (0x11, 0x1F)
        ]
    );
    assert!(
        lm::adjacent_ids_of(id, true).is_empty(),
        "dungeons have no adjacents"
    );

    let corner: Vec<(u8, u8)> = lm::adjacent_ids_of(lbid(0, 0xFE), false)
        .iter()
        .map(|a| (a.landblock_x(), a.landblock_y()))
        .collect();
    assert_eq!(
        corner,
        [(0, 0xFD), (1, 0xFE), (1, 0xFD)],
        "254 is the last row and column"
    );
    assert_eq!(lm::get_adjacent_id(lbid(0xFE, 5), Adjacency::East), None);
    assert_eq!(
        lm::get_adjacent_id(lbid(0xFE, 5), Adjacency(99)).map(|a| a.raw()),
        Some(0xFE05_0000)
    );
}

#[test]
fn loading_with_adjacents_caches_only_loaded_neighbours() {
    let mut h = H::plain();
    let a = lbid(0x40, 0x40);
    lm::get_landblock(&mut h.w, a, true, false);
    assert_eq!(lm::get_loaded_landblocks(&h.w).len(), 9);
    assert_eq!(h.lb(a).adjacents, lm::adjacent_ids_of(a, false));
    // A neighbour's cache holds only the loaded blocks around it, in adjacency order.
    let north = lbid(0x40, 0x41);
    let got: Vec<(u8, u8)> = h
        .lb(north)
        .adjacents
        .iter()
        .map(|x| (x.landblock_x(), x.landblock_y()))
        .collect();
    assert_eq!(
        got,
        [
            (0x40, 0x40),
            (0x3F, 0x41),
            (0x41, 0x41),
            (0x3F, 0x40),
            (0x41, 0x40)
        ]
    );

    // A dungeon (flat, cells, no buildings) loads no adjacents.
    let d = lbid(0x01, 0x02);
    let dats = FakeDats::new()
        .with_cell(
            0x0102_FFFF,
            CellLandblock {
                id: DataId(0x0102_FFFF),
                lbi_exists: 1,
                terrain: [0; 81],
                height: [0; 81],
            },
        )
        .with_cell(
            0x0102_FFFE,
            LandblockInfo {
                id: DataId(0x0102_FFFE),
                num_cells: 3,
                objects: vec![],
                pack_mask: 0,
                buildings: vec![],
                restrictions: None,
            },
        )
        .build()
        .unwrap();
    let mut h = H::new(MemContent::new(), dats);
    lm::get_landblock(&mut h.w, d, true, false);
    assert_eq!(lm::get_loaded_landblocks(&h.w), [d]);
    assert!(h.w.landblock_manager.landblocks.expect_mut(d).is_dungeon());
    assert!(h.w.landblock_manager.landblocks.expect_mut(d).has_dungeon());
}

// ---------------------------------------------------------------------------------------------
// Groups

#[test]
fn landblocks_group_by_spacing_merge_through_a_bridge_and_dungeons_stand_alone() {
    let mut h = H::plain();
    lm::get_landblock(&mut h.w, lbid(0x10, 0x10), false, false);
    lm::get_landblock(&mut h.w, lbid(0x13, 0x10), false, false); // distance 3 < 4: same group
    lm::get_landblock(&mut h.w, lbid(0x20, 0x10), false, false); // far: its own group
    h.tick();
    let groups = h.w.landblock_manager.landblock_groups();
    // Pending additions are taken from the end of the list.
    let members: Vec<Vec<u8>> = groups
        .iter()
        .map(|g| g.iter().map(|l| l.landblock_x()).collect())
        .collect();
    assert_eq!(members, [vec![0x20], vec![0x13, 0x10]]);
    assert_eq!(
        groups[1].to_string(),
        "x: 0x10 - 0x13, y: 0x10 - 0x10, w:   4, h:   1, Count:    2"
    );
    assert_eq!(
        h.lb(lbid(0x10, 0x10)).current_landblock_group,
        Some(groups[1].id)
    );

    // 0x1D joins 0x20's group; 0x16 joins 0x13's; 0x1A (4 from 0x16, 3 from 0x1D) joins 0x20's.
    for x in [0x1D, 0x16, 0x1A] {
        lm::get_landblock(&mut h.w, lbid(x, 0x10), false, false);
        h.tick();
    }
    let groups = h.w.landblock_manager.landblock_groups();
    let members: Vec<Vec<u8>> = groups
        .iter()
        .map(|g| g.iter().map(|l| l.landblock_x()).collect())
        .collect();
    assert_eq!(members, [vec![0x20, 0x1D, 0x1A], vec![0x13, 0x10, 0x16]]);

    // 0x18 is within 3 of both: it joins the first eligible group, which absorbs the other.
    lm::get_landblock(&mut h.w, lbid(0x18, 0x10), false, false);
    h.tick();
    let groups = h.w.landblock_manager.landblock_groups();
    assert_eq!(groups.len(), 1);
    let members: Vec<u8> = groups[0].iter().map(|l| l.landblock_x()).collect();
    assert_eq!(members, [0x20, 0x1D, 0x1A, 0x18, 0x13, 0x10, 0x16]);
    let gid = groups[0].id;
    for x in [0x10, 0x13, 0x16, 0x18, 0x1A, 0x1D, 0x20] {
        assert_eq!(h.lb(lbid(x, 0x10)).current_landblock_group, Some(gid));
    }
    assert_eq!(lm::landblock_groups_count(&h.w), 1);
}

#[test]
fn a_group_splits_where_its_landblocks_are_too_far_apart() {
    let mut h = H::plain();
    for x in [0x10u8, 0x13, 0x16] {
        lm::get_landblock(&mut h.w, lbid(x, 0x10), false, false);
    }
    h.tick();
    assert_eq!(h.w.landblock_manager.landblock_groups().len(), 1);

    // Unload the middle one with splitting enabled (it needs a threading flag, as in ACE).
    h.w.landblock_manager.multi_threaded_landblock_group_ticking = true;
    h.w.landblock_manager
        .landblocks
        .expect_mut(lbid(0x13, 0x10))
        .permaload = false;
    lm::add_to_destruction_queue(&mut h.w, lbid(0x13, 0x10));
    // The group's throttle: no split before NextTrySplitTime (5 minutes after it was built).
    h.tick();
    assert_eq!(
        h.w.landblock_manager.landblock_groups().len(),
        1,
        "too early to split"
    );
    assert!(!lm::is_loaded(&h.w, lbid(0x13, 0x10)));

    let mut g = LandblockGroup::new(
        empyrean_world::entity::landblock_group::LandblockGroupId(99),
        h.w.now.utc,
    );
    let table = &mut h.w.landblock_manager.landblocks;
    assert!(g.add(table, lbid(0x10, 0x10)));
    assert!(g.add(table, lbid(0x16, 0x10)));
    assert!(!g.add(table, lbid(0x16, 0x10)), "already present");
    let mut next = 1000;
    let splits = g
        .try_split(table, &mut next, h.w.now.utc)
        .expect("not a dungeon group");
    assert_eq!(splits.len(), 1);
    let kept: Vec<u8> = g.iter().map(|l| l.landblock_x()).collect();
    let split: Vec<u8> = splits[0].iter().map(|l| l.landblock_x()).collect();
    assert_eq!(
        (kept, split),
        (vec![0x10], vec![0x16]),
        "the helper starts from the last landblock"
    );
    assert_eq!(
        table.expect(lbid(0x16, 0x10)).current_landblock_group,
        Some(splits[0].id)
    );
    assert_eq!(
        g.next_try_split_time(),
        h.w.now.utc + TimeSpan::from_minutes(5.0)
    );
}

// ---------------------------------------------------------------------------------------------
// Dormancy and unloading

#[test]
fn an_idle_landblock_goes_dormant_after_a_minute_and_unloads_after_five() {
    let mut h = H::plain();
    let id = lbid(0x30, 0x30);
    let perm = lbid(0x50, 0x50);
    lm::get_landblock(&mut h.w, id, false, false);
    lm::get_landblock(&mut h.w, perm, false, true);
    h.tick();
    assert!(!h.lb(id).is_dormant);

    // Heartbeats every 5 s; dormant once lastActiveTime + 60 s < heartbeat.
    h.advance(60.0);
    h.tick();
    assert!(
        !h.lb(id).is_dormant,
        "exactly 60 s is not past the interval"
    );
    h.advance(5.0);
    h.tick();
    assert!(h.lb(id).is_dormant);
    assert!(!h.lb(perm).is_dormant, "permaloaded");

    // SetActive wakes it and restarts the clock.
    landblock::set_active(&mut h.w, id, false);
    assert!(!h.lb(id).is_dormant);
    h.advance(65.0);
    h.tick();
    assert!(h.lb(id).is_dormant);

    // lastActiveTime + 5 min < heartbeat: queued, and unloaded at the end of that tick.
    not_ported::take_local();
    h.advance(240.0);
    h.tick();
    assert!(!lm::is_loaded(&h.w, id));
    assert_eq!(lm::get_loaded_landblocks(&h.w), [perm]);
    assert_eq!(h.w.landblock_manager.landblock_groups().len(), 1);
    assert!(h.w.landblock_manager.destruction_queue().is_empty());
    assert_eq!(
        not_ported::take_local().get("ACE: LScape.unload_landblock"),
        None
    );
}

#[test]
fn a_keep_alive_object_holds_a_landblock_awake() {
    let mut h = H::plain();
    let id = LandblockId::new(0x3030_0000);
    lm::get_landblock(&mut h.w, id, false, false);
    let g = h.spawn(plain_object(0x8000_0001, 80007), 0x3030_0001);
    h.tick();
    assert!(!h.lb(id).has_no_keep_alive_objects);
    h.advance(400.0);
    h.tick();
    assert!(!h.lb(id).is_dormant);
    assert!(lm::is_loaded(&h.w, id));

    landblock::remove_world_object(&mut h.w, id, g, false, false, true);
    h.tick();
    assert!(h.lb(id).has_no_keep_alive_objects);
    h.advance(5.0);
    h.tick();
    assert!(!lm::is_loaded(&h.w, id), "idle for over five minutes");
}

// ---------------------------------------------------------------------------------------------
// Tick lists

#[test]
fn sorted_lists_insert_in_time_order_with_ace_tie_rules() {
    let times: HashMap<u32, f64> = [
        (1, 10.0),
        (2, 5.0),
        (3, 10.0),
        (4, 7.0),
        (5, f64::MAX),
        (6, 7.0),
        (7, 1.0),
    ]
    .into();
    let key = |g: ObjectGuid| times[&g.full()];
    let mut l = SortedByTime::new();
    for g in [1, 2, 3, 4, 5, 6, 7] {
        l.insert_sorted(ObjectGuid::new(g), key);
    }
    let order: Vec<u32> = l.iter().map(|g| g.full()).collect();
    // A tie with the last node goes after it (3 after 1); a tie inside goes before (6 before 4).
    // double.MaxValue never heartbeats (5).
    assert_eq!(order, [7, 2, 6, 4, 1, 3]);
    assert!(l.remove(ObjectGuid::new(4)));
    assert!(!l.contains(ObjectGuid::new(4)));
    assert_eq!(l.first(), Some(ObjectGuid::new(7)));
}

#[test]
fn pending_additions_sort_players_and_creatures_into_their_tick_lists() {
    let mut h = H::plain();
    let id = LandblockId::new(0x3131_0000);
    lm::get_landblock(&mut h.w, id, false, false);
    h.w.sessions.insert(
        empyrean_net::SessionId {
            client_id: 1,
            generation: 1,
        },
        empyrean_world::sessions::SessionData {
            player: Some(ObjectGuid::new(0x5000_0001)),
            ..Default::default()
        },
    );
    let p = h.spawn(player(0x5000_0001), 0x3131_0001);
    let c = h.spawn(creature(0x8000_0002), 0x3131_0002);
    let o = h.spawn(plain_object(0x8000_0003, 5), 0x3131_0003);
    assert_eq!(h.lb(id).pending_addition_guids().count(), 3);
    assert!(h.lb(id).players().is_empty());

    h.tick();
    assert_eq!(h.lb(id).players(), [p]);
    assert_eq!(
        h.lb(id)
            .sorted_creatures_by_next_tick()
            .iter()
            .copied()
            .collect::<Vec<_>>(),
        [c]
    );
    assert_eq!(h.objects_of(id), [p.full(), c.full(), o.full()]);
    assert_eq!(landblock::get_object(&h.w, id, o, true), Some(o));

    landblock::remove_world_object(&mut h.w, id, c, false, false, true);
    assert_eq!(
        landblock::get_object(&h.w, id, c, true),
        None,
        "a pending removal is not found"
    );
    h.tick();
    assert!(h.lb(id).sorted_creatures_by_next_tick().is_empty());
    assert_eq!(h.objects_of(id), [p.full(), o.full()]);
    assert_eq!(h.w.objects.get(c).unwrap().current_landblock, None);

    // Re-adding before the removal is processed cancels the removal.
    landblock::remove_world_object(&mut h.w, id, o, false, false, true);
    assert!(landblock::add_world_object(&mut h.w, id, o));
    h.tick();
    assert_eq!(h.objects_of(id), [p.full(), o.full()]);
}

// ---------------------------------------------------------------------------------------------
// Actions

#[test]
fn object_actions_follow_world_object_enqueue_action() {
    let mut h = H::plain();
    let id = LandblockId::new(0x3232_0000);
    lm::get_landblock(&mut h.w, id, false, false);
    h.tick();
    let l: Log = Arc::default();

    // On a landblock: the landblock's queue, run by its next tick.
    let item = h.spawn(plain_object(0x8000_0001, 5), 0x3232_0001);
    enqueue(&mut h.w, Actor::Object(item), note(&l, "on landblock"));
    assert_eq!(h.lb(id).action_queue().len(), 1);
    h.tick();
    assert_eq!(entries(&l), ["on landblock"]);

    // Detached: creatures and spell projectiles drop the work; other items go to the world queue.
    h.w.objects.insert(creature(0x8000_0002)).unwrap();
    enqueue(
        &mut h.w,
        Actor::Object(ObjectGuid::new(0x8000_0002)),
        note(&l, "creature"),
    );
    h.w.objects
        .insert(WorldObject {
            kind: KindData::SpellProjectile(Box::default()),
            ..plain_object(0x8000_0003, 5)
        })
        .unwrap();
    enqueue(
        &mut h.w,
        Actor::Object(ObjectGuid::new(0x8000_0003)),
        note(&l, "projectile"),
    );
    h.w.objects.insert(plain_object(0x8000_0004, 5)).unwrap();
    enqueue(
        &mut h.w,
        Actor::Object(ObjectGuid::new(0x8000_0004)),
        note(&l, "detached item"),
    );
    enqueue(
        &mut h.w,
        Actor::Object(ObjectGuid::new(0x8000_0099)),
        note(&l, "destroyed"),
    );
    assert_eq!(h.w.world_manager.action_queue.len(), 1);
    run_actions(&mut h.w, Actor::World);
    assert_eq!(entries(&l), ["on landblock", "detached item"]);

    h.w.sessions.insert(
        empyrean_net::SessionId {
            client_id: 1,
            generation: 1,
        },
        empyrean_world::sessions::SessionData {
            player: Some(ObjectGuid::new(0x5000_0001)),
            ..Default::default()
        },
    );
    let p = h.spawn(player(0x5000_0001), 0x3232_0002);
    enqueue(&mut h.w, Actor::Object(p), note(&l, "player"));
    assert_eq!(
        action_queue_mut(&mut h.w, Actor::Object(p)).map(|q| q.len()),
        Some(1)
    );

    // A landblock actor: its queue while loaded, nothing once unloaded. The landblock's queue runs
    // in TickMultiThreadedWork, before the players' Player_Tick (TickSingleThreadedWork), so the
    // player's action, enqueued first, runs second.
    enqueue(
        &mut h.w,
        Actor::Landblock(LandblockId::new(0x3232_0123)),
        note(&l, "landblock"),
    );
    assert_eq!(
        action_queue_mut(&mut h.w, Actor::Landblock(id)).map(|q| q.len()),
        Some(1)
    );
    h.tick();
    assert_eq!(entries(&l)[2..], ["landblock", "player"]);
    assert!(action_queue_mut(&mut h.w, Actor::Landblock(lbid(0x33, 0x33))).is_none());
    enqueue(
        &mut h.w,
        Actor::Landblock(lbid(0x33, 0x33)),
        note(&l, "unloaded"),
    );
    assert!(!entries(&l).contains(&"unloaded".to_owned()));
}

// ---------------------------------------------------------------------------------------------
// Relocation

#[test]
fn an_object_crossing_a_landblock_boundary_changes_membership() {
    let mut h = H::plain();
    let a = LandblockId::new(0x3434_0000);
    lm::get_landblock(&mut h.w, a, false, false);
    let g = h.spawn(plain_object(0x8000_0001, 5), 0x3434_0001);
    h.tick();
    assert_eq!(h.objects_of(a), [g.full()]);

    // The object's position moves east into 0x3534; physics reports it and the manager relocates it.
    h.w.objects
        .get_mut(g)
        .unwrap()
        .set_location(Some(Position::from_components(
            0x3534_0001,
            1.0,
            10.0,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
            false,
        )));
    not_ported::take_local();
    lm::relocate_object_for_physics(&mut h.w, g, true);
    let b = LandblockId::new(0x3534_0000);
    assert!(
        lm::is_loaded(&h.w, b),
        "loaded on demand, with its adjacents"
    );
    assert_eq!(lm::get_loaded_landblocks(&h.w).len(), 9);
    assert_eq!(h.w.objects.get(g).unwrap().current_landblock, Some(b));
    assert!(
        !not_ported::take_local().contains_key("ACE: WorldObject.EnqueueActionBroadcast"),
        "an adjacency move sends no removal"
    );
    assert_eq!(
        landblock::get_object(&h.w, a, g, true),
        None,
        "a pending removal hides it, adjacents unsearched"
    );
    assert_eq!(landblock::get_object(&h.w, b, g, false), Some(g));

    h.tick();
    assert!(h.objects_of(a).is_empty());
    assert_eq!(h.objects_of(b), [g.full()]);
    assert_eq!(landblock::get_object(&h.w, a, g, false), None);
    assert_eq!(
        landblock::get_object(&h.w, a, g, true),
        Some(g),
        "found through the adjacent landblock"
    );
}

// ---------------------------------------------------------------------------------------------
// Preloading, fog, mesh

#[test]
fn preload_config_loads_enabled_entries_with_their_flags() {
    let mut h = H::plain();
    let mut server = GameConfiguration {
        preloaded_landblocks: vec![
            PreloadedLandblocks {
                id: Some("E74EFFFF".into()),
                permaload: true,
                include_adjacents: true,
                enabled: true,
                ..Default::default()
            },
            PreloadedLandblocks {
                id: Some(" 0102FFFF\t".into()),
                permaload: false,
                include_adjacents: false,
                enabled: true,
                ..Default::default()
            },
            PreloadedLandblocks {
                id: Some("0x0303FFFF".into()),
                enabled: true,
                ..Default::default()
            },
            PreloadedLandblocks {
                id: Some("+0404FFFF".into()),
                enabled: true,
                ..Default::default()
            },
            PreloadedLandblocks {
                id: Some("0505FFFF".into()),
                enabled: false,
                ..Default::default()
            },
            PreloadedLandblocks {
                id: None,
                enabled: true,
                ..Default::default()
            },
        ],
        ..GameConfiguration::default()
    };
    lm::preload_config_landblocks(&mut h.w, &server);
    assert_eq!(
        lm::get_loaded_landblocks(&h.w).len(),
        10,
        "Hebian-To with its 8 adjacents, and 0x0102"
    );
    assert!(
        h.lb(lbid(0xE7, 0x4E)).permaload && h.lb(lbid(0xE8, 0x4F)).permaload,
        "permaload reaches the adjacents"
    );
    assert!(!h.lb(lbid(0x01, 0x02)).permaload);

    let mut h = H::plain();
    server.preloaded_landblocks = vec![PreloadedLandblocks {
        id: Some("0".into()),
        description: Some("Apartment Landblocks".into()),
        enabled: true,
        ..Default::default()
    }];
    lm::preload_config_landblocks(&mut h.w, &server);
    assert_eq!(lm::get_loaded_landblocks(&h.w).len(), 50);

    let mut h = H::plain();
    server.landblock_preloading = false;
    lm::preload_config_landblocks(&mut h.w, &server);
    assert!(lm::get_loaded_landblocks(&h.w).is_empty());
}

#[test]
fn global_fog_overrides_each_landblocks_own() {
    let mut h = H::plain();
    let id = lbid(0x60, 0x60);
    lm::get_landblock(&mut h.w, id, false, false);
    landblock::set_fog_color(
        &mut h.w,
        id,
        empyrean_entity::enums::EnvironChangeType::RedFog,
    );
    assert_eq!(
        landblock::fog_color(&h.w, id),
        empyrean_entity::enums::EnvironChangeType::RedFog
    );
    lm::do_environ_change(&mut h.w, empyrean_entity::enums::EnvironChangeType::BlueFog);
    assert_eq!(
        landblock::fog_color(&h.w, id),
        empyrean_entity::enums::EnvironChangeType::BlueFog
    );
    lm::do_environ_change(&mut h.w, empyrean_entity::enums::EnvironChangeType::Clear);
    assert_eq!(h.w.landblock_manager.global_fog_color, None);
    assert_eq!(
        landblock::fog_color(&h.w, id),
        empyrean_entity::enums::EnvironChangeType::RedFog
    );
}

#[test]
fn landblock_mesh_split_direction_and_cell_lookup() {
    // x = 0xA9*8+cx, y = 0xB4*8+cy; dw = x*y*0x0CCAC033 - x*0x421BE3BD + y*0x6C1AC587 - 0x519B8F25 (int).
    fn split(lx: i64, ly: i64, cx: i64, cy: i64) -> bool {
        let (x, y) = (lx * 8 + cx, ly * 8 + cy);
        // Keep the low 32 bits (C#'s unchecked int), then read bit 31.
        let dw =
            (x * y * 0x0CCA_C033 - x * 0x421B_E3BD + y * 0x6C1A_C587 - 0x519B_8F25) & 0xFFFF_FFFF;
        dw & 0x8000_0000 == 0
    }
    let mesh = LandblockMesh {
        landblock_id: lbid(0xA9, 0xB4),
        vertex_heights: vec![],
        vertices: vec![],
        triangles: vec![],
    };
    let mut seen = [0, 0];
    for cx in 0..8 {
        for cy in 0..8 {
            let got = mesh.get_split_dir(lbid(0xA9, 0xB4), cx, cy);
            assert_eq!(
                got,
                split(0xA9, 0xB4, i64::from(cx), i64::from(cy)),
                "cell ({cx}, {cy})"
            );
            seen[usize::from(got)] += 1;
        }
    }
    assert!(
        seen[0] > 0 && seen[1] > 0,
        "both split directions occur: {seen:?}"
    );

    assert_eq!(
        LandblockMesh::get_cell(Vector2::new(47.9, 48.0)),
        Vector2::new(1.0, 2.0)
    );
    assert_eq!(
        LandblockMesh::get_cell(Vector2::new(-5.0, 500.0)),
        Vector2::new(0.0, 7.0)
    );

    let mut mesh = mesh;
    mesh.build_triangles();
    assert_eq!(mesh.triangles.len(), 128);
    // ACE-BUG: cells are stored x-major ((x * 8 + y) * 2) but read y-major: cell (1, 0) reads the
    // triangles stored for cell (0, 1).
    assert_eq!(
        mesh.get_cell_triangles(Vector2::new(1.0, 0.0)),
        [mesh.triangles[2], mesh.triangles[3]]
    );
    assert_ne!(mesh.triangles[2], mesh.triangles[16]);
}

/// `Triangle` (Triangle.cs): the 2D area, containment, the plane's Z at a point; and
/// `LandblockMesh.GetZ` / `GetSplitter` over them.
#[test]
fn triangles_contain_points_and_give_the_planes_height() {
    use empyrean_entity::numerics::Vector3;
    use empyrean_world::entity::triangle::Triangle;
    let v = vec![
        Vector3::new(0.0, 0.0, 0.0),
        Vector3::new(24.0, 0.0, 24.0),
        Vector3::new(0.0, 24.0, 0.0),
        Vector3::new(24.0, 24.0, 24.0),
    ];
    let t = Triangle::new(0, 1, 2);
    assert_eq!(Triangle::area(v[0], v[1], v[2]), 288.0);
    assert!(t.contains(Vector2::new(4.0, 4.0), &v));
    assert!(!t.contains(Vector2::new(20.0, 20.0), &v));
    assert_eq!(
        t.get_z(&v, Vector2::new(6.0, 3.0)),
        6.0,
        "z = x on this plane"
    );
    assert_eq!(t.get_vertices(&v), [v[0], v[1], v[2]]);

    // a one-cell mesh: the first triangle holds (4, 4), the second (20, 20)
    let mesh = LandblockMesh {
        landblock_id: lbid(0xA9, 0xB4),
        vertex_heights: vec![],
        vertices: v.clone(),
        triangles: vec![[0, 1, 2], [1, 3, 2]],
    };
    assert_eq!(mesh.get_triangle(Vector2::new(4.0, 4.0)), [0, 1, 2]);
    assert_eq!(mesh.get_triangle(Vector2::new(20.0, 20.0)), [1, 3, 2]);
    assert_eq!(mesh.get_z(Vector2::new(20.0, 10.0)), 20.0);
    let line = mesh.get_splitter(&[[0, 1, 2], [3, 2, 1]]);
    assert_eq!(
        (line.start, line.end),
        (Vector2::new(0.0, 0.0), Vector2::new(24.0, 0.0)),
        "shared 1: 0 -> 1"
    );
}

/// A portal with a relative destination and no destination arrives at its own spot plus the offset,
/// facing the relative destination's whole rotation, even when the portal is turned (V298; ACE
/// mixed the portal's Y and Z into it).
#[test]
fn a_turned_portal_takes_its_relative_destinations_whole_rotation() {
    use empyrean_world::dispatch::Class;
    let mut h = super::emotes::H::new();
    let guid = ObjectGuid::new(0x7000_2000);
    let mut o = WorldObject::allocate(Class::Portal);
    o.guid = guid;
    o.biota.id = guid.full();
    o.set_property(
        empyrean_entity::enums::PropertyString::Name,
        "Arena 2".to_owned(),
    );
    o.set_property(PropertyDataId::Setup, land::TEST_SETUP);
    let s = std::f32::consts::FRAC_1_SQRT_2;
    // turned 90 degrees
    o.set_location(Some(Position::from_components(
        0xA9B4_0001,
        20.0,
        20.0,
        0.0,
        0.0,
        0.0,
        s,
        s,
        false,
    )));
    o.set_relative_destination(Some(Position::from_components(
        0, 0.0, 0.0, 63.0, 0.0, 0.0, 0.0, 1.0, false,
    )));
    h.w.objects.insert(o).expect("fresh guid");
    assert!(
        empyrean_world::dispatch::enter_world::enter_world(&mut h.w, guid),
        "the portal enters the world"
    );

    let at =
        h.w.objects
            .get(guid)
            .unwrap()
            .location()
            .expect("placed")
            .pos();
    let d =
        h.w.objects
            .get(guid)
            .unwrap()
            .destination()
            .expect("a destination");
    assert_eq!((d.pos().x, d.pos().y, d.pos().z), (at.x, at.y, at.z + 63.0));
    assert_eq!(
        (d.rotation_x, d.rotation_y, d.rotation_z, d.rotation_w),
        (0.0, 0.0, 0.0, 1.0)
    );
}
