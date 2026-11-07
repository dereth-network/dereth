//! Behaviour: none (tooling: what the Launch data overlay is distilled from, and its refusals)
//! `ac1999 dat-overlay` takes a commit's migrations only as reviewed, adds the Banderlings' casting
//! motion where each stance plays it, and walks a later file's records as far as ours lack them.
//! Fixture: synthetic commit listings, motion tables and records; with `launch-content`, the
//! operator's hash-pinned Launch files of `EMPYREAN_TEST_LAUNCH_FIXTURE`.

use std::collections::BTreeMap;

use super::*;

/// A commit's listing holding the named migrations as reviewed, and the tooling.
fn committed(files: &[&str]) -> BTreeMap<String, String> {
    let mut out: BTreeMap<String, String> = REVIEWED_TOOLING
        .iter()
        .map(|(p, h)| ((*p).to_owned(), (*h).to_owned()))
        .collect();
    for (name, hash, _) in REVIEWED.iter().filter(|(n, _, _)| files.contains(n)) {
        out.insert(format!("{MIGRATIONS}/{name}"), (*hash).to_owned());
    }
    out
}

fn all() -> Vec<&'static str> {
    REVIEWED.iter().map(|(n, _, _)| *n).collect()
}

#[test]
fn a_commit_with_the_eight_reviewed_migrations_takes_them_in_order() {
    let steps = plan(&committed(&all())).expect("reviewed");
    assert_eq!(steps.len(), 8);
    assert_eq!(steps[0], Step::Durations);
    assert_eq!(steps[2], Step::BanderlingCasts);
    assert_eq!(steps[7], Step::ResistableImperil);
    assert_eq!(steps.iter().filter(|s| s.is_event()).count(), 4);
}

#[test]
fn a_commit_with_the_first_three_takes_three() {
    let steps = plan(&committed(&all()[..3])).expect("reviewed");
    assert_eq!(
        steps,
        [Step::Durations, Step::SpellFixes, Step::BanderlingCasts]
    );
}

#[test]
fn a_migration_nobody_reviewed_is_refused_by_name() {
    let mut c = committed(&all());
    c.insert(
        format!("{MIGRATIONS}/0009-something-new.py"),
        "00".repeat(32),
    );
    let e = plan(&c).expect_err("unreviewed");
    assert!(e.contains("0009-something-new.py"), "{e}");
}

#[test]
fn a_reviewed_migration_whose_file_changed_is_refused_by_name() {
    let mut c = committed(&all());
    c.insert(
        format!("{MIGRATIONS}/0002-it160-spell-fixes.py"),
        "11".repeat(32),
    );
    let e = plan(&c).expect_err("changed");
    assert!(e.contains("0002-it160-spell-fixes.py has changed"), "{e}");
}

#[test]
fn a_changed_patcher_is_refused_whatever_the_migrations() {
    let mut c = committed(&all()[..1]);
    c.insert(REVIEWED_TOOLING[1].0.to_owned(), "22".repeat(32));
    let e = plan(&c).expect_err("changed tooling");
    assert!(e.contains("spell_patch.py"), "{e}");
}

#[test]
fn a_commit_in_the_middle_of_the_event_is_refused() {
    let e = plan(&committed(&all()[..4])).expect_err("only the start of the event");
    assert!(e.contains("in part"), "{e}");
}

/// A motion with `anims` animations of nothing.
/// The command's required options, then `extra`.
fn args(extra: &[&str]) -> Vec<String> {
    let revision = "e4".repeat(20);
    let mut out: Vec<String> = [
        "--revision",
        revision.as_str(),
        "--base",
        "launch",
        "--out",
        "overlay",
        "--world-key",
        "world",
    ]
    .iter()
    .map(|a| (*a).to_owned())
    .collect();
    out.extend(extra.iter().map(|a| (*a).to_owned()));
    out
}

#[test]
fn the_events_are_left_out_unless_asked_for() {
    let steps = plan(&committed(&all())).expect("reviewed");
    let o = parse(&args(&[])).expect("parsed");
    let (taken, left_out) = chosen(steps.clone(), o.with_events);
    assert_eq!(
        taken,
        [
            Step::Durations,
            Step::SpellFixes,
            Step::BanderlingCasts,
            Step::ResistableImperil
        ]
    );
    assert!(left_out);
    let o = parse(&args(&["--with-events"])).expect("parsed");
    assert_eq!(chosen(steps.clone(), o.with_events), (steps, false));
}

#[test]
fn a_commit_with_no_events_leaves_nothing_out() {
    let steps = plan(&committed(&all()[..3])).expect("reviewed");
    assert_eq!(chosen(steps.clone(), false), (steps, false));
}

#[test]
fn a_reference_is_refused_unless_the_events_are_asked_for() {
    let e = parse(&args(&["--reference", "portal.dat"])).expect_err("no events");
    assert!(e.contains("--with-events"), "{e}");
    let o = parse(&args(&["--with-events", "--reference", "portal.dat"])).expect("parsed");
    assert_eq!(o.references, [PathBuf::from("portal.dat")]);
}

#[test]
fn an_unknown_option_is_refused_by_name() {
    let e = parse(&args(&["--without-events"])).expect_err("unknown");
    assert!(
        e.contains("unknown") && e.contains("--without-events"),
        "{e}"
    );
}

#[test]
fn the_events_refuse_until_both_of_their_files_are_named() {
    let steps = plan(&committed(&all())).expect("reviewed");
    let e = check_references(&steps, false, true).expect_err("no retail CD portal");
    assert!(e.contains(cells::PORTAL_1999_10_09.sha256), "{e}");
    assert!(e.contains("--reference"), "{e}");
    let e = check_references(&steps, true, false).expect_err("no 2001 portal");
    assert!(e.contains(PORTAL_2001_09_12.sha256), "{e}");
    check_references(&steps, true, true).expect("both named");
    let (rules, _) = chosen(steps, false);
    check_references(&rules, false, false).expect("no events, no files");
}

fn motion(id: u32, anims: usize) -> Vec<u8> {
    motion_bytes(id, &vec![(0x0300_0001, 0, -1, 30.0); anims])
}

/// A Banderling-shaped table: no defaults or modifiers, the four casting cycles, each stance's
/// list from standing ready with six entries, and the list the new links go before.
fn table() -> Vec<u8> {
    let mut t = 0x0900_0007u32.to_le_bytes().to_vec();
    t.extend_from_slice(&0u32.to_le_bytes());
    t.extend_from_slice(&0u32.to_le_bytes());
    t.extend_from_slice(&4u32.to_le_bytes());
    for s in &STANCES {
        t.extend(motion((s.style << 16) | 0xD3, 1));
    }
    t.extend_from_slice(&0u32.to_le_bytes());
    t.extend_from_slice(&5u32.to_le_bytes());
    for s in &STANCES {
        t.extend_from_slice(&((s.style << 16) | 3).to_le_bytes());
        t.extend_from_slice(&6u32.to_le_bytes());
        for i in 0..6 {
            t.extend(motion(0x4500_0005 + i, 1));
        }
    }
    t.extend_from_slice(&LINKS_AFTER_CAST.to_le_bytes());
    t.extend_from_slice(&0u32.to_le_bytes());
    t
}

#[test]
fn each_banderling_stance_gains_a_way_into_casting_and_back() {
    let before = table();
    let after = patch_banderling_table(&before).expect("patched");
    // Four links into casting and four lists back to standing ready.
    let into: usize = STANCES.iter().map(|s| 8 + 16 * s.to_cast.len()).sum();
    let back: usize = STANCES.iter().map(|s| 16 + 16 * s.to_ready.len()).sum();
    assert_eq!(after.len(), before.len() + into + back);
    let links_at = 16 + 4 * 24 + 4;
    assert_eq!(read_u32(&after, links_at).expect("count"), 9);
    // Each casting cycle holds the arms up.
    assert_eq!(after[16..40], motion_bytes((0x47 << 16) | 0xD3, &[HOLD]));
    // The thrown stance's list from ready has a seventh entry, casting, in the sixth place.
    let list = links_at + 4;
    assert_eq!(read_u32(&after, list + 4).expect("entries"), 7);
    let sixth = (0..5).fold(list + 8, |at, _| past_motion(&after, at).expect("motion"));
    assert_eq!(read_u32(&after, sixth).expect("id"), MOTION_CAST_SPELL);
}

#[test]
fn a_banderling_table_that_already_casts_is_refused() {
    let once = patch_banderling_table(&table()).expect("patched");
    let e = patch_banderling_table(&once).expect_err("twice");
    assert!(e.contains("already casts"), "{e}");
}

#[test]
fn the_walk_takes_what_ours_lacks_and_stops_at_what_ours_has() {
    let setup = 0x0200_0001;
    let model = 0x0100_0001;
    let surface = 0x0800_0001;
    let picture = 0x0500_0001;
    let palette = 0x0400_0001;
    let sound = 0x2000_0001;
    let words = |ws: &[u32]| ws.iter().flat_map(|w| w.to_le_bytes()).collect::<Vec<u8>>();
    let mut later = BTreeMap::new();
    later.insert(setup, words(&[setup, model, sound, 0x0900_0999]));
    later.insert(model, words(&[model, 0, 1, surface]));
    later.insert(surface, words(&[surface, picture]));
    later.insert(picture, words(&[picture, 7, palette]));
    later.insert(palette, words(&[palette, 9]));
    later.insert(sound, words(&[sound, 1]));
    let ours: BTreeMap<u32, Vec<u8>> = [(surface, later[&surface].clone()), (sound, vec![0])]
        .into_iter()
        .collect();
    let (new, kept) = closure(
        &|id| later.get(&id).cloned(),
        &|id| ours.get(&id).cloned(),
        &[setup],
    )
    .expect("walked");
    assert_eq!(new.keys().copied().collect::<Vec<_>>(), [model, setup]);
    assert_eq!(kept, [sound]);
}

#[test]
fn a_picture_names_its_palette_and_nothing_else() {
    let mut picture = vec![0u8; 12];
    picture.extend_from_slice(&0x0400_0002u32.to_le_bytes());
    assert_eq!(
        references(0x0600_0001, &picture).expect("read"),
        [0x0400_0002]
    );
    picture.extend_from_slice(&7u32.to_le_bytes());
    assert!(references(0x0600_0001, &picture).expect("read").is_empty());
}

#[cfg(feature = "launch-content")]
mod launch {
    //! The operator's Launch pair, named by the fixture manifest.
    use super::*;

    fn portal() -> DatFile {
        let path = std::env::var_os("EMPYREAN_TEST_LAUNCH_FIXTURE")
            .expect("set EMPYREAN_TEST_LAUNCH_FIXTURE to the hash-pinned fixture manifest");
        let manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).expect("manifest")).expect("json");
        let dir = PathBuf::from(manifest["dat_directory"].as_str().expect("dat_directory"));
        pinned(&dir, "portal.dat", cells::PORTAL_1999_10_31).expect("the Launch portal")
    }

    fn files(portal: &DatFile) -> Files<'_> {
        Files {
            portal,
            retail_cd_portal: None,
            september_2001_portal: None,
        }
    }

    #[test]
    fn the_rules_of_the_reviewed_migrations_change_the_spell_table_and_the_banderlings() {
        let steps = plan(&committed(&all())).expect("reviewed");
        let rules: Vec<Step> = steps.into_iter().filter(|s| !s.is_event()).collect();
        let portal = portal();
        let edits = distill(&rules, &files(&portal)).expect("distilled");
        assert_eq!(
            edits.keys().copied().collect::<Vec<_>>(),
            [BANDERLING_MOTION_TABLE.raw(), SPELL_TABLE.raw()]
        );
        assert_eq!(
            source::hash(&edits[&BANDERLING_MOTION_TABLE.raw()]),
            BANDERLING_MOTION_TABLE_SHA256
        );
    }

    #[test]
    fn the_events_refuse_without_the_files_they_need() {
        let steps = plan(&committed(&all())).expect("reviewed");
        let portal = portal();
        let e = distill(&steps, &files(&portal)).expect_err("no references");
        assert!(e.contains("retail CD"), "{e}");
    }
}
