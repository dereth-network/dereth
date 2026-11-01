//! The landscape constants.

use auberean_assets::region::{Landscape, VertexFields};
use auberean_world::land::LandConstants;

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
