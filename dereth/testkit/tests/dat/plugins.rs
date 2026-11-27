//! Plugins over the retail data files: what a plugin reads of the game's own data for its maps.
//!
//! `ALL` is this file's own list, concatenated with the other subjects' in `census.rs`.

use std::sync::Arc;

use dereth_dat::RetailDatStore;
use dereth_plugin_host::Package;
use dereth_testkit::behaviours::note_asserted;
use dereth_testkit::plugins::PluginRun;

dereth_testkit::scenarios! {
    scenario_a_plugin_reads_a_landblock_and_a_dungeon_cell_of_the_game_data => a_plugin_reads_a_landblock_and_a_dungeon_cell_of_the_game_data ["plugins.data.a-plugin-reads-a-landblock-and-a-dungeon-cell-of-the-game-data"],
    scenario_the_navigator_maps_the_dungeon_floor_around_the_character => the_navigator_maps_the_dungeon_floor_around_the_character ["plugins.samples.the-navigator-maps-the-dungeon-floor-around-the-character"],
    scenario_the_dungeon_map_names_the_dungeon_marks_its_portals_and_zooms_and_pans => the_dungeon_map_names_the_dungeon_marks_its_portals_and_zooms_and_pans ["plugins.samples.the-navigator-names-and-marks-the-dungeon-map-which-zooms-and-pans"],
}

/// The retail data files, as the plugin host reads them.
#[derive(Debug)]
struct Store(Arc<RetailDatStore>);

impl dereth_plugin_host::gamedata::PluginGameData for Store {
    fn read_cell(&self, id: u32) -> Option<Vec<u8>> {
        self.0.read_cell(dereth_primitives::DataId(id)).ok()
    }
    fn read_portal(&self, id: u32) -> Option<Vec<u8>> {
        self.0.read_portal(dereth_primitives::DataId(id)).ok()
    }
}

/// Holtburg's landblock (`0xA9B4`): heights in metres on its 9 by 9 vertices, buildings and the
/// cells inside them; a dungeon's first cell (`0x01A40100`): its portals to the cells beside it,
/// the cells seen from it, its floor's outline and its box.
fn a_plugin_reads_a_landblock_and_a_dungeon_cell_of_the_game_data() {
    let dir = dereth_dat::testing::dat_dir();
    let store = Arc::new(RetailDatStore::open_dir(&dir).unwrap_or_else(|e| {
        panic!(
            "this scenario needs the retail dats under $DERETH_TEST_DAT_DIR ({}): {e}",
            dir.display()
        )
    }));
    let main = r#"
local lb = game.data.landblock(0xA9B4)
local hmin, hmax = math.huge, -math.huge
for _, h in ipairs(lb.heights) do
	hmin, hmax = math.min(hmin, h), math.max(hmax, h)
end
print("landblock", #lb.heights, #lb.terrain, lb.buildings > 0, #lb.cells > 0, hmax > hmin)
local d = game.data.landblock(0x01A4)
local c = game.data.cell(d.cells[1])
local floor = 0
for _, p in ipairs(c.floor) do
	floor = floor + #p
end
print("cell", string.format("0x%08X", c.id), #c.portals > 0, #c.visible > 0, floor > 0,
	c.bounds.max.x > c.bounds.min.x)
print("none", game.data.landblock(0xFFFF) == nil, game.data.cell(0x01A4FFF0) == nil)
"#;
    let package = Package {
        origin: "maps".into(),
        files: [
            (
                "plugin.toml".to_owned(),
                b"id = \"test.maps\"\nname = \"Maps\"\nversion = \"1.0.0\"\napi = \"0.6\"\ncontexts = [\"pregame\", \"world\"]\n"
                    .to_vec(),
            ),
            ("main.lua".to_owned(), main.as_bytes().to_vec()),
        ]
        .into(),
    };
    let mut run = PluginRun::new().with_game_data(Box::new(Store(store)));
    run.load(package);
    run.frame();
    assert_eq!(
        run.console("test.maps"),
        [
            "landblock\t81\t81\ttrue\ttrue\ttrue",
            "cell\t0x01A40100\ttrue\ttrue\ttrue\ttrue",
            "none\ttrue\ttrue",
        ]
    );
    note_asserted("plugins.data.a-plugin-reads-a-landblock-and-a-dungeon-cell-of-the-game-data");
}

fn store_again() -> Store {
    Store(retail_store())
}

/// The edges of a dungeon's floors at the height of `room`'s place (within 3 m) that no other
/// floor of that level shares, with an end within 160 pixels (3 a meter) of the place: the outline
/// a map of the level around a character standing there shows, at most 900 of them.
/// The edges the Navigator's map draws around a character standing in `room`: the outline of the
/// floors of the cells on its level (it stands within a cell's height, or the cell's floor is within
/// 2 m of its feet), and the outline of the cells within 8 m above or below that are not, each the
/// edges no other floor of its own set shares, that reach the 320-pixel view at 3 pixels a meter.
fn outline_in_view(store: &Store, landblock: u32, room: &dereth_plugin_api::Value) -> usize {
    let level = outline_of(store, landblock, room, true);
    let near = outline_of(store, landblock, room, false);
    (level + near).min(900)
}

/// The edges of one of the two sets of [`outline_in_view`]: its level's with `level`, else the
/// levels near it.
fn outline_of(
    store: &Store,
    landblock: u32,
    room: &dereth_plugin_api::Value,
    level: bool,
) -> usize {
    use std::collections::BTreeMap;
    type Edge = ((i64, i64), (i64, i64));
    let num = |v: &dereth_plugin_api::Value| v.as_f64().unwrap_or_default();
    let block = dereth_plugin_host::gamedata::landblock(store, landblock);
    let at = room.get("position");
    let (x0, y0, z0) = (num(at.get("x")), num(at.get("y")), num(at.get("z")));
    let key = |x: f64, y: f64| {
        (
            dereth_primitives::num::to_i64_f64((x * 10.0 + 0.5).floor()),
            dereth_primitives::num::to_i64_f64((y * 10.0 + 0.5).floor()),
        )
    };
    let mut edges: BTreeMap<Edge, (usize, [f64; 4])> = BTreeMap::new();
    for id in block.get("cells").as_list().unwrap_or_default() {
        let id = id
            .as_i64()
            .and_then(|i| u32::try_from(i).ok())
            .unwrap_or_default();
        let cell = dereth_plugin_host::gamedata::cell(store, id);
        let (Some(floor_z), Some(top_z)) = (
            cell.get("bounds").get("min").get("z").as_f64(),
            cell.get("bounds").get("max").get("z").as_f64(),
        ) else {
            continue;
        };
        let on_level = (z0 >= floor_z - 0.5 && z0 <= top_z) || (floor_z - z0).abs() <= 2.0;
        let keep = if level {
            on_level
        } else {
            !on_level && (floor_z - z0).abs() <= 8.0
        };
        if !keep {
            continue;
        }
        for poly in cell.get("floor").as_list().unwrap_or_default() {
            let pts = poly.as_list().unwrap_or_default();
            for i in 0..pts.len() {
                let (a, b) = (&pts[i], &pts[(i + 1) % pts.len()]);
                let (ax, ay, bx, by) = (
                    num(a.get("x")),
                    num(a.get("y")),
                    num(b.get("x")),
                    num(b.get("y")),
                );
                let (ka, kb) = (key(ax, ay), key(bx, by));
                let k = if ka <= kb { (ka, kb) } else { (kb, ka) };
                edges.entry(k).or_insert((0, [ax, ay, bx, by])).0 += 1;
            }
        }
    }
    let seen = |x: f64, y: f64| {
        let (px, py) = (160.0 + (x - x0) * 3.0, 160.0 - (y - y0) * 3.0);
        (0.0..=320.0).contains(&px) && (0.0..=320.0).contains(&py)
    };
    edges
        .values()
        .filter(|(n, [ax, ay, bx, by])| *n == 1 && (seen(*ax, *ay) || seen(*bx, *by)))
        .count()
}

fn retail_store() -> Arc<RetailDatStore> {
    let dir = dereth_dat::testing::dat_dir();
    Arc::new(RetailDatStore::open_dir(&dir).unwrap_or_else(|e| {
        panic!(
            "this scenario needs the retail dats under $DERETH_TEST_DAT_DIR ({}): {e}",
            dir.display()
        )
    }))
}

/// The Navigator sample's map: carried into a dungeon (`0x01A4`, its first room) by a portal, the
/// window's Map page reads the dungeon's rooms a few at a time and draws the outline of the floors
/// at the character's level around it, and dim the levels just above and below, with the
/// character's dot, on a canvas within its limit that fits the map's window.
fn the_navigator_maps_the_dungeon_floor_around_the_character() {
    use dereth_plugin_api::Value;
    const ID: &str = "network.dereth.navigator";
    let store = retail_store();
    let room = dereth_plugin_host::gamedata::cell(&Store(store.clone()), 0x01A4_0100);
    let at = room.get("position");
    let inside = dereth_plugin_host::places::from_value(
        &Value::map()
            .with("cell", 0x01A4_0100_u32)
            .with("x", at.get("x").as_f64().expect("the room's place"))
            .with("y", at.get("y").as_f64().expect("the room's place"))
            .with("z", at.get("z").as_f64().expect("the room's place")),
    )
    .expect("a cell place");
    let mut run = PluginRun::new().with_game_data(Box::new(Store(store)));
    run.load(dereth_testkit::plugins::read_package(
        &dereth_testkit::conformance::workspace_root().join("dereth/plugins/navigator"),
    ));
    run.scripted_world("Tester", Vec::new());
    run.scripted_portals(
        &[dereth_testkit::scripted_travel::ScriptedPortal {
            name: "Mite Hole".into(),
            class: 2070,
            at: dereth_plugin_host::places::from_coords(42.2, 33.85, 0.0).expect("on the map"),
            to: inside,
            source: dereth_navigation::EdgeSource::Published,
            refuses: false,
        }],
        Vec::new(),
    );
    run.advance(0.5);
    run.type_line("/nav travel portal Mite Hole");
    run.advance(8.0);
    assert!(
        run.player_place().is_none(),
        "in the dungeon: {:?}",
        run.moves()
    );
    run.type_line("/nav");
    run.advance(0.5);
    // Indoors, the Dungeon Map button is up, and opens the map in a window of its own.
    run.click(ID, "main.dungeon");
    assert!(
        run.host().window(ID, "dungeon").is_some_and(|w| w.open),
        "the dungeon map's window"
    );
    run.advance(20.0);
    // Its map fits its window: drawn as large as the room the window gives it.
    let layout = run.layout_dump();
    assert!(
        layout
            .lines()
            .any(|l| l.trim_start().starts_with("canvas #map") && l.contains(" drawn=")),
        "{layout}"
    );
    let dump = run.ui_dump();
    let canvas = dump
        .lines()
        .find(|l| l.trim_start().starts_with("canvas #map"))
        .expect("the map's canvas");
    let lines = canvas.matches("\"line\"").count();
    // One of the lines is the character's heading; the rest are the outlines.
    let expected = outline_in_view(&store_again(), 0x01A4, &room);
    assert!(
        expected > 10 && lines == expected + 1,
        "the floors' outlines at and near the character's level around it: {lines} lines drawn, \
         {expected} edges of the floors that no other floor of their level shares reach the \
         view:\n{canvas}"
    );
    assert!(
        canvas.contains("\"circle\""),
        "the character's dot:\n{canvas}"
    );
    // A monster a few meters off in the room, and the Navigator pointed at it: its dot, the
    // target's ring and the way there are drawn too, and the floors are still all there.
    let mut mite = inside;
    mite.frame.origin.x += 4.0;
    run.spawn(vec![dereth_testkit::plugins::ScriptedObject {
        id: 0x8000_0F01,
        name: "Mite".into(),
        obj_type: dereth_rules::weenie::item_type::CREATURE,
        bitfield: dereth_rules::weenie::bitfield::ATTACKABLE,
        ..Default::default()
    }]);
    run.place_object_at(0x8000_0F01, mite);
    run.type_line("/nav obj Mite");
    run.advance(5.0);
    let dump_after = run.ui_dump();
    let pointed = dump_after
        .lines()
        .find(|l| l.trim_start().starts_with("canvas #map"))
        .expect("the map's canvas");
    assert!(
        pointed.matches("\"circle\"").count() >= 3,
        "the character, the monster and the target, each a circle:\n{pointed}"
    );
    assert!(
        pointed.matches("\"line\"").count() >= expected + 2,
        "the outlines, the heading and the way to the monster:\n{pointed}"
    );
    // Every room read: no bar of how much is built, and the dungeon named by the portal into it.
    assert!(
        !canvas.contains("Building...") && canvas.contains("\"text\":\"Mite Hole\""),
        "built, and named:\n{canvas}"
    );
    note_asserted("plugins.samples.the-navigator-maps-the-dungeon-floor-around-the-character");
}

/// The map's canvas line in a dump of the plugin windows.
fn map_canvas(dump: &str) -> String {
    dump.lines()
        .find(|l| l.trim_start().starts_with("canvas #map"))
        .expect("the map's canvas")
        .to_owned()
}

/// The Navigator's dungeon map in Tusker Abode (`0x5C45`), gone into by the portal the world's
/// places name "Tusker Abode" (the Facility Hub's portal of the same name sends outdoors, and names
/// nothing): while the dungeon is read a bar says how much is built; once built the map is named
/// "Tusker Abode", the drop point is marked where the character stands, and the dungeon's way out
/// ("Surface", some 280 meters west) is out of view until the wheel zooms the map out and a drag
/// moves it there; Reset brings the view back to the character.
fn the_dungeon_map_names_the_dungeon_marks_its_portals_and_zooms_and_pans() {
    const ID: &str = "network.dereth.navigator";
    let at = |cell: u32, x: f64, y: f64, z: f64| {
        dereth_plugin_host::places::from_value(
            &dereth_plugin_api::Value::map()
                .with("cell", cell)
                .with("x", x)
                .with("y", y)
                .with("z", z),
        )
        .expect("a place")
    };
    let portal =
        |name: &str, class: u32, from, to| dereth_testkit::scripted_travel::ScriptedPortal {
            name: name.into(),
            class,
            at: from,
            to,
            source: dereth_navigation::EdgeSource::Published,
            refuses: false,
        };
    let mut run = PluginRun::new().with_game_data(Box::new(Store(retail_store())));
    run.load(dereth_testkit::plugins::read_package(
        &dereth_testkit::conformance::workspace_root().join("dereth/plugins/navigator"),
    ));
    run.scripted_world("Tester", Vec::new());
    run.scripted_portals(
        &[
            portal(
                "Tusker Abode",
                22654,
                dereth_plugin_host::places::from_coords(42.2, 33.85, 0.0).expect("on the map"),
                at(0x5C45_0384, 325.98, -99.593, 0.005),
            ),
            portal(
                "Surface",
                22655,
                at(0x5C45_0150, 46.951, -76.496, -11.995),
                at(0xF67B_000C, 37.096, 94.624, -0.095),
            ),
            portal(
                "Tusker Abode",
                42175,
                at(0x8A02_0123, 88.309, -48.327, -12.05),
                at(0xF67B_000C, 26.259, 84.692, -0.095),
            ),
        ],
        Vec::new(),
    );
    run.advance(0.5);
    // Into it by the dungeon found by its name.
    run.type_line("/nav nearest dungeon");
    run.type_line("/nav go");
    run.advance(8.0);
    assert!(
        run.printed()
            .iter()
            .any(|l| l == "[Nav] Arrived at Tusker Abode."),
        "{:?}",
        run.printed()
    );
    run.type_line("/nav");
    run.advance(0.5);
    run.click(ID, "main.dungeon");
    run.advance(1.0);
    let building = map_canvas(&run.ui_dump());
    assert!(
        building.contains("\"text\":\"Building...\"") && building.contains("\"op\":\"rect\""),
        "the bar while the dungeon is read:\n{building}"
    );
    run.advance(60.0);
    let built = map_canvas(&run.ui_dump());
    assert!(!built.contains("Building..."), "built:\n{built}");
    assert!(
        built.contains("\"text\":\"Tusker Abode\"") && !built.contains("0x5C45"),
        "named by the portal into it:\n{built}"
    );
    assert!(
        built.contains("\"text\":\"Drop point\"") && !built.contains("Surface"),
        "the drop point in view, the way out not:\n{built}"
    );
    let dump = run.ui_dump();
    assert!(
        dump.lines()
            .any(|l| l.contains("button #map_reset") && l.contains("text=\"Reset\"")),
        "{dump}"
    );
    // Zoomed out as far as it goes, then dragged right: the way out comes into view.
    run.canvas(ID, "dungeon.map", [160.0, 160.0, -12.0, 0.0, 0.0]);
    run.advance(0.5);
    run.canvas(ID, "dungeon.map", [160.0, 160.0, 0.0, 150.0, 0.0]);
    run.advance(0.5);
    let moved = map_canvas(&run.ui_dump());
    assert!(
        moved.contains("\"text\":\"Surface\""),
        "the way out, zoomed out and moved to:\n{moved}"
    );
    run.click(ID, "dungeon.map_reset");
    run.advance(0.5);
    let reset = map_canvas(&run.ui_dump());
    assert_eq!(reset, built, "Reset brings the view back");
    note_asserted(
        "plugins.samples.the-navigator-names-and-marks-the-dungeon-map-which-zooms-and-pans",
    );
}
