//! The executable reporting itself to a Dereth Network hub: a stand-in hub on loopback takes each
//! heartbeat it posts and answers it. Each step waits for the next heartbeat, with a time limit
//! only against a hang. Fixture: the built executable, the install, the authored world and a key
//! file in a scratch folder.

use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, UdpSocket};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::Instant;

use yalain_testkit::authored;
use yalain_testkit::session::{self as play};

use crate::session::{free_ports, must, scratch, Loopback, Running};

const KEY: &str = "drth_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

/// One heartbeat the stand-in hub took: its request line, its authorization and its body.
struct Beat {
    line: String,
    authorization: String,
    body: serde_json::Value,
}

/// A stand-in hub on loopback that takes every post, answers it `200`, and hands it on.
fn stand_in_hub() -> (u16, Receiver<Beat>) {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("a stand-in hub");
    let port = listener.local_addr().expect("its port").port();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().expect("the stream"));
            let mut line = String::new();
            let _ = reader.read_line(&mut line);
            let (mut length, mut authorization) = (0, String::new());
            loop {
                let mut h = String::new();
                if reader.read_line(&mut h).unwrap_or(0) == 0 || h.trim_end().is_empty() {
                    break;
                }
                if let Some((k, v)) = h.trim_end().split_once(':') {
                    match k.trim().to_ascii_lowercase().as_str() {
                        "content-length" => length = v.trim().parse().unwrap_or(0),
                        "authorization" => v.trim().clone_into(&mut authorization),
                        _ => {}
                    }
                }
            }
            let mut body = vec![0; length];
            let _ = reader.read_exact(&mut body);
            let _ = stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}");
            let beat = Beat {
                line: line.trim_end().to_owned(),
                authorization,
                body: serde_json::from_slice(&body).unwrap_or_default(),
            };
            if tx.send(beat).is_err() {
                return;
            }
        }
    });
    (port, rx)
}

/// The executable, reporting to the hub on `hub`, with its console on a pipe.
fn start_server(dir: &Path, login: u16, hub: u16) -> (Child, ChildStdin) {
    let install = auberean_dat::testing::require_target_install();
    let key = dir.join("hub.key");
    std::fs::write(&key, format!("{KEY}\n")).expect("the key file");
    let log = std::fs::File::create(dir.join("server.log")).expect("the server's log");
    let mut child = Command::new(env!("CARGO_BIN_EXE_yalain"))
        .arg("--dats")
        .arg(&install)
        .args(["--memory", "--login-port", &login.to_string()])
        .args(["--world-port", &(login + 2).to_string()])
        .arg("--world-db")
        .arg(authored::database_dir())
        .args(["--hub", &format!("http://127.0.0.1:{hub}")])
        .args(["--hub-slug", "shardfall", "--hub-key-file"])
        .arg(&key)
        .args(["--seconds", &crate::session::LIFE.to_string()])
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(log.try_clone().expect("the log twice"))
        .stderr(log)
        .spawn()
        .expect("the server starts");
    let console = child.stdin.take().expect("the console");
    (child, console)
}

/// The next heartbeat, within a limit that only guards against a hang.
fn next(beats: &Receiver<Beat>, what: &str, dir: &Path) -> Beat {
    must(
        beats
            .recv_timeout(crate::network::HANG)
            .map_err(|e| format!("{what}: {e}")),
        dir,
    )
}

/// The executable told to report to a hub posts its first heartbeat at once, as an Asheron's Call
/// 2 world with no one in it, under its slug and with its key; when its operator starts a
/// shutdown, it posts at once that it is in maintenance, counting the character in the world.
/// Behaviour: server.operator.reported-to-the-hub
#[test]
fn the_server_reports_itself_to_a_hub_as_an_auberean_world_with_the_characters_in_it() {
    let dir = scratch("yalain-live-hub");
    let (hub, beats) = stand_in_hub();
    let port = free_ports();
    let (child, mut console) = start_server(&dir, port, hub);
    let _server = Running(child);

    let first = next(&beats, "the first heartbeat", &dir);
    assert_eq!(first.line, "POST /v1/servers/shardfall/heartbeat HTTP/1.1");
    assert_eq!(first.authorization, format!("Bearer {KEY}"));
    assert_eq!(first.body["state"], "online");
    assert_eq!(first.body["playersOnline"], 0);
    assert_eq!(first.body["playersKind"], "connected-characters");
    assert_eq!(first.body["game"], "auberean");
    assert!(first.body["emulatorVersion"].is_string(), "{}", first.body);

    let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).expect("a client socket");
    socket.set_nonblocking(true).expect("nonblocking");
    let mut s = Loopback {
        socket,
        client: play::client(
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port),
            "reporter",
        ),
        start: Instant::now(),
        buf: vec![0; 65_536],
    };
    must(play::character_list(&mut s), &dir);
    let me = must(play::create(&mut s, "Beaconkeeper"), &dir);
    must(play::enter(&mut s, me), &dir);

    console.write_all(b"shutdown\n").expect("the console");
    console.flush().expect("the console");
    let counting = loop {
        let beat = next(&beats, "the heartbeat of the shutdown", &dir);
        if beat.body["state"] != "online" {
            break beat;
        }
    };
    assert_eq!(counting.body["state"], "maintenance");
    assert_eq!(counting.body["playersOnline"], 1);
    assert_eq!(counting.body["game"], "auberean");
}
