//! The chat command interpreter: a line beginning `/` or `@` is normalised, tokenised, and looked
//! up case-insensitively in a table of ~130 client-side handlers.
//!
//! These commands belong to chat and communication. Movement commands are interpreted
//! by the runtime's movement handler, while dotted console names are registered in `console`.
//! The three command systems have separate routing and behavior, and callers must
//! distinguish them.
//!
//! **An unknown `@command` is forwarded verbatim, with no access gate.** That is how every
//! game-master command reaches the server, and it was confirmed live by typing a command the
//! client has never heard of and getting the server's reply.

pub mod console;
pub mod help;
pub mod interp;
pub mod loadfile;
pub mod table;

pub use help::HelpType;
pub use interp::{CommandInterp, CommandOutcome, TalkFocus, NOT_A_VALID_COMMAND};
pub use table::CommandEntry;
