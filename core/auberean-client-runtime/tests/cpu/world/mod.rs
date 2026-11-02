//! The client's side of the world: the game clock, the player's position reports, what its
//! requests carry, the updates it takes, the data the server brings up to date, and the one data
//! cache everything reads records through, with the text elements' string downloads; the world the
//! objects are kept in; the session's end and the connection's measure.

mod cache;
mod clock;
mod download;
mod objects;
mod reports;
mod requests;
mod session;
mod strings;
mod updates;
