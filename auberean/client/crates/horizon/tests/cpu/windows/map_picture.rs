//! The map's picture, drawn from the world's vertices as Dereth's Horizon draws its own world: a
//! vertex's far color on the flat, the light by the land's slope, the sea where no landblock lies
//! or the ground is under water with a lighter band along the shore, the roads, and the grid.
//!
//! Behaviour: none (this client's own interface, not a behaviour of the retail client)

use auberean_horizon::world_map::{sea_color, MapGrid, WorldMap};

/// A height table a meter a step.
fn heights() -> [f32; 256] {
    let mut h = [0.0; 256];
    for (i, v) in h.iter_mut().enumerate() {
        *v = f32::from(u8::try_from(i).unwrap_or(0));
    }
    h
}

/// A world of `blocks` landblocks of sixteen squares, ten meters apart.
fn grid(blocks: (usize, usize)) -> MapGrid {
    MapGrid::new(blocks, 16, 10.0, heights())
}

/// Land over the vertices `x`, `y`: its height-table index and far color by vertex.
fn land(
    g: &mut MapGrid,
    x: std::ops::RangeInclusive<usize>,
    y: std::ops::RangeInclusive<usize>,
    at: impl Fn(usize, usize) -> (u8, [u8; 3]),
) {
    for vx in x {
        for vy in y.clone() {
            let (h, c) = at(vx, vy);
            g.set((vx, vy), h, c, false, false);
        }
    }
}

/// The pixel of square `(x, y)` (counted from the south-west), red, green and blue.
fn square(map: &WorldMap, (x, y): (usize, usize)) -> [u8; 3] {
    let row = map.size().1 - 1 - y;
    let p = map.window((x, row), (1, 1));
    [p.bgra[2], p.bgra[1], p.bgra[0]]
}

const GREEN: [u8; 3] = [100, 150, 50];
const SHORE: [u8; 3] = [148, 167, 190];
const ROAD: [u8; 3] = [186, 181, 167];

#[test]
fn flat_ground_is_drawn_in_its_far_color_and_where_no_landblock_lies_the_sea_is_lighter_by_the_shore(
) {
    let mut g = grid((2, 2));
    land(&mut g, 0..=16, 0..=16, |_, _| (10, GREEN));
    let map = g.prepare();
    assert_eq!(map.size(), (32, 32), "a pixel a square, north at the top");
    assert_eq!(square(&map, (4, 4)), GREEN, "flat: the vertex's own color");
    assert_eq!(
        square(&map, (18, 4)),
        SHORE,
        "two squares off the land: the shore's band"
    );
    assert_eq!(
        square(&map, (29, 29)),
        sea_color(),
        "far from the land: the open sea"
    );
}

#[test]
fn a_slope_facing_the_north_west_is_drawn_lighter_and_one_facing_away_darker() {
    let mut g = grid((1, 1));
    // A ridge along x = 8: rising two meters a square from the west, falling to the east.
    land(&mut g, 0..=16, 0..=16, |x, _| {
        let up = u8::try_from(8 - x.abs_diff(8)).unwrap_or(0);
        (20 + 2 * up, GREEN)
    });
    let map = g.prepare();
    let sum = |c: [u8; 3]| c.iter().map(|v| u32::from(*v)).sum::<u32>();
    let flat = sum(GREEN);
    let west = sum(square(&map, (3, 8)));
    let east = sum(square(&map, (12, 8)));
    assert!(west > flat && east < flat, "{west} {flat} {east}");
}

#[test]
fn ground_under_water_is_sea_and_a_road_is_drawn_in_the_roads_color() {
    let mut g = grid((1, 1));
    for x in 0..=16 {
        for y in 0..=16 {
            let wet = x >= 12;
            let road = (4..=5).contains(&y) && x < 12;
            g.set((x, y), 10, GREEN, wet, road);
        }
    }
    let map = g.prepare();
    assert_eq!(
        square(&map, (14, 10)),
        SHORE,
        "under water: sea, by the shore"
    );
    assert_eq!(square(&map, (6, 4)), ROAD, "between two road vertices");
    assert_eq!(square(&map, (6, 10)), GREEN);
}

#[test]
fn a_grid_line_is_drawn_every_five_units_through_the_middle() {
    // 800 pixels a side: a unit of the map's coordinates is four pixels, five twenty.
    let mut g = grid((50, 50));
    land(&mut g, 0..=800, 0..=800, |_, _| (10, GREEN));
    let map = g.prepare();
    let line = [72, 72, 72];
    for x in [400, 420, 380, 40, 760] {
        assert_eq!(square(&map, (x, 77)), line, "column {x}");
    }
    // Rows from the top: the 400th is the middle, the 380th five units north of it.
    for y in [399, 419, 379] {
        assert_eq!(square(&map, (77, y)), line, "{y} squares up");
    }
    assert_eq!(square(&map, (410, 77)), GREEN);
    assert_eq!(square(&map, (0, 77)), GREEN, "none on the edge");
    // A world too small for the lines to stand apart has none.
    let mut g = grid((4, 4));
    land(&mut g, 0..=64, 0..=64, |_, _| (10, GREEN));
    let small = g.prepare();
    assert_eq!(square(&small, (32, 10)), GREEN);
}

#[test]
fn the_tiles_cut_the_picture_and_the_overview_is_its_longer_side_shrunk() {
    let mut g = grid((80, 40));
    land(&mut g, 0..=1280, 0..=640, |_, _| (10, GREEN));
    let map = g.prepare();
    assert_eq!(map.size(), (1280, 640));
    assert_eq!(map.tiles(), (2, 1));
    let last = map.tile((1, 0));
    assert_eq!(
        (last.width, last.height),
        (256, 640),
        "short at the east and south"
    );
    assert_eq!(map.overview_size(), (2048, 1024));
}

/// An island of two landblocks by two whose south-west block is `(bx, by)`, its heights `height`
/// of its vertices from the island's south-west.
fn island(g: &mut MapGrid, (bx, by): (usize, usize), height: impl Fn(usize, usize) -> u8) {
    let (x0, y0) = (bx * 16, by * 16);
    land(g, x0 + 1..=x0 + 31, y0 + 1..=y0 + 31, |x, y| {
        (height(x - x0, y - y0), GREEN)
    });
}

/// Rolling ground, the same wherever it is laid.
fn rolling(x: usize, y: usize) -> u8 {
    u8::try_from(10 + (x * 7 + y * 3) % 50).unwrap_or(0)
}

#[test]
fn stretches_of_land_that_copy_one_another_are_left_out_and_land_like_no_other_stays() {
    let mut g = grid((24, 12));
    // Three copies of one island, one of them touched up at a vertex.
    for bx in [2, 8, 14] {
        island(&mut g, (bx, 8), rolling);
    }
    g.set((14 * 16 + 5, 8 * 16 + 5), 99, GREEN, false, false);
    // An island laid out as they are, with ground of its own, and a larger stretch.
    island(&mut g, (20, 8), |x, y| {
        u8::try_from(10 + (x * 5 + y * 11) % 40).unwrap_or(0)
    });
    land(&mut g, 17..=150, 17..=90, |x, y| (rolling(x, y), GREEN));
    let before = g.landmasses();
    assert_eq!(before.len(), 5);
    let dropped = g.drop_placeholder_land();
    let corners: Vec<_> = dropped
        .iter()
        .map(|p| (p.south_west, p.north_east, p.blocks))
        .collect();
    assert_eq!(
        corners,
        [
            ((2, 8), (3, 9), 4),
            ((8, 8), (9, 9), 4),
            ((14, 8), (15, 9), 4)
        ],
        "the three copies, from the west"
    );
    let after = g.landmasses();
    assert_eq!(
        after.len(),
        2,
        "the island of its own and the larger stretch"
    );
    assert!(before.contains(&after[0]) && before.contains(&after[1]));
    // Left out as sea in the picture.
    let map = g.prepare();
    assert_eq!(square(&map, (8 * 16 + 16, 8 * 16 + 16)), sea_color());
    assert_ne!(
        square(&map, (20 * 16 + 16, 8 * 16 + 16)),
        sea_color(),
        "kept"
    );
}

#[test]
fn islands_alike_in_shape_but_not_in_their_heights_all_stay() {
    let mut g = grid((24, 12));
    for (n, bx) in [2usize, 8, 14].into_iter().enumerate() {
        island(&mut g, (bx, 8), move |x, y| {
            u8::try_from(10 + (x * (3 + 2 * n) + y * (7 + n)) % 50).unwrap_or(0)
        });
    }
    assert!(g.drop_placeholder_land().is_empty());
    assert_eq!(g.landmasses().len(), 3);
}

#[test]
fn copies_that_differ_at_more_than_a_hundredth_of_their_vertices_stay() {
    let mut g = grid((24, 12));
    for bx in [2, 8] {
        island(&mut g, (bx, 8), rolling);
    }
    // Two landblocks by two hold 4 × 17 × 17 vertices; 12 of them differ.
    for i in 0..12 {
        g.set((8 * 16 + 4 + i, 8 * 16 + 4), 99, GREEN, false, false);
    }
    assert!(g.drop_placeholder_land().is_empty());
}

#[test]
fn a_speck_of_flat_ground_far_from_other_land_is_left_out_and_land_near_or_with_relief_stays() {
    use auberean_horizon::world_map::Why;
    let mut g = grid((128, 128));
    // A stretch of rolling ground, landblocks 40 to 79 across and 40 to 59 up; a flat speck 20
    // landblocks to its east; a flat speck at another height and an islet with relief 60 to
    // its north.
    land(&mut g, 641..=1279, 641..=959, |x, y| (rolling(x, y), GREEN));
    land(&mut g, 1601..=1615, 721..=735, |_, _| (12, GREEN));
    land(&mut g, 961..=975, 1921..=1935, |_, _| (10, GREEN));
    land(&mut g, 1601..=1615, 1921..=1935, |x, y| {
        (rolling(x, y), GREEN)
    });
    let dropped = g.drop_placeholder_land();
    let left: Vec<_> = dropped.iter().map(|p| (p.why, p.south_west)).collect();
    assert_eq!(
        left,
        [(Why::Speck, (60, 120))],
        "the flat speck far out alone"
    );
    assert_eq!(g.landmasses().len(), 3);
}

#[test]
fn a_world_of_one_flat_stretch_keeps_it() {
    let mut g = grid((8, 8));
    land(&mut g, 17..=40, 17..=40, |_, _| (10, GREEN));
    assert!(g.drop_placeholder_land().is_empty());
}

#[test]
fn the_land_framed_holds_all_the_land_but_the_placeholder_land() {
    let mut g = grid((128, 128));
    // Rolling ground, landblocks 40 to 79 across and 40 to 59 up; an islet with relief 60 to its
    // north; a flat speck far out to its north-west.
    land(&mut g, 641..=1279, 641..=959, |x, y| (rolling(x, y), GREEN));
    land(&mut g, 1601..=1615, 1921..=1935, |x, y| {
        (rolling(x, y), GREEN)
    });
    land(&mut g, 161..=175, 1601..=1615, |_, _| (10, GREEN));
    let b = g.clone().prepare().land_bounds().expect("land");
    let blocks = |v: f32| v * 128.0;
    assert_eq!(
        [b.west, b.south, b.east, b.north].map(blocks),
        [40.0, 40.0, 101.0, 121.0],
        "the stretch and the islet, not the speck"
    );
    assert_eq!(grid((4, 4)).land_bounds(), None, "no land");
}
