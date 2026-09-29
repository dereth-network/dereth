// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/MeleeWeapon.cs
//! Port of `Source/ACE.Server/WorldObjects/MeleeWeapon.cs`.

/// Non-property fields declared in `MeleeWeapon.cs`.
#[derive(Debug, Default)]
pub struct MeleeWeaponFields {}

// ---- constructors and SetEphemeralValues ----

/// `new MeleeWeapon(weenie, guid)` / `new MeleeWeapon(biota)`: the `WorldObject` constructor, then
/// MeleeWeapon's `SetEphemeralValues`.
// ACE: MeleeWeapon.MeleeWeapon
pub fn melee_weapon_ctor(
    o: &mut crate::world_objects::world_object::WorldObject,
    env: &crate::world_objects::world_object::CtorEnv<'_>,
    src: crate::world_objects::world_object::CtorSource,
) {
    crate::world_objects::world_object::world_object_ctor(o, env, src);
    melee_weapon_set_ephemeral_values(o, env);
}

// ACE: MeleeWeapon.SetEphemeralValues
fn melee_weapon_set_ephemeral_values(
    o: &mut crate::world_objects::world_object::WorldObject,
    _env: &crate::world_objects::world_object::CtorEnv<'_>,
) {
    o.wo.world_object_properties.current_motion_state = None;
}
