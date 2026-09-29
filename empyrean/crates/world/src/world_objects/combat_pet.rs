// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/CombatPet.cs
//! Port of `Source/ACE.Server/WorldObjects/CombatPet.cs`: the summonable monsters' combat AI, over
//! the monster AI.

use empyrean_entity::enums::CombatMode;
use empyrean_entity::ObjectGuid;

use crate::world_objects::monster::State;
use crate::world_objects::world_object::WorldObject;
use crate::world_objects::{creature_combat, monster, monster_awareness, monster_combat};
use crate::World;

/// Non-property fields declared in `CombatPet.cs`.
#[derive(Debug, Default)]
pub struct CombatPetFields {}

fn object(w: &World, g: ObjectGuid) -> &WorldObject {
    w.objects.get(g).unwrap_or_else(|| {
        panic!(
            "System.NullReferenceException: object 0x{:08X} is not in World.objects",
            g.full()
        )
    })
}

// ACE: CombatPet.Init
/// `Pet.Init`, then the pet fights: melee stance, awake, the device's gear ratings and the owner's
/// factions.
pub fn combat_pet_init(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    player: empyrean_entity::ObjectGuid,
    pet_device: empyrean_entity::ObjectGuid,
) -> Option<bool> {
    let success = crate::world_objects::pet::pet_init(w, this, player, pet_device);

    if success != Some(true) {
        return success;
    }

    creature_combat::set_combat_mode(w, this, CombatMode::Melee);
    monster::set_monster_state_value(w, this, State::Awake);
    monster_awareness::fields_mut(w, this).is_awake = true;

    // copy ratings from pet device
    let d = object(w, pet_device);
    let (
        gear_damage,
        gear_damage_resist,
        gear_crit_damage,
        gear_crit_damage_resist,
        gear_crit,
        gear_crit_resist,
    ) = (
        d.gear_damage(),
        d.gear_damage_resist(),
        d.gear_crit_damage(),
        d.gear_crit_damage_resist(),
        d.gear_crit(),
        d.gear_crit_resist(),
    );
    let faction1_bits = object(w, player).faction1_bits();

    let o = w.objects.get_mut(this).expect("ACE: this is null");
    o.set_damage_rating(gear_damage);
    o.set_damage_resist_rating(gear_damage_resist);
    o.set_crit_damage_rating(gear_crit_damage);
    o.set_crit_damage_resist_rating(gear_crit_damage_resist);
    o.set_crit_rating(gear_crit);
    o.set_crit_resist_rating(gear_crit_resist);

    // are CombatPets supposed to attack monsters that are in the same faction as the pet owner?
    // if not, there are a couple of different approaches to this
    // the easiest way for the code would be to simply set Faction1Bits for the CombatPet to match the pet owner's
    // however, retail pcaps did not contain Faction1Bits for CombatPets

    // doing this the easiest way for the code here, and just removing during appraisal
    o.set_faction1_bits(faction1_bits);

    Some(true)
}

// ACE: CombatPet.HandleFindTarget
/// Looks for a new target when the current one is gone, dead or out of sight.
pub fn combat_pet_handle_find_target(w: &mut crate::World, this: empyrean_entity::ObjectGuid) {
    let creature = monster_combat::attack_target(w, this).filter(|&g| object(w, g).is_creature());

    let keep = creature.is_some_and(|c| {
        !monster_combat::is_dead(object(w, c))
            && crate::world_objects::world_object::is_visible_target(w, this, c)
    });
    if !keep {
        crate::dispatch::find_next_target::find_next_target(w, this);
    }
}

// ACE: CombatPet.FindNextTarget
/// Targets the nearest attackable monster within visual range.
pub fn combat_pet_find_next_target(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
) -> bool {
    let nearby_monsters = get_nearby_monsters(w, this);
    if nearby_monsters.is_empty() {
        //Console.WriteLine($"{Name}.FindNextTarget(): empty");
        return false;
    }

    // get nearest monster
    let nearest = monster_awareness::build_target_distance(w, this, &nearby_monsters, true);

    if nearest[0].distance > monster_awareness::visual_awareness_range_sq(w, this) {
        //Console.WriteLine($"{Name}.FindNextTarget(): next object out-of-range (dist: {Math.Round(Math.Sqrt(nearest[0].Distance))})");
        return false;
    }

    monster_combat::set_attack_target(w, this, Some(nearest[0].target));

    //Console.WriteLine($"{Name}.FindNextTarget(): {AttackTarget.Name}");

    true
}

// ACE: CombatPet.GetNearbyMonsters
/// Returns a list of attackable monsters in this pet's visible targets.
pub fn get_nearby_monsters(w: &World, this: ObjectGuid) -> Vec<ObjectGuid> {
    let mut monsters = Vec::new();

    let h = object(w, this)
        .phys
        .expect("ACE: PhysicsObj is null (NullReferenceException)");
    let owner = crate::world_objects::pet::p_pet_owner(w, this);

    for creature in crate::physics::object_maint::get_visible_targets_values_of_type_creature(w, h)
    {
        // why does this need to be in here?
        let c = object(w, creature);
        if monster_combat::is_dead(c) || !c.attackable() || c.visibility() {
            //Console.WriteLine($"{Name}.GetNearbyMonsters(): refusing to add dead creature {creature.Name} ({creature.Guid})");
            continue;
        }

        // combat pets do not aggro monsters belonging to the same faction as the pet owner?
        if creature_combat::same_faction(w, this, creature) {
            // unless the pet owner or the pet is being retaliated against?
            if !creature_combat::has_retaliate_target(w, creature, owner)
                && !creature_combat::has_retaliate_target(w, creature, Some(this))
            {
                continue;
            }
        }

        monsters.push(creature);
    }

    monsters
}

// ACE: CombatPet.Sleep
/// Pets dont really go to sleep, per say: they keep scanning for new targets, which is the
/// reverse of the current ACE jurassic park model.
#[allow(unused_variables)]
pub fn combat_pet_sleep(w: &mut crate::World, this: empyrean_entity::ObjectGuid) {
    // empty by default
}

// ---- constructors and SetEphemeralValues ----

/// `new CombatPet(weenie, guid)` / `new CombatPet(biota)`: the `Pet` constructor, then
/// CombatPet's `SetEphemeralValues`.
// ACE: CombatPet.CombatPet
pub fn combat_pet_ctor(
    o: &mut crate::world_objects::world_object::WorldObject,
    env: &crate::world_objects::world_object::CtorEnv<'_>,
    src: crate::world_objects::world_object::CtorSource,
) {
    crate::world_objects::pet::pet_ctor(o, env, src);
    combat_pet_set_ephemeral_values(o, env);
}

/// Empty in ACE.
// ACE: CombatPet.SetEphemeralValues
fn combat_pet_set_ephemeral_values(
    _o: &mut crate::world_objects::world_object::WorldObject,
    _env: &crate::world_objects::world_object::CtorEnv<'_>,
) {
}
