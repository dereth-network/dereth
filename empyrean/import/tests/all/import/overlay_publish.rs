//! Vectors: shared synthetic packs, overlay journals and patch files.
//! Published overlays equal importing the base and the exported journal.
//! Fixture: the shared synthetic pack and SQLite overlay.

use crate::support::overlay_fixture::*;

#[test]
fn publish_builds_what_serv_import_builds_from_the_base_and_the_journal() {
    let dir = tmp("publish");
    let (db, o) = open(&dir);
    o.import_sql(&fixture_sql("sql/1 weenies/00100 Test Drudge.sql"), "a.sql")
        .unwrap();
    o.import_sql(&fixture_sql("sql/1 weenies/90001 New Thing.sql"), "b.sql")
        .unwrap();
    o.import_sql(&fixture_sql("sql/2 landblocks/A9B4.sql"), "c.sql")
        .unwrap();

    let published = dir.join("published.pack");
    let imported = o.publish(&published, None).unwrap();
    let journal_dir = dir.join("journal");
    let files = o.export_journal(&journal_dir).unwrap();
    assert_eq!(files.len(), 3);
    let (bytes, _) = build(&Build {
        sql: base_sql(),
        inputs: o.inputs_with(&journal_dir),
        now: o.now(),
    })
    .unwrap();
    let cli = Pack::from_bytes(bytes).unwrap();
    let pubd = Pack::open(&published).unwrap();
    pubd.verify_hash().unwrap();
    assert_eq!(pubd.header().content_hash, cli.header().content_hash);
    assert_eq!(pubd.bytes(), cli.bytes());
    assert_eq!(imported.stats.content_hash, cli.header().content_hash);

    // What the server read through the overlay is what the published pack holds.
    let fresh = PackContent::new(Pack::open(&published).unwrap());
    for wcid in [100, 200, 90001] {
        assert_eq!(fresh.get_weenie(wcid), db.get_weenie(wcid), "weenie {wcid}");
    }
    for lb in [0xA9B4, 0x0007] {
        assert_eq!(
            fresh.get_cached_instances_by_landblock(lb),
            db.get_cached_instances_by_landblock(lb)
        );
    }
    // Every record the overlay holds equals the published pack's.
    for (t, k, d) in o.layer().iter() {
        assert_eq!(pubd.raw(t, k).unwrap(), d.map(|b| &b[..]), "{t:?} {k}");
    }

    // The pack the overlay sits on is never overwritten.
    let running = dir.join("running.pack");
    std::fs::write(&running, db.base().pack().bytes()).unwrap();
    assert!(matches!(
        o.publish(&running, None),
        Err(OverlayError::Mismatch(_))
    ));
}

#[test]
fn empyrean_import_overlay_publishes_the_file_and_checks_its_base() {
    let dir = tmp("empyrean-import-overlay");
    let (_db, o) = open(&dir);
    o.import_sql(&fixture_sql("sql/1 weenies/00100 Test Drudge.sql"), "a.sql")
        .unwrap();
    o.import_sql(b"DELETE FROM `quest` WHERE `name` = 'TestQuest';", "b.sql")
        .unwrap();
    let file = o.path();

    // `empyrean-import --sql base.sql --overlay <file> --overlay-journal <dir> --out <pack>`.
    let out = dir.join("out.pack");
    let jdir = dir.join("journal");
    let imported =
        empyrean_content::overlay::publish(&base_inputs(), &file, &out, None, Some(&jdir)).unwrap();
    assert_eq!(imported.applied.len(), 2);
    let published = Pack::open(&out).unwrap();
    // The exported journal rebuilds it as plain patches.
    let (bytes, _) = build(&Build {
        sql: base_sql(),
        inputs: o.inputs_with(&jdir),
        now: o.now(),
    })
    .unwrap();
    assert_eq!(published.bytes(), &bytes[..]);
    assert_eq!(
        published.header().content_hash,
        o.build().unwrap().1.stats.content_hash
    );
    assert!(published
        .get::<empyrean_content::models::world::Quest>(TableId::QUEST, 1)
        .unwrap()
        .is_none());

    // Another base is refused before anything is written.
    let wrong = BaseInputs {
        sql: base_sql(),
        patches: vec![Input {
            kind: InputKind::Sql,
            path: fixtures().join("sql/3 update.sql"),
        }],
        era: empyrean_common::era::EraId::Eor,
    };
    let err =
        empyrean_content::overlay::publish(&wrong, &file, &dir.join("wrong.pack"), None, None)
            .unwrap_err();
    assert!(err.to_string().contains("made over dataset"), "{err}");
    assert!(!dir.join("wrong.pack").exists());
    // A missing overlay file is an error, not a new empty overlay.
    assert!(empyrean_content::overlay::publish(
        &base_inputs(),
        &dir.join("none.sqlite"),
        &dir.join("x.pack"),
        None,
        None
    )
    .is_err());
    assert!(!dir.join("none.sqlite").exists());
}
