//! Stopping the server's executable at once with a player in the world: typed `stop` at its
//! console, the console's input ended for a server told to stop then, or (on Linux and macOS)
//! the service manager's SIGTERM. Each way the character is kept where it walked to before the
//! server exits. A server not told to stop at the console's end plays on without its console.
//! Stopping to update: the built executable, in a temporary installation, takes a patch release of
//! itself from a stand-in repository on loopback, counts down, stops, installs it, passes its health
//! check and starts it; `update --check` reads the same repository. Nothing here touches a real
//! installation or the network beyond this machine.
//! Fixture: the authored world, a database file in a scratch folder, read back after the server
//! has gone.

use std::io::{Read as _, Write as _};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream, UdpSocket};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::time::{Duration, Instant};

use dereth_primitives::num::math;
use dereth_server_update::release::target_triple;
use dereth_server_update::testing::{Release, Repository};
use serde_json::json;
use yalain_store::{SqliteStore, Store as _};
use yalain_testkit::authored;
use yalain_testkit::session::{self as play, Drive};

use crate::session::{free_ports, must, scratch, serving, Loopback, Running};

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

const EXE: &str = std::env::consts::EXE_SUFFIX;
/// What the release declares it upgrades from.
const UPGRADES_FROM: &str = "0.0.1";

/// The version the running server is taken for: a pre-release of this build's version, so this
/// build's version is a newer patch release of the same minor line.
fn older(version: &str) -> String {
    format!("{version}-rc.1")
}

/// The built executable's facts, as it prints them.
fn built_facts() -> serde_json::Value {
    let out = Command::new(env!("CARGO_BIN_EXE_yalain"))
        .arg("--release-facts")
        .output()
        .expect("it runs");
    serde_json::from_slice(&out.stdout).expect("JSON")
}

/// The release this build is, or is a pre-release of: the updater never installs a pre-release, so
/// a tree that carries one is offered the release itself, and refuses it at its health check.
fn release_of_this_build() -> String {
    let facts = built_facts();
    let version = facts["version"].as_str().expect("a version");
    version.split('-').next().expect("a version").to_owned()
}

/// A repository holding this build's release as a patch release, its program this very build.
fn repository_of_this_build() -> Repository {
    let facts = built_facts();
    let version = release_of_this_build();
    let repository = Repository::start("yalain", "yalain-v");
    let program = std::fs::read(env!("CARGO_BIN_EXE_yalain")).expect("the built program");
    repository.publish(
        &Release {
            version: &version,
            files: vec![
                (format!("yalain{EXE}"), program),
                ("README.md".to_owned(), b"new guide".to_vec()),
            ],
            target: &target_triple(option_env!("YALAIN_BUILD_TARGET")),
            other_targets: &[],
            upgrade: json!({
                "kind": "patch",
                "previous": UPGRADES_FROM,
                "upgrades_from": UPGRADES_FROM,
                "databases": facts["databases"],
                "migrations": [],
                "world_database": facts["world_database"],
                "world_database_fetch": false,
                "config": [],
                "notes": "",
            }),
            prerelease: false,
        },
        |s| s,
    );
    repository
}

/// A temporary installation holding a copy of the built program.
fn installation(dir: &Path) -> PathBuf {
    let exe = dir.join(format!("yalain{EXE}"));
    std::fs::copy(env!("CARGO_BIN_EXE_yalain"), &exe).expect("the program copied");
    std::fs::write(dir.join("README.md"), b"old guide").expect("the guide");
    exe
}

/// Waits for `text` in the log `from` on; the place after it.
fn wait_for(dir: &Path, server: &mut Running, text: &str, from: usize) -> usize {
    wait_unless(dir, server, text, None, from)
}

/// [`wait_for`], failing at once should the server say `instead` first.
fn wait_unless(
    dir: &Path,
    server: &mut Running,
    text: &str,
    instead: Option<&str>,
    from: usize,
) -> usize {
    let started = Instant::now();
    loop {
        let log = std::fs::read_to_string(dir.join("server.log")).unwrap_or_default();
        if let Some(at) = log.get(from..).and_then(|rest| rest.find(text)) {
            return from + at + text.len();
        }
        if let Some(instead) = instead {
            assert!(
                !log.get(from..).unwrap_or_default().contains(instead),
                "the server said {instead:?} instead of {text:?}\n{log}"
            );
        }
        assert!(
            server.0.try_wait().expect("its state").is_none(),
            "the server stopped before it said {text:?}\n{log}"
        );
        assert!(
            started.elapsed().as_secs_f64() < play::HANG,
            "the server never said {text:?}\n{log}"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// A server taken for an older version counts down, stops, installs this build's version, passes
/// the health check and restarts into it in the same process, answering a stop afterwards. Its
/// status address is free for the health check and then for the new server, which answers there.
/// Behaviour: server.update.running-server-restarts-into-the-release
#[test]
fn the_running_server_counts_down_installs_and_restarts_into_a_patch_release() {
    let dir = scratch("yalain-live-updating");
    let exe = installation(&dir);
    let repository = repository_of_this_build();
    let version = release_of_this_build();
    let older = older(&version);
    let install = auberean_dat::testing::require_target_install();
    let login = free_ports();
    let status = SocketAddr::from((Ipv4Addr::LOCALHOST, free_ports()));
    let log = std::fs::File::create(dir.join("server.log")).expect("the server's log");
    let child = Command::new(&exe)
        .arg("--dats")
        .arg(&install)
        .arg("--data")
        .arg(dir.join("yalain.sqlite"))
        .args(["--login-port", &login.to_string()])
        .args(["--world-port", &(login + 2).to_string()])
        .args(["--status", &status.to_string()])
        .arg("--world-db")
        .arg(authored::database_dir())
        .args(["--seconds", &crate::session::LIFE.to_string()])
        .args(["--update", "patch", "--update-warning-seconds", "1"])
        .args(["--update-source", &repository.api()])
        .args(["--update-test-version", &older])
        .current_dir(&dir)
        .stdin(Stdio::piped())
        .stdout(log.try_clone().expect("the log twice"))
        .stderr(log)
        .spawn()
        .expect("the server starts");
    let mut server = Running(child);
    let pid = server.0.id();
    let mut console = server.0.stdin.take().expect("its console");

    let at = wait_for(
        &dir,
        &mut server,
        &format!("Update: taken for version {older}"),
        0,
    );
    // A tree that carries a pre-release: the program offered as the release says it is the
    // pre-release, so the health check refuses it and the server plays on, nothing changed. A
    // release's own tree goes on through the whole update.
    let built = built_facts()["version"]
        .as_str()
        .expect("a version")
        .to_owned();
    if built != version {
        wait_for(
            &dir,
            &mut server,
            &format!("server reports version {built}"),
            at,
        );
        assert!(
            server.0.try_wait().expect("its state").is_none(),
            "it plays on"
        );
        assert_eq!(
            std::fs::read(dir.join("README.md")).expect("it"),
            b"old guide"
        );
        return;
    }
    let at = wait_for(
        &dir,
        &mut server,
        &format!("Update: {version} is staged; restarting into it after a 1 s shutdown countdown"),
        at,
    );
    let at = wait_for(&dir, &mut server, "the world shuts down in 1 seconds", at);
    let at = wait_unless(
        &dir,
        &mut server,
        &format!("Update: {version} passed its health check"),
        Some("Rolling back to"),
        at,
    );
    let at = wait_for(
        &dir,
        &mut server,
        &format!("Update: starting {version}"),
        at,
    );
    wait_for(&dir, &mut server, "friends service listening on TCP", at);

    assert_eq!(server.0.id(), pid, "the same process on");
    let mut asked = TcpStream::connect(status).expect("the new server listens at the address");
    write!(asked, "GET /status HTTP/1.1\r\nHost: x\r\n\r\n").expect("asked");
    let mut answer = String::new();
    asked.read_to_string(&mut answer).expect("an answer");
    assert!(
        answer.contains(&format!("\"version\":\"{version}\"")),
        "{answer}"
    );
    assert_eq!(
        std::fs::read(dir.join("README.md")).expect("it"),
        b"new guide"
    );
    assert!(
        dir.join(format!("yalain{EXE}.previous-v{older}")).is_file(),
        "the old program kept aside"
    );
    assert!(
        std::fs::read_dir(&dir)
            .expect("the folder")
            .flatten()
            .any(|e| e
                .file_name()
                .to_string_lossy()
                .starts_with(&format!("yalain.sqlite.backup-update-v{older}-v{version}-"))),
        "the database backed up first"
    );

    writeln!(console, "stop").expect("typed");
    console.flush().expect("typed");
    let started = Instant::now();
    let exit = loop {
        if let Some(exit) = server.0.try_wait().expect("its state") {
            break exit;
        }
        assert!(
            started.elapsed().as_secs_f64() < play::HANG,
            "it never stopped"
        );
        std::thread::sleep(Duration::from_millis(50));
    };
    assert!(exit.success(), "{exit}");
}

/// `yalain update --check`, with no install or world in sight, says what it would install from the
/// source its configuration file names, in Empyrean's words.
/// Behaviour: server.update.check-by-hand
#[test]
fn update_check_reads_the_source_the_configuration_names_and_says_what_it_would_do() {
    let dir = scratch("yalain-live-update-check");
    let exe = installation(&dir);
    let repository = Repository::start("yalain", "yalain-v");
    let config = dir.join("yalain.toml");
    std::fs::write(
        &config,
        format!(
            "[server]\nupdate = \"patch\"\nupdate_source = \"{}\"\n",
            repository.api()
        ),
    )
    .expect("the configuration");
    let out = Command::new(&exe)
        .args(["update", "--check", "--config"])
        .arg(&config)
        .current_dir(&dir)
        .output()
        .expect("it runs");
    assert!(out.status.success(), "{out:?}");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("server.update = \"patch\""), "{text}");
    assert!(text.contains("This server: Yalain "), "{text}");
    assert!(text.contains("No newer release is published."), "{text}");
    assert!(text.contains("Nothing would be installed."), "{text}");
    assert!(
        !repository.web.asked.lock().expect("the list").is_empty(),
        "it asked the stand-in"
    );

    let usage = Command::new(&exe).arg("update").output().expect("it runs");
    assert_eq!(usage.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&usage.stderr).contains("give --check or --apply"),
        "{usage:?}"
    );
}
