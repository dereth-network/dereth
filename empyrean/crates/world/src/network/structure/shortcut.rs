// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Structure/Shortcut.cs
//! Port of `Source/ACE.Server/Network/Structure/Shortcut.cs`.

use empyrean_common::dotnet::binary_reader::{BinaryReader, ReadError};
use empyrean_store::models::shard::CharacterPropertiesShortcutBar;

use super::layered_spell::{self, read_layered_spell, LayeredSpell};
use crate::network::game_messages::game_message::write_record;

// ACE: Shortcut
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Shortcut {
    // ACE: Shortcut.Index
    /// position on shortcut bar
    pub index: u32,
    // ACE: Shortcut.ObjectId
    pub object_id: u32,
    // ACE: Shortcut.Spell
    /// unused? `null` for the parameterless constructor, which only the reader uses (and it
    /// always fills the spell), so it is not optional here.
    pub spell: LayeredSpell,
}

impl Shortcut {
    // ACE: Shortcut.Shortcut
    /// `new Shortcut(uint objectId, uint index)`.
    #[must_use]
    pub fn new(object_id: u32, index: u32) -> Self {
        Self {
            index,
            object_id,
            spell: LayeredSpell::default(),
        }
    }

    // ACE: Shortcut.Shortcut
    /// `new Shortcut(CharacterPropertiesShortcutBar shortcut)`: the bar index is stored 1-based.
    #[must_use]
    pub fn from_shortcut_bar(shortcut: &CharacterPropertiesShortcutBar) -> Self {
        Self {
            index: shortcut.shortcut_bar_index.wrapping_sub(1),
            object_id: shortcut.shortcut_object_id,
            spell: LayeredSpell::default(),
        }
    }
}

// ACE: ShortcutExtensions.ReadShortcut
pub fn read_shortcut(reader: &mut BinaryReader<'_>) -> Result<Shortcut, ReadError> {
    let index = reader.read_u32()?;
    let object_id = reader.read_u32()?;
    let spell = read_layered_spell(reader)?;
    Ok(Shortcut {
        index,
        object_id,
        spell,
    })
}

// ACE: ShortcutExtensions.Write
/// `writer.Write(Shortcut shortcut)`.
pub fn write(writer: &mut Vec<u8>, shortcut: &Shortcut) {
    write_record(writer, &[], |w| record(shortcut).write(w));
}

// ACE: ShortcutExtensions.Write
/// `writer.Write(ICollection<Shortcut> shortcuts)`.
pub fn write_list(writer: &mut Vec<u8>, shortcuts: &[Shortcut]) {
    let list: Vec<_> = shortcuts.iter().map(record).collect();
    write_record(writer, &[], |w| {
        w.packed_list(&list, |w, s| {
            s.write(w);
            Ok(())
        })
    });
}

/// The dereth-protocol record the `Write` extension below writes, field for field.
#[must_use]
pub fn record(shortcut: &Shortcut) -> dereth_protocol::login::ShortCutData {
    dereth_protocol::login::ShortCutData {
        index: shortcut.index.cast_signed(),
        object_id: dereth_primitives::ObjectId(shortcut.object_id),
        spell_id: layered_spell::id(&shortcut.spell),
    }
}
