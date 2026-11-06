//! Water: which landblocks hold it, at what height and of what kind.
//!
//! The region's water description holds two block maps, one byte per landblock: a height byte (an
//! index into the landscape's height table) and a water kind. A block has water when its kind
//! has water and the water's surface, a little under a meter above the height byte's height,
//! stands above the block's lowest ground. The surface is flat across the whole block; the ground
//! hides it wherever it is higher.

use auberean_assets::common::Color32;
use auberean_assets::region::{BlockMap, WaterDesc, WaterLight, WaterType};
use auberean_primitives::DataId;

use crate::land::LandConstants;

/// How far above its height byte's height the water's surface stands.
pub const SURFACE_RISE: f32 = 0.95;

/// The region's water: its description and its two block maps.
#[derive(Clone, Debug, PartialEq)]
pub struct WaterSet {
    pub desc: WaterDesc,
    pub heights: BlockMap,
    pub kinds: BlockMap,
}

/// One landblock's water.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlockWater {
    /// The surface's height, in meters.
    pub height: f32,
    /// The water's kind, its index in the description.
    pub kind: u32,
    /// The surface's material seen from above, and from below.
    pub above_material: DataId,
    pub below_material: DataId,
    /// The size of one repeat of the surface's picture, and of one of its polygons.
    pub tile_width: f32,
    pub poly_width: f32,
    pub color: Color32,
    /// What a body in it touches.
    pub physics_material: DataId,
}

/// A block map's value at block `(x, y)`, or its first value off the map.
// D2-12591
#[must_use]
pub fn block_map_value(m: &BlockMap, x: u32, y: u32) -> u8 {
    let at = if x < m.width && y < m.height {
        usize::try_from(m.height * x + y).ok()
    } else {
        Some(0)
    };
    at.and_then(|k| m.cells.get(k).copied()).unwrap_or(0)
}

/// The water kind of index `kind`: the description's kind with that index, else its default.
// D2-07522, D2-07523
#[must_use]
pub fn water_type(desc: &WaterDesc, kind: u32) -> Option<&WaterType> {
    desc.waters
        .iter()
        .find(|w| w.index == kind)
        .or(desc.default_water.as_ref())
}

impl WaterSet {
    /// The fog and light under the water of block `(x, y)`, by depth: its kind's table, empty for a
    /// kind with no water.
    // D2-10876
    #[must_use]
    pub fn lighting(&self, x: u32, y: u32) -> &[WaterLight] {
        let kind = u32::from(block_map_value(&self.kinds, x, y));
        water_type(&self.desc, kind)
            .filter(|t| t.no_water == 0)
            .map_or(&[], |t| t.lighting.as_slice())
    }

    /// The water of block `(x, y)`, whose lowest ground is `min_height`, if it has any.
    // D2-03723
    #[must_use]
    pub fn block(
        &self,
        land: &LandConstants,
        x: i32,
        y: i32,
        min_height: f32,
    ) -> Option<BlockWater> {
        let (x, y) = (u32::try_from(x).ok()?, u32::try_from(y).ok()?);
        let kind = u32::from(block_map_value(&self.kinds, x, y));
        let t = water_type(&self.desc, kind)?;
        if t.no_water != 0 {
            return None;
        }
        let byte = usize::from(block_map_value(&self.heights, x, y));
        let height = land.heights.get(byte).copied()? + SURFACE_RISE;
        // D2-07524, D2-07525, D2-07526, D2-07528
        (min_height < height).then_some(BlockWater {
            height,
            kind,
            above_material: t.above_material,
            below_material: t.below_material,
            tile_width: t.tile_width,
            poly_width: t.poly_width,
            color: t.color,
            // D2-07529
            physics_material: t.physics_material,
        })
    }
}
