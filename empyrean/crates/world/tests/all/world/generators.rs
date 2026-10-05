//! ACE: Source/ACE.Server/WorldObjects/WorldObject_Tick.cs::GeneratorUpdate
//! Heartbeats and generator profiles spawn/regenerate with ACE's RNG order (System.Random
//! reference).
//! Fixture: synthetic dats, isolated world state.

use std::sync::Arc;
use std::time::Duration;

use empyrean_common::clock::ClockSnapshot;
use empyrean_common::dotnet::datetime::DotNetDateTime;
use empyrean_common::dotnet::CsCast;
use empyrean_common::random::DotNetRandom;
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_common::time::Time;
use empyrean_content::models::world::treasure_wielded::TreasureWielded;
use empyrean_content::models::world::{Weenie as WeenieRow, WeeniePropertiesGenerator};
use empyrean_content::MemContent;
use empyrean_dat::FakeDats;
use empyrean_entity::enums::{
    GeneratorDestruct, GeneratorTimeType, PositionType, PropertyBool, PropertyDataId,
    PropertyFloat, PropertyInt, PropertyString, RegenerationType, WeenieType,
};
use empyrean_entity::models::PropertiesGenerator;
use empyrean_entity::{LandblockId, ObjectGuid};
use empyrean_world::dispatch::{self, Class};
use empyrean_world::entity::generator_profile::{self as gp, GeneratorProfile};
use empyrean_world::factories::world_object_factory as factory;
use empyrean_world::managers::landblock_manager as lm;
use empyrean_world::physics::phys_ext;
use empyrean_world::world_objects::world_object::{self, CtorEnv, WorldObject};
use empyrean_world::world_objects::{world_object_generators as gens, world_object_tick as tick};
use empyrean_world::World;

// ------------------------------------------------------------------------------------ helpers

const SEED: i32 = 7;
const GEN: u32 = 0x7000_1000;

fn utc0() -> DotNetDateTime {
    DotNetDateTime::new_hms(2026, 9, 22, 12, 0, 0)
}

fn snapshot(utc: DotNetDateTime) -> ClockSnapshot {
    ClockSnapshot {
        portal_year_ticks: 0.0,
        unix_time: Time::get_unix_time_at(utc),
        utc,
        monotonic: Duration::ZERO,
    }
}

use empyrean_testkit::EmptyShard;

/// The landblock the generators stand on.
const LB: u16 = 0xA9B4;
/// The synthetic setup every weenie carries (the testkit's).
const SETUP: u32 = empyrean_testkit::land::TEST_SETUP;

/// A world with no land (FakeDats): nothing can be placed.
fn unplaced_world(content: MemContent) -> World {
    let mut w = World::new(
        snapshot(utc0()),
        FakeDats::new().build().expect("empty fake dats"),
    );
    w.content = Arc::new(content);
    empyrean_world::managers::guid_manager::initialize(&mut w, &mut EmptyShard);
    w
}

/// A world whose landblock `LB` is flat land at 94 m (the generators' height), with the setup.
fn world(content: MemContent) -> World {
    let mut w = unplaced_world(content);
    empyrean_testkit::land::use_flat_land_with_test_setup(&mut w, &[LB], 47);
    w
}

fn landblock_id() -> LandblockId {
    LandblockId::new(u32::from(LB) << 16 | 0xFFFF)
}

/// `create`, then `LandblockManager.AddObject`: the generator stands on its loaded landblock.
fn place(w: &mut World, wcid: u32, guid: u32) -> ObjectGuid {
    let g = create(w, wcid, guid);
    assert!(
        lm::add_object(w, g, false),
        "the generator joined its landblock"
    );
    g
}

/// The landblock's objects, placed and pending, in .NET order.
fn on_landblock(w: &World) -> Vec<ObjectGuid> {
    let l = w.landblock_manager.landblocks.expect(landblock_id());
    l.world_object_guids()
        .chain(l.pending_addition_guids())
        .copied()
        .collect()
}

fn advance(w: &mut World, secs: f64) {
    w.now = snapshot(w.now.utc.add_seconds(secs));
}

fn plain(wcid: u32, name: &str) -> WeenieRow {
    WeenieRow::new(wcid, name, WeenieType::Generic)
        .with_string(PropertyString::Name, name)
        .with_did(PropertyDataId::Setup, SETUP)
}

fn row(
    wcid: u32,
    probability: f32,
    init_create: i32,
    max_create: i32,
) -> WeeniePropertiesGenerator {
    WeeniePropertiesGenerator {
        probability,
        weenie_class_id: wcid,
        init_create,
        max_create,
        ..Default::default()
    }
}

fn generator(
    wcid: u32,
    init: i32,
    max: i32,
    regen: f64,
    rows: Vec<WeeniePropertiesGenerator>,
) -> WeenieRow {
    let mut g = plain(wcid, "gen")
        .with_int(PropertyInt::InitGeneratedObjects, init)
        .with_int(PropertyInt::MaxGeneratedObjects, max)
        .with_position(
            PositionType::Location,
            0xA9B4_0019,
            [84.0, 7.1, 94.0],
            [1.0, 0.0, 0.0, 0.0],
        );
    if regen != 0.0 {
        g = g.with_float(PropertyFloat::RegenerationInterval, regen);
    }
    g.weenie_properties_generator = rows
        .into_iter()
        .enumerate()
        .map(|(k, r)| WeeniePropertiesGenerator {
            id: u32::try_from(k + 1).unwrap(),
            object_id: wcid,
            ..r
        })
        .collect();
    g
}

/// `WorldObjectFactory.CreateNewWorldObject(wcid)` with a fixed guid, added to the store.
fn create(w: &mut World, wcid: u32, guid: u32) -> ObjectGuid {
    let o = CtorEnv::with_world(w, |env| {
        factory::create_new_world_object_by_wcid(env, wcid, ObjectGuid::new(guid))
    })
    .expect("weenie exists");
    w.objects.insert(o).expect("fresh guid");
    ObjectGuid::new(guid)
}

fn obj(w: &World, g: ObjectGuid) -> &WorldObject {
    w.objects.get(g).expect("object in store")
}

fn profiles(w: &World, g: ObjectGuid) -> &[GeneratorProfile] {
    &obj(w, g).wo.world_object_generators.generator_profiles
}

fn spawned(w: &World, g: ObjectGuid, index: usize) -> Vec<ObjectGuid> {
    profiles(w, g)[index]
        .spawned
        .keys()
        .map(|&k| ObjectGuid::new(k))
        .collect()
}

/// `ThreadSafeRandom.Next(0.0f, 5)`, the heartbeat spread, from a reference draw.
fn spread(d: f64) -> f64 {
    d * f64::from(5.0f32 - 0.0f32)
}

/// The generator's heartbeat draw, then `GeneratorUpdate` and `GeneratorRegeneration` at `now`.
fn start_and_regenerate(w: &mut World, g: ObjectGuid) {
    let now = w.now.unix_time;
    tick::generator_update(w, g, now);
    tick::generator_regeneration(w, g, now);
}

// ------------------------------------------------------------------------------------ heartbeats

#[test]
fn construction_draws_the_heartbeat_rng_exactly_once() {
    let mut w = world(MemContent::new().weenie(plain(2001, "a")));
    ThreadSafeRandom::seed(SEED as u64);
    let mut reference = DotNetRandom::new(SEED);

    let a = create(&mut w, 2001, 0x7000_0001);
    let b = create(&mut w, 2001, 0x7000_0002);

    let now = w.now.unix_time;
    let t = &obj(&w, a).wo.world_object_tick;
    assert_eq!(t.next_heartbeat_time, now + spread(reference.next_double()));
    assert_eq!(t.cached_heartbeat_interval, 5.0);
    assert_eq!(t.next_generator_update_time, f64::MAX);
    assert_eq!(t.next_generator_regeneration_time, f64::MAX);
    assert_eq!(
        obj(&w, a).heartbeat_interval(),
        Some(5.0),
        "HeartbeatInterval = 5.0f is written to the biota"
    );
    assert_eq!(
        tick::next_heartbeat_time(obj(&w, b)),
        now + spread(reference.next_double())
    );

    // Nothing else drew: the next draw is the reference's third.
    assert_eq!(
        ThreadSafeRandom::next_float(0.0, 1.0),
        reference.next_double()
    );
    let hits = empyrean_common::not_ported::take_local();
    assert!(
        !hits.contains_key("ACE: WorldObject.InitializeHeartbeats"),
        "{hits:?}"
    );
    assert!(
        !hits.contains_key("ACE: WorldObject.InitializeGenerator"),
        "{hits:?}"
    );
}

#[test]
fn initialize_heartbeats_cases() {
    ThreadSafeRandom::seed(SEED as u64);
    let mut reference = DotNetRandom::new(SEED);

    // GamePiece: HeartbeatInterval 1.0; draws.
    let mut o = WorldObject::allocate(Class::GamePiece);
    o.biota.weenie_type = WeenieType::GamePiece;
    tick::world_object_initialize_heartbeats(&mut o, 100.0);
    assert_eq!(o.heartbeat_interval(), Some(1.0));
    assert_eq!(
        o.wo.world_object_tick.next_heartbeat_time,
        100.0 + spread(reference.next_double())
    );

    // HeartbeatInterval 0: no draw, no heartbeat. Negative RegenerationInterval is reset to 0.
    let mut o = WorldObject::allocate(Class::GenericObject);
    o.set_heartbeat_interval(Some(0.0));
    o.set_regeneration_interval(-3.0);
    tick::world_object_initialize_heartbeats(&mut o, 100.0);
    assert_eq!(o.wo.world_object_tick.next_heartbeat_time, f64::MAX);
    assert_eq!(o.regeneration_interval(), 0.0);
    assert_eq!(
        ThreadSafeRandom::next_float(0.0, 1.0),
        reference.next_double(),
        "HeartbeatInterval 0 must not draw"
    );

    // A generator starts right away; with RegenerationInterval 0 it never regenerates, otherwise its
    // NextGeneratorRegenerationTime keeps the field's value (0: due at once).
    let mut o = WorldObject::allocate(Class::GenericObject);
    o.wo.world_object_generators
        .generator_profiles
        .push(GeneratorProfile::new(
            o.guid,
            PropertiesGenerator::default(),
            0,
            utc0(),
        ));
    tick::world_object_initialize_heartbeats(&mut o, 100.0);
    assert_eq!(o.wo.world_object_tick.next_generator_update_time, 100.0);
    assert_eq!(
        o.wo.world_object_tick.next_generator_regeneration_time,
        f64::MAX
    );
    o.set_regeneration_interval(60.0);
    o.wo.world_object_tick.next_generator_regeneration_time = 0.0;
    tick::world_object_initialize_heartbeats(&mut o, 200.0);
    assert_eq!(o.wo.world_object_tick.next_generator_regeneration_time, 0.0);
    assert_eq!(o.wo.world_object_tick.cached_regeneration_interval, 60.0);
}

#[test]
fn heartbeat_stamps_and_reschedules_and_lifespan_counts_down() {
    let mut w = world(MemContent::new().weenie(plain(2001, "a")));
    let a = create(&mut w, 2001, 0x7000_0001);
    w.objects.get_mut(a).unwrap().set_lifespan(Some(10));

    advance(&mut w, 4.0);
    assert_eq!(obj(&w, a).get_remaining_lifespan(w.now.utc), 6);
    assert!(!obj(&w, a).is_lifespan_spent(w.now.utc));

    let now = w.now.unix_time;
    dispatch::heartbeat::heartbeat(&mut w, a, now);
    assert_eq!(
        obj(&w, a).get_property(PropertyFloat::HeartbeatTimestamp),
        Some(now)
    );
    assert_eq!(tick::next_heartbeat_time(obj(&w, a)), now + 5.0);

    advance(&mut w, 6.0);
    assert!(obj(&w, a).is_lifespan_spent(w.now.utc));
    let now = w.now.unix_time;
    dispatch::heartbeat::heartbeat(&mut w, a, now);
    assert!(
        w.objects
            .get(a)
            .is_none_or(|o| o.wo.world_object.is_destroyed),
        "the spent object is destroyed"
    );
}

// ------------------------------------------------------------------------------------ generators

#[test]
fn initial_spawn_selects_all_profiles_before_creating_any_object() {
    let content = MemContent::new()
        .weenie(plain(2001, "a"))
        .weenie(plain(2002, "b"))
        .weenie(generator(
            1000,
            3,
            3,
            60.0,
            vec![row(2001, 0.4, 1, 3), row(2002, 1.0, 1, 3)],
        ));
    let mut w = world(content);
    ThreadSafeRandom::seed(SEED as u64);
    let mut reference = DotNetRandom::new(SEED);

    let g = place(&mut w, 1000, GEN);
    let now = w.now.unix_time;
    assert_eq!(
        tick::next_heartbeat_time(obj(&w, g)),
        now + spread(reference.next_double())
    );
    assert_eq!(
        tick::next_generator_update_time(obj(&w, g)),
        now,
        "generators start right away"
    );
    assert!(obj(&w, g).is_generator());

    empyrean_common::not_ported::take_local();
    start_and_regenerate(&mut w, g);
    let hits = empyrean_common::not_ported::take_local();
    assert_eq!(hits.get("ACE: EmoteManager.OnGeneration"), None, "{hits:?}");
    // (Each spawn now runs the real `NotifyPlayers`; that all three entered is checked below.)

    // SelectAProfile draws once per selection, all before any spawn: `Next(0, total)` against the
    // cumulative probabilities 0.4 and 1.0.
    let total = 0.4f32 + (1.0f32 - 0.4f32);
    let mut picks = Vec::new();
    for _ in 0..3 {
        let rng = reference.next_double() * f64::from(total);
        picks.push(usize::from(rng >= f64::from(0.4f32)));
    }
    let n0 = picks.iter().filter(|&&p| p == 0).count();
    assert!(
        n0 > 0 && n0 < 3,
        "seed {SEED} should pick both profiles: {picks:?}"
    );

    // Spawn_HeartBeat then runs profile by profile: every profile-0 object is created first, each
    // constructor drawing its heartbeat.
    let s0 = spawned(&w, g, 0);
    let s1 = spawned(&w, g, 1);
    assert_eq!((s0.len(), s1.len()), (n0, 3 - n0));
    for (o, wcid) in s0
        .iter()
        .map(|&o| (o, 2001))
        .chain(s1.iter().map(|&o| (o, 2002)))
    {
        let child = obj(&w, o);
        assert_eq!(child.biota.weenie_class_id, wcid);
        assert_eq!(
            tick::next_heartbeat_time(child),
            now + spread(reference.next_double()),
            "creation order"
        );
        // Placed for real: linked to its generator, on the landblock, its body in a cell.
        assert!(!child.wo.world_object.is_destroyed);
        assert_eq!(child.wo.world_object_generators.generator, Some(g));
        assert_eq!(child.generator_id(), Some(GEN));
        assert_eq!(child.current_landblock, Some(landblock_id()));
        assert!(phys_ext::physics_obj(&w, o).is_some_and(|h| phys_ext::cur_cell(&w, h).is_some()));
    }
    assert!(
        s0.iter().max() < s1.iter().min(),
        "guids are taken in creation order"
    );
    // Guids from GuidManager: DynamicMin upwards; the landblock lists them in creation order.
    let mut created: Vec<ObjectGuid> = s0.iter().chain(s1.iter()).copied().collect();
    created.sort_by_key(|o| o.full());
    assert_eq!(
        created.first().map(|o| o.full()),
        Some(ObjectGuid::DYNAMIC_MIN)
    );
    assert_eq!(
        on_landblock(&w)[1..],
        created[..],
        "the generator first, then its spawns"
    );
    assert_eq!(
        ThreadSafeRandom::next_float(0.0, 1.0),
        reference.next_double(),
        "no other draw"
    );

    let gen = obj(&w, g);
    assert_eq!(gen.current_create(), 3);
    assert!(!gen.currently_powering_up());
    assert!(gen.generator_entered_world());
    assert!(profiles(&w, g)
        .iter()
        .all(|p| p.spawn_queue.is_empty() && !p.first_spawn));
    assert_eq!(tick::next_generator_update_time(obj(&w, g)), now + 5.0);
    assert_eq!(
        tick::next_generator_regeneration_time(obj(&w, g)),
        now + 60.0
    );
    assert_eq!(
        obj(&w, g).get_property(PropertyFloat::RegenerationTimestamp),
        Some(now)
    );
}

#[test]
fn init_create_stops_the_power_up_and_max_create_caps_the_regeneration() {
    let content = MemContent::new().weenie(plain(2001, "a")).weenie(generator(
        1000,
        2,
        4,
        60.0,
        vec![row(2001, -1.0, 3, 5)],
    ));
    let mut w = world(content);
    ThreadSafeRandom::seed(SEED as u64);
    let mut reference = DotNetRandom::new(SEED);
    let g = place(&mut w, 1000, GEN);
    reference.next_double(); // the generator's heartbeat

    // Power-up: one selection of the always-spawn profile enqueues its InitCreate (3, within the
    // generator's 4 slots); CurrentCreate 3 >= InitCreate 2 then stops the loop.
    start_and_regenerate(&mut w, g);
    assert_eq!(spawned(&w, g, 0).len(), 3);
    for _ in 0..4 {
        reference.next_double(); // one selection, three constructors
    }
    assert_eq!(
        ThreadSafeRandom::next_float(0.0, 1.0),
        reference.next_double()
    );

    // The first regeneration spawns only the generator's last free slot (MaxCreate 5 is beyond it).
    advance(&mut w, 60.0);
    let before = w.objects.len();
    let now = w.now.unix_time;
    tick::generator_regeneration(&mut w, g, now);
    assert_eq!(w.objects.len(), before + 1, "one object created");
    assert_eq!(
        spawned(&w, g, 0).len(),
        4,
        "the respawn entered the world and is registered"
    );
    assert_eq!(obj(&w, g).current_create(), 4);
    assert_eq!(on_landblock(&w).len(), 5);
    assert!(profiles(&w, g)[0].spawn_queue.is_empty());
    reference.next_double();
    reference.next_double();
    assert_eq!(
        ThreadSafeRandom::next_float(0.0, 1.0),
        reference.next_double(),
        "one selection, one constructor"
    );

    // Max already reached by spawned + queued: fill the slot and no selection draws at all.
    w.objects
        .get_mut(g)
        .unwrap()
        .wo
        .world_object_generators
        .generator_profiles[0]
        .spawn_queue
        .push(DotNetDateTime::MAX_VALUE);
    let now = w.now.unix_time;
    tick::generator_regeneration(&mut w, g, now);
    assert_eq!(
        ThreadSafeRandom::next_float(0.0, 1.0),
        reference.next_double(),
        "stop conditions: no draw"
    );
}

#[test]
fn initialize_generator_raises_max_to_init() {
    let content = MemContent::new().weenie(generator(1000, 2, 0, 0.0, vec![row(2001, -1.0, 1, 1)]));
    let mut w = world(content);
    let g = create(&mut w, 1000, GEN);
    assert_eq!(obj(&w, g).max_generated_objects(), 2);
    assert_eq!(profiles(&w, g).len(), 1);
    assert_eq!(profiles(&w, g)[0].id, 0);
    assert_eq!(
        tick::next_generator_regeneration_time(obj(&w, g)),
        f64::MAX,
        "RegenerationInterval 0"
    );
}

#[test]
fn a_kill_frees_the_slot_after_the_profile_delay() {
    let mut r = row(2001, -1.0, 1, 1);
    r.delay = Some(30.0);
    r.when_create = RegenerationType::Destruction.0;
    let content =
        MemContent::new()
            .weenie(plain(2001, "a"))
            .weenie(generator(1000, 1, 1, 10.0, vec![r]));
    let mut w = world(content);
    ThreadSafeRandom::seed(SEED as u64);
    let g = place(&mut w, 1000, GEN);
    start_and_regenerate(&mut w, g);
    let child = spawned(&w, g, 0)[0];
    assert_eq!(
        obj(&w, child).wo.world_object_generators.generator,
        Some(g),
        "Spawn linked it"
    );
    assert_eq!(obj(&w, child).current_landblock, Some(landblock_id()));

    // The spawned object is destroyed (killed): Destroy raises NotifyOfEvent(Destruction), its
    // generator frees the slot and times the profile out, and the object leaves the landblock,
    // the physics world and the store.
    world_object::destroy(&mut w, child, true, false);
    assert!(spawned(&w, g, 0).is_empty());
    assert_eq!(
        profiles(&w, g)[0].next_available,
        w.now.utc.add_seconds(30.0)
    );
    assert!(!w.objects.contains(child));
    assert!(phys_ext::physics_obj(&w, child).is_none());
    let l = w.landblock_manager.landblocks.expect(landblock_id());
    assert!(
        !l.pending_addition_guids().any(|&o| o == child),
        "removed from its landblock"
    );

    // Ten seconds later the profile is still timed out: nothing is selected or created.
    advance(&mut w, 10.0);
    let before = w.objects.len();
    let now = w.now.unix_time;
    tick::generator_regeneration(&mut w, g, now);
    assert_eq!(w.objects.len(), before);
    assert!(obj(&w, g).all_profiles_unavailable(w.now.utc));

    // At 30 s it is available again (the DIVERGE on IsAvailable: equal counts as later) and respawns.
    advance(&mut w, 20.0);
    let now = w.now.unix_time;
    tick::generator_regeneration(&mut w, g, now);
    assert_eq!(w.objects.len(), before + 1);
    assert_eq!(
        tick::next_generator_regeneration_time(obj(&w, g)),
        now + 10.0
    );
    let respawn = spawned(&w, g, 0);
    assert_eq!(respawn.len(), 1, "the respawn fills the freed slot");
    assert_ne!(
        respawn[0], child,
        "a recycled guid waits out GuidManager's recycle time; the respawn draws a new one"
    );
}

#[test]
fn notify_generator_maps_pickup_and_destruction_and_undef() {
    for (when_create, event, freed) in [
        (
            RegenerationType::Destruction,
            RegenerationType::PickUp,
            true,
        ),
        (
            RegenerationType::PickUp,
            RegenerationType::Destruction,
            true,
        ),
        (RegenerationType::Undef, RegenerationType::Destruction, true),
        (RegenerationType::Undef, RegenerationType::PickUp, true),
        (
            RegenerationType::Death,
            RegenerationType::Destruction,
            false,
        ),
    ] {
        let mut r = row(2001, -1.0, 1, 1);
        r.when_create = when_create.0;
        let content =
            MemContent::new()
                .weenie(plain(2001, "a"))
                .weenie(generator(1000, 1, 1, 10.0, vec![r]));
        let mut w = world(content);
        let g = create(&mut w, 1000, GEN);
        start_and_regenerate(&mut w, g);
        let child = spawned(&w, g, 0)[0];
        gp::notify_generator(&mut w, g, 0, child, event);
        assert_eq!(
            spawned(&w, g, 0).is_empty(),
            freed,
            "{when_create:?} / {event:?}"
        );
    }
}

#[test]
fn an_event_generator_without_regeneration_spawns_when_it_starts() {
    let content = MemContent::new().weenie(plain(2001, "a")).weenie(
        generator(1000, 1, 1, 0.0, vec![row(2001, -1.0, 1, 1)])
            .with_int(PropertyInt::GeneratorTimeType, GeneratorTimeType::Event.0)
            .with_string(PropertyString::GeneratorEvent, "SomeEvent"),
    );
    let mut w = world(content);
    let g = create(&mut w, 1000, GEN);
    empyrean_common::not_ported::take_local();

    // GeneratorUpdate alone: EventManager has no event of that name (IsEventAvailable is false), so
    // the status never changes, and StartGenerator generates at once for an event generator with
    // RegenerationInterval 0.
    let now = w.now.unix_time;
    tick::generator_update(&mut w, g, now);
    assert_eq!(spawned(&w, g, 0).len(), 1);
    assert!(!obj(&w, g).currently_powering_up());
    assert!(empyrean_common::not_ported::take_local()
        .keys()
        .all(|k| !k.starts_with("ACE: EventManager.")));
}

#[test]
fn a_real_time_generator_changes_status_one_update_late() {
    let content = MemContent::new().weenie(plain(2001, "a")).weenie(
        generator(1000, 1, 1, 60.0, vec![row(2001, -1.0, 1, 1)]).with_int(
            PropertyInt::GeneratorTimeType,
            GeneratorTimeType::RealTime.0,
        ),
    );
    let mut w = world(content);
    let g = create(&mut w, 1000, GEN);
    let start: i32 = CsCast::<i32>::cs_cast(w.now.unix_time) + 100;
    w.objects
        .get_mut(g)
        .unwrap()
        .set_generator_start_time(start);

    // Before its start time: the first check stages the change, the entered-world re-check applies
    // it (disabled), so the generator does not start.
    let now = w.now.unix_time;
    tick::generator_update(&mut w, g, now);
    assert!(obj(&w, g).generator_disabled());
    assert!(!obj(&w, g).currently_powering_up());

    // After the start time: one update stages, the next enables and starts it.
    advance(&mut w, 101.0);
    let now = w.now.unix_time;
    tick::generator_update(&mut w, g, now);
    assert!(obj(&w, g).generator_disabled(), "staged, not yet applied");
    advance(&mut w, 5.0);
    let now = w.now.unix_time;
    tick::generator_update(&mut w, g, now);
    assert!(!obj(&w, g).generator_disabled());
    assert!(obj(&w, g).currently_powering_up());

    tick::generator_regeneration(&mut w, g, now);
    assert_eq!(spawned(&w, g, 0).len(), 1);
}

#[test]
fn the_spawn_queue_drains_at_its_time_and_initial_delay_schedules_regeneration() {
    let content = MemContent::new().weenie(plain(2001, "a")).weenie(
        generator(1000, 1, 2, 60.0, vec![row(2001, -1.0, 1, 2)])
            .with_float(PropertyFloat::GeneratorInitialDelay, 15.0),
    );
    let mut w = world(content);
    let g = place(&mut w, 1000, GEN);

    // GeneratorInitialDelay with no RegenerationTimestamp: regenerate now; with one: after the delay.
    let now = w.now.unix_time;
    tick::generator_update(&mut w, g, now);
    assert!(obj(&w, g).currently_powering_up());
    assert_eq!(tick::next_generator_regeneration_time(obj(&w, g)), now);
    let o = w.objects.get_mut(g).unwrap();
    o.set_currently_powering_up(false);
    o.set_regeneration_timestamp(1.0);
    gens::start_generator(&mut w, g);
    assert_eq!(
        tick::next_generator_regeneration_time(obj(&w, g)),
        now + 15.0
    );

    // A queued spawn waits for its time; at exactly that time it spawns.
    let due = w.now.utc.add_seconds(3.0);
    w.objects
        .get_mut(g)
        .unwrap()
        .wo
        .world_object_generators
        .generator_profiles[0]
        .spawn_queue
        .push(due);
    gp::spawn_heart_beat(&mut w, g, 0);
    assert_eq!(profiles(&w, g)[0].spawn_queue.len(), 1);
    assert_eq!(spawned(&w, g, 0).len(), 0);
    assert!(
        !profiles(&w, g)[0].first_spawn,
        "any ProcessQueue pass ends the first spawn"
    );
    advance(&mut w, 3.0);
    let before = w.objects.len();
    gp::spawn_heart_beat(&mut w, g, 0);
    assert!(profiles(&w, g)[0].spawn_queue.is_empty());
    assert_eq!(w.objects.len(), before + 1, "spawned at its time");
    assert_eq!(
        spawned(&w, g, 0).len(),
        1,
        "the spawn entered the world and is registered"
    );
}

#[test]
fn probability_sums_skip_maxed_and_timed_out_profiles() {
    let mut o = WorldObject::allocate(Class::GenericObject);
    let mut maxed = PropertiesGenerator {
        probability: -1.0,
        max_create: 0,
        ..Default::default()
    };
    maxed.weenie_class_id = 2004;
    let p = |probability: f32| PropertiesGenerator {
        probability,
        max_create: 5,
        weenie_class_id: 2001,
        ..Default::default()
    };
    let now = utc0();
    let list = &mut o.wo.world_object_generators.generator_profiles;
    list.push(GeneratorProfile::new(o.guid, p(0.25), 0, now));
    list.push(GeneratorProfile::new(o.guid, p(0.75), 1, now));
    list.push(GeneratorProfile::new(o.guid, p(0.5), 2, now));
    list.push(GeneratorProfile::new(o.guid, maxed, 3, now));

    // 0.25, +0.5, then 0.5 < 0.75 restarts the step from 0: +0.5. The maxed -1 profile is skipped.
    assert_eq!(o.get_total_probability(now), 1.25);
    assert_eq!(
        (0..4)
            .map(|i| o.get_adjusted_probability(i, now))
            .collect::<Vec<_>>(),
        vec![0.25, 0.75, 1.25, -1.0]
    );
    assert_eq!(o.get_max_probability(), 0.75);

    // Profile 1 timed out: its step is skipped but it still sets the last probability.
    o.wo.world_object_generators.generator_profiles[1].next_available = now.add_seconds(1.0);
    assert_eq!(o.get_total_probability(now), 0.75);
    assert_eq!(o.get_adjusted_probability(2, now), 0.75);
    assert_eq!(o.generator_active_profiles(now), vec![1]);
    assert!(!o.all_profiles_maxed());

    // An available, unmaxed -1 profile makes the total 1.
    o.wo.world_object_generators.generator_profiles[3]
        .biota
        .max_create = -1;
    assert_eq!(o.get_total_probability(now), 1.0);
}

#[test]
fn reset_and_destroy_directive_clear_the_profiles() {
    let content = MemContent::new().weenie(plain(2001, "a")).weenie(generator(
        1000,
        2,
        2,
        60.0,
        vec![row(2001, -1.0, 2, 2)],
    ));
    let mut w = world(content);
    let g = create(&mut w, 1000, GEN);
    start_and_regenerate(&mut w, g);
    assert_eq!(spawned(&w, g, 0).len(), 2);
    assert_eq!(
        obj(&w, g).get_static_guid(spawned(&w, g, 0)[1].full()),
        Some(0)
    );

    gens::process_generator_destruction_directive(&mut w, g, GeneratorDestruct::Destroy, false);
    assert!(spawned(&w, g, 0).is_empty());
    assert_eq!(profiles(&w, g)[0].next_available, w.now.utc);

    start_and_regenerate(&mut w, g);
    dispatch::reset_generator::reset_generator(&mut w, g);
    assert_eq!(obj(&w, g).current_create(), 0);
}

#[test]
fn destroying_a_generator_runs_its_directive_and_a_wiped_out_generator_destroys_itself() {
    // Destroy -> OnGeneratorDestroy -> ProcessGeneratorDestructionDirective(GeneratorDestructionType).
    let content = MemContent::new().weenie(plain(2001, "a")).weenie(
        generator(1000, 1, 1, 60.0, vec![row(2001, -1.0, 1, 1)]).with_int(
            PropertyInt::GeneratorDestructionType,
            GeneratorDestruct::Destroy.0,
        ),
    );
    let mut w = world(content);
    let g = place(&mut w, 1000, GEN);
    start_and_regenerate(&mut w, g);
    let child = spawned(&w, g, 0)[0];
    assert!(w.objects.contains(child));
    world_object::destroy(&mut w, g, true, false);
    // DestroyAll destroyed the spawn; both left the store and the landblock.
    assert!(!w.objects.contains(child), "DestroyAll destroyed the spawn");
    assert!(!w.objects.contains(g));
    assert!(on_landblock(&w).is_empty());

    // GeneratorAutomaticDestruction: the last spawn of a generator with InitCreate > 0 goes, so
    // the generator is destroyed.
    let content = MemContent::new().weenie(plain(2001, "a")).weenie(
        generator(1000, 1, 1, 60.0, vec![row(2001, -1.0, 1, 1)])
            .with_bool(PropertyBool::GeneratorAutomaticDestruction, true),
    );
    let mut w = world(content);
    let g = place(&mut w, 1000, GEN);
    start_and_regenerate(&mut w, g);
    let child = spawned(&w, g, 0)[0];
    assert_eq!(
        obj(&w, child).wo.world_object_generators.generator,
        Some(g),
        "Spawn linked it"
    );
    assert!(!obj(&w, g).wo.world_object.is_destroyed);
    gens::notify_of_event(&mut w, child, RegenerationType::PickUp);
    assert!(!w.objects.contains(g), "the generator destroyed itself");
    assert!(
        w.objects.contains(child),
        "a picked-up spawn is not destroyed"
    );
}

#[test]
fn a_profile_of_an_undef_weenie_recycles_the_guid_it_drew() {
    // CreateNewWorldObject(wcid): the weenie exists, so a dynamic guid is drawn; CreateWorldObject
    // returns null for an Undef weenie, so the guid goes back to GuidManager and nothing spawns.
    let content = MemContent::new()
        .weenie(WeenieRow::new(2009, "undef", WeenieType::Undef))
        .weenie(generator(1000, 1, 1, 60.0, vec![row(2009, -1.0, 1, 1)]));
    let mut w = world(content);
    let g = place(&mut w, 1000, GEN);
    start_and_regenerate(&mut w, g);
    assert!(spawned(&w, g, 0).is_empty());
    assert_eq!(w.objects.len(), 1, "only the generator");
    let info = empyrean_world::managers::guid_manager::get_dynamic_guid_debug_info(&mut w);
    assert!(info.ends_with("recycled GUIDs available: 1"), "{info}");
}

/// V340 (a fix): a treasure profile (Contain | Treasure, as the world database's chests have
/// them) whose wielded-treasure roll creates nothing spawns nothing, and the generator goes on.
/// ACE iterated the roll's null list and threw.
#[test]
fn a_wielded_treasure_roll_that_creates_nothing_spawns_nothing() {
    let content = MemContent::new()
        .weenie(plain(2001, "sword"))
        .treasure_wielded(TreasureWielded {
            id: 1,
            treasure_type: 77,
            weenie_class_id: 2001,
            probability: 0.0,
            set_start: true,
            ..Default::default()
        })
        .weenie(generator(
            1000,
            1,
            1,
            60.0,
            vec![WeeniePropertiesGenerator {
                where_create: 72,
                ..row(77, -1.0, 1, 1)
            }],
        ));
    let mut w = world(content);
    let g = place(&mut w, 1000, GEN);
    start_and_regenerate(&mut w, g);
    assert!(spawned(&w, g, 0).is_empty());
    assert_eq!(w.objects.len(), 1, "only the generator");
}

/// An `event` row as the world database holds it (no time window unless given).
fn event_row(
    name: &str,
    state: empyrean_entity::enums::GameEventState,
) -> empyrean_content::models::world::Event {
    empyrean_content::models::world::Event {
        id: 1,
        name: name.to_owned(),
        start_time: -1,
        end_time: -1,
        state: state.0,
        ..Default::default()
    }
}

/// A placed event generator (`GeneratorTimeType.Event`, RegenerationInterval 0, as the world DB's
/// seasonal generators are) for `event`, destroying its spawns when it is disabled; the world
/// database holds the event `name` in `state`, loaded by `EventManager.Initialize`.
fn event_generator_world(
    event: &str,
    name: &str,
    state: empyrean_entity::enums::GameEventState,
) -> (World, ObjectGuid) {
    let content = MemContent::new()
        .weenie(plain(2001, "a"))
        .weenie(
            generator(1000, 1, 1, 0.0, vec![row(2001, -1.0, 1, 1)])
                .with_int(PropertyInt::GeneratorTimeType, GeneratorTimeType::Event.0)
                .with_int(
                    PropertyInt::GeneratorEndDestructionType,
                    GeneratorDestruct::Destroy.0,
                )
                .with_string(PropertyString::GeneratorEvent, event),
        )
        .event(event_row(name, state));
    let mut w = world(content);
    empyrean_world::managers::event_manager::initialize(&mut w);
    let g = place(&mut w, 1000, GEN);
    (w, g)
}

fn update(w: &mut World, g: ObjectGuid) {
    let now = w.now.unix_time;
    tick::generator_update(w, g, now);
}

/// The Fall Festival case: the event is `Off` (the world DB's state), so the generator's first
/// update stages the change and its entered-world re-check applies it: disabled, nothing spawned.
/// `@event start` turns it `On`: one update stages, the next enables it and, RegenerationInterval
/// being 0, generates at once. `@event stop`: one update stages, the next disables it and its
/// GeneratorEndDestructionType (Destroy) removes the spawn.
#[test]
fn an_event_generator_spawns_only_while_its_event_is_on() {
    use empyrean_entity::enums::GameEventState;
    use empyrean_world::managers::event_manager as em;

    let (mut w, g) = event_generator_world(
        "EventFallFestival",
        "EventFallFestival",
        GameEventState::Off,
    );

    update(&mut w, g);
    assert!(
        obj(&w, g).generator_disabled(),
        "disabled at once (the entered-world re-check)"
    );
    assert!(
        spawned(&w, g, 0).is_empty(),
        "no spawn while the event is off"
    );
    update(&mut w, g);
    assert!(spawned(&w, g, 0).is_empty());

    // `@event start EventFallFestival` (any case, with an @comment)
    assert!(em::start_event(&mut w, "eventfallfestival@why", None, None));
    assert_eq!(
        em::get_event_status(&w, "EventFallFestival"),
        GameEventState::On
    );
    update(&mut w, g);
    assert!(obj(&w, g).generator_disabled(), "staged, not yet applied");
    assert!(spawned(&w, g, 0).is_empty());
    update(&mut w, g);
    assert!(!obj(&w, g).generator_disabled());
    let child = spawned(&w, g, 0);
    assert_eq!(child.len(), 1, "spawned when the event starts");

    assert!(em::stop_event(&mut w, "EventFallFestival", None, None));
    assert_eq!(
        em::get_event_status(&w, "EventFallFestival"),
        GameEventState::Off
    );
    update(&mut w, g);
    assert_eq!(spawned(&w, g, 0), child, "staged");
    update(&mut w, g);
    assert!(obj(&w, g).generator_disabled());
    assert!(
        spawned(&w, g, 0).is_empty(),
        "GeneratorEndDestructionType Destroy"
    );
    assert!(w.objects.get(child[0]).is_none(), "the spawn was destroyed");
}

/// A `Disabled` event can be neither started nor stopped, and keeps its generator off; an `Enabled`
/// one counts as not started until it is started, so its generator is off too.
#[test]
fn disabled_and_enabled_events_keep_their_generators_off() {
    use empyrean_entity::enums::GameEventState;
    use empyrean_world::managers::event_manager as em;

    for state in [GameEventState::Disabled, GameEventState::Enabled] {
        let (mut w, g) = event_generator_world("Ev", "Ev", state);
        update(&mut w, g);
        update(&mut w, g);
        assert!(obj(&w, g).generator_disabled(), "{state:?}");
        assert!(spawned(&w, g, 0).is_empty(), "{state:?}");
    }

    let (mut w, g) = event_generator_world("Ev", "Ev", GameEventState::Disabled);
    assert!(!em::start_event(&mut w, "Ev", None, None));
    assert!(!em::stop_event(&mut w, "Ev", None, None));
    assert_eq!(em::get_event_status(&w, "Ev"), GameEventState::Disabled);
    update(&mut w, g);
    update(&mut w, g);
    assert!(spawned(&w, g, 0).is_empty());
}

/// An event stored `On` is started by `EventManager.Initialize` (names ignore case): its generator
/// spawns on its first update.
#[test]
fn an_event_that_is_on_at_start_up_spawns_at_once() {
    use empyrean_entity::enums::GameEventState;
    use empyrean_world::managers::event_manager as em;

    let (mut w, g) = event_generator_world("EV", "Ev", GameEventState::On);
    assert!(em::is_event_available(&w, "eV"));
    update(&mut w, g);
    assert!(!obj(&w, g).generator_disabled());
    assert_eq!(spawned(&w, g, 0).len(), 1);
}
