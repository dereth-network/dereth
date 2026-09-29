// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Cow.cs
//! Port of `Source/ACE.Server/WorldObjects/Cow.cs`.

/// Non-property fields declared in `Cow.cs`.
#[derive(Debug, Default)]
pub struct CowFields {}

// ---- virtual-dispatch targets ----

/// Handled in base.OnActivate -> EmoteManager.OnUse().
// ACE: Cow.ActOnUse
pub fn cow_act_on_use(
    _w: &mut crate::World,
    _this: empyrean_entity::ObjectGuid,
    _activator: empyrean_entity::ObjectGuid,
) {
}

// ---- constructors and SetEphemeralValues ----

/// `new Cow(weenie, guid)` / `new Cow(biota)`: the `Creature` constructor, then
/// Cow's `SetEphemeralValues`.
// ACE: Cow.Cow
pub fn cow_ctor(
    o: &mut crate::world_objects::world_object::WorldObject,
    env: &crate::world_objects::world_object::CtorEnv<'_>,
    src: crate::world_objects::world_object::CtorSource,
) {
    crate::world_objects::creature::creature_ctor(o, env, src);
    cow_set_ephemeral_values(o, env);
}

/// Empty in ACE.
// ACE: Cow.SetEphemeralValues
fn cow_set_ephemeral_values(
    _o: &mut crate::world_objects::world_object::WorldObject,
    _env: &crate::world_objects::world_object::CtorEnv<'_>,
) {
}
