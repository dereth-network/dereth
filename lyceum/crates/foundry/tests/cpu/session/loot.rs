//! Behaviour: none (tooling: the world's loot tables for Lyceum's Loot tab: where each is used, and one copied into the pack, put back, made, changed or taken away)

use lyceum_foundry::edit::{Session, Weenie};
use lyceum_foundry::loot::{LootKind, LootRows, Whose, NEW_ID};
use lyceum_foundry::sql_ids::{LootUse, Use};
use lyceum_foundry::wielded;

use crate::common::Fixture;

/// The server's: a drudge that drops death treasure 4 and is made wielding 37; a generator that
/// makes a drudge and, as treasure, table 4 and table 37; a chest whose generator makes table 7;
/// death treasures 4 and 7; and wielded treasure 37, a club or nothing, with a sub-set.
const SERVER: &str = "\
CREATE TABLE `weenie` (`class_Id` int, `class_Name` varchar(100), `type` int, `last_Modified` datetime);
INSERT INTO `weenie` VALUES (9001,'drudge',10,'2005-02-09 10:00:00'),(9002,'drudgegen',1,'2005-02-09 10:00:00'),(9003,'chest',20,'2005-02-09 10:00:00'),(309,'club',6,'2005-02-09 10:00:00'),(310,'dart',3,'2005-02-09 10:00:00');
CREATE TABLE `weenie_properties_string` (`id` int, `object_Id` int, `type` smallint, `value` text);
INSERT INTO `weenie_properties_string` VALUES (1,9001,1,'Drudge'),(2,9002,1,'Drudge Generator'),(3,9003,1,'Chest'),(4,309,1,'Club'),(5,310,1,'Dart');
CREATE TABLE `weenie_properties_d_i_d` (`id` int, `object_Id` int, `type` smallint, `value` int);
INSERT INTO `weenie_properties_d_i_d` VALUES (1,9001,35,4),(2,9001,32,37);
CREATE TABLE `weenie_properties_generator` (`id` int, `object_Id` int, `probability` float, `weenie_Class_Id` int, `delay` float, `init_Create` int, `max_Create` int, `when_Create` int, `where_Create` int);
INSERT INTO `weenie_properties_generator` VALUES (1,9002,0.5,9001,180,1,1,1,2),(2,9002,1,4,180,1,1,1,72),(3,9002,-1,37,180,1,1,1,80),(4,9003,-1,7,0,1,1,2,72);
CREATE TABLE `treasure_death` (`id` int, `treasure_Type` int, `tier` int, `loot_Quality_Mod` float, `unknown_Chances` int, `item_Chance` int, `item_Min_Amount` int, `item_Max_Amount` int, `item_Treasure_Type_Selection_Chances` int, `magic_Item_Chance` int, `magic_Item_Min_Amount` int, `magic_Item_Max_Amount` int, `magic_Item_Treasure_Type_Selection_Chances` int, `mundane_Item_Chance` int, `mundane_Item_Min_Amount` int, `mundane_Item_Max_Amount` int, `mundane_Item_Type_Selection_Chances` int, `last_Modified` datetime);
INSERT INTO `treasure_death` VALUES (4,4,2,0,19,100,1,2,10,100,2,3,9,100,0,0,1,'2005-02-09 10:00:00'),(7,7,4,0.25,19,100,0,0,1,100,3,5,9,100,0,0,1,'2005-02-09 10:00:00');
CREATE TABLE `treasure_wielded` (`id` int, `treasure_Type` int, `weenie_Class_Id` int, `palette_Id` int, `unknown_1` int, `shade` float, `stack_Size` int, `stack_Size_Variance` float, `probability` float, `unknown_3` int, `unknown_4` int, `unknown_5` int, `set_Start` bit(1), `has_Sub_Set` bit(1), `continues_Previous_Set` bit(1), `unknown_9` int, `unknown_10` int, `unknown_11` int, `unknown_12` int, `last_Modified` datetime);
INSERT INTO `treasure_wielded` VALUES (1,37,309,0,0,0,0,0,0.6,0,0,0,'\x01','\x01','\\0',0,0,0,0,'2005-02-09 10:00:00'),(2,37,310,0,0,0,10,0.2,1,0,0,0,'\x01','\\0','\\0',0,0,0,0,'2005-02-09 10:00:00'),(3,37,310,0,0,0,5,0,0.25,0,0,0,'\x01','\\0','\x01',0,0,0,0,'2005-02-09 10:00:00');
";

/// A pack weenie of `name` (a creature) with `rows` after its `weenie` row.
fn weenie(name: &str, rows: &str) -> String {
    format!(
        "DELETE FROM `weenie` WHERE `class_Id` = {{{{wcid:{name}}}}};\n\n\
         INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)\n\
         VALUES ({{{{wcid:{name}}}}}, '{name}', 10, '2026-10-25 00:00:00');\n\n{rows}"
    )
}

/// The world: the pack's drudge, dropping the server's death treasure 7; and the server's
/// database.
fn world(tag: &str) -> (Fixture, Session) {
    let f = Fixture::new(tag);
    let world = f.world();
    f.write(
        "sample/server/sql/weenies/rat.sql",
        &weenie(
            "rat",
            "INSERT INTO `weenie_properties_d_i_d` (`object_Id`, `type`, `value`)
VALUES ({{wcid:rat}},  35,          7) /* DeathTreasureType */;
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
fn every_table_is_listed_with_each_weenie_that_names_it_by_data_id_or_generator_treasure() {
    let (_f, s) = world("loot-uses");
    let all: Vec<(LootKind, u32, Whose)> = s
        .world_loot()
        .iter()
        .map(|t| (t.kind, t.id, t.whose))
        .collect();
    assert_eq!(
        all,
        [
            (LootKind::Death, 4, Whose::Server),
            (LootKind::Death, 7, Whose::Server),
            (LootKind::Wielded, 37, Whose::Server),
        ]
    );
    let uses = |kind, id| -> Vec<(Weenie, LootUse)> {
        s.loot_table(kind, id)
            .unwrap()
            .uses
            .into_iter()
            .map(|u| (u.by, u.how))
            .collect()
    };
    assert_eq!(
        uses(LootKind::Death, 4),
        [
            (Weenie::Class(9001), LootUse::Dies),
            (Weenie::Class(9002), LootUse::Generator { place: 72 }),
        ]
    );
    // A generator's treasure entry names the wielded table when no death table has its id.
    assert_eq!(
        uses(LootKind::Wielded, 37),
        [
            (Weenie::Class(9001), LootUse::Wields),
            (Weenie::Class(9002), LootUse::Generator { place: 80 }),
        ]
    );
    assert_eq!(
        uses(LootKind::Death, 7),
        [
            (sym("rat"), LootUse::Dies),
            (Weenie::Class(9003), LootUse::Generator { place: 72 }),
        ]
    );
    assert_eq!(s.loot_kind_of(37), LootKind::Wielded);
    assert_eq!(s.loot_kind_of(4), LootKind::Death);
    // A treasure entry is no use of the weenie whose class id is the table's: the generator
    // names the drudge, and only as an ordinary entry.
    assert!(s.uses_of(&Weenie::Class(4)).is_empty());
    assert_eq!(
        s.uses_of(&Weenie::Class(9001)),
        [(Weenie::Class(9002), Use::Generator)]
    );
    // The server's wielded table, read as the server draws it: the club 60% with its darts
    // always, then darts a quarter of the time.
    let rows = s.loot_table(LootKind::Wielded, 37).unwrap();
    let w = wielded::read(rows.wielded().unwrap());
    assert_eq!(w.sets.len(), 2);
    assert!((w.sets[0].entries[0].chance - 0.6).abs() < 1e-6);
    assert_eq!(w.sets[0].entries[0].then[0].entries[0].item.stack, 10);
    assert!((w.sets[1].nothing() - 0.75).abs() < 1e-6);
}

#[test]
fn a_servers_table_is_copied_into_the_pack_changed_field_by_field_and_put_back_each_as_one_step() {
    let (_f, mut s) = world("loot-change");
    let op = s.loot_change_here(LootKind::Death, 4).unwrap();
    s.apply(&op).unwrap();
    let t = s.loot_table(LootKind::Death, 4).unwrap();
    assert_eq!(t.whose, Whose::Changed { mine: true });
    assert_eq!(t.rows, t.server, "as the server has it");
    assert!(t
        .file
        .as_ref()
        .unwrap()
        .ends_with("server/sql/treasure_death/00004.sql"));
    assert!(s.loot_change_here(LootKind::Death, 4).is_err(), "once");
    assert_eq!(t.places(), 2, "the server's weenies name it still");
    // Its fields written: each as typed, a tier, a chance, a mix.
    for (column, value) in [
        ("tier", "5"),
        ("magic_Item_Chance", "40"),
        ("item_Treasure_Type_Selection_Chances", "8"),
        ("loot_Quality_Mod", "0.5"),
    ] {
        let op = s.death_set(4, column, value).unwrap();
        s.apply(&op).unwrap();
    }
    let now = s.treasure(4).unwrap();
    assert_eq!(
        (now.tier, now.magic.0, now.kinds.0, now.quality),
        (5, 40, 8, 0.5)
    );
    assert_eq!(s.history().0, 5, "one step each");
    // Not a value of it, or a least past its most: refused, nothing written.
    assert!(s.death_set(4, "tier", "9").is_err());
    assert!(s.death_set(4, "item_Chance", "fifty").is_err());
    assert!(s.death_set(4, "item_Min_Amount", "3").is_err(), "most is 2");
    assert!(s.death_set(4, "mundane_Item_Max_Amount", "2").is_ok());
    // The server's own is not the pack's to change.
    assert!(s.death_set(7, "tier", "1").is_err());
    // Put back: the server's stands again.
    let op = s.loot_take_away(LootKind::Death, 4, true).unwrap();
    s.apply(&op).unwrap();
    let back = s.loot_table(LootKind::Death, 4).unwrap();
    assert_eq!(back.whose, Whose::Server);
    assert_eq!(s.treasure(4).unwrap().tier, 2);
    s.undo().unwrap();
    assert_eq!(s.treasure(4).unwrap().tier, 5);
}

#[test]
fn a_new_table_is_named_by_a_symbol_numbered_by_the_build_and_deleted_only_once_nothing_names_it() {
    let (f, mut s) = world("loot-new");
    // Plain, unnamed: the first free name, numbered from NEW_ID up, its token in its own file.
    let (op, first) = s.loot_new(LootKind::Death, None, None, None).unwrap();
    assert_eq!(first, "sample:loot-1");
    s.apply(&op).unwrap();
    let id = s.treasure_id(&first).expect("numbered");
    assert_eq!(id, NEW_ID);
    let t = s.loot_table(LootKind::Death, id).unwrap();
    assert_eq!(t.whose, Whose::Pack { mine: true });
    assert_eq!(t.symbol.as_deref(), Some("{{treasure:loot-1}}"));
    // A weenie of the pack names it by its token; a server's table by its id.
    assert_eq!(s.loot_written(&t), "{{treasure:loot-1}}");
    let seven = s.loot_table(LootKind::Death, 7).unwrap();
    assert_eq!(s.loot_written(&seven), "7");
    assert_eq!(t.death().unwrap().tier, 1);
    let file = t.file.clone().unwrap();
    assert!(file.ends_with("server/sql/treasure_death/loot-1.sql"));
    assert!(s
        .sql_text(&file)
        .unwrap()
        .contains("WHERE `treasure_Type` = {{treasure:loot-1}};"));
    // Its fields written through its token.
    s.apply(&s.death_set(id, "tier", "3").unwrap()).unwrap();
    assert_eq!(s.treasure(id).unwrap().tier, 3);
    // A copy of the server's 7, named: the next id, of either kind.
    let (op, copy) = s
        .loot_new(LootKind::Death, Some(7), None, Some("rich"))
        .unwrap();
    s.apply(&op).unwrap();
    let copy = s.treasure_id(&copy).unwrap();
    assert_eq!(copy, NEW_ID + 1);
    assert_eq!(s.treasure(copy), s.treasure(7));
    assert!(
        s.loot_new(LootKind::Wielded, None, Some("309"), Some("rich"))
            .is_err(),
        "a name a table has"
    );
    assert!(s
        .loot_new(LootKind::Death, None, None, Some("Not A Name"))
        .is_err());
    let (op, w) = s
        .loot_new(LootKind::Wielded, None, Some("{{wcid:rat}}"), None)
        .unwrap();
    assert_eq!(w, "sample:loot-2");
    s.apply(&op).unwrap();
    let w = s.treasure_id(&w).unwrap();
    assert_eq!(w, NEW_ID + 2);
    // Its sets written back through its token.
    let rows = s.wielded_rows(w).unwrap();
    let mut tree = wielded::read(&rows);
    tree.set_chance(&[(0, 0)], 50.0).unwrap();
    s.apply(&s.wielded_set(w, &wielded::write(&tree)).unwrap())
        .unwrap();
    assert!((s.wielded_rows(w).unwrap()[0].probability - 0.5).abs() < 1e-6);
    assert!(
        s.loot_new(LootKind::Wielded, None, None, None).is_err(),
        "it starts with something"
    );
    // The rat dropping it by its token: not deleted.
    let rat = f.path().join("sample/server/sql/weenies/rat.sql");
    let text = s.sql_text(&rat).unwrap();
    let named = text.replace("35,          7)", "35,          {{treasure:loot-1}})");
    s.apply(&lyceum_foundry::edit::Op::WriteSql {
        file: rat.clone(),
        text: Some(named),
    })
    .unwrap();
    let t = s.loot_table(LootKind::Death, id).unwrap();
    assert_eq!(t.places(), 1);
    assert_eq!(t.uses[0].by, sym("rat"));
    assert!(s.loot_take_away(LootKind::Death, id, false).is_err());
    // The build writes its number where the token was.
    let filled = s
        .compiled()
        .sql
        .iter()
        .find(|(p, _)| p.ends_with("weenies/rat.sql"))
        .map(|(_, t)| t.clone())
        .unwrap();
    assert!(filled.contains(&format!("35,          {id})")), "{filled}");
    s.apply(&lyceum_foundry::edit::Op::WriteSql {
        file: rat,
        text: Some(text),
    })
    .unwrap();
    let op = s.loot_take_away(LootKind::Death, id, false).unwrap();
    s.apply(&op).unwrap();
    assert!(s.loot_table(LootKind::Death, id).is_none());
    // A server's table is not deleted, only put back.
    assert!(s.loot_take_away(LootKind::Death, 7, false).is_err());
}

#[test]
fn a_wielded_tables_sets_are_written_back_as_edited_and_read_as_the_server_draws_them() {
    let (_f, mut s) = world("loot-wielded");
    s.apply(&s.loot_change_here(LootKind::Wielded, 37).unwrap())
        .unwrap();
    let rows = s
        .loot_table(LootKind::Wielded, 37)
        .unwrap()
        .wielded()
        .unwrap()
        .to_vec();
    let mut w = wielded::read(&rows);
    // The club 80%, darts in the second set half the time.
    w.set_chance(&[(0, 0)], 80.0).unwrap();
    w.set_chance(&[(1, 0)], 50.0).unwrap();
    let op = s.wielded_set(37, &wielded::write(&w)).unwrap();
    s.apply(&op).unwrap();
    let t = s.loot_table(LootKind::Wielded, 37).unwrap();
    let again = wielded::read(t.wielded().unwrap());
    assert!((again.sets[0].entries[0].chance - 0.8).abs() < 1e-6);
    assert!((again.sets[1].entries[0].chance - 0.5).abs() < 1e-6);
    assert_eq!(again.sets[0].entries[0].then.len(), 1, "its sub-set kept");
    match &t.server {
        Some(LootRows::Wielded(server)) => assert_eq!(server, &rows),
        other => panic!("{other:?}"),
    }
    assert!(s.wielded_set(37, &[]).is_err(), "empty: deleted instead");
}
