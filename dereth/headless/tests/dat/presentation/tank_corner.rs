//! The Tank's route leg round a corner on the retail data, in a session recorded in the Training
//! Academy: from the room a new character starts in, a waypoint east and then north round the
//! corner of the passage out, out of a straight line of sight, is walked to along the client's own
//! route and reached never stuck. The desktop client runs the same runtime and the same plugin
//! host, so this is its walk too.
//! Fixture: the shipped retail DAT records, long-solo-play's recorded login into the Academy, and
//! the Tank package as it ships.

use dereth_headless::run::Options;
use dereth_headless::{parse, run};

/// Behaviour: plugins.samples.the-tank-walks-a-route-leg-round-a-retail-dungeon-corner
/// A route of two waypoints in the Academy: where the character starts, and 9 m off round the
/// corner of the passage out of its room, 13 m along the client's route. The Tank walks the leg
/// along that route and stands at the second waypoint, its route finished, never stuck.
///
/// Falsified by a leg walked straight at the waypoint (into the room's wall, stuck there first),
/// or a route that does not bend round the corner.
#[test]
fn the_tank_walks_a_route_leg_round_an_academy_corner_along_the_clients_route() {
    dereth_client_sdk::dat::testing::require_dats();
    let tank = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../plugins/tank");
    let opts = Options {
        dat_dir: dereth_client_sdk::dat::testing::dat_dir(),
        world: true,
        plugins: vec![tank],
        plugin_policy: Some(String::from(
            "policy_version = 1\n[automation]\ndefault = \"allow\"\n",
        )),
        // The Tank's own work decides what happens, not how busy the machine is.
        plugin_call_budget_ms: Some(10_000.0),
        ..Options::default()
    };
    // The recording is fed past the moment its player opened the door out of the room (the body
    // stays where it entered: only the player moves it). The Academy's first room is in landblock
    // 0x7F03: a place there is named by map coordinates offset from the landblock's corner, as a
    // route made in a dungeon names it. The first waypoint is where the character stands,
    // (12.3, -28.5); the second (16.5, -20.5), in the passage beyond the door.
    let commands = parse(
        "login long-solo-play +Brann\n\
         replay 40\n\
         tick 30\n\
         say /tank set EnableCombat false\n\
         say /tank set EnableBuffing false\n\
         say /tank set EnableLooting false\n\
         say /tank route new once\n\
         say /tank route add 99.618675S 0.248667W\n\
         say /tank route add 99.585417S 0.231250W\n\
         say /tank start\n\
         tick 2400\n\
         position\n\
         plugin console network.dereth.tank\n\
         quit\n",
    )
    .expect("the script parses");
    let mut out: Vec<u8> = Vec::new();
    run(&commands, &opts, &mut out).expect("the run finishes");
    let text = String::from_utf8(out).expect("text");
    let console: Vec<&str> = text
        .lines()
        .filter_map(|l| l.strip_prefix("console network.dereth.tank: "))
        .collect();
    let said = |s: &str| console.iter().any(|l| l.contains(s));
    assert!(
        said("waypoint 2 is out of sight; walking the client's route to it"),
        "{text}"
    );
    assert!(said("route: finished"), "{text}");
    assert!(
        !said("stuck:") && !said("travelling round") && !said("skipping waypoint"),
        "never stuck: {text}"
    );
    assert!(
        said("Idle: 0 kills, 0 deaths, 0 corpses recovered, 0 corpses looted, 0 items kept, 0 stuck recoveries"),
        "{text}"
    );
    let at = text
        .lines()
        .find_map(|l| l.strip_prefix("position cell="))
        .expect("a position");
    let (cell, origin) = at.split_once(" origin=").expect("cell and origin");
    let xyz: Vec<f32> = origin
        .split_whitespace()
        .next()
        .expect("coordinates")
        .split(',')
        .map(|v| v.parse().expect("a number"))
        .collect();
    assert_eq!(cell, "0x7F0301B0", "in the passage: {text}");
    let (dx, dy) = (xyz[0] - 16.5, xyz[1] + 20.5);
    assert!(
        dx * dx + dy * dy < 2.5 * 2.5,
        "at the second waypoint: {at}"
    );
}
