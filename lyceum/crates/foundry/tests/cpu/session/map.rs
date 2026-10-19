//! Behaviour: none (tooling: the world editor's map: a landblock's pixels from its terrain, and the picture kept in a file)

use dereth_assets::world::{CellLandblock, VERTEX_COUNT};
use dereth_dat::testing::ScratchDir;
use dereth_primitives::DataId;
use lyceum_foundry::map::{pixel_of, world_of, WorldMap, SIDE};

/// A color for each terrain type: its value in red.
fn colors() -> Vec<[u8; 3]> {
    (0..32u8).map(|t| [t * 7, 120, 60]).collect()
}

/// Landblock `lb`'s terrain: every vertex terrain type 5 at height entry 10, but vertex (2, 3)
/// terrain type 9 and vertex (4, 4) a road.
fn block(lb: u16) -> CellLandblock {
    let mut terrain = [5u16 << 2; VERTEX_COUNT];
    terrain[2 * 9 + 3] = 9 << 2;
    terrain[4 * 9 + 4] = (5 << 2) | 1;
    CellLandblock {
        id: DataId((u32::from(lb) << 16) | 0xFFFF),
        lbi_exists: 0,
        terrain,
        height: [10; VERTEX_COUNT],
    }
}

/// The map's color at world vertex `(gx, gy)`.
fn at(map: &WorldMap, gx: usize, gy: usize) -> [u8; 3] {
    let o = ((2040 - gy) * SIDE + gx) * 4;
    [map.rgba[o], map.rgba[o + 1], map.rgba[o + 2]]
}

#[test]
fn a_landblock_is_drawn_a_pixel_a_vertex_in_its_terrains_colors_with_roads_over_them_north_up() {
    let heights: Vec<f32> = (0..256).map(|i| i as f32).collect();
    let mut map = WorldMap::default();
    let lb = 0xA9B4;
    map.draw_block(lb, &block(lb), &heights, &colors());
    let (bx, by) = (0xA9 * 8, 0xB4 * 8);
    // Flat ground: each vertex its terrain's color exactly.
    assert_eq!(at(&map, bx, by), [35, 120, 60]);
    assert_eq!(at(&map, bx + 2, by + 3), [63, 120, 60]);
    assert_ne!(at(&map, bx + 4, by + 4), at(&map, bx, by), "the road");
    // North up: the landblock's north row is the picture's upper one.
    assert_eq!(WorldMap::block_corner(lb), (bx, 2040 - (by + 7)));
    let pixels = map.block_pixels(lb);
    assert_eq!(pixels.len(), 8 * 8 * 4);
    // Row 4 from the top is y = 3; column 2.
    let o = (4 * 8 + 2) * 4;
    assert_eq!(&pixels[o..o + 3], &[63, 120, 60]);
    // Ground rising to the east is lit from the north-west, so it is brighter.
    let mut hill = block(0x0101);
    for x in 0..9 {
        for y in 0..9 {
            hill.height[x * 9 + y] = u8::try_from(x * 4).unwrap();
        }
    }
    map.draw_block(0x0101, &hill, &heights, &colors());
    assert!(at(&map, 8 + 1, 8 + 1)[1] > 120, "{:?}", at(&map, 9, 9));
}

#[test]
fn map_pixels_and_world_meters_turn_into_each_other() {
    assert_eq!(pixel_of(0.0, 0.0), (0.0, 2040.0));
    assert_eq!(pixel_of(24.0 * 10.0, 24.0 * 2040.0), (10.0, 0.0));
    let (x, y) = world_of(1356.5, 600.25);
    assert_eq!(pixel_of(x, y), (1356.5, 600.25));
}

#[test]
fn the_picture_kept_in_a_file_reads_back_as_it_was() {
    let dir = ScratchDir::new("pack-map").unwrap();
    let heights = vec![0.0; 256];
    let mut map = WorldMap::default();
    map.draw_block(0x1234, &block(0x1234), &heights, &colors());
    let file = dir.path().join("cache").join("map.png");
    map.save(&file).unwrap();
    assert_eq!(WorldMap::load(&file), Some(map));
    std::fs::write(&file, b"not a picture").unwrap();
    assert_eq!(WorldMap::load(&file), None);
}
