// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Player_Monster.cs
//! Port of `Source/ACE.Server/WorldObjects/Player_Monster.cs`: player -> monster visibility
//! checks.

use empyrean_entity::enums::Tolerance;
use empyrean_entity::ObjectGuid;

use crate::world_objects::monster::State;
use crate::world_objects::world_object::WorldObject;
use crate::world_objects::{
    creature_combat, monster, monster_awareness, monster_combat, monster_navigation,
};
use crate::World;

/// Non-property fields declared in `Player_Monster.cs`.
#[derive(Debug, Default)]
pub struct PlayerMonsterFields {}

fn object(w: &World, g: ObjectGuid) -> &WorldObject {
    w.objects.get(g).unwrap_or_else(|| {
        panic!(
            "System.NullReferenceException: object 0x{:08X} is not in World.objects",
            g.full()
        )
    })
}

// ACE: Player.CheckMonsters
/// Wakes up any monsters within the applicable range.
pub fn check_monsters(w: &mut World, this: ObjectGuid) {
    let o = object(w, this);
    if !o.attackable() || o.wo.world_object.teleporting {
        return;
    }

    let h = o
        .phys
        .expect("ACE: PhysicsObj is null (NullReferenceException)");
    let visible_objs =
        crate::physics::object_maint::get_visible_objects_values_of_type_creature(w, h);

    for monster in visible_objs {
        if object(w, monster).is_player() {
            continue;
        }

        //if (Location.SquaredDistanceTo(monster.Location) <= monster.VisualAwarenessRangeSq)
        let dist_sq = monster_navigation::get_distance_sq_to_object(w, this, monster, true);
        if dist_sq <= f64::from(monster_awareness::visual_awareness_range_sq(w, monster)) {
            creature_combat::alert_monster(w, this, monster);
        }
    }
}

// ACE: Player.OnAttackMonster
/// Called when this player attacks a monster.
pub fn on_attack_monster(w: &mut World, this: ObjectGuid, monster: Option<ObjectGuid>) {
    let Some(monster) = monster else { return };
    if !object(w, this).attackable() {
        return;
    }

    /*Console.WriteLine($"{Name}.OnAttackMonster({monster.Name})");
    Console.WriteLine($"Attackable: {monster.Attackable}");
    Console.WriteLine($"Tolerance: {monster.Tolerance}");*/

    // faction mobs will retaliate against players belonging to the same faction
    if creature_combat::same_faction(w, this, monster) {
        creature_combat::add_retaliate_target(w, monster, this);
    }

    if monster::monster_state(w, monster) != State::Awake
        && (object(w, monster).tolerance() & creature_combat::PLAYER_COMBAT_PET_RETALIATE_EXCLUDE)
            == Tolerance(0)
    {
        monster_combat::set_attack_target(w, monster, Some(this));
        monster_awareness::wake_up(w, monster, true);
    }
}
