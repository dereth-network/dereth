//! The landscape constants, and whether a point or a circle lies over a triangle of ground, as the
//! land's ground and slope checks ask it.

use auberean_assets::region::{Landscape, VertexFields};
use auberean_world::land::LandConstants;
use auberean_world::math::{circle_over_polygon, over_polygon};

fn landscape() -> Landscape {
    Landscape {
        cell_side: 20.0,
        road_width: 4.0,
        cells_per_block_side: 8,
        pixels_per_cell_side: 2,
        max_object_height: 200.0,
        sky_height: 2000.0,
        vertex_fields: VertexFields {
            terrain: 7,
            scene: 5,
            encounter: 4,
            color: 8,
            road: 4,
            replace: 4,
        },
        heights: (0..256u16).map(|i| f32::from(i) * 10.0).collect(),
    }
}

/// Twenty-unit cells, eight to a block, two vertex steps each, give 160-unit blocks of 17 x 17
/// vertices ten units apart; the vertex word's fields sit one after another from bit 0; and the
/// height table accepts heights up to the sky height less the greatest object height.
///
/// Behaviour: world.land.the-landscape-record-gives-the-grid-fields-and-heights
#[test]
fn the_landscape_record_gives_the_grid_fields_and_heights() {
    let c = LandConstants::from_record(&landscape()).expect("in range");
    let g = c.grid();
    assert_eq!(g.block_length(), 160.0);
    assert_eq!(g.vertices_per_side(), 17);
    assert_eq!(g.vertex_spacing(), 10.0);
    let w = 0b1010_0110_1100_1011_0011_1000_1101_0101u32;
    assert_eq!(c.word.terrain.of(w), w & 0x7F);
    assert_eq!(c.word.scene.of(w), (w >> 7) & 0x1F);
    assert_eq!(c.word.encounter.of(w), (w >> 12) & 0xF);
    assert_eq!(c.word.color.of(w), (w >> 16) & 0xFF);
    assert_eq!(c.word.road.of(w), (w >> 24) & 0xF);
    assert_eq!(c.word.replace.of(w), w >> 28);
    assert_eq!(c.spec.height_max, 1800.0);
    assert_eq!(c.heights[180], 1800.0);
    assert_eq!(
        c.heights[181], 0.0,
        "the first height above the bound stops the table"
    );
}

/// A record whose cells are not a power of two from 2 to 16 a side, or whose fields need more
/// than 32 bits, is refused.
///
/// Behaviour: world.land.a-landscape-record-out-of-range-is-refused
#[test]
fn a_landscape_record_out_of_range_is_refused() {
    let mut l = landscape();
    l.cells_per_block_side = 6;
    assert!(LandConstants::from_record(&l).is_err());
    let mut l = landscape();
    l.vertex_fields.replace = 5;
    assert!(LandConstants::from_record(&l).is_err());
}

/// A point over a polygon may lie outside an edge by a band of 0.0002, in the edge's own units
/// (its length times the distance): over a triangle whose edge along the x axis is 2 m long, a
/// point 0.00009 m south of it is over the triangle and one 0.00011 m south is not; seen from the
/// other side, the same point is over only the clockwise triangle.
///
/// Behaviour: world.land.a-point-within-the-band-of-an-edge-is-over-the-polygon
#[test]
fn a_point_within_the_band_of_an_edge_is_over_the_polygon() {
    let tri = [[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [0.0, 2.0, 0.0]];
    assert!(over_polygon(&tri, [0.5, 0.5, 0.0], true));
    assert!(over_polygon(&tri, [0.5, -0.000_09, 0.0], true));
    assert!(!over_polygon(&tri, [0.5, -0.000_11, 0.0], true));
    assert!(!over_polygon(&tri, [0.5, 0.5, 0.0], false));
    let turned = [tri[0], tri[2], tri[1]];
    assert!(over_polygon(&turned, [0.5, 0.5, 0.0], false));
}

/// The slope check's circle reaches a polygon by its edges' values, which grow with an edge's
/// length: a circle of 2 m about a point 1 m outside a triangle's 10 m edge, beside it, does not
/// reach the triangle (the edge's value there is 10), one about a point 0.1 m outside does (the
/// value is 1, and the point is beside the edge); a vertex within the radius reaches it from
/// anywhere, a point past both ends of an edge needs the next edge or a vertex, and a polygon
/// standing on its edge (no height to its normal) is never reached.
///
/// Behaviour: world.land.a-circle-reaches-a-polygon-by-its-edges-values
#[test]
fn a_circle_reaches_a_polygon_by_its_edges_values() {
    let tri = [[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [0.0, 10.0, 0.0]];
    let reaches = |x: f32, y: f32, r: f32| circle_over_polygon(&tri, 1.0, [x, y, 0.0], true, r);
    assert!(!reaches(5.0, -1.0, 2.0), "10 m along the edge stops it");
    assert!(reaches(5.0, -0.1, 2.0));
    assert!(reaches(11.0, -1.0, 2.0), "the corner is within 2 m");
    assert!(!reaches(13.0, -1.0, 2.0));
    assert!(reaches(3.0, 3.0, 0.5), "over the triangle");
    assert!(!circle_over_polygon(&tri, 0.0, [3.0, 3.0, 0.0], true, 0.5));
}
