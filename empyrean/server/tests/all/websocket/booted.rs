//! Divergence: the WebSocket endpoint is Empyrean's own; ACE speaks UDP only.
//! Behaviour: none (a transport this server adds; the claims are its settings' contract).
//! The booted server listens for WebSocket connections only when `[server.websocket]` enables
//! it, reports the URL on its status endpoint, and will not start a plain listener on an address
//! that is not loopback without `behind_tls_proxy`.
//! Fixture: the built server, retail dat files and world.pack.

use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The dat folder and the content pack, when both are present.
fn content() -> Option<(PathBuf, PathBuf)> {
    if !cfg!(feature = "real-content") {
        return None;
    }
    let client = dereth_dat::testing::dat_dir();
    let pack = empyrean_common::test_paths::world_pack();
    if dereth_dat::testing::have_dats() && pack.is_file() {
        Some((client, pack))
    } else {
        eprintln!(
            "(no retail dats at {} or no world.pack at {}: the WebSocket boot check did not run)",
            client.display(),
            pack.display()
        );
        None
    }
}

/// A loopback TCP port nothing listens on now.
fn free_port() -> u16 {
    TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .expect("a free port")
}

/// Starts the server in `dir` with `websocket` as its `[server.websocket]` section and a status
/// endpoint on `status`.
fn boot(dir: &Path, client: &Path, pack: &Path, udp: u16, status: u16, websocket: &str) -> Child {
    std::fs::write(
        dir.join("empyrean.toml"),
        format!(
            "[server]\ndat_files_directory = '{}'\nworld_pack_path = '{}'\nlandblock_preloading = false\n\
             interactive_console = false\nstatus_address = \"127.0.0.1:{status}\"\n\
             [server.network]\nhost = \"127.0.0.1\"\nport = {udp}\n{websocket}\n",
            client.display(),
            pack.display()
        ),
    )
    .unwrap();
    Command::new(env!("CARGO_BIN_EXE_empyrean-server"))
        .args(["--run-for", "60"])
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("empyrean-server runs")
}

/// The status endpoint's JSON, once the server answers (up to 60 s of start-up).
fn status(port: u16) -> Option<String> {
    let end = Instant::now() + Duration::from_secs(60);
    while Instant::now() < end {
        if let Ok(mut s) = TcpStream::connect((Ipv4Addr::LOCALHOST, port)) {
            let _ = s.write_all(b"GET /status HTTP/1.0\r\n\r\n");
            let mut body = String::new();
            let _ = s.read_to_string(&mut body);
            if body.starts_with("HTTP/1.0 200") {
                return Some(body);
            }
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    None
}

fn listens(port: u16) -> bool {
    TcpStream::connect_timeout(
        &SocketAddr::from((Ipv4Addr::LOCALHOST, port)),
        Duration::from_millis(500),
    )
    .is_ok()
}

#[test]
fn with_websocket_disabled_nothing_listens_and_enabled_it_is_reported() {
    let Some((client, pack)) = content() else {
        return;
    };
    let dir = scratch("websocket-boot");
    let (ws, st) = (free_port(), free_port());
    let mut off = boot(&dir, &client, &pack, 19_430, st, "");
    let body = status(st).expect("the server came up");
    assert!(body.contains("\"websocket_url\":null"), "{body}");
    assert!(!listens(ws), "nothing listens with WebSocket off");
    let _ = off.kill();
    let _ = off.wait();

    let (ws, st) = (free_port(), free_port());
    let mut on = boot(
        &dir,
        &client,
        &pack,
        19_440,
        st,
        &format!("[server.websocket]\nenabled = true\nlisten = \"127.0.0.1:{ws}\"\n"),
    );
    let body = status(st).expect("the server came up");
    assert!(
        body.contains(&format!("\"websocket_url\":\"ws://127.0.0.1:{ws}/\"")),
        "{body}"
    );
    assert!(listens(ws), "the endpoint listens");
    let _ = on.kill();
    let _ = on.wait();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_plain_listener_on_a_public_address_stops_the_start_up() {
    let Some((client, pack)) = content() else {
        return;
    };
    let dir = scratch("websocket-plain-public");
    let child = boot(
        &dir,
        &client,
        &pack,
        19_450,
        free_port(),
        &format!(
            "[server.websocket]\nenabled = true\nlisten = \"0.0.0.0:{}\"\n",
            free_port()
        ),
    );
    let out = child.wait_with_output().expect("the server exits");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "it stopped with a failure");
    assert!(
        stderr.contains("would serve plain ws://") && stderr.contains("behind_tls_proxy"),
        "{stderr}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
