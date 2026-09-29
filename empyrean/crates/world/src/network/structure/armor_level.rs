// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Structure/ArmorLevel.cs
//! Port of `Source/ACE.Server/Network/Structure/ArmorLevel.cs`.

use empyrean_entity::enums::PropertyInt;
use empyrean_entity::ObjectGuid;

use crate::entity::body_part::{self as body_parts, BodyPart};
use crate::network::game_messages::game_message::write_record;
use crate::world_objects::world_object_networking::shims;
use crate::World;

// ACE: ArmorLevel
/// Handles the per-body part AL display for the character panel.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ArmorLevel {
    // ACE: ArmorLevel.Head
    pub head: u32,
    // ACE: ArmorLevel.Chest
    pub chest: u32,
    // ACE: ArmorLevel.Abdomen
    pub abdomen: u32,
    // ACE: ArmorLevel.UpperArm
    pub upper_arm: u32,
    // ACE: ArmorLevel.LowerArm
    pub lower_arm: u32,
    // ACE: ArmorLevel.Hand
    pub hand: u32,
    // ACE: ArmorLevel.UpperLeg
    pub upper_leg: u32,
    // ACE: ArmorLevel.LowerLeg
    pub lower_leg: u32,
    // ACE: ArmorLevel.Foot
    pub foot: u32,
}

// ACE: ArmorLevel.ArmorLevel
/// `new ArmorLevel(Creature creature)`: base + enchanted AL per body part.
pub fn armor_level_new(w: &World, creature: ObjectGuid) -> ArmorLevel {
    ArmorLevel {
        head: get_armor_level(w, creature, BodyPart::Head),
        chest: get_armor_level(w, creature, BodyPart::Chest),
        abdomen: get_armor_level(w, creature, BodyPart::Abdomen),
        upper_arm: get_armor_level(w, creature, BodyPart::UpperArm),
        lower_arm: get_armor_level(w, creature, BodyPart::LowerArm),
        hand: get_armor_level(w, creature, BodyPart::Hand),
        upper_leg: get_armor_level(w, creature, BodyPart::UpperLeg),
        lower_leg: get_armor_level(w, creature, BodyPart::LowerLeg),
        foot: get_armor_level(w, creature, BodyPart::Foot),
    }
}

// ACE: ArmorLevel.GetArmorLevel
/// The total AL of the armor and clothing covering `body_part`; `+ 9999` (shown as `*` by the
/// client) when every layer is unenchantable.
#[allow(clippy::cast_sign_loss)] // `(uint)totalAL` after `Math.Max(0, ..)`
pub fn get_armor_level(w: &World, creature: ObjectGuid, body_part: BodyPart) -> u32 {
    // get armor pieces covering this body part
    let layers = get_armor_clothing(w, creature, body_part);

    // get total AL
    let mut total_al: i32 = 0;

    for layer in &layers {
        let base_al = w
            .objects
            .get(*layer)
            .and_then(|o| o.get_property(PropertyInt::ArmorLevel))
            .unwrap_or(0);

        // impen / brittlemail
        let mod_al = shims::enchantment_manager_get_armor_mod(w, *layer);

        total_al = total_al.wrapping_add(base_al.wrapping_add(mod_al));
    }

    // doesn't handle negatives?
    total_al = total_al.max(0);

    // if all layers for this body part are unenchantable,
    // send totalAL + 9999 for client to display *
    if !layers.is_empty() && !layers.iter().any(|i| shims::is_enchantable(w, *i)) {
        total_al = total_al.wrapping_add(9999);
    }

    total_al as u32
}

// ACE: ArmorLevel.GetArmorClothing
/// The equipped `Clothing` objects covering `body_part`, in `EquippedObjects` order.
pub fn get_armor_clothing(w: &World, creature: ObjectGuid, body_part: BodyPart) -> Vec<ObjectGuid> {
    let body_location = body_parts::get_flags_coverage(body_parts::get_coverage_mask(body_part));

    crate::world_objects::creature_equipment::equipped_objects_values(w, creature)
        .into_iter()
        .filter(|e| {
            w.objects.get(*e).is_some_and(|o| {
                o.is_clothing() && body_parts::has_any(o.clothing_priority(), &body_location)
            })
        })
        .collect()
}

// ACE: ArmorLevelExtensions.Write
pub fn write(writer: &mut Vec<u8>, armor_level: &ArmorLevel) {
    write_record(writer, &[], |w| {
        for v in record(armor_level) {
            w.u32(v);
        }
    });
}

/// The nine dwords dereth-protocol's appraisal record holds for the armor levels, in the `Write`
/// extension's order.
#[must_use]
pub fn record(armor_level: &ArmorLevel) -> [u32; 9] {
    [
        armor_level.head,
        armor_level.chest,
        armor_level.abdomen,
        armor_level.upper_arm,
        armor_level.lower_arm,
        armor_level.hand,
        armor_level.upper_leg,
        armor_level.lower_leg,
        armor_level.foot,
    ]
}
