// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/AdvocateFane.cs
//! Port of `Source/ACE.Server/WorldObjects/AdvocateFane.cs`: the fane that makes a player an
//! Advocate (sets `AdvocateQuest`).

use empyrean_entity::enums::{
    ChatMessageType, CombatMode, MotionCommand, MotionStance, PKLevel, PropertyString,
    WeenieErrorWithString, WeenieType,
};
use empyrean_entity::ObjectGuid;

use crate::entity::actions::action_chain::ActionChain;
use crate::entity::actions::i_actor::Actor;
use crate::entity::activation_result::ActivationResult;
use crate::entity::motion::Motion;
use crate::managers::player_manager::player_session;
use crate::network::game_event::events::game_event_weenie_error_with_string::game_event_weenie_error_with_string;
use crate::network::game_messages::game_message::{enqueue_send, GameMessage};
use crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use crate::world_objects::world_object::WorldObject;
use crate::world_objects::{
    creature_combat, player_inventory, player_use, world_object_equipment, world_object_networking,
};
use crate::World;

/// Non-property fields declared in `AdvocateFane.cs`.
#[derive(Debug, Default)]
pub struct AdvocateFaneFields {}

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

fn name(w: &World, g: ObjectGuid) -> String {
    crate::dispatch::name::name(w, g).unwrap_or_default()
}

fn send(w: &mut World, player: ObjectGuid, msg: GameMessage) {
    let session =
        player_session(w, player).expect("ACE: Player.Session is null (NullReferenceException)");
    enqueue_send(w, session, msg);
}

/// `new GameEventWeenieErrorWithString(player.Session, The_IsCurrentlyInUse, Name)`.
fn in_use(w: &mut World, this: ObjectGuid, player: ObjectGuid) -> GameMessage {
    let name = name(w, this);
    let session =
        player_session(w, player).expect("ACE: Player.Session is null (NullReferenceException)");
    let data = w.sessions.get_mut(session).expect("the player's session");
    game_event_weenie_error_with_string(data, WeenieErrorWithString::The_IsCurrentlyInUse, &name)
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

// ---- virtual-dispatch targets ----

// ACE: AdvocateFane.CheckUseRequirements
pub fn advocate_fane_check_use_requirements(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    activator: empyrean_entity::ObjectGuid,
) -> ActivationResult {
    if !w.objects.get(activator).is_some_and(WorldObject::is_player) {
        return ActivationResult::new(false);
    }
    let player = activator;

    if obj(w, player).wo.world_object.teleporting {
        return ActivationResult::new(false);
    }

    if obj(w, player).wo.world_object.is_busy {
        return ActivationResult::new(false);
    }

    if obj(w, this).wo.world_object.is_busy {
        return ActivationResult::with_message(in_use(w, this, player));
    }

    ActivationResult::new(true)
}

// ACE: AdvocateFane.ActOnUse
pub fn advocate_fane_act_on_use(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    activator: empyrean_entity::ObjectGuid,
) {
    if !w.objects.get(activator).is_some_and(WorldObject::is_player) {
        return;
    }
    let player = activator;

    if obj(w, this).wo.world_object.is_busy {
        let msg = in_use(w, this, player);
        send(w, player, msg);
        return;
    }

    obj_mut(w, this).wo.world_object.is_busy = true;
    obj_mut(w, player).wo.world_object.is_busy = true;

    let p = obj(w, player);
    // PlayerKillers, Admins and Sentinels can't be Advocates.
    if p.advocate_quest_player()
        || p.pk_level() != PKLevel::NPK
        || p.is_admin()
        || p.biota.weenie_type == WeenieType::Admin
        || p.is_sentinel()
        || p.biota.weenie_type == WeenieType::Sentinel
    {
        let mut action_chain = ActionChain::new();

        let use_target_failure_animation = obj(w, this).use_target_failure_animation();
        let fail_motion = if use_target_failure_animation != MotionCommand::Invalid {
            use_target_failure_animation
        } else {
            MotionCommand::Twitch2
        };
        let motion = Motion::from_world_object(w, this, fail_motion, 1.0);
        world_object_networking::enqueue_broadcast_motion(w, this, &motion, None, None);

        let use_time = motion_table_animation_length(w, this, fail_motion);

        player_use::fields_mut(w, player).last_use_time += use_time;

        action_chain.add_delay_seconds(w, f64::from(use_time));

        action_chain.add_action(Actor::Object(player), move |w: &mut World| {
            obj_mut(w, player).wo.world_object.is_busy = false;
            reset(w, this);
        });

        action_chain.enqueue_chain(w);

        return;
    }

    let mut fane_timer = ActionChain::new();

    if creature_combat::combat_mode(w, player) != CombatMode::NonCombat {
        let stance_time = creature_combat::set_combat_mode(w, player, CombatMode::NonCombat);
        fane_timer.add_delay_seconds(w, f64::from(stance_time));

        player_use::fields_mut(w, player).last_use_time += stance_time;
    }

    // ACE-BUG: the player's motion tests UseTargetSuccessAnimation but plays UseUserAnimation, so a fane with a success animation and no user animation plays MotionCommand.Invalid (0) on the player.
    let use_motion = if obj(w, this).use_target_success_animation() != MotionCommand::Invalid {
        obj(w, this).use_user_animation()
    } else {
        MotionCommand::BowDeep
    };
    let anim_time = world_object_networking::enqueue_motion(
        w,
        player,
        &mut fane_timer,
        use_motion,
        1.0,
        true,
        None,
        false,
        false,
    );
    player_use::fields_mut(w, player).last_use_time += anim_time;

    let use_target_success_animation = obj(w, this).use_target_success_animation();
    let success_motion = if use_target_success_animation != MotionCommand::Invalid {
        use_target_success_animation
    } else {
        MotionCommand::Twitch1
    };
    let success_time = world_object_networking::enqueue_motion(
        w,
        this,
        &mut fane_timer,
        success_motion,
        1.0,
        true,
        None,
        false,
        false,
    );
    player_use::fields_mut(w, player).last_use_time += success_time;

    fane_timer.add_action(Actor::Object(player), move |w: &mut World| {
        obj_mut(w, player).set_advocate_quest_player(true);
        let use_message = obj(w, this).get_property(PropertyString::UseMessage);
        let msg = game_message_system_chat(
            use_message.as_deref().unwrap_or_default(),
            ChatMessageType::Broadcast,
        );
        send(w, player, msg);

        if let Some(use_create_item) = obj(w, this).use_create_item() {
            if let Some(item) =
                world_object_equipment::create_new_world_object_by_wcid(w, use_create_item)
            {
                let item_guid = item.guid;
                w.objects.insert(item).unwrap_or_else(|_| {
                    panic!(
                        "a new dynamic guid 0x{:08X} is already live",
                        item_guid.full()
                    )
                });
                if player_inventory::try_create_in_inventory_with_networking(w, player, item_guid)
                    .is_none()
                {
                    // DIVERGE: ACE drops an item TryCreateInInventoryWithNetworking refused (the GC takes it); it leaves World.objects (4.5a's rule for unplaced new objects).
                    w.objects.remove(item_guid);
                }
            }
        }

        if crate::managers::property_manager::get_bool(w, "advocate_fane_auto_bestow", false, true)
            .item
        {
            let level: i32 = empyrean_common::dotnet::CsCast::cs_cast(
                crate::managers::property_manager::get_double(
                    w,
                    "advocate_fane_auto_bestow_level",
                    0.0,
                    true,
                )
                .item,
            );
            crate::entity::advocate::bestow(w, player, level);
        }
    });

    fane_timer.add_action(Actor::Object(player), move |w: &mut World| {
        obj_mut(w, player).wo.world_object.is_busy = false;
        reset(w, this);
    });

    fane_timer.enqueue_chain(w);
}

// ACE: AdvocateFane.Reset
pub fn reset(w: &mut World, this: ObjectGuid) {
    if let Some(o) = w.objects.get_mut(this) {
        o.wo.world_object.is_busy = false;
    }
}

// ---- constructors and SetEphemeralValues ----

/// `new AdvocateFane(weenie, guid)` / `new AdvocateFane(biota)`: the `WorldObject` constructor, then
/// AdvocateFane's `SetEphemeralValues`.
// ACE: AdvocateFane.AdvocateFane
pub fn advocate_fane_ctor(
    o: &mut crate::world_objects::world_object::WorldObject,
    env: &crate::world_objects::world_object::CtorEnv<'_>,
    src: crate::world_objects::world_object::CtorSource,
) {
    crate::world_objects::world_object::world_object_ctor(o, env, src);
    advocate_fane_set_ephemeral_values(o, env);
}

// ACE: AdvocateFane.SetEphemeralValues
fn advocate_fane_set_ephemeral_values(
    o: &mut crate::world_objects::world_object::WorldObject,
    _env: &crate::world_objects::world_object::CtorEnv<'_>,
) {
    o.wo.world_object_properties.current_motion_state =
        Some(Motion::from_stance(MotionStance::NonCombat));
}
