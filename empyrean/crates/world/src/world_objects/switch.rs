// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Switch.cs
//! Port of `Source/ACE.Server/WorldObjects/Switch.cs`.

/// Non-property fields declared in `Switch.cs`.
#[derive(Debug, Default)]
pub struct SwitchFields {}

// ---- virtual-dispatch targets ----

// ACE: Switch.OnActivate
pub fn switch_on_activate(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    activator: empyrean_entity::ObjectGuid,
) {
    use crate::entity::actions::action_chain::ActionChain;
    use crate::entity::actions::i_actor::Actor;

    if !w
        .objects
        .get(activator)
        .is_some_and(crate::world_objects::world_object::WorldObject::is_creature)
    {
        return;
    }

    // move this to base?
    let sound = crate::network::game_messages::messages::game_message_sound::game_message_sound(
        activator,
        empyrean_entity::enums::Sound::TriggerActivated,
        1.0,
    );
    crate::world_objects::world_object_networking::enqueue_broadcast(w, this, true, &[sound]);

    let mut action_chain = ActionChain::new();

    let o = w.objects.get(this).expect("ACE: this is null");
    if o.motion_table_id() != 0 {
        let use_animation = o.use_target_animation().map_or(
            empyrean_entity::enums::MotionCommand::Twitch1,
            empyrean_entity::enums::MotionCommand,
        );

        crate::world_objects::world_object_networking::enqueue_motion(
            w,
            this,
            &mut action_chain,
            use_animation,
            1.0,
            false,
            None,
            false,
            false,
        );
    }

    let o = w.objects.get(this).expect("ACE: this is null");
    // `Time.GetUnixTime() < ResetTimestamp`: a null ResetTimestamp compares false
    if o.reset_timestamp().is_some_and(|t| w.now.unix_time < t) {
        let activation_failure =
            o.get_property(empyrean_entity::enums::PropertyString::ActivationFailure);
        if let Some(activation_failure) = activation_failure {
            if w.objects
                .get(activator)
                .is_some_and(crate::world_objects::world_object::WorldObject::is_player)
            {
                let player = activator;
                action_chain.add_action(Actor::Object(this), move |w: &mut crate::World| {
                    let msg = crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat(
                        &activation_failure,
                        empyrean_entity::enums::ChatMessageType::Broadcast,
                    );
                    let session = crate::managers::player_manager::player_session(w, player).expect("ACE: player.Session is null (NullReferenceException)");
                    crate::network::game_messages::game_message::enqueue_send(w, session, msg);
                });
            }
        }
    } else {
        action_chain.add_action(Actor::Object(this), move |w: &mut crate::World| {
            crate::world_objects::world_object_use::world_object_on_activate(w, this, activator);
        });

        action_chain.add_action(Actor::Object(this), move |w: &mut crate::World| {
            let Some(o) = w.objects.get(this) else { return };
            // `ResetInterval > 0`: a null ResetInterval compares false
            if o.reset_interval().is_some_and(|i| i > 0.0) {
                let reset_timestamp = w.now.unix_time + o.reset_interval().unwrap_or(0.0);
                w.objects
                    .get_mut(this)
                    .expect("ACE: this is null")
                    .set_reset_timestamp(Some(reset_timestamp));
            }
        });
    }

    action_chain.enqueue_chain(w);
}

// ACE: Switch.ActOnUse
pub fn switch_act_on_use(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    activator: empyrean_entity::ObjectGuid,
) {
    let o = w.objects.get(this).expect("ACE: this is null");
    if let Some(spell_did) = o.spell_did() {
        if o.activation_response()
            .contains(empyrean_entity::enums::ActivationResponse::CastSpell)
        {
            let spell = crate::entity::spell::Spell::new(w, spell_did, true);

            crate::world_objects::world_object_magic::try_cast_spell(
                w,
                this,
                &spell,
                Some(activator),
                None,
                None,
                false,
                false,
                true,
            );
        }
    }
}

// ACE: Switch.SetLinkProperties
pub fn switch_set_link_properties(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    wo: empyrean_entity::ObjectGuid,
) {
    w.objects
        .get_mut(wo)
        .expect("ACE: wo is null (NullReferenceException)")
        .set_activation_target(this.full());
}

// ---- constructors and SetEphemeralValues ----

/// `new Switch(weenie, guid)` / `new Switch(biota)`: the `WorldObject` constructor, then
/// Switch's `SetEphemeralValues`.
// ACE: Switch.Switch
pub fn switch_ctor(
    o: &mut crate::world_objects::world_object::WorldObject,
    env: &crate::world_objects::world_object::CtorEnv<'_>,
    src: crate::world_objects::world_object::CtorSource,
) {
    crate::world_objects::world_object::world_object_ctor(o, env, src);
    switch_set_ephemeral_values(o, env);
}

/// Empty in ACE.
// ACE: Switch.SetEphemeralValues
fn switch_set_ephemeral_values(
    _o: &mut crate::world_objects::world_object::WorldObject,
    _env: &crate::world_objects::world_object::CtorEnv<'_>,
) {
}
