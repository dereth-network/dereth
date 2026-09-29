//! Vectors: tests/fixtures/patches and local overlay restart and atomicity cases.
//! Overlay updates remain atomic and survive reopening.
//! Fixture: the shared synthetic pack and SQLite overlay.

use crate::support::overlay_fixture::*;

#[test]
fn a_weenie_is_replaced_deleted_and_added_through_the_overlay() {
    let dir = tmp("weenie");
    let (db, o) = open(&dir);
    assert_eq!(ints(&db, 100), [(1, 16), (25, 5)]);
    let cached = db.get_cached_weenie(100).unwrap();

    // Replace (the fixture patch sets Level 7).
    let applied = o
        .import_sql(
            &fixture_sql("sql/1 weenies/00100 Test Drudge.sql"),
            "00100 Test Drudge.sql",
        )
        .unwrap();
    assert_eq!(applied.records["weenie"].replaced, 1);
    // The cache still holds the old weenie until it is cleared, as in ACE.
    assert!(Arc::ptr_eq(&db.get_cached_weenie(100).unwrap(), &cached));
    assert!(db.clear_cached_weenie(100));
    assert_eq!(
        db.get_cached_weenie(100).unwrap().class_name.as_deref(),
        Some("ace100-testdrudge")
    );
    assert_eq!(ints(&db, 100), [(1, 16), (25, 7)]);
    // The pack itself is untouched.
    assert!(
        db.base()
            .pack()
            .get::<empyrean_content::models::world::Weenie>(TableId::WEENIE, 100)
            .unwrap()
            .unwrap()
            .class_name
            == "testdrudge"
    );

    // Delete: the pack's record is hidden, and so is its index entry.
    o.import_sql(
        b"DELETE FROM `weenie` WHERE `class_Id` = 200;",
        "delete 200.sql",
    )
    .unwrap();
    assert!(db.get_weenie(200).is_none());
    assert!(!db.get_all_weenie_class_names().contains_key(&200));
    assert!(db.get_weenie_by_class_name("testsword").is_none());

    // Add.
    o.import_sql(
        &fixture_sql("sql/1 weenies/90001 New Thing.sql"),
        "90001 New Thing.sql",
    )
    .unwrap();
    let added = db.get_weenie(90001).unwrap();
    assert_eq!(
        db.get_weenie_by_class_name(&added.class_name)
            .unwrap()
            .class_id,
        90001
    );
    assert_eq!(
        db.get_all_weenie_class_names()
            .keys()
            .copied()
            .collect::<Vec<_>>(),
        [100, 90001]
    );
    assert_eq!(o.journal().unwrap().len(), 3);
}

#[test]
fn a_landblock_instance_is_replaced_deleted_and_added_through_the_overlay() {
    let dir = tmp("instance");
    let (db, o) = open(&dir);
    let lb = 0xA9B4;
    let before = db.get_cached_instances_by_landblock(lb);
    assert_eq!(before.len(), 2);

    // Replace one instance's position.
    o.import_sql(
        b"UPDATE `landblock_instance` SET `origin_X` = 99.5 WHERE `guid` = 0x7A9B4001;",
        "nudge.sql",
    )
    .unwrap();
    assert!(db.clear_cached_instances_by_landblock(lb));
    let now = db.get_cached_instances_by_landblock(lb);
    assert_eq!(
        now.iter().map(|i| i.origin_x).collect::<Vec<_>>(),
        [99.5, 11.0]
    );
    assert_eq!(
        now[0].landblock_instance_link.len(),
        1,
        "links stay with the rewritten record"
    );

    // Add one in a landblock the pack has no instances for.
    o.import_sql(
        b"INSERT INTO `landblock_instance` (`guid`, `weenie_Class_Id`, `obj_Cell_Id`, `origin_X`, `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`, `is_Link_Child`, `last_Modified`)\nVALUES (0x70123000, 100, 0x0123001F, 1, 2, 3, 1, 0, 0, 0, False, '2021-11-01 00:00:00');",
        "0123.sql",
    )
    .unwrap();
    let added = db.get_cached_instances_by_landblock(0x0123);
    assert_eq!(
        added.iter().map(|i| i.guid).collect::<Vec<_>>(),
        [0x7012_3000]
    );
    assert_eq!(
        db.get_landblock_instance_by_guid(0x7012_3000)
            .unwrap()
            .weenie_class_id,
        100
    );

    // Delete a whole landblock (the fixture's LandblockInstanceWriter-style file).
    o.import_sql(&fixture_sql("sql/2 landblocks/A9B4.sql"), "A9B4.sql")
        .unwrap();
    db.clear_cached_instances_by_landblock(lb);
    assert!(db.get_cached_instances_by_landblock(lb).is_empty());
    assert!(db.get_landblock_instance_by_guid(0x7A9B_4001).is_none());
    assert_eq!(
        db.get_landblock_instances_by_wcid(200)
            .iter()
            .map(|i| i.guid)
            .collect::<Vec<_>>(),
        [0x7000_7001]
    );
}

#[test]
fn the_overlay_survives_a_restart() {
    let dir = tmp("restart");
    {
        let (_db, o) = open(&dir);
        o.import_sql(&fixture_sql("sql/1 weenies/00100 Test Drudge.sql"), "a.sql")
            .unwrap();
        o.import_sql(
            b"DELETE FROM `landblock_instance` WHERE `guid` = 0x7A9B4002;",
            "b.sql",
        )
        .unwrap();
    }
    // A new process: the pack reopened and the overlay read back without replaying anything.
    let (db, o) = open(&dir);
    assert_eq!(ints(&db, 100), [(1, 16), (25, 7)]);
    assert_eq!(
        db.get_cached_instances_by_landblock(0xA9B4)
            .iter()
            .map(|i| i.guid)
            .collect::<Vec<_>>(),
        [0x7A9B_4001]
    );
    assert_eq!(
        o.journal()
            .unwrap()
            .iter()
            .map(|e| e.path.as_str())
            .collect::<Vec<_>>(),
        ["a.sql", "b.sql"]
    );
    // And a further write replays the journal first, so it builds on both.
    o.import_sql(
        b"UPDATE `weenie` SET `class_Name` = 'renamed' WHERE `class_Id` = 100;",
        "c.sql",
    )
    .unwrap();
    let w = db.get_weenie(100).unwrap();
    assert_eq!(
        (w.class_name.as_str(), w.weenie_properties_int.len()),
        ("renamed", 2)
    );
}

#[test]
fn a_failing_file_changes_nothing() {
    let dir = tmp("failing");
    let (db, o) = open(&dir);
    o.import_sql(&fixture_sql("sql/1 weenies/00100 Test Drudge.sql"), "a.sql")
        .unwrap();
    // The first statement would run, the second fails (duplicate primary key).
    let err = o
        .import_sql(b"DELETE FROM `weenie` WHERE `class_Id` = 200;\nINSERT INTO `weenie` VALUES (100, 'dup', 10, '2021-11-01 00:00:00');", "bad.sql")
        .unwrap_err();
    assert!(err.to_string().contains("bad.sql"), "{err}");
    assert!(
        db.get_weenie(200).is_some(),
        "the file's first statement is not kept"
    );
    assert_eq!(o.journal().unwrap().len(), 1);
    // The next write starts from the committed state, not from the half-run file.
    o.import_sql(
        b"UPDATE `weenie` SET `type` = 7 WHERE `class_Id` = 200;",
        "c.sql",
    )
    .unwrap();
    assert_eq!(db.get_weenie(200).unwrap().r#type, 7);
    assert_eq!(ints(&db, 100), [(1, 16), (25, 7)]);
}

#[test]
fn an_overlay_belongs_to_one_pack_and_one_base() {
    let dir = tmp("mismatch");
    {
        let (_db, o) = open(&dir);
        o.import_sql(b"DELETE FROM `weenie` WHERE `class_Id` = 200;", "a.sql")
            .unwrap();
    }
    // Over another pack, the file is refused.
    let other = Pack::from_bytes(
        build(&Build {
            sql: base_sql(),
            inputs: vec![Input {
                kind: InputKind::Sql,
                path: fixtures().join("sql/3 update.sql"),
            }],
            now: default_now(),
        })
        .unwrap()
        .0,
    )
    .unwrap();
    let err = ContentOverlay::open(
        &dir.join("overlay.sqlite"),
        base_inputs(),
        other.header(),
        default_now(),
    )
    .unwrap_err();
    assert!(matches!(err, OverlayError::Mismatch(_)), "{err}");

    // With base inputs that do not build the running pack, the first write is refused.
    let dir = tmp("wrong-base");
    let db = PackContent::new(other);
    let o = ContentOverlay::open(
        &dir.join("overlay.sqlite"),
        base_inputs(),
        db.base().pack().header(),
        default_now(),
    )
    .unwrap();
    let err = o
        .import_sql(b"DELETE FROM `weenie` WHERE `class_Id` = 200;", "a.sql")
        .unwrap_err();
    assert!(err.to_string().contains("dataset"), "{err}");
    assert!(o.journal().unwrap().is_empty());
}
