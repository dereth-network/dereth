// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Pet_Monster.cs
//! Port of `Source/ACE.Server/WorldObjects/Pet_Monster.cs`: combat pets waking up monsters.

use empyrean_entity::enums::Tolerance;
use empyrean_entity::ObjectGuid;

use crate::world_objects::monster::State;
use crate::world_objects::world_object::WorldObject;
use crate::world_objects::{
    creature_combat, monster, monster_awareness, monster_combat, monster_navigation,
};
use crate::World;

/// Non-property fields declared in `Pet_Monster.cs`.
#[derive(Debug, Default)]
pub struct PetMonsterFields {}

/// `Creature.PlayerCombatPet_MoveExclude` (`Monster_Combat.cs`): if one of these is set, potential
/// aggro from Player or CombatPet movement terminates immediately.
const PLAYER_COMBAT_PET_MOVE_EXCLUDE: Tolerance = Tolerance(
    Tolerance::NoAttack.0
        | Tolerance::Appraise.0
        | Tolerance::Provoke.0
        | Tolerance::Retaliate.0
        | Tolerance::Monster.0,
);

fn object(w: &World, g: ObjectGuid) -> &WorldObject {
    w.objects.get(g).unwrap_or_else(|| {
        panic!(
            "System.NullReferenceException: object 0x{:08X} is not in World.objects",
            g.full()
        )
    })
}

// ACE: CombatPet.PetCheckMonsters
/// Wakes up any monsters within the applicable range.
pub fn pet_check_monsters(w: &mut World, this: ObjectGuid) {
    //if (!Attackable) return;

    let h = object(w, this)
        .phys
        .expect("ACE: PhysicsObj is null (NullReferenceException)");
    let creatures = crate::physics::object_maint::get_visible_targets_values_of_type_creature(w, h);

    for monster in creatures {
        if monster_combat::is_dead(object(w, monster)) {
            continue;
        }

        //if (Location.SquaredDistanceTo(monster.Location) <= monster.VisualAwarenessRangeSq)
        let dist_sq = monster_navigation::get_distance_sq_to_object(w, this, monster, true);
        if dist_sq <= f64::from(monster_awareness::visual_awareness_range_sq(w, monster)) {
            pet_alert_monster(w, this, monster);
        }
    }
}

// ACE: CombatPet.PetAlertMonster
/// Wakes up a monster if it can be alerted.
fn pet_alert_monster(w: &mut World, this: ObjectGuid, monster: ObjectGuid) -> bool {
    let m = object(w, monster);
    if !m.attackable()
        || monster::monster_state(w, monster) != State::Idle
        || (m.tolerance() & PLAYER_COMBAT_PET_MOVE_EXCLUDE) != Tolerance(0)
    {
        return false;
    }

    // if the combat pet's owner belongs to a faction,
    // and the monster also belongs to the same faction, don't aggro the monster?
    if creature_combat::same_faction(w, this, monster) {
        // unless the pet owner or the pet is being retaliated against?
        let owner = crate::world_objects::pet::p_pet_owner(w, this);
        if !creature_combat::has_retaliate_target(w, monster, owner)
            && !creature_combat::has_retaliate_target(w, monster, Some(this))
        {
            return false;
        }

        creature_combat::add_retaliate_target(w, monster, this);
    }

    monster_combat::set_attack_target(w, monster, Some(this));
    monster_awareness::wake_up(w, monster, true);

    true
}

// ACE: CombatPet.PetOnAttackMonster
/// Called when a combat pet attacks a monster.
pub fn pet_on_attack_monster(w: &mut World, this: ObjectGuid, monster: ObjectGuid) {
    /*Console.WriteLine($"{Name}.PetOnAttackMonster({monster.Name})");
    Console.WriteLine($"Attackable: {monster.Attackable}");
    Console.WriteLine($"Tolerance: {monster.Tolerance}");*/

    // faction mobs will retaliate against combat pets belonging to the same faction
    if creature_combat::same_faction(w, monster, this) {
        creature_combat::add_retaliate_target(w, monster, this);
    }

    if monster::monster_state(w, monster) == State::Idle
        && (object(w, monster).tolerance() & creature_combat::PLAYER_COMBAT_PET_RETALIATE_EXCLUDE)
            == Tolerance(0)
    {
        monster_combat::set_attack_target(w, monster, Some(this));
        monster_awareness::wake_up(w, monster, true);
    }
}
