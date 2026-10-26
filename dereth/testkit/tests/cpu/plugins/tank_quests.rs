//! The Tank sample's quest steps, the plugin's own (VTank gives and uses items only through its
//! meta expressions): an item handed to a person, one picked up off the ground, one used on
//! something, each finished before the route that types it goes on; a journey with the macro
//! stopped; the Training Academy profile it ships, read whole for each starting town; and the
//! litmus profile, a fresh character's hunt from Holtburg.

use dereth_rules::weenie::{bitfield, item_type};
use dereth_testkit::behaviours::note_asserted;
use dereth_testkit::plugins::{
    read_package, PluginRun, ScriptedDoor, ScriptedObject, SCRIPTED_PLAYER,
};

use super::samples;
use super::tank::{held, logged, tank, TANK};

dereth_testkit::scenarios! {
    scenario_the_tank_hands_an_item_over_picks_one_up_and_uses_one_on_something_before_its_route_goes_on => the_tank_hands_an_item_over_picks_one_up_and_uses_one_on_something_before_its_route_goes_on ["plugins.samples.the-tank-hands-an-item-over-picks-one-up-and-uses-one-on-something-before-its-route-goes-on"],
    scenario_the_tank_goes_somewhere_by_a_journey_with_the_macro_stopped => the_tank_goes_somewhere_by_a_journey_with_the_macro_stopped ["plugins.samples.the-tank-goes-somewhere-by-a-journey-with-the-macro-stopped"],
    scenario_the_tanks_journey_steps_down_a_ledge_only_a_drop_leads_on_from_when_no_higher_than_a_step_down => the_tanks_journey_steps_down_a_ledge_only_a_drop_leads_on_from_when_no_higher_than_a_step_down ["plugins.samples.the-tanks-journey-steps-down-a-ledge-only-a-drop-leads-on-from-when-no-higher-than-a-step-down"],
    scenario_the_tank_walks_within_reach_of_a_door_before_using_a_key_on_it => the_tank_walks_within_reach_of_a_door_before_using_a_key_on_it ["plugins.samples.the-tank-walks-within-reach-of-what-it-uses-an-item-on"],
    scenario_the_tanks_academy_profile_reads_whole_for_every_starting_town => the_tanks_academy_profile_reads_whole_for_every_starting_town ["plugins.samples.the-tanks-academy-profile-reads-whole-for-every-starting-town"],
    scenario_the_tanks_litmus_profile_sets_up_travels_to_its_dungeon_and_goes_back_when_far => the_tanks_litmus_profile_sets_up_travels_to_its_dungeon_and_goes_back_when_far ["plugins.samples.the-tanks-litmus-profile-sets-up-travels-to-its-dungeon-and-goes-back-when-far"],
}

const MASTER: u32 = 0x8000_00A0;
const TOKEN: u32 = 0x8000_00A1;
const CAP: u32 = 0x8000_00A2;
const KEY: u32 = 0x8000_00A3;
const LIBRARY: u32 = 0x8000_00A4;
const OIL: u32 = 0x8000_00A5;
const DIRK: u32 = 0x8000_00A6;

/// A person, a token in the pack for them, a cap on the ground, a key in the pack and the locked
/// door it opens, an oil in the pack and the training weapon it is used on, wielded, which the oil
/// remakes as the Academy's own.
fn academy_room() -> Vec<ScriptedObject> {
    vec![
        ScriptedObject {
            id: MASTER,
            name: "Training Master".into(),
            obj_type: item_type::CREATURE,
            // A person stands where they are: used, they are not picked up.
            bitfield: bitfield::STUCK,
            place: Some((42.2, 33.8001)),
            ..ScriptedObject::default()
        },
        held(TOKEN, "Academy Token", item_type::MISC),
        ScriptedObject {
            id: CAP,
            name: "Leather Cap".into(),
            obj_type: item_type::ARMOR,
            place: Some((42.2001, 33.8)),
            ..ScriptedObject::default()
        },
        ScriptedObject {
            useability: Some(0x0020_0008),
            target_type: Some(0x280),
            ..held(KEY, "Academy Library Key", item_type::KEY)
        },
        ScriptedObject {
            id: LIBRARY,
            name: "Academy Library".into(),
            obj_type: item_type::MISC,
            place: Some((42.2, 33.7999)),
            door: Some(ScriptedDoor { locked: true }),
            ..ScriptedObject::default()
        },
        ScriptedObject {
            useability: Some(0x000A_0008),
            target_type: Some(item_type::MELEE_WEAPON),
            ..held(OIL, "Oil of Rendering", item_type::MISC)
        },
        ScriptedObject {
            locations: Some(0x0010_0000),
            worn: true,
            remade_as: Some("Academy Dirk".into()),
            ..held(DIRK, "Training Dirk", item_type::MELEE_WEAPON)
        },
    ]
}

/// A route of four chat waypoints typing the quest steps, then a point: the token handed to the
/// Training Master, the cap picked up, the key used on the library's door, the oil on whatever
/// training weapon the character holds (a name ending in `*`). Each is done, in order, before the
/// route goes on; the items go where the steps say, and the weapon the oil remakes, which comes back
/// in the pack, is wielded again. The window's Route page
/// makes the same waypoints from the item named and the selection, and does a step at once.
fn the_tank_hands_an_item_over_picks_one_up_and_uses_one_on_something_before_its_route_goes_on() {
    let (mut run, _) = tank(academy_room());
    for line in [
        "/tank set EnableCombat false",
        "/tank set EnableBuffing false",
        "/tank route new once",
        "/tank route chat /tank give Academy Token to Training Master",
        "/tank route chat /tank pickup Leather Cap",
        "/tank route chat /tank useon Academy Library Key on Academy Library",
        "/tank route chat /tank useon Oil of Rendering on Training *",
        "/tank route add",
        "/tank start",
    ] {
        run.type_line(line);
    }
    run.advance(30.0);
    let console = run.console(TANK);
    let at = |needle: &str| console.iter().position(|l| l.contains(needle));
    let (gave, picked, used, rearmed, oiled, finished) = (
        at("gave Academy Token to Training Master"),
        at("picked up Leather Cap"),
        at("used Academy Library Key on Academy Library"),
        at("wielding Academy Dirk"),
        at("used Oil of Rendering on Training Dirk"),
        at("route: finished"),
    );
    assert!(
        gave.is_some()
            && gave < picked
            && picked < used
            && used < rearmed
            && rearmed < oiled
            && oiled < finished,
        "each step in order, the remade weapon wielded, the route finished last: {console:#?}"
    );
    let sent = run.sent();
    for request in [
        format!("give {TOKEN:#x} {MASTER:#x} 1"),
        format!("put_item_in_container {CAP:#x} {SCRIPTED_PLAYER:#x} 0"),
        format!("use_with_target {KEY:#x} {LIBRARY:#x}"),
        format!("use_with_target {OIL:#x} {DIRK:#x}"),
    ] {
        assert!(sent.contains(&request), "{request}: {sent:?}");
    }
    let names: Vec<String> = run.held_items().into_iter().map(|(_, n, _)| n).collect();
    assert!(
        names.iter().any(|n| n == "Leather Cap") && !names.iter().any(|n| n == "Academy Token"),
        "the cap held, the token gone: {names:?}"
    );

    // The Route page: an item named and a person selected make a give waypoint; the item named
    // alone a pick-up; Give does it at once.
    let (mut run, _) = tank(academy_room());
    run.type_line("/tank route new once");
    run.set(TANK, "main.r_item", "Academy Token".into());
    run.select(MASTER);
    run.click(TANK, "main.r_give");
    run.set(TANK, "main.r_item", "Leather Cap".into());
    run.click(TANK, "main.r_pickup");
    run.type_line("/tank route points");
    assert!(
        run.printed()
            .iter()
            .any(|l| l == "[Tank] Route: once, 2 waypoints: chat, chat"),
        "{:#?}",
        run.printed()
    );
    run.set(TANK, "main.r_item", "Academy Token".into());
    run.select(MASTER);
    run.click(TANK, "main.q_give");
    run.advance(10.0);
    assert!(
        logged(&run, "gave Academy Token to Training Master")
            && run
                .sent()
                .contains(&format!("give {TOKEN:#x} {MASTER:#x} 1")),
        "{:#?}",
        run.console(TANK)
    );
    note_asserted(
        "plugins.samples.the-tank-hands-an-item-over-picks-one-up-and-uses-one-on-something-before-its-route-goes-on",
    );
}

/// A key used on a locked door two and a half metres off: the Tank walks to within a metre and a
/// half of the door first, so the use is made where the character stands and not after a walk of
/// the server's own, which a closed door can stop.
fn the_tank_walks_within_reach_of_a_door_before_using_a_key_on_it() {
    let (ns, ew) = (42.2, 33.8 + 2.5 / 240.0);
    let (mut run, _) = tank(vec![
        ScriptedObject {
            useability: Some(0x0020_0008),
            target_type: Some(0x280),
            ..held(KEY, "Academy Library Key", item_type::KEY)
        },
        ScriptedObject {
            id: LIBRARY,
            name: "Academy Library".into(),
            obj_type: item_type::MISC,
            place: Some((ns, ew)),
            door: Some(ScriptedDoor { locked: true }),
            ..ScriptedObject::default()
        },
    ]);
    for line in [
        "/tank set EnableCombat false",
        "/tank set EnableBuffing false",
        "/tank route new once",
        "/tank route chat /tank useon Academy Library Key on Academy Library",
        "/tank start",
    ] {
        run.type_line(line);
    }
    run.advance(15.0);
    assert!(
        logged(&run, "used Academy Library Key on Academy Library"),
        "{:#?}",
        run.console(TANK)
    );
    let request = format!("use_with_target {KEY:#x} {LIBRARY:#x}");
    assert!(run.sent().contains(&request), "{:?}", run.sent());
    let (pns, pew, _) = run.player_place().expect("placed");
    let (dn, de) = ((pns - ns) * 240.0, (pew - ew) * 240.0);
    assert!(
        dn * dn + de * de <= 1.5 * 1.5,
        "within reach of the door: {dn:.2} m north, {de:.2} m east of it"
    );
    note_asserted("plugins.samples.the-tank-walks-within-reach-of-what-it-uses-an-item-on");
}

/// `/tank go <place>`: the macro running stops, and the character travels there by a journey (here
/// through a portal), the Tank saying it arrived; it does not hunt there. The Run page's Go does
/// the same with the place typed beside Hunt.
fn the_tank_goes_somewhere_by_a_journey_with_the_macro_stopped() {
    use dereth_testkit::scripted_travel::ScriptedPortal;
    let place = |ns: f64, ew: f64| {
        dereth_plugin_host::places::from_coords(ns, ew, 0.0).expect("on the map")
    };
    let mut run = PluginRun::new();
    run.host().set_world_policy(
        dereth_plugin_host::policy_doc::parse(
            "policy_version = 1\n[automation]\ndefault = \"allow\"\n",
        )
        .expect("a policy"),
    );
    run.load(read_package(&samples().join("tank")));
    run.scripted_portals(
        &[ScriptedPortal {
            name: "Exit to Holtburg".into(),
            class: 29338,
            at: place(42.2, 33.85),
            to: place(40.0, 30.0),
            source: dereth_navigation::EdgeSource::Published,
            refuses: false,
        }],
        Vec::new(),
    );
    run.scripted_world(
        "Tester",
        vec![ScriptedObject {
            id: 0x8000_00A8,
            name: "Drudge Skulker".into(),
            obj_type: item_type::CREATURE,
            bitfield: bitfield::ATTACKABLE,
            place: Some((40.0, 30.0002)),
            ..ScriptedObject::default()
        }],
    );
    for line in [
        "/tank set EnableBuffing false",
        "/tank start",
        "/tank go portal Exit to Holtburg",
    ] {
        run.type_line(line);
    }
    run.advance(25.0);
    let console = run.console(TANK);
    let at = |needle: &str| console.iter().position(|l| l.contains(needle));
    let (stopped, planned, arrived) = (
        at("stopped: going to portal Exit to Holtburg"),
        at("journey: walk, portal Exit to Holtburg"),
        at("arrived at portal Exit to Holtburg"),
    );
    assert!(
        stopped.is_some() && stopped < planned && planned < arrived,
        "stopped, then the journey, then there: {console:#?}"
    );
    assert!(
        at("target Drudge Skulker").is_none(),
        "no hunting where it went: {console:#?}"
    );
    let (ns, ew, _) = run.player_place().expect("a place");
    assert!(
        (ns - 40.0).abs() < 0.01 && (ew - 30.0).abs() < 0.01,
        "at {ns}, {ew}"
    );
    note_asserted("plugins.samples.the-tank-goes-somewhere-by-a-journey-with-the-macro-stopped");
}

/// The Training Academy profile the Tank ships: a meta for each starting town's Academy, its
/// routes embedded, each read whole (its twenty-one states, nothing it holds left uncarried), and
/// the loot profile that keeps the quest's items.
fn the_tanks_academy_profile_reads_whole_for_every_starting_town() {
    let (mut run, _) = tank(Vec::new());
    for town in ["holtburg", "shoushi", "yaraq", "sanamar"] {
        run.type_line(&format!("/tank meta academy-{town}.met"));
        run.advance(1.0);
        let console = run.console(TANK);
        let read = format!("meta academy-{town}.met: 21 states");
        assert!(
            console.iter().any(|l| l.ends_with(&read)),
            "{read}: {console:#?}"
        );
    }
    assert!(
        !logged(&run, "not carried") && !logged(&run, "does not read"),
        "{:#?}",
        run.console(TANK)
    );
    run.type_line("/tank profile academy.utl");
    run.advance(1.0);
    assert!(
        logged(&run, "loot profile academy.utl: 3 rules"),
        "{:#?}",
        run.console(TANK)
    );
    note_asserted("plugins.samples.the-tanks-academy-profile-reads-whole-for-every-starting-town");
}

/// The litmus profile: a VTank meta for a fresh character in Holtburg. Run, it reads whole (its six
/// states), loads its loot profile, sets the macro up (a full pack goes to the vendor) and its
/// vendor, travels by a journey through the Holtburg Redoubt portal into the dungeon, and walks its
/// route there. Dead, with nothing to go back for (as the game says), and back to life in town,
/// the macro travels to its hunting place again by itself.
fn the_tanks_litmus_profile_sets_up_travels_to_its_dungeon_and_goes_back_when_far() {
    use dereth_plugin_api::Value;
    use dereth_testkit::scripted_travel::ScriptedPortal;
    const BROKER: u32 = 0x8000_00B8;
    let outdoors = |ns: f64, ew: f64| {
        dereth_plugin_host::places::from_coords(ns, ew, 0.0).expect("on the map")
    };
    // The Redoubt's entrance, where its portal sends: block 0x0163, (140, -40, 0).
    let entrance = dereth_plugin_host::places::from_value(
        &Value::map()
            .with("cell", 0x0163_01AC_u32)
            .with("x", 140.0)
            .with("y", -40.0)
            .with("z", 0.0),
    )
    .expect("a cell place");
    let (mut run, _) = tank(vec![ScriptedObject {
        id: BROKER,
        name: "Contract Broker".into(),
        obj_type: item_type::CREATURE,
        bitfield: bitfield::VENDOR,
        place: Some((42.2, 33.8002)),
        vendor: Some(dereth_testkit::plugins::ScriptedVendor {
            buys: item_type::MELEE_WEAPON,
        }),
        ..ScriptedObject::default()
    }]);
    run.scripted_portals(
        &[ScriptedPortal {
            name: "Holtburg Redoubt".into(),
            class: 4935,
            at: outdoors(42.2, 33.85),
            to: entrance,
            source: dereth_navigation::EdgeSource::Published,
            refuses: false,
        }],
        Vec::new(),
    );
    run.type_line("/tank meta litmus-holtburg.met");
    run.advance(1.0);
    run.type_line("/tank start");
    let mut t = 0.0;
    while t < 90.0 && !logged(&run, "(1 portals, 0 recalls)") {
        run.advance(0.5);
        t += 0.5;
    }
    let console = run.console(TANK);
    let at = |needle: &str| {
        console
            .iter()
            .position(|l| l.contains(needle))
            .unwrap_or_else(|| panic!("{needle:?} not in {console:#?}"))
    };
    at("meta litmus-holtburg.met: 6 states");
    at("loot profile litmus.utl: 8 rules");
    at("meta: onfullpack = vendor");
    at("meta: recovercorpse = true");
    let planned = at("journey: walk, portal Holtburg Redoubt");
    assert!(at("travelling to portal Holtburg Redoubt") < planned);
    assert!(planned < at("(1 portals, 0 recalls)"));
    at("route from the meta: linear, 15 waypoints");
    assert!(
        !logged(&run, "not carried") && !logged(&run, "does not read"),
        "{console:#?}"
    );
    let vendor = store_vendor(&mut run);
    assert_eq!(vendor.as_deref(), Some("Contract Broker"));
    // The pyreals the sales bring are bought as trade notes there.
    run.type_line("/tank buy");
    assert!(
        run.printed()
            .iter()
            .any(|l| l.contains("buy Trade Note (250,000) up to 250 (0 held)")),
        "{:#?}",
        run.printed()
    );
    // A death that left nothing to go back for, and back to life in town: to the hunting place
    // again.
    let before = run
        .console(TANK)
        .iter()
        .filter(|l| l.contains("travelling to portal Holtburg Redoubt"))
        .count();
    run.die();
    run.server_says(&[dereth_testkit::plugins::ui_event(
        &dereth_protocol::comms::CommunicationTextboxString {
            text: "You have retained all your items. You do not need to recover your corpse!"
                .into(),
            text_type: 0,
        },
    )]);
    run.teleport(42.2, 33.8);
    let mut t = 0.0;
    while t < 60.0
        && run
            .console(TANK)
            .iter()
            .filter(|l| l.contains("travelling to portal Holtburg Redoubt"))
            .count()
            == before
    {
        run.advance(0.5);
        t += 0.5;
    }
    assert!(
        run.console(TANK)
            .iter()
            .filter(|l| l.contains("travelling to portal Holtburg Redoubt"))
            .count()
            > before,
        "{:#?}",
        run.console(TANK)
    );
    note_asserted("plugins.samples.the-tanks-litmus-profile-sets-up-travels-to-its-dungeon-and-goes-back-when-far");
}

/// The character in a pen whose walls stand 10 m off on three sides and whose fourth, north, is a
/// ledge `height` metres high; a journey to a place 48 m north answers that only a drop off the
/// ledge leads on.
fn penned_above_a_ledge(height: f64) -> PluginRun {
    use dereth_testkit::plugins::{START_EW, START_NS};
    let (mut run, _) = tank(Vec::new());
    let m = |metres: f64| metres / 240.0;
    let (s, n, w, e) = (
        START_NS - m(10.0),
        START_NS + m(10.0),
        START_EW - m(10.0),
        START_EW + m(10.0),
    );
    run.wall((s, w), (n, w));
    run.wall((s, e), (n, e));
    run.wall((s, w), (s, e));
    run.ledge((n, w), (n, e), height);
    run.type_line("/tank set EnableBuffing false");
    run
}

/// Where only a drop off a ledge leads on (the client's journeys leave a drop to the player), a
/// journey of the Tank's steps down one no higher than the body steps down without falling: it
/// walks to the ledge's edge, steps off, and makes the journey again, arriving. A higher one it
/// leaves, the journey failing as the client's did.
fn the_tanks_journey_steps_down_a_ledge_only_a_drop_leads_on_from_when_no_higher_than_a_step_down()
{
    use dereth_testkit::plugins::START_NS;
    let mut run = penned_above_a_ledge(0.9);
    run.type_line("/tank go 42.4N 33.8E");
    let mut t = 0.0;
    while t < 60.0 && !logged(&run, "arrived ") {
        run.advance(0.5);
        t += 0.5;
    }
    assert!(
        logged(
            &run,
            "journey: only a drop of 0.9 m off a ledge leads on; stepping down it"
        ),
        "{:#?}",
        run.console(TANK)
    );
    assert!(logged(&run, "arrived "), "{:#?}", run.console(TANK));
    let (ns, _, _) = run.player_place().expect("placed");
    assert!(
        ns > START_NS + 40.0 / 240.0,
        "beyond the ledge: {:.1} m north",
        (ns - START_NS) * 240.0
    );
    // Two metres high: left as it is.
    let mut run = penned_above_a_ledge(2.0);
    run.type_line("/tank go 42.4N 33.8E");
    let mut t = 0.0;
    while t < 60.0 && !logged(&run, "journey failed: only_by_drop: a drop of 2.0 m") {
        run.advance(0.5);
        t += 0.5;
    }
    assert!(
        logged(&run, "journey failed: only_by_drop: a drop of 2.0 m"),
        "{:#?}",
        run.console(TANK)
    );
    assert!(
        !logged(&run, "stepping down it"),
        "{:#?}",
        run.console(TANK)
    );
    let (ns, _, _) = run.player_place().expect("placed");
    assert!(ns < START_NS + 10.0 / 240.0, "still above the ledge");
    note_asserted("plugins.samples.the-tanks-journey-steps-down-a-ledge-only-a-drop-leads-on-from-when-no-higher-than-a-step-down");
}

/// The vendor the Tank's vendor trip goes to, as its setting says.
fn store_vendor(run: &mut PluginRun) -> Option<String> {
    run.type_line("/tank settings");
    run.advance(0.5);
    run.printed()
        .iter()
        .find_map(|l| l.strip_prefix("[Tank] Vendor = ").map(str::to_owned))
}
