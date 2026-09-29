// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/World/WeeniePropertiesSpellBook.cs
//! `WeeniePropertiesSpellBook`: one row of the world-database table `weenie_properties_spell_book`.

/// SpellBook Properties of Weenies
// ACE: WeeniePropertiesSpellBook
#[derive(Debug, Clone, PartialEq, Default)]
pub struct WeeniePropertiesSpellBook {
    /// Unique Id of this Property
    pub id: u32,
    /// Id of the object this property belongs to
    pub object_id: u32,
    /// Id of Spell
    pub spell: i32,
    /// Chance to cast this spell
    pub probability: f32,
    // ACE: WeeniePropertiesSpellBook.Object navigates back to the parent row, not carried.
}
