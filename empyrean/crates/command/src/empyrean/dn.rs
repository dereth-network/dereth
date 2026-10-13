//! `dn` and `derethnetwork`: the Dereth Network's command. Not ACE.
//!
//! `@dn` (typed `/dn` too: the retail client forwards a command it does not know, rewriting `/`
//! to `@`) is every player's: it puts a character on the network and takes it off, lists and adds
//! its friends on other worlds, links it to its player's Discord account, squelches a whole world
//! and runs channels.
//! `@derethnetwork` is the same command spelled out, and `@dng <text>` is `@dn global <text>`
//! made short: a line in the Global room. Two of its words are the operator's:
//! `@dn admin` (Sentinel and above, and the server console) blocks a world or one remote
//! character for everyone here (bare, `@dn admin block` shows the blocks and the network's
//! state), and moves the Global room to another of the world's rooms
//! (`@dn admin global <room>`), and `@dn verify <token>` (Admin, and the console) proves to the
//! hub that this world is the one its operator registered. The rows are no more than their
//! parsing; what they do is `empyrean_world::federation::commands`.

use empyrean_entity::enums::{AccessLevel, ChatMessageType};
use empyrean_net::SessionId;
use empyrean_world::federation::commands;
use empyrean_world::World;

use crate::command_handler_attribute::CommandHandlerAttribute;
use crate::command_handler_flag::CommandHandlerFlag;
use crate::command_handler_info::CommandHandlerInfo;
use crate::handlers::command_handler_helper::write_output_info;

/// The command's two names, each a row of its own (new names, added after ACE's).
pub const NAMES: [&str; 2] = ["dn", "derethnetwork"];

/// The short command for a line in the Global room (a new name, added after `dn`'s rows).
pub const GLOBAL: &str = "dng";

/// The rows. Every player may use the command, and so may the console; the operator's words
/// check their own access level.
#[must_use]
pub fn command_handlers() -> Vec<CommandHandlerInfo> {
    let mut rows: Vec<CommandHandlerInfo> = NAMES
        .iter()
        .map(|name| {
            let (handler, handler_name) = crate::handler!(handle_dn);
            CommandHandlerInfo {
                handler,
                handler_name,
                attribute: CommandHandlerAttribute::with_description(
                    name,
                    AccessLevel::Player,
                    CommandHandlerFlag::None,
                    "The Dereth Network: tells, friends and channels with other worlds.",
                    "on | off | friends [add|remove <Name@world>] | global <text> | squelch|unsquelch <world> | channel ... | link | unlink | help",
                ),
            }
        })
        .collect();
    let (handler, handler_name) = crate::handler!(handle_dng);
    rows.push(CommandHandlerInfo {
        handler,
        handler_name,
        attribute: CommandHandlerAttribute::with_description(
            GLOBAL,
            AccessLevel::Player,
            CommandHandlerFlag::None,
            "The Dereth Network: say a line in the Global room.",
            "<text>",
        ),
    });
    rows
}

/// Whether `session` (the console when `None`) is Sentinel or above, as the command table decides
/// it for a row of that level.
fn is_sentinel(w: &World, session: Option<SessionId>) -> bool {
    let Some(session) = session else {
        return true;
    };
    w.sessions
        .player(session)
        .and_then(|g| w.objects.get(g))
        .is_some_and(|p| p.is_sentinel_prop() || p.is_envoy() || p.is_arch() || p.is_admin_prop())
}

/// Whether `session` (the console when `None`) is an Admin.
fn is_admin(w: &World, session: Option<SessionId>) -> bool {
    let Some(session) = session else {
        return true;
    };
    w.sessions
        .player(session)
        .and_then(|g| w.objects.get(g))
        .is_some_and(empyrean_world::world_objects::world_object::WorldObject::is_admin_prop)
}

/// `@dn ...`: `admin ...` for the operator (in game or at the console); anything else for the
/// player of `session`, in the world.
pub fn handle_dn(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let first = parameters.first().map(|p| p.to_ascii_lowercase());
    if first.as_deref() == Some("admin") && is_sentinel(w, session) {
        let player = session.and_then(|s| w.sessions.player(s));
        let out = commands::admin_by(w, player, &parameters[1..]);
        write_output_info(w, session, &out, ChatMessageType::Broadcast);
        return;
    }
    if first.as_deref() == Some("verify") && is_admin(w, session) {
        let player = session.and_then(|s| w.sessions.player(s));
        let by = player
            .and_then(|p| empyrean_world::dispatch::name::name(w, p))
            .map(|n| n.trim_start_matches('+').to_owned())
            .unwrap_or_else(|| "the console".to_owned());
        let out = commands::verify(w, player, &by, &parameters[1..]);
        write_output_info(w, session, &out, ChatMessageType::Broadcast);
        return;
    }
    let Some(session) = session else {
        write_output_info(
            w,
            None,
            &commands::admin_help("dn"),
            ChatMessageType::Broadcast,
        );
        return;
    };
    let Some(player) = w.sessions.player(session) else {
        return;
    };
    let in_world = w
        .objects
        .get(player)
        .is_some_and(|p| p.current_landblock.is_some());
    if in_world {
        commands::dn(w, player, parameters);
    }
}

/// `@dng <text>`: the player of `session` says `text` in the Global room, as `@dn global <text>`.
pub fn handle_dng(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let Some(session) = session else {
        write_output_info(
            w,
            None,
            "dng is a player's command: a line in the Global room.",
            ChatMessageType::Broadcast,
        );
        return;
    };
    let Some(player) = w.sessions.player(session) else {
        return;
    };
    let in_world = w
        .objects
        .get(player)
        .is_some_and(|p| p.current_landblock.is_some());
    if in_world {
        commands::global(w, player, &parameters.join(" "));
    }
}
