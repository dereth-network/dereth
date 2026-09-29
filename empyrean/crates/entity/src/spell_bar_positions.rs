// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/SpellBarPositions.cs
//! `SpellBarPositions`: one spell-bar slot, stored zero-based.

/// ACE: SpellBarPositions
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpellBarPositions {
    // ACE: SpellBarPositions.SpellBarId
    pub spell_bar_id: u32,
    // ACE: SpellBarPositions.SpellBarPositionId
    pub spell_bar_position_id: u32,
    // ACE: SpellBarPositions.SpellId
    pub spell_id: u32,
}

impl SpellBarPositions {
    /// Takes one-based bar and position ids and stores them zero-based (a 0 wraps: `uint`
    /// arithmetic is unchecked).
    // ACE: SpellBarPositions.SpellBarPositions
    #[must_use]
    pub fn new(spell_bar_id: u32, spell_bar_position_id: u32, spell_id: u32) -> Self {
        SpellBarPositions {
            spell_bar_id: spell_bar_id.wrapping_sub(1),
            spell_bar_position_id: spell_bar_position_id.wrapping_sub(1),
            spell_id,
        }
    }
}
