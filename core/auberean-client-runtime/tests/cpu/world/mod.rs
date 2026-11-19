//! The client's side of the world: the game clock, the player's position reports, what its
//! requests carry, the updates it takes, the data the server brings up to date, and the one data
//! cache everything reads records through, with the text elements' string downloads; the player's
//! controls the client carries; the world the objects are kept in; the session's end and the
//! connection's measure; how many players a world says it has; the selection keys' rules; the
//! game's questions to the player; the player's fellowship, allegiance, friends and kingdom; the
//! map's markers; the player's pets and its mount; the player's level, experience and vitae; the
//! shortcut bar's clean-up; the notices the frame's hook hears.

mod cache;
mod clock;
mod controls;
mod download;
mod hook;
mod markers;
mod objects;
mod pets;
mod players;
mod progress;
mod questions;
mod reports;
mod requests;
mod session;
mod social;
mod strings;
mod targeting;
mod toolbar;
mod updates;
