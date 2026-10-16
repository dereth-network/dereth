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
    let water = super::super::Water {
        shore,
        depth: 2.5,
        shelf: 30.0,
    };
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
