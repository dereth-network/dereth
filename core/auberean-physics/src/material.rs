//! What a place makes of the body in it: a body keeps a physics material, and each cell it goes
//! into updates that material with what holds there. The region's properties lay values over the
//! world block by block, a discrete one taking the value of the block, the others blended between
//! the values at the block's corners; an interior cell adds its landblock's own properties and
//! then its own. The material's gravity is the body's, and its properties are the surroundings'
//! to its collisions.

use auberean_assets::property::{Property, PropertyCollection, PropertyList, PropertyValue};
use auberean_assets::region::{BlockMap, MappedProperty};
use auberean_primitives::Vec3 as AssetVec3;
use auberean_world::property::BlockProperties;
use auberean_world::water::block_map_value;
use dereth_primitives::Vec3;

use crate::cells::interpolate;

/// The property naming the gravity where a body is.
pub const GRAVITY: u32 = 0x8000_0001;

/// What a blended property's values are.
const INTEGER: u32 = 0x4000_0002;
const FLOAT: u32 = 0x4000_0003;
const VECTOR: u32 = 0x4000_0004;
const COLOR: u32 = 0x4000_0005;

/// `entry`'s value at point `at` of block `(x, y)`: the block's own value when it is discrete, else
/// the values at the block's four corners blended at the point, all four being of the property's
/// type; `None` when one is missing.
// D2-12581, D2-12564, D2-12562, D2-12565, D2-12566, D2-12567, D2-12573, D2-12574, D2-12575, D2-12568
#[must_use]
pub fn block_property(
    entry: &MappedProperty,
    map: &BlockMap,
    x: u32,
    y: u32,
    at: Vec3,
) -> Option<Property> {
    let value = |x: u32, y: u32| entry.values.get(usize::from(block_map_value(map, x, y)));
    if entry.discrete != 0 {
        return value(x, y).cloned();
    }
    let corners = [
        value(x, y)?,
        value(x + 1, y)?,
        value(x, y + 1)?,
        value(x + 1, y + 1)?,
    ];
    let blend = |c: [f32; 4]| interpolate(at, c[0], c[1], c[2], c[3]);
    let first = corners[0];
    let new = match first.value_type {
        INTEGER => {
            let mut c = [0.0; 4];
            for (k, p) in corners.iter().enumerate() {
                #[allow(clippy::cast_precision_loss)]
                {
                    c[k] = p.value.inq_integer()? as f32;
                }
            }
            PropertyValue::Integer(dereth_primitives::num::floor_to_i32(blend(c)))
        }
        FLOAT => {
            let mut c = [0.0; 4];
            for (k, p) in corners.iter().enumerate() {
                c[k] = p.value.inq_float()?;
            }
            PropertyValue::Float(blend(c))
        }
        VECTOR => {
            let mut c = [AssetVec3::new(0.0, 0.0, 0.0); 4];
            for (k, p) in corners.iter().enumerate() {
                c[k] = p.value.inq_vector()?;
            }
            PropertyValue::Vector(AssetVec3::new(
                blend(c.map(|v| v.x)),
                blend(c.map(|v| v.y)),
                blend(c.map(|v| v.z)),
            ))
        }
        // D2-12576
        COLOR => {
            let mut c = [[0.0; 4]; 4];
            for (k, p) in corners.iter().enumerate() {
                c[k] = p.value.inq_color()?.channels();
            }
            let part = |i: usize| blend(c.map(|v| v[i]));
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u32;
            PropertyValue::Color(auberean_assets::common::Color32(
                (byte(part(3)) << 24)
                    | (byte(part(0)) << 16)
                    | (byte(part(1)) << 8)
                    | byte(part(2)),
            ))
        }
        _ => return None,
    };
    Some(Property {
        value: new,
        ..first.clone()
    })
}

/// Updates `material` with every property the region lays over block `(x, y)`, at point `at` of it.
// D2-12585, D2-13691
pub fn update_block_material(
    properties: &BlockProperties,
    material: &mut PropertyCollection,
    list: &PropertyList,
    x: u32,
    y: u32,
    at: Vec3,
) {
    for (entry, map) in properties.desc.properties.iter().zip(&properties.maps) {
        let Some(map) = map else {
            continue;
        };
        if let Some(p) = block_property(entry, map, x, y, at) {
            material.update_property(list, &p);
        }
    }
}
