//! ACE: Source/ACE.Server/Entity/Landblock.cs::AddWorldObjectInternal
//! Landblock loads instances/encounters via the real factory; shard biota restore; dormancy
//! unload destroys new objects; Destroy leaves landblock before store; FadeOutAndDestroy after
//! 1s; shutdown unloads all; undef instance aborts load.
//! Fixture: synthetic dats, isolated world state.

use std::sync::Arc;
use std::time::Duration;

use empyrean_common::clock::{ClockSnapshot, VirtualClock};
use empyrean_common::dotnet::datetime::TimeSpan;
use empyrean_common::not_ported;
use empyrean_content::models::world::{
    Encounter, LandblockInstance, LandblockInstanceLink, Weenie,
};
use empyrean_content::MemContent;
use empyrean_dat::FakeDats;
use empyrean_entity::enums::{PropertyDataId, WeenieType};
use empyrean_entity::{Biota, LandblockId, ObjectGuid, Position};
use empyrean_testkit::land;
use empyrean_world::entity::timers::TimersState;
use empyrean_world::factories::world_object_factory as factory;
use empyrean_world::managers::guid_manager::{self as gm, ShardGuidQueries};
use empyrean_world::managers::landblock_manager as lm;
use empyrean_world::managers::server_manager::{self as sm, ShutdownStage};
use empyrean_world::managers::world_manager::NoWire;
use empyrean_world::world_objects::kinds::KindData;
use empyrean_world::world_objects::world_object::{self as wo, CtorEnv, WorldObject};
use empyrean_world::World;

/// `GuidManager`'s start-up reads: an empty dynamic range with one sequence gap.
struct Shard {
    gaps: Vec<(u32, u32)>,
}

impl ShardGuidQueries for Shard {
    fn get_max_guid_found_in_range(&mut self, _min: u32, _max: u32) -> u32 {
        u32::MAX
    }

    fn get_sequence_gaps(&mut self, _min: u32, _limit: u32) -> Vec<(u32, u32)> {
        self.gaps.clone()
    }
}

const LB: u32 = 0xA9B4_0000;

fn id() -> LandblockId {
    LandblockId::new(LB | 0xFFFF)
}

struct H {
    w: World,
    clock: VirtualClock,
}

impl H {
    fn new(content: MemContent) -> Self {
        let clock = VirtualClock::default();
        let timers = TimersState::new(&clock);
        let now = ClockSnapshot::take(&clock, timers.portal_year_ticks);
        let mut w = World::new(
            now,
            empyrean_testkit::dats::with_stat_tables(FakeDats::new())
                .build()
                .unwrap(),
        );
        w.timers = timers;
        w.content = Arc::new(content);
        gm::initialize(
            &mut w,
            &mut Shard {
                gaps: vec![(0x8000_0010, 0x8000_0010)],
            },
        );
        land::use_flat_land_with_test_setup(&mut w, &[0xA9B4, 0x3030], 0);
        H { w, clock }
    }

    /// One `LandblockManager.Tick` at the clock's time.
    fn lm_tick(&mut self) {
        self.w.now = ClockSnapshot::take(&self.clock, self.w.timers.portal_year_ticks);
        let pyt = self.w.timers.portal_year_ticks;
        lm::tick(&mut self.w, pyt);
    }

    /// One `WorldManager.UpdateWorld`.
    fn world_tick(&mut self) {
        let now = ClockSnapshot::take(&self.clock, self.w.timers.portal_year_ticks);
        self.w.tick(now, &mut NoWire);
    }

    fn advance(&mut self, secs: f64) {
        let d = Duration::from_secs_f64(secs);
        self.clock.advance(d);
        empyrean_world::entity::timers::advance_portal_year_ticks(
            &mut self.w,
            TimeSpan::from_ticks(i64::try_from(d.as_nanos() / 100).unwrap()),
        );
    }

    fn objects_of(&self) -> Vec<u32> {
        self.w
            .landblock_manager
            .landblocks
            .expect(id())
            .world_object_guids()
            .map(|g| g.full())
            .collect()
    }

    fn get(&self, guid: u32) -> Option<&WorldObject> {
        self.w.objects.get(ObjectGuid::new(guid))
    }
}

const SETUP: u32 = land::TEST_SETUP;

fn weenie(wcid: u32, name: &str, weenie_type: WeenieType) -> Weenie {
    Weenie::new(wcid, name, weenie_type).with_did(PropertyDataId::Setup, SETUP)
}

fn content() -> MemContent {
    let mut linked = LandblockInstance::new(0x7A9B_4001, 20, LB | 0x0001, [1.0, 2.0, 3.0]);
    linked.landblock_instance_link.push(LandblockInstanceLink {
        id: 1,
        parent_guid: 0x7A9B_4001,
        child_guid: 0x7A9B_4009,
        ..Default::default()
    });
    let child = LandblockInstance {
        is_link_child: true,
        ..LandblockInstance::new(0x7A9B_4009, 30, LB | 0x0001, [5.0, 5.0, 0.0])
    };
    MemContent::new()
        .weenie(weenie(20, "door", WeenieType::Door))
        .weenie(weenie(21, "chest", WeenieType::Chest))
        .weenie(weenie(22, "thing", WeenieType::Generic))
        .weenie(weenie(30, "drudge", WeenieType::Creature))
        .weenie(weenie(40, "undef", WeenieType::Undef))
        .landblock_instance(LandblockInstance::new(
            0x7A9B_4005,
            21,
            LB | 0x0002,
            [20.0, 30.0, 5.0],
        ))
        .landblock_instance(linked)
        .landblock_instance(child)
        .landblock_instance(LandblockInstance::new(
            0x7A9B_4003,
            22,
            LB | 0x0003,
            [7.0, 8.0, 9.0],
        ))
        .encounter(Encounter {
            id: 1,
            landblock: 0xA9B4,
            weenie_class_id: 30,
            cell_x: 2,
            cell_y: 3,
            ..Encounter::default()
        })
        .encounter(Encounter {
            id: 2,
            landblock: 0xA9B4,
            weenie_class_id: 30,
            cell_x: 4,
            cell_y: 1,
            ..Encounter::default()
        })
        .encounter(Encounter {
            id: 3,
            landblock: 0xA9B4,
            weenie_class_id: 40,
            cell_x: 7,
            cell_y: 7,
            ..Encounter::default()
        })
}

/// [`content`] plus an instance of the Undef weenie after the others.
fn content_with_undef_instance() -> MemContent {
    content().landblock_instance(LandblockInstance::new(
        0x7A9B_4007,
        40,
        LB | 0x0003,
        [7.0, 8.0, 9.0],
    ))
}

#[test]
fn a_landblock_loads_its_instances_and_encounters_through_the_real_factory() {
    let mut h = H::new(content());
    not_ported::take_local();
    lm::get_landblock(&mut h.w, id(), false, false);
    h.lm_tick();

    assert_eq!(
        h.objects_of(),
        [
            0x7A9B_4005,
            0x7A9B_4001,
            0x7A9B_4009,
            0x7A9B_4003,
            0x8000_0010,
            0x8000_0000
        ]
    );

    let chest = h.get(0x7A9B_4005).unwrap();
    assert!(
        matches!(chest.kind, KindData::Chest(_))
            && chest.container.is_some()
            && chest.creature.is_none()
    );
    let loc = chest.location().unwrap();
    assert_eq!(
        (
            loc.landblock_id().raw(),
            loc.pos().x,
            loc.pos().y,
            loc.pos().z
        ),
        (LB | 0x0002, 20.0, 30.0, 5.0)
    );
    let door = h.get(0x7A9B_4001).unwrap();
    assert!(matches!(door.kind, KindData::Door(_)) && door.container.is_none());
    assert!(matches!(
        h.get(0x7A9B_4003).unwrap().kind,
        KindData::GenericObject(_)
    ));
    for g in [0x8000_0010, 0x8000_0000] {
        let enc = h.get(g).unwrap();
        assert!(
            matches!(enc.kind, KindData::Creature)
                && enc.creature.is_some()
                && enc.container.is_some()
        );
        assert_eq!(enc.current_landblock, Some(id()));
        assert!(
            !enc.wo.world_object_database.biota_originated_from_database,
            "built from a weenie"
        );
        assert!(
            enc.biota_originated_from_or_has_been_saved_to_database(),
            "saved by the first SaveDB"
        );
    }
    // The Undef encounter drew 0x80000001, built nothing, and recycled it.
    assert_eq!(gm::dynamic_current(&h.w), 0x8000_0002);
    assert!(gm::get_dynamic_guid_debug_info(&mut h.w).ends_with("recycled GUIDs available: 1"));

    let hits = not_ported::take_local();
    assert_eq!(
        hits.get("ACE: WorldObjectFactory.CreateNewWorldObjects"),
        None,
        "{hits:?}"
    );
    assert_eq!(
        hits.get("ACE: WorldObjectFactory.CreateNewWorldObject"),
        None
    );
    assert_eq!(hits.get("ACE: WorldObject.LinkedInstances"), None);
    let child = h.get(0x7A9B_4009).unwrap();
    assert_eq!(
        child.activation_target(),
        0x7A9B_4001,
        "the door's one link child"
    );
    assert_eq!(
        child.wo.world_object_links.parent_link,
        Some(ObjectGuid::new(0x7A9B_4001))
    );
    assert_eq!(
        h.get(0x7A9B_4001)
            .unwrap()
            .wo
            .world_object_links
            .child_links,
        [ObjectGuid::new(0x7A9B_4009)]
    );
}

#[test]
fn create_new_world_objects_restores_a_shard_biota_and_its_missing_location() {
    let h = H::new(content());
    let instances = h.w.content.get_cached_instances_by_landblock(0xA9B4);
    let biota = {
        let env = CtorEnv::without_content(&h.w);
        let weenie = Arc::new(empyrean_entity::Weenie {
            weenie_class_id: 22,
            weenie_type: WeenieType::Generic,
            ..Default::default()
        });
        let mut o =
            factory::create_world_object(&env, Some(weenie), ObjectGuid::new(0x7A9B_4003)).unwrap();
        o.set_location(None);
        o.biota.clone()
    };
    let w = h.w;
    let built = CtorEnv::with_world(&w, |env| {
        factory::create_new_world_objects(env, &instances, &[biota], None)
    })
    .expect("no Undef instance");
    let guids: Vec<u32> = built.iter().map(|o| o.guid.full()).collect();
    assert_eq!(guids, [0x7A9B_4005, 0x7A9B_4001, 0x7A9B_4003]);
    let restored = &built[2];
    assert!(
        restored.biota_originated_from_or_has_been_saved_to_database(),
        "the (Biota) constructor"
    );
    assert!(!built[0].biota_originated_from_or_has_been_saved_to_database());
    let loc = restored.location().unwrap();
    assert_eq!(
        (loc.pos().x, loc.pos().y, loc.pos().z),
        (7.0, 8.0, 9.0),
        "restored from the instance"
    );
}

#[test]
fn unload_after_dormancy_destroys_new_objects_and_detaches_shard_objects() {
    let mut h = H::new(content());
    lm::get_landblock(&mut h.w, id(), false, false);
    h.lm_tick();

    // An object restored from the shard joins the landblock.
    let from_db = {
        let mut biota = Biota {
            id: 0x7A9B_4100,
            weenie_class_id: 22,
            weenie_type: WeenieType::Generic,
            ..Default::default()
        };
        assert!(h.w.shard.base_database().save_biota(&mut biota, false));
        let env = CtorEnv::without_content(&h.w);
        let mut o = factory::create_world_object_from_biota(&env, biota).unwrap();
        o.set_property(PropertyDataId::Setup, SETUP);
        o.set_location(Some(Position::from_components(
            LB | 0x0004,
            10.0,
            10.0,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
            false,
        )));
        o
    };
    h.w.objects.insert(from_db).unwrap();
    assert!(lm::add_object(
        &mut h.w,
        ObjectGuid::new(0x7A9B_4100),
        false
    ));
    h.lm_tick();
    assert_eq!(h.objects_of().len(), 7);

    // Dormant after 60 s, unloaded once lastActiveTime + 5 min has passed.
    h.advance(65.0);
    h.lm_tick();
    h.advance(240.0);
    h.lm_tick();
    assert!(!lm::is_loaded(&h.w, id()));

    for g in [
        0x7A9B_4005,
        0x7A9B_4001,
        0x7A9B_4009,
        0x7A9B_4003,
        0x8000_0010,
        0x8000_0000,
    ] {
        assert!(h.get(g).is_none(), "{g:08X} left the store");
    }
    for g in [0x8000_0010, 0x8000_0000] {
        assert!(
            h.w.shard.base_database().get_biota(g, false).is_some(),
            "{g:08X} saved"
        );
    }
    assert!(
        gm::get_dynamic_guid_debug_info(&mut h.w).ends_with("recycled GUIDs available: 1"),
        "only the Undef encounter's"
    );

    assert!(
        h.w.shard
            .base_database()
            .get_biota(0x7A9B_4100, false)
            .is_some(),
        "not destroyed"
    );
    assert!(
        h.get(0x7A9B_4100).is_none(),
        "Saved and detached, it leaves World.objects"
    );
    assert_eq!(h.w.objects.len(), 0, "nothing of the landblock is left");
}

#[test]
fn destroy_leaves_the_landblock_before_the_store_and_removes_a_shard_biota() {
    let mut h = H::new(content());
    lm::get_landblock(&mut h.w, id(), false, false);
    h.lm_tick();

    // In the shard and restored from it: RemoveBiotaFromDatabase queues the removal.
    let mut biota = Biota {
        id: 0x7A9B_4100,
        weenie_class_id: 22,
        weenie_type: WeenieType::Generic,
        ..Default::default()
    };
    assert!(h.w.shard.base_database().save_biota(&mut biota, false));
    let mut o =
        factory::create_world_object_from_biota(&CtorEnv::without_content(&h.w), biota).unwrap();
    o.set_property(PropertyDataId::Setup, SETUP);
    h.w.objects.insert(o).unwrap();
    let from_db = ObjectGuid::new(0x7A9B_4100);
    h.w.objects
        .get_mut(from_db)
        .unwrap()
        .set_location(Some(Position::from_components(
            LB | 0x0004,
            1.0,
            1.0,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
            false,
        )));
    assert!(lm::add_object(&mut h.w, from_db, false));

    // Never saved, though a biota with its id sits in the shard: nothing is removed there.
    let mut stray = Biota {
        id: 0x7A9B_4003,
        weenie_class_id: 22,
        ..Default::default()
    };
    assert!(h.w.shard.base_database().save_biota(&mut stray, false));

    h.lm_tick();
    wo::destroy(&mut h.w, from_db, true, false);
    wo::destroy(&mut h.w, ObjectGuid::new(0x7A9B_4003), true, false);
    assert!(h.get(0x7A9B_4100).is_none() && h.get(0x7A9B_4003).is_none());
    // Removed from the landblock (pending until its next pass), not only from the store.
    h.lm_tick();
    assert_eq!(
        h.objects_of(),
        [
            0x7A9B_4005,
            0x7A9B_4001,
            0x7A9B_4009,
            0x8000_0010,
            0x8000_0000
        ]
    );
    let mut shard = h.w.shard.base_database();
    assert!(
        shard.get_biota(0x7A9B_4100, false).is_none(),
        "RemoveBiota for a shard object"
    );
    assert!(
        shard.get_biota(0x7A9B_4003, false).is_some(),
        "not originated, not saved: no RemoveBiota"
    );
}

#[test]
fn fade_out_and_destroy_removes_the_object_a_second_later() {
    let mut h = H::new(content());
    lm::get_landblock(&mut h.w, id(), false, false);
    h.world_tick();
    assert!(h.get(0x7A9B_4005).is_some());

    wo::fade_out_and_destroy(&mut h.w, ObjectGuid::new(0x7A9B_4005), true);
    h.advance(0.5);
    h.world_tick();
    assert!(h.get(0x7A9B_4005).is_some(), "the 1 s delay has not passed");
    h.advance(0.6);
    h.world_tick();
    h.advance(0.1);
    h.world_tick();
    assert!(h.get(0x7A9B_4005).is_none());
    assert!(!h.objects_of().contains(&0x7A9B_4005));
}

#[test]
fn shutdown_queues_every_loaded_landblock_and_waits_for_them_to_unload() {
    let mut h = H::new(content());
    let other = LandblockId::from_xy(0x30, 0x30);
    lm::get_landblock(&mut h.w, id(), false, false);
    lm::get_landblock(&mut h.w, other, false, true);
    h.w.world_manager.world_active = true;
    h.world_tick();

    sm::set_shutdown_interval(&mut h.w, 0);
    sm::do_shutdown_now(&mut h.w);
    h.advance(0.001);
    h.world_tick();
    not_ported::take_local();
    assert!(!sm::shutdown_server(&mut h.w));
    assert_eq!(
        h.w.server_manager.shutdown_stage(),
        Some(ShutdownStage::UnloadingLandblocks)
    );
    let mut queued: Vec<LandblockId> = h.w.landblock_manager.destruction_queue().to_vec();
    queued.sort_by_key(|l| l.raw());
    let mut loaded = lm::get_loaded_landblocks(&h.w);
    loaded.sort_by_key(|l| l.raw());
    assert_eq!(
        queued, loaded,
        "every loaded landblock, permaloaded ones too"
    );
    assert_eq!(
        not_ported::take_local()
            .get("ACE: LandblockManager.AddAllActiveLandblocksToDestructionQueue"),
        None
    );

    // The next UpdateGameWorld unloads them; the shutdown then goes on to stop the world.
    h.advance(1.0);
    h.world_tick();
    assert!(lm::get_loaded_landblocks(&h.w).is_empty());
    assert!(!sm::shutdown_server(&mut h.w));
    assert_eq!(
        h.w.server_manager.shutdown_stage(),
        Some(ShutdownStage::StoppingWorld)
    );
}

#[test]
fn an_undef_instance_aborts_the_landblock_load() {
    let mut h = H::new(content_with_undef_instance());
    lm::get_landblock(&mut h.w, id(), false, false);
    assert!(
        h.w.landblock_manager
            .landblocks
            .expect(id())
            .action_queue()
            .is_empty(),
        "nothing was enqueued"
    );
    assert_eq!(
        gm::dynamic_current(&h.w),
        0x8000_0000,
        "the encounters drew no guid"
    );
    h.lm_tick();
    assert!(h.objects_of().is_empty());
    assert!(!h
        .w
        .landblock_manager
        .landblocks
        .expect(id())
        .create_world_objects_completed());
    assert_eq!(h.w.objects.len(), 0);

    // The factory reports the instance that threw.
    let instances = h.w.content.get_cached_instances_by_landblock(0xA9B4);
    let err = CtorEnv::with_world(&h.w, |env| {
        factory::create_new_world_objects(env, &instances, &[], None)
    })
    .unwrap_err();
    assert_eq!((err.instance_guid, err.weenie_class_id), (0x7A9B_4007, 40));
    // restrict_wcid skips it (the linked-house lookup in House.cs).
    let only = CtorEnv::with_world(&h.w, |env| {
        factory::create_new_world_objects(env, &instances, &[], Some(21))
    })
    .unwrap();
    assert_eq!(
        only.iter().map(|o| o.guid.full()).collect::<Vec<_>>(),
        [0x7A9B_4005]
    );
}
