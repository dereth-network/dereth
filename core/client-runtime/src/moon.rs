//! `@moon` carried out on the world (CD-moon-command).
//!
//! What is asked for is kept with the world, and the Remastered sky reads it every frame over the
//! moons' calendar. Nothing is sent.

use dereth_client_model::cmd::moon;

/// What is asked for, as the world keeps it and the scene reads it.
pub use dereth_client_model::cmd::moon::{calendar_days, Moon, MoonCommand, Moons, MOONS};

use crate::world_state::WorldState;

/// Carry out `command` on the world, and return the sentence that answers it: each moon's phase
/// as the world's clock stands.
pub fn apply(ws: &mut WorldState, command: MoonCommand) -> String {
    ws.moons.obey(command);
    let (year, day, time_of_day) = ws.game_time();
    moon::describe(&ws.moons, Some(calendar_days(year, day, time_of_day)))
}
