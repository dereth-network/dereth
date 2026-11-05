//! The executable set up from a configuration file in its working folder, with an option given on
//! its command line too, answering its status endpoint. Each step waits for the endpoint's
//! answer, with a time limit only against a hang. Fixture: the built executable, the install, the
//! authored world and a configuration file in a scratch folder.

use std::io::{Read as _, Write as _};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream, UdpSocket};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Instant;

use yalain_testkit::authored;
use yalain_testkit::session::{self as play};

use crate::session::{must, scratch, serving, Loopback, LIFE};

/// What the log says before the status endpoint's address.
const STATUS_AT: &str = "status endpoint listening on http://127.0.0.1:";

/// The status endpoint's port, from the log.
pub(crate) fn status_port(dir: &Path) -> u16 {
    let log = std::fs::read_to_string(dir.join("server.log")).unwrap_or_default();
    log.lines()
        .find_map(|l| {
            let at = l.find(STATUS_AT)? + STATUS_AT.len();
            l[at..].split('/').next()?.parse().ok()
        })
        .unwrap_or_else(|| panic!("the status endpoint's port\n{log}"))
}

/// `/status`'s body once the endpoint answers.
pub(crate) fn status(port: u16, dir: &Path) -> serde_json::Value {
    let until = Instant::now() + crate::network::HANG;
    loop {
        if let Ok(mut s) = TcpStream::connect((Ipv4Addr::LOCALHOST, port)) {
            s.write_all(b"GET /status HTTP/1.1\r\nHost: localhost\r\n\r\n")
                .expect("sent");
            let mut text = String::new();
            s.read_to_string(&mut text).expect("an answer");
            let body = text.split("\r\n\r\n").nth(1).unwrap_or_default();
            return must(
                serde_json::from_str(body).map_err(|e| format!("{e}: {text}")),
                dir,
            );
        }
        assert!(Instant::now() < until, "the status endpoint never answered");
        std::thread::yield_now();
    }
}

/// A TOML string.
fn quoted(p: &Path) -> String {
    format!("{:?}", p.to_string_lossy())
}

/// The executable started with no option but one finds `yalain.toml` in its working folder and
/// takes every setting from it (the install, the world database, its ports, the logins it holds,
/// the status endpoint), the command line's world name winning over the file's; its status
/// endpoint answers with the world's state, counting a player once its character is in the world.
/// Behaviour: server.operator.config-file-sets-every-option
/// Behaviour: server.operator.status-endpoint
#[test]
fn the_server_takes_its_settings_from_its_config_file_and_answers_its_status_endpoint() {
    let dir = scratch("yalain-live-configured");
    let install = auberean_dat::testing::require_target_install();
    // In the form of Empyrean's empyrean.toml, with a key of Empyrean's Yalain has no setting
    // for, which is ignored. The status endpoint on a port the system hands out, which the log
    // names.
    let (port, _server, ()) = serving(&dir, |port| {
        std::fs::write(
            dir.join("yalain.toml"),
            format!(
                "[server]\nworld_name = \"From The File\"\ndat_files_directory = {}\n\
                 world_database_path = {}\nstatus_address = \"127.0.0.1:0\"\n\
                 landblock_preloading = true\n\
                 [server.network]\nport = {port}\nmaximum_allowed_sessions = 5\n\
                 [database]\nin_memory = true\n[testing]\nrun_seconds = {LIFE}\n",
                quoted(&install),
                quoted(&authored::database_dir()),
            ),
        )
        .expect("the configuration file");
        let log = std::fs::File::create(dir.join("server.log")).expect("the server's log");
        let child = Command::new(env!("CARGO_BIN_EXE_yalain"))
            .args(["--world-name", "From The Command Line"])
            .current_dir(&dir)
            .stdin(Stdio::null())
            .stdout(log.try_clone().expect("the log twice"))
            .stderr(log)
            .spawn()
            .expect("the server starts");
        (child, ())
    });
    let status_port = status_port(&dir);

    let first = status(status_port, &dir);
    assert_eq!(first["world_name"], "From The Command Line");
    assert_eq!(first["max_logins"], 5);
    assert_eq!(first["server"], "yalain");
    assert_eq!(first["game"], "auberean");
    assert_eq!(first["state"], "online");
    assert_eq!(first["players_online"], 0);

    let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).expect("a client socket");
    socket.set_nonblocking(true).expect("nonblocking");
    let mut s = Loopback {
        socket,
        client: play::client(
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port),
            "configured",
        ),
        start: Instant::now(),
        buf: vec![0; 65_536],
    };
    must(play::character_list(&mut s), &dir);
    let me = must(play::create(&mut s, "Fileborn"), &dir);
    must(play::enter(&mut s, me), &dir);
    let until = Instant::now() + crate::network::HANG;
    loop {
        let now = status(status_port, &dir);
        if now["players_online"] == 1 {
            assert_eq!(now["logins"], 1);
            break;
        }
        assert!(Instant::now() < until, "never counted: {now}");
        std::thread::yield_now();
    }
    let log = std::fs::read_to_string(dir.join("server.log")).unwrap_or_default();
    assert!(log.contains("configuration read from"), "{log}");
    assert!(
        log.contains("server.landblock_preloading: Empyrean's setting"),
        "{log}"
    );
}

/// The executable set up from a file whose `[server.websocket]` cannot be served stops at its start
/// with exit code 1 and says why: a plain listener off loopback (the section's own default),
/// half a certificate and a certificate that will not read. It reads nothing of the install first.
/// Behaviour: server.operator.websocket-settings-refused-as-empyreans
#[test]
fn a_websocket_setting_that_cannot_be_served_stops_the_server_at_its_start() {
    let dir = scratch("yalain-live-websocket-refused");
    for (keys, says) in [
        ("", "plain ws://"),
        ("tls_certificate = \"c.pem\"", "set together"),
        ("listen = \"nowhere\"", "is not an ip:port"),
    ] {
        std::fs::write(
            dir.join("yalain.toml"),
            format!("[server.websocket]\nenabled = true\n{keys}\n"),
        )
        .expect("the configuration file");
        let out = Command::new(env!("CARGO_BIN_EXE_yalain"))
            .current_dir(&dir)
            .stdin(Stdio::null())
            .output()
            .expect("the server runs");
        let said = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.status.code(), Some(1), "{keys}: {said}");
        assert!(said.contains(says), "{keys}: {said}");
    }
}
