//! Behaviour: none (tooling: the checks of a retail building a pack moves, over the retail files)

use lyceum_foundry::edit::{Fault, Session};
use lyceum_foundry::source::Rotation;

/// The sample world open for editing.
fn session(tag: &str) -> (dereth_dat::testing::ScratchDir, Session) {
    let (dir, world) = super::sample::sample(tag);
    let mut s = Session::open(&world, &dereth_dat::testing::dat_dir(), None).unwrap();
    s.set_today("2026-10-19 00:00:00");
    (dir, s)
}

/// Building `index` of landblock `lb` moved to `at`, turned `heading`: the checks' faults about
/// it.
fn checks_after_moving(tag: &str, lb: u16, index: usize, at: [f32; 3], heading: f32) -> Vec<Fault> {
    let (dir, mut s) = session(tag);
    let op = s
        .move_building(lb, index, at, Rotation::Heading(heading))
        .unwrap();
    s.apply(&op).unwrap();
    let overlay = dir.path().join("overlay");
    s.write_scratch(&overlay).unwrap();
    let head = format!("0x{lb:04X}FFFE: building {index}:");
    s.check(&overlay)
        .unwrap()
        .into_iter()
        .filter(|f| f.text.starts_with(&head))
        .collect()
}

const HOLTBURG: u16 = 0xA9B4;

/// Holtburg's building 8, a cottage at (79.5, 37.5, 94.0), turned 45° (heading 45).
const COTTAGE: usize = 8;

#[test]
fn a_building_moved_a_little_on_level_ground_is_warned_of_nothing() {
    let f = checks_after_moving("moves-quiet", HOLTBURG, COTTAGE, [80.0, 37.5, 94.0], 45.0);
    assert!(f.is_empty(), "{f:#?}");
}

#[test]
fn a_building_moved_onto_another_is_warned_of_and_the_warning_stands_at_it() {
    let f = checks_after_moving("moves-onto", HOLTBURG, COTTAGE, [104.0, 36.0, 94.0], 45.0);
    let w = f
        .iter()
        .find(|f| f.text.contains("overlaps building 7"))
        .unwrap_or_else(|| panic!("{f:#?}"));
    assert!(w.warning);
    let (lb, at) = w.at.unwrap();
    assert_eq!(lb, HOLTBURG);
    assert!(
        (at.x - 104.0).abs() < 0.01 && (at.y - 36.0).abs() < 0.01,
        "{at:?}"
    );
}

#[test]
fn a_building_moved_over_a_static_is_warned_of() {
    // Static 95 stands at (106.0, 17.2, 94.0).
    let f = checks_after_moving("moves-static", HOLTBURG, COTTAGE, [106.0, 17.5, 94.0], 45.0);
    assert!(
        f.iter()
            .any(|f| f.text.contains("static 95") && f.text.contains("stands inside it")),
        "{f:#?}"
    );
}

#[test]
fn a_building_raised_off_the_ground_is_warned_of_its_doors() {
    let f = checks_after_moving("moves-raised", HOLTBURG, COTTAGE, [79.5, 37.5, 97.0], 45.0);
    assert!(
        f.iter()
            .any(|f| f.text.contains("door into cell") && f.text.contains("3.0 m over the ground")),
        "{f:#?}"
    );
}

#[test]
fn a_house_moved_off_its_plot_is_warned_of() {
    // 3C98's one building, a house on a plot of one land cell.
    let f = checks_after_moving("moves-house", 0x3C98, 0, [60.0, 60.0, 82.0], 0.0);
    assert!(
        f.iter()
            .any(|f| f.text.contains("a house moved off its plot")),
        "{f:#?}"
    );
}
