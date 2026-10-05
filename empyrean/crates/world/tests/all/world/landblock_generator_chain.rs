//! ACE: Source/ACE.Server/Entity/Landblock.cs::Init
//! A generator landblock loads, spawns, ticks, respawns and unloads in ACE's order; instances
//! first among server objects; adjacent landblocks share server objects.
//! Fixture: synthetic dats, isolated world state.

use std::sync::Arc;
use std::time::Duration;

use empyrean_common::clock::{ClockSnapshot, VirtualClock};
use empyrean_common::dotnet::datetime::TimeSpan;
use empyrean_common::random::DotNetRandom;
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_content::models::world::{
    Encounter, LandblockInstance, Weenie, WeeniePropertiesGenerator,
};
use empyrean_content::MemContent;
use empyrean_dat::FakeDats;
use empyrean_entity::enums::{
    PropertyDataId, PropertyFloat, PropertyInt, PropertyString, WeenieType,
};
use empyrean_entity::{LandblockId, ObjectGuid};
use empyrean_testkit::land;
use empyrean_world::entity::timers::TimersState;
use empyrean_world::managers::guid_manager as gm;
use empyrean_world::managers::landblock_manager as lm;
use empyrean_world::physics::phys_ext;
use empyrean_world::world_objects::world_object::{self as wo, WorldObject};
use empyrean_world::world_objects::world_object_tick as tick;
use empyrean_world::World;

use empyrean_testkit::EmptyShard;

const LB: u32 = 0xA9B4_0000;
const SETUP: u32 = land::TEST_SETUP;
const SEED: i32 = 11;
/// The generator instance's static guid.
const GEN: u32 = 0x7A9B_4001;
const GEN_WCID: u32 = 1000;
const SPAWN_WCID: u32 = 2001;
const ENCOUNTER_WCID: u32 = 2002;

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
        gm::initialize(&mut w, &mut EmptyShard);
        // Flat land at height 0 (so the encounter's GetZ reads 0) and one setup.
        land::use_flat_land_with_test_setup(&mut w, &[0xA9B4], 0);
        H { w, clock }
    }

    /// One `LandblockManager.Tick` at the clock's time.
    fn tick(&mut self) {
        self.w.now = ClockSnapshot::take(&self.clock, self.w.timers.portal_year_ticks);
        let pyt = self.w.timers.portal_year_ticks;
        lm::tick(&mut self.w, pyt);
    }

    fn advance(&mut self, secs: f64) {
        let d = Duration::from_secs_f64(secs);
        self.clock.advance(d);
        empyrean_world::entity::timers::advance_portal_year_ticks(
            &mut self.w,
            TimeSpan::from_ticks(i64::try_from(d.as_nanos() / 100).unwrap()),
        );
    }

    /// Ticks once a second for `secs` seconds.
    fn run(&mut self, secs: u32) {
        for _ in 0..secs {
            self.advance(1.0);
            self.tick();
        }
    }

    fn obj(&self, g: ObjectGuid) -> &WorldObject {
        self.w.objects.get(g).expect("object in store")
    }

    /// The landblock's objects (after its pending additions and removals were processed).
    fn objects_of(&self) -> Vec<ObjectGuid> {
        self.w
            .landblock_manager
            .landblocks
            .expect(id())
            .world_object_guids()
            .copied()
            .collect()
    }

    fn spawned(&self) -> Vec<ObjectGuid> {
        let g = self.obj(ObjectGuid::new(GEN));
        g.wo.world_object_generators.generator_profiles[0]
            .spawned
            .keys()
            .map(|&k| ObjectGuid::new(k))
            .collect()
    }

    fn in_a_cell(&self, g: ObjectGuid) -> bool {
        phys_ext::physics_obj(&self.w, g).is_some_and(|h| phys_ext::cur_cell(&self.w, h).is_some())
    }
}

fn creature(wcid: u32, name: &str) -> Weenie {
    Weenie::new(wcid, name, WeenieType::Creature)
        .with_string(PropertyString::Name, name)
        .with_did(PropertyDataId::Setup, SETUP)
}

/// A generator of two `SPAWN_WCID`s (one profile, probability 1: each selection draws), with a
/// 10 s regeneration and a 5 s profile delay; an encounter of `ENCOUNTER_WCID` in cell (2, 3).
fn content() -> MemContent {
    let mut generator = Weenie::new(GEN_WCID, "gen", WeenieType::Generic)
        .with_string(PropertyString::Name, "gen")
        .with_did(PropertyDataId::Setup, SETUP)
        .with_int(PropertyInt::InitGeneratedObjects, 2)
        .with_int(PropertyInt::MaxGeneratedObjects, 2)
        .with_float(PropertyFloat::RegenerationInterval, 10.0);
    generator.weenie_properties_generator = vec![WeeniePropertiesGenerator {
        id: 1,
        object_id: GEN_WCID,
        probability: 1.0,
        weenie_class_id: SPAWN_WCID,
        delay: Some(5.0),
        init_create: 1,
        max_create: 2,
        ..Default::default()
    }];
    MemContent::new()
        .weenie(generator)
        .weenie(creature(SPAWN_WCID, "spawn"))
        // TimeToRot -1: a dynamic creature with no generator would otherwise rot away after
        // WorldObject_Decay's default 5 minutes, before the unload below saves it
        .weenie(creature(ENCOUNTER_WCID, "encounter").with_float(PropertyFloat::TimeToRot, -1.0))
        .landblock_instance(LandblockInstance::new(
            GEN,
            GEN_WCID,
            LB | 0x0019,
            [100.0, 100.0, 0.0],
        ))
        .encounter(Encounter {
            id: 1,
            landblock: 0xA9B4,
            weenie_class_id: ENCOUNTER_WCID,
            cell_x: 2,
            cell_y: 3,
            ..Encounter::default()
        })
}

/// `ThreadSafeRandom.Next(0.0f, 5)`: the heartbeat spread of a constructor, from a reference draw.
fn spread(d: f64) -> f64 {
    d * f64::from(5.0f32 - 0.0f32)
}

#[test]
fn a_generator_landblock_loads_spawns_ticks_respawns_and_unloads_in_aces_order() {
    let mut h = H::new(content());
    ThreadSafeRandom::seed(SEED as u64);
    let mut reference = DotNetRandom::new(SEED);
    let gen = ObjectGuid::new(GEN);

    // ---- Load: Init builds the generator instance, then the encounter (a GuidManager guid);
    // each constructor draws its heartbeat, in that order.
    let load_time = h.w.now.unix_time;
    lm::get_landblock(&mut h.w, id(), false, false);
    let encounter = ObjectGuid::new(ObjectGuid::DYNAMIC_MIN);
    assert_eq!(
        gm::dynamic_current(&h.w),
        ObjectGuid::DYNAMIC_MIN + 1,
        "the encounter's guid is drawn at load"
    );

    // The first tick runs the load's delegates (the objects join the landblock and the physics
    // world), then the generator's due update and regeneration: the power-up selects the profile
    // twice (one draw each) and then spawns both objects, each constructor drawing its heartbeat.
    // It runs at the load's time, so no heartbeat is due yet (each spread is above 0).
    h.tick();
    let first_tick = h.w.now.unix_time;
    assert_eq!(first_tick, load_time);
    assert_eq!(
        tick::next_heartbeat_time(h.obj(gen)),
        load_time + spread(reference.next_double()),
        "generator first"
    );
    assert_eq!(
        tick::next_heartbeat_time(h.obj(encounter)),
        load_time + spread(reference.next_double()),
        "then the encounter"
    );
    reference.next_double(); // SelectAProfile
    reference.next_double(); // SelectAProfile
    let spawns = h.spawned();
    assert_eq!(spawns.len(), 2);
    for &s in &spawns {
        assert_eq!(
            tick::next_heartbeat_time(h.obj(s)),
            first_tick + spread(reference.next_double()),
            "spawn order"
        );
    }
    assert_eq!(
        ThreadSafeRandom::next_float(0.0, 1.0),
        reference.next_double(),
        "no other draw"
    );
    assert_eq!(
        spawns.iter().map(|g| g.full()).collect::<Vec<_>>(),
        [ObjectGuid::DYNAMIC_MIN + 1, ObjectGuid::DYNAMIC_MIN + 2]
    );

    // Everything is on the landblock and in the physics world. The spawns entered through
    // EnterWorld -> LandblockManager.AddObject into pendingAdditions, which the landblock's first
    // heartbeat (later in the same tick) processed.
    assert!(h
        .w
        .landblock_manager
        .landblocks
        .expect(id())
        .create_world_objects_completed());
    assert_eq!(h.objects_of(), [gen, encounter, spawns[0], spawns[1]]);
    for g in [gen, encounter, spawns[0], spawns[1]] {
        assert_eq!(h.obj(g).current_landblock, Some(id()), "{g:?}");
        assert!(h.in_a_cell(g), "{g:?} has a body in a cell");
    }
    for &s in &spawns {
        assert_eq!(h.obj(s).wo.world_object_generators.generator, Some(gen));
        // Spawn_Default puts it at the generator's position; placement slides it clear of the bodies there.
        let (at, from) = (h.obj(s).location().unwrap(), h.obj(gen).location().unwrap());
        assert_eq!(at.landblock_id().landblock(), 0xA9B4);
        assert!(
            at.squared_distance_to(&from) <= 2.0 * 2.0,
            "{at:?} near {from:?}"
        );
    }
    // The physics landblock's server objects: CreateWorldObjects set the generator's Order to 0
    // and sorted, so it leads; the others follow in the order they entered.
    let bodies: Vec<_> = [gen, encounter, spawns[0], spawns[1]]
        .iter()
        .map(|&g| phys_ext::physics_obj(&h.w, g).unwrap())
        .collect();
    assert_eq!(phys_ext::get_server_objects(&h.w, 0xA9B4, false), bodies);
    assert_eq!(phys_ext::server_record(&h.w, bodies[0]).unwrap().order, 0);

    // ---- Ticking: heartbeats and generator updates run when due.
    h.run(7);
    let now = h.w.now.unix_time;
    for g in [gen, encounter, spawns[0], spawns[1]] {
        let stamp = h
            .obj(g)
            .get_property(PropertyFloat::HeartbeatTimestamp)
            .expect("heartbeat ran");
        assert!(stamp > first_tick && stamp <= now, "{g:?}");
        assert_eq!(
            tick::next_heartbeat_time(h.obj(g)),
            stamp + 5.0,
            "rescheduled by its interval"
        );
    }
    let update = h
        .obj(gen)
        .get_property(PropertyFloat::GeneratorUpdateTimestamp)
        .expect("generator updated");
    assert!(update > first_tick, "GeneratorUpdate runs every 5 s");
    assert_eq!(tick::next_generator_update_time(h.obj(gen)), update + 5.0);

    // ---- A kill: Destroy frees the slot; the profile waits its delay, and the next regeneration
    // after it respawns the object.
    let victim = spawns[0];
    wo::destroy(&mut h.w, victim, true, false);
    assert!(!h.w.objects.contains(victim));
    assert!(phys_ext::physics_obj(&h.w, victim).is_none());
    assert_eq!(
        phys_ext::get_server_objects(&h.w, 0xA9B4, false),
        [bodies[0], bodies[1], bodies[3]],
        "DestroyObject"
    );
    assert_eq!(h.spawned(), [spawns[1]]);
    h.tick();
    assert_eq!(
        h.objects_of(),
        [gen, encounter, spawns[1]],
        "removed from the landblock"
    );
    h.run(16);
    let respawned = h.spawned();
    assert_eq!(respawned.len(), 2, "regeneration refilled the slot");
    let fresh = respawned.into_iter().find(|&g| g != spawns[1]).unwrap();
    assert_eq!(
        fresh.full(),
        ObjectGuid::DYNAMIC_MIN + 3,
        "a new guid (a recycled one waits out its time)"
    );
    assert!(h.in_a_cell(fresh));
    // worldObjects is a .NET Dictionary: the new entry takes the removed one's free slot.
    assert_eq!(h.objects_of(), [gen, encounter, fresh, spawns[1]]);

    h.run(60);
    assert!(h.w.landblock_manager.landblocks.expect(id()).is_dormant);
    h.run(300);
    assert!(!lm::is_loaded(&h.w, id()));
    assert_eq!(h.w.objects.len(), 0, "no object is left in the store");
    assert!(
        phys_ext::get_server_objects(&h.w, 0xA9B4, false).is_empty(),
        "the physics landblock is gone"
    );
    let info = gm::get_dynamic_guid_debug_info(&mut h.w);
    assert!(
        info.ends_with("recycled GUIDs available: 3"),
        "the three destroyed dynamic guids: {info}"
    );
    let mut shard = h.w.shard.base_database();
    assert!(
        shard.get_biota(encounter.full(), false).is_some(),
        "the encounter was saved"
    );
    assert!(
        shard.get_biota(gen.full(), false).is_none()
            && shard.get_biota(fresh.full(), false).is_none(),
        "generators and spawns are not"
    );
}

#[test]
fn create_world_objects_puts_the_instances_first_among_the_server_objects() {
    // An object that joins the landblock before CreateWorldObjects' delegate runs keeps Order 1;
    // the instances get Order 0 and SortObjects moves them ahead of it.
    let mut h = H::new(content());
    lm::get_landblock(&mut h.w, id(), false, false);
    let early = ObjectGuid::new(0x7A9B_4100);
    let mut o = WorldObject {
        guid: early,
        ..WorldObject::default()
    };
    o.set_property(PropertyDataId::Setup, SETUP);
    o.set_heartbeat_interval(Some(0.0));
    tick::world_object_initialize_heartbeats(&mut o, 0.0);
    o.set_location(Some(empyrean_entity::Position::from_components(
        LB | 0x0001,
        20.0,
        20.0,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
        false,
    )));
    h.w.objects.insert(o).unwrap();
    assert!(lm::add_object(&mut h.w, early, false));
    h.tick();
    let body = |g: ObjectGuid| phys_ext::physics_obj(&h.w, g).unwrap();
    let objects = phys_ext::get_server_objects(&h.w, 0xA9B4, false);
    assert_eq!(
        objects[..2],
        [body(ObjectGuid::new(GEN)), body(early)],
        "the instance, then the early object"
    );
}

#[test]
fn adjacent_landblocks_share_their_server_objects_through_set_adjacents() {
    // LandblockManager.SetAdjacents -> PhysicsLandblock.SetAdjacents: an object on 0xA9B4 is a
    // server object of 0xAAB4's neighbourhood, as 0xAAB4 loaded after it (pSync). 0xA9B4's own
    // physics adjacents stay as they were when it loaded alone: ACE re-syncs a neighbour's
    // Adjacents without pSync.
    let mut h = H::new(MemContent::new());
    land::use_flat_land_with_test_setup(&mut h.w, &[0xA9B4, 0xAAB4], 0);
    let east = LandblockId::new(0xAAB4_FFFF);
    lm::get_landblock(&mut h.w, id(), false, false);
    lm::get_landblock(&mut h.w, east, false, false);
    let g = ObjectGuid::new(0x7A9B_4100);
    let mut o = WorldObject {
        guid: g,
        ..WorldObject::default()
    };
    o.set_property(PropertyDataId::Setup, SETUP);
    o.set_heartbeat_interval(Some(0.0));
    tick::world_object_initialize_heartbeats(&mut o, 0.0);
    o.set_location(Some(empyrean_entity::Position::from_components(
        LB | 0x0001,
        20.0,
        20.0,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
        false,
    )));
    h.w.objects.insert(o).unwrap();
    assert!(lm::add_object(&mut h.w, g, false));
    let body = phys_ext::physics_obj(&h.w, g).unwrap();
    assert!(phys_ext::get_server_objects(&h.w, 0xAAB4, false).is_empty());
    assert_eq!(phys_ext::get_server_objects(&h.w, 0xAAB4, true), [body]);
    assert_eq!(
        h.w.landblock_manager.landblocks.expect(id()).adjacents,
        [east],
        "the server landblock's own list is current"
    );
}
