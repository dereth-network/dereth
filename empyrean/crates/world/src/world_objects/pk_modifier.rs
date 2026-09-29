// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/PKModifier.cs
//! Port of `Source/ACE.Server/WorldObjects/PKModifier.cs`: the altars that turn a player PK or
//! NPK.

use empyrean_common::dotnet::CsCast;
use empyrean_entity::enums::{
    ChatMessageType, MotionCommand, MotionStance, PKLevel, PlayerKillerStatus, PropertyInt,
    PropertyString, WeenieError, WeenieErrorWithString,
};
use empyrean_entity::ObjectGuid;

use crate::entity::actions::action_chain::ActionChain;
use crate::entity::actions::i_actor::Actor;
use crate::entity::activation_result::ActivationResult;
use crate::entity::motion::Motion;
use crate::managers::player_manager::player_session;
use crate::managers::property_manager;
use crate::network::game_event::events::game_event_weenie_error::game_event_weenie_error;
use crate::network::game_event::events::game_event_weenie_error_with_string::game_event_weenie_error_with_string;
use crate::network::game_messages::game_message::{enqueue_send, GameMessage};
use crate::network::game_messages::messages::game_message_public_update_property_int::game_message_public_update_property_int;
use crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use crate::world_objects::world_object::WorldObject;
use crate::world_objects::{player_networking, player_use, world_object_networking};
use crate::World;

/// Non-property fields declared in `PKModifier.cs`.
#[derive(Debug, Default)]
pub struct PKModifierFields {}

fn obj(w: &World, g: ObjectGuid) -> &WorldObject {
    w.objects
        .get(g)
        .expect("ACE: object is null (NullReferenceException)")
}

fn obj_mut(w: &mut World, g: ObjectGuid) -> &mut WorldObject {
    w.objects
        .get_mut(g)
        .expect("ACE: object is null (NullReferenceException)")
}

fn send(w: &mut World, player: ObjectGuid, msg: GameMessage) {
    let session =
        player_session(w, player).expect("ACE: Player.Session is null (NullReferenceException)");
    enqueue_send(w, session, msg);
}

fn send_chat(w: &mut World, player: ObjectGuid, text: &str) {
    send(
        w,
        player,
        game_message_system_chat(text, ChatMessageType::Broadcast),
    );
}

fn session_data(w: &mut World, player: ObjectGuid) -> &mut crate::sessions::SessionData {
    let session =
        player_session(w, player).expect("ACE: Player.Session is null (NullReferenceException)");
    w.sessions.get_mut(session).expect("the player's session")
}

/// `new GameEventWeenieError(player.Session, error)`.
fn weenie_error(w: &mut World, player: ObjectGuid, error: WeenieError) -> GameMessage {
    game_event_weenie_error(session_data(w, player), error)
}

/// `DatManager.PortalDat.ReadFromDat<MotionTable>(MotionTableId).GetAnimationLength(motion)`.
fn motion_table_animation_length(w: &World, this: ObjectGuid, motion: MotionCommand) -> f32 {
    let id = obj(w, this).motion_table_id();
    let mt = w
        .dats
        .portal_dat()
        .read_from_dat::<empyrean_dat::file_types::MotionTable>(id);
    crate::physics::motion_table::get_animation_length_of(w, mt.as_deref(), motion)
}

/// `string.IsNullOrWhiteSpace(value)`.
fn is_null_or_white_space(value: Option<&str>) -> bool {
    value.is_none_or(|s| s.chars().all(char::is_whitespace))
}

impl WorldObject {
    // ACE: PKModifier.IsPKSwitch
    #[must_use]
    pub fn is_pk_switch(&self) -> bool {
        self.pk_level_modifier() == 1
    }

    // ACE: PKModifier.IsNPKSwitch
    #[must_use]
    pub fn is_npk_switch(&self) -> bool {
        self.pk_level_modifier() == -1
    }
}

// ---- virtual-dispatch targets ----

// ACE: PKModifier.CheckUseRequirements
pub fn pk_modifier_check_use_requirements(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    activator: empyrean_entity::ObjectGuid,
) -> ActivationResult {
    if !w.objects.get(activator).is_some_and(WorldObject::is_player) {
        return ActivationResult::new(false);
    }
    let player = activator;

    if crate::entity::damage_history_info::player_is_olthoi_player(w, player) {
        player_networking::send_weenie_error(w, player, WeenieError::OlthoiCannotInteractWithThat);
        return ActivationResult::new(false);
    }

    if obj(w, player).pk_level() > PKLevel::PK
        || property_manager::get_bool(w, "pk_server", false, true).item
        || property_manager::get_bool(w, "pkl_server", false, true).item
    {
        let use_pk_server_error = obj(w, this).get_property(PropertyString::UsePkServerError);
        if !is_null_or_white_space(use_pk_server_error.as_deref()) {
            send_chat(
                w,
                player,
                use_pk_server_error.as_deref().unwrap_or_default(),
            );
        }

        return ActivationResult::new(false);
    }

    if obj(w, player).player_killer_status() == PlayerKillerStatus::PKLite {
        let use_pk_server_error = obj(w, this).get_property(PropertyString::UsePkServerError);
        if !is_null_or_white_space(use_pk_server_error.as_deref()) {
            send_chat(
                w,
                player,
                use_pk_server_error.as_deref().unwrap_or_default(),
            );
        }

        send_chat(
            w,
            player,
            "Player Killer Lites may not change their PK status.",
        ); // not sure how retail handled this case

        return ActivationResult::new(false);
    }

    if obj(w, player).wo.world_object.teleporting {
        return ActivationResult::new(false);
    }

    if obj(w, player).wo.world_object.is_busy {
        return ActivationResult::new(false);
    }

    let p = obj(w, player);
    if p.is_advocate() || p.advocate_quest_player() || p.advocate_state() {
        return ActivationResult::with_message(weenie_error(
            w,
            player,
            WeenieError::AdvocatesCannotChangePKStatus,
        ));
    }

    if obj(w, player).minimum_time_since_pk().is_some() {
        return ActivationResult::with_message(weenie_error(
            w,
            player,
            WeenieError::CannotChangePKStatusWhileRecovering,
        ));
    }

    if obj(w, this).wo.world_object.is_busy {
        let name = crate::dispatch::name::name(w, this).unwrap_or_default();
        let msg = game_event_weenie_error_with_string(
            session_data(w, player),
            WeenieErrorWithString::The_IsCurrentlyInUse,
            &name,
        );
        return ActivationResult::with_message(msg);
    }

    ActivationResult::new(true)
}

// ACE: PKModifier.ActOnUse
pub fn pk_modifier_act_on_use(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    activator: empyrean_entity::ObjectGuid,
) {
    if !w.objects.get(activator).is_some_and(WorldObject::is_player) {
        return;
    }
    let player = activator;

    if obj(w, this).wo.world_object.is_busy {
        let name = crate::dispatch::name::name(w, this).unwrap_or_default();
        let msg = game_event_weenie_error_with_string(
            session_data(w, player),
            WeenieErrorWithString::The_IsCurrentlyInUse,
            &name,
        );
        send(w, player, msg);
        return;
    }

    // `(Time.GetUnixTime() - player.PkTimestamp) < MinimumTimeSincePk`: this altar's MinimumTimeSincePk
    // (a null one compares false)
    let since_pk = w.now.unix_time - obj(w, player).pk_timestamp();
    let too_soon = obj(w, this)
        .minimum_time_since_pk()
        .is_some_and(|m| since_pk < m);
    if obj(w, player).pk_level() == PKLevel::PK && obj(w, this).is_npk_switch() && too_soon {
        obj_mut(w, this).wo.world_object.is_busy = true;
        obj_mut(w, player).wo.world_object.is_busy = true;

        let mut action_chain = ActionChain::new();

        let use_target_failure_animation = obj(w, this).use_target_failure_animation();
        if use_target_failure_animation != MotionCommand::Invalid {
            let use_motion = use_target_failure_animation;
            let motion = Motion::from_world_object(w, this, use_motion, 1.0);
            world_object_networking::enqueue_broadcast_motion(w, this, &motion, None, None);

            let use_time = motion_table_animation_length(w, this, use_motion);

            player_use::fields_mut(w, player).last_use_time += use_time;

            action_chain.add_delay_seconds(w, f64::from(use_time));
        }

        action_chain.add_action(Actor::Object(player), move |w: &mut World| {
            let msg = weenie_error(w, player, WeenieError::YouFeelAHarshDissonance);
            send(w, player, msg);
            obj_mut(w, player).wo.world_object.is_busy = false;
            reset(w, this);
        });

        action_chain.enqueue_chain(w);

        return;
    }

    let player_pk_level = obj(w, player).pk_level();
    if (player_pk_level == PKLevel::NPK && obj(w, this).is_pk_switch())
        || (player_pk_level == PKLevel::PK && obj(w, this).is_npk_switch())
    {
        obj_mut(w, this).wo.world_object.is_busy = true;
        obj_mut(w, player).wo.world_object.is_busy = true;

        let use_target_success_animation = obj(w, this).use_target_success_animation();
        let use_motion = if use_target_success_animation != MotionCommand::Invalid {
            use_target_success_animation
        } else {
            MotionCommand::Twitch1
        };
        let motion = Motion::from_world_object(w, this, use_motion, 1.0);
        world_object_networking::enqueue_broadcast_motion(w, this, &motion, None, None);

        let use_time = motion_table_animation_length(w, this, use_motion);

        player_use::fields_mut(w, player).last_use_time += use_time;

        let mut action_chain = ActionChain::new();

        action_chain.add_delay_seconds(w, f64::from(use_time));

        action_chain.add_action(Actor::Object(player), move |w: &mut World| {
            let use_message = obj(w, this).get_property(PropertyString::UseMessage);
            send_chat(w, player, use_message.as_deref().unwrap_or_default());
            let modifier = obj(w, this).pk_level_modifier();
            let p = obj_mut(w, player);
            p.set_pk_level_modifier(p.pk_level_modifier().wrapping_add(modifier));

            if p.pk_level() == PKLevel::PK {
                p.set_player_killer_status_prop(PlayerKillerStatus::PK);
            } else {
                p.set_player_killer_status_prop(PlayerKillerStatus::NPK);
            }

            let status: i32 = p.player_killer_status().0.cs_cast();
            let msg =
                game_message_public_update_property_int(p, PropertyInt::PlayerKillerStatus, status);
            world_object_networking::enqueue_broadcast(w, player, true, &[msg]);
            //player.ApplySoundEffects(Sound.Open); // in pcaps, but makes no sound/has no effect. ?
            obj_mut(w, player).wo.world_object.is_busy = false;
            reset(w, this);
        });

        action_chain.enqueue_chain(w);
    } else {
        let activation_failure = obj(w, this).get_property(PropertyString::ActivationFailure);
        send_chat(w, player, activation_failure.as_deref().unwrap_or_default());
    }
}

// ACE: PKModifier.Reset
pub fn reset(w: &mut World, this: ObjectGuid) {
    if let Some(o) = w.objects.get_mut(this) {
        o.wo.world_object.is_busy = false;
    }
}

// ---- constructors and SetEphemeralValues ----

/// `new PKModifier(weenie, guid)` / `new PKModifier(biota)`: the `WorldObject` constructor, then
/// PKModifier's `SetEphemeralValues`.
// ACE: PKModifier.PKModifier
pub fn pk_modifier_ctor(
    o: &mut crate::world_objects::world_object::WorldObject,
    env: &crate::world_objects::world_object::CtorEnv<'_>,
    src: crate::world_objects::world_object::CtorSource,
) {
    crate::world_objects::world_object::world_object_ctor(o, env, src);
    pk_modifier_set_ephemeral_values(o, env);
}

// ACE: PKModifier.SetEphemeralValues
fn pk_modifier_set_ephemeral_values(
    o: &mut crate::world_objects::world_object::WorldObject,
    _env: &crate::world_objects::world_object::CtorEnv<'_>,
) {
    o.wo.world_object_properties.current_motion_state =
        Some(Motion::from_stance(MotionStance::NonCombat));

    if o.is_npk_switch() {
        o.wo.world_object.object_description_flags |=
            empyrean_entity::enums::ObjectDescriptionFlag::NpkSwitch;
    }

    if o.is_pk_switch() {
        o.wo.world_object.object_description_flags |=
            empyrean_entity::enums::ObjectDescriptionFlag::PkSwitch;
    }
}
