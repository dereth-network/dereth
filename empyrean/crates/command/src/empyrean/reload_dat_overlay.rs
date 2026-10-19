//! `reload-dat-overlay`: the world's data overlay read again, and landblocks built again from it.
//! Not ACE (V452).
//!
//! A world builder writes the world's data overlay while the world runs. `@reload-dat-overlay
//! [<folder>] [<landblock> ...]` opens the world's files again with the overlay in `<folder>`
//! over them (the one over them now, read afresh, when none is named), then builds each landblock
//! named again from the new files and the world database, as `reload-landblock` builds the
//! player's own: its objects (players stay), its terrain, cells, buildings and statics. It needs no
//! player, so a tool driving the server's console can run it after each change it writes. A folder
//! whose path has a space is given in quotes.

use std::path::PathBuf;

use empyrean_entity::enums::{AccessLevel, ChatMessageType};
use empyrean_net::SessionId;
use empyrean_world::managers::dat_overlay;
use empyrean_world::World;

use crate::command_handler_attribute::CommandHandlerAttribute;
use crate::command_handler_flag::CommandHandlerFlag;
use crate::command_handler_info::CommandHandlerInfo;
use crate::handlers::command_handler_helper::{write_output_error, write_output_info};

/// The `reload-dat-overlay` row (a new name, added after ACE's rows).
#[must_use]
pub fn command_handlers() -> Vec<CommandHandlerInfo> {
    let (handler, handler_name) = crate::handler!(handle_reload_dat_overlay);
    vec![CommandHandlerInfo {
        handler,
        handler_name,
        attribute: CommandHandlerAttribute::with_description(
            "reload-dat-overlay",
            AccessLevel::Developer,
            CommandHandlerFlag::None,
            "Reads the world's data overlay again, and builds landblocks again from it.",
            "[<folder>] [<landblock> ...]\n<folder> - the overlay's folder; the one read now if not given\n<landblock> - a landblock to build again, as XXYY",
        ),
    }]
}

/// A landblock as the command takes it: `XXYY`, or a cell or landblock id `0xXXYYnnnn`.
#[must_use]
pub fn landblock_of(p: &str) -> Option<u16> {
    let p = p.trim();
    if let Some(hex) = p.strip_prefix("0x").or_else(|| p.strip_prefix("0X")) {
        let id = u32::from_str_radix(hex, 16).ok()?;
        return u16::try_from(id >> 16).ok();
    }
    (p.len() == 4)
        .then(|| u16::from_str_radix(p, 16).ok())
        .flatten()
}

/// `reload-dat-overlay`: reopens the world's files with the overlay, then builds each named
/// landblock again; says how many of them were loaded.
pub fn handle_reload_dat_overlay(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let mut folder: Option<PathBuf> = None;
    let mut blocks: Vec<u16> = Vec::new();
    for p in parameters {
        match landblock_of(p) {
            Some(lb) => blocks.push(lb),
            None if folder.is_none() && blocks.is_empty() => folder = Some(PathBuf::from(p)),
            None => {
                write_output_error(
                    w,
                    session,
                    &format!("{p} is not a landblock: give it as XXYY"),
                    ChatMessageType::Broadcast,
                );
                return;
            }
        }
    }
    if let Err(e) = dat_overlay::reopen(w, folder.as_deref()) {
        write_output_error(
            w,
            session,
            &format!("The data overlay was not read again: {e}"),
            ChatMessageType::Broadcast,
        );
        return;
    }
    let built = blocks
        .iter()
        .filter(|lb| dat_overlay::reload_landblock(w, **lb))
        .count();
    write_output_info(
        w,
        session,
        &format!(
            "The data overlay is read again; {built} of {} landblock(s) named were loaded and are built again.",
            blocks.len()
        ),
        ChatMessageType::Broadcast,
    );
}
