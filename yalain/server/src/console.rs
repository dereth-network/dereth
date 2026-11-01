//! The operator's console: commands typed into the server's standard input, one per line,
//! answered in its log.
//!
//! ```text
//! reload   compile the world database again (the --world-db folder, or the pinned release) and
//!          take what can change while the world runs; a database that does not compile leaves
//!          the world as it was
//! shutdown [message]
//!          count down (two minutes unless set otherwise), telling every player in the world as
//!          the 2017 worlds did (at two minutes, then every thirty seconds), then stop; the
//!          message, if given, is shown to every player in the world first
//! cancel-shutdown
//!          stop the countdown, and tell every player in the world
//! set-shutdown-interval <0-99999>
//!          how many seconds the next shutdown counts down
//! stop-now [message]
//!          stop the server at once, keeping every character in the world
//! save-world
//!          keep every character in the world now; the world goes on
//! dn       the world's link to the Dereth Network and its blocks; `dn block <world or
//!          Name@world>` and `dn unblock ...` block a world or a character for everyone on this
//!          world, or lift it (`yalain_federation`'s operator commands)
//! hero     `hero <name>` makes a character a hero (it earns focus, and has its greatest at
//!          once), `hero <name> off` no longer one; in the world or out of it
//! help     list the commands
//! ```
//!
//! A server started with no standard input (a service, a test) has no console and runs as
//! before.

use std::io::BufRead as _;
use std::sync::mpsc::{self, Receiver, TryRecvError};

/// The commands, as `help` lists them.
pub const HELP: &str = "commands: reload (compile the world database again and take what can \
                        change while the world runs), shutdown [message] (tell the players and \
                        stop after the countdown), cancel-shutdown, set-shutdown-interval \
                        <seconds> (the countdown's length, 0 to 99999), stop-now [message], \
                        save-world (keep every character in the world now), dn (the Dereth Network: dn, dn block <world or Name@world>, \
                        dn unblock <world or Name@world>), hero (hero <name>, hero <name> off: make a character a \
                        hero or no longer one), help";

/// A command typed at the console.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Reload,
    /// `shutdown [message]`: the message, if one was given.
    Shutdown(Option<String>),
    CancelShutdown,
    /// `set-shutdown-interval <seconds>`: the seconds, `None` for a line without a number from 0
    /// to 99999.
    SetShutdownInterval(Option<u32>),
    /// `stop-now [message]`.
    StopNow(Option<String>),
    SaveWorld,
    Help,
    /// `dn` and the words after it.
    Network(Vec<String>),
    /// `hero <name> [off]`: the character's name, and whether it is made a hero; `None` for a
    /// line without a name.
    Hero(Option<(String, bool)>),
    /// A word that is no command.
    Unknown(String),
}

/// The command a line holds: its first word, in any case; `None` for a blank line.
#[must_use]
pub fn parse(line: &str) -> Option<Command> {
    let word = line.split_whitespace().next()?.to_ascii_lowercase();
    // What follows the command's word, as typed.
    let rest = || {
        let after = line.trim_start()[word.len()..].trim();
        (!after.is_empty()).then(|| after.to_owned())
    };
    Some(match word.as_str() {
        "reload" => Command::Reload,
        "shutdown" => Command::Shutdown(rest()),
        "cancel-shutdown" => Command::CancelShutdown,
        "set-shutdown-interval" => Command::SetShutdownInterval(
            rest()
                .and_then(|n| n.parse::<u32>().ok())
                .filter(|n| *n <= yalain_world::shutdown::MAX_SECONDS),
        ),
        "stop-now" => Command::StopNow(rest()),
        "save-world" => Command::SaveWorld,
        "help" | "?" => Command::Help,
        "dn" => Command::Network(line.split_whitespace().skip(1).map(str::to_owned).collect()),
        "hero" => {
            let words: Vec<&str> = line.split_whitespace().skip(1).collect();
            Command::Hero(match words[..] {
                [name] => Some((name.to_owned(), true)),
                [name, off] if off.eq_ignore_ascii_case("off") => Some((name.to_owned(), false)),
                _ => None,
            })
        }
        _ => Command::Unknown(word),
    })
}

/// The lines typed into standard input, as they come. The channel ends with the input.
#[must_use]
pub fn lines() -> Receiver<String> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    rx
}

/// Work running on a thread of its own, so the world goes on meanwhile.
pub struct Job<T>(Receiver<Result<T, String>>);

/// Start `work` on a thread of its own.
pub fn start<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Job<T> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(work());
    });
    Job(rx)
}

impl<T> Job<T> {
    /// Its result once it is done.
    pub fn poll(&self) -> Option<Result<T, String>> {
        match self.0.try_recv() {
            Ok(r) => Some(r),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                Some(Err("the work stopped before it was done".into()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{parse, Command};

    /// The console's shutdown commands, as Empyrean's: `shutdown` with an optional message (as
    /// typed, its spaces inside kept), `cancel-shutdown`, `set-shutdown-interval` with 0 to 99999
    /// seconds, `stop-now` with an optional message, and `save-world`, the command's word in any
    /// case.
    /// Behaviour: server.operator.shutdown-message-and-interval
    #[test]
    fn the_consoles_shutdown_commands_read_their_message_and_seconds() {
        let m = |s: &str| Some(s.to_owned());
        assert_eq!(parse("shutdown"), Some(Command::Shutdown(None)));
        assert_eq!(parse("  SHUTDOWN   "), Some(Command::Shutdown(None)));
        assert_eq!(
            parse("shutdown Back in  ten minutes. "),
            Some(Command::Shutdown(m("Back in  ten minutes.")))
        );
        assert_eq!(parse("cancel-shutdown"), Some(Command::CancelShutdown));
        assert_eq!(
            parse("set-shutdown-interval 600"),
            Some(Command::SetShutdownInterval(Some(600)))
        );
        assert_eq!(
            parse("set-shutdown-interval 0"),
            Some(Command::SetShutdownInterval(Some(0)))
        );
        for bad in [
            "set-shutdown-interval",
            "set-shutdown-interval 100000",
            "set-shutdown-interval -1",
            "set-shutdown-interval soon",
        ] {
            assert_eq!(
                parse(bad),
                Some(Command::SetShutdownInterval(None)),
                "{bad}"
            );
        }
        assert_eq!(parse("stop-now"), Some(Command::StopNow(None)));
        assert_eq!(
            parse("Stop-Now Restarting."),
            Some(Command::StopNow(m("Restarting.")))
        );
        assert_eq!(parse("save-world"), Some(Command::SaveWorld));
    }
}
