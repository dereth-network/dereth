// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Ammunition.cs
//! Port of `Source/ACE.Server/WorldObjects/Ammunition.cs`.

/// Non-property fields declared in `Ammunition.cs`.
#[derive(Debug, Default)]
pub struct AmmunitionFields {}

// ---- virtual-dispatch targets: each `not_ported!` until it is ported ----

// ACE: Ammunition.OnCollideObject
pub fn ammunition_on_collide_object(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    target: empyrean_entity::ObjectGuid,
) {
    crate::world_objects::projectile_collision_helper::on_collide_object(w, this, target);
}

// ACE: Ammunition.OnCollideEnvironment
pub fn ammunition_on_collide_environment(w: &mut crate::World, this: empyrean_entity::ObjectGuid) {
    crate::world_objects::projectile_collision_helper::on_collide_environment(w, this);
}

/// Do nothing.
// ACE: Ammunition.ActOnUse
pub fn ammunition_act_on_use(
    _w: &mut crate::World,
    _this: empyrean_entity::ObjectGuid,
    _activator: empyrean_entity::ObjectGuid,
) {
}

// ---- constructors and SetEphemeralValues ----

/// `new Ammunition(weenie, guid)` / `new Ammunition(biota)`: the `Stackable` constructor, then
/// Ammunition's `SetEphemeralValues`.
// ACE: Ammunition.Ammunition
pub fn ammunition_ctor(
    o: &mut crate::world_objects::world_object::WorldObject,
    env: &crate::world_objects::world_object::CtorEnv<'_>,
    src: crate::world_objects::world_object::CtorSource,
) {
    crate::world_objects::stackable::stackable_ctor(o, env, src);
    ammunition_set_ephemeral_values(o, env);
}

/// Empty in ACE.
// ACE: Ammunition.SetEphemeralValues
fn ammunition_set_ephemeral_values(
    _o: &mut crate::world_objects::world_object::WorldObject,
    _env: &crate::world_objects::world_object::CtorEnv<'_>,
) {
}
