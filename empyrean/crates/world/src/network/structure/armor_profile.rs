// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Structure/ArmorProfile.cs
//! Port of `Source/ACE.Server/Network/Structure/ArmorProfile.cs`.

use empyrean_entity::enums::DamageType;
use empyrean_entity::ObjectGuid;

use crate::network::game_messages::game_message::write_record;
use crate::world_objects::world_object_networking::shims;
use crate::World;

// ACE: ArmorProfile
/// All of the resistance levels for a piece of armor / clothing (natural, banes, lures).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ArmorProfile {
    // ACE: ArmorProfile.SlashingProtection
    pub slashing_protection: f32,
    // ACE: ArmorProfile.PiercingProtection
    pub piercing_protection: f32,
    // ACE: ArmorProfile.BludgeoningProtection
    pub bludgeoning_protection: f32,
    // ACE: ArmorProfile.ColdProtection
    pub cold_protection: f32,
    // ACE: ArmorProfile.FireProtection
    pub fire_protection: f32,
    // ACE: ArmorProfile.AcidProtection
    pub acid_protection: f32,
    // ACE: ArmorProfile.NetherProtection
    pub nether_protection: f32,
    // ACE: ArmorProfile.LightningProtection
    pub lightning_protection: f32,
}

// ACE: ArmorProfile.ArmorProfile
pub fn armor_profile_new(w: &World, armor: ObjectGuid) -> ArmorProfile {
    ArmorProfile {
        slashing_protection: get_armor_mod(w, armor, DamageType::Slash),
        piercing_protection: get_armor_mod(w, armor, DamageType::Pierce),
        bludgeoning_protection: get_armor_mod(w, armor, DamageType::Bludgeon),
        cold_protection: get_armor_mod(w, armor, DamageType::Cold),
        fire_protection: get_armor_mod(w, armor, DamageType::Fire),
        acid_protection: get_armor_mod(w, armor, DamageType::Acid),
        nether_protection: get_armor_mod(w, armor, DamageType::Nether),
        lightning_protection: get_armor_mod(w, armor, DamageType::Electric),
    }
}

// ACE: ArmorProfile.GetArmorMod
/// The effective RL of a piece of armor or clothing against one damage type: the base
/// resistance (default 1) plus banes/lures, clamped to [-2, 2]. ACE's `armor == null` checks come
/// after `armor` has already been dereferenced, so they never fire.
#[allow(clippy::cast_possible_truncation)] // `(float)` of a double
pub fn get_armor_mod(w: &World, armor: ObjectGuid, damage_type: DamageType) -> f32 {
    let r#type = shims::enchantment_manager_get_impen_bane_key(damage_type);
    let base_resistance = w
        .objects
        .get(armor)
        .expect("ACE: armor is null")
        .get_property(r#type)
        .unwrap_or(1.0);

    // banes/lures
    let resistance_mod = shims::enchantment_manager_get_armor_mod_vs_type(w, armor, damage_type);

    let mut effective_rl = (base_resistance + f64::from(resistance_mod)) as f32;

    // resistance clamp
    // TODO: this would be a good place to test with client values
    //if (effectiveRL > 2.0f)
    //effectiveRL = 2.0f;
    effective_rl = math_clamp(effective_rl, -2.0, 2.0);

    effective_rl
}

/// `Math.Clamp(float, float, float)`: NaN passes through.
fn math_clamp(value: f32, min: f32, max: f32) -> f32 {
    if value < min {
        min
    } else if value > max {
        max
    } else {
        value
    }
}

// ACE: ArmorProfileExtensions.Write
/// Writes the ArmorProfile to the network stream.
pub fn write(writer: &mut Vec<u8>, profile: &ArmorProfile) {
    write_record(writer, &[], |w| record(profile).write(w));
}

/// The dereth-protocol record the `Write` extension below writes, field for field.
#[must_use]
pub fn record(profile: &ArmorProfile) -> dereth_protocol::types::ArmorProfile {
    dereth_protocol::types::ArmorProfile {
        mod_vs_slash: profile.slashing_protection,
        mod_vs_pierce: profile.piercing_protection,
        mod_vs_bludgeon: profile.bludgeoning_protection,
        mod_vs_cold: profile.cold_protection,
        mod_vs_fire: profile.fire_protection,
        mod_vs_acid: profile.acid_protection,
        mod_vs_nether: profile.nether_protection,
        mod_vs_electric: profile.lightning_protection,
    }
}
