//! The Navigator sample, in the manner of GoArrow: places searched by name and type from its own
//! list, the client's portal knowledge, what the character saw and the player's own places; the
//! character's coordinates and an arrow that follows an object; coordinates seen in chat; and the
//! route finder's ways to a place with and without portals and recalls. Each runs the sample from
//! `dereth/plugins/navigator` in a scripted world.

use dereth_client_net::client_session::SessionEvent;
use dereth_plugin_api::Value;
use dereth_primitives::Position;
use dereth_testkit::behaviours::note_asserted;
use dereth_testkit::plugins::{read_package, PluginRun, ScriptedObject};
use dereth_testkit::scripted_travel::ScriptedPortal;

dereth_testkit::scenarios! {
    scenario_the_navigator_finds_places_by_name_and_type_and_points_at_the_nearest => the_navigator_finds_places_by_name_and_type_and_points_at_the_nearest ["plugins.samples.the-navigator-finds-places-by-name-and-type-and-points-at-the-nearest"],
    scenario_the_navigator_keeps_the_players_own_places_outdoors_and_in_a_dungeon => the_navigator_keeps_the_players_own_places_outdoors_and_in_a_dungeon ["plugins.samples.the-navigator-keeps-the-players-own-places-outdoors-and-in-a-dungeon"],
    scenario_the_navigator_lists_coordinates_seen_in_chat_and_points_at_the_one_picked => the_navigator_lists_coordinates_seen_in_chat_and_points_at_the_one_picked ["plugins.samples.the-navigator-lists-coordinates-seen-in-chat-and-points-at-the-one-picked"],
    scenario_the_navigator_finds_each_way_to_a_place_and_goes_the_one_picked => the_navigator_finds_each_way_to_a_place_and_goes_the_one_picked ["plugins.samples.the-navigator-finds-each-way-to-a-place-and-goes-the-one-picked"],
    scenario_the_navigators_arrow_follows_the_selected_object_and_its_hud_shows_where_the_character_is => the_navigators_arrow_follows_the_selected_object_and_its_hud_shows_where_the_character_is ["plugins.samples.the-navigators-arrow-follows-the-selected-object-and-its-hud-shows-where-the-character-is"],
    scenario_the_navigator_points_where_the_player_clicks_on_the_map => the_navigator_points_where_the_player_clicks_on_the_map ["plugins.samples.the-navigator-points-where-the-player-clicks-on-the-map"],
}

const ID: &str = "network.dereth.navigator";

fn navigator() -> PluginRun {
    let mut run = PluginRun::new();
    run.load(read_package(
        &dereth_testkit::conformance::workspace_root().join("dereth/plugins/navigator"),
    ));
    run
}

fn outdoors(ns: f64, ew: f64) -> Position {
    dereth_plugin_host::places::from_coords(ns, ew, 0.0).expect("on the map")
}

/// A dungeon's place: cell `0x019E0114`, as a portal leads there.
fn in_the_dungeon() -> Position {
    dereth_plugin_host::places::from_value(
        &Value::map()
            .with("cell", 0x019E_0114_u32)
            .with("x", 10.0)
            .with("y", -40.0)
            .with("z", 0.0),
    )
    .expect("a cell place")
}

/// Two portals the client's store knows: one into a dungeon a short walk east of the start, one
/// north of it to Arwic, far to the east.
fn portals() -> Vec<ScriptedPortal> {
    vec![
        ScriptedPortal {
            name: "Drudge Hideout".into(),
            class: 2068,
            at: outdoors(42.2, 33.85),
            to: in_the_dungeon(),
            source: dereth_navigation::EdgeSource::Published,
            refuses: false,
        },
        ScriptedPortal {
            name: "Portal to Arwic".into(),
            class: 2069,
            at: outdoors(42.25, 33.8),
            to: outdoors(33.6, 56.8),
            source: dereth_navigation::EdgeSource::Published,
            refuses: false,
        },
    ]
}

fn printed_since(run: &PluginRun, from: usize) -> Vec<String> {
    run.printed().into_iter().skip(from).collect()
}

/// `/nav find` lists the places whose name holds the words, of the type named last, nearest first:
/// the package's towns, the portals the client knows and the dungeon a portal leads into, and a
/// lifestone the character saw; `/nav nearest <type>` points at the nearest of a type, and a
/// dungeon is gone into through its portal; the window's search box and type choice list them too.
fn the_navigator_finds_places_by_name_and_type_and_points_at_the_nearest() {
    let mut run = navigator();
    run.scripted_world("Tester", Vec::new());
    run.scripted_portals(&portals(), Vec::new());
    run.spawn(vec![ScriptedObject {
        id: 0x7000_0101,
        name: "Life Stone".into(),
        obj_type: dereth_rules::weenie::item_type::LIFESTONE,
        place: Some((42.21, 33.81)),
        ..ScriptedObject::default()
    }]);
    run.advance(0.5);

    let from = run.printed().len();
    run.type_line("/nav find hideout");
    let lines = printed_since(&run, from);
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("[Nav] Drudge Hideout (portal) 42.2N 33.9E, 12 m"))
            && lines
                .iter()
                .any(|l| l.starts_with("[Nav] Drudge Hideout (dungeon) 42.2N 33.9E")),
        "the portal and the dungeon it leads into: {lines:?}"
    );
    let from = run.printed().len();
    run.type_line("/nav find ar town");
    let lines = printed_since(&run, from);
    assert!(
        lines
            .first()
            .is_some_and(|l| l == "[Nav] Arwic (town) 33.6N 56.8E, 5.9 km"),
        "the nearest town holding \"ar\" first: {lines:?}"
    );
    assert!(
        lines.len() > 2
            && lines.iter().all(|l| l.contains("(town)"))
            && lines.iter().any(|l| l.starts_with("[Nav] Yaraq (town)"))
            && lines.iter().any(|l| l.starts_with("[Nav] Al-Arqas (town)")),
        "towns only, the type named keeping the others out: {lines:?}"
    );

    run.type_line("/nav nearest lifestone");
    assert!(
        run.overlays().contains("label=\"Life Stone\""),
        "the arrow points at the lifestone the character saw:\n{}",
        run.overlays()
    );
    run.type_line("/nav nearest dungeon");
    let overlays = run.overlays();
    assert!(
        overlays.contains("arrow network.dereth.navigator")
            && overlays.contains("label=\"Drudge Hideout\"")
            && overlays.contains("distance=12.0"),
        "the arrow points at the dungeon's entrance:\n{overlays}"
    );
    run.type_line("/nav go");
    run.advance(8.0);
    assert!(
        run.player_place().is_none(),
        "the character went through the portal into the dungeon: {:?}",
        run.moves()
    );
    assert!(
        run.printed()
            .iter()
            .any(|l| l == "[Nav] Arrived at Drudge Hideout."),
        "{:?}",
        run.printed()
    );

    // The window's search: the words and the type, nearest first.
    run.type_line("/nav");
    run.set(ID, "main.search", Value::from("holt"));
    let dump = run.ui_dump();
    assert!(dump.contains("Holtburg"), "{dump}");
    run.set(ID, "main.search", Value::from("portal"));
    let dump = run.ui_dump();
    assert!(
        !dump.contains("Holtburg")
            && dump.contains("Portal to Arwic")
            && dump.contains("Drudge Hideout"),
        "{dump}"
    );
    note_asserted(
        "plugins.samples.the-navigator-finds-places-by-name-and-type-and-points-at-the-nearest",
    );
}

/// `/nav save` keeps where the character stands under a name, outdoors by its coordinates and in
/// a dungeon by its cell; `/nav <name>` points at it from elsewhere, `/nav here` says where the
/// character is, and `/nav forget` lets one go.
fn the_navigator_keeps_the_players_own_places_outdoors_and_in_a_dungeon() {
    let mut run = navigator();
    run.scripted_world("Tester", Vec::new());
    run.scripted_portals(&portals(), Vec::new());
    run.advance(0.5);
    run.type_line("/nav save Camp");
    run.place_player(42.3, 33.8);
    run.type_line("/nav camp");
    let overlays = run.overlays();
    assert!(
        overlays.contains("label=\"Camp\"") && overlays.contains("distance=24.0"),
        "the saved place, 24 m south:\n{overlays}"
    );

    // Into the dungeon by its portal, then a place saved there by its cell.
    run.type_line("/nav travel portal Drudge Hideout");
    run.advance(8.0);
    assert!(
        run.player_place().is_none(),
        "in the dungeon: {:?}",
        run.moves()
    );
    let from = run.printed().len();
    run.type_line("/nav here");
    assert_eq!(
        printed_since(&run, from),
        ["[Nav] Here: cell 0x019E0114 (10.0, -40.0, 0.0)."]
    );
    run.type_line("/nav save Deep");
    let from = run.printed().len();
    run.type_line("/nav find saved");
    let lines = printed_since(&run, from);
    assert!(
        lines.contains(&"[Nav] Deep (saved) cell 0x019E0114 (10.0, -40.0, 0.0), 0 m".to_owned())
            && lines
                .iter()
                .any(|l| l.starts_with("[Nav] Camp (saved) 42.2N 33.8E")),
        "both kept, the one here measured: {lines:?}"
    );
    run.type_line("/nav forget camp");
    let from = run.printed().len();
    run.type_line("/nav find saved");
    let lines = printed_since(&run, from);
    assert!(
        lines.iter().any(|l| l.starts_with("[Nav] Deep (saved)"))
            && !lines.iter().any(|l| l.contains("Camp")),
        "{lines:?}"
    );
    note_asserted(
        "plugins.samples.the-navigator-keeps-the-players-own-places-outdoors-and-in-a-dungeon",
    );
}

/// A line said on a chat channel (`0x800` the fellowship's, `0x0100_0000` the co-vassals').
fn on_channel(channel: u32, who: &str, text: &str) -> SessionEvent {
    use dereth_protocol::Message;
    let m = dereth_protocol::comms::CommunicationChannelBroadcastRecv {
        channel,
        sender_name: who.into(),
        message: text.into(),
    };
    let mut w = dereth_protocol::archive::Writer::new();
    w.u32(dereth_protocol::comms::CommunicationChannelBroadcastRecv::OPCODE.0);
    m.write(&mut w).expect("the broadcast writes");
    SessionEvent::UiEvent {
        opcode: dereth_protocol::comms::CommunicationChannelBroadcastRecv::OPCODE,
        blob: w.into_inner(),
    }
}

/// Every pair of coordinates heard in chat lands in the window's list, newest first, with who said
/// it; picking one points the arrow at it, and `/nav last` points at the newest.
fn the_navigator_lists_coordinates_seen_in_chat_and_points_at_the_one_picked() {
    let mut run = navigator();
    run.scripted_world("Tester", Vec::new());
    run.advance(0.5);
    run.server_says(&[on_channel(
        0x800,
        "Alice",
        "Meet me at 42.3N, 33.8E by the well",
    )]);
    run.server_says(&[on_channel(0x0100_0000, "Bob", "nothing to see here")]);
    run.server_says(&[on_channel(0x0100_0000, "Bob", "drudges at 41.9N 34.1E")]);
    run.type_line("/nav");
    let dump = run.ui_dump();
    let bob = dump
        .find("41.9N 34.1E")
        .expect("Bob's coordinates are listed");
    let alice = dump
        .find("42.3N 33.8E")
        .expect("Alice's coordinates are listed");
    assert!(bob < alice, "newest first:\n{dump}");
    assert!(
        dump.contains("Alice") && dump.contains("Bob"),
        "who said them:\n{dump}"
    );

    run.host()
        .ui_input(
            ID,
            "main",
            dereth_plugin_host::UiInput::Select {
                node: "chat".into(),
                rows: vec![2],
            },
        )
        .expect("the chat list takes a pick");
    run.frame();
    assert!(
        run.ui_dump()
            .contains("To: 42.3N 33.8E (42.3N 33.8E), 24 m N"),
        "picked, to go to or point at:\n{}",
        run.ui_dump()
    );
    run.click(ID, "main.point");
    let overlays = run.overlays();
    assert!(
        overlays.contains("label=\"42.3N 33.8E\"") && overlays.contains("distance=24.0"),
        "Point puts the arrow on Alice's place, picked second in the list:\n{overlays}"
    );
    run.type_line("/nav last");
    assert!(
        run.overlays().contains("label=\"41.9N 34.1E\""),
        "/nav last points at the newest:\n{}",
        run.overlays()
    );
    note_asserted(
        "plugins.samples.the-navigator-lists-coordinates-seen-in-chat-and-points-at-the-one-picked",
    );
}

/// `/nav routes` asks the client for each way to the target: the best, through a portal, and on
/// foot without it, each said with its legs and listed on the Route page with the legs of the one
/// picked; going the best way tells each leg as it begins and arrives through the portal.
fn the_navigator_finds_each_way_to_a_place_and_goes_the_one_picked() {
    let mut run = navigator();
    run.scripted_world("Tester", Vec::new());
    run.scripted_portals(&portals(), Vec::new());
    run.advance(0.5);
    run.type_line("/nav arwic");
    let from = run.printed().len();
    run.type_line("/nav routes");
    run.advance(1.0);
    let lines = printed_since(&run, from);
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("[Nav] Best to Arwic: walk, portal Portal to Arwic, walk")),
        "the best way, through the portal: {lines:?}"
    );
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("[Nav] No portals to Arwic: walk (about")),
        "on foot, without the portal: {lines:?}"
    );
    assert!(
        !lines.iter().any(|l| l.starts_with("[Nav] No recalls")),
        "a way the same as one before is said once: {lines:?}"
    );
    let dump = run.ui_dump();
    assert!(
        dump.contains("portal Portal to Arwic") && dump.contains("33.6N 56.8E"),
        "the Route page lists the best way's legs:\n{dump}"
    );

    run.click(ID, "main.route_go");
    run.advance(10.0);
    let lines = run.printed();
    assert!(
        lines
            .iter()
            .any(|l| l == "[Nav] To the portal Portal to Arwic.")
            && lines.iter().any(|l| l == "[Nav] Arrived at Arwic."),
        "{lines:?}"
    );
    let (ns, ew, _) = run.player_place().expect("outdoors");
    assert!(
        (ns - 33.6).abs() < 0.05 && (ew - 56.8).abs() < 0.05,
        "at Arwic: {ns} {ew}"
    );
    note_asserted(
        "plugins.samples.the-navigator-finds-each-way-to-a-place-and-goes-the-one-picked",
    );
}

/// The HUD shows the character's own coordinates; `/nav sel` puts the arrow and a world marker on
/// the selected object, and the HUD's distance follows the object as it moves.
fn the_navigators_arrow_follows_the_selected_object_and_its_hud_shows_where_the_character_is() {
    const GUARD: u32 = 0x7000_0201;
    let mut run = navigator();
    run.scripted_world(
        "Tester",
        vec![ScriptedObject {
            id: GUARD,
            name: "Town Guard".into(),
            obj_type: dereth_rules::weenie::item_type::CREATURE,
            place: Some((42.25, 33.8)),
            ..ScriptedObject::default()
        }],
    );
    run.advance(0.5);
    assert!(
        run.ui_dump().contains("Here: 42.2N 33.8E"),
        "the HUD shows where the character is:\n{}",
        run.ui_dump()
    );
    run.select(GUARD);
    run.type_line("/nav sel");
    run.advance(0.5);
    let arrow = |run: &PluginRun| {
        run.overlays()
            .lines()
            .find(|l| l.starts_with("arrow network.dereth.navigator"))
            .map(str::to_owned)
            .unwrap_or_default()
    };
    let line = arrow(&run);
    assert!(
        line.contains("label=\"Town Guard\"")
            && line.contains("object=0x70000201")
            && line.contains("distance=12.0"),
        "the arrow is on the object:\n{}",
        run.overlays()
    );
    assert!(
        run.ui_dump().contains("Town Guard: 12 m N"),
        "{}",
        run.ui_dump()
    );
    run.move_object(GUARD, 42.3, 33.8);
    run.advance(0.5);
    assert!(
        arrow(&run).contains("distance=24.0"),
        "the arrow follows the object:\n{}",
        run.overlays()
    );
    assert!(
        run.ui_dump().contains("Town Guard: 24 m N"),
        "the distance follows the object:\n{}",
        run.ui_dump()
    );
    note_asserted("plugins.samples.the-navigators-arrow-follows-the-selected-object-and-its-hud-shows-where-the-character-is");
}

/// A left click on the game's map points the arrow at the spot clicked, with a marker in the world
/// and one on the map there; a right click leaves the target as it was.
fn the_navigator_points_where_the_player_clicks_on_the_map() {
    use dereth_client_contract::panels::map::{MapButton, MapClick};
    let click = |ns, ew, button| MapClick {
        ns,
        ew,
        button,
        ctrl: false,
        shift: false,
        alt: false,
    };
    let mut run = navigator();
    run.scripted_world("Tester", Vec::new());
    run.advance(0.5);
    run.map_click(&click(42.3, 33.8, MapButton::Left));
    run.advance(0.5);
    let overlays = run.overlays();
    assert!(
        overlays.contains("arrow network.dereth.navigator")
            && overlays.contains("label=\"42.3N 33.8E\"")
            && overlays.contains("turn=0 distance=24.0")
            && overlays.contains("world network.dereth.navigator")
            && overlays.contains("map network.dereth.navigator"),
        "the arrow and the markers are on the spot clicked, 24 m north:\n{overlays}"
    );
    assert!(
        run.ui_dump().contains("42.3N 33.8E: 24 m N"),
        "the HUD says how far:\n{}",
        run.ui_dump()
    );
    run.map_click(&click(41.0, 30.0, MapButton::Right));
    run.advance(0.5);
    let overlays = run.overlays();
    assert!(
        overlays.contains("label=\"42.3N 33.8E\"") && !overlays.contains("41.0N"),
        "a right click leaves the target as it was:\n{overlays}"
    );
    note_asserted("plugins.samples.the-navigator-points-where-the-player-clicks-on-the-map");
}
