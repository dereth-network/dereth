//! How a world playing ac1999's loot rules names an item with its material ("Gold Gorget").
// Rules ported from ac1999 (Adam Plumb), AGPL-3.0: Server/Source/WeenieObject.cpp,
// Server/Source/PhatSDK/Support/TreasureTable.cpp.

use crate::world_objects::world_object::WorldObject;
use empyrean_entity::enums::{MaterialType, WeenieType};

/// Material names by material code, 1 (Ceramic) to 77 (Teak). The 1999 portal file has no
/// material name table, so these are carried here.
const MATERIALS: [&str; 77] = [
    "Ceramic",
    "Porcelain",
    "Cloth",
    "Linen",
    "Satin",
    "Silk",
    "Velvet",
    "Wool",
    "Gem",
    "Agate",
    "Amber",
    "Amethyst",
    "Aquamarine",
    "Azurite",
    "Black Garnet",
    "Black Opal",
    "Bloodstone",
    "Carnelian",
    "Citrine",
    "Diamond",
    "Emerald",
    "Fire Opal",
    "Green Garnet",
    "Green Jade",
    "Hematite",
    "Imperial Topaz",
    "Jet",
    "Lapis Lazuli",
    "Lavender Jade",
    "Malachite",
    "Moonstone",
    "Onyx",
    "Opal",
    "Peridot",
    "Red Garnet",
    "Red Jade",
    "Rose Quartz",
    "Ruby",
    "Sapphire",
    "Smoky Quartz",
    "Sunstone",
    "Tiger Eye",
    "Tourmaline",
    "Turquoise",
    "White Jade",
    "White Quartz",
    "White Sapphire",
    "Yellow Garnet",
    "Yellow Topaz",
    "Zircon",
    "Ivory",
    "Leather",
    "Armoredillo Hide",
    "Gromnie Hide",
    "Reed Shark Hide",
    "Metal",
    "Brass",
    "Bronze",
    "Copper",
    "Gold",
    "Iron",
    "Pyreal",
    "Silver",
    "Steel",
    "Stone",
    "Alabaster",
    "Granite",
    "Marble",
    "Obsidian",
    "Sandstone",
    "Serpentine",
    "Wood",
    "Ebony",
    "Mahogany",
    "Oak",
    "Pine",
    "Teak",
];

/// A material's name; a code outside the table reads "Unknown".
#[must_use]
pub fn material_name(material: MaterialType) -> &'static str {
    usize::try_from(material.0)
        .ok()
        .and_then(|code| code.checked_sub(1))
        .and_then(|i| MATERIALS.get(i))
        .copied()
        .unwrap_or("Unknown")
}

/// `name` with the object's material in front ("Gold Gorget"). A player, a gem (which would read
/// "Ruby Ruby"), leather and an object with no material keep `name` as it is.
#[must_use]
pub fn with_material(o: &WorldObject, name: String) -> String {
    if o.is_player() || o.weenie_type() == WeenieType::Gem {
        return name;
    }
    match o.material_type() {
        Some(m) if m.0 != 0 && m != MaterialType::Leather => {
            format!("{} {name}", material_name(m))
        }
        _ => name,
    }
}
