//! Behaviour: none (tooling: conflicts between a world's packs, the world's word on which wins, and the one report of them all)

use std::path::{Path, PathBuf};

use lyceum_foundry::compile::{self, Field};
use lyceum_foundry::conflict::Conflict;
use lyceum_foundry::edit::{Op, Session, VertexSet};
use lyceum_foundry::scratch::Scratch;
use lyceum_foundry::world::World;
use lyceum_foundry::Error;

use crate::common::{self, Fixture, HOME};

/// A world of the packs `names`, in load order, with `extra` at the end of its world file.
fn world(f: &Fixture, names: &[&str], extra: &str) -> PathBuf {
    for n in names {
        f.write(
            &format!("{n}/pack.toml"),
            &format!("name = \"{n}\"\nversion = \"0.1.0\"\nlicense = \"MIT\"\n"),
        );
    }
    let list: Vec<String> = names.iter().map(|n| format!("\"{n}\"")).collect();
    f.write(
        "world.toml",
        &format!(
            "key = \"test-world\"\npacks = [{}]\nout = \"build\"\n{extra}",
            list.join(", ")
        ),
    )
}

fn build(f: &Fixture, world: &Path) -> Result<compile::Built, Error> {
    compile::build(world, &f.dats(), None, false)
}

/// The conflicts a build of `world` stops with.
fn conflicts(f: &Fixture, world: &Path) -> Vec<Conflict> {
    match build(f, world) {
        Err(Error::Conflicts(c)) => c,
        Err(e) => panic!("another error: {e}"),
        Ok(_) => panic!("it built"),
    }
}

/// Each conflict as `what: the packs that set it`.
fn summary(c: &[Conflict]) -> Vec<String> {
    c.iter()
        .map(|c| {
            let packs: Vec<&str> = c.claims.iter().map(|x| x.pack.as_str()).collect();
            format!("{}: {}", c.what, packs.join(", "))
        })
        .collect()
}

/// A height at landblock-local `(x, y)` of `lb`, as the built world holds it.
fn height(f: &Fixture, lb: u16, x: usize, y: usize) -> u8 {
    use dereth_assets::world::CellLandblock;
    use dereth_assets::Decode;
    let id = (u32::from(lb) << 16) | 0xFFFF;
    let b = common::read_through(&f.dats(), &f.out(), dereth_dat::ModernDat::Cell, id).unwrap();
    let t = CellLandblock::decode_payload(dereth_primitives::DataId(id), &b).unwrap();
    t.height[x * 9 + y]
}

fn texture(category: u32) -> String {
    format!(
        "[[surface-texture]]\nreplace = 0x05000010\nlevels = [0x06000010]\ncategory = {category}\n"
    )
}

/// Class 1234 of the server's, written whole, at `level`.
fn weenie(level: u32) -> String {
    format!(
        "DELETE FROM `weenie` WHERE `class_Id` = 1234;\n\
         \n\
         INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)\n\
         VALUES (1234, 'drudge', 10, '2026-10-0{} 00:00:00');\n\
         \n\
         INSERT INTO `weenie_properties_int` (`object_Id`, `type`, `value`)\n\
         VALUES (1234,  25, {level}) /* Level */\n     , (1234,   1, 2);\n",
        level % 9 + 1
    )
}

/// The server's spawn 0x70101005 of landblock `HOME`, moved to `x`.
fn spawn_moved(x: u32) -> String {
    format!(
        "DELETE FROM `landblock_instance` WHERE `guid` = 0x70101005;\n\
         \n\
         INSERT INTO `landblock_instance` (`guid`, `weenie_Class_Id`, `obj_Cell_Id`, `origin_X`, `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`, `is_Link_Child`, `last_Modified`)\n\
         VALUES (0x70101005, 1234, 0x01010019, {x}, 4, 94, 1, 0, 0, 0, False, '2026-10-01 00:00:00');\n"
    )
}

const SPAWN_REMOVED: &str = "DELETE FROM `landblock_instance` WHERE `guid` = 0x70101005;\n";

fn quest(min_delta: u32) -> String {
    format!(
        "DELETE FROM `quest` WHERE `name` = 'DrudgeKills';\n\
         \n\
         INSERT INTO `quest` (`name`, `min_Delta`, `max_Solves`, `message`, `last_Modified`)\n\
         VALUES ('DrudgeKills', {min_delta}, 1, 'Drudges slain', '2026-10-01 00:00:00');\n"
    )
}

#[test]
fn two_packs_replacing_one_record_conflict_unless_they_replace_it_alike() {
    let f = Fixture::new("conflict-record");
    let w = world(&f, &["a", "b"], "");
    f.write("a/records/t.toml", &texture(1));
    f.write("b/records/t.toml", &format!("# b's own\n{}", texture(2)));
    let c = conflicts(&f, &w);
    assert_eq!(summary(&c), ["record 0x05000010: a, b"]);
    for (claim, pack) in c[0].claims.iter().zip(["a", "b"]) {
        assert!(claim
            .at
            .file
            .ends_with(Path::new(pack).join("records/t.toml")));
        assert_eq!(claim.sets, "replaces it with a surface-texture");
    }
    assert!(c[0].claims[1].at.line > c[0].claims[0].at.line);
    f.write("b/records/t.toml", &texture(1));
    build(&f, &w).expect("the same record from both is no conflict");
}

#[test]
fn two_packs_setting_the_same_vertices_differently_conflict_as_one_run() {
    let f = Fixture::new("conflict-vertices");
    let w = world(&f, &["a", "b"], "");
    f.write(
        "a/edits/landblocks/0101.toml",
        "[[vertex]]\nat = [2, 2]\nto = [3, 4]\nheight = 50\nroad = 1\n",
    );
    // Overlapping a's at x 3, y 3 to 4; the road is set alike.
    f.write(
        "b/edits/landblocks/0101.toml",
        "\n[[vertex]]\nat = [3, 3]\nto = [5, 5]\nheight = 60\nroad = 1\n",
    );
    let c = conflicts(&f, &w);
    assert_eq!(
        summary(&c),
        ["the height of 2 vertices from 11,11 to 11,12: a, b"]
    );
    assert_eq!(c[0].landblocks.iter().copied().collect::<Vec<_>>(), [HOME]);
    assert_eq!(
        c[0].claims
            .iter()
            .map(|x| (x.at.line, x.sets.as_str()))
            .collect::<Vec<_>>(),
        [(1, "sets it to 50"), (2, "sets it to 60")]
    );
}

#[test]
fn two_packs_moving_one_static_differently_or_one_removing_what_the_other_moves_conflict() {
    let f = Fixture::new("conflict-statics");
    let w = world(&f, &["a", "b"], "");
    f.write(
        "a/edits/landblocks/0101.toml",
        "[[move-static]]\nindex = 0\nat = [11.0, 10.0, 5.0]\n\n\
         [[remove-static]]\nindex = 1\n",
    );
    f.write(
        "b/edits/landblocks/0101.toml",
        "[[move-static]]\nindex = 0\nat = [12.0, 10.0, 5.0]\n\n\
         [[move-static]]\nindex = 1\nheading = 90\n",
    );
    let c = conflicts(&f, &w);
    assert_eq!(summary(&c), ["static 0101 0: a, b", "static 0101 1: a, b"]);
    assert_eq!(c[0].claims[0].sets, "moves it to (11, 10, 5)");
    assert_eq!(c[0].claims[1].sets, "moves it to (12, 10, 5)");
    assert_eq!(c[1].claims[0].sets, "removes it");
    assert_eq!(c[1].claims[1].sets, "turns it to heading 90");
    // Moved alike, it is no conflict; removed by both, neither.
    f.write(
        "b/edits/landblocks/0101.toml",
        "[[move-static]]\nindex = 0\nat = [11.0, 10.0, 5.0]\n\n\
         [[remove-static]]\nindex = 1\n",
    );
    build(&f, &w).unwrap();
}

#[test]
fn two_packs_writing_one_weenie_class_differently_conflict_on_the_rows_that_differ() {
    let f = Fixture::new("conflict-weenie");
    let w = world(&f, &["a", "b"], "");
    f.write("a/server/sql/weenies/1234 drudge.sql", &weenie(10));
    f.write("b/server/sql/weenies/1234 drudge.sql", &weenie(20));
    let c = conflicts(&f, &w);
    assert_eq!(summary(&c), ["weenie 1234: a, b"]);
    // The date it was written differs too, and is no part of it.
    assert_eq!(c[0].claims[0].sets, "sets int 25: 10");
    assert_eq!(c[0].claims[1].sets, "sets int 25: 20");
    assert!(c[0].claims.iter().all(|x| x.at.line == 1));
    f.write("b/server/sql/weenies/1234 drudge.sql", &weenie(10));
    build(&f, &w).expect("the same class from both is no conflict");
}

#[test]
fn two_packs_changing_one_spawn_or_one_quest_differently_conflict() {
    let f = Fixture::new("conflict-spawn-quest");
    let w = world(&f, &["a", "b"], "");
    f.write("a/server/sql/landblocks/0101 moved.sql", &spawn_moved(84));
    f.write("b/server/sql/landblocks/0101 gone.sql", SPAWN_REMOVED);
    f.write("a/server/sql/quests/DrudgeKills.sql", &quest(3600));
    f.write("b/server/sql/quests/DrudgeKills.sql", &quest(7200));
    let c = conflicts(&f, &w);
    assert_eq!(
        summary(&c),
        ["spawn 0x70101005: a, b", "quest drudgekills: a, b"]
    );
    assert!(c[0].claims[0].sets.starts_with(
        "sets instance: weenie_Class_Id = 1234, obj_Cell_Id = 16842777, origin_X = 84"
    ));
    assert_eq!(c[0].claims[1].sets, "deletes it");
    assert_eq!(c[0].landblocks.iter().copied().collect::<Vec<_>>(), [HOME]);
    assert_eq!(
        c[1].claims[0].sets,
        "sets quest: min_Delta = 3600, max_Solves = 1, message = Drudges slain"
    );
}

#[test]
fn two_packs_writing_one_loot_table_differently_conflict_and_an_override_settles_it() {
    use lyceum_foundry::loot::{death_text, plain_death, wielded_text};
    use lyceum_foundry::sql_ids::WieldedItem;
    let f = Fixture::new("conflict-loot");
    let w = world(&f, &["a", "b"], "");
    let death = |tier: i32| {
        let t = lyceum_foundry::sql_ids::Treasure {
            tier,
            ..plain_death()
        };
        death_text("37", &t, "2026-10-25 00:00:00")
    };
    let wielded = |p: f32| {
        let r = WieldedItem {
            weenie: "309".into(),
            probability: p,
            set_start: true,
            ..WieldedItem::default()
        };
        wielded_text("37", &[r], "2026-10-25 00:00:00", &|_| None)
    };
    f.write("a/server/sql/treasure_death/00037.sql", &death(2));
    f.write("b/server/sql/treasure_death/00037.sql", &death(3));
    f.write("a/server/sql/treasure_wielded/00037.sql", &wielded(0.5));
    f.write("b/server/sql/treasure_wielded/00037.sql", &wielded(0.5));
    let c = conflicts(&f, &w);
    assert_eq!(
        summary(&c),
        ["death-treasure 37: a, b"],
        "the same rows agree"
    );
    assert!(
        c[0].claims[0].sets.contains("tier = 2"),
        "{}",
        c[0].claims[0].sets
    );
    // b wins it: its file is built, a's left out.
    let w = world(
        &f,
        &["a", "b"],
        "\n[[override]]\npack = \"b\"\nover = \"a\"\nitems = [\"death-treasure 37\"]\n",
    );
    build(&f, &w).unwrap();
    let sql = f.out().join("sql");
    assert!(!sql.join("a/treasure_death/00037.sql").exists());
    assert!(sql.join("b/treasure_death/00037.sql").is_file());
}

#[test]
fn two_packs_each_naming_a_new_loot_table_alike_get_two_ids_and_no_conflict() {
    use lyceum_foundry::loot::{death_text, plain_death, NEW_ID};
    let f = Fixture::new("conflict-loot-symbols");
    let w = world(&f, &["a", "b"], "");
    let rich = death_text("{{treasure:rich}}", &plain_death(), "2026-10-25 00:00:00");
    f.write("a/server/sql/treasure_death/rich.sql", &rich);
    f.write("b/server/sql/treasure_death/rich.sql", &rich);
    build(&f, &w).expect("a:rich and b:rich are two tables");
    let sql = f.out().join("sql");
    let id_of = |p: &str| {
        let text = std::fs::read_to_string(sql.join(p)).unwrap();
        assert!(!text.contains("{{"), "numbered: {text}");
        lyceum_foundry::sql_ids::loot_ids(&text)
    };
    let mut ids = id_of("a/treasure_death/rich.sql");
    ids.extend(id_of("b/treasure_death/rich.sql"));
    ids.sort_unstable();
    assert_eq!(ids, [NEW_ID, NEW_ID + 1]);
    // The lockfile keeps them.
    let lock = lyceum_foundry::lock::Lock::read(&f.path().join("world.lock")).unwrap();
    assert_eq!(
        lock.get(lyceum_foundry::symbol::Kind::Treasure, "a:rich"),
        Some(NEW_ID)
    );
}

#[test]
fn an_override_settles_a_conflict_the_build_takes_the_winners_and_lists_what_it_overrode() {
    let f = Fixture::new("conflict-override");
    // a wins class 1234, the record and everything of landblock 0101; b wins everything else.
    let w = world(
        &f,
        &["a", "b"],
        "\n[[override]]\npack = \"a\"\nover = \"b\"\nitems = [\"weenie 1234\", \"record 0x05000010\", \"landblock 0101\"]\n\
         \n[[override]]\npack = \"b\"\nover = \"a\"\n",
    );
    f.write("a/records/t.toml", &texture(1));
    f.write("b/records/t.toml", &texture(2));
    f.write(
        "a/edits/landblocks/0101.toml",
        "[[vertex]]\nat = [3, 3]\nheight = 50\n",
    );
    f.write(
        "b/edits/landblocks/0101.toml",
        "[[vertex]]\nat = [3, 3]\nheight = 60\n",
    );
    f.write("a/server/sql/weenies/1234 drudge.sql", &weenie(10));
    f.write("b/server/sql/weenies/1234 drudge.sql", &weenie(20));
    f.write("a/server/sql/landblocks/0101 moved.sql", &spawn_moved(84));
    f.write("b/server/sql/landblocks/0101 gone.sql", SPAWN_REMOVED);
    f.write("a/server/sql/quests/DrudgeKills.sql", &quest(3600));
    f.write("b/server/sql/quests/DrudgeKills.sql", &quest(7200));
    let built = build(&f, &w).unwrap();
    let mut overrode: Vec<String> = built
        .overridden
        .iter()
        .map(|o| format!("{}: {}", o.what, o.winner.pack))
        .collect();
    overrode.sort();
    assert_eq!(
        overrode,
        [
            "quest drudgekills: b",
            "record 0x05000010: a",
            "spawn 0x70101005: a",
            "vertex 11,11 height: a",
            "weenie 1234: a",
        ]
    );
    assert!(built.overridden[0].to_string().contains(" over "));
    // The winners' are built; the losers' SQL for each item is left out.
    assert_eq!(height(&f, HOME, 3, 3), 50);
    let sql = f.out().join("sql");
    assert!(sql.join("a/weenies/1234 drudge.sql").is_file());
    assert!(!sql.join("b/weenies/1234 drudge.sql").exists());
    assert!(sql.join("a/landblocks/0101 moved.sql").is_file());
    assert!(!sql.join("b/landblocks/0101 gone.sql").exists());
    assert!(!sql.join("a/quests/DrudgeKills.sql").exists());
    assert!(sql.join("b/quests/DrudgeKills.sql").is_file());
    let t = common::read_through(
        &f.dats(),
        &f.out(),
        dereth_dat::ModernDat::Portal,
        0x0500_0010,
    )
    .unwrap();
    let t = <dereth_assets::SurfaceTexture as dereth_assets::Decode>::decode_payload(
        dereth_primitives::DataId(0x0500_0010),
        &t,
    )
    .unwrap();
    assert_eq!(t.data_category, 1, "a's, though b's comes later");
}

#[test]
fn an_override_of_one_item_leaves_the_rest_in_conflict() {
    let f = Fixture::new("conflict-override-one");
    let w = world(
        &f,
        &["a", "b"],
        "\n[[override]]\npack = \"b\"\nover = \"a\"\nitems = [\"quest DRUDGEKILLS\"]\n",
    );
    f.write("a/server/sql/quests/DrudgeKills.sql", &quest(3600));
    f.write("b/server/sql/quests/DrudgeKills.sql", &quest(7200));
    f.write("a/server/sql/weenies/1234 drudge.sql", &weenie(10));
    f.write("b/server/sql/weenies/1234 drudge.sql", &weenie(20));
    assert_eq!(summary(&conflicts(&f, &w)), ["weenie 1234: a, b"]);
}

#[test]
fn a_statement_of_several_items_keeps_the_rows_of_those_that_stand() {
    let f = Fixture::new("conflict-mixed-rows");
    let w = world(
        &f,
        &["a", "b"],
        "\n[[override]]\npack = \"b\"\nover = \"a\"\n",
    );
    // One statement of two classes' rows; b changes one of them.
    f.write(
        "a/server/sql/weenies/both.sql",
        "INSERT INTO `weenie_properties_int` (`object_Id`, `type`, `value`)\n\
         VALUES (1234, 25, 10)\n     , (1235, 25, 11);\n",
    );
    f.write(
        "b/server/sql/weenies/1234.sql",
        "INSERT INTO `weenie_properties_int` (`object_Id`, `type`, `value`)\nVALUES (1234, 25, 20);\n",
    );
    build(&f, &w).unwrap();
    let a = std::fs::read_to_string(f.out().join("sql/a/weenies/both.sql")).unwrap();
    assert_eq!(
        a,
        "INSERT INTO `weenie_properties_int` (`object_Id`, `type`, `value`)\nVALUES (1235, 25, 11);\n"
    );
}

#[test]
fn an_override_that_names_no_pack_or_no_item_is_refused_at_its_line() {
    let f = Fixture::new("conflict-bad-override");
    let w = world(
        &f,
        &["a", "b"],
        "\n[[override]]\npack = \"a\"\nover = \"c\"\n",
    );
    let e = build(&f, &w).unwrap_err().to_string();
    assert!(
        e.contains("world.toml:5") && e.contains("no pack named c"),
        "{e}"
    );
    let w = world(
        &f,
        &["a", "b"],
        "\n[[override]]\npack = \"a\"\nover = \"b\"\nitems = [\"wenie 1234\"]\n",
    );
    let e = World::read(&w).unwrap_err().to_string();
    assert!(
        e.contains("world.toml:5") && e.contains("is not an item"),
        "{e}"
    );
    let w = world(
        &f,
        &["a", "b"],
        "\n[[override]]\npack = \"a\"\nover = \"b\"\n\n[[override]]\npack = \"b\"\nover = \"a\"\n",
    );
    let e = World::read(&w).unwrap_err().to_string();
    assert!(e.contains("each win everything over the other"), "{e}");
}

/// Three packs at odds: every kind of conflict among them.
fn three_packs(f: &Fixture, extra: &str) -> PathBuf {
    let w = world(f, &["a", "b", "c"], extra);
    f.write("a/records/t.toml", &texture(1));
    f.write("b/records/t.toml", &texture(2));
    f.write(
        "a/edits/landblocks/0101.toml",
        "[[vertex]]\nat = [3, 3]\nheight = 50\n\n[[remove-static]]\nindex = 1\n",
    );
    f.write(
        "b/edits/landblocks/0101.toml",
        "[[vertex]]\nat = [3, 3]\nheight = 60\n\n[[move-static]]\nindex = 0\nat = [12.0, 10.0, 5.0]\n",
    );
    f.write(
        "c/edits/landblocks/0101.toml",
        "[[vertex]]\nat = [3, 3]\nheight = 70\n\n[[move-static]]\nindex = 0\nat = [13.0, 10.0, 5.0]\n\n\
         [[move-static]]\nindex = 1\nheading = 90\n",
    );
    f.write("a/server/sql/weenies/1234 drudge.sql", &weenie(10));
    f.write("c/server/sql/weenies/1234 drudge.sql", &weenie(30));
    f.write("b/server/sql/landblocks/0101 moved.sql", &spawn_moved(84));
    f.write("c/server/sql/landblocks/0101 gone.sql", SPAWN_REMOVED);
    f.write("a/server/sql/quests/DrudgeKills.sql", &quest(3600));
    f.write("b/server/sql/quests/DrudgeKills.sql", &quest(7200));
    w
}

#[test]
fn a_world_of_three_packs_reports_every_conflict_with_each_packs_file_line_and_what_it_sets() {
    let f = Fixture::new("conflict-three");
    let w = three_packs(&f, "");
    let e = build(&f, &w).unwrap_err();
    let Error::Conflicts(c) = &e else {
        panic!("{e}")
    };
    assert_eq!(
        summary(c),
        [
            "record 0x05000010: a, b",
            "vertex 11,11 height: a, b, c",
            "static 0101 0: b, c",
            "static 0101 1: a, c",
            "weenie 1234: a, c",
            "spawn 0x70101005: b, c",
            "quest drudgekills: a, b",
        ]
    );
    let text = e.to_string();
    assert!(text.starts_with("7 thing(s) set differently"), "{text}");
    for c in c {
        for claim in &c.claims {
            assert!(claim.at.line > 0, "{c}");
            assert!(claim.at.file.starts_with(f.path().join(&claim.pack)), "{c}");
            let line = format!("{}, {}: {}", claim.pack, claim.at, claim.sets);
            assert!(text.contains(&line), "{line}\n{text}");
        }
    }
    // The vertex's three: each pack's file at its first line.
    let v = &c[1];
    assert_eq!(
        v.claims.iter().map(|x| x.sets.as_str()).collect::<Vec<_>>(),
        ["sets it to 50", "sets it to 60", "sets it to 70"]
    );
    // c winning over both settles every conflict c is in, and leaves a's and b's.
    let w = three_packs(
        &f,
        "\n[[override]]\npack = \"c\"\nover = \"a\"\n\n[[override]]\npack = \"c\"\nover = \"b\"\n",
    );
    assert_eq!(
        summary(&conflicts(&f, &w)),
        ["record 0x05000010: a, b", "quest drudgekills: a, b"]
    );
}

#[test]
fn an_editor_opens_a_world_in_conflict_lists_its_conflicts_and_refuses_an_edit_that_makes_another()
{
    let f = Fixture::new("conflict-editor");
    let w = three_packs(&f, "");
    let s = Session::open(&w, &f.dats(), None).expect("a world in conflict opens");
    let (_scratch, first) = Scratch::create(&f.path().join("scratch"), &s).unwrap();
    let faults = s.check(&first.dir).unwrap();
    let listed: Vec<&str> = faults
        .iter()
        .filter(|x| !x.warning)
        .map(|x| x.text.as_str())
        .collect();
    let built = match build(&f, &w) {
        Err(Error::Conflicts(c)) => c,
        other => panic!("{other:?}"),
    };
    for c in &built {
        assert!(
            listed.contains(&c.line().as_str()),
            "{}\n{listed:#?}",
            c.line()
        );
    }
    let vertex = faults
        .iter()
        .find(|x| x.text.contains("vertex 11,11"))
        .unwrap();
    assert_eq!(vertex.at.map(|(lb, _)| lb), Some(HOME));
    // c, the pack being edited, painting a vertex a sets otherwise: refused, and nothing
    // changes.
    let paint = Op::SetVertices(vec![VertexSet {
        gx: 12,
        gy: 12,
        field: Field::Height,
        value: 90,
    }]);
    f.write(
        "a/edits/landblocks/0101.toml",
        "[[vertex]]\nat = [3, 3]\nheight = 50\n\n[[vertex]]\nat = [4, 4]\nheight = 80\n\n\
         [[remove-static]]\nindex = 1\n",
    );
    let mut s = Session::open(&w, &f.dats(), None).unwrap();
    let before = s.compiled().conflicts().len();
    let e = s.apply(&paint).unwrap_err();
    assert!(
        matches!(&e, Error::Conflicts(c) if c.len() == 1 && c[0].what == "vertex 12,12 height"),
        "{e}"
    );
    assert_eq!(s.compiled().conflicts().len(), before);
    // Once the world says c wins its landblock over a, the same edit is taken.
    let w = world(
        &f,
        &["a", "b", "c"],
        "\n[[override]]\npack = \"c\"\nover = \"a\"\nitems = [\"landblock 0101\"]\n",
    );
    let mut s = Session::open(&w, &f.dats(), None).unwrap();
    s.apply(&paint).unwrap();
    assert_eq!(s.terrain(HOME).unwrap().height[4 * 9 + 4], 90);
}
