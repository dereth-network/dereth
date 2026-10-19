//! Behaviour: none (tooling: the data ids server SQL names, against a small set of files)

use std::path::Path;

use lyceum_foundry::compile;
use lyceum_foundry::dats::DatSet;
use lyceum_foundry::sql_ids::{check, named_ids};
use lyceum_foundry::Error;

use crate::common::Fixture;

const WEENIE: &str = "\
INSERT INTO `weenie_properties_d_i_d` (`object_Id`, `type`, `value`)
VALUES ({{wcid:npc}},   1, 0x02000001) /* Setup */
     , ({{wcid:npc}},   2, 0x09000001) /* MotionTable */
     , ({{wcid:npc}},   6, 0x02000001) /* PaletteBase */
     , ({{wcid:npc}},  20, 0x41000003) /* InitMotion */
     , ({{wcid:npc}},   8, 0) /* Icon */;

INSERT INTO `weenie_properties_anim_part` (`object_Id`, `index`, `animation_Id`)
VALUES ({{wcid:npc}},  3, 0x01000001)
     , ({{wcid:npc}},  6, 0x01000002);

INSERT INTO `weenie_properties_texture_map` (`object_Id`, `index`, `old_Id`, `new_Id`)
VALUES ({{wcid:npc}}, 3, 0x05000010, 0x05000011);
";

const SPAWN: &str = "\
DELETE FROM `landblock_instance` WHERE `guid` = {{guid:npc@0101}};

INSERT INTO `landblock_instance` (`guid`, `weenie_Class_Id`, `obj_Cell_Id`, `origin_X`, `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`, `is_Link_Child`, `last_Modified`)
VALUES ({{guid:npc@0101}}, {{wcid:npc}}, 0x01010009, 36, 18, 5, 1, 0, 0, 0, False, '2026-10-18 00:00:00');

INSERT INTO `landblock_instance` (`guid`, `weenie_Class_Id`, `obj_Cell_Id`, `origin_X`, `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`, `is_Link_Child`, `last_Modified`)
VALUES ({{guid:far@0505}}, {{wcid:npc}}, 0x05050009, 36, 18, 5, 1, 0, 0, 0, False, '2026-10-18 00:00:00');

INSERT INTO `landblock_instance` (`guid`, `weenie_Class_Id`, `obj_Cell_Id`, `origin_X`, `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`, `is_Link_Child`, `last_Modified`)
VALUES ({{guid:open@0101}}, {{wcid:npc}}, 0x01010000, 100, 18, 5, 1, 0, 0, 0, False, '2026-10-18 00:00:00');
";

#[test]
fn every_data_id_column_is_read_and_numbers_that_name_no_record_are_skipped() {
    let named = named_ids(Path::new("w.sql"), WEENIE);
    let ids: Vec<(u32, usize)> = named.iter().map(|n| (n.id, n.at.line)).collect();
    // The setup, the motion table, the palette base, two parts and the texture change's two
    // textures; the motion command and the zero icon name nothing.
    assert_eq!(
        ids,
        vec![
            (0x0200_0001, 2),
            (0x0900_0001, 3),
            (0x0200_0001, 4),
            (0x0100_0001, 9),
            (0x0100_0002, 10),
            (0x0500_0010, 13),
            (0x0500_0011, 13),
        ]
    );
}

#[test]
fn ids_the_files_do_not_hold_or_of_the_wrong_kind_are_faults_at_their_line() {
    let f = Fixture::new("pack-sql-ids");
    let set = DatSet::open(&f.dats()).unwrap();
    let faults = check(&set, &named_ids(Path::new("w.sql"), WEENIE));
    let has = |s: &str| faults.iter().any(|p| p.contains(s));
    assert!(
        has("w.sql:3: weenie_properties_d_i_d type 2 names motion table 0x09000001, which the files do not hold"),
        "{faults:?}"
    );
    assert!(
        has("w.sql:4: weenie_properties_d_i_d type 6 names 0x02000001, which is not a palette"),
        "{faults:?}"
    );
    assert!(
        has("w.sql:10: weenie_properties_anim_part animation_Id names GfxObj 0x01000002"),
        "{faults:?}"
    );
    assert!(
        has("w.sql:13: weenie_properties_texture_map new_Id names image texture 0x05000011"),
        "{faults:?}"
    );
    // The setup, the first part and the old texture are in the files.
    assert_eq!(faults.len(), 4, "{faults:?}");
}

#[test]
fn a_dump_is_read_by_the_columns_of_its_own_table_definition() {
    let dump = "\
CREATE TABLE `weenie_properties_d_i_d` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT COMMENT 'Unique Id, of this Property',
  `object_Id` int(10) unsigned NOT NULL,
  `type` smallint(5) unsigned NOT NULL,
  `value` int(10) unsigned NOT NULL,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB;
INSERT INTO `weenie_properties_d_i_d` VALUES (1,4,1,33554433),(2,4,2,150994945),(3,4,21,7);
";
    let named = named_ids(Path::new("dump.sql"), dump);
    let ids: Vec<u32> = named.iter().map(|n| n.id).collect();
    assert_eq!(ids, vec![0x0200_0001, 0x0900_0001]);
}

#[test]
fn a_spawn_in_a_cell_the_files_do_not_hold_stops_the_build() {
    let f = Fixture::new("pack-sql-cells");
    let world = f.world();
    f.write("sample/server/sql/spawn.sql", SPAWN);
    let e = compile::build(&world, &f.dats(), None, false).unwrap_err();
    let Error::Invalid(faults) = e else {
        panic!("{e}")
    };
    // The instances in 0x0101 stand on a landblock the files hold (cell 0 is an outdoor place
    // whose cell the server works out); 0x0505 is not there.
    assert_eq!(faults.len(), 1, "{faults:?}");
    assert!(
        faults[0].ends_with(
            "spawn.sql:7: landblock_instance obj_Cell_Id names cell 0x05050009, which the files do not hold"
        ),
        "{faults:?}"
    );
}
