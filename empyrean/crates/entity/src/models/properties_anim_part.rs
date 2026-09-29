// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Models/PropertiesAnimPart.cs
//! `PropertiesAnimPart`: an animation-part change.

/// ACE: PropertiesAnimPart. `Clone` copies every field; ACE's own `Clone()` is [`PropertiesAnimPart::ace_clone`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PropertiesAnimPart {
    // ACE: PropertiesAnimPart.Index
    pub index: u8,
    // ACE: PropertiesAnimPart.AnimationId
    pub animation_id: u32,
}

impl PropertiesAnimPart {
    /// ACE's `Clone()`: a copy of every field.
    // ACE: PropertiesAnimPart.Clone
    #[must_use]
    pub fn ace_clone(&self) -> PropertiesAnimPart {
        PropertiesAnimPart {
            index: self.index,
            animation_id: self.animation_id,
        }
    }
}
