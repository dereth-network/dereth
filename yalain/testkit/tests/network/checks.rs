//! The run's own checks, on the hub's program: a program that ends during a wait fails the wait
//! at once, with its name, how it ended and its log; a program that loses a port to another
//! program before it binds it is started again on fresh ports; one that ends for anything else
//! is not.
//!
//! Behaviour: none (checks the end-to-end run's own harness: its waits and its starts)

use std::net::{Ipv4Addr, TcpListener};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;
use std::time::Duration;

use crate::programs::{
    program, second_game_ports, serve, settle, wait_for, wait_within, Process, Scratch,
    HUB_LISTENING,
};

/// A guard against a wait that never notices, far past the end it waits on.
const GUARD: Duration = Duration::from_secs(120);

/// The hub's program serving on `listen`, its files in `dir`.
fn hub(dir: &Path, listen: &str, n: usize) -> Process {
    let hub = program("DERETH_NETWORK_HUB_BIN", "dereth-network-hub");
    Process::start(
        "hub",
        serve(&hub, dir, listen),
        &dir.join(format!("hub-{n}.log")),
        false,
    )
}

/// What a panic said.
fn said(failed: Box<dyn std::any::Any + Send>) -> String {
    failed.downcast::<String>().map(|s| *s).unwrap_or_else(|f| {
        f.downcast::<&str>()
            .map(|s| (*s).to_owned())
            .unwrap_or_default()
    })
}

#[test]
fn a_program_that_ends_during_a_wait_fails_the_wait_with_its_name_its_end_and_its_log() {
    let scratch = Scratch::new("dereth-network-ends");
    let hub = hub(&scratch.dir, "127.0.0.1:0", 0);
    wait_for(
        "the hub listening",
        &[&hub],
        || hub.saw(HUB_LISTENING),
        || hub.tail(20).join("\n"),
    );
    let mut crashed = false;
    let failed = catch_unwind(AssertUnwindSafe(|| {
        wait_within(
            GUARD,
            "what never comes",
            &[&hub],
            || {
                if !crashed {
                    hub.crash();
                    crashed = true;
                }
                false
            },
            || "what the run shows".to_owned(),
        );
    }));
    let message = said(failed.expect_err("the wait fails"));
    assert!(
        message.starts_with("what never comes: hub ended ("),
        "{message}"
    );
    assert!(message.contains(HUB_LISTENING), "its log: {message}");
    assert!(message.ends_with("what the run shows"), "{message}");
    assert!(!message.contains("it never came"), "{message}");
}

#[test]
fn a_program_that_loses_a_port_before_it_binds_it_is_started_again_on_fresh_ports() {
    let scratch = Scratch::new("dereth-network-lost-port");
    let dir = &scratch.dir;
    // The port chosen for the hub, taken by another program before the hub binds it.
    let taken = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("a port");
    let lost = taken.local_addr().expect("its port").port();
    let mut chosen = Vec::new();
    let settled = settle(
        hub(dir, &format!("127.0.0.1:{lost}"), 0),
        lost,
        HUB_LISTENING,
        &[],
        || {
            let p = second_game_ports();
            chosen.push(p);
            (hub(dir, &format!("127.0.0.1:{p}"), chosen.len()), p)
        },
    );
    assert_eq!(settled.starts, 2);
    assert_eq!(chosen, [settled.port]);
    assert_ne!(settled.port, lost);
    assert!(
        settled
            .process
            .saw(&format!("{HUB_LISTENING}{}", settled.port)),
        "{:?}",
        settled.process.tail(20)
    );
    settled.process.alive().expect("the hub serving");
}

#[test]
fn a_program_that_ends_at_its_start_for_anything_but_a_port_is_not_started_again() {
    let scratch = Scratch::new("dereth-network-bad-start");
    let dir = &scratch.dir;
    let mut again = 0;
    let failed = catch_unwind(AssertUnwindSafe(|| {
        settle(hub(dir, "no address", 0), 0, HUB_LISTENING, &[], || {
            again += 1;
            (hub(dir, "127.0.0.1:0", again), 0)
        })
    }));
    let message = said(failed.err().expect("the start fails"));
    assert!(
        message.starts_with("hub has its ports: it never did\nhub ended ("),
        "{message}"
    );
    assert_eq!(again, 0);
}
