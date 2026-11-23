//! The Navigator sample, in the vein of GoArrow: places searched by name and type from what the
//! client learned of the world (its places, its portal knowledge) and the player's own places, in
//! one list with the coordinates seen in chat and the spots picked on the map; a destination that
//! may follow an object; and the route finder's ways to a place with and without portals and
//! recalls. Each runs the sample from `dereth/plugins/navigator` in a scripted world.

use dereth_client_net::client_session::SessionEvent;
use dereth_plugin_api::Value;
use dereth_primitives::Position;
use dereth_testkit::behaviours::note_asserted;
use dereth_testkit::plugins::{read_package, PluginRun, ScriptedObject};
use dereth_testkit::scripted_travel::ScriptedPortal;

dereth_testkit::scenarios! {
    scenario_the_navigator_searches_a_whole_eras_places_and_keeps_running => the_navigator_searches_a_whole_eras_places_and_keeps_running ["plugins.samples.the-navigator-finds-places-by-name-and-type-and-points-at-the-nearest"],
    scenario_the_navigator_says_what_it_knows_before_a_search => the_navigator_says_what_it_knows_before_a_search ["plugins.samples.the-navigator-says-what-it-knows-before-a-search"],
    scenario_a_plugins_journey_hands_a_token_back_only_when_it_says_it_may => a_plugins_journey_hands_a_token_back_only_when_it_says_it_may ["plugins.nav.handing-a-token-back-asks-the-player-first"],
    scenario_the_navigator_finds_places_by_name_and_type_and_points_at_the_nearest => the_navigator_finds_places_by_name_and_type_and_points_at_the_nearest ["plugins.samples.the-navigator-finds-places-by-name-and-type-and-points-at-the-nearest"],
    scenario_the_navigator_keeps_the_players_own_places_outdoors_and_in_a_dungeon => the_navigator_keeps_the_players_own_places_outdoors_and_in_a_dungeon ["plugins.samples.the-navigator-keeps-the-players-own-places-outdoors-and-in-a-dungeon"],
    scenario_the_navigator_lists_coordinates_seen_in_chat_and_points_at_the_one_picked => the_navigator_lists_coordinates_seen_in_chat_and_points_at_the_one_picked ["plugins.samples.the-navigator-lists-coordinates-seen-in-chat-and-points-at-the-one-picked"],
    scenario_the_navigator_says_each_way_to_a_place_in_chat_and_goes_the_best => the_navigator_says_each_way_to_a_place_in_chat_and_goes_the_best ["plugins.samples.the-navigator-says-each-way-to-a-place-in-chat-and-goes-the-best"],
    scenario_the_navigators_destination_follows_the_selected_object_and_the_window_shows_where_the_character_is => the_navigators_destination_follows_the_selected_object_and_the_window_shows_where_the_character_is ["plugins.samples.the-navigators-destination-follows-the-selected-object-and-the-window-shows-where-the-character-is"],
    scenario_the_navigator_takes_a_spot_picked_on_the_map_by_a_right_or_double_click => the_navigator_takes_a_spot_picked_on_the_map_by_a_right_or_double_click ["plugins.samples.the-navigator-takes-a-spot-picked-on-the-map-by-a-right-or-double-click"],
    scenario_the_navigator_brings_no_places_of_its_own => the_navigator_brings_no_places_of_its_own ["plugins.samples.the-navigator-brings-no-places-of-its-own"],
    scenario_the_navigator_stops_going_to_a_target_it_was_pointed_away_from_or_cleared => the_navigator_stops_going_to_a_target_it_was_pointed_away_from_or_cleared ["plugins.samples.the-navigator-stops-going-to-a-target-it-was-pointed-away-from-or-cleared"],
    scenario_the_navigators_buttons_are_up_only_when_they_can_act => the_navigators_buttons_are_up_only_when_they_can_act ["plugins.samples.the-navigators-buttons-are-up-only-when-they-can-act"],
    scenario_the_navigators_one_list_holds_saved_chat_and_map_places_under_its_filter_and_deletes_them => the_navigators_one_list_holds_saved_chat_and_map_places_under_its_filter_and_deletes_them ["plugins.samples.the-navigators-one-list-holds-saved-chat-and-map-places-under-its-filter-and-deletes-them"],
    scenario_the_navigator_keeps_its_target_map_clicks_and_chat_coordinates_across_a_reload => the_navigator_keeps_its_target_map_clicks_and_chat_coordinates_across_a_reload ["plugins.samples.the-navigator-keeps-its-target-map-clicks-and-chat-coordinates-across-a-reload"],
    scenario_the_navigator_walks_round_a_wall_where_the_client_makes_no_journeys => the_navigator_walks_round_a_wall_where_the_client_makes_no_journeys ["plugins.samples.the-navigator-walks-round-a-wall-where-the-client-makes-no-journeys"],
    scenario_the_navigator_lists_a_place_once_and_puts_its_exact_name_first => the_navigator_lists_a_place_once_and_puts_its_exact_name_first ["plugins.samples.the-navigator-finds-places-by-name-and-type-and-points-at-the-nearest"],
    scenario_a_route_walks_round_a_wall_to_a_waypoint_out_of_sight => a_route_walks_round_a_wall_to_a_waypoint_out_of_sight ["plugins.nav.a-route-walks-round-a-wall-to-a-waypoint-out-of-sight"],
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

/// The line of the window's dump for the node `id`.
fn node_line(dump: &str, id: &str) -> String {
    dump.lines()
        .filter(|l| !l.trim_start().starts_with("window "))
        .find(|l| l.contains(&format!(" #{id} ")) || l.trim_end().ends_with(&format!(" #{id}")))
        .unwrap_or_default()
        .to_owned()
}

fn printed_since(run: &PluginRun, from: usize) -> Vec<String> {
    run.printed().into_iter().skip(from).collect()
}

/// `/nav find` lists the places whose name holds the words, of the type named last, nearest first:
/// the portals the client knows, the dungeon a portal leads into and the place outdoors another
/// sends to, and the lifestones and shops the client learned of the world; `/nav nearest <type>`
/// points at the nearest of a type, and a dungeon is gone into through its portal; the window's
/// search box and type choice list them too.
fn the_navigator_finds_places_by_name_and_type_and_points_at_the_nearest() {
    use dereth_navigation::{EdgeSource, PlaceKind};
    let mut run = navigator();
    run.scripted_world("Tester", Vec::new());
    run.scripted_portals(&portals(), Vec::new());
    run.learn_place(
        PlaceKind::Lifestone,
        EdgeSource::Learned,
        "Life Stone",
        (42.21, 33.81),
    );
    run.learn_place(
        PlaceKind::Vendor,
        EdgeSource::Published,
        "Grocer",
        (42.19, 33.79),
    );
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
    run.type_line("/nav find ar destination");
    let lines = printed_since(&run, from);
    assert_eq!(
        lines,
        ["[Nav] Arwic (destination) 33.6N 56.8E, 5.9 km"],
        "where the portal to Arwic sends, the type named keeping the others out"
    );
    let from = run.printed().len();
    run.type_line("/nav find vendor");
    let lines = printed_since(&run, from);
    assert!(
        lines.len() == 1 && lines[0].starts_with("[Nav] Grocer (vendor) 42.2N 33.8E, 3 m"),
        "the shop the server published: {lines:?}"
    );

    run.type_line("/nav nearest lifestone");
    assert!(
        run.overlays().contains("label=\"Life Stone\""),
        "the lifestone the client learned is the destination, marked:\n{}",
        run.overlays()
    );
    run.type_line("/nav nearest dungeon");
    assert!(
        node_line(&run.ui_dump(), "to").contains("Destination: Drudge Hideout (42.2N 33.9E)"),
        "the dungeon, by its entrance:\n{}",
        run.ui_dump()
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
    run.set(ID, "main.search", Value::from("grocer"));
    let dump = run.ui_dump();
    assert!(dump.contains("Grocer"), "{dump}");
    run.set(ID, "main.search", Value::from("portal"));
    let dump = run.ui_dump();
    assert!(
        !dump.contains("Grocer")
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
    assert!(
        run.overlays().contains("label=\"Camp\"")
            && node_line(&run.ui_dump(), "to").contains("Destination: Camp (42.2N 33.8E)"),
        "the saved place, where it was saved:\n{}",
        run.ui_dump()
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
/// it, typed "chat"; picking one makes it the destination, and `/nav last` makes the newest it.
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
    run.set(ID, "main.filter", Value::from("Chat"));
    let results = node_line(&run.ui_dump(), "results");
    assert!(
        results.contains(r#"[["Bob: 41.9N 34.1E","chat","41.9N 34.1E","X"],["Alice: 42.3N 33.8E","chat","42.3N 33.8E","X"]]"#),
        "newest first, with who said them:\n{results}"
    );
    run.choose(ID, "main.results", vec![2]);
    run.frame();
    assert!(
        node_line(&run.ui_dump(), "to").contains("Destination: Alice: 42.3N 33.8E (42.3N 33.8E)"),
        "picked, the destination:\n{}",
        run.ui_dump()
    );
    assert!(
        run.overlays().contains("label=\"Alice: 42.3N 33.8E\""),
        "marked:\n{}",
        run.overlays()
    );
    run.type_line("/nav last");
    assert!(
        node_line(&run.ui_dump(), "to").contains("Destination: 41.9N 34.1E"),
        "/nav last makes the newest the destination:\n{}",
        run.ui_dump()
    );
    note_asserted(
        "plugins.samples.the-navigator-lists-coordinates-seen-in-chat-and-points-at-the-one-picked",
    );
}

/// `/nav routes` asks the client for each way to the destination: the best, through a portal, and
/// on foot without it, each said in chat with its legs; Go goes the best way, telling each leg as
/// it begins, and arrives through the portal.
fn the_navigator_says_each_way_to_a_place_in_chat_and_goes_the_best() {
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
    run.type_line("/nav");
    run.click(ID, "main.go");
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
        "plugins.samples.the-navigator-says-each-way-to-a-place-in-chat-and-goes-the-best",
    );
}

/// The window shows where the character is, by the Name box; `/nav sel` makes the selected object
/// the destination, marked in the world on the object (no arrow), which the marker follows as it
/// moves.
fn the_navigators_destination_follows_the_selected_object_and_the_window_shows_where_the_character_is(
) {
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
        node_line(&run.ui_dump(), "here").contains("42.2N 33.8E"),
        "the window shows where the character is:\n{}",
        run.ui_dump()
    );
    run.select(GUARD);
    run.type_line("/nav sel");
    run.advance(0.5);
    let marker = |run: &PluginRun| {
        run.overlays()
            .lines()
            .find(|l| l.starts_with("world network.dereth.navigator"))
            .map(str::to_owned)
            .unwrap_or_default()
    };
    assert!(
        marker(&run).contains("label=\"Town Guard\"") && marker(&run).contains("0x70000201"),
        "the marker is on the object:\n{}",
        run.overlays()
    );
    assert!(
        !run.overlays().contains("arrow network.dereth.navigator"),
        "and no arrow:\n{}",
        run.overlays()
    );
    assert!(
        node_line(&run.ui_dump(), "to").contains("Destination: Town Guard (following it)"),
        "{}",
        run.ui_dump()
    );
    run.move_object(GUARD, 42.3, 33.8);
    run.advance(0.5);
    assert!(
        marker(&run).contains("0x70000201"),
        "the marker stays on the object as it moves:\n{}",
        run.overlays()
    );
    note_asserted("plugins.samples.the-navigators-destination-follows-the-selected-object-and-the-window-shows-where-the-character-is");
}

/// A spot on the game's map picked with a right click, or with two left clicks there in a row,
/// is the destination, with a marker in the world and one on the map; a single left click (the
/// map's own, for dragging it) leaves the destination as it was.
fn the_navigator_takes_a_spot_picked_on_the_map_by_a_right_or_double_click() {
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
    run.map_click(&click(41.0, 30.0, MapButton::Left));
    run.advance(0.5);
    assert!(
        !run.overlays().contains("network.dereth.navigator"),
        "a single left click picks nothing:\n{}",
        run.overlays()
    );
    run.map_click(&click(42.3, 33.8, MapButton::Right));
    run.advance(0.5);
    let overlays = run.overlays();
    assert!(
        overlays.contains("label=\"42.3N 33.8E\"")
            && overlays.contains("world network.dereth.navigator")
            && overlays.contains("map network.dereth.navigator")
            && !overlays.contains("arrow network.dereth.navigator"),
        "a right click: the markers are on the spot:\n{overlays}"
    );
    run.map_click(&click(42.1, 33.9, MapButton::Left));
    run.map_click(&click(42.1, 33.9, MapButton::Left));
    run.advance(0.5);
    assert!(
        node_line(&run.ui_dump(), "to").contains("Destination: 42.1N 33.9E"),
        "a double click there: {}",
        run.ui_dump()
    );
    note_asserted(
        "plugins.samples.the-navigator-takes-a-spot-picked-on-the-map-by-a-right-or-double-click",
    );
}

/// The character's start moved `east` and `north` meters, as map coordinates `(ns, ew)`.
fn off(east: f64, north: f64) -> (f64, f64) {
    (42.2 + north / 240.0, 33.8 + east / 240.0)
}

/// The package holds the Navigator's code and its manifest and nothing else: no list of places
/// and no routes. Before the client knows anything of the world, nothing is found; the towns and
/// houses the game's data names are found once the client has read them, and a place the player
/// saves is found too.
fn the_navigator_brings_no_places_of_its_own() {
    use dereth_client_contract::panels::map::MapPlaceKind;
    let dir = dereth_testkit::conformance::workspace_root().join("dereth/plugins/navigator");
    let mut files: Vec<String> = Vec::new();
    let mut dirs = vec![dir.clone()];
    while let Some(d) = dirs.pop() {
        for e in std::fs::read_dir(&d)
            .expect("the package's folder")
            .flatten()
        {
            let path = e.path();
            if path.is_dir() {
                dirs.push(path);
            } else {
                files.push(
                    path.strip_prefix(&dir)
                        .expect("inside the package")
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }
    files.sort();
    assert!(
        files
            .iter()
            .all(|f| f == "plugin.toml" || (f.ends_with(".lua") && !f.contains('/'))),
        "only code and the manifest: {files:?}"
    );

    let mut run = navigator();
    run.scripted_world("Tester", Vec::new());
    run.advance(0.5);
    for words in ["a", "e", "holt"] {
        let from = run.printed().len();
        run.type_line(&format!("/nav find {words}"));
        assert_eq!(
            printed_since(&run, from),
            [format!("[Nav] No place matches {words}.")],
            "nothing known of the world yet"
        );
    }
    // As the world map measures them: the radar's numbers less half a cell, 0.05.
    run.data_places(&[
        (MapPlaceKind::Town, "Holtburg", (42.05, 33.55)),
        (MapPlaceKind::Landmark, "Bandit Castle", (39.95, 29.95)),
        (MapPlaceKind::Cottage, "Cottage", (42.2, 33.75)),
    ]);
    run.advance(0.5);
    let from = run.printed().len();
    run.type_line("/nav find holt town");
    let lines = printed_since(&run, from);
    assert!(
        lines.len() == 1 && lines[0].starts_with("[Nav] Holtburg (town) 42.1N 33.6E"),
        "the town the data names: {lines:?}"
    );
    let from = run.printed().len();
    run.type_line("/nav find house");
    let lines = printed_since(&run, from);
    assert!(
        lines.len() == 1 && lines[0].starts_with("[Nav] Cottage (house) 42.2N 33.8E, 12 m"),
        "a house by its building: {lines:?}"
    );
    run.type_line("/nav save Home");
    let from = run.printed().len();
    run.type_line("/nav find home");
    let lines = printed_since(&run, from);
    assert!(
        lines.len() == 1 && lines[0].starts_with("[Nav] Home (saved) 42.2N 33.8E"),
        "{lines:?}"
    );
    note_asserted("plugins.samples.the-navigator-brings-no-places-of-its-own");
}

/// Going to a target, the character stops when the player points the Navigator somewhere else
/// or clears it: the journey was to the old target.
fn the_navigator_stops_going_to_a_target_it_was_pointed_away_from_or_cleared() {
    let start = |run: &mut PluginRun| {
        run.place_player(42.2, 33.8);
        run.type_line("/nav 42.2N 34.0E");
        run.type_line("/nav go");
        run.advance(1.0);
        let (_, ew, _) = run.player_place().expect("outdoors");
        assert!(ew > 33.801, "under way east: {ew}");
    };
    let mut run = navigator();
    run.scripted_world("Tester", Vec::new());
    run.advance(0.5);

    start(&mut run);
    run.type_line("/nav clear");
    run.advance(0.5);
    let (_, stopped_at, _) = run.player_place().expect("outdoors");
    run.advance(5.0);
    let (_, ew, _) = run.player_place().expect("outdoors");
    assert!(
        (ew - stopped_at).abs() < 0.002,
        "cleared, the character stops where it was: {stopped_at} then {ew}"
    );

    start(&mut run);
    run.type_line("/nav 42.3N 33.8E");
    run.advance(0.5);
    let (_, stopped_at, _) = run.player_place().expect("outdoors");
    run.advance(5.0);
    let (_, ew, _) = run.player_place().expect("outdoors");
    assert!(
        (ew - stopped_at).abs() < 0.002,
        "pointed elsewhere, the character stops going east: {stopped_at} then {ew}"
    );
    assert!(
        run.overlays().contains("label=\"42.3N 33.8E\""),
        "the arrow on the new target:\n{}",
        run.overlays()
    );
    // A journey started over one under way: the first's end, arriving late, leaves the second
    // going, and clearing stops it.
    start(&mut run);
    run.type_line("/nav travel 42.3N 33.8E");
    run.advance(1.0);
    run.type_line("/nav clear");
    run.advance(0.5);
    let stopped_at = run.player_place().expect("outdoors");
    run.advance(5.0);
    let (ns, ew, _) = run.player_place().expect("outdoors");
    assert!(
        (ns - stopped_at.0).abs() < 0.002 && (ew - stopped_at.1).abs() < 0.002,
        "the second journey cleared, the character stops: {stopped_at:?} then {ns} {ew}"
    );
    assert!(ns < 42.29, "and never arrives: {ns}");
    note_asserted(
        "plugins.samples.the-navigator-stops-going-to-a-target-it-was-pointed-away-from-or-cleared",
    );
}

/// Stop is up only while the Navigator is going somewhere, Clear only while it has a destination,
/// and Dungeon Map only indoors: outdoors it is down, saying so when the pointer rests on it.
fn the_navigators_buttons_are_up_only_when_they_can_act() {
    let mut run = navigator();
    run.scripted_world("Tester", Vec::new());
    run.advance(0.5);
    run.type_line("/nav");
    run.advance(0.5);
    let button = |run: &PluginRun, id: &str| node_line(&run.ui_dump(), id);
    assert!(
        button(&run, "stop").contains("enabled=false"),
        "{}",
        run.ui_dump()
    );
    assert!(
        button(&run, "clear").contains("enabled=false"),
        "{}",
        run.ui_dump()
    );
    let dungeon = button(&run, "dungeon");
    assert!(
        dungeon.contains("enabled=false") && dungeon.contains("tooltip=\"You're outdoors\""),
        "outdoors: {dungeon}"
    );
    run.type_line("/nav 42.2N 34.0E");
    assert!(
        button(&run, "clear").contains("enabled=true"),
        "{}",
        run.ui_dump()
    );
    assert!(
        button(&run, "stop").contains("enabled=false"),
        "{}",
        run.ui_dump()
    );
    run.click(ID, "main.go");
    run.advance(0.5);
    assert!(
        button(&run, "stop").contains("enabled=true"),
        "going: {}",
        run.ui_dump()
    );
    run.click(ID, "main.stop");
    assert!(
        button(&run, "stop").contains("enabled=false"),
        "stopped: {}",
        run.ui_dump()
    );
    run.click(ID, "main.clear");
    assert!(
        button(&run, "clear").contains("enabled=false"),
        "cleared: {}",
        run.ui_dump()
    );
    note_asserted("plugins.samples.the-navigators-buttons-are-up-only-when-they-can-act");
}

/// The destination, the spots picked on the map and the coordinates seen in chat are kept for the
/// world: after the plugin reloads, the destination is marked again and the list holds them.
fn the_navigator_keeps_its_target_map_clicks_and_chat_coordinates_across_a_reload() {
    use dereth_client_contract::panels::map::{MapButton, MapClick};
    let mut run = navigator();
    run.scripted_world("Tester", Vec::new());
    run.advance(0.5);
    run.server_says(&[on_channel(0x800, "Alice", "camp at 41.9N, 34.1E")]);
    run.map_click(&MapClick {
        ns: 42.0,
        ew: 33.5,
        button: MapButton::Right,
        ctrl: false,
        shift: false,
        alt: false,
    });
    run.type_line("/nav 42.3N 33.8E");
    run.advance(0.5);
    run.type_line(&format!("/plugin reload {ID}"));
    run.advance(1.0);
    let overlays = run.overlays();
    assert!(
        overlays.contains("world network.dereth.navigator")
            && overlays.contains("label=\"42.3N 33.8E\""),
        "the destination marked again:\n{overlays}"
    );
    run.type_line("/nav");
    let dump = run.ui_dump();
    assert!(
        dump.contains("41.9N 34.1E") && dump.contains("Alice") && dump.contains("42.0N 33.5E"),
        "the chat's coordinates and the map's spot kept:\n{dump}"
    );
    note_asserted(
        "plugins.samples.the-navigator-keeps-its-target-map-clicks-and-chat-coordinates-across-a-reload",
    );
}

/// On a client that walks for plugins and plans walking routes but makes no journeys, Go follows
/// the client's route round a wall to a place behind it, corner by corner, rather than walking
/// into the wall.
fn the_navigator_walks_round_a_wall_where_the_client_makes_no_journeys() {
    let mut run = PluginRun::with_config(|c| {
        c.capabilities
            .retain(|&k| k != dereth_plugin_api::HostCapability::NavJourney);
    });
    run.load(read_package(
        &dereth_testkit::conformance::workspace_root().join("dereth/plugins/navigator"),
    ));
    run.scripted_world("Tester", Vec::new());
    run.advance(0.5);
    run.wall(off(-4.0, 6.0), off(4.0, 6.0));
    let (ns, ew) = off(0.0, 12.0);
    run.type_line(&format!("/nav {ns:.4}N {ew:.4}E"));
    run.type_line("/nav go");
    run.advance(15.0);
    let (at_ns, at_ew, _) = run.player_place().expect("outdoors");
    let meters = dereth_primitives::num::math::hypot((at_ns - ns) * 240.0, (at_ew - ew) * 240.0);
    assert!(
        meters <= 3.5,
        "round the wall to the place behind it: {meters:.1} m short ({:?})",
        run.printed()
    );
    assert!(
        run.printed()
            .iter()
            .any(|l| l.starts_with("[Nav] Arrived at")),
        "{:?}",
        run.printed()
    );
    note_asserted(
        "plugins.samples.the-navigator-walks-round-a-wall-where-the-client-makes-no-journeys",
    );
}

/// A route's waypoint behind a wall is reached along the client's walking route round the wall's
/// end, not by walking straight into the wall; a waypoint in a straight line of sight is walked
/// to straight. The route is the player's own VTank route in the Navigator's data folder.
fn a_route_walks_round_a_wall_to_a_waypoint_out_of_sight() {
    use dereth_plugin_host::PluginFiles;
    let files = super::SharedFiles::default();
    let mut run = PluginRun::new().with_files(Box::new(files.clone()));
    run.load(read_package(
        &dereth_testkit::conformance::workspace_root().join("dereth/plugins/navigator"),
    ));
    run.scripted_world("Tester", Vec::new());
    run.wall(off(-4.0, 6.0), off(4.0, 6.0));
    let (n1, e1) = off(6.0, 0.0);
    let (n2, e2) = off(0.0, 12.0);
    // A once route of two points: one east in plain sight, one north behind the wall.
    let route = format!("uTank2 NAV 1.2\n4\n2\n0\n{e1}\n{n1}\n0\n0\n0\n{e2}\n{n2}\n0\n0\n");
    files
        .clone()
        .write(ID, "behind.nav", route.as_bytes())
        .expect("the route saved");
    run.advance(0.5);
    run.type_line("/nav route behind.nav");
    run.advance(20.0);
    let (ns, ew, _) = run.player_place().expect("outdoors");
    let meters = dereth_primitives::num::math::hypot((ns - n2) * 240.0, (ew - e2) * 240.0);
    assert!(
        meters <= 3.5 && run.printed().iter().any(|l| l == "[Nav] Route done."),
        "the waypoint behind the wall reached: {meters:.1} m from it ({:?}; {:?})",
        run.printed(),
        run.moves()
    );
    note_asserted("plugins.nav.a-route-walks-round-a-wall-to-a-waypoint-out-of-sight");
}

/// A plugin's journey that says it may hand a token back (`hand_backs = true`) reaches the travel
/// agreed; one that does not, refused, so a journey out of the Training Academy ends at once
/// rather than waiting on a question.
fn a_plugins_journey_hands_a_token_back_only_when_it_says_it_may() {
    let main = r#"
commands.add("agreed", "a journey that may hand a token back", function()
	nav.path_to({ cell = 0x019E0114 }, { hand_backs = true })
end)
commands.add("unagreed", "a journey that may not", function()
	nav.path_to({ cell = 0x019E0114 })
end)
"#;
    let mut run = PluginRun::new();
    run.load(dereth_plugin_host::Package {
        origin: "journeys".into(),
        files: [
            (
                "plugin.toml".to_owned(),
                b"id = \"test.journeys\"\nname = \"Journeys\"\nversion = \"1.0.0\"\napi = \"0.7\"\n\n[permissions]\n\"act.interactive\" = \"Goes where it is told.\"\n\n[capabilities]\noptional = [\"nav.journey\"]\n"
                    .to_vec(),
            ),
            ("main.lua".to_owned(), main.as_bytes().to_vec()),
        ]
        .into(),
    });
    run.scripted_world("Tester", Vec::new());
    run.advance(0.5);
    let hand_backs = |run: &PluginRun| run.last_journey_allow().map(|a| a.hand_backs);
    run.type_line("/agreed");
    run.advance(0.5);
    assert_eq!(
        hand_backs(&run),
        Some(dereth_client_runtime::travel::HandBacks::Agreed),
        "{:?}",
        run.console("test.journeys")
    );
    run.type_line("/unagreed");
    run.advance(0.5);
    assert_eq!(
        hand_backs(&run),
        Some(dereth_client_runtime::travel::HandBacks::Refused)
    );
    note_asserted("plugins.nav.handing-a-token-back-asks-the-player-first");
}

/// A world as big as the end of retail's package (5,300 portals and 11,300 places): a search
/// answers with the nearest that match, and the Navigator keeps running.
fn the_navigator_searches_a_whole_eras_places_and_keeps_running() {
    let mut run = navigator();
    run.scripted_world("Tester", Vec::new());
    let start = outdoors(42.0, 33.0);
    let spot = |i: u32| {
        // Spread over the map, about a twentieth of a degree apart.
        let (ns, ew) = (
            f64::from(i % 120) * 0.5 - 30.0,
            f64::from(i / 120) * 0.5 - 30.0,
        );
        outdoors(ns, ew)
    };
    let portals: Vec<ScriptedPortal> = (0..5300_u32)
        .map(|i| ScriptedPortal {
            name: if i % 53 == 0 {
                format!("Drudge Hideout {i}")
            } else {
                format!("Portal to Place {i}")
            },
            class: 10_000 + i,
            at: spot(i),
            to: spot((i + 1) % 5300),
            source: dereth_navigation::EdgeSource::Hint,
            refuses: false,
        })
        .collect();
    run.scripted_portals(&portals, Vec::new());
    for i in 0..11_300_u32 {
        let (ns, ew) = (
            f64::from(i % 150) * 0.4 - 30.0,
            f64::from(i / 150) * 0.4 + 0.1,
        );
        run.learn_place(
            dereth_navigation::PlaceKind::Npc,
            dereth_navigation::EdgeSource::Hint,
            &format!("Person {i}"),
            (ns, ew),
        );
    }
    let _ = start;
    run.advance(1.0);
    let from = run.printed().len();
    run.type_line("/nav find hideout");
    run.advance(1.0);
    let said = printed_since(&run, from);
    let found: Vec<&String> = said
        .iter()
        .filter(|l| l.contains("Drudge Hideout"))
        .collect();
    assert!(found.len() >= 8, "the nearest that match: {said:?}");
    assert!(
        said.iter().any(|l| l.starts_with("[Nav] ... and ")),
        "and how many more: {said:?}"
    );
    assert_eq!(run.status(ID), "running", "{said:?}");
    note_asserted(
        "plugins.samples.the-navigator-finds-places-by-name-and-type-and-points-at-the-nearest",
    );
}

/// Before a search, the Navigator says how many portals and places the client knows and where
/// each was learned (the era's package, the server, the character's own play, the game's data);
/// `/nav known` says it in chat.
fn the_navigator_says_what_it_knows_before_a_search() {
    let mut run = navigator();
    run.scripted_world("Tester", Vec::new());
    let from = run.printed().len();
    run.type_line("/nav known");
    assert_eq!(
        printed_since(&run, from),
        ["[Nav] Knows no portals and no places."],
        "a world the client knows nothing of"
    );
    run.scripted_portals(&portals(), Vec::new());
    run.learn_place(
        dereth_navigation::PlaceKind::Npc,
        dereth_navigation::EdgeSource::Published,
        "Guard",
        (42.1, 33.6),
    );
    run.data_places(&[(
        dereth_client_contract::panels::map::MapPlaceKind::Town,
        "Holtburg",
        (42.1, 33.6),
    )]);
    run.advance(0.5);
    let from = run.printed().len();
    run.type_line("/nav known");
    let said = printed_since(&run, from);
    assert_eq!(said.len(), 1, "{said:?}");
    assert!(
        said[0].ends_with("and 2 places (1 from the server, 1 from the game's data)."),
        "{said:?}"
    );
    assert!(said[0].starts_with("[Nav] Knows 2 portals ("), "{said:?}");
    // The window: the counts alone on its status line, and its own width (72 cells of 7)
    // however long its destination and status lines are.
    let long = "Sentry of the Far Watchtower Above the Old Mine Road North of Holtburg Town";
    run.learn_place(
        dereth_navigation::PlaceKind::Npc,
        dereth_navigation::EdgeSource::Published,
        long,
        (42.2, 33.7),
    );
    run.advance(0.5);
    run.type_line("/nav");
    assert!(
        node_line(&run.ui_dump(), "status").contains(r#""Knows 2 portals and 3 places.""#),
        "{}",
        run.ui_dump()
    );
    run.set(ID, "main.search", Value::from("sentry"));
    run.type_line(&format!("/nav {long}"));
    run.click(ID, "main.go");
    let dump = run.layout_dump();
    assert!(
        node_line(&run.ui_dump(), "to").contains(long) && dump.contains("window #main 504x"),
        "{dump}\n{}",
        run.ui_dump()
    );
    note_asserted("plugins.samples.the-navigator-says-what-it-knows-before-a-search");
}

/// The window's one list, seven places tall, holds the places found, the player's saved places, the coordinates seen
/// in chat and the spots picked on the map; the Filter shows one of those alone. A saved place, a
/// line of chat and a spot on the map carry a mark in the list's narrow last column, and a click
/// there deletes the entry (forgets the saved place); a click elsewhere on the row picks it.
fn the_navigators_one_list_holds_saved_chat_and_map_places_under_its_filter_and_deletes_them() {
    use dereth_client_contract::panels::map::{MapButton, MapClick};
    let mut run = navigator();
    run.scripted_world("Tester", Vec::new());
    run.advance(0.5);
    run.server_says(&[on_channel(0x800, "Alice", "camp at 41.9N, 34.1E")]);
    run.map_click(&MapClick {
        ns: 42.0,
        ew: 33.5,
        button: MapButton::Right,
        ctrl: false,
        shift: false,
        alt: false,
    });
    run.type_line("/nav save Home");
    run.type_line("/nav clear");
    run.type_line("/nav");
    let results = node_line(&run.ui_dump(), "results");
    assert!(
        results.contains(r#"["42.0N 33.5E","map","42.0N 33.5E","X"]"#)
            && results.contains(r#"["Alice: 41.9N 34.1E","chat","41.9N 34.1E","X"]"#)
            && results.contains(r#"["Home","saved","42.2N 33.8E","X"]"#),
        "all in one list, each deletable:\n{results}"
    );
    let layout = run.layout_dump();
    assert!(
        layout
            .lines()
            .any(|l| l.trim_start().starts_with("list #results") && l.ends_with(" rows=7")),
        "the list shows seven places:
{layout}"
    );
    for (filter, only) in [("Saved", "Home"), ("Chat", "Alice"), ("Map", "42.0N 33.5E")] {
        run.set(ID, "main.filter", Value::from(filter));
        let results = node_line(&run.ui_dump(), "results");
        assert_eq!(
            results.matches("\"X\"").count(),
            1,
            "{filter} alone: {results}"
        );
        assert!(results.contains(only), "{filter}: {results}");
    }
    // A click on a row away from the mark picks it; on the mark, deletes it.
    run.set(ID, "main.filter", Value::from("Everything"));
    run.click_cell(ID, "main.results", 1, 1);
    assert!(
        node_line(&run.ui_dump(), "to").contains("Destination: 42.0N 33.5E"),
        "{}",
        run.ui_dump()
    );
    run.click_cell(ID, "main.results", 1, 4);
    let results = node_line(&run.ui_dump(), "results");
    assert!(
        !results.contains("42.0N 33.5E"),
        "the map's spot deleted: {results}"
    );
    run.set(ID, "main.filter", Value::from("Saved"));
    run.click_cell(ID, "main.results", 1, 4);
    let from = run.printed().len();
    run.type_line("/nav find saved");
    assert!(
        !printed_since(&run, from).iter().any(|l| l.contains("Home")),
        "the saved place forgotten"
    );
    note_asserted("plugins.samples.the-navigators-one-list-holds-saved-chat-and-map-places-under-its-filter-and-deletes-them");
}

/// A place the client knows twice at one spot (seen, and the server's) is one row; a search for a
/// name puts the place of exactly that name first, however far, ahead of nearer places whose
/// names only hold it, and `/nav <name>` points at it.
fn the_navigator_lists_a_place_once_and_puts_its_exact_name_first() {
    use dereth_client_contract::panels::map::MapPlaceKind;
    use dereth_navigation::{EdgeSource, PlaceKind};
    let mut run = navigator();
    run.scripted_world("Tester", Vec::new());
    for source in [EdgeSource::Learned, EdgeSource::Published] {
        run.learn_place(
            PlaceKind::Vendor,
            source,
            "Holtburg Casino VIP",
            (42.21, 33.81),
        );
    }
    // Far off: the town the game's data names.
    run.data_places(&[(MapPlaceKind::Town, "Holtburg", (12.05, 23.55))]);
    // Three portals far off that all send to one spot just outside it: one destination.
    let to_holtburg = |i: u32| ScriptedPortal {
        name: "Portal to Holtburg".into(),
        class: 3000 + i,
        at: outdoors(10.0 + f64::from(i), 20.0),
        to: outdoors(12.0, 23.5),
        source: dereth_navigation::EdgeSource::Hint,
        refuses: false,
    };
    run.scripted_portals(
        &[to_holtburg(0), to_holtburg(1), to_holtburg(2)],
        Vec::new(),
    );
    run.advance(0.5);
    let from = run.printed().len();
    run.type_line("/nav find holtburg");
    let lines = printed_since(&run, from);
    let named = |what: &str| lines.iter().filter(|l| l.contains(what)).count();
    assert_eq!(named("(destination)"), 1, "one destination: {lines:?}");
    assert_eq!(named("(vendor)"), 1, "the vendor once: {lines:?}");
    assert_eq!(named("(portal)"), 3, "the three portals: {lines:?}");
    assert!(
        lines[0].starts_with("[Nav] Holtburg (town)")
            || lines[0].starts_with("[Nav] Holtburg (destination)"),
        "a place named Holtburg first: {lines:?}"
    );
    assert!(
        lines[1].starts_with("[Nav] Holtburg (town)")
            || lines[1].starts_with("[Nav] Holtburg (destination)"),
        "and the other: {lines:?}"
    );
    run.type_line("/nav Holtburg");
    assert!(
        node_line(&run.ui_dump(), "to").contains("Destination: Holtburg ("),
        "the town, not the nearer vendor:\n{}",
        run.ui_dump()
    );
    // A journey the client cannot make at all is said to be, not dropped.
    let from = run.printed().len();
    run.type_line("/nav travel portal Nowhere");
    run.advance(0.5);
    let lines = printed_since(&run, from);
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("[Nav] Can't go to portal Nowhere: ")),
        "{lines:?}"
    );
    note_asserted(
        "plugins.samples.the-navigator-finds-places-by-name-and-type-and-points-at-the-nearest",
    );
}
