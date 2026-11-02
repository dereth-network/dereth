//! A server whose ports another program holds stops before it starts, with a line a program reads
//! and an exit code of its own, so the program starting it can try other ports: a UDP port of the
//! login or the world, or the friends service's TCP port beside the login. Fixture: sockets of
//! this test holding one port each time, on loopback.

use std::net::{Ipv4Addr, TcpListener, UdpSocket};
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use yalain_testkit::authored;
use yalain_testkit::session as play;

use crate::session::{free_ports, ports_taken, scratch, Running, Turn, LIFE, PORTS_TAKEN, STARTS};

/// The server started on loopback, the login at `login` and the world at `world`: how it exited,
/// and its log.
fn exit_of(dir: &Path, login: u16, world: u16) -> (ExitStatus, String) {
    let install = auberean_dat::testing::require_target_install();
    let log = std::fs::File::create(dir.join("server.log")).expect("the server's log");
    // It reads the install before it binds, as any start does: it takes a turn.
    let _turn = Turn::take();
    let child = Command::new(env!("CARGO_BIN_EXE_yalain"))
        .arg("--dats")
        .arg(&install)
        .args(["--memory", "--bind", "127.0.0.1"])
        .args(["--login-port", &login.to_string()])
        .args(["--world-port", &world.to_string()])
        .arg("--world-db")
        .arg(authored::database_dir())
        .args(["--seconds", &LIFE.to_string()])
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(log.try_clone().expect("the log twice"))
        .stderr(log)
        .spawn()
        .expect("the server starts");
    let mut server = Running(child);
    let started = Instant::now();
    let exit = loop {
        if let Some(exit) = server.0.try_wait().expect("the server's state") {
            break exit;
        }
        assert!(
            started.elapsed().as_secs_f64() < play::HANG,
            "the server never stopped"
        );
        std::thread::sleep(Duration::from_millis(50));
    };
    (
        exit,
        std::fs::read_to_string(dir.join("server.log")).unwrap_or_default(),
    )
}

/// A socket bound by `bind` to an even port on loopback (a service's first port), and its port:
/// held by this test from the moment it is bound, so no other program can take it first. The
/// ports tried start beside one the system hands out (Linux hands out odd ones to a bind).
fn even<S>(bind: impl Fn(u16) -> std::io::Result<S>) -> (S, u16) {
    let from = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0))
        .and_then(|s| s.local_addr())
        .map_or(30_000, |a| a.port() & !1);
    for p in (from..u16::MAX - 1)
        .step_by(2)
        .chain((1_024..from).step_by(2))
    {
        if let Ok(s) = bind(p) {
            return (s, p);
        }
    }
    panic!("no even port on loopback");
}

/// A port of the world's held by another program, then the friends service's: each time the
/// server stops at once with exit code 3, its log's line saying which ports and ending in
/// `[ports-taken]`.
/// Behaviour: server.operator.ports-taken-said-with-its-own-exit-code
#[test]
fn a_server_whose_port_another_program_holds_says_so_and_exits_with_its_own_code() {
    let dir = scratch("yalain-live-ports-taken");

    let (world, world_port) = even(|p| UdpSocket::bind((Ipv4Addr::LOCALHOST, p)));
    let (exit, log) = exit_of(&dir, free_ports(), world_port);
    assert_eq!(exit.code(), Some(PORTS_TAKEN), "{exit}\n{log}");
    assert!(ports_taken(&log), "{log}");
    assert!(!log.contains("[listening]"), "{log}");
    drop(world);

    // The friends service's TCP port is the login's number; the login's UDP ports, not held here,
    // can be taken by another program first, and the server then stops before it reaches the
    // friends service: it is started again with another port held.
    for start in 1..=STARTS {
        let (friends, login) = even(|p| TcpListener::bind((Ipv4Addr::LOCALHOST, p)));
        let (exit, log) = exit_of(&dir, login, free_ports());
        drop(friends);
        if !log.contains("friends service") && start < STARTS {
            continue;
        }
        assert_eq!(exit.code(), Some(PORTS_TAKEN), "{exit}\n{log}");
        assert!(ports_taken(&log), "{log}");
        assert!(log.contains("friends service"), "{log}");
        return;
    }
}
