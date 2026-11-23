//! Routes run smoothly: `route:walk` and `nav.move_along` over the host, what a movement that
//! arrived leaves held for the next, and the Tank's route. Each runs a small plugin, or the Tank
//! sample, in a scripted world, frame by frame.

use dereth_testkit::behaviours::note_asserted;
use dereth_testkit::plugins::{read_package, PluginRun, ScriptedObject, FRAME_RATE};

dereth_testkit::scenarios! {
    scenario_a_route_runs_its_plain_points_as_one_movement => a_route_runs_its_plain_points_as_one_movement ["plugins.nav.a-route-runs-its-plain-points-as-one-movement"],
    scenario_a_route_stops_where_it_has_something_to_do => a_route_stops_where_it_has_something_to_do ["plugins.nav.a-route-stops-where-it-has-something-to-do"],
    scenario_what_an_arrival_holds_is_let_go_of => what_an_arrival_holds_is_let_go_of ["plugins.nav.what-an-arrival-holds-is-let-go-of"],
    scenario_chained_moves_run_on_past_a_target_they_pass_within_range => chained_moves_run_on_past_a_target_they_pass_within_range ["plugins.nav.a-move-chained-on-arrival-keeps-the-run-lock"],
    scenario_a_path_move_opens_a_shut_door_across_its_way_and_runs_on => a_path_move_opens_a_shut_door_across_its_way_and_runs_on ["plugins.nav.a-path-move-opens-a-shut-door-on-its-way"],
    scenario_a_path_move_a_locked_door_stops_says_which_door_and_why => a_path_move_a_locked_door_stops_says_which_door_and_why ["plugins.nav.a-path-move-opens-a-shut-door-on-its-way"],
    scenario_the_tank_runs_a_bendy_stretch_of_its_route_in_one_move => the_tank_runs_a_bendy_stretch_of_its_route_in_one_move ["plugins.samples.the-tank-runs-through-its-routes-plain-waypoints"],
    scenario_the_tank_runs_through_its_routes_plain_waypoints => the_tank_runs_through_its_routes_plain_waypoints ["plugins.samples.the-tank-runs-through-its-routes-plain-waypoints"],
}

const ID: &str = "test.routes";

/// A scripted world with a plugin of `main` in it, the player about to type.
fn plugin(main: &str) -> PluginRun {
    let mut run = PluginRun::new();
    run.load(dereth_plugin_host::Package {
        origin: "routes".into(),
        files: [
            (
                "plugin.toml".to_owned(),
                format!("id = \"{ID}\"\nname = \"Routes\"\nversion = \"1.0.0\"\napi = \"0.7\"\n\n[permissions]\n\"act.interactive\" = \"Walks where it is told.\"\n\n[capabilities]\nrequired = [\"nav.moveto\"]\n")
                    .into_bytes(),
            ),
            ("main.lua".to_owned(), main.as_bytes().to_vec()),
        ]
        .into(),
    });
    run.scripted_world(
        "Tester",
        vec![ScriptedObject {
            id: CHEST,
            name: "Chest".into(),
            obj_type: dereth_rules::weenie::item_type::CONTAINER,
            place: Some((42.2, 33.8136)),
            ..ScriptedObject::default()
        }],
    );
    run.advance(0.5);
    run
}

const CHEST: u32 = 0x8000_0090;

/// One frame at a time until `done` says, for at most `secs`: how many frames it took.
fn frames_until(run: &mut PluginRun, secs: u32, done: impl Fn(&PluginRun) -> bool) -> u32 {
    for n in 0..secs * 30 {
        if done(run) {
            return n;
        }
        run.advance(1.0 / FRAME_RATE);
    }
    panic!(
        "not within {secs} s: {:?}; {:?}",
        run.console(ID),
        run.moves()
    );
}

fn said(run: &PluginRun, what: &str) -> bool {
    run.console(ID).iter().any(|l| l == what)
}

fn count(run: &PluginRun, what: &str) -> usize {
    run.moves().iter().filter(|m| *m == what).count()
}

/// A circular route of twenty points is run through, round and past its wrap, with the forward
/// key pressed once, no stop, no run lock, and each point told in order.
fn a_route_runs_its_plain_points_as_one_movement() {
    let mut run = plugin(
        r#"
local navlib = require("dereth.nav")
commands.add("walk", "walk a circle", function()
	local me, places = game.player.position, {}
	local r = 18
	for i = 1, 20 do
		local a = (i - 1) * 2 * math.pi / 20
		places[i] = { ns = me.ns + r * math.sin(a) / 240, ew = me.ew + (r - r * math.cos(a)) / 240 }
	end
	local route = navlib.route(places, "circular")
	route.at = 2
	task.spawn(function()
		route:walk({ range = 1, straight = true, on_point = function(i) print("reached", i) end })
	end)
end)
"#,
    );
    run.type_line("/walk");
    frames_until(&mut run, 120, |r| r.console(ID).len() >= 45);
    let expected: Vec<String> = (2..=20)
        .chain(1..=20)
        .chain(1..=6)
        .map(|i| format!("reached\t{i}"))
        .collect();
    assert_eq!(run.console(ID)[..45], expected, "each point, in order");
    assert_eq!(
        run.moves(),
        ["hold forward"],
        "run through every bend and the wrap, never by the run lock"
    );
    note_asserted("plugins.nav.a-route-runs-its-plain-points-as-one-movement");
}

/// A route stands for its pause and for a point's `on_point` that waits, raises an `on_point`'s
/// error, and stops where a plugin stops it.
fn a_route_stops_where_it_has_something_to_do() {
    let mut run = plugin(
        r#"
local navlib = require("dereth.nav")
local function north(n) return { ns = 42.2 + n * 0.025, ew = 33.8 } end
local function route()
	return setmetatable({ kind = "once", points = {
		{ kind = "point", position = north(1) },
		{ kind = "pause", ms = 2000 },
		{ kind = "point", position = north(2) },
		{ kind = "point", position = north(3) },
		{ kind = "point", position = north(4) },
	} }, getmetatable(navlib.route({})))
end
commands.add("walk", "walk north", function()
	task.spawn(function()
		route():walk({ range = 1, on_point = function(i)
			print("reached", i)
			if i == 3 then
				task.sleep(2000)
				print("waited")
			end
		end })
		print("done")
	end)
end)
commands.add("fail", "walk south into an error", function()
	task.spawn(function()
		local back = navlib.route({ north(3), north(2), north(1) })
		print("failed", (pcall(back.walk, back, { range = 1, on_point = function(i)
			if i == 1 then error("no further") end
		end })))
	end)
end)
"#,
    );
    run.type_line("/walk");
    frames_until(&mut run, 20, |r| said(r, "reached\t1"));
    // The pause: standing within a few frames, and all through it.
    run.advance(0.3);
    let paused = run.player_place();
    run.advance(1.5);
    assert_eq!(run.player_place(), paused, "it stands through the pause");
    assert_eq!(count(&run, "hold forward"), 1);
    frames_until(&mut run, 20, |r| said(r, "reached\t3"));
    run.advance(0.3);
    let waiting = run.player_place();
    run.advance(1.5);
    assert_eq!(run.player_place(), waiting, "it waits for on_point");
    frames_until(&mut run, 30, |r| said(r, "done"));
    run.advance(0.2);
    assert_eq!(
        run.console(ID),
        [
            "reached\t1",
            "reached\t2",
            "reached\t3",
            "waited",
            "reached\t4",
            "reached\t5",
            "done"
        ]
    );
    assert_eq!(count(&run, "hold forward"), 3, "{:?}", run.moves());
    assert_eq!(count(&run, "let go forward"), 3, "{:?}", run.moves());

    run.type_line("/fail");
    frames_until(&mut run, 30, |r| said(r, "failed\tfalse"));
    run.advance(0.3);
    let stopped = run.player_place();
    run.advance(1.0);
    assert_eq!(run.player_place(), stopped, "an error in on_point stops it");
    assert!(!run.moves().iter().any(|m| m.starts_with("autorun")));
    note_asserted("plugins.nav.a-route-stops-where-it-has-something-to-do");
}

/// What an arrival leaves held is let go of when no movement follows, before a use that does, at
/// a stop, and when the plugin is unloaded.
fn what_an_arrival_holds_is_let_go_of() {
    const EAST: &str = r#"
local function east() return { ns = 42.2, ew = game.player.position.ew + 0.0125 } end
commands.add("to", "a move", function()
	task.spawn(function() await(nav.move_to(east(), 1)) print("arrived") end)
end)
commands.add("along", "a path", function()
	task.spawn(function() await(nav.move_along({ east() }, { range = 1 })) print("arrived") end)
end)
commands.add("use", "a move, then a use", function()
	task.spawn(function()
		await(nav.move_to(east(), 1))
		print("arrived")
		act.use(0x80000090)
	end)
end)
commands.add("stop", "a path, then a stop", function()
	task.spawn(function()
		await(nav.move_along({ east() }, { range = 1 }))
		print("arrived")
		nav.stop()
	end)
end)
"#;
    for (command, off) in [("/to", "autorun off"), ("/along", "let go forward")] {
        let mut run = plugin(EAST);
        run.type_line(command);
        frames_until(&mut run, 10, |r| said(r, "arrived"));
        assert_eq!(
            count(&run, off),
            0,
            "{command}: held for a movement to follow"
        );
        let frames = frames_until(&mut run, 2, |r| count(r, off) == 1);
        assert!(frames <= 2, "{command}: let go {frames} frames later");
    }

    let mut run = plugin(EAST);
    run.type_line("/use");
    let used = format!("use {CHEST:#x}");
    frames_until(&mut run, 10, |r| count(r, "autorun off") == 1);
    assert!(said(&run, "arrived"));
    assert!(
        !run.sent().contains(&used),
        "standing first, the use after: {:?}",
        run.sent()
    );
    frames_until(&mut run, 2, |r| r.sent().contains(&used));

    let mut run = plugin(EAST);
    run.type_line("/stop");
    frames_until(&mut run, 10, |r| said(r, "arrived"));
    run.advance(1.0 / FRAME_RATE);
    assert_eq!(
        count(&run, "let go forward"),
        1,
        "stopped: {:?}",
        run.moves()
    );

    let mut run = plugin(EAST);
    run.type_line("/along");
    frames_until(&mut run, 10, |r| said(r, "arrived"));
    run.unload(ID);
    run.advance(1.0 / FRAME_RATE);
    assert_eq!(
        count(&run, "let go forward"),
        1,
        "unloaded: {:?}",
        run.moves()
    );
    note_asserted("plugins.nav.what-an-arrival-holds-is-let-go-of");
}

/// Moves chained through points a little off a straight line, each arriving within a metre:
/// close to each, the character runs on past it rather than stopping to turn for it, so the run
/// lock is set once and let go once.
fn chained_moves_run_on_past_a_target_they_pass_within_range() {
    let mut run = plugin(
        r#"
commands.add("go", "east through five points", function()
	task.spawn(function()
		for i = 1, 5 do
			local off = (i % 2 == 0) and 0.0004 or -0.0004
			await(nav.move_to({ ns = 42.2 + off, ew = 33.8 + i * 0.03 }, 1))
			print("arrived", i)
		end
	end)
end)
"#,
    );
    run.type_line("/go");
    frames_until(&mut run, 30, |r| said(r, "arrived\t5"));
    run.advance(0.3);
    assert_eq!(
        run.moves()
            .iter()
            .filter(|m| m.starts_with("autorun"))
            .count(),
        2,
        "{:?}",
        run.moves()
    );
    note_asserted("plugins.nav.a-move-chained-on-arrival-keeps-the-run-lock");
}

const DOOR: u32 = 0x8000_0095;

/// A world with a door six metres north of the character, in the gap of a wall running east and
/// west, and a plugin that runs north through it and on to the north-east.
fn through_a_door(locked: bool) -> PluginRun {
    let mut run = plugin(
        r#"
commands.add("go", "north through the door", function()
	local me = game.player.position
	local function at(east, north) return { ns = me.ns + north / 240, ew = me.ew + east / 240 } end
	task.spawn(function()
		local p = nav.move_along({ at(0, 12), at(6, 18) }, { range = 1 })
		local ok, r = pawait(p)
		print("through", ok, ok and r.points or r, p.door and p.door.why or "")
	end)
end)
"#,
    );
    run.spawn(vec![ScriptedObject {
        id: DOOR,
        name: "Door".into(),
        obj_type: dereth_rules::weenie::item_type::MISC,
        place: Some((42.2 + 6.0 / 240.0, 33.8)),
        door: Some(dereth_testkit::plugins::ScriptedDoor { locked }),
        ..ScriptedObject::default()
    }]);
    let ns = 42.2 + 6.0 / 240.0;
    run.wall((ns, 33.8 - 20.0 / 240.0), (ns, 33.8 - 1.0 / 240.0));
    run.wall((ns, 33.8 + 1.0 / 240.0), (ns, 33.8 + 20.0 / 240.0));
    run.advance(0.2);
    run
}

/// A move along whose leg runs through a shut door stops at it, uses it, and once it stands open
/// runs on through it to its last point.
fn a_path_move_opens_a_shut_door_across_its_way_and_runs_on() {
    let mut run = through_a_door(false);
    run.type_line("/go");
    frames_until(&mut run, 30, |r| {
        r.console(ID).iter().any(|l| l.starts_with("through"))
    });
    assert_eq!(run.console(ID), ["through\ttrue\t2\t"], "{:?}", run.moves());
    let used = format!("use {DOOR:#x}");
    assert_eq!(
        run.sent().iter().filter(|s| **s == used).count(),
        1,
        "{:?}",
        run.sent()
    );
    let (ns, _, _) = run.player_place().expect("outdoors");
    assert!(ns > 42.2 + 15.0 / 240.0, "through the door and on: {ns}");
    assert_eq!(
        count(&run, "hold forward"),
        2,
        "stood at the door: {:?}",
        run.moves()
    );
    note_asserted("plugins.nav.a-path-move-opens-a-shut-door-on-its-way");
}

/// A move along a locked door stops fails at once, saying which door and that it is locked.
fn a_path_move_a_locked_door_stops_says_which_door_and_why() {
    let mut run = through_a_door(true);
    run.type_line("/go");
    frames_until(&mut run, 10, |r| {
        r.console(ID).iter().any(|l| l.starts_with("through"))
    });
    assert_eq!(
        run.console(ID),
        ["through\tfalse\tdoor_shut: Door is locked, the server says\tlocked"]
    );
    run.advance(0.2);
    assert_eq!(count(&run, "let go forward"), count(&run, "hold forward"));
    note_asserted("plugins.nav.a-path-move-opens-a-shut-door-on-its-way");
}

/// The Tank runs a bendy stretch of plain waypoints as one movement, without letting the forward
/// key go.
fn the_tank_runs_a_bendy_stretch_of_its_route_in_one_move() {
    use dereth_plugin_host::PluginFiles;
    use std::fmt::Write as _;
    const TANK: &str = "network.dereth.tank";
    let files = super::SharedFiles::default();
    let mut run = PluginRun::new().with_files(Box::new(files.clone()));
    run.host().set_world_policy(
        dereth_plugin_host::policy_doc::parse(
            "policy_version = 1\n[automation]\ndefault = \"allow\"\n",
        )
        .expect("a policy"),
    );
    run.load(read_package(
        &dereth_testkit::conformance::workspace_root().join("dereth/plugins/tank"),
    ));
    run.scripted_world("Tester", Vec::new());
    // Eight waypoints six metres apart, bending 30 degrees one way and the other.
    let mut route = String::from("uTank2 NAV 1.2\n4\n8\n");
    let (mut north, mut east, mut heading) = (1.0_f64, 0.0_f64, 0.0_f64);
    for i in 0..8 {
        let _ = write!(
            route,
            "0\n{}\n{}\n0\n0\n",
            33.8 + east / 240.0,
            42.2 + north / 240.0
        );
        heading += if i % 2 == 0 { 30.0 } else { -30.0 };
        north += 6.0 * dereth_primitives::num::math::cos(heading.to_radians());
        east += 6.0 * dereth_primitives::num::math::sin(heading.to_radians());
    }
    files
        .clone()
        .write(TANK, "routes/bendy.nav", route.as_bytes())
        .expect("the route saved");
    run.type_line("/tank route bendy.nav");
    run.type_line("/tank start");
    for _ in 0..80 {
        if run
            .console(TANK)
            .iter()
            .any(|l| l.contains("route: finished"))
        {
            break;
        }
        run.advance(0.5);
    }
    let console = run.console(TANK);
    assert!(
        console
            .iter()
            .any(|l| l.contains("route: running through waypoints 1 to 8")),
        "{console:#?}"
    );
    assert!(
        console.iter().any(|l| l.contains("route: finished")),
        "{console:#?}"
    );
    run.advance(1.0);
    assert_eq!(count(&run, "hold forward"), 1, "{:?}", run.moves());
    assert_eq!(count(&run, "let go forward"), 1, "{:?}", run.moves());
    note_asserted("plugins.samples.the-tank-runs-through-its-routes-plain-waypoints");
}

/// The Tank runs through its route's plain waypoints without letting the forward key go between
/// them, and never by the run lock.
fn the_tank_runs_through_its_routes_plain_waypoints() {
    use dereth_plugin_host::PluginFiles;
    use std::fmt::Write as _;
    const TANK: &str = "network.dereth.tank";
    let files = super::SharedFiles::default();
    let mut run = PluginRun::new().with_files(Box::new(files.clone()));
    run.host().set_world_policy(
        dereth_plugin_host::policy_doc::parse(
            "policy_version = 1\n[automation]\ndefault = \"allow\"\n",
        )
        .expect("a policy"),
    );
    run.load(read_package(
        &dereth_testkit::conformance::workspace_root().join("dereth/plugins/tank"),
    ));
    run.scripted_world("Tester", Vec::new());
    // Eight waypoints north, bending a little.
    let mut route = String::from("uTank2 NAV 1.2\n4\n8\n");
    for i in 1..=8 {
        let n = f64::from(i);
        let (ns, ew) = (42.2 + n * 0.025, 33.8 + n * n * 0.0005);
        let _ = write!(route, "0\n{ew}\n{ns}\n0\n0\n");
    }
    files
        .clone()
        .write(TANK, "routes/smooth.nav", route.as_bytes())
        .expect("the route saved");
    run.type_line("/tank route smooth.nav");
    run.type_line("/tank start");
    for _ in 0..80 {
        if run
            .console(TANK)
            .iter()
            .any(|l| l.contains("route: finished"))
        {
            break;
        }
        run.advance(0.5);
    }
    assert!(
        run.console(TANK)
            .iter()
            .any(|l| l.contains("route: finished")),
        "{:#?}",
        run.console(TANK)
    );
    run.advance(1.0);
    assert_eq!(count(&run, "hold forward"), 1, "{:?}", run.moves());
    assert_eq!(count(&run, "let go forward"), 1, "{:?}", run.moves());
    assert!(!run.moves().iter().any(|m| m.starts_with("autorun")));
    note_asserted("plugins.samples.the-tank-runs-through-its-routes-plain-waypoints");
}
