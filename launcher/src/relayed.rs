//! Playing a world through a relay on this computer ([`dereth_launch::relayed`]).
//!
//! Play starts `empyrean-relay` on two free loopback ports, waits for its status ping to answer,
//! and starts the client towards it. The relay runs as long as the client does: the launcher stops
//! it a moment after the client ends, once it has told the server, and it stops by itself should
//! the launcher go first. Its words go in the data folder, one file per world, written anew at each
//! launch.

use std::net::UdpSocket;
use std::path::Path;
use std::process::{Child, Stdio};
use std::time::{Duration, Instant};

use dereth_launch::relayed;
use dereth_launch::world::World;

use crate::backend::{lock, LaunchOutcome, Shared};
use crate::components;

/// The ports tried for a relay, each with the port above it.
const PORTS: std::ops::Range<u16> = 9400..9500;

/// How many relays are tried before giving up: another program can take a port just found free.
const TRIES: usize = 3;

/// How long a relay has to answer on its port.
const READY_WAIT: Duration = Duration::from_secs(10);

/// How long a relay outlives its last session, so a client that signs in again keeps it, and one
/// left behind by a launcher that went away stops.
const IDLE_EXIT_SECONDS: u64 = 600;

/// How long a relay is left after its client ends: long enough to log the character off and say
/// what the session exchanged.
pub const LOG_OFF_WAIT: Duration = Duration::from_secs(5);

/// A relay that answered, and the port the client connects to.
pub struct Started {
    pub child: Child,
    pub port: u16,
}

/// Stops a relay and reaps it.
pub fn stop(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// Stops a relay whose client has ended, `wait` from now, without holding up the launcher.
pub fn stop_after(mut child: Child, wait: Duration) {
    std::thread::spawn(move || {
        std::thread::sleep(wait);
        stop(&mut child);
    });
}

/// Two free ports in a row among `ports`: a relay listens on the first and the one above it.
fn free_ports(ports: std::ops::Range<u16>) -> Option<u16> {
    ports.step_by(2).find(|&port| {
        let free = |p| UdpSocket::bind((std::net::Ipv4Addr::LOCALHOST, p)).is_ok();
        free(port) && free(port + 1)
    })
}

/// Waits until `answers` says the relay is up, `child` has ended, or `wait` has passed.
fn wait_until_ready(
    child: &mut Child,
    wait: Duration,
    answers: impl Fn() -> bool,
) -> Result<(), String> {
    let until = Instant::now() + wait;
    while Instant::now() < until {
        if answers() {
            return Ok(());
        }
        if let Ok(Some(status)) = child.try_wait() {
            return Err(format!("it stopped as it started ({status})"));
        }
    }
    Err("it did not answer".to_owned())
}

/// Starts the relay `program` for `world`, with its words in `log`.
///
/// # Errors
/// What went wrong, for the player.
pub fn start(program: &Path, world: &World, log: &Path) -> Result<Started, String> {
    let mut last = String::new();
    for _ in 0..TRIES {
        let port = free_ports(PORTS).ok_or("No free ports for the relay on this computer.")?;
        let args = relayed::relay_args(world, port, IDLE_EXIT_SECONDS)
            .ok_or("This world has no address to connect to.")?;
        let words = std::fs::File::create(log).map_err(|e| format!("{}: {e}", log.display()))?;
        let mut child = components::command(program)
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(words)
            .spawn()
            .map_err(|e| format!("Could not start the relay: {e}"))?;
        let ready = wait_until_ready(&mut child, READY_WAIT, || {
            crate::backend::ping_status(&std::net::Ipv4Addr::LOCALHOST.to_string(), port).is_some()
        });
        match ready {
            Ok(()) => return Ok(Started { child, port }),
            Err(why) => {
                stop(&mut child);
                last = why;
            }
        }
    }
    Err(format!(
        "Could not start the relay: {last}. See {}.",
        log.display()
    ))
}

/// Starts the client a choice names on a world played through a relay: the relay first, the one
/// that came with the client ([`components::relay_program`]).
pub fn launch(b: &Shared, c: &components::Shared, choice: crate::backend::Choice) -> LaunchOutcome {
    let err = |message: String| LaunchOutcome::Error { message };
    let Some((world, log)) = ({
        let backend = lock(b);
        let world = backend.world(&choice.world_slug);
        world.map(|w| {
            let log = backend.data_folder().join(format!("relay-{}.log", w.slug));
            (w, log)
        })
    }) else {
        return err("That world is no longer listed.".into());
    };
    let Some(program) = components::relay_program(c) else {
        return err(format!(
            "{} needs the newest Dereth. Update Dereth, then press Play again.",
            world.name
        ));
    };
    match start(&program, &world, &log) {
        Ok(started) => lock(b).launch_through(choice, started),
        Err(message) => err(message),
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (how the launcher starts a relay).
    use super::*;

    /// A process that runs until it is stopped: a shell waiting on its input.
    fn long_process() -> Child {
        let mut command = if cfg!(windows) {
            let mut c = std::process::Command::new("cmd");
            c.args(["/C", "pause"]);
            c
        } else {
            let mut c = std::process::Command::new("sh");
            c.args(["-c", "read x"]);
            c
        };
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap()
    }

    #[test]
    fn free_ports_passes_over_a_port_in_use_and_its_neighbor() {
        let taken = UdpSocket::bind("127.0.0.1:0").unwrap();
        let port = taken.local_addr().unwrap().port();
        let from = port.min(65000) & !1;
        let found = free_ports(from..from.saturating_add(40)).unwrap();
        assert!(found != port && found + 1 != port);
        assert_eq!(found % 2, from % 2, "the range's own steps");
        assert_eq!(free_ports(from..from), None, "an empty range finds none");
    }

    #[test]
    fn a_relay_that_answers_is_ready_at_once() {
        let mut child = long_process();
        // A wait of an hour: only a relay that did not answer would use it.
        let ready = wait_until_ready(&mut child, Duration::from_secs(3600), || true);
        stop(&mut child);
        assert_eq!(ready, Ok(()));
    }

    #[test]
    fn a_relay_that_stops_as_it_starts_is_not_waited_for() {
        let mut child = long_process();
        stop(&mut child);
        let ready = wait_until_ready(&mut child, Duration::from_secs(3600), || false);
        assert!(ready.unwrap_err().contains("stopped as it started"));
    }

    #[test]
    fn a_relay_that_never_answers_is_given_up_on() {
        let mut child = long_process();
        let ready = wait_until_ready(&mut child, Duration::ZERO, || true);
        stop(&mut child);
        assert_eq!(ready, Err("it did not answer".to_owned()));
    }
}
