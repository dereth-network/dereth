//! ACE: Source/ACE.Server/Entity/Landblock.cs::TickSingleThreadedWork
//! One object whose heartbeat panics is caught alone: its heartbeat goes on, and the landblock
//! pass is not unwound.
//! Fixture: a virtual-time TestServer, isolated stores and synthetic dats.

use std::sync::Arc;

use empyrean_content::models::world::Weenie;
use empyrean_content::MemContent;
use empyrean_dat::FakeDats;
use empyrean_entity::enums::{
    EmoteCategory, PropertyDataId, PropertyFloat, PropertyString, WeenieType,
};
use empyrean_entity::models::properties_emote::PropertiesEmote;
use empyrean_entity::{ObjectGuid, Position};
use empyrean_testkit::land::{self, TEST_SETUP};
use empyrean_testkit::{EmptyShard, TestServer};
use empyrean_world::managers::guid_manager;
use empyrean_world::managers::landblock_manager;
use empyrean_world::world_objects::world_object::CtorEnv;

const LB: u32 = 0xA9B4_0000;
const DRUDGE: u32 = 7;

fn server() -> TestServer {
    let content = MemContent::new().weenie(
        Weenie::new(DRUDGE, "drudge", WeenieType::Creature)
            .with_string(PropertyString::Name, "Drudge")
            .with_did(PropertyDataId::Setup, TEST_SETUP),
    );
    let mut ts = TestServer::with_setup(
        empyrean_testkit::dats::with_stat_tables(FakeDats::new())
            .build()
            .expect("fake dats"),
        |w| {
            w.content = Arc::new(content);
            guid_manager::initialize(w, &mut EmptyShard);
        },
    );
    land::use_flat_land_with_test_setup(&mut ts.world, &[0xA9B4], 0);
    ts
}

/// A drudge standing at `x`, 20 with the emote table `sets`.
fn drudge(ts: &mut TestServer, x: f32, sets: Vec<PropertiesEmote>) -> ObjectGuid {
    let w = &mut ts.world;
    let weenie = w.content.get_cached_weenie(DRUDGE).expect("test weenie");
    let g = guid_manager::new_dynamic_guid(w);
    let mut o = CtorEnv::with_world(w, |env| {
        empyrean_world::factories::world_object_factory::create_world_object(env, Some(weenie), g)
    })
    .expect("constructible");
    o.set_location(Some(Position::from_components(
        LB | 0x0001,
        x,
        20.0,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
        false,
    )));
    o.biota.properties_emote = Some(Arc::new(sets));
    w.objects.insert(o).expect("fresh");
    assert!(landblock_manager::add_object(w, g, false));
    g
}

fn heartbeat_at(ts: &TestServer, g: ObjectGuid) -> Option<f64> {
    ts.world
        .objects
        .get(g)
        .expect("live object")
        .get_property(PropertyFloat::HeartbeatTimestamp)
}

/// A heartbeat emote set with no actions panics in the drudge's first idle heartbeat (an inherited
/// fault that leaves it busy, so later heartbeats skip the emote). The panic is caught around that
/// heartbeat alone: the drudge is put back on the heartbeat list and its later heartbeats run, the
/// other drudge's heartbeats run throughout, and the loop's stage guard never sees the panic.
#[test]
pub(crate) fn a_panicking_heartbeat_is_caught_alone_and_its_heartbeat_goes_on() {
    let mut ts = server();
    let faulty = drudge(
        &mut ts,
        20.0,
        vec![PropertiesEmote {
            category: EmoteCategory::HeartBeat,
            probability: 1.0,
            ..PropertiesEmote::default()
        }],
    );
    let healthy = drudge(&mut ts, 30.0, Vec::new());
    let _ = TestServer::take_not_ported();

    ts.advance(30.0);

    assert_eq!(ts.world.world_manager.object_tick_exceptions, 1);
    assert_eq!(
        ts.world.world_manager.stage_exceptions, 0,
        "the landblock pass was not unwound"
    );
    let now = ts.world.now.unix_time;
    let faulty_at = heartbeat_at(&ts, faulty).expect("the faulty drudge heartbeat again");
    let healthy_at = heartbeat_at(&ts, healthy).expect("the healthy drudge heartbeat");
    assert!(now - faulty_at < 15.0, "{faulty_at} at {now}");
    assert!(now - healthy_at < 15.0, "{healthy_at} at {now}");
}
