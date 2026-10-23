//! `save-world`: every player and what the world keeps saved now, as a clean shutdown saves them,
//! with the world left running. Not ACE (V455).
//!
//! A clean shutdown logs every player off, which saves each with what it carries, unloads every
//! landblock, which saves the objects a landblock keeps (houses and what is stored in them, and
//! anything else kept in the shard), and saves the offline players with changes. `save-world` saves
//! the same, in place: each player online, each loaded landblock's objects, and the offline players
//! with changes. It says what it is saving at once, and `Saved.` once the database has written all
//! of it (or that it could not), since the saves are written on the database's own thread.
//!
//! ACE has no such command: its `save` (an admin's sanctuary point) keeps its name and meaning.

use empyrean_entity::enums::{AccessLevel, ChatMessageType};
use empyrean_net::SessionId;
use empyrean_world::entity::landblock;
use empyrean_world::managers::{landblock_manager, player_manager};
use empyrean_world::world_objects::player_database;
use empyrean_world::World;

use crate::command_handler_attribute::CommandHandlerAttribute;
use crate::command_handler_flag::CommandHandlerFlag;
use crate::command_handler_info::CommandHandlerInfo;
use crate::handlers::command_handler_helper::{write_output_error, write_output_info};

/// The `save-world` row (a new name, added after ACE's rows).
#[must_use]
pub fn command_handlers() -> Vec<CommandHandlerInfo> {
    let (handler, handler_name) = crate::handler!(handle_save_world);
    vec![CommandHandlerInfo {
        handler,
        handler_name,
        attribute: CommandHandlerAttribute::with_count(
            "save-world",
            AccessLevel::Admin,
            CommandHandlerFlag::None,
            0,
            "Saves every player and what the world keeps now, as a clean shutdown does, and keeps the world running.",
            "",
        ),
    }]
}

/// What `save-world` says it is saving.
#[must_use]
pub fn saving_message(players: usize, landblocks: usize) -> String {
    let players = match players {
        1 => "1 player".to_owned(),
        n => format!("{n} players"),
    };
    let landblocks = match landblocks {
        1 => "1 landblock".to_owned(),
        n => format!("{n} landblocks"),
    };
    format!("Saving {players} and {landblocks}...")
}

/// `save-world`: saves each player online, each loaded landblock's objects and the offline players
/// with changes, then says `Saved.` once the database has written them.
pub fn handle_save_world(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    let players = player_manager::get_all_online(w);
    for player in &players {
        player_database::save_player_to_database(w, *player);
    }
    let landblocks = landblock_manager::get_loaded_landblocks(w);
    for id in &landblocks {
        landblock::save_now(w, *id);
    }
    player_manager::save_offline_players_with_changes(w);
    write_output_info(
        w,
        session,
        &saving_message(players.len(), landblocks.len()),
        ChatMessageType::Broadcast,
    );
    // The database thread runs its jobs in order, and hands a job's answer back only after what
    // came before it is written: this one's answer comes once every save above is.
    w.shard
        .get_current_queue_wait_time(Some(Box::new(move |w: &mut World, _| {
            if w.shard.is_writer_stopped() {
                write_output_error(
                    w,
                    session,
                    "The save failed: the database stopped taking writes. See the log.",
                    ChatMessageType::Broadcast,
                );
            } else {
                write_output_info(w, session, "Saved.", ChatMessageType::Broadcast);
            }
        })));
}
