// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/LightSource.cs
//! Port of `Source/ACE.Server/WorldObjects/LightSource.cs`.

/// Non-property fields declared in `LightSource.cs`.
#[derive(Debug, Default)]
pub struct LightSourceFields {}

// ---- virtual-dispatch targets ----

/// Do nothing.
// ACE: LightSource.ActOnUse
pub fn light_source_act_on_use(
    _w: &mut crate::World,
    _this: empyrean_entity::ObjectGuid,
    _activator: empyrean_entity::ObjectGuid,
) {
}

// ---- constructors and SetEphemeralValues ----

/// `new LightSource(weenie, guid)` / `new LightSource(biota)`: the `GenericObject` constructor, then
/// LightSource's `SetEphemeralValues`.
// ACE: LightSource.LightSource
pub fn light_source_ctor(
    o: &mut crate::world_objects::world_object::WorldObject,
    env: &crate::world_objects::world_object::CtorEnv<'_>,
    src: crate::world_objects::world_object::CtorSource,
) {
    crate::world_objects::generic_object::generic_object_ctor(o, env, src);
    light_source_set_ephemeral_values(o, env);
}

/// Empty in ACE.
// ACE: LightSource.SetEphemeralValues
fn light_source_set_ephemeral_values(
    _o: &mut crate::world_objects::world_object::WorldObject,
    _env: &crate::world_objects::world_object::CtorEnv<'_>,
) {
}
