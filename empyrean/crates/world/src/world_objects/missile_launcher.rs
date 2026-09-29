// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/MissileLauncher.cs
//! Port of `Source/ACE.Server/WorldObjects/MissileLauncher.cs`.

/// Non-property fields declared in `MissileLauncher.cs`.
#[derive(Debug, Default)]
pub struct MissileLauncherFields {}

// ---- constructors and SetEphemeralValues ----

/// `new MissileLauncher(weenie, guid)` / `new MissileLauncher(biota)`: the `WorldObject` constructor, then
/// MissileLauncher's `SetEphemeralValues`.
// ACE: MissileLauncher.MissileLauncher
pub fn missile_launcher_ctor(
    o: &mut crate::world_objects::world_object::WorldObject,
    env: &crate::world_objects::world_object::CtorEnv<'_>,
    src: crate::world_objects::world_object::CtorSource,
) {
    crate::world_objects::world_object::world_object_ctor(o, env, src);
    missile_launcher_set_ephemeral_values(o, env);
}

/// Empty in ACE.
// ACE: MissileLauncher.SetEphemeralValues
fn missile_launcher_set_ephemeral_values(
    _o: &mut crate::world_objects::world_object::WorldObject,
    _env: &crate::world_objects::world_object::CtorEnv<'_>,
) {
}
