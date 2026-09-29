// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Command/CommandHandler.cs
//! Port of `Source/ACE.Server/Command/CommandHandler.cs`.

use empyrean_net::SessionId;
use empyrean_world::World;

// (ACE's `CommandHandler` delegate: a type, no members.)
/// `delegate void CommandHandler(Session session, params string[] parameters)`: every command
/// runs on the world thread with the world. `session` is `None` for the console.
pub type CommandHandler = fn(w: &mut World, session: Option<SessionId>, parameters: &[String]);
