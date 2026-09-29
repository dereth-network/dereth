// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Bindstone.cs
//! Port of `Source/ACE.Server/WorldObjects/Bindstone.cs`.
//!
//! An allegiance bindstone: a Seneschal or better sets the allegiance's `Sanctuary` (its hometown
//! recall) where they stand.

use empyrean_entity::enums::{
    ChatMessageType, CombatMode, MotionCommand, MotionStance, PropertyString, WeenieError,
};

use crate::entity::actions::action_chain::ActionChain;
use crate::entity::actions::i_actor::Actor;
use crate::entity::i_player::IPlayer;
use crate::managers::player_manager::player_session;
use crate::network::game_messages::game_message::enqueue_send;
use crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use crate::network::motion::movement_data::Motion;
use crate::world_objects::world_object::WorldObject;
use crate::world_objects::{
    creature_combat, player_allegiance, player_networking, player_use, world_object_networking,
    world_object_use,
};
use crate::World;

/// Non-property fields declared in `Bindstone.cs`.
#[derive(Debug, Default)]
pub struct BindstoneFields {}

// ---- virtual-dispatch targets ----

/// This is raised by Player.HandleActionUseItem.
/// The item does not exist in the players possession.
/// If the item was outside of range, the player will have been commanded to move using DoMoveTo before ActOnUse is called.
/// When this is called, it should be assumed that the player is within range.
// ACE: Bindstone.ActOnUse
pub fn bindstone_act_on_use(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    activator: empyrean_entity::ObjectGuid,
) {
    if !w.objects.get(activator).is_some_and(WorldObject::is_player) {
        return;
    }
    let player = activator;

    // check if player is in an allegiance
    if !player_allegiance::has_allegiance(w, player) {
        player_networking::send_weenie_error(w, player, WeenieError::YouAreNotInAllegiance);
        return;
    }

    if player_allegiance::allegiance_permission_level(w, player)
        < empyrean_entity::enums::AllegiancePermissionLevel::Seneschal
    {
        player_networking::send_weenie_error(
            w,
            player,
            WeenieError::YouDoNotHaveAuthorityInAllegiance,
        );
        return;
    }

    let mut action_chain = ActionChain::new();
    if creature_combat::combat_mode(w, player) != CombatMode::NonCombat {
        let stance_time = creature_combat::set_combat_mode(w, player, CombatMode::NonCombat);
        action_chain.add_delay_seconds(w, f64::from(stance_time));

        player_use::fields_mut(w, player).last_use_time += stance_time;
    }

    action_chain.add_action(Actor::Object(this), move |w: &mut World| {
        world_object_networking::enqueue_broadcast_motion(
            w,
            this,
            &Motion::new(MotionStance::NonCombat, MotionCommand::Twitch1, 1.0),
            None,
            None,
        );
    });

    // player animation?
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

    action_chain.add_action(Actor::Object(this), move |w: &mut World| {
        if world_object_use::is_within_use_radius_of(w, player, this, None) {
            let allegiance = player_allegiance::i_player_allegiance(w, IPlayer::Online(player))
                .expect("ACE: player.Allegiance is null (NullReferenceException)");
            let location = w.objects.get(player).and_then(WorldObject::location);
            w.objects
                .get_mut(allegiance)
                .expect("ACE: player.Allegiance is null (NullReferenceException)")
                .set_sanctuary(location);
            crate::dispatch::save_biota_to_database::save_biota_to_database(w, allegiance, true);

            let use_message = w
                .objects
                .get(this)
                .and_then(|o| o.get_property(PropertyString::UseMessage));
            let m = game_message_system_chat(
                use_message.as_deref().unwrap_or_default(),
                ChatMessageType::Magic,
            );
            let session = player_session(w, player)
                .expect("ACE: player.Session is null (NullReferenceException)");
            enqueue_send(w, session, m);
        } else {
            player_networking::send_weenie_error(w, player, WeenieError::YouHaveMovedTooFar);
        }
    });

    action_chain.enqueue_chain(w);
}

// ---- constructors and SetEphemeralValues ----

/// `new Bindstone(weenie, guid)` / `new Bindstone(biota)`: the `WorldObject` constructor, then
/// Bindstone's `SetEphemeralValues`.
// ACE: Bindstone.Bindstone
pub fn bindstone_ctor(
    o: &mut crate::world_objects::world_object::WorldObject,
    env: &crate::world_objects::world_object::CtorEnv<'_>,
    src: crate::world_objects::world_object::CtorSource,
) {
    crate::world_objects::world_object::world_object_ctor(o, env, src);
    bindstone_set_ephemeral_values(o, env);
}

// ACE: Bindstone.SetEphemeralValues
fn bindstone_set_ephemeral_values(
    o: &mut crate::world_objects::world_object::WorldObject,
    _env: &crate::world_objects::world_object::CtorEnv<'_>,
) {
    o.wo.world_object.object_description_flags |=
        empyrean_entity::enums::ObjectDescriptionFlag::BindStone;

    o.set_property(
        empyrean_entity::enums::PropertyInt::ShowableOnRadar,
        i32::from(empyrean_entity::enums::RadarBehavior::ShowAlways.0),
    );
    o.set_property(
        empyrean_entity::enums::PropertyInt::RadarBlipColor,
        i32::from(empyrean_entity::enums::RadarColor::LifeStone.0),
    );
}
