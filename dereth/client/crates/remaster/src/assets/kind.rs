//! What a material is, as the later light and look passes read it: a [`Kind`], from the data
//! category of the picture a surface draws, with the exceptions written out.
//!
//! Retail's pictures carry a data category from 0 to 10. The client itself reads only two of
//! them (6, the interface's pictures, and 10, pictures kept in their authored format), but the
//! categories follow what the pictures are of, and every surface record of the dats draws a
//! picture of category 0, 1, 2, 3, 4, 5, 7, 8 or 9. [`BY_CATEGORY`] gives each category its kind;
//! [`of`] then makes the exceptions, in this order:
//!
//! 1. where it is drawn settles the ground and the sky ([`Use::Terrain`], [`Use::Sky`]);
//! 2. a surface that adds its light onto what is behind it is an effect wherever it is drawn;
//! 3. an untextured surface has no picture and so no category: it is architecture on a static or
//!    in a cell, an item on an object.

use crate::assets::Material;

/// What a material is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Kind {
    /// The landscape's ground, and the pictures the ground is made from.
    Ground,
    /// Trees, bushes, grass, flowers and other plants.
    Foliage,
    /// Bodies, faces and what characters wear, and creatures' skins.
    Character,
    /// Things carried, worn on the body as gear, or set down: weapons, shields, tools, signs,
    /// scrolls and small objects.
    Item,
    /// Buildings, walls, floors, roofs, rooms and other built structures, and the world's rock.
    Architecture,
    /// Glows, sparks, flames, spell effects and other light-giving pictures.
    Effect,
    /// The sky's dome, clouds, sun, moons and stars.
    Sky,
    /// Water: a ground cell whose every corner is water.
    Water,
}

/// Where a surface is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Use {
    /// A block's ground.
    Terrain,
    /// An interior cell's own walls, floors and ceilings.
    Cell,
    /// A static, a piece of scenery or a building's shell placed in a block or a cell.
    Static,
    /// An object the server placed, or the player's body.
    Object,
    /// An object of the sky.
    Sky,
}

/// Each data category's kind, with what the pictures of that category are.
pub const BY_CATEGORY: &[(u32, Kind)] = &[
    // A handful of decorations: hangings, doors, ornaments.
    (0, Kind::Architecture),
    // Trees, bark, leaves, grass and flowers.
    (1, Kind::Foliage),
    // The ground's terrain pictures, water among them.
    (2, Kind::Ground),
    // The sky's gradients, clouds, sun, moons and stars.
    (3, Kind::Sky),
    // Faces, bodies, clothing and armor.
    (4, Kind::Character),
    // Creatures' skins, scales, fur and wings.
    (5, Kind::Character),
    // The interface's pictures; no surface draws one.
    (6, Kind::Item),
    // Weapons, shields, signs, scrolls, furniture and other objects.
    (7, Kind::Item),
    // Walls, floors, roofs, wood, stone and buildings' fronts.
    (8, Kind::Architecture),
    // Glows, sparks, swirls and other effects.
    (9, Kind::Effect),
    // The ground's blend masks, kept as they were authored; no surface draws one.
    (10, Kind::Ground),
];

/// The kind of a picture of data category `category`, when the table has one.
#[must_use]
pub fn of_category(category: u32) -> Option<Kind> {
    BY_CATEGORY
        .iter()
        .find(|(c, _)| *c == category)
        .map(|(_, k)| *k)
}

/// A material's kind, drawn as `used`. See the [module](self) for the order of the rules.
#[must_use]
pub fn of(material: &Material, used: Use) -> Kind {
    match used {
        Use::Terrain => return Kind::Ground,
        Use::Sky => return Kind::Sky,
        Use::Cell | Use::Static | Use::Object => {}
    }
    if material.additive {
        return Kind::Effect;
    }
    match (material.data_category.and_then(of_category), used) {
        (Some(k), _) => k,
        (None, Use::Object) => Kind::Item,
        (None, _) => Kind::Architecture,
    }
}

/// Whether a terrain word's type is water.
#[must_use]
pub fn is_water(word: u16) -> bool {
    is_water_type(u32::from((word >> 2) & 0x1F))
}

/// Whether a terrain type is water.
#[must_use]
pub fn is_water_type(t: u32) -> bool {
    (16..=20).contains(&t)
}

/// Whether terrain type `t` is mud or marsh.
#[must_use]
pub fn is_mud_type(t: u32) -> bool {
    matches!(t, 4 | 5)
}

/// A ground cell's kind from its four corners' terrain words: water when every corner is.
#[must_use]
pub fn of_ground(corners: [u16; 4]) -> Kind {
    if corners.iter().all(|w| is_water(*w)) {
        Kind::Water
    } else {
        Kind::Ground
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental)
    use super::*;
    use crate::assets::BlendFactor;

    fn material(category: Option<u32>, additive: bool) -> Material {
        Material {
            kind: Kind::Item,
            surface_type: 2,
            data_category: category,
            texture: None,
            clip_map: false,
            alpha_test: None,
            vertex_alpha: 255,
            vertex_colored: false,
            translucent: None,
            additive,
            blended: additive,
            src_blend: BlendFactor::One,
            dst_blend: if additive {
                BlendFactor::One
            } else {
                BlendFactor::Zero
            },
            z_write: !additive,
            fogged: !additive,
            solid: None,
            luminosity: 0.0,
            diffuse: 1.0,
            maps: None,
        }
    }

    /// **Every data category 0 to 10 has a kind of its own, once**, so no picture's kind is a
    /// guess.
    #[test]
    fn every_data_category_has_one_kind() {
        for c in 0..=10 {
            assert_eq!(
                BY_CATEGORY.iter().filter(|(k, _)| *k == c).count(),
                1,
                "category {c}"
            );
        }
        assert_eq!(of_category(11), None);
    }

    /// **The exceptions come first**: the ground and the sky by where they are drawn, an additive
    /// surface as an effect anywhere, and an untextured one by where it is drawn.
    #[test]
    fn where_it_is_drawn_and_additive_light_come_before_the_category() {
        let foliage = material(Some(1), false);
        assert_eq!(of(&foliage, Use::Static), Kind::Foliage);
        assert_eq!(of(&foliage, Use::Terrain), Kind::Ground);
        assert_eq!(of(&foliage, Use::Sky), Kind::Sky);
        assert_eq!(of(&material(Some(8), true), Use::Cell), Kind::Effect);
        assert_eq!(of(&material(Some(3), true), Use::Sky), Kind::Sky);
        assert_eq!(of(&material(None, false), Use::Object), Kind::Item);
        assert_eq!(of(&material(None, false), Use::Cell), Kind::Architecture);
        assert_eq!(of(&material(Some(4), false), Use::Object), Kind::Character);
    }

    /// **A ground cell is water only when all four corners are.**
    #[test]
    fn a_ground_cell_is_water_only_when_every_corner_is() {
        let water = 17 << 2;
        let grass = 3 << 2;
        assert_eq!(of_ground([water; 4]), Kind::Water);
        assert_eq!(of_ground([water, water, water, grass]), Kind::Ground);
        assert!(is_water(16 << 2) && is_water(20 << 2) && !is_water(21 << 2));
    }
}
