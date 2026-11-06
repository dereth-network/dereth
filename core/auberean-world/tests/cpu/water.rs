//! Water on synthetic block maps.

use auberean_assets::region::{BlockMap, Landscape, VertexFields, WaterDesc, WaterType};
use auberean_primitives::DataId;
use auberean_world::land::LandConstants;
use auberean_world::water::WaterSet;

fn land() -> LandConstants {
    LandConstants::from_record(&Landscape {
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
        heights: (0..256u16).map(|i| f32::from(i) * 2.0).collect(),
    })
    .expect("in range")
}

/// A 4 x 4 block map, x major: `cells[4 x + y]`.
fn map(f: impl Fn(u32, u32) -> u8) -> BlockMap {
    BlockMap {
        height: 4,
        width: 4,
        cells: (0..4)
            .flat_map(|x| (0..4).map(move |y| (x, y)))
            .map(|(x, y)| f(x, y))
            .collect(),
    }
}

fn water(index: u32, no_water: u32) -> WaterType {
    WaterType {
        index,
        no_water,
        above_material: DataId(0x3100_0001),
        tile_width: 10.0,
        poly_width: 160.0,
        ..WaterType::default()
    }
}

/// A block holds water when its kind has water and the surface, 0.95 above its height byte's
/// height, stands above the block's lowest ground; the maps are read x major.
///
/// Behaviour: world.water.a-block-holds-water-above-its-lowest-ground
#[test]
fn a_block_holds_water_above_its_lowest_ground() {
    let set = WaterSet {
        desc: WaterDesc {
            version: 0,
            height_map: DataId(0x1900_0000),
            kind_map: DataId(0x1900_0001),
            default_water: Some(water(0, 1)),
            waters: vec![water(1, 0)],
        },
        heights: map(|x, y| u8::try_from(10 * x + y).unwrap_or(0)),
        kinds: map(|x, _| u8::from(x == 2)),
    };
    let land = land();
    let w = set.block(&land, 2, 1, 0.0).expect("water at (2, 1)");
    assert!(
        (w.height - (2.0 * 21.0 + 0.95)).abs() < 1e-4,
        "{}",
        w.height
    );
    assert_eq!(w.above_material, DataId(0x3100_0001));
    assert!(
        set.block(&land, 2, 1, 50.0).is_none(),
        "ground everywhere above the surface"
    );
    assert!(
        set.block(&land, 1, 1, 0.0).is_none(),
        "the default kind has no water"
    );
}

/// A sky object shows by its water effect word: 0 above and under water alike, 1 only with the
/// camera under water, anything else never.
///
/// Behaviour: world.water.a-sky-object-shows-by-its-water-effect
#[test]
fn a_sky_object_shows_by_its_water_effect() {
    let piece = |water_effect| auberean_world::sky::SkyPiece {
        index: 0,
        object: DataId(0),
        heading: 0.0,
        pitch: 0.0,
        color: auberean_world::color::Color::WHITE,
        after: false,
        rotation: auberean_primitives::Quat::IDENTITY,
        water_effect,
    };
    assert!(piece(0).shows(false) && piece(0).shows(true));
    assert!(!piece(1).shows(false) && piece(1).shows(true));
    assert!(!piece(2).shows(false) && !piece(2).shows(true));
}
