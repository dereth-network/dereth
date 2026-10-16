//! Behaviour: none (experimental Horizon rendering effects: which of a block's retained vertices
//! its objects' detail levels draw, listed without copying the geometry again)

use dereth_render_hifi::shared::static_feed::LevelRuns;
use dereth_render_hifi::snapshot::HifiLevelRun;

fn run(object: u32, level: u32, start: u32, end: u32) -> HifiLevelRun {
    HifiLevelRun {
        object,
        level,
        start,
        end,
        placed: true,
    }
}

/// A block's landscape (six vertices), then a batch of two objects at two levels each and a run
/// drawn at every distance: object 0's levels at 0..6 and 6..9, object 1's at 9..15 and 15..18,
/// and 18..21 always.
fn block() -> LevelRuns {
    let mut runs = LevelRuns::default();
    runs.every(6);
    runs.batch(
        &[
            run(0, 0, 0, 6),
            run(0, 1, 6, 9),
            run(1, 0, 9, 15),
            run(1, 1, 15, 18),
            run(HifiLevelRun::EVERY, 0, 18, 21),
        ],
        21,
    );
    runs
}

#[test]
fn each_object_draws_the_level_it_stands_at_and_the_rest_is_drawn_whatever_the_levels() {
    let runs = block();
    assert_eq!(runs.vertices(), 27);
    let at = |levels: &[u32]| runs.indices(levels);
    let landscape: Vec<u32> = (0..6).collect();
    let always: Vec<u32> = (24..27).collect();
    let expect = |parts: &[std::ops::Range<u32>]| {
        let mut v = landscape.clone();
        for p in parts {
            v.extend(p.clone());
        }
        v.extend(&always);
        v
    };
    assert_eq!(at(&[0, 0]), expect(&[6..12, 15..21]));
    assert_eq!(at(&[1, 0]), expect(&[12..15, 15..21]));
    assert_eq!(at(&[1, 1]), expect(&[12..15, 21..24]));
}

#[test]
fn an_object_with_no_level_or_at_a_level_it_has_no_run_for_draws_nothing_of_itself() {
    let runs = block();
    let only_landscape_and_always: Vec<u32> = (0..6).chain(24..27).collect();
    assert_eq!(runs.indices(&[]), only_landscape_and_always);
    assert_eq!(runs.indices(&[7, 7]), only_landscape_and_always);
}

#[test]
fn an_object_drawn_as_a_card_turned_to_the_eye_is_never_drawn_from_the_retained_geometry() {
    let mut runs = LevelRuns::default();
    runs.batch(
        &[
            HifiLevelRun {
                placed: false,
                ..run(0, 0, 0, 6)
            },
            run(1, 0, 6, 9),
        ],
        9,
    );
    assert_eq!(runs.indices(&[0, 0]), (6..9).collect::<Vec<u32>>());
}

#[test]
fn a_batch_with_no_runs_is_drawn_whole_and_runs_past_its_whole_triangles_are_cut() {
    let mut runs = LevelRuns::default();
    runs.batch(&[], 9);
    runs.batch(&[run(0, 0, 0, 7)], 6);
    assert_eq!(runs.vertices(), 15);
    assert_eq!(runs.indices(&[0]), (0..15).collect::<Vec<u32>>());
    assert_eq!(runs.indices(&[1]), (0..9).collect::<Vec<u32>>());
}

#[test]
fn the_finest_geometry_takes_every_object_at_its_finest_level_whatever_the_levels() {
    let runs = block();
    let finest: Vec<u32> = (0..12).chain(15..21).chain(24..27).collect();
    assert_eq!(runs.finest(), finest);
    let mut cards = LevelRuns::default();
    cards.batch(
        &[
            HifiLevelRun {
                placed: false,
                ..run(0, 0, 0, 6)
            },
            run(1, 0, 6, 9),
        ],
        9,
    );
    assert_eq!(cards.finest(), (6..9).collect::<Vec<u32>>());
}

#[test]
fn a_change_of_level_draws_the_same_of_a_copy_that_does_not_hold_the_object() {
    let runs = block();
    // Objects 0 and 1 are the copy's; object 2 is another copy's.
    assert_eq!(runs.choice(&[0, 1, 0]), runs.choice(&[0, 1, 1]));
    assert_eq!(runs.choice(&[0, 1]), runs.choice(&[0, 1, 2]));
    assert_ne!(runs.choice(&[0, 1, 0]), runs.choice(&[1, 1, 0]));
    // An object moved to a level it has no run for draws nothing of itself, as the list says.
    assert_ne!(runs.choice(&[0, 1]), runs.choice(&[0, 7]));
    assert_eq!(runs.choice(&[0, 7]), runs.choice(&[0, 9]));
}
