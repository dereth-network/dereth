//! The recorded characters, as the shard held them before the recording.
//!
//! A character the recording creates (`Character_SendCharGenResult`) is created again over the
//! wire by the replay itself, from that same message with a synthetic name. A character the
//! recording only logs in to is **reconstructed** here from what its first login sent the
//! client: the PlayerDescription (every property table, attributes, vitals, skills, spell book,
//! options and spell bars), the character's own CreateObject (its weenie and location) and the
//! CreateObjects of its possessions (weenie, container or wielder, wield location, stack size,
//! value, burden, structure). Each biota starts from its weenie in `world.pack`, as ACE's own
//! `WeenieConverter.ConvertToBiota` does, and the recorded values are laid over it.
//!
//! What the client is never sent cannot be reconstructed: server-only properties, the
//! instantiation position, quest registry, allegiance,
//! friends, squelches and titles. The replay report names this as the reason where it matters.
//! The lifestone (Sanctuary) is never sent either, but a recorded lifestone recall shows it; the
//! replay seeds it from there (`replay::recorded_sanctuary`). Likewise a portal that let the
//! character through shows its quest restriction was met; the replay seeds that quest as solved
//! (`replay::recorded_portal_uses`).
//! Names are synthetic; the recorded ones are never read into anything kept.

use dereth_protocol::login::LoginPlayerDescription;
use dereth_protocol::objects::ObjectCreatePayload;
use empyrean_content::WorldDatabase;
use empyrean_entity::adapter::quality_bridge::{ac_qualities_into_biota, position_from_wire};
use empyrean_entity::adapter::weenie_converter::convert_to_biota;
use empyrean_entity::enums::{
    PositionType, PropertyBool, PropertyInstanceId, PropertyInt, PropertyString,
};
use empyrean_entity::Biota;
use empyrean_store::models::shard::{
    Character, CharacterPropertiesShortcutBar, CharacterPropertiesSpellBar,
};

/// A reconstructed character: its biota, its possessions' biotas and its character row.
#[derive(Debug, Clone)]
pub struct Reconstructed {
    pub biota: Biota,
    pub possessions: Vec<Biota>,
    pub character: Character,
    /// Possessions whose weenie is not in the pack (left out).
    pub missing_weenies: usize,
}

/// Marks a registry row whose `EnchantmentCategory` is its spell's `MetaSpellType` (filled in by the
/// replay, which has the spell table). It lives with the qualities bridge now.
pub use empyrean_entity::adapter::quality_bridge::ENCHANTMENT_CATEGORY_FROM_SPELL;

/// The biota of `create`'s weenie under `guid`, or `None` when the pack lacks the weenie.
fn from_weenie(
    content: &dyn WorldDatabase,
    guid: u32,
    create: &ObjectCreatePayload,
) -> Option<Biota> {
    let weenie = content.get_cached_weenie(create.wdesc.wcid)?;
    Some(convert_to_biota(&weenie, guid, false, false))
}

/// Reconstructs the character `guid` of account `account_id` from its first recorded login.
///
/// `own` is the character's CreateObject; `possessions` are its possessions' CreateObjects in send
/// order (each one's container or wielder is the character or an earlier possession).
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn reconstruct(
    content: &dyn WorldDatabase,
    guid: u32,
    account_id: u32,
    name: &str,
    pd: &LoginPlayerDescription,
    own: &ObjectCreatePayload,
    possessions: &[ObjectCreatePayload],
) -> Reconstructed {
    let mut b = from_weenie(content, guid, own).unwrap_or_else(|| Biota {
        id: guid,
        weenie_class_id: own.wdesc.wcid,
        ..Biota::default()
    });
    // every property table, then the attributes, vitals, skills, spell book and the enchantment
    // registry (the PlayerDescription carries it whole, so each row is read back)
    ac_qualities_into_biota(&pd.qualities, &mut b);
    // Attackable is not a login property, but the character's own CreateObject carries it
    // (`CalculateObjectDescriptionFlag`: the Attackable flag is `Attackable`). The weenie's value
    // is wrong for an admin weenie whose character the recording's server made attackable, and
    // the reverse; monsters wake (`Player.CheckMonsters`) only for an attackable character.
    if own.wdesc.bitfield & 0x10 != 0 {
        b.try_remove_property(PropertyBool::Attackable); // ACE's setter: true removes the property
    } else {
        b.set_property(PropertyBool::Attackable, false);
    }
    b.set_property(PropertyString::Name, name.to_owned());
    if let Some(p) = &own.physicsdesc.position {
        b.set_property_position(PositionType::Location, position_from_wire(p));
    }

    // possessions, placed in their containers in send order
    let mut items = Vec::new();
    let mut missing_weenies = 0;
    let mut placement: std::collections::HashMap<u32, i32> = std::collections::HashMap::new();
    for c in possessions {
        let Some(mut item) = from_weenie(content, c.id.0, c) else {
            missing_weenies += 1;
            continue;
        };
        let w = &c.wdesc;
        if let Some(container) = w.container_id {
            item.set_property(PropertyInstanceId::Container, container.0);
            let slot = placement.entry(container.0).or_insert(0);
            item.set_property(PropertyInt::PlacementPosition, *slot);
            *slot += 1;
        }
        if let Some(wielder) = w.wielder_id {
            item.set_property(PropertyInstanceId::Wielder, wielder.0);
        }
        if let Some(l) = w.location {
            item.set_property(PropertyInt::CurrentWieldedLocation, l.cast_signed());
        }
        if let Some(s) = w.stack_size {
            item.set_property(PropertyInt::StackSize, i32::from(s));
        }
        if let Some(v) = w.value {
            item.set_property(PropertyInt::Value, v.cast_signed());
        }
        if let Some(v) = w.burden {
            item.set_property(PropertyInt::EncumbranceVal, i32::from(v));
        }
        if let Some(v) = w.structure {
            item.set_property(PropertyInt::Structure, i32::from(v));
        }
        items.push(item);
    }

    let pm = &pd.player_module;
    let mut character = Character {
        id: guid,
        account_id,
        name: name.to_owned(),
        // not a first login: the recording's character had logged in before
        total_logins: 1,
        character_options_1: pm.options.cast_signed(),
        character_options_2: pm.options2.cast_signed(),
        spellbook_filters: pm.spell_filters,
        ..Character::default()
    };
    for (bar, spells) in pm.spell_bars.iter().enumerate() {
        for (index, spell) in spells.iter().enumerate() {
            character
                .character_properties_spell_bar
                .push(CharacterPropertiesSpellBar {
                    character_id: guid,
                    spell_bar_number: u32::try_from(bar).unwrap_or(0),
                    spell_bar_index: u32::try_from(index).unwrap_or(0),
                    spell_id: *spell,
                });
        }
    }
    for s in pm.shortcuts.iter().flatten() {
        character
            .character_properties_shortcut_bar
            .push(CharacterPropertiesShortcutBar {
                character_id: guid,
                shortcut_bar_index: s.index.cast_unsigned(),
                shortcut_object_id: s.object_id.0,
            });
    }
    Reconstructed {
        biota: b,
        possessions: items,
        character,
        missing_weenies,
    }
}
