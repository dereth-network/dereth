// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Command/Handlers/HelpCommands.cs
//! Port of `Source/ACE.Server/Command/Handlers/HelpCommands.cs`.
//!
//! `acecommands` sorts with `orderby cmd.Attribute.Command`, the current culture's (en-US) string
//! order: [`crate::handlers::admin_commands::culture_compare`] (vectored against .NET over every
//! command name, `help_acecommands_console`).

use empyrean_entity::enums::{AccessLevel, ChatMessageType, WeenieError};
use empyrean_net::SessionId;
use empyrean_world::network::game_event::events::game_event_weenie_error::game_event_weenie_error;
use empyrean_world::network::game_event::game_event_message::session_data;
use empyrean_world::network::game_messages::game_message::{enqueue_send, enqueue_send_many};
use empyrean_world::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use empyrean_world::World;

use crate::command_handler_attribute::CommandHandlerAttribute;
use crate::command_handler_flag::CommandHandlerFlag;
use crate::command_handler_info::{CommandHandlerInfo, NamedHandler};
use crate::command_manager::{self, console_write_line};
use crate::handler;
use crate::handlers::admin_commands::{culture_compare, try_parse_access_level};

/// This file's `[CommandHandler]` decorations, in declaration order.
#[must_use]
pub fn command_handlers() -> Vec<CommandHandlerInfo> {
    // DIVERGE: Empyrean's `emphelp` and `empcommands` (crate::empyrean) carry ACE's descriptions; ACE's names are listed as the same command (brand).
    let rows: [(CommandHandlerAttribute, NamedHandler); 2] = [
        (
            CommandHandlerAttribute::with_count(
                "acehelp",
                AccessLevel::Player,
                CommandHandlerFlag::None,
                0,
                "Same as @emphelp (ACE's name).",
                "(command)",
            ),
            handler!(handle_ace_help),
        ),
        (
            CommandHandlerAttribute::with_count(
                "acecommands",
                AccessLevel::Player,
                CommandHandlerFlag::None,
                0,
                "Same as @empcommands (ACE's name).",
                "<access level or search>",
            ),
            handler!(handle_ace_commands),
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

fn session_access_level(w: &World, session: SessionId) -> AccessLevel {
    w.sessions
        .get(session)
        .map_or(AccessLevel::Player, |s| s.access_level)
}

// ACE: HelpCommands.HandleACEHelp
/// `acehelp (command)`: displays help.
pub fn handle_ace_help(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    if parameters.is_empty() {
        if let Some(s) = session {
            // DIVERGE: ACE's text names `@acehelp`, `@acecommands` and ACEmulator; ours names our commands and Empyrean (brand).
            let msg = concat!(
                "Note: You may substitute a forward slash (/) for the at symbol (@).\n",
                "Use @help to get more information about commands supported by the client.\n",
                "Available help:\n",
                "@emphelp commands - Lists all commands.\n",
                "You can also use @empcommands to get a complete list of the supported Empyrean commands available to you.\n",
                "To get more information about a specific command, use @emphelp command\n",
            );
            enqueue_send(
                w,
                s,
                game_message_system_chat(msg, ChatMessageType::Broadcast),
            );
        }

        return;
    }

    // `commands` in any case.
    if empyrean_world::managers::player_manager::equals_ordinal_ignore_case(
        &parameters[0],
        "commands",
    ) {
        // Mimick @help commands command
        handle_ace_commands(w, session, parameters);
        return;
    }

    for command in command_manager::get_command_by_name(&parameters[0]) {
        if let Some(s) = session {
            if command.attribute.flags == CommandHandlerFlag::ConsoleInvoke {
                continue;
            }
            if session_access_level(w, s) < command.attribute.access {
                continue;
            }

            let msg = format!(
                "@{} - {}\nUsage: @{} {}\n",
                command.attribute.command,
                command.attribute.description,
                command.attribute.command,
                command.attribute.usage
            );

            enqueue_send(
                w,
                s,
                game_message_system_chat(&msg, ChatMessageType::Broadcast),
            );

            return;
        }

        if command.attribute.flags == CommandHandlerFlag::RequiresWorld {
            continue;
        }
        console_write_line(&format!(
            "{} - {}",
            command.attribute.command, command.attribute.description
        ));
        console_write_line(&format!(
            "Usage: {} {}",
            command.attribute.command, command.attribute.usage
        ));

        return;
    }

    if let Some(s) = session {
        // DIVERGE: our command names (brand).
        let msg = concat!(
            "Use @empcommands to get a complete list of commands available for you to use.\n",
            "To get more information about a specific command, use @emphelp command\n",
        );

        let unknown = game_message_system_chat(
            &format!("Unknown command: {}", parameters[0]),
            ChatMessageType::Help,
        );
        let error =
            game_event_weenie_error(session_data(w, s), WeenieError::ThatIsNotAValidCommand);
        let help = game_message_system_chat(msg, ChatMessageType::Broadcast);
        enqueue_send_many(w, s, [unknown, error, help]);
    } else {
        console_write_line(&format!("Unknown command: {}", parameters[0]));
    }
}

// ACE: HelpCommands.HandleACECommands
/// `acecommands <access level or search>`: lists all commands.
pub fn handle_ace_commands(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let mut command_list: Vec<String> = Vec::new();

    // DIVERGE: our command name (brand).
    let msg_header = concat!(
        "Note: You may substitute a forward slash (/) for the at symbol (@).\n",
        "For more information, type @emphelp < command >.\n"
    );

    if session.is_none() {
        console_write_line("For more information, type emphelp < command >.");
    }

    let mut access_level = session.map_or(AccessLevel::Admin, |s| session_access_level(w, s));
    let mut exact = false;
    let mut search: Option<&str> = None;

    if !parameters.is_empty() {
        let param = &parameters[0];
        match try_parse_access_level(param, true) {
            Some(p_access_level) if p_access_level <= access_level => {
                access_level = p_access_level;
                exact = true;
            }
            _ => search = Some(param),
        }
    }

    let restrict = if session.is_some() {
        CommandHandlerFlag::ConsoleInvoke
    } else {
        CommandHandlerFlag::RequiresWorld
    };

    let mut commands: Vec<CommandHandlerInfo> = command_manager::get_commands()
        .into_iter()
        .filter(|cmd| {
            let a = &cmd.attribute;
            (if exact {
                a.access == access_level
            } else {
                a.access <= access_level
            }) && a.flags != restrict
                && search.is_none_or(|search| {
                    contains_ordinal_ignore_case(
                        &format!(
                            "{} {} {}",
                            a.access.to_dotnet_string(),
                            a.command,
                            a.description
                        ),
                        search,
                    )
                })
        })
        .collect();
    // `orderby`: a stable sort by the culture's string order
    commands.sort_by(|a, b| culture_compare(&a.attribute.command, &b.attribute.command));

    for command in &commands {
        command_list.push(format!(
            "@{} - {}",
            command.attribute.command, command.attribute.description
        ));
    }

    let msg = command_list.join("\n");

    if let Some(s) = session {
        enqueue_send(
            w,
            s,
            game_message_system_chat(&format!("{msg_header}{msg}"), ChatMessageType::Broadcast),
        );
    } else {
        console_write_line(&msg);
    }
}

/// `s.Contains(value, StringComparison.OrdinalIgnoreCase)`: each character upper-cased by the
/// simple invariant mapping (a mapping that changes its UTF-16 length, or maps a non-ASCII
/// character to an ASCII one, leaves it as it is), then an ordinal search.
#[must_use]
pub fn contains_ordinal_ignore_case(s: &str, value: &str) -> bool {
    fn up(c: char) -> char {
        let mut u = c.to_uppercase();
        match (u.next(), u.next()) {
            (Some(x), None)
                if x.len_utf16() == c.len_utf16() && (c.is_ascii() || !x.is_ascii()) =>
            {
                x
            }
            _ => c,
        }
    }
    let s: Vec<char> = s.chars().map(up).collect();
    let value: Vec<char> = value.chars().map(up).collect();
    value.is_empty()
        || s.windows(value.len())
            .any(|window| window == value.as_slice())
}
