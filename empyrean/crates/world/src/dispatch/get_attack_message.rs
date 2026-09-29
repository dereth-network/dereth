// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/WorldObject.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/WorldObject.cs`; do not edit by hand
//! Virtual dispatch for `WorldObject.GetAttackMessage`: each call
//! runs the implementation of the nearest class up the object's chain that declares or overrides
//! the member. The rules are in `dispatch/mod.rs`.

/// Dispatch for `WorldObject.GetAttackMessage(Creature creature, DamageType damageType, uint amount)` (Source/ACE.Server/WorldObjects/WorldObject.cs).
/// Overridden by: none.
pub fn get_attack_message(
    w: &crate::World,
    this: empyrean_entity::ObjectGuid,
    creature: empyrean_entity::ObjectGuid,
    damage_type: empyrean_entity::enums::DamageType,
    amount: u32,
) -> String {
    // No class overrides it: every object runs WorldObject's implementation.
    crate::world_objects::world_object::world_object_get_attack_message(
        w,
        this,
        creature,
        damage_type,
        amount,
    )
}
