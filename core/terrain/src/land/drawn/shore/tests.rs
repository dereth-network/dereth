//! Behaviour: none (the water, an art-pack presentation's own: where it lies is worked out from
//! retail's water terrain types and heights)

use super::*;

/// A terrain word of terrain type `t`.
fn word(t: u16) -> u16 {
    t << 2
}

/// A `side` by `side` grid, row by row from the south, every vertex land of grass at height
/// `h`, with the vertices `wet` names water of fresh standing water.
fn grid(side: usize, h: f32, wet: &[(usize, usize)]) -> (Vec<f32>, Vec<u16>) {
    let heights = vec![h; side * side];
    let mut words = vec![word(1); side * side];
    for &(x, y) in wet {
        words[y * side + x] = word(17);
    }
    (heights, words)
}

#[test]
fn the_waters_edge_runs_halfway_between_a_water_vertex_and_the_land_beside_it() {
    let side = 9;
    // A lone water vertex in the middle.
    let (heights, words) = grid(side, 10.0, &[(4, 4)]);
    let f = water_field(side, &heights, &words);
    let at = |x: usize, y: usize| f[y * side + x][0];
    assert!((at(4, 4) - CELL * 0.5).abs() < 1e-3, "{}", at(4, 4));
    // The land beside it lies as far outside, so the edge is halfway along the cell's side.
    for (x, y) in [(3, 4), (5, 4), (4, 3), (4, 5)] {
        assert!(
            (at(x, y) + CELL * 0.5).abs() < 1e-3,
            "({x}, {y}) {}",
            at(x, y)
        );
    }
    // A diagonal neighbor lies farther out, so the edge bends round the water vertex.
    let diagonal = -(2f32.sqrt() - 0.5) * CELL;
    assert!((at(5, 5) - diagonal).abs() < 1e-3, "{}", at(5, 5));
    // Farther land lies farther out.
    assert!(at(0, 0) < at(5, 5));
}

#[test]
fn a_water_vertex_lies_deeper_inside_the_water_the_farther_it_is_from_land() {
    let side = 11;
    let wet: Vec<(usize, usize)> = (1..10).flat_map(|x| (1..10).map(move |y| (x, y))).collect();
    let (heights, words) = grid(side, 10.0, &wet);
    let f = water_field(side, &heights, &words);
    let at = |x: usize, y: usize| f[y * side + x][0];
    // From the edge in: half a cell, a cell and a half, two and a half.
    assert!((at(1, 5) - CELL * 0.5).abs() < 1e-3);
    assert!((at(2, 5) - CELL * 1.5).abs() < 1e-3);
    assert!((at(3, 5) - CELL * 2.5).abs() < 1e-3);
    assert!(at(5, 5) > at(3, 5));
    // The corner of the water lies no deeper inside it than the middle of its side.
    assert!(at(2, 2) < at(2, 5) + 1e-3);
}

#[test]
fn the_water_stays_level_up_to_a_rising_bank_and_follows_land_lower_than_it() {
    let side = 5;
    // Water at 10 m along the west two columns; the bank beside it rises to 13 m, and one vertex
    // of it lies at 8 m, lower than the water.
    let wet: Vec<(usize, usize)> = (0..5).flat_map(|y| [(0, y), (1, y)]).collect();
    let (mut heights, words) = grid(side, 10.0, &wet);
    for y in 0..side {
        heights[y * side + 2] = 13.0;
    }
    heights[2 * side + 2] = 8.0;
    let f = water_field(side, &heights, &words);
    let below = |x: usize, y: usize| f[y * side + x][1];
    // In the water, the level is the vertex's own.
    assert!(below(1, 1).abs() < 1e-6);
    // On the rising bank, the level lies as far below as the bank rises over the water.
    assert!((below(2, 0) - 3.0).abs() < 1e-4, "{}", below(2, 0));
    // On land lower than the water, the level is the land's own.
    assert!(below(2, 2).abs() < 1e-6, "{}", below(2, 2));
    // Land with no water about it keeps its own.
    assert!(below(4, 1).abs() < 1e-6);
}

#[test]
fn a_world_with_no_water_lies_outside_it_everywhere() {
    let side = 6;
    let (heights, words) = grid(side, 3.0, &[]);
    let f = water_field(side, &heights, &words);
    assert!(f
        .iter()
        .all(|[inside, below]| *inside == -REACH && *below == 0.0));
}

#[test]
fn a_value_is_held_as_the_nearest_half_float() {
    for v in [
        0.0f32,
        12.0,
        36.0,
        -12.0,
        3.3,
        -21.941_125,
        959.7,
        -960.0,
        0.000_7,
        1e-6,
    ] {
        let h = half(v);
        // A half float keeps eleven bits of a value: within one part in two thousand.
        assert!(
            (h - v).abs() <= v.abs() / 2048.0 + 6e-8,
            "{v} is held as {h}"
        );
        assert_eq!(half(h), h, "{v}: a held value is held as itself");
    }
}

/// A lake of `side` vertices a side at 10 meters, every vertex west of `shore` water, the land
/// east of it rising a meter a cell.
fn lake(side: usize, shore: usize) -> (Vec<f32>, Vec<u16>) {
    let mut heights = vec![10.0; side * side];
    let mut words = vec![word(17); side * side];
    for y in 0..side {
        for x in shore..side {
            // LINT-OK: a vertex count, exact in f32.
            #[allow(clippy::cast_precision_loss)]
            let rise = (x - shore) as f32;
            heights[y * side + x] = 10.0 + rise;
            words[y * side + x] = word(1);
        }
    }
    (heights, words)
}

#[test]
fn the_bank_beside_the_water_never_lies_under_its_level_at_the_edge_and_falls_away_without_a_step()
{
    let side = 24;
    let (heights, words) = lake(side, 8);
    let field = held(&water_field(side, &heights, &words));
    let shore = Shore {
        side,
        heights: &heights,
        field: &field,
    };
    // Out from the lake's edge, a bank whose ground the offset lowers by a meter once clear of
    // the water's pins: the floor holds it at the water's level at the edge, and lets it fall
    // away as it goes out, with no step.
    let water = super::super::Water { shore, slope: 3.0 };
    let knobs = super::super::Knobs::default();
    let src = |gx: i32, gy: i32| {
        // LINT-OK: clamped to a grid of two dozen vertices a side.
        #[allow(
            clippy::cast_sign_loss,
            clippy::cast_possible_wrap,
            clippy::cast_possible_truncation
        )]
        let k =
            gy.clamp(0, side as i32 - 1) as usize * side + gx.clamp(0, side as i32 - 1) as usize;
        super::super::Vertex {
            height: heights[k],
            word: words[k],
        }
    };
    struct Grid<F: Fn(i32, i32) -> super::super::Vertex>(F);
    impl<F: Fn(i32, i32) -> super::super::Vertex> super::super::GroundSource for Grid<F> {
        fn vertex(&self, gx: i32, gy: i32) -> Option<super::super::Vertex> {
            Some((self.0)(gx, gy))
        }
    }
    let grid = Grid(src);
    let y = 12.0 * CELL + 5.0;
    let mut last: Option<(f32, f32)> = None;
    let mut seen_edge = false;
    for k in 0..1200u16 {
        let x = 5.0 * CELL + f32::from(k) * 0.1;
        let inside = shore.distance(x, y, 0.0);
        let p = super::super::Point::at(&grid, x, y);
        let near = -super::super::smoothstep(4.0, 12.0, -inside);
        let h = super::super::drawn_at(&p, x, y, near, 0.0, Some(&water), &knobs).height;
        if inside <= 0.0 && inside > -2.0 {
            assert!(
                h >= shore.level(x, y) - 2.0 * FALL - 1e-4,
                "{x}: the bank at {h} dips under the water at {}",
                shore.level(x, y)
            );
            seen_edge = true;
        }
        if let Some((lx, lh)) = last {
            assert!(
                (h - lh).abs() < 0.05,
                "a step between {lx} and {x}: {lh} to {h}"
            );
        }
        last = Some((x, h));
    }
    assert!(seen_edge, "the line crosses the water's edge");
}

/// How far physics stands a body under retail's ground at world `(x, y)` of a grid whose vertices
/// `wet` says are water, as its cell's water corners say: 0.9 m in a cell of water corners alone,
/// 0.45 m or 0.1 m in a cell with some, by whether the corner nearest is water, and none in a cell
/// with none.
fn physics_sink(wet: impl Fn(i32, i32) -> bool, x: f32, y: f32) -> f32 {
    let (cx, cy) = (floor_to_i32(x / CELL), floor_to_i32(y / CELL));
    let corners = [
        wet(cx, cy),
        wet(cx + 1, cy),
        wet(cx, cy + 1),
        wet(cx + 1, cy + 1),
    ];
    // LINT-OK: cell coordinates, exact in f32.
    #[allow(clippy::cast_precision_loss)]
    let (fx, fy) = (x / CELL - cx as f32, y / CELL - cy as f32);
    let nearest = wet(cx + i32::from(fx > 0.5), cy + i32::from(fy > 0.5));
    if corners.iter().all(|w| *w) {
        0.9
    } else if !corners.iter().any(|w| *w) {
        0.0
    } else if nearest {
        0.45
    } else {
        0.1
    }
}

#[test]
fn the_bed_lies_where_physics_stands_a_body_eased_between_depths_and_sloping_up_to_the_edge() {
    let side = 24;
    // A pond with a ragged edge and a spit of land into it, so its cells hold every mix of water
    // and land corners.
    let wet: Vec<(usize, usize)> = (4..20)
        .flat_map(|x| (4..20).map(move |y| (x, y)))
        .filter(|&(x, y)| {
            !(x == 4 && y % 3 == 0) && !(y == 19 && x % 4 == 1) && !(x == 12 && y < 9)
        })
        .collect();
    let (heights, words) = grid(side, 10.0, &wet);
    let field = held(&water_field(side, &heights, &words));
    let shore = Shore {
        side,
        heights: &heights,
        field: &field,
    };
    let is_wet = |gx: i32, gy: i32| {
        usize::try_from(gx)
            .ok()
            .zip(usize::try_from(gy).ok())
            .is_some_and(|(x, y)| x < side && y < side && is_water(words[y * side + x]))
    };
    let slope = 3.0;
    let half = 0.5 * CELL;
    let to_seam = |v: f32| (v - (v / half).round() * half).abs();
    let (mut exact, mut eased, mut seen) = (0, 0, [0usize; 4]);
    for j in 0..400u16 {
        let y = 2.0 * CELL + f32::from(j) * 0.05 * CELL;
        let mut last: Option<(f32, f32)> = None;
        for i in 0..800u16 {
            let x = 2.0 * CELL + f32::from(i) * 0.025 * CELL;
            let want = physics_sink(is_wet, x, y);
            let inside = shore.distance(x, y, 0.0);
            let bed = shore.bed(x, y, inside, slope);
            let near_seam = to_seam(x) < SINK_EASE || to_seam(y) < SINK_EASE;
            if inside <= 0.0 {
                assert!(bed == 0.0, "({x}, {y}) {inside} outside the water: {bed}");
            } else if !near_seam && inside >= slope {
                // Clear of the edge's slope and of where quarters meet, exactly where physics
                // stands a body.
                assert!(
                    (bed - want).abs() < 1e-5,
                    "({x}, {y}) the bed {bed} m down where physics sinks a body {want} m"
                );
                exact += 1;
                let k = [0.0, 0.1, 0.45, 0.9]
                    .iter()
                    .position(|d: &f32| (d - want).abs() < 1e-6)
                    .expect("a depth");
                seen[k] += 1;
            } else {
                assert!((0.0..=0.9 + 1e-6).contains(&bed), "({x}, {y}) {bed}");
                eased += 1;
            }
            // Where physics sinks a body, the bed is never drawn deeper than it stands.
            if !near_seam {
                assert!(bed <= want + 1e-5, "({x}, {y}) the bed {bed} under {want}");
            }
            // No step anywhere: the bed slopes at most about one in two.
            if let Some((lx, lb)) = last {
                assert!(
                    (bed - lb).abs() <= 0.5 * 0.025 * CELL,
                    "a step from {lb} at {lx} to {bed} at {x}, {y} north"
                );
            }
            last = Some((x, bed));
        }
    }
    assert!(
        exact > 50_000 && eased > 10_000,
        "the bed matched where bodies stand {exact} times, eased {eased}"
    );
    assert!(
        seen[2] > 1000 && seen[3] > 1000,
        "the pond holds each depth: {seen:?}"
    );
}

#[test]
fn nothing_by_the_water_but_its_bed_is_drawn_lowered_or_raised() {
    let side = 24;
    // The lake's bank first falls a meter under the water's level, as beside a river on a
    // hillside, then rises.
    let (mut heights, words) = lake(side, 8);
    for y in 0..side {
        for x in 8..10 {
            heights[y * side + x] = 9.0;
        }
    }
    let field = held(&water_field(side, &heights, &words));
    let shore = Shore {
        side,
        heights: &heights,
        field: &field,
    };
    let water = super::super::Water { shore, slope: 3.0 };
    let knobs = super::super::Knobs::default();
    let src = |gx: i32, gy: i32| {
        // LINT-OK: clamped to a grid of two dozen vertices a side.
        #[allow(
            clippy::cast_sign_loss,
            clippy::cast_possible_wrap,
            clippy::cast_possible_truncation
        )]
        let k =
            gy.clamp(0, side as i32 - 1) as usize * side + gx.clamp(0, side as i32 - 1) as usize;
        super::super::Vertex {
            height: heights[k],
            word: words[k],
        }
    };
    struct Grid<F: Fn(i32, i32) -> super::super::Vertex>(F);
    impl<F: Fn(i32, i32) -> super::super::Vertex> super::super::GroundSource for Grid<F> {
        fn vertex(&self, gx: i32, gy: i32) -> Option<super::super::Vertex> {
            Some((self.0)(gx, gy))
        }
    }
    let grid = Grid(src);
    let y = 12.0 * CELL + 5.0;
    let (mut wet, mut dry) = (0, 0);
    for k in 0..1500u16 {
        let x = 3.0 * CELL + f32::from(k) * 0.1;
        let p = super::super::Point::at(&grid, x, y);
        // Without an offset the ground is retail's, less the bed where there is one, and there
        // is none out of the water.
        let bed = shore.bed(x, y, shore.distance(x, y, 0.0), 3.0);
        let h = super::super::drawn_at(&p, x, y, 0.0, 0.0, Some(&water), &knobs).height;
        assert!(
            (h - (p.retail - bed)).abs() < 1e-5,
            "{x}: drawn at {h}, retail's {} less the bed {bed}",
            p.retail
        );
        if shore.distance(x, y, 0.0) > 0.0 {
            wet += 1;
        } else {
            assert!(bed == 0.0, "{x}: a bed {bed} out of the water");
            dry += 1;
        }
    }
    assert!(
        wet > 50 && dry > 50,
        "the line crosses the water ({wet}) and the land ({dry})"
    );
}
