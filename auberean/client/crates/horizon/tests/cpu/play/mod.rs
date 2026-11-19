//! The play screen's HUD: the parameter and target bars and the status row, the hotbar, the
//! selection's marks in the world, the minimap, the log window and its chat entry, the game's
//! questions, the party list and the conflict lamp, the experience band, the link line and the
//! vitae lamp; and the HUD's elements kept apart.
//!
//! Behaviour: none (this client's own interface, not a behaviour of the retail client)

mod chat;
pub mod fit;
mod gamepad;
mod hotbar;
mod hud;
mod layout;
mod marker;
mod minimap;
pub mod party;
mod questions;
mod status;
