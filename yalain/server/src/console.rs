//! The operator's console: commands typed into the server's standard input, one per line,
//! answered in its log.
//!
//! ```text
//! reload   compile the world database again (the --world-db folder, or the pinned release) and
//!          take what can change while the world runs; a database that does not compile leaves
//!          the world as it was
//! shutdown count down two minutes, telling every player in the world as the 2017 worlds did
//!          (at two minutes, then every thirty seconds), then stop
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
                        change while the world runs), shutdown (tell the players and stop in two \
                        minutes), hero (hero <name>, hero <name> off: make a character a \
                        hero or no longer one), help";

/// A command typed at the console.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Reload,
    Shutdown,
    Help,
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
    Some(match word.as_str() {
        "reload" => Command::Reload,
        "shutdown" => Command::Shutdown,
        "help" | "?" => Command::Help,
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
