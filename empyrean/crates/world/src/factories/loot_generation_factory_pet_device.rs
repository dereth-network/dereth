// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/LootGenerationFactory_PetDevice.cs
//! Port of `Source/ACE.Server/Factories/LootGenerationFactory_PetDevice.cs`.

use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_tables::logic::tables::workmanship_chance;

use crate::world_objects::world_object::WorldObject;

/// Each of the six gear ratings has a 50% chance of a rolled rating; then workmanship.
// ACE: LootGenerationFactory.MutatePetDevice
pub(crate) fn mutate_pet_device(pet_device: &mut WorldObject, tier: i32) {
    if !pet_device.is_pet_device() {
        return;
    }

    let rating_chance = 0.5f32;

    // add rng ratings to pet device
    // linear or biased?
    if f64::from(rating_chance) > ThreadSafeRandom::next_float(0.0, 1.0) {
        pet_device.set_gear_damage(Some(generate_pet_device_rating(tier)));
    }
    if f64::from(rating_chance) > ThreadSafeRandom::next_float(0.0, 1.0) {
        pet_device.set_gear_damage_resist(Some(generate_pet_device_rating(tier)));
    }
    if f64::from(rating_chance) > ThreadSafeRandom::next_float(0.0, 1.0) {
        pet_device.set_gear_crit_damage(Some(generate_pet_device_rating(tier)));
    }
    if f64::from(rating_chance) > ThreadSafeRandom::next_float(0.0, 1.0) {
        pet_device.set_gear_crit_damage_resist(Some(generate_pet_device_rating(tier)));
    }
    if f64::from(rating_chance) > ThreadSafeRandom::next_float(0.0, 1.0) {
        pet_device.set_gear_crit(Some(generate_pet_device_rating(tier)));
    }
    if f64::from(rating_chance) > ThreadSafeRandom::next_float(0.0, 1.0) {
        pet_device.set_gear_crit_resist(Some(generate_pet_device_rating(tier)));
    }

    pet_device.set_item_workmanship(Some(workmanship_chance::roll(tier)));
}

/// 1-10, plus another 1-10 with a `0.4 + tier * 0.02` chance.
// ACE: LootGenerationFactory.GeneratePetDeviceRating
fn generate_pet_device_rating(tier: i32) -> i32 {
    // thanks to morosity for this formula!
    let mut base_rating = ThreadSafeRandom::next(1, 10);

    #[allow(clippy::cast_precision_loss)]
    let chance = 0.4f32 + tier as f32 * 0.02f32;
    let rng = ThreadSafeRandom::next_float(0.0, 1.0);
    if rng < f64::from(chance) {
        base_rating += ThreadSafeRandom::next(1, 10);
    }

    base_rating
}
