// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Lockpick.cs
//! Port of `Source/ACE.Server/WorldObjects/Lockpick.cs`.

/// Non-property fields declared in `Lockpick.cs`.
#[derive(Debug, Default)]
pub struct LockpickFields {}

// ---- virtual-dispatch targets: each `not_ported!` until it is ported ----

/// An Olthoi cannot pick locks; anyone else uses the lockpick through `UnlockerHelper`.
// ACE: Lockpick.HandleActionUseOnTarget
pub fn lockpick_handle_action_use_on_target(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    player: empyrean_entity::ObjectGuid,
    target: empyrean_entity::ObjectGuid,
) {
    let is_olthoi_player = w
        .objects
        .get(player)
        .and_then(|o| o.player.as_ref())
        .is_some_and(|p| p.player_properties.is_olthoi_player);
    if is_olthoi_player {
        crate::world_objects::player_use::send_use_done_event(
            w,
            player,
            empyrean_entity::enums::WeenieError::OlthoiCannotInteractWithThat,
        );
        return;
    }

    crate::world_objects::lock::use_unlocker(w, player, this, target);
}

// ---- constructors and SetEphemeralValues ----

/// `new Lockpick(weenie, guid)` / `new Lockpick(biota)`: the `WorldObject` constructor, then
/// Lockpick's `SetEphemeralValues`.
// ACE: Lockpick.Lockpick
pub fn lockpick_ctor(
    o: &mut crate::world_objects::world_object::WorldObject,
    env: &crate::world_objects::world_object::CtorEnv<'_>,
    src: crate::world_objects::world_object::CtorSource,
) {
    crate::world_objects::world_object::world_object_ctor(o, env, src);
    lockpick_set_ephemeral_values(o, env);
}

// ACE: Lockpick.SetEphemeralValues
fn lockpick_set_ephemeral_values(
    o: &mut crate::world_objects::world_object::WorldObject,
    _env: &crate::world_objects::world_object::CtorEnv<'_>,
) {
    o.wo.world_object.object_description_flags |=
        empyrean_entity::enums::ObjectDescriptionFlag::Lockpick;
}
