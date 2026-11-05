//! The properties a region lays over the world, block by block: each is a list of values and a
//! block map saying which value holds at each landblock.

use auberean_assets::region::{BlockMap, MappedProperty, PropertyDesc};

use crate::water::block_map_value;

/// The property saying whether a block may be walked on.
const PASSABILITY: u32 = 0x8000_0004;
/// Its value for a block that may be.
const PASSABLE: u32 = 0x8000_0001;
/// Its value for a block that may not.
const IMPASSABLE: u32 = 0x8000_0002;

/// A region's property description and the block maps it names, in its order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BlockProperties {
    pub desc: PropertyDesc,
    pub maps: Vec<Option<BlockMap>>,
}

impl BlockProperties {
    /// The first entry for `property`, with its block map.
    #[must_use]
    pub fn entry(&self, property: u32) -> Option<(&MappedProperty, Option<&BlockMap>)> {
        let k = self
            .desc
            .properties
            .iter()
            .position(|p| p.property == property)?;
        Some((
            self.desc.properties.get(k)?,
            self.maps.get(k).and_then(Option::as_ref),
        ))
    }

    /// Whether a body may walk on block `(x, y)`: not when the passability map's value there says
    /// so; yes with no such property, map or value.
    // D2-12582, D2-12569
    #[must_use]
    pub fn passable(&self, x: u32, y: u32) -> bool {
        let Some((entry, Some(map))) = self.entry(PASSABILITY) else {
            return true;
        };
        let value = entry
            .values
            .get(usize::from(block_map_value(map, x, y)))
            .and_then(|p| p.value.inq_enum());
        match value {
            Some(PASSABLE) => true,
            Some(v) => v != IMPASSABLE,
            None => true,
        }
    }
}
