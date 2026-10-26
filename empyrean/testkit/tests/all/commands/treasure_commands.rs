//! Divergence: V460
//! The console's `import-treasure` brings a loot table's file from the content folder into the
//! running world, which reads it afresh; `roll-treasure` draws a table and says what each draw
//! made, and nothing it makes is left in the world.
//! Fixture: a virtual-time TestServer over the content fixture dump with an overlay, a statue
//! weenie, and loot tables written into its content folder.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use empyrean_command::command_manager;
use empyrean_command::empyrean::treasure;
use empyrean_content::import::patch::{sources, Input, InputKind};
use empyrean_content::import::{build_from, default_now};
use empyrean_content::overlay::{BaseInputs, ContentOverlay};
use empyrean_content::pack::Pack;
use empyrean_content::PackContent;
use empyrean_testkit::{EmptyShard, TestServer};
use empyrean_world::managers::{guid_manager, property_manager};

const STATUE: u32 = 7101;

fn base_sql() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../crates/content/tests/fixtures/patches/base.sql")
}

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "empyrean-testkit-treasure-commands-{}-{name}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("content")).unwrap();
    d
}

/// The world database: the fixture dump with a statue, and an overlay at `dir/overlay.sqlite`.
fn world_database(dir: &Path) -> PackContent {
    let setup = dir.join("setup.sql");
    std::fs::write(
        &setup,
        format!(
            "INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)\n\
             VALUES ({STATUE}, 'lootstatue', 1, '2021-11-01 00:00:00');\n\
             INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)\n\
             VALUES ({STATUE}, 1, 'Loot Statue');\n"
        ),
    )
    .unwrap();
    let patches = vec![Input {
        kind: InputKind::Sql,
        path: setup,
    }];
    let dump = std::fs::File::open(base_sql()).unwrap();
    let (bytes, _) = build_from(dump, sources(&patches), default_now()).unwrap();
    let db = PackContent::new(Pack::from_bytes(bytes).unwrap());
    let overlay = ContentOverlay::open(
        &dir.join("overlay.sqlite"),
        BaseInputs {
            sql: base_sql(),
            patches,
            era: empyrean_common::era::EraId::Eor,
        },
        db.base().pack().header(),
        default_now(),
    )
    .unwrap()
    .shared();
    db.base().attach_overlay(overlay).unwrap();
    db
}

/// Death treasure `id` of tier `tier`, which drops nothing, as its file writes it.
fn death(id: u32, tier: i32) -> String {
    format!(
        "DELETE FROM `treasure_death` WHERE `treasure_Type` = {id};\n\n\
         INSERT INTO `treasure_death` (`treasure_Type`, `tier`, `loot_Quality_Mod`, `unknown_Chances`, `item_Chance`, `item_Min_Amount`, `item_Max_Amount`, `item_Treasure_Type_Selection_Chances`, `magic_Item_Chance`, `magic_Item_Min_Amount`, `magic_Item_Max_Amount`, `magic_Item_Treasure_Type_Selection_Chances`, `mundane_Item_Chance`, `mundane_Item_Min_Amount`, `mundane_Item_Max_Amount`, `mundane_Item_Type_Selection_Chances`, `last_Modified`)\n\
         VALUES ({id}, {tier}, 0, 19, 0, 0, 0, 9, 0, 0, 0, 8, 0, 0, 0, 7, '2026-10-25 00:00:00');\n"
    )
}

/// Wielded treasure `id`: the statue always, and a stack of three of it half the time.
fn wielded(id: u32) -> String {
    format!(
        "DELETE FROM `treasure_wielded` WHERE `treasure_Type` = {id};\n\n\
         INSERT INTO `treasure_wielded` (`treasure_Type`, `weenie_Class_Id`, `palette_Id`, `unknown_1`, `shade`, `stack_Size`, `stack_Size_Variance`, `probability`, `unknown_3`, `unknown_4`, `unknown_5`, `set_Start`, `has_Sub_Set`, `continues_Previous_Set`, `unknown_9`, `unknown_10`, `unknown_11`, `unknown_12`, `last_Modified`)\n\
         VALUES ({id}, {STATUE}, 0, 0, 0, 0, 0, 1, 0, 0, 0, True, False, False, 0, 0, 0, 0, '2026-10-25 00:00:00');\n"
    )
}

/// What the console says to `line`, a command of `import-treasure` or `roll-treasure`.
fn console(ts: &mut TestServer, line: &str) -> Vec<String> {
    let mut words = line.split_whitespace();
    let command = words.next().unwrap();
    let parameters: Vec<String> = words.map(str::to_owned).collect();
    command_manager::start_console_capture();
    match command {
        "import-treasure" => treasure::handle_import_treasure(&mut ts.world, None, &parameters),
        _ => treasure::handle_roll_treasure(&mut ts.world, None, &parameters),
    }
    command_manager::take_console_output()
}

#[test]
fn a_loot_table_imported_is_read_afresh_and_a_roll_says_what_each_draw_made_and_leaves_nothing() {
    let dir = tmp("import-roll");
    let mut ts = TestServer::new();
    ts.world.content = Arc::new(world_database(&dir));
    guid_manager::initialize(&mut ts.world, &mut EmptyShard);
    let content = dir.join("content");
    assert!(property_manager::modify_string(
        &ts.world,
        "content_folder",
        content.to_str().unwrap()
    ));
    let write = |rel: &str, text: &str| {
        let p = content.join("sql").join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    };
    // Nothing yet: no table, no file.
    assert_eq!(
        console(&mut ts, "roll-treasure 99"),
        ["Rolled treasure 99: there is no loot table 99."]
    );
    assert!(console(&mut ts, "import-treasure death 99")[0].starts_with("Couldn't find "));
    assert_eq!(
        console(&mut ts, "import-treasure gold 99"),
        ["Unknown loot table kind: give death or wielded."]
    );
    // Imported, then drawn: tier 3, nothing made.
    write("treasure_death/00099.sql", &death(99, 3));
    assert_eq!(
        console(&mut ts, "import-treasure death 99"),
        ["Imported 00099.sql"]
    );
    assert_eq!(
        console(&mut ts, "roll-treasure 99 2"),
        [
            "Rolling treasure 99: death treasure, tier 3, 2 time(s).",
            "Treasure roll 1: nothing",
            "Treasure roll 2: nothing",
            "Rolled treasure 99: 2 time(s).",
        ]
    );
    // Changed and imported again: the world reads it afresh.
    write("treasure_death/00099.sql", &death(99, 5));
    console(&mut ts, "import-treasure death 99");
    assert_eq!(
        ts.world.content.get_cached_death_treasure(99).unwrap().tier,
        5
    );
    assert!(console(&mut ts, "roll-treasure 99")[0].contains("tier 5"));
    // A wielded table, drawn: the statue each time, and nothing left in the world.
    write("treasure_wielded/00098 statue.sql", &wielded(98));
    assert_eq!(
        console(&mut ts, "import-treasure wielded 98"),
        ["Imported 00098 statue.sql"]
    );
    let objects = ts.world.objects.len();
    assert_eq!(
        console(&mut ts, "roll-treasure 98 2"),
        [
            "Rolling treasure 98: wielded treasure, 2 time(s).",
            &format!("Treasure roll 1: {STATUE} Loot Statue"),
            &format!("Treasure roll 2: {STATUE} Loot Statue"),
            "Rolled treasure 98: 2 time(s).",
        ]
    );
    assert_eq!(ts.world.objects.len(), objects, "nothing made stays");
    assert_eq!(
        console(&mut ts, "roll-treasure 98 1001"),
        ["Draw a table 1 to 1000 times."]
    );
    // The overlay is the world's until it goes.
    drop(ts);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_draws_things_are_said_by_class_count_and_name_and_a_tables_files_by_its_id() {
    assert_eq!(treasure::made_words(&[]), "nothing");
    assert_eq!(
        treasure::made_words(&[(273, 200, "Pyreal".into()), (309, 1, "Club".into())]),
        "273 x200 Pyreal; 309 Club"
    );
    assert!(treasure::is_table_file("00037.sql", Some(37)));
    assert!(treasure::is_table_file("00037 club.sql", Some(37)));
    assert!(!treasure::is_table_file("000370.sql", Some(37)));
    assert!(treasure::is_table_file("100000.sql", Some(100_000)));
    assert!(!treasure::is_table_file("00037.json", None));
    command_manager::initialize(None);
    for (name, console_only) in [("import-treasure", false), ("roll-treasure", true)] {
        let rows = command_manager::get_command_by_name(name);
        assert_eq!(rows.len(), 1, "{name} registered once");
        assert_eq!(rows[0].attribute.flags.0 & 1 != 0, console_only, "{name}");
    }
}
