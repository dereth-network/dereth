// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Structure/LayeredSpell.cs
//! Port of `Source/ACE.Server/Network/Structure/LayeredSpell.cs`.

use empyrean_common::dotnet::binary_reader::{BinaryReader, ReadError};
use empyrean_entity::models::PropertiesEnchantmentRegistry;

use crate::network::game_messages::game_message::write_record;

// ACE: LayeredSpell
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LayeredSpell {
    // ACE: LayeredSpell.SpellId
    pub spell_id: u16,
    // ACE: LayeredSpell.Layer
    pub layer: u16,
}

impl LayeredSpell {
    // ACE: LayeredSpell.LayeredSpell
    /// `new LayeredSpell(ushort spellId, ushort layer)`.
    #[must_use]
    pub const fn new(spell_id: u16, layer: u16) -> Self {
        Self { spell_id, layer }
    }

    // ACE: LayeredSpell.LayeredSpell
    /// `new LayeredSpell(PropertiesEnchantmentRegistry enchantment)`.
    #[must_use]
    #[allow(clippy::cast_possible_truncation)] // `(ushort)enchantment.SpellId`
    pub fn from_registry(enchantment: &PropertiesEnchantmentRegistry) -> Self {
        Self {
            spell_id: enchantment.spell_id as u16,
            layer: enchantment.layer_id,
        }
    }
}

// ACE: LayeredSpellExtensions.ReadLayeredSpell
pub fn read_layered_spell(reader: &mut BinaryReader<'_>) -> Result<LayeredSpell, ReadError> {
    let spell_id = reader.read_u16()?;
    let layer = reader.read_u16()?;
    Ok(LayeredSpell { spell_id, layer })
}

// ACE: LayeredSpellExtensions.Write
/// `writer.Write(LayeredSpell spell)`.
pub fn write(writer: &mut Vec<u8>, spell: &LayeredSpell) {
    write_record(writer, &[], |w| w.u32(id(spell)));
}

// ACE: LayeredSpellExtensions.Write
/// `writer.Write(List<LayeredSpell> spells)`.
pub fn write_list(writer: &mut Vec<u8>, spells: &[LayeredSpell]) {
    let ids: Vec<u32> = spells.iter().map(id).collect();
    write_record(writer, &[], |w| {
        w.packed_list(&ids, |w, v| {
            w.u32(*v);
            Ok(())
        })
    });
}

// ACE: LayeredSpellExtensions.Write
/// `writer.Write(PropertiesEnchantmentRegistry enchantment)`.
#[allow(clippy::cast_possible_truncation)] // `(ushort)enchantment.SpellId`
pub fn write_registry(writer: &mut Vec<u8>, enchantment: &PropertiesEnchantmentRegistry) {
    write_record(writer, &[], |w| w.u32(registry_id(enchantment)));
}

// ACE: LayeredSpellExtensions.Write
/// `writer.Write(List<PropertiesEnchantmentRegistry> enchantments)`.
pub fn write_registry_list(writer: &mut Vec<u8>, enchantments: &[PropertiesEnchantmentRegistry]) {
    let ids: Vec<u32> = enchantments.iter().map(registry_id).collect();
    write_record(writer, &[], |w| {
        w.packed_list(&ids, |w, v| {
            w.u32(*v);
            Ok(())
        })
    });
}

/// The layered spell id dereth-protocol's records carry: `SpellId`, then `Layer`, two ushorts, as one
/// dword `spellId | layer << 16`.
#[must_use]
pub fn id(spell: &LayeredSpell) -> u32 {
    u32::from(spell.spell_id) | (u32::from(spell.layer) << 16)
}

/// [`id`] of `new LayeredSpell(enchantment)`: `(ushort)enchantment.SpellId` and its layer.
#[must_use]
#[allow(clippy::cast_possible_truncation)] // `(ushort)enchantment.SpellId`
pub fn registry_id(enchantment: &PropertiesEnchantmentRegistry) -> u32 {
    u32::from(enchantment.spell_id as u16) | (u32::from(enchantment.layer_id) << 16)
}
