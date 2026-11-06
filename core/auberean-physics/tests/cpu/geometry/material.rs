//! What the region lays over a block: a color blended from the block's four corners.

use auberean_assets::common::Color32;
use auberean_assets::property::{Property, PropertyValue};
use auberean_assets::region::{BlockMap, MappedProperty};
use auberean_physics::material::block_property;
use auberean_physics::rules::BLOCK_LENGTH;
use dereth_primitives::Vec3;

const COLOR: u32 = 0x4000_0005;

fn color(word: u32) -> Property {
    Property {
        id: 0x77,
        value_type: COLOR,
        value: PropertyValue::Color(Color32(word)),
        ..Property::default()
    }
}

/// A block map of one block with four corners, each naming its own value: by columns east, each
/// from the south.
fn corners() -> BlockMap {
    BlockMap {
        height: 2,
        width: 2,
        cells: vec![0, 1, 2, 3],
    }
}

fn at(x: f32, y: f32, entry: &MappedProperty) -> Option<Property> {
    block_property(entry, &corners(), 0, 0, Vec3::new(x, y, 0.0))
}

/// A color laid over blocks blends channel by channel between the block's four corners, alpha
/// among them: each corner's own at the corner, the channels' middle in the middle. A corner of
/// another type gives no value.
///
/// Behaviour: data.block-property.a-color-blends-between-the-corners
#[test]
fn a_color_laid_over_blocks_blends_between_the_corners() {
    let mut entry = MappedProperty {
        property: 0x77,
        discrete: 0,
        values: vec![
            color(0xFF00_0000),
            color(0x0000_FF00),
            color(0x00FF_0000),
            color(0x0000_00FF),
        ],
        ..MappedProperty::default()
    };
    let word = |p: Option<Property>| p.and_then(|p| p.value.inq_color()).map(|c| c.0);
    assert_eq!(word(at(0.0, 0.0, &entry)), Some(0xFF00_0000));
    assert_eq!(word(at(BLOCK_LENGTH, 0.0, &entry)), Some(0x00FF_0000));
    assert_eq!(word(at(0.0, BLOCK_LENGTH, &entry)), Some(0x0000_FF00));
    assert_eq!(
        word(at(BLOCK_LENGTH, BLOCK_LENGTH, &entry)),
        Some(0x0000_00FF)
    );
    let middle = BLOCK_LENGTH / 2.0;
    let c = at(middle, middle, &entry)
        .and_then(|p| p.value.inq_color())
        .expect("a color")
        .channels();
    for channel in c {
        assert!((channel - 0.25).abs() < 0.01, "{c:?}");
    }
    entry.values[2] = Property {
        id: 0x77,
        value_type: COLOR,
        value: PropertyValue::Float(1.0),
        ..Property::default()
    };
    assert_eq!(at(0.0, 0.0, &entry), None, "a corner of another type");
}
