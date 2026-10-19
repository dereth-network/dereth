//! Behaviour: none (tooling: the server's own spawns in the world editor: shown, moved and taken away through a pack's override file)

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
