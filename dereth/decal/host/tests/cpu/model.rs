//! Behaviour: none (a host inside another program: what its plugins hear of a recorded session)
//!
//! A plugin hears a session the same in this host, fed the session's messages through the C
//! interface as Decal hands them over, as in Dereth's own conformance driver, fed the same
//! session's datagrams: the probe plugin (`tests/plugins/probe`) reports each kind of event it heard
//! with its count and an order-sensitive digest, and the character and its pack as they last read
//! in the world; the two reports are equal for every recording that reaches the world once. The
//! driver follows only a recording's first choice of character, so one that logs in again is the
//! same up to its first logout, and this host follows the player in each time after. This host
//! also tells of the character list and of portal space, which it reads off the messages and the
//! conformance driver does not model: the comparison leaves the list out and runs without portal
//! space, and a test of its own checks both.

use dereth_client_sdk::net::client_session::testing::{Corpus, Direction};
use dereth_testkit::plugins::{read_package, PluginRun};

use crate::abi::{plugin_dir, settings, Replay};

/// The recordings that reach the world once. (Dereth's conformance driver follows a recording's
/// first character choice only; one that logs in again is checked on its own.)
const SESSIONS: &[&str] = &[
    "long-solo-play",
    "early-inventory-and-casting",
    "fellowship-one-vassal",
    "house-purchase-and-trade",
    "melee-attack-run",
    "short-play-with-training",
];

/// A recording whose player logs out and in again, five times.
const RELOGS: &str = "requested-death-vitae-salvage";

/// `0xF7C8 Login_SendEnterWorldRequest` and `0xF653 Login_ExecuteLogOff`, as the client sends them.
const ENTER_WORLD: u32 = 0xF7C8;
const LOG_OFF: u32 = 0xF653;

const PROBE: &str = "test.probe";

/// The kinds of event this host tells of and Dereth's conformance driver does not.
const HOST_FACTS: &[&str] = &["session.pregame"];

fn is_host_fact(line: &str) -> bool {
    HOST_FACTS
        .iter()
        .any(|k| line.split('\t').next() == Some(k))
}

/// The recording through this host, to its end; and the seconds of it after the entry. With
/// `portal_space`, the host tells of it as it reads it; without, it tells nothing of it, as
/// Dereth's conformance driver does not (and so takes nothing in it as the world settling).
fn through_the_interface(session: &str, portal_space: bool) -> (Replay, f64) {
    let s = settings(&[plugin_dir("probe")]).with("no_portal_space", !portal_space);
    let mut r = Replay::new(&s, session);
    r.frame();
    r.login();
    let end = Corpus::shared(session).blobs.last().map_or(0.0, |b| {
        #[allow(clippy::cast_precision_loss)] // microseconds in an hour's recording
        let t = b.t_rel_micros as f64;
        t / 1e6
    });
    let rest = (end - r.now).max(0.0) + 1.0;
    r.advance(rest);
    (r, rest)
}

/// The probe's report less what only this host tells of.
fn shared(console: &[String]) -> Vec<String> {
    report(console)
        .into_iter()
        .filter(|l| !is_host_fact(l))
        .collect()
}

/// The recording through Dereth's own driver, over `rest` seconds after the entry.
fn through_dereths_driver(session: &str, rest: f64) -> PluginRun {
    let mut run = PluginRun::new();
    run.load(read_package(&plugin_dir("probe")));
    run.login(session, None);
    run.advance(rest);
    run
}

/// The probe's last report: its console from the last line that names the character.
fn report(console: &[String]) -> Vec<String> {
    let from = console
        .iter()
        .rposition(|l| l.starts_with("player\t"))
        .unwrap_or(console.len());
    console[from..].to_vec()
}

/// The last `count` lines of a console.
fn last(console: &[String], count: usize) -> Vec<String> {
    console[console.len().saturating_sub(count)..].to_vec()
}

/// Where the probe's `kind` first differs between the two hosts: the line, and the first 200
/// characters of it that differ, as each host has them.
fn first_difference(
    here: &mut Replay,
    there: &mut PluginRun,
    kind: &str,
) -> Option<(String, String, String)> {
    for from in (1..100_000).step_by(40) {
        here.type_line(&format!("/probe digests {kind} {from} 40"));
        there.type_line(&format!("/probe digests {kind} {from} 40"));
        let a = last(&here.console(PROBE), 40);
        let b = last(&there.console(PROBE), 40);
        let Some((x, _)) = a.iter().zip(&b).find(|(x, y)| x != y) else {
            continue;
        };
        let line = x.split('\t').next().unwrap_or_default().to_owned();
        for off in (1..1_000_000).step_by(200) {
            here.type_line(&format!("/probe part {kind} {line} {off}"));
            there.type_line(&format!("/probe part {kind} {line} {off}"));
            let a = last(&here.console(PROBE), 1).concat();
            let b = last(&there.console(PROBE), 1).concat();
            if a != b {
                return Some((line, a, b));
            }
        }
    }
    None
}

#[test]
fn a_plugin_hears_every_recorded_session_as_it_hears_it_in_dereth() {
    for session in SESSIONS {
        let (mut here, rest) = through_the_interface(session, false);
        let mut there = through_dereths_driver(session, rest);
        assert!(here.type_line("/probe"), "the probe claims its command");
        there.type_line("/probe");
        let mine = report(&here.console(PROBE));
        let theirs = report(&there.console(PROBE));
        assert!(
            mine.first().is_some_and(|l| l.starts_with("player\t")),
            "{session}: the probe reports the character: {mine:?}"
        );
        let shared: Vec<&String> = mine.iter().filter(|l| !is_host_fact(l)).collect();
        let theirs: Vec<&String> = theirs.iter().collect();
        if shared != theirs {
            // Where the first kind that differs first differs.
            let kind = shared
                .iter()
                .zip(&theirs)
                .find(|(a, b)| a != b)
                .map(|(a, _)| a.split('\t').next().unwrap_or_default().to_owned())
                .unwrap_or_default();
            let first = first_difference(&mut here, &mut there, &kind);
            panic!(
                "{session}: this host against Dereth's driver:\n  here  {shared:?}\n  there {theirs:?}\n\
                 first {kind} line that differs: {first:?}"
            );
        }
    }
}

#[test]
fn this_host_tells_of_the_character_list_before_the_login_and_of_each_portal_space_ended() {
    let (mut here, _) = through_the_interface("long-solo-play", true);
    here.type_line("/probe");
    let report = report(&here.console(PROBE));
    let at = |kind: &str| {
        report
            .iter()
            .position(|l| l.starts_with(&format!("{kind}\t")))
    };
    let count = |kind: &str| -> Option<u32> {
        report
            .iter()
            .find(|l| l.starts_with(&format!("{kind}\t")))
            .and_then(|l| l.split('\t').nth(1)?.parse().ok())
    };
    assert!(
        at("session.pregame") < at("session.login"),
        "the list first: {report:?}"
    );
    assert!(
        count("session.portal_enter").is_some_and(|n| n > 0),
        "{report:?}"
    );
    assert_eq!(
        count("session.portal_enter"),
        count("session.portal_exit"),
        "every portal space ends: {report:?}"
    );
}

#[test]
fn this_host_follows_the_player_into_the_world_each_time_they_choose_a_character() {
    let blobs = &Corpus::shared(RELOGS).blobs;
    let sent = |opcode: u32| {
        blobs
            .iter()
            .filter(|b| b.dir == Direction::ClientToServer && b.opcode == opcode)
            .map(|b| {
                #[allow(clippy::cast_precision_loss)] // microseconds in an hour's recording
                let t = b.t_rel_micros as f64;
                t / 1e6
            })
            .collect::<Vec<_>>()
    };
    let entries = sent(ENTER_WORLD);
    let first_logout = sent(LOG_OFF)[0];
    // Up to the first logout, the same as in Dereth's driver.
    let s = settings(&[plugin_dir("probe")]).with("no_portal_space", true);
    let mut here = Replay::new(&s, RELOGS);
    here.frame();
    here.login();
    let rest = first_logout - here.now - 0.5;
    here.advance(rest);
    let mut there = through_dereths_driver(RELOGS, rest);
    here.type_line("/probe");
    there.type_line("/probe");
    assert_eq!(
        shared(&here.console(PROBE)),
        report(&there.console(PROBE)),
        "until the first logout"
    );
    // Then in again after each: the character read in the world after the last.
    let end = blobs.last().map_or(0.0, |b| {
        #[allow(clippy::cast_precision_loss)] // microseconds in an hour's recording
        let t = b.t_rel_micros as f64;
        t / 1e6
    });
    here.advance(end - here.now + 1.0);
    here.type_line("/probe");
    let report = report(&here.console(PROBE));
    let count = |kind: &str| -> Option<usize> {
        report
            .iter()
            .find(|l| l.starts_with(&format!("{kind}\t")))
            .and_then(|l| l.split('\t').nth(1)?.parse().ok())
    };
    assert!(entries.len() > 1, "the recording logs in again");
    assert_eq!(
        count("session.enter_world"),
        Some(entries.len()),
        "{report:?}"
    );
    let player: Vec<&str> = report[0].split('\t').collect();
    assert!(
        player
            .get(2)
            .and_then(|l| l.parse::<u32>().ok())
            .is_some_and(|l| l > 0),
        "the character read after the last entry: {report:?}"
    );
}
