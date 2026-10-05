//! Divergence: V442, the status ping's live facts.
//! Each pass of the world loop tells the status ping whether the world is open or shutting down,
//! and how many are on, before it reads the pass's datagrams.
//! Fixture: a virtual-time TestServer on synthetic dats.

use dereth_primitives::era::EraFeatures;
use empyrean_net::status_ping::{StatusFacts, StatusPing, StatusPingConfig, WorldState};
use empyrean_testkit::TestServer;
use empyrean_world::managers::world_manager::{self, WorldStatusState};

fn facts(ts: &TestServer) -> StatusFacts {
    ts.world
        .net
        .status_ping
        .as_ref()
        .expect("set up")
        .facts
        .clone()
}

#[test]
fn the_world_loop_keeps_the_status_pings_state_and_players_current() {
    let mut ts = TestServer::new();
    ts.world.net.status_ping = Some(StatusPing::new(
        StatusPingConfig::default(),
        StatusFacts {
            features: EraFeatures::END_OF_RETAIL,
            ..StatusFacts::default()
        },
        Box::new(|| [1; 32]),
    ));
    ts.world.world_manager.world_status = WorldStatusState::Closed;
    ts.step();
    assert_eq!(facts(&ts).state, WorldState::Starting, "not open yet");
    world_manager::open(&mut ts.world, None);
    ts.step();
    let f = facts(&ts);
    assert_eq!((f.state, f.players), (WorldState::Open, 0));
    ts.world.server_manager.shutdown_initiated = true;
    ts.step();
    assert_eq!(facts(&ts).state, WorldState::ShuttingDown);
}
