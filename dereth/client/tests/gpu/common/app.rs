//! Device-backed constructors over shared real application fixtures.
//!
//! Behaviour: none (shared fixtures)

use dereth_client::app::App;

#[path = "../../common/app.rs"]
mod shared;

pub use shared::{
    app_with_recorded_body_with, body, frames, gameplay, key, movement_key, player_description,
    position, unhide_player, unhide_recorded_player,
};

/// Start the UI and configured static scene, then advance into gameplay.
pub fn app_in_gameplay(frames: u32) -> App {
    shared::app_in_gameplay_with(frames, App::new)
}

/// Build the recorded player on real terrain and finish its recorded unhide.
pub fn app_with_recorded_body() -> App {
    shared::app_with_recorded_body_with(App::new)
}
