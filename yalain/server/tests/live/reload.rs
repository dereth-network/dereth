//! The operator's `reload` typed into the running executable's console: a database that does not
//! compile leaves the world as it was; one that does changes what the guard says to a player
//! already in the world, and reports the guard's new place as waiting for a restart. Fixture: a
//! copy of the authored world's database in a scratch folder, edited between reloads.

use std::io::Write as _;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::time::Instant;

use yalain_testkit::authored;
use yalain_testkit::session::{self as play, call, Drive};

use crate::session::{free_ports, must, scratch, Loopback, Running};

/// The guard's lines, relative to the database's folder.
const GUARD_TALK: &str = "hall/server/talk/47002DFD.toml";
/// The hall's placed objects.
const HALL_PLACES: &str = "hall/server/landblocks/0298.toml";
/// A line the guard says after the reload (a string of the guard's table).
const NEW_LINE: u32 = 0x6369_1D90;

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("a folder");
    for e in std::fs::read_dir(from).expect("the database").flatten() {
        let p = e.path();
        let target = to.join(e.file_name());
        if p.is_dir() {
            copy_dir(&p, &target);
        } else {
            std::fs::copy(&p, &target).expect("a file copied");
        }
    }
}

/// The executable serving the database in `db`, with its console on a pipe.
fn start_server(dir: &Path, db: &Path, login: u16) -> (Child, ChildStdin) {
    let install = auberean_dat::testing::require_target_install();
    let log = std::fs::File::create(dir.join("server.log")).expect("the server's log");
    let mut child = Command::new(env!("CARGO_BIN_EXE_yalain"))
        .arg("--dats")
        .arg(&install)
        .args(["--memory", "--login-port", &login.to_string()])
        .args(["--world-port", &(login + 2).to_string()])
        .arg("--world-db")
        .arg(db)
        .args(["--seconds", "300"])
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(log.try_clone().expect("the log twice"))
        .stderr(log)
        .spawn()
        .expect("the server starts");
    let console = child.stdin.take().expect("the console");
    (child, console)
}

fn edit(db: &Path, file: &str, from: &str, to: &str) {
    let p = db.join(file);
    let text = std::fs::read_to_string(&p).expect("the file");
    assert!(text.contains(from), "{file} holds {from}");
    std::fs::write(&p, text.replacen(from, to, 1)).expect("written");
}

fn logged(dir: &Path, needle: &str) -> usize {
    std::fs::read_to_string(dir.join("server.log"))
        .unwrap_or_default()
        .matches(needle)
        .count()
}

/// Type `reload` and keep the client going until the log says how it went, then that line.
fn reload(s: &mut Loopback, console: &mut ChildStdin, dir: &Path) -> Result<String, String> {
    let done = |dir: &Path| {
        logged(dir, "did not compile; the world is unchanged") + logged(dir, ": reloaded: ")
    };
    let before = done(dir);
    writeln!(console, "reload").map_err(|e| e.to_string())?;
    console.flush().map_err(|e| e.to_string())?;
    let started = Instant::now();
    while done(dir) == before {
        if started.elapsed().as_secs() > 120 {
            return Err("the reload never answered".into());
        }
        s.step();
    }
    let log = std::fs::read_to_string(dir.join("server.log")).unwrap_or_default();
    Ok(log
        .lines()
        .rev()
        .find(|l| l.contains("did not compile") || l.contains(": reloaded: "))
        .unwrap_or_default()
        .to_string())
}

/// Use the guard; the string ids among the words of the tell it answers with.
fn guard_says(s: &mut Loopback, guard: u64, strings: &[u32]) -> Result<Vec<u32>, String> {
    let from = s.client_ref().observed.len();
    play::talk_to(s, guard)?;
    let tells: Vec<&[u8]> = s.client_ref().observed[from..]
        .iter()
        .filter(|(_, f, _, _)| *f == call::HEAR_TELL)
        .filter_map(|(_, _, a, _)| a.as_ref().map(|a| a.0.as_slice()))
        .collect();
    Ok(strings
        .iter()
        .copied()
        .filter(|id| {
            tells
                .iter()
                .any(|t| t.windows(4).any(|w| w == id.to_le_bytes()))
        })
        .collect())
}

/// The executable, serving a copy of the authored world, reloads it from its console while a
/// player of our client is in the world: a broken database changes nothing and says so; a
/// database where the guard has a new line and a new place gives the player the new line on the
/// next use, and reports the place as needing a restart.
/// Behaviour: server.operator.reload-from-the-console
#[test]
fn reload_at_the_console_changes_the_running_world_or_nothing() {
    let dir = scratch("yalain-live-reload");
    let db = dir.join("world");
    copy_dir(&authored::database_dir(), &db);
    let port = free_ports();
    let (child, mut console) = start_server(&dir, &db, port);
    let _server = Running(child);
    let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).expect("a client socket");
    socket.set_nonblocking(true).expect("nonblocking");
    let login = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    let mut s = Loopback {
        socket,
        client: play::client(login, "live"),
        start: Instant::now(),
        buf: vec![0; 65_536],
    };
    must(play::character_list(&mut s), &dir);
    let me = must(play::create(&mut s, "Reloader"), &dir);
    must(play::enter(&mut s, me), &dir);
    must(play::see(&mut s, &[authored::GUARD]), &dir);
    let guard = play::find(s.client_ref(), authored::GUARD).expect("the guard");
    let at = guard.place.expect("placed").origin;
    must(play::walk_to(&mut s, [at.x, at.y, at.z], 2.0), &dir);
    let lines = [authored::GUARD_LINE.0, NEW_LINE];
    assert_eq!(
        must(guard_says(&mut s, guard.id, &lines), &dir),
        [authored::GUARD_LINE.0],
        "the line before"
    );

    // A database that does not read: nothing changes.
    let talk = std::fs::read_to_string(db.join(GUARD_TALK)).expect("the guard's lines");
    std::fs::write(db.join(GUARD_TALK), "[[talk]\n").expect("broken");
    let said = must(reload(&mut s, &mut console, &dir), &dir);
    assert!(said.contains("the world is unchanged"), "{said}");
    assert_eq!(
        must(guard_says(&mut s, guard.id, &lines), &dir),
        [authored::GUARD_LINE.0],
        "the line as it was"
    );

    // A new line and a new place: the line now, the place after a restart.
    std::fs::write(db.join(GUARD_TALK), talk).expect("restored");
    edit(
        &db,
        GUARD_TALK,
        "string = 0x63691D8F }\nline_name = \"ID_GoAway\"",
        &format!("string = {NEW_LINE:#X} }}"),
    );
    edit(&db, HALL_PLACES, "at = [52.16,", "at = [53.16,");
    let said = must(reload(&mut s, &mut console, &dir), &dir);
    assert!(
        said.contains("reloaded: talk; needs a restart: placed objects;"),
        "{said}"
    );
    assert_eq!(
        must(guard_says(&mut s, guard.id, &lines), &dir),
        [NEW_LINE],
        "the new line"
    );
    must(play::log_off(&mut s), &dir);
}
