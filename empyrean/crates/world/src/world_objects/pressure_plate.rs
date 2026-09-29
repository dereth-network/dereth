// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/PressurePlate.cs
//! Port of `Source/ACE.Server/WorldObjects/PressurePlate.cs`.

/// Non-property fields declared in `PressurePlate.cs`.
#[derive(Debug, Default)]
pub struct PressurePlateFields {
    // ACE: PressurePlate.LastUseTime
    /// The last time this pressure plate was activated (`DateTime`, default `MinValue`).
    pub last_use_time: empyrean_common::dotnet::datetime::DotNetDateTime,
}

fn fields_mut(w: &mut crate::World, this: empyrean_entity::ObjectGuid) -> &mut PressurePlateFields {
    match &mut w.objects.get_mut(this).expect("ACE: this is null").kind {
        crate::world_objects::kinds::KindData::PressurePlate(d) => &mut d.pressure_plate,
        _ => panic!("InvalidCastException: {this:?} is not a PressurePlate"),
    }
}

// ---- virtual-dispatch targets: each `not_ported!` until it is ported ----

// ACE: PressurePlate.SetLinkProperties
pub fn pressure_plate_set_link_properties(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    wo: empyrean_entity::ObjectGuid,
) {
    w.objects
        .get_mut(wo)
        .expect("ACE: wo is null (NullReferenceException)")
        .set_activation_target(this.full());
}

/// Called when a player runs over the pressure plate.
// ACE: PressurePlate.OnCollideObject
pub fn pressure_plate_on_collide_object(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    target: empyrean_entity::ObjectGuid,
) {
    crate::dispatch::on_activate::on_activate(w, this, target);
}

/// Activates the object linked to a pressure plate.
// ACE: PressurePlate.OnActivate
pub fn pressure_plate_on_activate(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    activator: empyrean_entity::ObjectGuid,
) {
    // handle monsters walking on pressure plates
    if !w
        .objects
        .get(activator)
        .is_some_and(crate::world_objects::world_object::WorldObject::is_player)
    {
        return;
    }
    let player = activator;

    // prevent continuous event stream
    // TODO: should this go in base.OnActivate()?

    let current_time = w.now.utc;
    if current_time < fields_mut(w, this).last_use_time.add_seconds(2.0) {
        return;
    }

    fields_mut(w, this).last_use_time = current_time;

    let use_sound = w.objects.get(this).expect("ACE: this is null").use_sound();
    let sound = crate::network::game_messages::messages::game_message_sound::game_message_sound(
        player, use_sound, 1.0,
    );
    crate::world_objects::world_object_networking::enqueue_broadcast(w, player, true, &[sound]);

    let o = w.objects.get(this).expect("ACE: this is null");
    // `Time.GetUnixTime() < ResetTimestamp`: a null ResetTimestamp compares false
    if o.reset_timestamp().is_some_and(|t| w.now.unix_time < t) {
        if let Some(activation_failure) =
            o.get_property(empyrean_entity::enums::PropertyString::ActivationFailure)
        {
            let msg = crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat(
                &activation_failure,
                empyrean_entity::enums::ChatMessageType::Broadcast,
            );
            let session = crate::managers::player_manager::player_session(w, player)
                .expect("ACE: player.Session is null (NullReferenceException)");
            crate::network::game_messages::game_message::enqueue_send(w, session, msg);
        }
    } else {
        crate::world_objects::world_object_use::world_object_on_activate(w, this, activator);

        let Some(o) = w.objects.get(this) else { return };
        // `ResetInterval > 0`: a null ResetInterval compares false
        if o.reset_interval().is_some_and(|i| i > 0.0) {
            let reset_timestamp = w.now.unix_time + o.reset_interval().unwrap_or(0.0);
            w.objects
                .get_mut(this)
                .expect("ACE: this is null")
                .set_reset_timestamp(Some(reset_timestamp));
        }
    }
}

/// Do nothing.
// ACE: PressurePlate.ActOnUse
pub fn pressure_plate_act_on_use(
    _w: &mut crate::World,
    _this: empyrean_entity::ObjectGuid,
    _activator: empyrean_entity::ObjectGuid,
) {
}

// ---- constructors and SetEphemeralValues ----

/// `new PressurePlate(weenie, guid)` / `new PressurePlate(biota)`: the `WorldObject` constructor, then
/// PressurePlate's `SetEphemeralValues`.
// ACE: PressurePlate.PressurePlate
pub fn pressure_plate_ctor(
    o: &mut crate::world_objects::world_object::WorldObject,
    env: &crate::world_objects::world_object::CtorEnv<'_>,
    src: crate::world_objects::world_object::CtorSource,
) {
    crate::world_objects::world_object::world_object_ctor(o, env, src);
    pressure_plate_set_ephemeral_values(o, env);
}

// ACE: PressurePlate.SetEphemeralValues
fn pressure_plate_set_ephemeral_values(
    o: &mut crate::world_objects::world_object::WorldObject,
    _env: &crate::world_objects::world_object::CtorEnv<'_>,
) {
    if o.use_sound() == empyrean_entity::enums::Sound::Invalid {
        o.set_use_sound(empyrean_entity::enums::Sound::TriggerActivated);
    }
}
