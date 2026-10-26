//! Behaviour: none (tooling: a pack's own weenie deleted in the world editor once nothing names it)

use dereth_primitives::Vec3;
use lyceum_foundry::edit::{Op, Session, Weenie};
use lyceum_foundry::loot::LootKind;
use lyceum_foundry::sql_ids::Use;

use crate::common::{Fixture, HOME};

/// The server's own: a drudge.
const SERVER: &str = "\
CREATE TABLE `weenie` (`class_Id` int, `class_Name` varchar(100), `type` int, `last_Modified` datetime);
INSERT INTO `weenie` VALUES (7924,'testdrudge',10,'2005-02-09 10:00:00');
";

/// A pack weenie of `name` (a creature) with `rows` after its `weenie` row.
fn weenie(name: &str, rows: &str) -> String {
    format!(
        "DELETE FROM `weenie` WHERE `class_Id` = {{{{wcid:{name}}}}};\n\n\
         INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)\n\
         VALUES ({{{{wcid:{name}}}}}, '{name}', 10, '2026-10-25 00:00:00');\n\n{rows}"
    )
}

/// The pack's sword, guard (who wields the sword) and lone (whose emote gives itself), over the
/// server's drudge.
fn world(tag: &str) -> (Fixture, Session) {
    let f = Fixture::new(tag);
    let world = f.world();
    f.write("sample/server/sql/weenies/sword.sql", &weenie("sword", ""));
    f.write(
        "sample/server/sql/weenies/guard.sql",
        &weenie(
            "guard",
            "INSERT INTO `weenie_properties_create_list` (`object_Id`, `destination_Type`, `weenie_Class_Id`, `stack_Size`, `palette`, `shade`, `try_To_Bond`)
VALUES ({{wcid:guard}}, 2, {{wcid:sword}}, 1, 0, 0, False);
",
        ),
    );
    f.write(
        "sample/server/sql/weenies/lone.sql",
        &weenie(
            "lone",
            "INSERT INTO `weenie_properties_emote` (`object_Id`, `category`, `probability`, `weenie_Class_Id`, `style`, `substyle`, `quest`, `vendor_Type`, `min_Health`, `max_Health`)
VALUES ({{wcid:lone}}, 7, 1, {{wcid:lone}}, NULL, NULL, NULL, NULL, NULL, NULL);
",
        ),
    );
    let mut s = Session::open(&world, &f.dats(), None).unwrap();
    s.set_today("2026-10-25 00:00:00");
    s.learn_weenies(SERVER);
    (f, s)
}

fn sym(name: &str) -> Weenie {
    Weenie::Symbol(format!("sample:{name}"))
}

#[test]
fn a_weenie_nothing_names_is_deleted_as_one_step_and_undo_brings_it_back() {
    let (_f, mut s) = world("delete-weenie");
    // Its own emote naming itself does not keep it.
    let lone = sym("lone");
    assert!(s.named_anywhere(&lone).is_empty());
    let (file, mine) = s.weenie_file(&lone).unwrap();
    assert!(mine);
    let steps = s.history().0;
    let op = s.weenie_removal(&lone).unwrap();
    assert_eq!(
        op,
        Op::WriteSql {
            file: file.clone(),
            text: None
        }
    );
    s.apply(&op).unwrap();
    assert_eq!(s.history().0, steps + 1);
    assert!(s.weenie_file(&lone).is_none());
    assert!(!s.pack_weenies().contains_key("sample:lone"));
    s.undo().unwrap();
    assert_eq!(s.weenie_file(&lone).map(|(f, _)| f), Some(file));
}

#[test]
fn a_weenie_still_named_anywhere_is_not_deleted_and_each_place_is_listed() {
    let (_f, mut s) = world("delete-weenie-used");
    let sword = sym("sword");
    // The guard wields it.
    let named = s.named_anywhere(&sword);
    assert_eq!(
        named.by,
        [(sym("guard"), Use::CreateList { destination: 2 })]
    );
    assert!(named.spawns.is_empty() && named.loot.is_empty());
    // Placed in the world, and held by a wielded loot table, too.
    s.apply(&Op::PlaceSpawn {
        landblock: HOME,
        weenie: sword.clone(),
        origin: Vec3::new(20.0, 20.0, 94.0),
        heading: 0.0,
    })
    .unwrap();
    let (op, _) = s
        .loot_new(
            LootKind::Wielded,
            None,
            Some("{{wcid:sword}}"),
            Some("guard-kit"),
        )
        .unwrap();
    s.apply(&op).unwrap();
    let named = s.named_anywhere(&sword);
    assert_eq!(named.spawns.len(), 1);
    assert_eq!(named.loot.len(), 1);
    assert_eq!(named.places(), 3);
    let steps = s.history().0;
    let e = s.weenie_removal(&sword).unwrap_err().to_string();
    assert!(e.contains("still used in 3 place(s)"), "{e}");
    assert_eq!(s.history().0, steps, "nothing changed");
    assert!(s.weenie_file(&sword).is_some());
}

#[test]
fn the_servers_weenies_and_a_packs_change_of_one_are_not_deleted() {
    let (f, mut s) = world("delete-weenie-server");
    let e = s
        .weenie_removal(&Weenie::Class(7924))
        .unwrap_err()
        .to_string();
    assert!(e.contains("the server's weenies are not deleted"), "{e}");
    // The pack's change of the drudge is put back, not deleted.
    let file = s.class_file(7924, "Test Drudge");
    s.apply(&Op::WriteSql {
        file,
        text: Some(
            "DELETE FROM `weenie` WHERE `class_Id` = 7924;\n\nINSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)\nVALUES (7924, 'testdrudge', 10, '2026-10-25 00:00:00');\n".into(),
        ),
    })
    .unwrap();
    assert!(s.weenie_file(&Weenie::Class(7924)).is_some_and(|(_, m)| m));
    assert!(s.weenie_removal(&Weenie::Class(7924)).is_err());
    // A weenie no pack writes is no pack's to delete.
    let e = s.weenie_removal(&sym("ghost")).unwrap_err().to_string();
    assert!(e.contains("no file of the packs writes"), "{e}");
    drop(f);
}

#[test]
fn what_the_packs_sql_says_of_their_weenies_is_read_again_after_each_change_and_its_undo() {
    let (_f, mut s) = world("pack-facts-again");
    let (sword, _) = s.weenie_file(&sym("sword")).unwrap();
    assert_eq!(s.class_facts(7924).and_then(|f| f.name), None);
    // A copy of the server's drudge in the pack, named: what the pack says of it, at once.
    let copy = sword.with_file_name("testdrudge.sql");
    let named = "INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)\nVALUES (7924, 'testdrudge', 10, '2026-10-25 00:00:00');\n\nINSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)\nVALUES (7924, 1, 'Pack Drudge');\n";
    s.apply(&Op::WriteSql {
        file: copy,
        text: Some(named.to_owned()),
    })
    .unwrap();
    assert_eq!(
        s.class_facts(7924).and_then(|f| f.name).as_deref(),
        Some("Pack Drudge")
    );
    assert!(s.pack_classes().contains_key(&7924));
    // Undone: the server's again.
    s.undo().unwrap();
    assert_eq!(s.class_facts(7924).and_then(|f| f.name), None);
    assert!(!s.pack_classes().contains_key(&7924));
    // A pack weenie taken away and brought back: gone, then there again.
    let op = s.weenie_removal(&sym("lone")).unwrap();
    s.apply(&op).unwrap();
    assert!(!s.pack_weenies().contains_key("sample:lone"));
    s.undo().unwrap();
    assert!(s.pack_weenies().contains_key("sample:lone"));
}
