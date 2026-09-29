// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Creature_Melee.cs
//! Port of `Source/ACE.Server/WorldObjects/Creature_Melee.cs`.
//!
//! Creature melee combat for players and monsters: dual wield and two-handed state, multistrike,
//! strike counts and cleaving.

use empyrean_entity::enums::{AttackType, MotionStance, TargetingTactic};
use empyrean_entity::ObjectGuid;

use crate::physics::phys_ext;
use crate::world_objects::world_object::WorldObject;
use crate::World;
use dereth_physics::PhysHandle;
use empyrean_common::dotnet::sort::{float_compare_to, list_sort};

/// Non-property fields declared in `Creature_Melee.cs`.
#[derive(Debug, Default)]
pub struct CreatureMeleeFields {
    /// Dual wield alternate will be true if *next* attack is offhand.
    // ACE: Creature.DualWieldAlternate
    pub dual_wield_alternate: bool,
}

fn object(w: &World, g: ObjectGuid) -> &WorldObject {
    w.objects.get(g).unwrap_or_else(|| {
        panic!(
            "System.NullReferenceException: object 0x{:08X} is not in World.objects",
            g.full()
        )
    })
}

/// `CurrentMotionState?.Stance`.
fn stance(w: &World, this: ObjectGuid) -> Option<MotionStance> {
    object(w, this)
        .wo
        .world_object_properties
        .current_motion_state
        .as_ref()
        .map(|m| m.stance)
}

/// Returns TRUE for DualWieldCombat mode.
// ACE: Creature.IsDualWieldAttack
#[must_use]
pub fn is_dual_wield_attack(w: &World, this: ObjectGuid) -> bool {
    stance(w, this) == Some(MotionStance::DualWieldCombat)
}

// ACE: Creature.DualWieldAlternate
/// # Panics
/// When `this` is missing or not a Creature.
#[must_use]
pub fn dual_wield_alternate(w: &World, this: ObjectGuid) -> bool {
    object(w, this)
        .creature
        .as_ref()
        .expect("InvalidCastException: not a Creature")
        .creature_melee
        .dual_wield_alternate
}

/// Returns TRUE for TwoHandedCombat mode.
// ACE: Creature.TwoHandedCombat
#[must_use]
pub fn two_handed_combat(w: &World, this: ObjectGuid) -> bool {
    let s = stance(w, this);
    s == Some(MotionStance::TwoHandedSwordCombat) || s == Some(MotionStance::TwoHandedStaffCombat)
}

/// Determines if a weapon can double or triple strike, and appends the appropriate multistrike
/// prefix to a MotionCommand.
// ACE: Creature.MultiStrike
#[must_use]
pub fn multi_strike(attack_type: AttackType, action: &str) -> String {
    if (attack_type & AttackType::MultiStrike) == AttackType::Undef {
        return action.to_owned();
    }

    let double_strike = if action.ends_with("Thrust") {
        AttackType::DoubleThrust
    } else {
        AttackType::DoubleSlash
    };
    let triple_strike = AttackType(double_strike.0.wrapping_mul(2));

    if attack_type.contains(triple_strike) {
        return format!("Triple{action}");
    }
    if attack_type.contains(double_strike) {
        format!("Double{action}")
    } else {
        action.to_owned()
    }
}

/// Returns the attack types for a weapon.
// ACE: Creature.GetWeaponAttackType
#[must_use]
pub fn get_weapon_attack_type(w: &World, weapon: Option<ObjectGuid>) -> AttackType {
    match weapon {
        None => AttackType::Undef,
        Some(weapon) => object(w, weapon).w_attack_type(),
    }
}

/// Returns the number of strikes for a weapon, between 1-3 strikes.
// ACE: Creature.GetNumStrikes
#[must_use]
pub fn get_num_strikes(w: &World, this: ObjectGuid, weapon: Option<ObjectGuid>) -> i32 {
    let attack_type = get_weapon_attack_type(w, weapon);

    get_num_strikes_of(w, this, attack_type)
}

/// Returns the number of strikes for an AttackType, between 1-3 strikes.
///
/// # Panics
/// Without a `CurrentMotionState` (ACE: `NullReferenceException`).
// ACE: Creature.GetNumStrikes
#[must_use]
pub fn get_num_strikes_of(w: &World, this: ObjectGuid, attack_type: AttackType) -> i32 {
    let s = stance(w, this).expect("System.NullReferenceException: CurrentMotionState");
    if s == MotionStance::TwoHandedSwordCombat || s == MotionStance::TwoHandedStaffCombat {
        return 2;
    }

    if (attack_type & AttackType::MultiStrike) == AttackType::Undef {
        return 1;
    }

    if attack_type.contains(AttackType::TripleSlash)
        || attack_type.contains(AttackType::TripleThrust)
        || attack_type.contains(AttackType::OffhandTripleSlash)
        || attack_type.contains(AttackType::OffhandTripleThrust)
    {
        3
    } else if attack_type.contains(AttackType::DoubleSlash)
        || attack_type.contains(AttackType::DoubleThrust)
        || attack_type.contains(AttackType::OffhandDoubleSlash)
        || attack_type.contains(AttackType::OffhandDoubleThrust)
    {
        2
    } else {
        1
    }
}

/// The world object behind a physics body (`a.WeenieObj.WorldObject`).
fn world_object_of(w: &World, h: PhysHandle) -> Option<ObjectGuid> {
    phys_ext::weenie_obj(w, h).world_object(w)
}

/// Compares two bodies by the squared distance of their world objects to this creature
/// (`Location.SquaredDistanceTo(...)`, then `float.CompareTo`).
///
/// # Panics
/// Without locations (ACE: `NullReferenceException`).
// ACE: Creature.DistanceComparator
#[must_use]
pub fn distance_comparator(w: &World, this: ObjectGuid, a: PhysHandle, b: PhysHandle) -> i32 {
    // use square distance to make things a bit faster
    let location = object(w, this)
        .location()
        .expect("System.NullReferenceException: Location");
    let loc = |h| world_object_of(w, h).and_then(|g| object(w, g).location());
    let dist1 = location.squared_distance_to(loc(a).as_ref());
    let dist2 = location.squared_distance_to(loc(b).as_ref());

    float_compare_to(dist1, dist2)
}

// ACE: Creature.CleaveRange
pub const CLEAVE_RANGE: f32 = 5.0;
// ACE: Creature.CleaveRangeSq
pub const CLEAVE_RANGE_SQ: f32 = CLEAVE_RANGE * CLEAVE_RANGE;
// ACE: Creature.CleaveAngle
pub const CLEAVE_ANGLE: f32 = 180.0;

// ACE: Creature.CleaveCylRange
pub const CLEAVE_CYL_RANGE: f32 = 2.0;

/// Performs a cleaving attack for two-handed weapons: the list of cleave targets to hit with this
/// attack (`None` for a non-cleaving weapon).
///
/// # Panics
/// Without physics bodies (ACE: `NullReferenceException` on `PhysicsObj`).
// ACE: Creature.GetCleaveTarget
pub fn get_cleave_target(
    w: &mut World,
    this: ObjectGuid,
    target: ObjectGuid,
    weapon: ObjectGuid,
) -> Option<Vec<ObjectGuid>> {
    let player = object(w, this).is_player();

    if !object(w, weapon).is_cleaving() {
        return None;
    }

    // sort visible objects by ascending distance
    let body = object(w, this)
        .phys
        .expect("System.NullReferenceException: PhysicsObj");
    let mut visible =
        crate::physics::object_maint::get_visible_objects_values_where(w, body, |o| {
            world_object_of(w, o).is_some()
        });
    list_sort(&mut visible, |a, b| distance_comparator(w, this, *a, *b));

    let mut cleave_targets = Vec::new();
    let total_cleaves = object(w, weapon).cleave_targets();

    // DIVERGE: a target destroyed since the swing (V202) is gone from the store; ACE's held
    // reference keeps its PhysicsObj, which is no longer visible, so skipping nothing is the same.
    let target_body = w.objects.get(target).map(|o| {
        o.phys
            .expect("System.NullReferenceException: target.PhysicsObj")
    });

    for obj in visible {
        // cleaving skips original target
        if Some(obj) == target_body {
            continue;
        }

        // only cleave creatures
        let Some(creature) = world_object_of(w, obj).filter(|&g| object(w, g).is_creature()) else {
            continue;
        };
        if teleporting(w, creature)
            || crate::world_objects::monster_combat::is_dead(object(w, creature))
        {
            continue;
        }

        if player && check_pk_status_vs_target(w, this, creature) {
            continue;
        }

        let c = object(w, creature);
        if !c.attackable() && c.targeting_tactic() == TargetingTactic::None
            || teleporting(w, creature)
        {
            continue;
        }

        if c.is_combat_pet() && (player || object(w, this).is_combat_pet()) {
            continue;
        }

        // no objects in cleave range
        let cyl_dist =
            crate::world_objects::world_object_use::get_cylinder_distance(w, this, creature);
        if cyl_dist > CLEAVE_CYL_RANGE {
            return Some(cleave_targets);
        }

        // only cleave in front of attacker
        let angle = crate::world_objects::creature_navigation::get_angle(w, this, creature);
        if angle.abs() > CLEAVE_ANGLE / 2.0 {
            continue;
        }

        // found cleavable object
        cleave_targets.push(creature);
        if cleave_targets.len() == usize::try_from(total_cleaves).unwrap_or(usize::MAX) {
            break;
        }
    }
    Some(cleave_targets)
}

/// `WorldObject.Teleporting` (an auto-property of `WorldObject.cs`).
fn teleporting(w: &World, creature: ObjectGuid) -> bool {
    w.objects
        .get(creature)
        .is_some_and(|o| o.wo.world_object.teleporting)
}

/// `player.CheckPKStatusVsTarget(creature, null) != null` (`Player_Combat.cs`).
fn check_pk_status_vs_target(w: &mut World, player: ObjectGuid, creature: ObjectGuid) -> bool {
    crate::world_objects::player_combat::check_pk_status_vs_target(w, player, Some(creature), None)
        .is_some()
}
