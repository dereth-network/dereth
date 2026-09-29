// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Caster.cs
//! Port of `Source/ACE.Server/WorldObjects/Caster.cs`.

/// Non-property fields declared in `Caster.cs`.
#[derive(Debug, Default)]
pub struct CasterFields {}

// ---- constructors and SetEphemeralValues ----

/// `new Caster(weenie, guid)` / `new Caster(biota)`: the `WorldObject` constructor, then
/// Caster's `SetEphemeralValues`.
// ACE: Caster.Caster
pub fn caster_ctor(
    o: &mut crate::world_objects::world_object::WorldObject,
    env: &crate::world_objects::world_object::CtorEnv<'_>,
    src: crate::world_objects::world_object::CtorSource,
) {
    crate::world_objects::world_object::world_object_ctor(o, env, src);
    caster_set_ephemeral_values(o, env);
}

/// Empty in ACE.
// ACE: Caster.SetEphemeralValues
fn caster_set_ephemeral_values(
    _o: &mut crate::world_objects::world_object::WorldObject,
    _env: &crate::world_objects::world_object::CtorEnv<'_>,
) {
}
