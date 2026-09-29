// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Lifestone.cs
//! Port of `Source/ACE.Server/WorldObjects/Lifestone.cs`.

/// Non-property fields declared in `Lifestone.cs`.
#[derive(Debug, Default)]
pub struct LifestoneFields {}

// ---- virtual-dispatch targets ----

// ACE: Lifestone.ActOnUse
pub fn lifestone_act_on_use(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    activator: empyrean_entity::ObjectGuid,
) {
    use crate::entity::actions::action_chain::ActionChain;
    use crate::entity::actions::i_actor::Actor;
    use crate::world_objects::{creature_combat, player_use, world_object_networking};
    use empyrean_entity::enums::{
        ChatMessageType, CombatMode, MotionCommand, PropertyString, Sound, WeenieError,
    };

    if !w
        .objects
        .get(activator)
        .is_some_and(crate::world_objects::world_object::WorldObject::is_player)
    {
        return;
    }
    let player = activator;

    let mut action_chain = ActionChain::new();
    if creature_combat::combat_mode(w, player) != CombatMode::NonCombat {
        let stance_time = creature_combat::set_combat_mode(w, player, CombatMode::NonCombat);
        action_chain.add_delay_seconds(w, f64::from(stance_time));

        player_use::fields_mut(w, player).last_use_time += stance_time;
    }

    action_chain.add_action(Actor::Object(this), move |w: &mut crate::World| {
        let sound = crate::network::game_messages::messages::game_message_sound::game_message_sound(
            player,
            Sound::LifestoneOn,
            1.0,
        );
        world_object_networking::enqueue_broadcast(w, player, true, &[sound]);
    });

    let anim_time = world_object_networking::enqueue_motion(
        w,
        player,
        &mut action_chain,
        MotionCommand::Sanctuary,
        1.0,
        true,
        None,
        false,
        false,
    );
    player_use::fields_mut(w, player).last_use_time += anim_time;

    action_chain.add_action(Actor::Object(this), move |w: &mut crate::World| {
        if crate::world_objects::world_object_use::is_within_use_radius_of(w, player, this, None) {
            let location = w.objects.get(player).and_then(crate::world_objects::world_object::WorldObject::location);
            w.objects.get_mut(player).expect("ACE: player is null").set_sanctuary(location);
            let use_message = w.objects.get(this).and_then(|o| o.get_property(PropertyString::UseMessage));
            let msg = crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat(
                use_message.as_deref().unwrap_or_default(),
                ChatMessageType::Magic,
            );
            let session = crate::managers::player_manager::player_session(w, player).expect("ACE: player.Session is null (NullReferenceException)");
            crate::network::game_messages::game_message::enqueue_send(w, session, msg);
            let stamina = w.objects.get(player).expect("ACE: player is null").stamina();
            let current = stamina.current(w.objects.get(player).expect("ACE: player is null"));
            // `(uint)Math.Round(player.Stamina.Current / 2f)`
            #[allow(clippy::cast_precision_loss)]
            let half = current as f32 / 2.0;
            let new_stamina: u32 = empyrean_common::dotnet::CsCast::cs_cast(empyrean_common::dotnet::math::round(f64::from(half)));
            crate::world_objects::creature_vitals::creature_update_vital_uint(w, player, stamina, new_stamina);
        } else {
            crate::world_objects::player_networking::send_weenie_error(w, player, WeenieError::YouHaveMovedTooFar);
        }
    });

    action_chain.enqueue_chain(w);
}

// ---- constructors and SetEphemeralValues ----

/// `new Lifestone(weenie, guid)` / `new Lifestone(biota)`: the `WorldObject` constructor, then
/// Lifestone's `SetEphemeralValues`.
// ACE: Lifestone.Lifestone
pub fn lifestone_ctor(
    o: &mut crate::world_objects::world_object::WorldObject,
    env: &crate::world_objects::world_object::CtorEnv<'_>,
    src: crate::world_objects::world_object::CtorSource,
) {
    crate::world_objects::world_object::world_object_ctor(o, env, src);
    lifestone_set_ephemeral_values(o, env);
}

// ACE: Lifestone.SetEphemeralValues
fn lifestone_set_ephemeral_values(
    o: &mut crate::world_objects::world_object::WorldObject,
    _env: &crate::world_objects::world_object::CtorEnv<'_>,
) {
    o.wo.world_object.object_description_flags |=
        empyrean_entity::enums::ObjectDescriptionFlag::LifeStone;

    o.set_radar_color(Some(empyrean_entity::enums::RadarColor::LifeStone));
}
