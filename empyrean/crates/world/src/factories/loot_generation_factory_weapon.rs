// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/LootGenerationFactory_Weapon.cs
//! Port of `Source/ACE.Server/Factories/LootGenerationFactory_Weapon.cs`.

use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_content::models::world::TreasureDeath;
use empyrean_tables::logic::tables::weapon_type_chance;

use crate::factories::loot_generation_factory::tables_logic::tables::quality_chance;
use crate::factories::loot_generation_factory_caster::create_caster;
use crate::factories::loot_generation_factory_melee::create_melee_weapon;
use crate::factories::loot_generation_factory_missile::create_missile_weapon;
use crate::world_objects::world_object::WorldObject;
use crate::World;

/// This is only called by /testlootgen command. The actual lootgen system doesn't use this.
// ACE: LootGenerationFactory.CreateWeapon
pub(crate) fn create_weapon(
    w: &mut World,
    profile: &TreasureDeath,
    is_magical: bool,
) -> Option<WorldObject> {
    let weapon_type = weapon_type_chance::roll(profile.tier);

    if weapon_type.is_melee_weapon() {
        create_melee_weapon(w, profile, is_magical)
    } else if weapon_type.is_missile_weapon() {
        create_missile_weapon(w, profile, is_magical, true)
    } else {
        create_caster(w, profile, is_magical)
    }
}

/// A weapon speed multiplier from the quality level: 1 (no bonus), or `1 - (level * 0.025 + rng)`,
/// a 67.5% - 100% range.
// ACE: LootGenerationFactory.RollWeaponSpeedMod
pub(crate) fn roll_weapon_speed_mod(treasure_death: &TreasureDeath) -> f32 {
    let quality_level = quality_chance::roll(treasure_death);

    if quality_level == 0 {
        return 1.0; // no bonus
    }

    #[allow(clippy::cast_possible_truncation)]
    let rng = ThreadSafeRandom::next_float(-0.025, 0.025) as f32;

    // min/max range: 67.5% - 100%
    #[allow(clippy::cast_precision_loss)]
    let weapon_speed_mod = 1.0f32 - (quality_level as f32 * 0.025 + rng);

    //Console.WriteLine($"WeaponSpeedMod: {weaponSpeedMod}");

    weapon_speed_mod
}
