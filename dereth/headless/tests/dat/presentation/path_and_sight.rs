//! A walking route and lines of sight on the retail data, in a session the player entered: in
//! Holtburg, the client's planner answers a route into a building through its closed door (the
//! first route asked for, so the doors the world holds are planned with from the first), and a
//! route round a building to a place behind it; the line of sight there is blocked by the building
//! while the one up the open street is clear; and the character walked along the route's corners
//! stands there, in sight of it, at the end. The desktop client runs the same runtime and the same
//! plugin host, so these are its routes and sights too.
//! Fixture: the shipped retail DAT records, first-login-walk-jump's recorded login into the world
//! (fed on to the moment the server shows the character), and a plugin written for the test.

use dereth_headless::run::Options;
use dereth_headless::{parse, run};

/// The plugin: on `/route`, it asks for the route into a building, looks along two lines of sight,
/// asks for the route to the place behind the building, walks its corners one move at a time, and
/// says where it ended.
const MAIN: &str = r#"
local function metres(a, b)
	return math.sqrt(((a.ns - b.ns) * 240) ^ 2 + ((a.ew - b.ew) * 240) ^ 2)
end
-- Twelve metres west of the character, in a building through its door; twenty-five south-west,
-- behind a building; twenty north, up the street.
local INSIDE = { ns = 42.255, ew = 33.778, z = 94 }
local BEHIND = { ns = 42.2029, ew = 33.7378, z = 94 }
local OPEN = { ns = 42.2550 + 20 / 240, ew = 33.828, z = 94 }
commands.add("route", "walk a route behind a building", function()
	task.spawn(function()
		local inside = await(nav.path(INSIDE))
		local door = inside.doors[1] and game.world:get(inside.doors[1].object)
		print("door", #inside.doors, door and door.class)
		print("sight", nav.sight(BEHIND), nav.sight(OPEN))
		local r = await(nav.path(BEHIND))
		local p = game.player.position
		print("route", #r.points > 2, r.length > metres(p, BEHIND) + 1, r.complete)
		for i = 2, #r.points do
			local ok, why = pawait(nav.move_to(r.points[i], 1))
			if not ok then
				print("leg", i, why)
				break
			end
		end
		local q = game.player.position
		print("arrived", metres(q, BEHIND) < 2, nav.sight(BEHIND))
	end)
end)
"#;

/// Behaviour: plugins.nav.a-walking-route-on-the-retail-data-goes-round-a-building-and-a-line-of-sight-stops-at-it
/// In a replayed login at Holtburg: the first route asked for, into a building, names its closed
/// door; the line of sight to a place behind a building is blocked and the one up the open street
/// is clear; the route there bends round the building, longer than the straight line; walked
/// corner by corner, it brings the character there, in sight of it.
///
/// Falsified by a planner that plans its first route before it knows the world's doors (the door
/// is missing, and a body walking it stops at the door), a host that answers sight from nothing,
/// or a route that runs straight through the building (the walk ends stuck at its wall).
#[test]
fn a_route_round_a_building_brings_the_character_behind_it_and_sight_stops_at_its_walls() {
    dereth_client_sdk::dat::testing::require_dats();
    let dir = std::env::temp_dir().join(format!("dere-path-and-sight-{}", std::process::id()));
    let package = dir.join("route");
    std::fs::create_dir_all(&package).expect("a temporary folder");
    std::fs::write(
        package.join("plugin.toml"),
        "id = \"test.route\"\nname = \"Route\"\nversion = \"0.1.0\"\napi = \"0.7\"\n\n\
         [permissions]\n\"act.interactive\" = \"Walks a route when you type.\"\n\n\
         [capabilities]\nrequired = [\"nav.path\", \"nav.moveto\"]\n",
    )
    .expect("the manifest");
    std::fs::write(package.join("main.lua"), MAIN).expect("the plugin");
    let opts = Options {
        dat_dir: dereth_client_sdk::dat::testing::dat_dir(),
        world: true,
        plugins: vec![package],
        ..Options::default()
    };
    let commands = parse(
        "login first-login-walk-jump +Aldis\n\
         replay 9\n\
         tick 30\n\
         say /route\n\
         tick 1200\n\
         plugin console test.route\n\
         quit\n",
    )
    .expect("the script parses");
    let mut out: Vec<u8> = Vec::new();
    let result = run(&commands, &opts, &mut out);
    let _ = std::fs::remove_dir_all(&dir);
    result.expect("the run finishes");
    let text = String::from_utf8(out).expect("text");
    let console: Vec<&str> = text
        .lines()
        .filter_map(|l| l.strip_prefix("console test.route: "))
        .collect();
    assert_eq!(
        console,
        [
            "door\t1\tDoor",
            "sight\tfalse\ttrue",
            "route\ttrue\ttrue\ttrue",
            "arrived\ttrue\ttrue"
        ],
        "{text}"
    );
}
