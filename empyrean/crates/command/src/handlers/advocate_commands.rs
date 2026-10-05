// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Command/Handlers/AdvocateCommands.cs
//! Port of `Source/ACE.Server/Command/Handlers/AdvocateCommands.cs`.
//!
//! Callees that are not ported yet are private functions at the bottom of this file.

use empyrean_common::dotnet::{format, to_string};
use empyrean_entity::enums::{AccessLevel, ChatMessageType, PropertyBool};
use empyrean_entity::{ObjectGuid, Position};
use empyrean_net::SessionId;
use empyrean_world::entity::advocate;
use empyrean_world::entity::i_player::{self, IPlayer};
use empyrean_world::managers::player_manager;
use empyrean_world::network::game_messages::game_message::enqueue_send;
use empyrean_world::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use empyrean_world::physics::phys_ext;
use empyrean_world::world_objects::{player_combat, player_location};
use empyrean_world::World;

use crate::command_handler_attribute::CommandHandlerAttribute;
use crate::command_handler_flag::CommandHandlerFlag;
use crate::command_handler_info::{CommandHandlerInfo, NamedHandler};
use crate::command_parameter_helpers::{
    self as cph, ACECommandParameter, ACECommandParameterType, AceParamValue,
};
use crate::handler;
use crate::handler_common::session_player;
use crate::handlers::command_handler_helper::send_server_message;

/// This file's `[CommandHandler]` decorations, in declaration order.
#[must_use]
pub fn command_handlers() -> Vec<CommandHandlerInfo> {
    let a = AccessLevel::Advocate;
    let world = CommandHandlerFlag::RequiresWorld;
    let rows: [(CommandHandlerAttribute, NamedHandler); 4] = [
        (
            CommandHandlerAttribute::with_count(
                "attackable",
                a,
                world,
                1,
                "Sets whether monsters will attack you or not.",
                concat!(
                    "[ on | off ]\n",
                    "This command sets whether monsters will attack you unprovoked.\n When turned on, monsters will attack you as if you are a normal player.\n When turned off, monsters will ignore you.",
                ),
            ),
            handler!(handle_attackable),
        ),
        (
            CommandHandlerAttribute::with_count("bestow", a, world, 2, "Sets a character's Advocate Level.", "<name> <level>\nAdvocates can bestow any level less than their own."),
            handler!(handle_bestow),
        ),
        (
            CommandHandlerAttribute::with_count(
                "remove",
                a,
                world,
                1,
                "Removes the specified character from the Advocate ranks.",
                "<character name>\nAdvocates can remove Advocate status for any Advocate of lower level than their own.",
            ),
            handler!(handle_remove),
        ),
        (
            CommandHandlerAttribute::with_count(
                "tele",
                a,
                world,
                1,
                "Teleports you(or a player) to some location.",
                concat!(
                    "[name] <longitude> <latitude>\nExample: /tele 0n0w\nExample: /tele plats4days 37s,67w\n",
                    "This command teleports yourself (or the specified character) to the given longitude and latitude.",
                ),
            ),
            handler!(handle_tele),
        ),
    ];
    rows.into_iter()
        .map(|(attribute, (handler, handler_name))| CommandHandlerInfo {
            handler,
            handler_name,
            attribute,
        })
        .collect()
}

fn require(session: Option<SessionId>) -> SessionId {
    session.expect("NullReferenceException: session")
}

fn system_chat(w: &mut World, session: SessionId, message: &str) {
    enqueue_send(
        w,
        session,
        game_message_system_chat(message, ChatMessageType::Broadcast),
    );
}

fn advocate_level_of(w: &World, player: ObjectGuid) -> Option<i32> {
    w.objects.get(player).and_then(|o| o.advocate_level())
}

/// `session.Player.IsAdvocate && session.Player.AdvocateLevel < 5` (a null level compares false).
fn is_low_advocate(w: &World, player: ObjectGuid) -> bool {
    w.objects.get(player).is_some_and(|o| o.is_advocate())
        && advocate_level_of(w, player).is_some_and(|l| l < 5)
}

// ACE: AdvocateCommands.HandleAttackable
/// `attackable [ on | off ]`: sets whether monsters will attack you or not.
pub fn handle_attackable(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    // usage: @attackable { on,off}
    // This command sets whether monsters will attack you unprovoked.When turned on, monsters will attack you as if you are a normal player.  When turned off, monsters will ignore you.
    // @attackable - Sets whether monsters will attack you or not.

    let s = require(session);
    let me = session_player(w, s);
    if is_low_advocate(w, me) {
        return;
    }

    let param = parameters[0].as_str();

    if param == "off" {
        empyrean_world::world_objects::player_properties::update_property_bool(
            w,
            me,
            me,
            PropertyBool::Attackable,
            Some(false),
            true,
        );
        system_chat(
            w,
            s,
            "Monsters will only attack you if provoked by you first.",
        );
    } else {
        // case "on": default:
        empyrean_world::world_objects::player_properties::update_property_bool(
            w,
            me,
            me,
            PropertyBool::Attackable,
            Some(true),
            true,
        );
        system_chat(w, s, "Monsters will attack you normally.");
    }
}

/// `HandleBestow`'s parameter parsing: `(level, advocateLevel, advocateName)`, where
/// `advocateLevel` is `None` when `int.TryParse` fails or the level is outside 1..=7 (and
/// `advocateName` is then not used).
#[must_use]
pub fn parse_bestow(parameters: &[String]) -> (String, Option<i32>, String) {
    let char_name = parameters.join(" ").trim().to_owned();

    let level = parameters[parameters.len() - 1].clone();

    let advocate_level = cph::dotnet_parse::int_try_parse(&level).filter(|l| (1..=7).contains(l));

    // Not ACE's (a fix): the name is the line less its last word (the
    // level) and the spaces before it; ACE trimmed every trailing space and every trailing
    // character found in the level's digits, so "Bob 11 1" became "Bob".
    let advocate_name = char_name
        .strip_suffix(level.trim())
        .unwrap_or(&char_name)
        .trim_end()
        .to_owned();

    (level, advocate_level, advocate_name)
}

// ACE: AdvocateCommands.HandleBestow
/// `bestow <name> <level>`: sets a character's Advocate Level. Advocates can bestow any level
/// less than their own.
pub fn handle_bestow(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let s = require(session);
    let me = session_player(w, s);
    let (level, advocate_level, advocate_name) = parse_bestow(parameters);

    let Some(advocate_level) = advocate_level else {
        system_chat(w, s, &format!("{level} is not a valid advocate level."));
        return;
    };

    let (player_to_find, _) = player_manager::find_by_name(w, &advocate_name);

    let Some(player_to_find) = player_to_find else {
        system_chat(
            w,
            s,
            &format!("{advocate_name} was not found in the database."),
        );
        return;
    };
    let name = i_player::name(w, player_to_find).unwrap_or_default();

    let IPlayer::Online(player) = player_to_find else {
        system_chat(
            w,
            s,
            &format!("{name} is not online. Cannot complete bestowal process."),
        );
        return;
    };

    //if (!Advocate.IsAdvocate(player))
    //{
    //    session.Network.EnqueueSend(new GameMessageSystemChat($"{playerToFind.Name} is not an Advocate.", ChatMessageType.Broadcast));
    //    return;
    //}

    if player_combat::is_pk(w, player) || player_manager::property_manager_get_bool(w, "pk_server")
    {
        system_chat(
            w,
            s,
            &format!("{name} in a Player Killer and cannot be an Advocate."),
        );
        return;
    }

    let (my_level, their_level) = (advocate_level_of(w, me), advocate_level_of(w, player));
    if matches!((my_level, their_level), (Some(m), Some(t)) if m <= t) {
        system_chat(w, s, &format!("You cannot change {name}'s Advocate status because they are equal to or out rank you."));
        return;
    }

    let is_admin = w.objects.get(me).is_some_and(|o| o.is_admin_prop());
    if my_level.is_some_and(|m| advocate_level >= m) && !is_admin {
        system_chat(w, s, &format!("You cannot bestow {name}'s Advocate rank to {advocate_level} because that is equal to or higher than your rank."));
        return;
    }

    if their_level == Some(advocate_level) {
        system_chat(
            w,
            s,
            &format!("{name}'s Advocate rank is already at level {advocate_level}."),
        );
        return;
    }

    if !advocate::can_accept_advocate_items(w, player, advocate_level) {
        system_chat(w, s, &format!("You cannot change {name}'s Advocate status because they do not have capacity for the advocate items."));
        return;
    }

    if advocate::bestow(w, player, advocate_level) {
        system_chat(
            w,
            s,
            &format!("{name} is now an Advocate, level {advocate_level}."),
        );
    } else {
        system_chat(w, s, &format!("Advocate bestowal of {name} failed."));
    }
}

// ACE: AdvocateCommands.HandleRemove
/// `remove <character name>`: removes the specified character from the Advocate ranks.
pub fn handle_remove(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let s = require(session);
    let me = session_player(w, s);
    let char_name = parameters.join(" ").trim().to_owned();

    let (player_to_find, _) = player_manager::find_by_name(w, &char_name);

    let Some(player_to_find) = player_to_find else {
        system_chat(w, s, &format!("{char_name} was not found in the database."));
        return;
    };
    let name = i_player::name(w, player_to_find).unwrap_or_default();

    let IPlayer::Online(player) = player_to_find else {
        system_chat(
            w,
            s,
            &format!("{name} is not online. Cannot complete removal process."),
        );
        return;
    };

    if !advocate::is_advocate(w, player) {
        system_chat(w, s, &format!("{name} is not an Advocate."));
        return;
    }

    // An advocate of equal or higher level cannot be removed.
    if matches!((advocate_level_of(w, me), advocate_level_of(w, player)), (Some(m), Some(t)) if m <= t)
    {
        system_chat(w, s, &format!("You cannot remove {name}'s Advocate status because they are equal to or out rank you."));
        return;
    }

    if advocate::remove(w, player) {
        system_chat(w, s, &format!("{name} is no longer an Advocate."));
    } else {
        system_chat(w, s, &format!("Advocate removal of {name} failed."));
    }
}

// ACE: AdvocateCommands.HandleTele
/// `tele [name] <longitude> <latitude>`: teleports you (or a player) to some location.
pub fn handle_tele(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    // Used PhatAC source to implement most of this.  Thanks Pea!

    // usage: @tele [name] longitude latitude
    // This command teleports yourself (or the specified character) to the given longitude and latitude.
    // @tele - Teleports you(or a player) to some location.

    let s = require(session);
    let me = session_player(w, s);
    if is_low_advocate(w, me) {
        return;
    }

    let mut ace_params = vec![
        ACECommandParameter {
            r#type: ACECommandParameterType::OnlinePlayerNameOrIid,
            required: false,
            default_value: Some(AceParamValue::Player(me)),
            ..ACECommandParameter::default()
        },
        ACECommandParameter {
            r#type: ACECommandParameterType::Location,
            required: true,
            error_message: Some(
                "You must supply a location to teleport to.\nExample: /tele 37s,67w".to_owned(),
            ),
            ..ACECommandParameter::default()
        },
    ];
    if !cph::resolve_ace_parameters(w, session, parameters, &mut ace_params, false) {
        return;
    }

    let position: Position = *ace_params[1].as_position();

    // Check if water block
    if lscape_get_landblock_is_entirely_water(w, position.landblock_id().raw()) {
        send_server_message(
            w,
            session,
            &format!(
                "Landblock 0x{} is entirely filled with water, and is impassable",
                format(position.landblock_id().landblock(), "X4")
            ),
            ChatMessageType::Broadcast,
        );
        return;
    }

    send_server_message(
        w,
        session,
        &format!(
            "Position: [Cell: 0x{} | Offset: {}, {}, {} | Facing: {}, {}, {}, {}]",
            format(position.landblock_id().landblock(), "X4"),
            to_string(position.position_x),
            to_string(position.position_y),
            to_string(position.position_z),
            to_string(position.rotation_x),
            to_string(position.rotation_y),
            to_string(position.rotation_z),
            to_string(position.rotation_w)
        ),
        ChatMessageType::Broadcast,
    );

    let target = ace_params[0]
        .as_player()
        .expect("NullReferenceException: AsPlayer");
    player_location::teleport(w, target, &position, false);
}

// ---------------------------------------------------------------------------------------------
// Callees that are not ported yet
// ---------------------------------------------------------------------------------------------

/// `LScape.get_landblock(landcell).WaterType == LandDefs.WaterType.EntirelyWater`: the physics
/// landscape's landblock (the shared physics crate's `WaterType`, whose `EntirelyWater` is 2).
///
/// # Panics
/// A landblock the landscape cannot load (ACE dereferences the null landblock).
fn lscape_get_landblock_is_entirely_water(w: &World, landcell: u32) -> bool {
    let landblock_cell = (landcell & 0xFFFF_0000) | 0x0001;
    let cell = phys_ext::get_landcell(w, landblock_cell)
        .expect("NullReferenceException: LScape.get_landblock");
    let landblock = w
        .physics
        .land()
        .landblock(cell.landblock())
        .expect("NullReferenceException: LScape.get_landblock");
    landblock.water_type as u8 == 2
}
