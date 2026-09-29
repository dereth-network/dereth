// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Missile.cs
//! Port of `Source/ACE.Server/WorldObjects/Missile.cs`.

/// Non-property fields declared in `Missile.cs`.
#[derive(Debug, Default)]
pub struct MissileFields {}

// ---- constructors and SetEphemeralValues ----

/// `new Missile(weenie, guid)` / `new Missile(biota)`: the `Stackable` constructor, then
/// Missile's `SetEphemeralValues`.
// ACE: Missile.Missile
pub fn missile_ctor(
    o: &mut crate::world_objects::world_object::WorldObject,
    env: &crate::world_objects::world_object::CtorEnv<'_>,
    src: crate::world_objects::world_object::CtorSource,
) {
    crate::world_objects::stackable::stackable_ctor(o, env, src);
    missile_set_ephemeral_values(o, env);
}

/// Empty in ACE.
// ACE: Missile.SetEphemeralValues
fn missile_set_ephemeral_values(
    _o: &mut crate::world_objects::world_object::WorldObject,
    _env: &crate::world_objects::world_object::CtorEnv<'_>,
) {
}
