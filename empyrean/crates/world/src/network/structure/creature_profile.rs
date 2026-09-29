// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Structure/CreatureProfile.cs
//! Port of `Source/ACE.Server/Network/Structure/CreatureProfile.cs`.

use empyrean_entity::enums::{PropertyAttribute, PropertyAttribute2nd};
use empyrean_entity::ObjectGuid;

use crate::network::game_messages::game_message::write_record;
use crate::world_objects::world_object_networking::shims;
use crate::World;

// ACE: CreatureProfileFlags
/// `[Flags] enum CreatureProfileFlags` (`int`).
#[allow(non_snake_case, non_upper_case_globals)]
pub mod CreatureProfileFlags {
    pub const HasBuffsDebuffs: i32 = 0x1;
    pub const Unknown1: i32 = 0x2; // TODO: decode flags
    pub const Unknown2: i32 = 0x4;
    pub const ShowAttributes: i32 = 0x8;
}

// ACE: CreatureProfile
/// Handles the assessment of creatures (monsters / players).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CreatureProfile {
    // ACE: CreatureProfile.Flags
    /// These flags indicate which members will be available for assessment.
    pub flags: i32,

    // ACE: CreatureProfile.Health
    pub health: u32,
    // ACE: CreatureProfile.HealthMax
    pub health_max: u32,

    // ACE: CreatureProfile.Strength
    pub strength: u32,
    // ACE: CreatureProfile.Endurance
    pub endurance: u32,
    // ACE: CreatureProfile.Quickness
    pub quickness: u32,
    // ACE: CreatureProfile.Coordination
    pub coordination: u32,
    // ACE: CreatureProfile.Focus
    pub focus: u32,
    // ACE: CreatureProfile.Self
    pub self_: u32,

    // ACE: CreatureProfile.Stamina
    pub stamina: u32,
    // ACE: CreatureProfile.Mana
    pub mana: u32,
    // ACE: CreatureProfile.StaminaMax
    pub stamina_max: u32,
    // ACE: CreatureProfile.ManaMax
    pub mana_max: u32,

    // ACE: CreatureProfile.AttributeHighlights
    /// `AttributeMask`: highlight enable bitmask, 0=no, 1=yes.
    pub attribute_highlights: u32,
    // ACE: CreatureProfile.AttributeColors
    /// `AttributeMask`: highlight color bitmask, 0=red, 1=green.
    pub attribute_colors: u32,
}

// ACE: CreatureProfile.CreatureProfile
/// `new CreatureProfile(Creature creature, bool success = true)`.
pub fn creature_profile_new(w: &mut World, creature: ObjectGuid, success: bool) -> CreatureProfile {
    let mut p = CreatureProfile::default();

    if success {
        p.flags |= CreatureProfileFlags::ShowAttributes;
    }

    p.health = shims::creature_vital_current(w, creature, PropertyAttribute2nd::MaxHealth);
    p.health_max = shims::creature_vital_max_value(w, creature, PropertyAttribute2nd::MaxHealth);

    if !success {
        return p;
    }

    p.strength = shims::creature_attribute_current(w, creature, PropertyAttribute::Strength);
    p.endurance = shims::creature_attribute_current(w, creature, PropertyAttribute::Endurance);
    p.quickness = shims::creature_attribute_current(w, creature, PropertyAttribute::Quickness);
    p.coordination =
        shims::creature_attribute_current(w, creature, PropertyAttribute::Coordination);
    p.focus = shims::creature_attribute_current(w, creature, PropertyAttribute::Focus);
    p.self_ = shims::creature_attribute_current(w, creature, PropertyAttribute::Self_);

    p.stamina = shims::creature_vital_current(w, creature, PropertyAttribute2nd::MaxStamina);
    p.mana = shims::creature_vital_current(w, creature, PropertyAttribute2nd::MaxMana);
    p.stamina_max = shims::creature_vital_max_value(w, creature, PropertyAttribute2nd::MaxStamina);
    p.mana_max = shims::creature_vital_max_value(w, creature, PropertyAttribute2nd::MaxMana);

    p.attribute_highlights = shims::attribute_mask_helper_get_attribute_highlights(w, creature);
    p.attribute_colors = shims::attribute_mask_helper_get_attribute_colors(w, creature);

    if p.attribute_highlights != 0 {
        p.flags |= CreatureProfileFlags::HasBuffsDebuffs;
    }

    p
}

// ACE: CreatureProfileExtensions.Write
#[allow(clippy::cast_possible_truncation)] // `(ushort)` of the masks
pub fn write(writer: &mut Vec<u8>, profile: &CreatureProfile) {
    write_record(writer, &[], |w| record(profile).write(w));
}

/// The dereth-protocol record the `Write` extension below writes, field for field.
#[must_use]
#[allow(clippy::cast_possible_truncation)] // `(ushort)` of the masks
pub fn record(profile: &CreatureProfile) -> dereth_protocol::types::CreatureAppraisalProfile {
    dereth_protocol::types::CreatureAppraisalProfile {
        flags: profile.flags.cast_unsigned(),
        health: profile.health,
        max_health: profile.health_max,
        // has flags & 0x8?
        attributes: (profile.flags & CreatureProfileFlags::ShowAttributes != 0).then_some([
            profile.strength,
            profile.endurance,
            profile.quickness,
            profile.coordination,
            profile.focus,
            profile.self_,
            profile.stamina,
            profile.mana,
            profile.stamina_max,
            profile.mana_max,
        ]),
        // has flags & 0x1? The two ushorts, highlights then colours.
        enchantment_bitfield: (profile.flags & CreatureProfileFlags::HasBuffsDebuffs != 0)
            .then_some(
                u32::from(profile.attribute_highlights as u16)
                    | (u32::from(profile.attribute_colors as u16) << 16),
            ),
    }
}
