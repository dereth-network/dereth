// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Sequence/SequenceManager.cs
//! Port of `Source/ACE.Server/Network/Sequence/SequenceManager.cs`.
//!
//! Each world object owns one [`SequenceManager`] (ACE `WorldObject.Sequences`). The message
//! builders read it through [`HasSequences`], which `WorldObject` implements; a test may implement
//! it on a value that pairs a `WorldObject` with a manager.

use std::collections::HashMap;

use empyrean_entity::enums::{
    PositionType, PropertyAttribute, PropertyAttribute2nd, PropertyBool, PropertyDataId,
    PropertyFloat, PropertyInstanceId, PropertyInt, PropertyInt64, PropertyString, Skill, Vital,
};

use super::byte_sequence::ByteSequence;
use super::i_sequence::ISequence;
use super::sequence_type::SequenceType;
use super::u_short_sequence::UShortSequence;
use crate::world_objects::world_object::WorldObject;

/// A value ACE passes as the `property` of a `SequenceManager` call: one of the typed overloads,
/// each of which casts to `uint` (a `ushort` enum widens; `Skill`, an `int` enum, reinterprets).
pub trait SequenceProperty: Copy {
    /// `(uint)property`.
    fn sequence_key(self) -> u32;
}

impl SequenceProperty for u32 {
    fn sequence_key(self) -> u32 {
        self
    }
}

macro_rules! ushort_sequence_property {
    ($($t:ty),*) => {$(
        impl SequenceProperty for $t {
            fn sequence_key(self) -> u32 {
                u32::from(self.0)
            }
        }
    )*};
}

ushort_sequence_property!(
    PositionType,
    PropertyAttribute,
    PropertyAttribute2nd,
    PropertyBool,
    PropertyDataId,
    PropertyFloat,
    PropertyInstanceId,
    PropertyInt,
    PropertyInt64,
    PropertyString
);

impl SequenceProperty for Skill {
    fn sequence_key(self) -> u32 {
        // `(uint)skill`: an int enum, reinterpreted.
        self.0.cast_unsigned()
    }
}

impl SequenceProperty for Vital {
    fn sequence_key(self) -> u32 {
        self.0
    }
}

// ACE: SequenceManager
/// ACE `SequenceManager`: one lazily created sequence per `(SequenceType, property)`, keyed
/// `(uint)type << 16 | property`. The dictionary is only ever looked up, never enumerated, so a
/// `HashMap` is faithful.
#[derive(Debug, Default)]
pub struct SequenceManager {
    sequence_list: HashMap<u32, Box<dyn ISequence>>,
}

impl SequenceManager {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    // ACE: SequenceManager.GetCurrentSequence
    /// `GetCurrentSequence(SequenceType type)`: property 0.
    pub fn get_current_sequence(&mut self, r#type: SequenceType) -> Vec<u8> {
        self.get_sequence(r#type, 0).current_bytes()
    }

    // ACE: SequenceManager.GetNextSequence
    /// `GetNextSequence(SequenceType type)`: property 0.
    pub fn get_next_sequence(&mut self, r#type: SequenceType) -> Vec<u8> {
        self.get_sequence(r#type, 0).next_bytes()
    }

    /// The typed `GetCurrentSequence(SequenceType, <property>)` overloads.
    pub fn get_current_sequence_of(
        &mut self,
        r#type: SequenceType,
        property: impl SequenceProperty,
    ) -> Vec<u8> {
        self.get_sequence(r#type, property.sequence_key())
            .current_bytes()
    }

    /// The typed `GetNextSequence(SequenceType, <property>)` overloads.
    pub fn get_next_sequence_of(
        &mut self,
        r#type: SequenceType,
        property: impl SequenceProperty,
    ) -> Vec<u8> {
        self.get_sequence(r#type, property.sequence_key())
            .next_bytes()
    }

    // ACE: SequenceManager.GetSequence
    /// The sequence for `(type, property)`, created on first use: a `UShortSequence` for the object
    /// sequences, a `UShortSequence(1, 0x7FFF)` for `Motion` (the MSB is reserved), and an unprimed
    /// `ByteSequence` for everything else.
    pub fn get_sequence(&mut self, r#type: SequenceType, property: u32) -> &mut dyn ISequence {
        let key = (r#type as u32) << 16 | property;

        self.sequence_list
            .entry(key)
            .or_insert_with(|| match r#type {
                SequenceType::ObjectPosition
                | SequenceType::ObjectMovement
                | SequenceType::ObjectState
                | SequenceType::ObjectVector
                | SequenceType::ObjectTeleport
                | SequenceType::ObjectServerControl
                | SequenceType::ObjectForcePosition
                | SequenceType::ObjectVisualDesc
                | SequenceType::ObjectInstance => {
                    Box::new(UShortSequence::new_primed(true, u16::MAX))
                }

                // MSB is reserved, so set max value to exclude it.
                SequenceType::Motion => Box::new(UShortSequence::new(1, 0x7FFF)),

                _ => Box::new(ByteSequence::new_primed(false, u8::MAX)),
            })
            .as_mut()
    }

    // ACE: SequenceManager.SetSequence
    /// Replaces the property-0 sequence of `type`.
    pub fn set_sequence(&mut self, r#type: SequenceType, sequence: Box<dyn ISequence>) {
        let key = (r#type as u32) << 16;

        self.sequence_list.insert(key, sequence);
    }
}

/// What an ACE message constructor reads from its `WorldObject` argument: the object itself (its
/// `Guid` and properties) and its `Sequences`.
///
/// This is not an ACE type. `WorldObject` implements it as
/// ```ignore
/// impl HasSequences for WorldObject {
///     fn world_object(&self) -> &WorldObject { self }
///     fn sequences(&mut self) -> &mut SequenceManager { &mut self.sequences }
/// }
/// ```
/// so every builder can be handed a `&mut WorldObject` directly.
pub trait HasSequences {
    /// The object (`worldObject.Guid`, its properties).
    fn world_object(&self) -> &WorldObject;
    /// `worldObject.Sequences`.
    fn sequences(&mut self) -> &mut SequenceManager;
}
