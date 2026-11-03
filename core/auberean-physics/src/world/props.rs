//! An object's own properties, as its description gives them: each property goes by its group to
//! what it sets. Physics, gameplay and pusher properties are kept as the object's own (what a body
//! touching it reports and is pushed by), a physics property naming the physics material keeping
//! its id too; an ethereal property that is a truth value sets or clears its bit of the object's
//! physics type; a drawing property is kept too, and sets how the object is drawn: whether it casts
//! a shadow on the landscape or a volume of shadow, whether it is drawn with those seen through,
//! and whether it is drawn in the game at all.

use auberean_assets::entity::{Entity, EntityLookup, CHAIN_LIMIT};
use auberean_assets::property::{
    PhysicsMaterial, Property, PropertyCollection, PropertyGroup, PropertyValue,
};
use auberean_dat::qualified::divine_type;

/// The record type of an entity.
const ENTITY_TYPE: u32 = 0x4000_0049;

use super::World;
use crate::collision::manager::BodyKey;

/// The property groups an object's description sorts its properties into.
pub mod group {
    pub const PHYSICS: u32 = 0x4000_0001;
    pub const RENDER: u32 = 0x4000_0002;
    pub const ETHEREAL: u32 = 0x4000_0003;
    pub const GAMEPLAY: u32 = 0x4000_0007;
    pub const PUSHER: u32 = 0x4000_000D;
}

/// The physics property naming an object's physics material.
pub const MATERIAL_PROPERTY: u32 = 0x4000_0001;

/// The drawing properties that say how an object is drawn.
pub mod render {
    /// It casts a shadow on the landscape (into the sun's shadow map).
    pub const LANDSCAPE_SHADOW_CAST: u32 = 0x4000_0014;
    /// It casts a volume of shadow (the dynamic objects' shadows).
    pub const VOLUME_SHADOW_CAST: u32 = 0x4000_0015;
    /// It is not drawn in the game.
    pub const INVISIBLE_INGAME: u32 = 0x4000_004D;
    /// It is drawn with the objects seen through.
    pub const TRANSLUCENT: u32 = 0x4000_005C;
}

/// The bits of an object's graphics state.
pub mod graphics {
    /// It casts a volume of shadow.
    pub const VOLUME_SHADOW: u32 = 0x1;
    /// It casts a shadow on the landscape.
    pub const LANDSCAPE_SHADOW: u32 = 0x2;
    /// It is drawn with the objects seen through.
    pub const TRANSLUCENT: u32 = 0x4;
    /// Outdoors and not seen through, it takes the land's shadow where it stands. No drawing
    /// property says otherwise; only the sky's own objects, which are no object of the world, are
    /// without it.
    pub const TAKES_LAND_SHADOW: u32 = 0x8;
    /// The state every object starts with.
    pub const NEW: u32 = TAKES_LAND_SHADOW;
}

/// The bit of the graphics state a drawing property that is a truth value sets or clears.
fn graphics_bit(id: u32) -> Option<u32> {
    match id {
        render::LANDSCAPE_SHADOW_CAST => Some(graphics::LANDSCAPE_SHADOW),
        render::VOLUME_SHADOW_CAST => Some(graphics::VOLUME_SHADOW),
        render::TRANSLUCENT => Some(graphics::TRANSLUCENT),
        _ => None,
    }
}

/// The graphics state an object made from entity `e` takes from its drawing properties (an
/// entity a landblock or a cell places, kept whole there): the entities it is made from first
/// (along its object while that is an entity, found by `entity`), then its own, each truth value
/// among those saying how it is drawn setting or clearing its bit, as
/// [`World::apply_entity_properties`] applies them to an object in the world.
#[must_use]
pub fn entity_graphics_state(e: &Entity, entity: EntityLookup<'_>) -> u32 {
    let mut parents = Vec::new();
    let mut at = e.object.unwrap_or(0);
    for _ in 0..CHAIN_LIMIT {
        if divine_type(at) != ENTITY_TYPE {
            break;
        }
        let Some(p) = entity(at) else {
            break;
        };
        at = p.object.unwrap_or(0);
        parents.push(p);
    }
    let own = std::iter::once(e.properties.as_ref());
    parents
        .iter()
        .rev()
        .map(|p| p.properties.as_ref())
        .chain(own)
        .flatten()
        .flat_map(PropertyCollection::iter)
        .fold(graphics::NEW, |state, p| {
            match (graphics_bit(p.id), &p.value) {
                (Some(bit), PropertyValue::Bool(true)) => state | bit,
                (Some(bit), PropertyValue::Bool(false)) => state & !bit,
                _ => state,
            }
        })
}

impl World {
    /// Keeps `p` among entry `key`'s own properties (made when it has none), unless it holds its
    /// default.
    fn keep_property(&mut self, key: BodyKey, p: &Property) -> bool {
        let list = self.properties.clone();
        let Some(e) = self.entries.get_mut(&key) else {
            return false;
        };
        if p.is_default() {
            return true;
        }
        e.properties
            .get_or_insert_with(Box::default)
            .properties
            .update_property(&list, p)
    }

    /// A physics property: kept as the entry's own; the one naming its physics material keeps the
    /// material's id.
    // D2-06039
    pub fn apply_physics_property(&mut self, key: BodyKey, p: &Property) -> bool {
        let kept = self.keep_property(key, p);
        if p.id != MATERIAL_PROPERTY {
            return kept;
        }
        match (&p.value, self.entries.get_mut(&key)) {
            (PropertyValue::DataFile(id), Some(e)) => {
                e.material = id.0;
                true
            }
            _ => false,
        }
    }

    // D2-06040
    pub fn apply_pusher_property(&mut self, key: BodyKey, p: &Property) -> bool {
        self.keep_property(key, p)
    }

    // D2-06041
    pub fn apply_gameplay_property(&mut self, key: BodyKey, p: &Property) -> bool {
        self.keep_property(key, p)
    }

    /// A drawing property: kept as the entry's own; a truth value among those saying how the
    /// object is drawn sets or clears its bit of the graphics state, or (not drawn in the game)
    /// whether it is drawn in the game. True when it set something.
    // D2-06038, D2-06015
    pub fn apply_render_property(&mut self, key: BodyKey, p: &Property) -> bool {
        self.keep_property(key, p);
        let PropertyValue::Bool(on) = p.value else {
            return false;
        };
        if p.id == render::INVISIBLE_INGAME {
            return self.entries.get_mut(&key).is_some_and(|e| {
                e.game_draw = Some(!on);
                true
            });
        }
        let Some(bit) = graphics_bit(p.id) else {
            return false;
        };
        self.set_graphics_state(key, bit, on)
    }

    /// Sets (or clears) bits `bits` of entry `key`'s graphics state; false for no such entry.
    // D2-06033
    pub fn set_graphics_state(&mut self, key: BodyKey, bits: u32, on: bool) -> bool {
        self.entries.get_mut(&key).is_some_and(|e| {
            if on {
                e.graphics_state |= bits;
            } else {
                e.graphics_state &= !bits;
            }
            true
        })
    }

    /// Whether every bit of `bits` is set in entry `key`'s graphics state.
    // D2-06034
    #[must_use]
    pub fn graphics_state(&self, key: BodyKey, bits: u32) -> bool {
        self.entries
            .get(&key)
            .is_some_and(|e| e.graphics_state & bits == bits)
    }

    /// Whether entry `key` is drawn in the game, once one of its drawing properties has said.
    #[must_use]
    pub fn game_draw(&self, key: BodyKey) -> Option<bool> {
        self.entries.get(&key).and_then(|e| e.game_draw)
    }

    /// An ethereal property that is a truth value sets or clears its bit of the entry's physics
    /// type; false for any other property.
    // D2-06037
    pub fn apply_ethereal_property(&mut self, key: BodyKey, p: &Property) -> bool {
        let PropertyValue::Bool(on) = p.value else {
            return false;
        };
        let list = self.properties.clone();
        self.entries
            .get_mut(&key)
            .is_some_and(|e| e.physics_type.set_property(&list, p.id, on))
    }

    /// One property, by its group; one of no group these take is not applied.
    // D2-06155
    pub fn apply_property(&mut self, key: BodyKey, p: &Property) -> bool {
        match p.group() {
            group::PHYSICS => self.apply_physics_property(key, p),
            group::RENDER => self.apply_render_property(key, p),
            group::ETHEREAL => self.apply_ethereal_property(key, p),
            group::GAMEPLAY => self.apply_gameplay_property(key, p),
            group::PUSHER => self.apply_pusher_property(key, p),
            _ => false,
        }
    }

    /// Every property of a group; true only when each was applied.
    // D2-06154
    pub fn apply_property_group(&mut self, key: BodyKey, g: &PropertyGroup) -> bool {
        g.properties
            .iter()
            .fold(true, |all, p| self.apply_property(key, p) & all)
    }

    /// The properties of entity record `id` as an object made from it takes them: an entity made
    /// from another entity is that one's object first, with its properties, and then takes its own,
    /// each applied whole (so the outermost entity's own properties stand, an entity with none
    /// leaving none). An id that is not an entity, or is not found, applies nothing (false).
    pub fn apply_entity_properties(
        &mut self,
        key: BodyKey,
        id: u32,
        entity: EntityLookup<'_>,
    ) -> bool {
        let mut chain = Vec::new();
        let mut at = id;
        for _ in 0..CHAIN_LIMIT {
            if divine_type(at) != ENTITY_TYPE {
                break;
            }
            let Some(e) = entity(at) else {
                break;
            };
            at = e.object.unwrap_or(0);
            chain.push(e);
        }
        let none = PropertyCollection::default();
        chain.iter().rev().fold(!chain.is_empty(), |all, e| {
            self.apply_property_collection(key, e.properties.as_ref().unwrap_or(&none)) & all
        })
    }

    /// A whole description's properties, after the entry's own are dropped; true only when every
    /// one was applied. Applying them again is the same.
    // D2-06187, D2-06036
    pub fn apply_property_collection(&mut self, key: BodyKey, c: &PropertyCollection) -> bool {
        if let Some(e) = self.entries.get_mut(&key) {
            e.properties = None;
        }
        c.groups
            .iter()
            .fold(true, |all, g| self.apply_property_group(key, g) & all)
    }

    /// Entry `key`'s physics material id (0: none) and its own property groups (none until it has
    /// any).
    // D2-06043
    #[must_use]
    pub fn object_properties(&self, key: BodyKey) -> (u32, Option<&[PropertyGroup]>) {
        let Some(e) = self.entries.get(&key) else {
            return (0, None);
        };
        (
            e.material,
            e.properties
                .as_deref()
                .map(|m: &PhysicsMaterial| m.properties.groups.as_slice()),
        )
    }
}
