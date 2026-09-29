// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Creature_Attributes.cs
//! Port of `Source/ACE.Server/WorldObjects/Creature_Attributes.cs`.

use empyrean_common::dotnet::DotNetDict;
use empyrean_entity::enums::PropertyAttribute;

use crate::world_objects::entity::creature_attribute::CreatureAttribute;
use crate::world_objects::world_object::WorldObject;

/// Non-property fields declared in `Creature_Attributes.cs`.
#[derive(Debug, Default)]
pub struct CreatureAttributesFields {
    // ACE: Creature.Attributes
    /// Filled by the constructor (`Creature.SetEphemeralValues`) with the six attributes.
    pub attributes: DotNetDict<PropertyAttribute, CreatureAttribute>,
}

impl WorldObject {
    /// `Creature.Attributes`.
    ///
    /// # Panics
    /// When this object is not a Creature.
    #[must_use]
    pub fn attributes(&self) -> &DotNetDict<PropertyAttribute, CreatureAttribute> {
        &self
            .creature
            .as_ref()
            .expect("Creature.Attributes on an object that is not a Creature")
            .creature_attributes
            .attributes
    }

    /// `Creature.Attributes`, for the constructor.
    ///
    /// # Panics
    /// When this object is not a Creature.
    pub fn attributes_mut(&mut self) -> &mut DotNetDict<PropertyAttribute, CreatureAttribute> {
        &mut self
            .creature
            .as_mut()
            .expect("Creature.Attributes on an object that is not a Creature")
            .creature_attributes
            .attributes
    }

    /// `Attributes[attribute]`: `KeyNotFoundException` (a panic) when absent.
    fn attribute_of(&self, attribute: PropertyAttribute) -> CreatureAttribute {
        self.attributes()
            .get(&attribute)
            .copied()
            .unwrap_or_else(|| {
                panic!(
                    "KeyNotFoundException: Creature.Attributes[{}]",
                    attribute.to_dotnet_string()
                )
            })
    }

    // ACE: Creature.Strength
    #[must_use]
    pub fn strength(&self) -> CreatureAttribute {
        self.attribute_of(PropertyAttribute::Strength)
    }

    // ACE: Creature.Endurance
    #[must_use]
    pub fn endurance(&self) -> CreatureAttribute {
        self.attribute_of(PropertyAttribute::Endurance)
    }

    // ACE: Creature.Coordination
    #[must_use]
    pub fn coordination(&self) -> CreatureAttribute {
        self.attribute_of(PropertyAttribute::Coordination)
    }

    // ACE: Creature.Quickness
    #[must_use]
    pub fn quickness(&self) -> CreatureAttribute {
        self.attribute_of(PropertyAttribute::Quickness)
    }

    // ACE: Creature.Focus
    #[must_use]
    pub fn focus(&self) -> CreatureAttribute {
        self.attribute_of(PropertyAttribute::Focus)
    }

    // ACE: Creature.Self
    /// `Creature.Self` (`self` is a Rust keyword).
    #[must_use]
    pub fn self_(&self) -> CreatureAttribute {
        self.attribute_of(PropertyAttribute::Self_)
    }
}
