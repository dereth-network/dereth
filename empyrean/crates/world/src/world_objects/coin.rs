// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Coin.cs
//! Port of `Source/ACE.Server/WorldObjects/Coin.cs`.

/// Non-property fields declared in `Coin.cs`.
#[derive(Debug, Default)]
pub struct CoinFields {}

// ---- constructors and SetEphemeralValues ----

/// `new Coin(weenie, guid)` / `new Coin(biota)`: the `Stackable` constructor, then
/// Coin's `SetEphemeralValues`.
// ACE: Coin.Coin
pub fn coin_ctor(
    o: &mut crate::world_objects::world_object::WorldObject,
    env: &crate::world_objects::world_object::CtorEnv<'_>,
    src: crate::world_objects::world_object::CtorSource,
) {
    crate::world_objects::stackable::stackable_ctor(o, env, src);
    coin_set_ephemeral_values(o, env);
}

/// Empty in ACE.
// ACE: Coin.SetEphemeralValues
fn coin_set_ephemeral_values(
    _o: &mut crate::world_objects::world_object::WorldObject,
    _env: &crate::world_objects::world_object::CtorEnv<'_>,
) {
}
