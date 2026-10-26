//! The daemon: a client that stays in the world until it is stopped, connecting again whenever its
//! connection is lost (a server restarted, a network fault).
//!
//! `--daemon` runs the script as each connection's setup (`enter`, `plugin load`, the plugins'
//! own commands), then runs at the wall clock's pace (`run forever`) until Ctrl+C. A lost
//! connection ends that client: the daemon waits, builds a new one and runs the script again. It
//! waits [`FIRST_WAIT`] seconds after a connection that reached the world, and twice as long after
//! each attempt in a row that did not, up to [`LONGEST_WAIT`].

use std::io::Write;
use std::sync::atomic::Ordering;

use crate::run::{Options, Run, RunError};
use crate::script::Command;

/// The wait after a connection that reached the world was lost, seconds.
pub const FIRST_WAIT: f64 = 5.0;
/// The longest wait between attempts, seconds.
pub const LONGEST_WAIT: f64 = 60.0;

/// How one connection's client ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionEnd {
    /// The daemon was stopped (Ctrl+C), or its script quit: it ends.
    Stopped,
    /// The connection was lost, or never came up: the daemon connects again. `entered` says
    /// whether the client reached the world first.
    Lost { reason: String, entered: bool },
}

/// The wait before the next attempt, after `failures` attempts in a row that did not reach the
/// world (0 after one that did).
#[must_use]
pub fn wait_after(failures: u32) -> f64 {
    let doubled = FIRST_WAIT * 2f64.powi(i32::try_from(failures.min(16)).unwrap_or(16));
    doubled.min(LONGEST_WAIT)
}

/// The daemon's loop over connections: `session(n)` runs the `n`th connection's client to its
/// end; `wait(secs)` waits, answering false when the daemon was stopped meanwhile; `note` hears
/// each connection's end. Answers how many connections were made.
pub fn daemon_loop(
    mut session: impl FnMut(u32) -> SessionEnd,
    mut wait: impl FnMut(f64) -> bool,
    mut note: impl FnMut(String),
) -> u32 {
    let mut made = 0;
    let mut failures = 0;
    loop {
        made += 1;
        match session(made) {
            SessionEnd::Stopped => {
                note(format!("stopped after {made} connection(s)"));
                return made;
            }
            SessionEnd::Lost { reason, entered } => {
                failures = if entered { 0 } else { failures + 1 };
                let secs = wait_after(failures);
                note(format!(
                    "connection {made} ended{}: {reason}; connecting again in {secs:.0} s",
                    if entered { "" } else { " before the world" }
                ));
                if !wait(secs) {
                    note(format!("stopped after {made} connection(s)"));
                    return made;
                }
            }
        }
    }
}

/// Run `commands` as a daemon over `--connect`: each connection runs them, then runs until it is
/// stopped or lost; a lost one is made again. Answers how many connections were made.
///
/// # Errors
/// No `--connect`: a daemon has nothing to connect to.
pub fn daemon(commands: &[Command], opts: &Options, out: &mut dyn Write) -> Result<u32, RunError> {
    let Some(server) = opts.connect.clone() else {
        return Err(RunError::Startup(
            "--daemon needs --connect <host:port>".to_owned(),
        ));
    };
    let stop = opts.stop.clone();
    let made = daemon_loop(
        |n| {
            tracing::info!(target: "daemon", "connection {n}: to {server} as {}", opts.account);
            let mut run = match Run::new(opts, &mut *out) {
                Ok(run) => run,
                Err(e) => {
                    return SessionEnd::Lost {
                        reason: e.to_string(),
                        entered: false,
                    }
                }
            };
            let mut result = run.script(commands);
            if result.is_ok() && !run.quit && !stop.load(Ordering::Relaxed) {
                result = run.run_forever();
            }
            let entered = run.entered;
            run.finish();
            match result {
                Ok(()) => SessionEnd::Stopped,
                Err(_) if stop.load(Ordering::Relaxed) => SessionEnd::Stopped,
                Err(e) => SessionEnd::Lost {
                    reason: e.to_string(),
                    entered,
                },
            }
        },
        |secs| {
            let until = std::time::Instant::now() + std::time::Duration::from_secs_f64(secs);
            while std::time::Instant::now() < until {
                if stop.load(Ordering::Relaxed) {
                    return false;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            !stop.load(Ordering::Relaxed)
        },
        |line| tracing::info!(target: "daemon", "{line}"),
    );
    Ok(made)
}

#[cfg(test)]
mod tests {
    use super::{daemon_loop, wait_after, SessionEnd, FIRST_WAIT, LONGEST_WAIT};

    fn lost(entered: bool) -> SessionEnd {
        SessionEnd::Lost {
            reason: "the server went away".to_owned(),
            entered,
        }
    }

    #[test]
    fn the_wait_doubles_with_each_failed_attempt_up_to_a_minute() {
        assert!((wait_after(0) - FIRST_WAIT).abs() < 1e-9);
        assert!((wait_after(1) - 10.0).abs() < 1e-9);
        assert!((wait_after(3) - 40.0).abs() < 1e-9);
        assert!((wait_after(4) - LONGEST_WAIT).abs() < 1e-9);
        assert!((wait_after(40) - LONGEST_WAIT).abs() < 1e-9);
    }

    #[test]
    fn a_lost_connection_is_made_again_until_the_daemon_is_stopped() {
        // In the world, lost (a server restarted); two attempts that find no server; in the world
        // again; stopped.
        let mut ends = vec![
            lost(true),
            lost(false),
            lost(false),
            lost(true),
            SessionEnd::Stopped,
        ]
        .into_iter();
        let mut waits = Vec::new();
        let mut notes = Vec::new();
        let made = daemon_loop(
            |_| ends.next().expect("no more connections than ends"),
            |secs| {
                waits.push(secs);
                true
            },
            |line| notes.push(line),
        );
        assert_eq!(made, 5);
        assert_eq!(waits, vec![5.0, 10.0, 20.0, 5.0]);
        assert!(
            notes[0].starts_with("connection 1 ended: the server went away"),
            "{notes:?}"
        );
        assert!(
            notes[1].contains("connection 2 ended before the world"),
            "{notes:?}"
        );
        assert_eq!(
            notes.last().map(String::as_str),
            Some("stopped after 5 connection(s)")
        );
    }

    #[test]
    fn a_stop_during_a_wait_ends_the_daemon_without_connecting_again() {
        let mut made_calls = 0;
        let made = daemon_loop(
            |_| {
                made_calls += 1;
                lost(true)
            },
            |_| false,
            |_| {},
        );
        assert_eq!((made, made_calls), (1, 1));
    }
}
