// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/WorldObject_Combat.cs
//! Port of `Source/ACE.Server/WorldObjects/WorldObject_Combat.cs`.

/// Non-property fields declared in `WorldObject_Combat.cs`.
#[derive(Debug, Default)]
pub struct WorldObjectCombatFields {}

// ---- virtual-dispatch targets: each `not_ported!` until it is ported ----

// ACE: WorldObject.CheckPKStatusVsTarget
#[allow(unused_variables)]
pub fn world_object_check_pk_status_vs_target(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    target: empyrean_entity::ObjectGuid,
    spell: (),
) -> Option<Vec<empyrean_entity::enums::WeenieErrorWithString>> {
    None
}

// ---- WorldObject_Combat.cs members ----

fn has_proc_self_targeted(
    w: &crate::World,
    g: empyrean_entity::ObjectGuid,
    self_target: bool,
) -> bool {
    let o = w.objects.get(g).unwrap_or_else(|| {
        panic!(
            "System.NullReferenceException: object 0x{:08X} is not in World.objects",
            g.full()
        )
    });
    crate::world_objects::world_object_weapon::has_proc(o)
        && o.proc_spell_self_targeted() == self_target
}

// ACE: WorldObject.TryProcEquippedItems
/// The procs an attack can set off: on this object (a phial, or a monster with a proc spell on
/// itself), on the weapon (melee weapon or missile launcher), on a monster that fired this
/// projectile, and on the attacker's equipped aetheria.
pub fn try_proc_equipped_items(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    attacker: empyrean_entity::ObjectGuid,
    target: empyrean_entity::ObjectGuid,
    self_target: bool,
    weapon: Option<empyrean_entity::ObjectGuid>,
) {
    use crate::world_objects::world_object_weapon::try_proc_item;

    // handle procs directly on this item -- ie. phials
    // this could also be monsters with the proc spell directly on the creature
    if has_proc_self_targeted(w, this, self_target) {
        // projectile
        // monster
        try_proc_item(w, this, attacker, Some(target), self_target);
    }

    // handle proc spells for weapon
    // this could be a melee weapon, or a missile launcher
    if let Some(weapon) = weapon {
        if has_proc_self_targeted(w, weapon, self_target) {
            // weapon
            try_proc_item(w, weapon, attacker, Some(target), self_target);
        }
    }

    if attacker != this && has_proc_self_targeted(w, attacker, self_target) {
        // handle special case -- missile projectiles from monsters w/ a proc directly on the mob
        // monster
        try_proc_item(w, attacker, attacker, Some(target), self_target);
    }

    // handle aetheria procs
    if w.objects
        .get(attacker)
        .is_some_and(crate::world_objects::world_object::WorldObject::is_creature)
    {
        let wielder = attacker;
        let equipped_aetheria: Vec<empyrean_entity::ObjectGuid> =
            crate::world_objects::creature_equipment::equipped_objects_values(w, wielder)
                .into_iter()
                .filter(|&i| {
                    w.objects.get(i).is_some_and(|o| {
                        crate::entity::aetheria::is_aetheria(o.biota.weenie_class_id)
                    }) && has_proc_self_targeted(w, i, self_target)
                })
                .collect();

        // aetheria
        for aetheria in equipped_aetheria {
            try_proc_item(w, aetheria, attacker, Some(target), self_target);
        }
    }
}
