//! Behaviour: none (tooling: a dungeon pulled from one world into a world repository)

use std::collections::BTreeMap;
use std::path::Path;

use lyceum_foundry::pull::{self, Exit};
use lyceum_foundry::repo::RowSchema;
use lyceum_foundry::sql_ids::server_content;

/// The server's world around dungeon `0101`: a generator in it with two creatures it links, a
/// chest, and a portal out to the land at `A9B4`; the portal into it, placed twice on the land; and
/// a portal from it into another dungeon, `0102`.
const SERVER: &str = "\
INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`) VALUES (1154,'gen',1,'2005-02-09 10:00:00'),(7924,'drudge',10,'2005-02-09 10:00:00'),(1919,'chest',20,'2005-02-09 10:00:00'),(3631,'surface',7,'2005-02-09 10:00:00'),(3630,'way-in',7,'2005-02-09 10:00:00'),(1430,'deeper',7,'2005-02-09 10:00:00');
INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`) VALUES (3631,1,'Surface'),(3630,1,'Carved Cave'),(1430,1,'Halls'),(7924,1,'Drudge');
INSERT INTO `weenie_properties_position` (`object_Id`, `position_Type`, `obj_Cell_Id`, `origin_X`, `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`) VALUES (3631,2,0xA9B40031,84,141,42,1,0,0,0),(3630,2,0x01010100,10,-30,0,1,0,0,0),(1430,2,0x01020150,49.5,-6.8,0,1,0,0,0);
INSERT INTO `landblock_instance` (`guid`, `weenie_Class_Id`, `obj_Cell_Id`, `origin_X`, `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`, `is_Link_Child`, `last_Modified`) VALUES (0x70101000,1154,0x01010100,10,-30,0,1,0,0,0,'\\0','2019-05-16 00:00:00'),(0x70101001,7924,0x01010101,12.5,-31,0,0.6,0,0,-0.8,'\u{1}','2019-05-16 00:00:00'),(0x70101002,7924,0x01010102,14,-32,0,1,0,0,0,'\u{1}','2019-05-16 00:00:00'),(0x70101003,1919,0x01010103,20,-40,0,1,0,0,0,'\\0','2019-05-16 00:00:00'),(0x70101004,3631,0x01010104,30,-50,0,1,0,0,0,'\\0','2019-05-16 00:00:00'),(0x70101005,1430,0x01010105,31,-50,0,1,0,0,0,'\\0','2019-05-16 00:00:00'),(0x7A9B4000,3630,0xA9B40031,84,141,42,1,0,0,0,'\\0','2019-05-16 00:00:00'),(0x7C98C000,3630,0xC98C0020,50,50,10,1,0,0,0,'\\0','2019-05-16 00:00:00'),(0x70102000,7924,0x01020150,1,1,0,1,0,0,0,'\\0','2019-05-16 00:00:00');
INSERT INTO `landblock_instance_link` (`parent_GUID`, `child_GUID`, `last_Modified`) VALUES (0x70101000,0x70101001,'2019-05-16 00:00:00'),(0x70101000,0x70101002,'2019-05-16 00:00:00');
";

/// A pack of the source world: the chest moved, and a spawn of its own new cell, which the build
/// leaves as a token.
const PACKS: &str = "\
INSERT INTO `landblock_instance` (`guid`, `weenie_Class_Id`, `obj_Cell_Id`, `origin_X`, `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`, `is_Link_Child`, `last_Modified`) VALUES (0x70101003,1919,0x01010103,21,-41,0,1,0,0,0,'\\0','2026-11-24 00:00:00'),({{guid:pack:new-chest}},1919,{{cell:pack:room-000@0101}},1,1,0,1,0,0,0,'\\0','2026-11-24 00:00:00');
";

fn prov(note: &str) -> toml::Table {
    let mut t = toml::Table::new();
    t.insert("from".into(), toml::Value::String("authored".into()));
    t.insert("note".into(), toml::Value::String(note.into()));
    t
}

/// The server's vocabulary for placements, as `empyrean-import schema` writes it.
fn schema() -> RowSchema {
    RowSchema::of_schema(&serde_json::json!({ "rows": { "placement": {
        "file": "landblock/{cell >> 16:04X}.toml",
        "item": "spawn {guid:#010X}",
        "key": ["guid"],
        "fields": {
            "guid": "hex", "class": "int", "cell": "hex",
            "at": { "list": "float" }, "rotation": { "list": "float" },
            "link_child": "bool", "last_modified": "time",
            "links": { "list": { "either": ["hex", { "fields": { "child": "hex", "last_modified": "time" } }] } },
        },
    } } }))
    .unwrap()
    .unwrap()
}

/// What the source world places in the dungeon is planned as placement rows, the pack's over the
/// server's, each generator with the creatures it links; a pack's spawn the build left a token is
/// left out and named. The portal into the dungeon is its way in, placed twice on the land; its
/// exits are the portal onto the land and the one into another dungeon.
#[test]
fn a_dungeons_things_are_planned_as_rows_with_its_ways_in_and_out() {
    let (server, packs) = (server_content(SERVER), server_content(PACKS));
    let p = pull::plan(&server, &packs, 0x0101, &prov("server"), &prov("pack"));
    let guids: Vec<u32> = p.rows.keys().copied().collect();
    assert_eq!(
        guids,
        [
            0x7010_1000,
            0x7010_1001,
            0x7010_1002,
            0x7010_1003,
            0x7010_1004,
            0x7010_1005
        ],
        "the dungeon's own, not the other dungeon's or the land's"
    );
    let row = |g: u32| &p.rows[&g];
    let int = |v: &toml::Value| v.as_integer().unwrap();
    let links: Vec<i64> = row(0x7010_1000)["links"]
        .as_array()
        .unwrap()
        .iter()
        .map(int)
        .collect();
    assert_eq!(links, [0x7010_1001, 0x7010_1002]);
    assert_eq!(row(0x7010_1001)["link_child"].as_bool(), Some(true));
    assert_eq!(
        row(0x7010_1001)["rotation"].as_array().unwrap()[0].as_float(),
        Some(0.6),
        "written as typed"
    );
    let chest = row(0x7010_1003);
    assert_eq!(
        chest["at"].as_array().unwrap()[0].as_float(),
        Some(21.0),
        "the pack's"
    );
    assert_eq!(chest["prov"]["note"].as_str(), Some("pack"));
    assert_eq!(row(0x7010_1000)["prov"]["note"].as_str(), Some("server"));
    assert_eq!(p.skipped.len(), 1, "{:?}", p.skipped);
    assert_eq!(
        p.classes.iter().copied().collect::<Vec<_>>(),
        [1154, 1430, 1919, 3631, 7924]
    );
    assert_eq!(p.portals, [(3630, 2, Some("Carved Cave".to_owned()))]);
    assert_eq!(
        p.exits,
        [
            Exit {
                guid: 0x7010_1004,
                class: 3631,
                name: Some("Surface".into()),
                to: 0xA9B4_0031,
                at: Some([84.0, 141.0, 42.0]),
            },
            Exit {
                guid: 0x7010_1005,
                class: 1430,
                name: Some("Halls".into()),
                to: 0x0102_0150,
                at: Some([49.5, -6.8, 0.0]),
            },
        ]
    );
    assert!(p.exits[0].outdoors() && !p.exits[1].outdoors());
}

/// The rows are written into the pack's file for their landblock, after what it holds, in the
/// repository's own form (guids, cells and links in hexadecimal); a free guid of a landblock is
/// the lowest of its authored range its rows don't use.
#[test]
fn the_rows_are_written_into_the_landblocks_file_in_the_repositorys_form() {
    let (server, packs) = (server_content(SERVER), server_content(PACKS));
    let p = pull::plan(&server, &packs, 0x0101, &prov("server"), &prov("pack"));
    let pack = Path::new("pack");
    let file = pack.join("landblock/0101.toml");
    let before = "# the repository's own\n[[placement]]\nguid = 0x70101800\nclass = 1154\ncell = 0x01010100\nat = [1.0, 2.0, 3.0]\nrotation = [1.0, 0.0, 0.0, 0.0]\n";
    let files: BTreeMap<_, _> = [(file.clone(), before.to_owned())].into();
    let rows: Vec<toml::Table> = p.rows.values().cloned().collect();
    let out = pull::write_rows(&schema(), pack, &rows, &mut |f| files.get(f).cloned()).unwrap();
    assert_eq!(out.keys().collect::<Vec<_>>(), [&file]);
    let text = &out[&file];
    assert!(text.starts_with(before), "{text}");
    for line in [
        "guid = 0x70101000\n",
        "cell = 0x01010100\n",
        "class = 1154\n",
    ] {
        assert!(text.contains(line), "{line}{text}");
    }
    assert!(text.contains("links = [0x70101001, 0x70101002]"), "{text}");
    assert_eq!(text.matches("[[placement]]").count(), 7);
    let taken = [0x7010_1800, 0x7010_1801].into_iter().collect();
    assert_eq!(pull::free_guid(0x0101, &taken), Some(0x7010_1802));
}

/// The classes a repository lacks are added to its library list by class, each with its name, under
/// a line naming the dungeon; those it names already, by number, are not added again, and adding
/// them twice adds nothing.
#[test]
fn the_classes_a_repository_lacks_are_added_to_its_library_list_once() {
    let list = "# Doors.\nweenie 278\nweenie corpse  # by its name\n";
    let missing = [
        (1626, Some("Silver Rat".to_owned())),
        (278, Some("Door".to_owned())),
        (516, None),
    ];
    let (text, added) = pull::add_to_library(list, 0x01A9, &missing);
    assert_eq!(added, [516, 1626]);
    assert!(text.starts_with(list.trim_end()), "{text}");
    assert!(
        text.ends_with(
            "\n\n# The classes dungeon 01A9 places, added by lyceum pull-dungeon.\nweenie 516\nweenie 1626  # Silver Rat\n"
        ),
        "{text}"
    );
    let (again, none) = pull::add_to_library(&text, 0x01A9, &missing);
    assert!(none.is_empty());
    assert_eq!(again, text);
}
