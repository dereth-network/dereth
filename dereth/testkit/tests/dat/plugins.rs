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
fn outline_in_view(store: &Store, landblock: u32, room: &dereth_plugin_api::Value) -> usize {
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
        let Some(floor_z) = cell.get("bounds").get("min").get("z").as_f64() else {
            continue;
        };
        if (floor_z - z0).abs() > 3.0 {
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
        .min(900)
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
/// at the character's level around it, with the character's dot, on a canvas within its limit.
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
    run.host()
        .ui_input(
            ID,
            "main",
            dereth_plugin_host::UiInput::Select {
                node: "pages".into(),
                rows: vec![5],
            },
        )
        .expect("the Dungeon page");
    run.advance(20.0);
    let dump = run.ui_dump();
    let canvas = dump
        .lines()
        .find(|l| l.trim_start().starts_with("canvas #map"))
        .expect("the map's canvas");
    let lines = canvas.matches("\"line\"").count();
    // One of the lines is the character's heading; the rest are the outline.
    let expected = outline_in_view(&store_again(), 0x01A4, &room);
    assert!(
        expected > 10 && lines == expected + 1,
        "the floor's outline at the character's level around it: {lines} lines drawn, {expected} \
         edges of the level's floors that no other floor shares reach the view:\n{canvas}"
    );
    assert!(
        canvas.contains("\"circle\""),
        "the character's dot:\n{canvas}"
    );
    let note = dump
        .lines()
        .find(|l| l.contains("label #map_note"))
        .expect("the map's note");
    let read = note
        .split('"')
        .nth(1)
        .and_then(|t| t.split(' ').next())
        .and_then(|n| n.parse::<u32>().ok());
    let all = note
        .split(" of ")
        .nth(1)
        .and_then(|t| t.split(' ').next())
        .and_then(|n| n.parse::<u32>().ok());
    assert!(read.is_some() && read == all, "every room read: {note}");
    note_asserted("plugins.samples.the-navigator-maps-the-dungeon-floor-around-the-character");
}
