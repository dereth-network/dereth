//! Stopping the server's executable at once with a player in the world: typed `stop` at its
//! console, the console's input ended for a server told to stop then, or (on Linux and macOS)
//! the service manager's SIGTERM. Each way the character is kept where it walked to before the
//! server exits. A server not told to stop at the console's end plays on without its console.
//! Fixture: the authored world, a database file in a scratch folder, read back after the server
//! has gone.

use std::io::Write as _;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::time::Instant;

use dereth_primitives::num::math;
use yalain_store::{SqliteStore, Store as _};
use yalain_testkit::authored;
use yalain_testkit::session::{self as play, Drive};

use crate::session::{must, scratch, serving, Loopback};

/// How the test stops the server.
#[derive(Clone, Copy)]
enum Stop {
    /// `stop` typed at the console.
    Console,
    /// The console's input ended, the server told to stop then.
    ConsoleEnds,
    /// The system's SIGTERM.
    #[cfg(unix)]
    Terminate,
}

/// The executable keeping its accounts and characters in `dir`'s database file, its console on a
/// pipe (or no console), with `more` options.
fn start_server(dir: &Path, login: u16, more: &[&str], console: Stdio) -> Child {
    let install = auberean_dat::testing::require_target_install();
    let log = std::fs::File::create(dir.join("server.log")).expect("the server's log");
    Command::new(env!("CARGO_BIN_EXE_yalain"))
        .arg("--dats")
        .arg(&install)
        .arg("--data")
        .arg(dir.join("yalain.sqlite"))
        .args(["--login-port", &login.to_string()])
        .args(["--world-port", &(login + 2).to_string()])
        .arg("--world-db")
        .arg(authored::database_dir())
        .args(["--seconds", &crate::session::LIFE.to_string()])
        .args(more)
        .current_dir(dir)
        .stdin(console)
        .stdout(log.try_clone().expect("the log twice"))
        .stderr(log)
        .spawn()
        .expect("the server starts")
}

/// The server's log so far.
fn log_of(dir: &Path) -> String {
    std::fs::read_to_string(dir.join("server.log")).unwrap_or_default()
}

/// A player of account `account` on loopback, its character `name` made and in the world.
fn player_in_world(dir: &Path, port: u16, account: &str, name: &str) -> (Loopback, u64) {
    let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).expect("a client socket");
    socket.set_nonblocking(true).expect("nonblocking");
    let login = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    let mut s = Loopback {
        socket,
        client: play::client(login, account),
        start: Instant::now(),
        buf: vec![0; 65_536],
    };
    must(play::character_list(&mut s), dir);
    let me = must(play::create(&mut s, name), dir);
    must(play::enter(&mut s, me), dir);
    (s, me)
}

/// A player walks away from where it entered, then the server is stopped as `how` says: the
/// server exits by itself, saying it kept one character, and the database holds the character
/// where it walked to.
fn stopped_server_keeps_the_walker(name: &str, how: Stop) {
    let dir = scratch(&format!("yalain-live-stopping-{name}"));
    let more: &[&str] = match how {
        Stop::ConsoleEnds => &["--stop-at-console-end"],
        _ => &[],
    };
    let (port, mut server, ()) = serving(&dir, |port| {
        (start_server(&dir, port, more, Stdio::piped()), ())
    });
    let mut console: Option<ChildStdin> = server.0.stdin.take();
    let (mut s, me) = player_in_world(&dir, port, "stopper", name);
    let entered = s.client_ref().body.origin;
    must(play::see(&mut s, &[authored::GUARD]), &dir);
    let guard = play::find(s.client_ref(), authored::GUARD).expect("the guard");
    let at = guard.place.expect("placed").origin;
    must(play::walk_to(&mut s, [at.x, at.y, at.z], 2.0), &dir);
    play::wait(&mut s, 1.0);
    let walked = s.client_ref().body.origin;
    assert!(
        math::hypotf(walked.x - entered.x, walked.y - entered.y) > 2.0,
        "it walked away from where it entered"
    );

    match how {
        Stop::Console => {
            let console = console.as_mut().expect("the console");
            writeln!(console, "stop").expect("typed");
            console.flush().expect("typed");
        }
        Stop::ConsoleEnds => drop(console.take()),
        #[cfg(unix)]
        Stop::Terminate => {
            let sent = Command::new("kill")
                .args(["-TERM", &server.0.id().to_string()])
                .status()
                .expect("kill runs");
            assert!(sent.success(), "SIGTERM sent");
        }
    }
    let started = Instant::now();
    let exit = loop {
        if let Some(exit) = server.0.try_wait().expect("the server's state") {
            break exit;
        }
        assert!(
            started.elapsed().as_secs_f64() < play::HANG,
            "the server never stopped"
        );
        s.step();
    };
    let log = log_of(&dir);
    assert!(exit.success(), "a clean exit: {exit}\n{log}");
    assert!(
        log.contains("stopping: 1 characters in the world kept"),
        "{log}"
    );
    // The lines a program reads, in their fixed form.
    let tagged = |words: &str, tag: &str| {
        log.lines()
            .any(|l| l.contains(&format!(" yalain: {words}")) && l.ends_with(&format!(" [{tag}]")))
    };
    assert!(
        tagged("in the world at ", &format!("entered \"{name}\" {me:#x}")),
        "{log}"
    );
    assert!(tagged("listening on ", "listening"), "{log}");
    match how {
        Stop::Console => assert!(tagged("stopping now", "ok stop-now"), "{log}"),
        Stop::ConsoleEnds => {
            assert!(log.contains("stopping: the console's input ended"), "{log}");
        }
        #[cfg(unix)]
        Stop::Terminate => {}
    }

    let mut store = SqliteStore::open(&dir.join("yalain.sqlite")).expect("the database opens");
    let kept = store
        .world_state(me)
        .expect("reads")
        .and_then(|st| st.position)
        .expect("kept with a position");
    let d = math::hypotf(kept.origin[0] - walked.x, kept.origin[1] - walked.y);
    assert!(
        d < 1.0,
        "kept where it walked to ({d} m off): {kept:?}\n{log}"
    );
}

/// `stop` typed at the console stops the server at once, every character in the world kept first.
/// Behaviour: server.operator.stop-and-exit-at-the-console
#[test]
fn stop_at_the_console_keeps_the_character_where_it_walked_and_exits() {
    stopped_server_keeps_the_walker("Stopwalker", Stop::Console);
}

/// The service manager's SIGTERM stops the server at once, every character in the world kept first.
/// Behaviour: server.operator.stopping-keeps-everyone-in-the-world
#[cfg(unix)]
#[test]
fn sigterm_keeps_the_character_where_it_walked_and_exits() {
    stopped_server_keeps_the_walker("Termwalker", Stop::Terminate);
}

/// A server told to stop when its console's input ends (a program holding the console has gone)
/// stops at once, every character in the world kept first.
/// Behaviour: server.operator.stop-when-the-console-ends
#[test]
fn the_console_ending_stops_a_server_told_to_stop_then_keeping_the_character() {
    stopped_server_keeps_the_walker("Endwalker", Stop::ConsoleEnds);
}

/// A server with no console (its input ended at once, as a service's is) and not told to stop at
/// the console's end plays on: a player logs in and enters after the console has ended, and the
/// server is still running with it in the world.
/// Behaviour: server.operator.stop-when-the-console-ends
#[test]
fn a_server_not_told_to_stop_plays_on_when_its_console_ends() {
    let dir = scratch("yalain-live-stopping-plays-on");
    let (port, mut server, ()) = serving(&dir, |port| {
        (start_server(&dir, port, &[], Stdio::null()), ())
    });
    let started = Instant::now();
    while !log_of(&dir).contains("the console's input ended") {
        assert!(
            started.elapsed().as_secs_f64() < play::HANG,
            "the console never ended\n{}",
            log_of(&dir)
        );
        assert!(
            server.0.try_wait().expect("the server's state").is_none(),
            "the server stopped\n{}",
            log_of(&dir)
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let (mut s, me) = player_in_world(&dir, port, "player", "Playson");
    play::wait(&mut s, 1.0);
    assert!(
        server.0.try_wait().expect("the server's state").is_none(),
        "still running\n{}",
        log_of(&dir)
    );
    let log = log_of(&dir);
    assert!(
        log.contains(&format!(" [entered \"Playson\" {me:#x}]")),
        "{log}"
    );
    assert!(!log.contains("stopping"), "{log}");
}
