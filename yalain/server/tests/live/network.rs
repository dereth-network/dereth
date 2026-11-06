//! The executable linked to the Dereth Network: a stand-in hub on loopback takes its heartbeats
//! and its link on one port, welcomes it, and hands it another world's tell and watch. Each step
//! waits for the next frame, with a time limit only against a hang. Fixture: the built
//! executable, the install, the authored world and a key file in a scratch folder.

use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use dereth_network_protocol::limits::Limits;
use dereth_network_protocol::wire::PROTOCOL;
use dereth_network_protocol::{Address, Message, Outcome, WorldInfo};
use tungstenite::handshake::server::{Request, Response};
use tungstenite::WebSocket;
use yalain_testkit::authored;

use crate::session::{must, scratch, serving, LIFE};

pub(crate) const KEY: &str = "drth_BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB";

/// A guard against a hang, not the time a step is expected to take.
pub(crate) const HANG: Duration = Duration::from_secs(900);

/// A link the stand-in hub took: the path and the authorization it was opened with.
pub(crate) struct Linked {
    pub(crate) path: String,
    pub(crate) authorization: String,
    pub(crate) ws: WebSocket<TcpStream>,
}

/// A stand-in hub on loopback: every heartbeat is answered `200`; every link is upgraded and
/// handed on.
pub(crate) fn stand_in_hub() -> (u16, Receiver<Linked>) {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("a stand-in hub");
    let port = listener.local_addr().expect("its port").port();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let mut first = [0u8; 4];
            if stream.peek(&mut first).is_err() {
                continue;
            }
            if &first != b"GET " {
                answer_heartbeat(stream);
                continue;
            }
            let (mut path, mut authorization) = (String::new(), String::new());
            // The callback's error type is the handshake library's own, large by its design.
            #[allow(clippy::result_large_err)]
            let upgraded = tungstenite::accept_hdr(stream, |req: &Request, res: Response| {
                req.uri().path().clone_into(&mut path);
                if let Some(v) = req.headers().get("authorization") {
                    v.to_str().unwrap_or("").clone_into(&mut authorization);
                }
                Ok(res)
            });
            let Ok(ws) = upgraded else { continue };
            if tx
                .send(Linked {
                    path,
                    authorization,
                    ws,
                })
                .is_err()
            {
                return;
            }
        }
    });
    (port, rx)
}

/// Reads one heartbeat post and answers it.
fn answer_heartbeat(mut stream: TcpStream) {
    let mut reader = BufReader::new(stream.try_clone().expect("the stream"));
    let mut length = 0;
    loop {
        let mut h = String::new();
        if reader.read_line(&mut h).unwrap_or(0) == 0 || h.trim_end().is_empty() {
            break;
        }
        if let Some((k, v)) = h.trim_end().split_once(':') {
            if k.trim().eq_ignore_ascii_case("content-length") {
                length = v.trim().parse().unwrap_or(0);
            }
        }
    }
    let mut body = vec![0; length];
    let _ = reader.read_exact(&mut body);
    let _ =
        stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}");
}

/// The executable, on the network through the hub on `hub`, with its console on a pipe.
pub(crate) fn start_server(dir: &Path, login: u16, hub: u16) -> (Child, ChildStdin) {
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
        .arg("--federation")
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

/// The next link message the world sends, skipping the transport's own frames.
pub(crate) fn next(ws: &mut WebSocket<TcpStream>, what: &str) -> Result<Message, String> {
    let until = Instant::now() + HANG;
    while Instant::now() < until {
        match ws.read() {
            Ok(tungstenite::Message::Text(t)) => {
                return Message::from_json(t.as_str()).map_err(|e| format!("{what}: {e:?}"));
            }
            Ok(_) => {}
            Err(tungstenite::Error::Io(e))
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(e) => return Err(format!("{what}: {e}")),
        }
    }
    Err(format!("{what}: nothing came"))
}

pub(crate) fn send(ws: &mut WebSocket<TcpStream>, m: &Message) {
    ws.send(tungstenite::Message::text(m.to_json()))
        .expect("the hub sends");
}

/// The server's log holds `line`, within a limit that only guards against a hang.
fn logged(dir: &Path, line: &str) -> Result<(), String> {
    let until = Instant::now() + HANG;
    while Instant::now() < until {
        let log = std::fs::read_to_string(dir.join("server.log")).unwrap_or_default();
        if log.contains(line) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err(format!("the log never said {line:?}"))
}

/// The executable on the network links to its hub with the world's key and says `hello` first;
/// once welcomed, another world's tell to it is answered and its watch answered, both as for a
/// character not present, and the console's `dn` shows the hub
/// connected and blocks a world.
/// Behaviour: server.network.linked-to-a-hub
#[test]
fn the_server_links_to_its_hub_and_answers_another_worlds_tell_and_watch() {
    let dir = scratch("yalain-live-network");
    let (hub, links) = stand_in_hub();
    let (_, _server, mut console) = serving(&dir, |port| start_server(&dir, port, hub));

    let mut link = must(
        links
            .recv_timeout(HANG)
            .map_err(|e| format!("the link: {e}")),
        &dir,
    );
    assert_eq!(link.path, "/v1/servers/shardfall/link");
    assert_eq!(link.authorization, format!("Bearer {KEY}"));
    link.ws
        .get_ref()
        .set_read_timeout(Some(Duration::from_millis(200)))
        .expect("a read timeout");
    let hello = must(next(&mut link.ws, "hello"), &dir);
    assert!(
        matches!(&hello, Message::Hello { proto, software, .. } if *proto == PROTOCOL && software.starts_with("yalain ")),
        "{hello:?}"
    );
    send(
        &mut link.ws,
        &Message::Welcome {
            proto: PROTOCOL,
            world: "shardfall".into(),
            hub: "stand-in".into(),
            worlds: vec![WorldInfo {
                slug: "coldeve".into(),
                name: "Coldeve".into(),
                online: true,
            }],
            rooms: Vec::new(),
            limits: Limits::DEFAULT,
        },
    );
    let a = |s: &str| Address::parse(s).expect("an address");
    send(
        &mut link.ws,
        &Message::Tell {
            id: "h1".into(),
            from: a("Bob@coldeve"),
            to: a("Beaconkeeper@shardfall"),
            text: "anyone there?".into(),
            friend: true,
        },
    );
    send(
        &mut link.ws,
        &Message::Watch {
            owner: a("Bob@coldeve"),
            target: a("Beaconkeeper@shardfall"),
            on: true,
        },
    );
    let mut got = Vec::new();
    while got.len() < 2 {
        got.push(must(next(&mut link.ws, "the answers"), &dir));
    }
    assert!(
        got.contains(&Message::TellResult {
            id: "h1".into(),
            outcome: Outcome::Offline,
            message: None,
        }),
        "{got:?}"
    );
    assert!(
        got.contains(&Message::Presence {
            to: a("Bob@coldeve"),
            of: a("Beaconkeeper@shardfall"),
            online: false,
        }),
        "{got:?}"
    );
    console.write_all(b"dn\n").expect("the console");
    console.flush().expect("the console");
    must(
        logged(
            &dir,
            "This world is on the Dereth Network as shardfall. The hub is connected. 1 world is online.",
        ),
        &dir,
    );
    console
        .write_all(b"dn block coldeve\n")
        .expect("the console");
    console.flush().expect("the console");
    must(
        logged(&dir, "coldeve is blocked for everyone on this world."),
        &dir,
    );
}

/// A world set up from its configuration file's `[server.websocket]`, on the network and with a
/// status endpoint, tells its hub in its `hello` and its status endpoint where browsers reach it:
/// the file's `public_url`. A page from the file's `allowed_origins` may read the status too.
/// Behaviour: server.network.websocket-url-told-to-the-hub
/// Behaviour: server.operator.websocket-settings-read-as-empyreans
/// Behaviour: server.operator.websocket-url-on-the-status-endpoint
#[test]
fn a_world_tells_its_hub_and_its_status_endpoint_its_public_websocket_address() {
    let dir = scratch("yalain-live-websocket-hub");
    let install = auberean_dat::testing::require_target_install();
    let (hub, links) = stand_in_hub();
    std::fs::write(dir.join("network.key"), format!("{KEY}\n")).expect("the key file");
    let quoted = |p: &Path| format!("{:?}", p.to_string_lossy());
    let (_, _server, ()) = serving(&dir, |port| {
        std::fs::write(
            dir.join("yalain.toml"),
            format!(
                "[server]\ndat_files_directory = {}\nworld_database_path = {}\n\
                 status_address = \"127.0.0.1:0\"\n\
                 [server.network]\nport = {port}\n\
                 [server.websocket]\nenabled = true\nlisten = \"127.0.0.1:0\"\n\
                 public_url = \"wss://play.example.org/ws\"\n\
                 allowed_origins = [\"https://play.example.org\"]\n\
                 [database]\nin_memory = true\n[testing]\nrun_seconds = {LIFE}\n\
                 [federation]\nenabled = true\nhub_url = \"ws://127.0.0.1:{hub}\"\n\
                 slug = \"shardfall\"\n",
                quoted(&install),
                quoted(&authored::database_dir()),
            ),
        )
        .expect("the configuration file");
        let log = std::fs::File::create(dir.join("server.log")).expect("the server's log");
        let child = Command::new(env!("CARGO_BIN_EXE_yalain"))
            .current_dir(&dir)
            .stdin(Stdio::null())
            .stdout(log.try_clone().expect("the log twice"))
            .stderr(log)
            .spawn()
            .expect("the server starts");
        (child, ())
    });
    let mut link = must(
        links
            .recv_timeout(HANG)
            .map_err(|e| format!("the link: {e}")),
        &dir,
    );
    link.ws
        .get_ref()
        .set_read_timeout(Some(Duration::from_millis(200)))
        .expect("a read timeout");
    let hello = must(next(&mut link.ws, "hello"), &dir);
    let Message::Hello { websocket_url, .. } = hello else {
        panic!("not a hello: {hello:?}");
    };
    assert_eq!(websocket_url.as_deref(), Some("wss://play.example.org/ws"));
    let status_port = crate::configured::status_port(&dir);
    let status = crate::configured::status(status_port, &dir);
    assert_eq!(status["websocket_url"], "wss://play.example.org/ws");
    let mut asked = TcpStream::connect((Ipv4Addr::LOCALHOST, status_port)).expect("connects");
    asked
        .write_all(b"GET /status HTTP/1.1\r\nHost: x\r\nOrigin: https://play.example.org\r\n\r\n")
        .expect("sent");
    let mut text = String::new();
    asked.read_to_string(&mut text).expect("an answer");
    assert!(
        text.contains("Access-Control-Allow-Origin: https://play.example.org\r\n"),
        "{text}"
    );
}
