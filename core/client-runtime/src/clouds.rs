//! `@clouds` carried out on the world (CD-cloud-command).
//!
//! What is asked for is kept with the world, and the Remastered sky reads it every frame over its
//! own settings. Nothing is sent.

use dereth_client_model::cmd::clouds;

/// What is asked for, as the world keeps it and the scene reads it.
pub use dereth_client_model::cmd::clouds::{CloudOption, Clouds, CloudsCommand, OPTIONS};

use crate::world_state::WorldState;

/// Carry out `command` on the world, and return the sentence that answers it.
pub fn apply(ws: &mut WorldState, command: CloudsCommand) -> String {
    ws.clouds.obey(command);
    clouds::describe(&ws.clouds)
}
