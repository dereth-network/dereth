// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Command/Handlers/AdminShardCommands.cs
//! Port of `Source/ACE.Server/Command/Handlers/AdminShardCommands.cs`.
//!
//! The shutdown commands drive `ServerManager` (empyrean-world), whose shutdown thread is a state
//! machine the world thread polls (see `server_manager`'s module docs).
//!
//! DIVERGE: ACE prints `DateTime.Now` / `ToLocalTime()` (the host's local time) next to the UTC
//! times; the world has only its UTC clock (`w.now.utc`), so the local times read as UTC, as in
//! `server_manager`.

use empyrean_common::dotnet::{to_string, TimeSpan};
use empyrean_common::extensions::date_time_extensions::to_common_string;
use empyrean_entity::enums::{AccessLevel, ChatMessageType, PropertyString};
use empyrean_entity::ObjectGuid;
use empyrean_net::SessionId;
use empyrean_world::managers::world_manager::{self, WorldStatusState};
use empyrean_world::managers::{player_manager, server_manager};
use empyrean_world::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use empyrean_world::World;

use crate::command_handler_attribute::CommandHandlerAttribute;
use crate::command_handler_flag::CommandHandlerFlag;
use crate::command_handler_info::{CommandHandlerInfo, NamedHandler};
use crate::command_parameter_helpers::dotnet_parse;
use crate::handler;
use crate::handlers::command_handler_helper;

/// This file's `[CommandHandler]` decorations, in declaration order.
#[must_use]
pub fn command_handlers() -> Vec<CommandHandlerInfo> {
    let a = AccessLevel::Admin;
    let none = CommandHandlerFlag::None;
    let rows: [(CommandHandlerAttribute, NamedHandler); 5] = [
        (CommandHandlerAttribute::with_count("cancel-shutdown", a, none, 0, "Stops an active server shutdown.", ""), handler!(handle_cancel_shutdown)),
        (
            CommandHandlerAttribute::with_count(
                "set-shutdown-interval",
                a,
                none,
                1,
                "Changes the delay, in seconds, before the server will shutdown.",
                "< 0-99999 > in seconds",
            ),
            handler!(handle_set_shutdown_interval),
        ),
        (
            CommandHandlerAttribute::with_count(
                "stop-now",
                a,
                none,
                -1,
                "Shuts the server down, immediately!",
                "\nThis command will attempt to safely logoff all players, before shutting down the server.",
            ),
            handler!(shutdown_server_now),
        ),
        (
            CommandHandlerAttribute::with_count(
                "shutdown",
                a,
                none,
                0,
                "Begins the server shutdown process. Optionally displays a shutdown message, if a string is passed.",
                concat!(
                    "< Optional Shutdown Message >\n",
                    "\tUse @cancel-shutdown to abort an active shutdown!\n",
                    "\tSet the shutdown delay in seconds with @set-shutdown-interval < 0-99999 >",
                ),
            ),
            handler!(shutdown_server),
        ),
        (
            CommandHandlerAttribute::with_count(
                "world",
                a,
                none,
                0,
                "Open or Close world to player access.",
                "[open | close] <boot>\nIf closing world, using @world close boot will force players to logoff immediately",
            ),
            handler!(handle_help),
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

/// `session?.Player`.
fn session_player(w: &World, session: Option<SessionId>) -> Option<ObjectGuid> {
    session.map(|s| {
        w.sessions
            .player(s)
            .expect("NullReferenceException: session.Player")
    })
}

/// `(session == null) ? "CONSOLE" : session.Player.Name`.
fn admin_name(w: &World, session: Option<SessionId>) -> String {
    match session_player(w, session) {
        None => "CONSOLE".to_owned(),
        Some(p) => w
            .objects
            .get(p)
            .and_then(|o| o.get_property(PropertyString::Name))
            .unwrap_or_default(),
    }
}

// ACE: AdminShardCommands.HandleCancelShutdown
/// Cancels an in-progress shutdown event.
pub fn handle_cancel_shutdown(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    let admin_name = admin_name(w, session);
    let shutdown_time = w.server_manager.shutdown_time;
    let msg = format!(
        "{admin_name} has requested the pending shut down @ {} ({} UTC) be cancelled.",
        to_common_string(shutdown_time),
        to_common_string(shutdown_time)
    );
    log::info!("{msg}");
    let issuer = session_player(w, session);
    player_manager::broadcast_to_audit_channel(w, issuer, &msg);

    server_manager::cancel_shutdown(w);
}

// ACE: AdminShardCommands.HandleSetShutdownInterval
/// Increase or decrease the server shutdown interval in seconds
pub fn handle_set_shutdown_interval(
    w: &mut World,
    session: Option<SessionId>,
    parameters: &[String],
) {
    if !parameters.is_empty() {
        // delay server shutdown for up to x minutes
        // limit to uint length 65535
        let parse_int: String = if parameters[0].encode_utf16().count() > 5 {
            substring_utf16(&parameters[0], 5)
        } else {
            parameters[0].clone()
        };
        if let Some(new_shutdown_interval) = dotnet_parse::uint_try_parse(&parse_int) {
            // newShutdownInterval is represented as a time element
            // (ACE's `if (newShutdownInterval > uint.MaxValue) newShutdownInterval = uint.MaxValue;` never holds.)

            let admin_name = admin_name(w, session);
            let msg = format!(
                "{admin_name} has requested the shut down interval be changed from {} seconds to {new_shutdown_interval} seconds.",
                w.server_manager.shutdown_interval
            );
            //log.Info(msg);
            let issuer = session_player(w, session);
            player_manager::broadcast_to_audit_channel(w, issuer, &msg);

            // set the interval
            server_manager::set_shutdown_interval(w, new_shutdown_interval);

            // message the admin
            let text = format!(
                "Shutdown Interval (seconds to shutdown server) has been set to {}.",
                w.server_manager.shutdown_interval
            );
            command_handler_helper::write_output_info(
                w,
                session,
                &text,
                ChatMessageType::Broadcast,
            );
            return;
        }
    }
    command_handler_helper::write_output_info(
        w,
        session,
        "Usage: /set-shutdown-interval <00000>",
        ChatMessageType::Broadcast,
    );
}

/// `s.Substring(0, n)` over UTF-16 units (a split surrogate pair becomes U+FFFD).
fn substring_utf16(s: &str, n: usize) -> String {
    let units: Vec<u16> = s.encode_utf16().take(n).collect();
    String::from_utf16_lossy(&units)
}

// ACE: AdminShardCommands.ShutdownServerNow
/// Immediately begins the shutdown process by setting the shutdown interval to 0 before executing
/// the shutdown method
pub fn shutdown_server_now(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let admin_name = admin_name(w, session);
    let msg = format!("{admin_name} has initiated an immediate server shut down.");
    //log.Info(msg);
    let issuer = session_player(w, session);
    player_manager::broadcast_to_audit_channel(w, issuer, &msg);

    server_manager::set_shutdown_interval(w, 0);
    shutdown_server(w, session, parameters);
}

// ACE: AdminShardCommands.ShutdownServer
/// Function to shutdown the server from console or in-game.
pub fn shutdown_server(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    if w.server_manager.shutdown_initiated {
        command_handler_helper::write_output_info(
            w,
            session,
            "Shutdown is already in progress.",
            ChatMessageType::Broadcast,
        );
        return;
    }

    let mut admin_text = String::new();
    if !parameters.is_empty() {
        admin_text = parameters.join(" ");
    }

    let mut admin_name = admin_name(w, session);
    let hide_name = admin_text.is_empty();

    let time_till_shutdown = TimeSpan::from_seconds(f64::from(w.server_manager.shutdown_interval));
    let time_remaining = format!(
        "The server will shut down in {}",
        time_remaining_text(time_till_shutdown)
    );

    let now = w.now.utc;
    log::info!(
        "{admin_name} initiated a complete server shutdown @ {} ({} UTC)",
        to_common_string(now),
        to_common_string(now)
    );
    log::info!("{time_remaining}");
    let issuer = session_player(w, session);
    player_manager::broadcast_to_audit_channel(
        w,
        issuer,
        &format!(
            "{admin_name} initiated a complete server shutdown @ {} ({} UTC)",
            to_common_string(now),
            to_common_string(now)
        ),
    );

    if !admin_text.is_empty() {
        log::info!("Admin message: {admin_text}");
        player_manager::broadcast_to_audit_channel(
            w,
            issuer,
            &format!("{admin_name} sent the following message for the shutdown: {admin_text}"),
        );
    }

    if admin_name == "CONSOLE" {
        admin_name = "System".to_owned();
    }

    let generic_msg_to_players =
        shutdown_broadcast_text(time_till_shutdown, hide_name, &admin_name);

    if hide_name {
        player_manager::broadcast_to_all(
            w,
            &game_message_system_chat(&generic_msg_to_players, ChatMessageType::WorldBroadcast),
        );
    } else {
        let text = format!("Broadcast from {admin_name}> {admin_text}\n{generic_msg_to_players}");
        player_manager::broadcast_to_all(
            w,
            &game_message_system_chat(&text, ChatMessageType::WorldBroadcast),
        );
    }

    server_manager::begin_shutdown(w);
}

/// `ShutdownServer`'s `timeRemaining` tail: whole minutes above 120 seconds, else the seconds.
#[must_use]
pub fn time_remaining_text(time_till_shutdown: TimeSpan) -> String {
    if time_till_shutdown.total_seconds() > 120.0 {
        // (int)timeTillShutdown.TotalMinutes
        #[allow(clippy::cast_possible_truncation)]
        let minutes = time_till_shutdown.total_minutes() as i32;
        format!("{minutes} minutes.")
    } else {
        format!("{} seconds.", to_string(time_till_shutdown.total_seconds()))
    }
}

/// `ShutdownServer`'s `timeHrs`/`timeMins`/`timeSecs`/`time` and `genericMsgToPlayers`, given
/// `adminName` after "CONSOLE" became "System" (split out for the vectors).
#[must_use]
pub fn shutdown_broadcast_text(
    time_till_shutdown: TimeSpan,
    hide_name: bool,
    admin_name: &str,
) -> String {
    let sdt = time_till_shutdown;
    let time_hrs = format!(
        "{}{}",
        if sdt.hours() >= 1 {
            sdt.format("%h")
        } else {
            String::new()
        },
        if sdt.hours() >= 2 {
            " hours"
        } else if sdt.hours() == 1 {
            " hour"
        } else {
            ""
        }
    );
    let time_mins = format!(
        "{}{}",
        if sdt.minutes() != 0 {
            sdt.format("%m")
        } else {
            String::new()
        },
        if sdt.minutes() >= 2 {
            " minutes"
        } else if sdt.minutes() == 1 {
            " minute"
        } else {
            ""
        }
    );
    let time_secs = format!(
        "{}{}",
        if sdt.seconds() != 0 {
            sdt.format("%s")
        } else {
            String::new()
        },
        if sdt.seconds() >= 2 {
            " seconds"
        } else if sdt.seconds() == 1 {
            " second"
        } else {
            ""
        }
    );
    let mut time = time_hrs.clone();
    if !time_mins.is_empty() {
        time += if time_hrs.is_empty() { "" } else { ", " };
        time += &time_mins;
    }
    if !time_secs.is_empty() {
        time += if !time_hrs.is_empty() || !time_mins.is_empty() {
            " and "
        } else {
            ""
        };
        time += &time_secs;
    }

    let from = if hide_name { "System" } else { admin_name };
    let mut generic_msg_to_players = format!(
        "Broadcast from {from}> {} - This Asheron's Call Server will be shutting down in {time}{}{}",
        if time_till_shutdown.total_minutes() > 1.5 { "ATTENTION" } else { "WARNING" },
        if sdt.total_minutes() <= 1.0 { "!" } else { "." },
        if time_till_shutdown.total_minutes() <= 3.0 {
            format!(" Please log out{}", if sdt.total_minutes() <= 1.0 { "!" } else { "." })
        } else {
            String::new()
        }
    );

    if sdt.total_milliseconds() == 0.0 {
        generic_msg_to_players = format!("Broadcast from {from}> ATTENTION - This Asheron's Call Server is shutting down NOW!!!!");
    }
    generic_msg_to_players
}

// ACE: AdminShardCommands.HandleHelp
/// `world [open | close] <boot>`: open or close the world to player access.
pub fn handle_help(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let mut open = false;
    let mut close = false;
    let mut boot_players = false;

    let status = match w.world_manager.world_status {
        WorldStatusState::Closed => "Closed",
        WorldStatusState::Open => "Open",
    };
    let mut message = format!(
        "World is currently {status}\nPlease specify state to change\n@world [open | close] <boot>\nIf closing world, using @world close boot will force players to logoff immediately"
    );
    if !parameters.is_empty() {
        match parameters[0].to_lowercase().as_str() {
            "open" => {
                if w.world_manager.world_status == WorldStatusState::Open {
                    message = "World is already open.".to_owned();
                } else {
                    message = "Opening world to players...".to_owned();
                    open = true;
                }
            }
            "close" => {
                if w.world_manager.world_status == WorldStatusState::Closed {
                    message = "World is already closed.".to_owned();
                } else {
                    if parameters.len() > 1 && parameters[1].to_lowercase() == "boot" {
                        boot_players = true;
                    }
                    message = "Closing world".to_owned();
                    if boot_players {
                        message += ", and booting all online players.";
                    } else {
                        message += "...";
                    }

                    close = true;
                }
            }
            _ => {}
        }
    }

    command_handler_helper::write_output_info(
        w,
        session,
        &message,
        ChatMessageType::WorldBroadcast,
    );

    let player = session_player(w, session);
    if open {
        world_manager::open(w, player);
    } else if close {
        world_manager::close(w, player, boot_players);
    }
}
