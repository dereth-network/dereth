//! Behaviour: none (tooling: the server's own spawns in the world editor: shown, moved and taken away through a pack's override file, and given as the server places them)

use dereth_primitives::Vec3;
use lyceum_foundry::edit::{Op, Session};
use lyceum_foundry::sql_ids::server_content;

use crate::common::{Fixture, HOME, SETUP};

/// A server weenie drawn with the base's setup, a generator of it in `HOME` and the one instance
/// the generator links, as a dump writes them: no column lists, bits as a dump writes them.
const SERVER: &str = "\
CREATE TABLE `weenie` (`class_Id` int, `class_Name` varchar(100), `type` int, `last_Modified` datetime);
INSERT INTO `weenie` VALUES (7924,'testdrudge',10,'2005-02-09 10:00:00'),(1154,'gen',1,'2005-02-09 10:00:00');
CREATE TABLE `weenie_properties_string` (`id` int, `object_Id` int, `type` smallint, `value` text);
INSERT INTO `weenie_properties_string` VALUES (1,7924,1,'Test Drudge'),(2,1154,1,'Generator');
CREATE TABLE `weenie_properties_d_i_d` (`id` int, `object_Id` int, `type` smallint, `value` int);
INSERT INTO `weenie_properties_d_i_d` VALUES (3,7924,1,33554433),(4,1154,1,33554433);
CREATE TABLE `landblock_instance` (`guid` int, `weenie_Class_Id` int, `obj_Cell_Id` int, `origin_X` float, `origin_Y` float, `origin_Z` float, `angles_W` float, `angles_X` float, `angles_Y` float, `angles_Z` float, `is_Link_Child` bit(1), `last_Modified` datetime);
INSERT INTO `landblock_instance` VALUES (1879113728,1154,16842777,84.0067,4.37315,94.005,0.92388,0,0,-0.382683,'\\0','2019-05-16 00:00:00'),(1879113729,7924,16842777,86,5,94,1,0,0,0,'\u{1}','2019-05-16 00:00:00');
CREATE TABLE `landblock_instance_link` (`id` int, `parent_GUID` int, `child_GUID` int, `last_Modified` datetime);
INSERT INTO `landblock_instance_link` VALUES (1,1879113728,1879113729,'2019-05-16 00:00:00');
";

/// The generator's guid and the instance it links, as an override file writes them.
const GENERATOR: &str = "0x70010000";
const CHILD: &str = "0x70010001";

#[test]
fn a_dump_says_its_instances_their_links_and_which_a_generator_owns() {
    let c = server_content(SERVER);
    assert_eq!(c.weenies.len(), 2);
    assert_eq!(c.weenies[&7924].setup, Some(SETUP));
    assert_eq!(c.instances.len(), 2);
    assert_eq!(c.instances[0].cell, 0x0101_0019);
    assert!(!c.instances[0].link_child && c.instances[1].link_child);
    assert_eq!(c.links, [(0x7001_0000, 0x7001_0001)]);
}

#[test]
fn the_servers_own_spawns_are_shown_and_a_move_or_a_removal_is_written_into_the_override_file() {
    let f = Fixture::new("server-spawns");
    let mut s = Session::open(&f.world(), &f.dats(), None).unwrap();
    s.set_today("2026-10-19 00:00:00");
    s.learn_weenies(SERVER);
    assert!(s.spawns().is_empty(), "none shown yet");
    assert_eq!(
        s.show_server_spawns([HOME]).into_iter().collect::<Vec<_>>(),
        [HOME]
    );
    assert!(s.show_server_spawns([HOME]).is_empty(), "shown already");
    let file = s.override_file(HOME);
    assert!(file.ends_with("server/sql/landblocks/0101 overrides.sql"));
    assert_eq!(s.override_landblock(&file), Some(HOME));
    let spawns = s.spawns();
    assert_eq!(spawns.len(), 2);
    assert!(spawns.iter().all(|x| x.server && x.file == file));
    assert_eq!(spawns[0].guid, GENERATOR);
    assert_eq!(spawns[0].setup, Some(dereth_primitives::DataId(SETUP)));
    // Moved, the generator is written into the file whole, with the link to its instance.
    s.apply(&Op::MoveSpawn {
        file: file.clone(),
        guid: GENERATOR.into(),
        origin: Vec3::new(90.0, 10.0, 94.0),
        heading: 90.0,
    })
    .unwrap();
    let text = std::fs::read_to_string(&file).ok();
    assert!(text.is_none(), "written when saved, not before");
    let spawns = s.spawns();
    let g = spawns.iter().find(|x| x.guid == GENERATOR).unwrap();
    assert!(!g.server && g.file == file);
    assert!((g.frame.origin.x - 90.0).abs() < 1e-3 && (g.frame.origin.y - 10.0).abs() < 1e-3);
    assert!(spawns.iter().any(|x| x.guid == CHILD && x.server));
    // Its instance taken away: deleted, and its link with it.
    s.apply(&Op::RemoveServerSpawn {
        file: file.clone(),
        guid: CHILD.into(),
    })
    .unwrap();
    assert!(!s.spawns().iter().any(|x| x.guid == CHILD));
    s.save().unwrap();
    let text = std::fs::read_to_string(&file).unwrap();
    for line in [
        "DELETE FROM `landblock_instance` WHERE `guid` = 0x70010000;",
        "VALUES (0x70010000, 1154, 0x01010019, 90.0, 10.0, 94.0,",
        "/* Generator */;",
        "VALUES (0x70010000, 0x70010001, '2026-10-19 00:00:00');",
        "DELETE FROM `landblock_instance` WHERE `guid` = 0x70010001;",
        "DELETE FROM `landblock_instance_link` WHERE `child_GUID` = 0x70010001;",
    ] {
        assert!(text.contains(line), "{line}\n{text}");
    }
    // Moved again, the file keeps the removal and the link.
    s.apply(&Op::MoveSpawn {
        file: file.clone(),
        guid: GENERATOR.into(),
        origin: Vec3::new(91.0, 10.0, 94.0),
        heading: 90.0,
    })
    .unwrap();
    let spawns = s.spawns();
    assert_eq!(spawns.len(), 1);
    assert!((spawns[0].frame.origin.x - 91.0).abs() < 1e-3);
    // Undone, the server's own are back as they were.
    for _ in 0..3 {
        s.undo().unwrap();
    }
    let spawns = s.spawns();
    assert_eq!(spawns.len(), 2);
    assert!(spawns.iter().all(|x| x.server));
    assert!((spawns[0].frame.origin.x - 84.0067).abs() < 1e-3);
    // A guid the server does not place is refused.
    assert!(s
        .apply(&Op::MoveSpawn {
            file,
            guid: "0x70010009".into(),
            origin: Vec3::new(1.0, 1.0, 1.0),
            heading: 0.0,
        })
        .is_err());
}

#[test]
fn the_servers_own_spawn_is_given_as_its_world_places_it_with_its_links_both_ways() {
    let f = Fixture::new("server-own-spawn");
    let mut s = Session::open(&f.world(), &f.dats(), None).unwrap();
    s.learn_weenies(SERVER);
    let g = s.server_own_spawn(HOME, 0x7001_0000).unwrap();
    assert_eq!(
        (g.instance.weenie.as_str(), g.instance.cell),
        ("1154", 0x0101_0019)
    );
    assert!((g.instance.origin[0] - 84.0067).abs() < 1e-4 && !g.instance.link_child);
    assert_eq!(g.children, [0x7001_0001]);
    assert!(g.parents.is_empty());
    let c = s.server_own_spawn(HOME, 0x7001_0001).unwrap();
    assert!(c.instance.link_child && c.children.is_empty());
    assert_eq!(c.parents, [0x7001_0000]);
    // Learned again, each link once.
    s.learn_weenies(SERVER);
    assert_eq!(
        s.server_own_spawn(HOME, 0x7001_0000).unwrap().children,
        [0x7001_0001]
    );
    // None it does not place, nor in another landblock.
    assert!(s.server_own_spawn(HOME, 0x7001_0009).is_none());
    assert!(s.server_own_spawn(0x0102, 0x7001_0000).is_none());
}

#[test]
fn a_server_generator_says_its_radius_and_the_instances_linked_to_it() {
    let f = Fixture::new("server-generator");
    let mut s = Session::open(&f.world(), &f.dats(), None).unwrap();
    s.learn_weenies(&format!(
        "{SERVER}\
CREATE TABLE `weenie_properties_float` (`id` int, `object_Id` int, `type` smallint, `value` double);
INSERT INTO `weenie_properties_float` VALUES (9,1154,43,12.5);
"
    ));
    s.show_server_spawns([HOME]);
    let spawns = s.spawns();
    let generator = spawns.iter().find(|x| x.guid == GENERATOR).unwrap();
    let g = s.generator(generator).expect("a generator");
    assert_eq!(g.radius, Some(12.5));
    assert_eq!(g.children, [CHILD]);
    assert!(
        g.profiles.is_empty(),
        "the server's own profiles are not read"
    );
    let child = spawns.iter().find(|x| x.guid == CHILD).unwrap();
    assert_eq!(s.generator(child), None, "the drudge makes nothing");
}

#[test]
fn a_server_weenie_appears_in_every_landblock_shown_or_not_but_where_a_pack_took_it_away() {
    use lyceum_foundry::edit::{Spawn, Weenie};
    let f = Fixture::new("appearances");
    let mut s = Session::open(&f.world(), &f.dats(), None).unwrap();
    s.set_today("2026-10-24 00:00:00");
    // The drudge in two landblocks more, one of them a dungeon's room, and a thousand of the
    // generator across the world.
    let (far, deep) = (0x0505u16, 0x0606u16);
    let row = |guid: u32, class: u32, cell: u32| {
        format!("({guid},{class},{cell},10,10,0,1,0,0,0,'\\0','2019-05-16 00:00:00')")
    };
    let mut rows = vec![
        row(0x7050_5000, 7924, 0x0505_0001),
        row(0x7060_6000, 7924, 0x0606_0100),
    ];
    rows.extend((0..1000u32).map(|k| row(0x7100_0000 + k, 1154, ((0x1000 + k) << 16) | 1)));
    s.learn_weenies(&format!(
        "{SERVER}INSERT INTO `landblock_instance` VALUES {};\n",
        rows.join(",")
    ));
    let drudge = Weenie::Class(7924);
    let lbs = |a: &[Spawn]| a.iter().map(Spawn::landblock).collect::<Vec<_>>();
    // None of its landblocks shown, every one of the server's is listed, and only its own are
    // read, not the world's.
    let read = s.server_spawns_read();
    let a = s.appearances(&drudge);
    assert_eq!(lbs(&a), [HOME, far, deep]);
    assert_eq!(s.server_spawns_read() - read, 3, "its own three read");
    assert!(a
        .iter()
        .all(|x| x.server && x.file == s.override_file(x.landblock())));
    assert_eq!(a[2].cell, 0x0606_0100);
    assert!(s.spawns().is_empty(), "none shown");
    // Found by its file and guid though its landblock is not shown.
    let found = s.spawn(&a[1].file, &a[1].guid).expect("the far one");
    assert_eq!((found.cell, found.server), (0x0505_0001, true));
    // The pack takes the far one away and writes the room's again, moved: the far one is gone,
    // the room's listed once, as the pack's.
    s.apply(&Op::RemoveServerSpawn {
        file: a[1].file.clone(),
        guid: a[1].guid.clone(),
    })
    .unwrap();
    s.apply(&Op::MoveSpawn {
        file: a[2].file.clone(),
        guid: a[2].guid.clone(),
        origin: Vec3::new(12.0, 10.0, 0.0),
        heading: 0.0,
    })
    .unwrap();
    // And places one of its own in another landblock.
    s.apply(&Op::PlaceSpawn {
        landblock: 0x0102,
        weenie: drudge.clone(),
        origin: Vec3::new(20.0, 20.0, 94.0),
        heading: 0.0,
    })
    .unwrap();
    let a = s.appearances(&drudge);
    assert_eq!(lbs(&a), [HOME, 0x0102, deep]);
    assert!(a[0].server && !a[1].server && !a[2].server);
    assert!((a[2].frame.origin.x - 12.0).abs() < 1e-3);
    assert_eq!(
        s.spawn(&a[1].file, &a[1].guid).map(|x| x.cell),
        Some(a[1].cell)
    );
    let gone = s.override_file(far);
    assert!(s.spawn(&gone, "0x70505000").is_none(), "taken away");
    // The generator's thousand are its own.
    assert_eq!(s.appearances(&Weenie::Class(1154)).len(), 1001);
}

#[test]
fn a_weenie_stands_in_the_landblocks_of_the_servers_spawns_of_it_and_the_packs() {
    let f = Fixture::new("standing");
    let mut s = Session::open(&f.world(), &f.dats(), None).unwrap();
    s.set_today("2026-10-19 00:00:00");
    s.learn_weenies(SERVER);
    let set = |c: &[u32]| {
        c.iter()
            .copied()
            .collect::<std::collections::BTreeSet<u32>>()
    };
    let lbs = |l: &[u16]| {
        l.iter()
            .copied()
            .collect::<std::collections::BTreeSet<u16>>()
    };
    // Every one of the server's, shown or not.
    assert_eq!(s.landblocks_standing(&set(&[7924])), lbs(&[HOME]));
    assert_eq!(s.landblocks_standing(&set(&[1])), lbs(&[]));
    // A pack's, as compiled.
    s.apply(&Op::PlaceSpawn {
        landblock: 0x0102,
        weenie: lyceum_foundry::edit::Weenie::Class(7924),
        origin: Vec3::new(20.0, 20.0, 94.0),
        heading: 0.0,
    })
    .unwrap();
    assert_eq!(s.landblocks_standing(&set(&[7924])), lbs(&[HOME, 0x0102]));
}

#[test]
fn who_names_a_weenie_is_the_servers_own_and_the_packs_and_a_packs_change_stands_for_the_servers() {
    use lyceum_foundry::edit::Weenie;
    use lyceum_foundry::sql_ids::Use;
    let f = Fixture::new("server-uses");
    // The server's generator makes its drudge, and its body parts are learned with it.
    let mut s = Session::open(&f.world(), &f.dats(), None).unwrap();
    s.learn_weenies(&format!(
        "{SERVER}\
CREATE TABLE `weenie_properties_generator` (`id` int, `object_Id` int, `probability` float, `weenie_Class_Id` int);
INSERT INTO `weenie_properties_generator` VALUES (1,1154,-1,7924);
CREATE TABLE `weenie_properties_body_part` (`id` int, `object_Id` int, `key` smallint, `d_Type` int, `d_Val` int);
INSERT INTO `weenie_properties_body_part` VALUES (1,7924,0,4,3);
"
    ));
    let drudge = Weenie::Class(7924);
    assert_eq!(s.uses_of(&drudge), [(Weenie::Class(1154), Use::Generator)]);
    assert!(s
        .typical_body_part(0)
        .is_some_and(|r| r.iter().any(|(k, v)| k == "d_Val" && v == "3")));
    assert!(s.typical_body_part(1).is_none());
    // A pack's weenie wields it; the pack's change of the generator makes it no more.
    let (file, _) = s.new_weenie_file("guard").unwrap();
    s.apply(&Op::WriteSql {
        file,
        text: Some("DELETE FROM `weenie` WHERE `class_Id` = {{wcid:guard}};\nINSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)\nVALUES ({{wcid:guard}}, 'guard', 10, '2005-02-09 10:00:00');\nINSERT INTO `weenie_properties_create_list` (`object_Id`, `destination_Type`, `weenie_Class_Id`, `stack_Size`, `palette`, `shade`, `try_To_Bond`)\nVALUES ({{wcid:guard}}, 2, 7924, 1, 0, 0, False);\n".into()),
    })
    .unwrap();
    s.apply(&Op::WriteSql {
        file: s.class_file(1154, "gen"),
        text: Some("DELETE FROM `weenie` WHERE `class_Id` = 1154;\nINSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)\nVALUES (1154, 'gen', 1, '2005-02-09 10:00:00');\n".into()),
    })
    .unwrap();
    assert_eq!(
        s.uses_of(&drudge),
        [(
            Weenie::Symbol("sample:guard".into()),
            Use::CreateList { destination: 2 }
        )]
    );
}
