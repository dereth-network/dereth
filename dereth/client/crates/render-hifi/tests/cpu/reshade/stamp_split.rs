//! Behaviour: none (experimental Horizon rendering effects: where the world replay stops to keep
//! its depth before any building is stamped, which changes the work done and no pixel)

use dereth_render::{PipelineKey, VertexFormat};
use dereth_render_cpu::pso::portal_stamp_mask;
use dereth_render_hifi::composite::stamp_split;
use dereth_render_hifi::derive::cache::catalogue_keys;
use dereth_render_hifi::{DrawNote, Mark, SideTables};

/// A world of `n` commands with a draw at every one, the commands in `stamps` stamping a
/// building's openings, and these marks.
fn tables(n: u32, stamps: &[u32], marks: &[(u32, Mark)]) -> SideTables {
    let mut t = SideTables::default();
    t.marks.push((0, Mark::WorldBegin));
    t.marks.extend_from_slice(marks);
    t.marks.push((n, Mark::WorldEnd));
    t.marks.sort_by_key(|(at, _)| *at);
    let stamp = PipelineKey::portal_stamp(portal_stamp_mask::BUILDING);
    let ordinary = PipelineKey {
        vertex_format: VertexFormat::all()[0],
        ..catalogue_keys()[0]
    };
    for cmd in 0..n {
        t.draws.push(DrawNote {
            cmd,
            key: if stamps.contains(&cmd) {
                stamp
            } else {
                ordinary
            },
            splat: false,
            frame: 0,
        });
    }
    t
}

#[test]
fn the_replay_stops_at_the_first_building_stamp() {
    let t = tables(40, &[25, 26, 31], &[]);
    assert_eq!(stamp_split(&t, 0..40), Some(25));
    // Within a range that starts after it, the next stamp.
    assert_eq!(stamp_split(&t, 27..40), Some(31));
}

#[test]
fn the_replay_stops_before_the_marks_that_stand_at_the_stamp_so_none_is_handed_over_twice() {
    let t = tables(
        40,
        &[25],
        &[
            (25, Mark::Interiors),
            (24, Mark::TerrainEnd),
            (22, Mark::Interiors),
        ],
    );
    // Commands 24 and 25 have marks; 23 has none, so the second half starts there.
    assert_eq!(stamp_split(&t, 0..40), Some(23));
}

#[test]
fn a_range_with_no_stamp_after_its_start_is_replayed_whole() {
    assert_eq!(stamp_split(&tables(40, &[], &[]), 0..40), None);
    // A stamp only at the range's first command leaves nothing before it to keep.
    assert_eq!(stamp_split(&tables(40, &[10], &[]), 10..40), None);
    // Nor does a stamp whose marks reach back to the range's start.
    let t = tables(
        40,
        &[3],
        &[
            (1, Mark::Interiors),
            (2, Mark::Interiors),
            (3, Mark::Interiors),
        ],
    );
    assert_eq!(stamp_split(&t, 1..40), None);
    // A stamp outside the range is not looked at.
    assert_eq!(stamp_split(&tables(40, &[35], &[]), 0..30), None);
}
