//! Behaviour: none (tooling: the world editor's edit session, over a small set of files)

use std::path::Path;

use dereth_dat::ModernDat;
use dereth_primitives::{DataId, Vec3};
use lyceum_foundry::compile::{self, Field, StaticOrigin};
use lyceum_foundry::edit::{Op, Session, VertexSet, Weenie};
use lyceum_foundry::source::{self, Height, Place, Rotation};
use lyceum_foundry::world::World;

use crate::common::{Fixture, GFXOBJ, HOME, SETUP};

fn open(f: &Fixture, world: &Path) -> Session {
    let mut s = Session::open(world, &f.dats(), None).unwrap();
    s.set_today("2026-10-18 00:00:00");
    s
}

fn at(x: f32, y: f32, z: f32) -> Place {
    Place {
        x,
        y,
        z: Height::At(z),
    }
}

/// What a full build of the sources as they stand in `s` makes.
fn full(s: &Session) -> compile::Compiled {
    let world = World::read(&s.world().path).unwrap();
    let lock = lyceum_foundry::lock::Lock::read(&world.lock_path()).unwrap();
    compile::compile(&world, s.packs(), s.base(), &lock).unwrap()
}

fn assert_same_as_full(s: &Session) {
    let f = full(s);
    let c = s.compiled();
    assert_eq!(
        c.records, f.records,
        "the records of an incremental compile"
    );
    assert_eq!(c.sql, f.sql, "the SQL");
    assert_eq!(c.lock, f.lock, "the ids");
    assert_eq!(c.statics, f.statics, "where each static came from");
}

#[test]
fn a_placed_static_is_one_named_entry_in_its_landblock_file_and_undo_takes_it_back() {
    let f = Fixture::new("edit-place");
    let world = f.world();
    let mut s = open(&f, &world);
    let applied = s
        .apply(&Op::PlaceStatic {
            landblock: 0x0102,
            model: DataId(SETUP),
            at: at(12.0, 34.5, 56.0),
            rotation: Rotation::Heading(90.0),
            name: None,
        })
        .unwrap();
    assert_eq!(
        applied
            .touched
            .landblocks
            .iter()
            .copied()
            .collect::<Vec<_>>(),
        [0x0102]
    );
    let origin = StaticOrigin::Pack {
        pack: 0,
        name: Some("sample:static-1".into()),
        entry: 0,
    };
    assert_eq!(applied.placed, Some((0x0102, origin.clone())));
    let info = s.statics(0x0102).expect("a statics record is made");
    assert_eq!(info.objects.len(), 1);
    assert_eq!(s.origins(0x0102), [origin]);
    assert_eq!(s.terrain(0x0102).unwrap().lbi_exists, 1);
    let written = s.save().unwrap();
    let file = f.path().join("sample/edits/landblocks/0102.toml");
    assert_eq!(written, std::slice::from_ref(&file));
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "[[static]]\nname = \"static-1\"\nmodel = 0x02000001\nat = [12.0, 34.5, 56.0]\nheading = 90.0\n"
    );
    assert_same_as_full(&s);
    s.undo().unwrap();
    assert!(
        s.statics(0x0102).is_none(),
        "the record is the base's again"
    );
    assert_eq!(s.terrain(0x0102).unwrap().lbi_exists, 0);
    assert_same_as_full(&s);
    s.redo().unwrap();
    assert_eq!(s.statics(0x0102).unwrap().objects.len(), 1);
}

#[test]
fn a_base_static_moved_twice_is_one_move_by_its_index() {
    let f = Fixture::new("edit-move-base");
    let world = f.world();
    let mut s = open(&f, &world);
    for x in [99.0, 120.0] {
        s.apply(&Op::MoveStatic {
            landblock: HOME,
            which: StaticOrigin::Base(1),
            at: Some(at(x, 98.0, 7.0)),
            rotation: None,
        })
        .unwrap();
    }
    s.save().unwrap();
    assert_eq!(
        std::fs::read_to_string(f.path().join("sample/edits/landblocks/0101.toml")).unwrap(),
        "[[move-static]]\nindex = 1\nat = [120.0, 98.0, 7.0]\n"
    );
    let o = s.statics(HOME).unwrap().objects[1];
    assert_eq!(
        (o.id, o.frame.origin),
        (DataId(SETUP), Vec3::new(120.0, 98.0, 7.0))
    );
    assert_same_as_full(&s);
}

const HAND_WRITTEN: &str = "\
# Two posts by the road, written by hand.

# The first, by the gate.
[[static]]
name = \"post-a\"
model = 0x01000001
at = [1.0, 2.0, \"ground\"] # set on the ground

[[static]]
name = \"post-b\"
model = 0x01000001
at = [4.0, 5.0, 6.0]

# A mound.
[[vertex]]
at = [3, 3]
height = 40
";

#[test]
fn editing_a_hand_written_file_keeps_its_comments_and_the_entries_it_does_not_touch() {
    let f = Fixture::new("edit-keep");
    let world = f.world();
    let file = f.write("sample/edits/landblocks/0101.toml", HAND_WRITTEN);
    let mut s = open(&f, &world);
    // HOME's own two statics come first, then the pack's.
    let origins = s.origins(HOME);
    assert_eq!(origins.len(), 4);
    s.apply(&Op::MoveStatic {
        landblock: HOME,
        which: origins[3].clone(),
        at: Some(at(4.5, 5.0, 6.0)),
        rotation: Some(Rotation::Heading(45.0)),
    })
    .unwrap();
    s.apply(&Op::RemoveStatic {
        landblock: HOME,
        which: origins[2].clone(),
    })
    .unwrap();
    s.save().unwrap();
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "\
# Two posts by the road, written by hand.

[[static]]
name = \"post-b\"
model = 0x01000001
at = [4.5, 5.0, 6.0]
heading = 45.0

# A mound.
[[vertex]]
at = [3, 3]
height = 40
"
    );
    assert_same_as_full(&s);
}

#[test]
fn an_edge_vertex_is_set_where_a_file_already_sets_it_else_in_its_owning_landblock() {
    let f = Fixture::new("edit-vertex-owner");
    let world = f.world();
    let mut s = open(&f, &world);
    // World (16, 9) is 0x0201's (0, 1) and 0x0101's (8, 1): 0x0201 owns it.
    let set = |value| {
        Op::SetVertices(vec![VertexSet {
            gx: 16,
            gy: 9,
            field: Field::Height,
            value,
        }])
    };
    let applied = s.apply(&set(30)).unwrap();
    assert_eq!(
        applied
            .touched
            .landblocks
            .iter()
            .copied()
            .collect::<Vec<_>>(),
        [0x0101, 0x0201]
    );
    assert_eq!(s.terrain(0x0101).unwrap().height[8 * 9 + 1], 30);
    assert_eq!(s.terrain(0x0201).unwrap().height[1], 30);
    s.save().unwrap();
    assert_eq!(
        std::fs::read_to_string(f.path().join("sample/edits/landblocks/0201.toml")).unwrap(),
        "[[vertex]]\nat = [0, 1]\nheight = 30\n"
    );
    assert!(!f.path().join("sample/edits/landblocks/0101.toml").exists());

    // A file that sets it from the other side keeps it.
    let g = Fixture::new("edit-vertex-kept");
    let world = g.world();
    let west = g.write(
        "sample/edits/landblocks/0101.toml",
        "[[vertex]]\nat = [8, 1]\nheight = 20\n",
    );
    let mut s = open(&g, &world);
    s.apply(&set(31)).unwrap();
    s.save().unwrap();
    assert_eq!(
        std::fs::read_to_string(&west).unwrap(),
        "[[vertex]]\nat = [8, 1]\nheight = 31\n"
    );
    assert!(!g.path().join("sample/edits/landblocks/0201.toml").exists());
    assert_same_as_full(&s);
}

#[test]
fn an_unset_vertex_shows_the_world_again_and_a_row_left_setting_nothing_goes() {
    let f = Fixture::new("edit-vertex-unset");
    let world = f.world();
    let file = f.write(
        "sample/edits/landblocks/0202.toml",
        "[[vertex]]\nat = [0, 0]\nto = [2, 2]\nterrain = 5\n\n[[vertex]]\nat = [1, 1]\nheight = 40\n\n\
         [[vertex]]\nat = [2, 1]\nheight = 41\nterrain = 7\n",
    );
    let mut s = open(&f, &world);
    // World (17, 17) is 0x0202's (1, 1), (18, 17) its (2, 1), and (16, 16) its (0, 0).
    assert_eq!(s.vertex(17, 17).unwrap().height, 40);
    s.apply(&Op::UnsetVertices(vec![
        (17, 17, Field::Height),
        (18, 17, Field::Height),
        (16, 16, Field::Terrain),
    ]))
    .unwrap();
    // The world's heights again: every vertex at 10 + x + y.
    assert_eq!(s.vertex(17, 17).unwrap().height, 12);
    assert_eq!(s.vertex(18, 17).unwrap().height, 13);
    // What else a row sets stays, and a rectangle's setting is not taken back.
    assert_eq!(s.vertex(18, 17).unwrap().terrain, 7);
    assert_eq!(s.vertex(16, 16).unwrap().terrain, 5);
    s.save().unwrap();
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "[[vertex]]\nat = [0, 0]\nto = [2, 2]\nterrain = 5\n\n[[vertex]]\nat = [2, 1]\nterrain = 7\n"
    );
    assert_same_as_full(&s);
    s.undo().unwrap();
    assert_eq!(s.vertex(17, 17).unwrap().height, 40);
}

#[test]
fn a_corner_vertex_reaches_all_four_landblocks_and_one_inside_a_rectangle_is_set_alone() {
    let f = Fixture::new("edit-vertex-corner");
    let world = f.world();
    f.write(
        "sample/edits/landblocks/0202.toml",
        "[[vertex]]\nat = [0, 0]\nto = [2, 2]\nterrain = 5\n",
    );
    let mut s = open(&f, &world);
    // World (16, 16) is the corner the four landblocks share; the rectangle sets its terrain.
    let applied = s
        .apply(&Op::SetVertices(vec![VertexSet {
            gx: 16,
            gy: 16,
            field: Field::Terrain,
            value: 9,
        }]))
        .unwrap();
    assert_eq!(applied.touched.landblocks.len(), 4);
    for (lb, x, y) in [
        (0x0101u16, 8usize, 8usize),
        (0x0102, 8, 0),
        (0x0201, 0, 8),
        (0x0202, 0, 0),
    ] {
        assert_eq!(s.terrain(lb).unwrap().terrain_type(x, y), 9, "{lb:04X}");
    }
    // Its neighbor in the rectangle keeps the rectangle's.
    assert_eq!(s.terrain(0x0202).unwrap().terrain_type(1, 1), 5);
    s.save().unwrap();
    assert_eq!(
        std::fs::read_to_string(f.path().join("sample/edits/landblocks/0202.toml")).unwrap(),
        "[[vertex]]\nat = [0, 0]\nto = [2, 2]\nterrain = 5\n\n[[vertex]]\nat = [0, 0]\nterrain = 9\n"
    );
    assert_same_as_full(&s);
}

#[test]
fn a_spawn_is_one_sql_file_its_instance_numbered_as_a_build_numbers_it() {
    let f = Fixture::new("edit-spawn");
    let world = f.world();
    let mut s = open(&f, &world);
    let applied = s
        .apply(&Op::PlaceSpawn {
            landblock: HOME,
            weenie: Weenie::Class(7924),
            origin: Vec3::new(36.0, 18.0, 5.0),
            heading: 180.0,
        })
        .unwrap();
    let file = f
        .path()
        .join("sample/server/sql/landblocks/0101 weenie-7924.sql");
    assert_eq!(applied.spawned.as_deref(), Some(file.as_path()));
    assert!(applied.touched.sql);
    let filled = &s.compiled().sql[Path::new("sample/landblocks/0101 weenie-7924.sql")];
    assert!(
        filled.contains(
            "VALUES (0x70101FFF, 7924, 0x01010009, 36.0, 18.0, 5.0, 0.0, 0.0, 0.0, -1.0, False, '2026-10-18 00:00:00');"
        ),
        "{filled}"
    );
    assert_same_as_full(&s);
    let written = s.save().unwrap();
    assert_eq!(written, [file, world.with_file_name("world.lock")]);
    assert!(std::fs::read_to_string(world.with_file_name("world.lock"))
        .unwrap()
        .contains("\"sample:weenie-7924@0101\" = 0x70101FFF"));
    // The spawn's instance reads back from the source.
    let spawns = s.spawns();
    assert_eq!(spawns.len(), 1);
    assert_eq!(spawns[0].cell, 0x0101_0009);
    assert_eq!(spawns[0].frame.origin, Vec3::new(36.0, 18.0, 5.0));
}

/// Two instances written by hand in one statement, with a comment between them.
const PAIR: &str = "\
/* Two guards at the gate. */
INSERT INTO `landblock_instance` (`guid`, `weenie_Class_Id`, `obj_Cell_Id`, `origin_X`, `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`, `is_Link_Child`, `last_Modified`)
VALUES ({{guid:guard-a@0101}}, 7924, 0x01010001, 1.5, 2.5, 5.0, 1.0, 0.0, 0.0, 0.0, False, '2026-01-01 00:00:00') /* the left one */
     , ({{guid:guard-b@0101}}, 7924, 0x01010001, 4.5, 2.5, 5.0, 1.0, 0.0, 0.0, 0.0, False, '2026-01-01 00:00:00') /* the right one */;
";

#[test]
fn a_moved_spawn_is_rewritten_where_its_values_are_written_and_undo_puts_them_back() {
    let f = Fixture::new("edit-spawn-move");
    let world = f.world();
    let pair = f.write("sample/server/sql/landblocks/0101 guards.sql", PAIR);
    let mut s = open(&f, &world);
    let guard = |s: &Session, guid: &str| {
        s.spawns()
            .into_iter()
            .find(|x| x.guid == guid)
            .expect("the guard")
    };
    s.apply(&Op::MoveSpawn {
        file: pair.clone(),
        guid: "{{guid:guard-b@0101}}".into(),
        origin: Vec3::new(60.0, 70.0, 6.25),
        heading: 90.0,
    })
    .unwrap();
    assert_same_as_full(&s);
    let b = guard(&s, "{{guid:guard-b@0101}}");
    // Cell (2, 2) of the landblock, outdoors.
    assert_eq!(b.cell, 0x0101_0013);
    assert_eq!(b.frame.origin, Vec3::new(60.0, 70.0, 6.25));
    let heading = dereth_primitives::frame::get_heading(&b.frame);
    assert!((heading - 90.0).abs() < 0.01, "{heading}");
    // Only the moved instance's cell, origin and angles changed; its comment, its other
    // values and the other instance are as they were written.
    let q = lyceum_foundry::compile::heading_quat(90.0);
    let f6 = |v: f32| {
        let t = format!("{v:.6}");
        let t = t.trim_end_matches('0');
        if t.ends_with('.') {
            format!("{t}0")
        } else {
            t.to_owned()
        }
    };
    let expected = PAIR.replace(
        "({{guid:guard-b@0101}}, 7924, 0x01010001, 4.5, 2.5, 5.0, 1.0, 0.0, 0.0, 0.0,",
        &format!(
            "({{{{guid:guard-b@0101}}}}, 7924, 0x01010013, 60.0, 70.0, 6.25, {}, {}, {}, {},",
            f6(q.w),
            f6(q.x),
            f6(q.y),
            f6(q.z)
        ),
    );
    assert_ne!(expected, PAIR);
    s.save().unwrap();
    assert_eq!(std::fs::read_to_string(&pair).unwrap(), expected);
    assert_eq!(
        guard(&s, "{{guid:guard-a@0101}}").frame.origin,
        Vec3::new(1.5, 2.5, 5.0)
    );
    // A spawn cannot leave its landblock, and a file of two is not taken away for one.
    assert!(s
        .apply(&Op::MoveSpawn {
            file: pair.clone(),
            guid: "{{guid:guard-a@0101}}".into(),
            origin: Vec3::new(200.0, 10.0, 0.0),
            heading: 0.0,
        })
        .is_err());
    assert!(s.apply(&Op::RemoveSpawn { file: pair.clone() }).is_err());
    s.undo().unwrap();
    assert_same_as_full(&s);
    assert_eq!(guard(&s, "{{guid:guard-b@0101}}").cell, 0x0101_0001);
    s.save().unwrap();
    assert_eq!(std::fs::read_to_string(&pair).unwrap(), PAIR);
}

#[test]
fn every_incremental_compile_gives_what_a_full_build_of_the_same_sources_gives() {
    let f = Fixture::new("edit-incremental");
    let world = f.world();
    f.write("sample/edits/landblocks/0101.toml", HAND_WRITTEN);
    let mut s = open(&f, &world);
    let ops = vec![
        Op::PlaceStatic {
            landblock: 0x0202,
            model: DataId(GFXOBJ),
            at: Place {
                x: 10.0,
                y: 10.0,
                z: Height::Ground,
            },
            rotation: Rotation::Heading(0.0),
            name: None,
        },
        // A hill over the corner the four landblocks share, under the ground static.
        Op::SetVertices(
            [(15, 15), (16, 15), (16, 16), (17, 16), (16, 17)]
                .into_iter()
                .map(|(gx, gy)| VertexSet {
                    gx,
                    gy,
                    field: Field::Height,
                    value: 60,
                })
                .collect(),
        ),
        Op::MoveStatic {
            landblock: HOME,
            which: StaticOrigin::Base(0),
            at: Some(at(50.0, 60.0, 1.0)),
            rotation: Some(Rotation::Heading(10.0)),
        },
        Op::RemoveStatic {
            landblock: HOME,
            which: StaticOrigin::Base(1),
        },
        Op::PlaceSpawn {
            landblock: 0x0201,
            weenie: Weenie::Class(1),
            origin: Vec3::new(100.0, 100.0, 0.0),
            heading: 0.0,
        },
        Op::SetVertices(vec![VertexSet {
            gx: 16,
            gy: 16,
            field: Field::Road,
            value: 1,
        }]),
    ];
    for op in &ops {
        s.apply(op).unwrap();
        assert_same_as_full(&s);
    }
    for _ in 0..3 {
        s.undo().unwrap();
        assert_same_as_full(&s);
    }
    s.redo().unwrap();
    assert_same_as_full(&s);
    // And the overlay written from it is the build's, byte for byte.
    s.save().unwrap();
    compile::build(&world, &f.dats(), None, false).unwrap();
    let mine = f.path().join("mine");
    compile::write_overlay(s.compiled(), s.base(), "test-world", &mine).unwrap();
    for t in [ModernDat::Portal, ModernDat::Cell] {
        let name = dereth_dat::overlay::container_name(t);
        let built = f.out().join("overlay").join(name);
        assert_eq!(
            std::fs::read(mine.join(name)).ok(),
            std::fs::read(&built).ok(),
            "{name}"
        );
    }
}

#[test]
fn saving_writes_only_the_files_whose_text_changed() {
    let f = Fixture::new("edit-save");
    let world = f.world();
    let a = f.write("sample/edits/landblocks/0101.toml", HAND_WRITTEN);
    let b = f.write(
        "sample/edits/landblocks/0202.toml",
        "# untouched\n[[vertex]]\nat = [1, 1]\nheight = 12\n",
    );
    let mut s = open(&f, &world);
    // A change that is taken back again leaves its file as it was.
    s.apply(&Op::SetVertices(vec![VertexSet {
        gx: 17,
        gy: 17,
        field: Field::Height,
        value: 13,
    }]))
    .unwrap();
    s.undo().unwrap();
    s.apply(&Op::RemoveStatic {
        landblock: HOME,
        which: StaticOrigin::Base(0),
    })
    .unwrap();
    assert!(s.unsaved());
    assert_eq!(s.save().unwrap(), std::slice::from_ref(&a));
    assert!(!s.unsaved());
    assert_eq!(
        std::fs::read_to_string(&b).unwrap(),
        "# untouched\n[[vertex]]\nat = [1, 1]\nheight = 12\n"
    );
    assert!(std::fs::read_to_string(&a)
        .unwrap()
        .ends_with("\n[[remove-static]]\nindex = 0\nmodel = 0x01000001\n"));
    // The pack reads back as the session holds it.
    let pack = source::read_pack(&f.path().join("sample")).unwrap();
    assert_eq!(pack.edits.len(), 2);
}

#[test]
fn a_brush_stroke_dragged_over_several_frames_is_one_undo() {
    let f = Fixture::new("edit-stroke");
    let world = f.world();
    let mut s = open(&f, &world);
    let raise = |gx, value| {
        Op::SetVertices(vec![VertexSet {
            gx,
            gy: 8 + 4,
            field: Field::Height,
            value,
        }])
    };
    s.apply(&raise(8 + 3, 40)).unwrap();
    s.apply_into_last(&raise(8 + 4, 41)).unwrap();
    s.apply_into_last(&raise(8 + 5, 42)).unwrap();
    assert_eq!(s.history(), (1, 0));
    s.undo().unwrap();
    let t = s.terrain(HOME).unwrap();
    assert_eq!(
        (
            t.height[3 * 9 + 4],
            t.height[4 * 9 + 4],
            t.height[5 * 9 + 4]
        ),
        (17, 18, 19),
        "the fixture's own heights"
    );
    assert_same_as_full(&s);
}

#[test]
fn edits_written_as_text_read_and_one_that_does_not_read_says_which() {
    use lyceum_foundry::brush::Brush;
    use lyceum_foundry::script::{actions, Action};
    let a = actions(
        "[[edit]]\ndo = \"undo\"\n\n[[edit]]\ndo = \"brush\"\nbrush = \"raise\"\namount = 2\n\
         landblock = \"0101\"\nat = [3.0, 4.0]\nradius = 30\n",
        "sample",
    )
    .unwrap();
    assert_eq!(
        a,
        [
            Action::Undo,
            Action::Brush {
                brush: Brush::Raise(2),
                landblock: 0x0101,
                x: 3.0,
                y: 4.0,
                radius: 30.0
            }
        ]
    );
    let e = actions(
        "[[edit]]\ndo = \"undo\"\n\n[[edit]]\ndo = \"place-static\"\nlandblock = \"0101\"\n\
         model = 0x05000001\nat = [1, 2, 3]\n",
        "sample",
    )
    .unwrap_err();
    assert!(e.starts_with("edit 2: a static's model is"), "{e}");
    // A building moved, by its place in the statics record.
    let a = actions(
        "[[edit]]\ndo = \"move-building\"\nlandblock = \"0101\"\nindex = 2\n\
         at = [1.0, 2.0, 3.5]\nheading = 90\n",
        "sample",
    )
    .unwrap();
    assert_eq!(
        a,
        [Action::MoveBuilding {
            landblock: 0x0101,
            index: 2,
            at: [1.0, 2.0, 3.5],
            rotation: lyceum_foundry::source::Rotation::Heading(90.0),
        }]
    );
    let e = actions(
        "[[edit]]\ndo = \"move-building\"\nlandblock = \"0101\"\nat = [1, 2, 3]\n",
        "sample",
    )
    .unwrap_err();
    assert!(e.contains("index = 0"), "{e}");
}

#[test]
fn a_dungeon_is_named_and_given_a_look_in_its_landblocks_file_and_the_list_calls_it_so() {
    let f = Fixture::new("edit-dungeon-name");
    let world = f.world();
    let mut s = open(&f, &world);
    let lb = 0x0102;
    s.apply(&Op::SetDungeon {
        landblock: lb,
        name: Some("Warren of Tests".into()),
        look: Some(0x019E),
    })
    .unwrap();
    let info = s.dungeon_info(lb);
    assert_eq!(
        (info.name.as_deref(), info.look),
        (Some("Warren of Tests"), Some(0x019E))
    );
    // Named before its first room: the list has it by its name.
    let listed = s.dungeons(&std::collections::BTreeMap::new());
    assert!(listed
        .iter()
        .any(|d| d.landblock == lb && d.name.as_deref() == Some("Warren of Tests")));
    // Saved, the file reads back the same; unnamed, the section goes.
    s.save().unwrap();
    let text = std::fs::read_to_string(f.path().join("sample/edits/landblocks/0102.toml")).unwrap();
    assert!(
        text.contains("[dungeon]") && text.contains("name = \"Warren of Tests\""),
        "{text}"
    );
    let reopened = open(&f, &world);
    assert_eq!(reopened.dungeon_info(lb), info);
    s.apply(&Op::SetDungeon {
        landblock: lb,
        name: None,
        look: None,
    })
    .unwrap();
    assert_eq!(s.dungeon_info(lb), source::DungeonInfo::default());
}

#[test]
fn the_drop_point_is_where_the_portals_standing_outdoors_send_the_one_most_share() {
    use lyceum_foundry::edit::{drop_point, Entrance};
    let e = |cell: u32, to: (u32, f32)| Entrance {
        guid: String::new(),
        landblock: (cell >> 16) as u16,
        at: Vec3::ZERO,
        cell,
        to: Some((to.0, Vec3::new(to.1, 1.0, 0.0))),
    };
    let outdoor = 0xA9B4_0011;
    let indoor = 0x01FD_0101;
    // Two outdoors to one place, one outdoors to another, three from rooms to a third.
    let all = [
        e(outdoor, (0x019E_0114, 40.0)),
        e(outdoor, (0x019E_0114, 40.0)),
        e(outdoor, (0x019E_0100, 10.0)),
        e(indoor, (0x019E_0105, 5.0)),
        e(indoor, (0x019E_0105, 5.0)),
        e(indoor, (0x019E_0105, 5.0)),
    ];
    assert_eq!(
        drop_point(&all).map(|(c, p)| (c, p.x)),
        Some((0x019E_0114, 40.0))
    );
    // With none outdoors, the one most share of those in rooms.
    assert_eq!(
        drop_point(&all[3..]).map(|(c, p)| (c, p.x)),
        Some((0x019E_0105, 5.0))
    );
    assert_eq!(drop_point(&[]), None);
}
