// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/GenericObject.cs
//! Port of `Source/ACE.Server/WorldObjects/GenericObject.cs`.

/// Non-property fields declared in `GenericObject.cs`.
#[derive(Debug, Default)]
pub struct GenericObjectFields {}

// ---- virtual-dispatch targets ----

// ACE: GenericObject.ActOnUse
pub fn generic_object_act_on_use(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    activator: empyrean_entity::ObjectGuid,
) {
    if !w
        .objects
        .get(activator)
        .is_some_and(crate::world_objects::world_object::WorldObject::is_player)
    {
        return;
    }
    let player = activator;

    let use_sound = w.objects.get(this).expect("ACE: this is null").use_sound();
    if use_sound.0 > 0 {
        let msg = crate::network::game_messages::messages::game_message_sound::game_message_sound(
            player, use_sound, 1.0,
        );
        let session = crate::managers::player_manager::player_session(w, player)
            .expect("ACE: player.Session is null (NullReferenceException)");
        crate::network::game_messages::game_message::enqueue_send(w, session, msg);
    }
}

// ---- constructors and SetEphemeralValues ----

/// `new GenericObject(weenie, guid)` / `new GenericObject(biota)`: the `WorldObject` constructor, then
/// GenericObject's `SetEphemeralValues`.
// ACE: GenericObject.GenericObject
pub fn generic_object_ctor(
    o: &mut crate::world_objects::world_object::WorldObject,
    env: &crate::world_objects::world_object::CtorEnv<'_>,
    src: crate::world_objects::world_object::CtorSource,
) {
    crate::world_objects::world_object::world_object_ctor(o, env, src);
    generic_object_set_ephemeral_values(o, env);
}

// ACE: GenericObject.SetEphemeralValues
fn generic_object_set_ephemeral_values(
    o: &mut crate::world_objects::world_object::WorldObject,
    _env: &crate::world_objects::world_object::CtorEnv<'_>,
) {
    //StackSize = null;
    //StackUnitEncumbrance = null;
    //StackUnitValue = null;
    //MaxStackSize = null;

    // Linkable Item Generator (linkitemgen2minutes) fix
    if o.biota.weenie_class_id == 4142 {
        o.set_max_generated_objects(0);
        o.set_init_generated_objects(0);
    }
}
