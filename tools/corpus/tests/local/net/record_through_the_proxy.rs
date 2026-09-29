//! Contracts for `dereth-corpus record`: the proxy, the split and the scrub, end to end.
//! Fixture: two committed recordings replayed through the proxy by a scripted client and server.
//! Behaviour: none (a tool's contract, not client behaviour)
//!
//! No retail client is needed: a fake client and a fake server, each a pair of local UDP sockets,
//! play two committed recordings back to back through a real `dereth-corpus record` process, each
//! side sending its next datagram only once the previous one has arrived at the other, so the
//! capture's order is the recording's order. The two recordings are two logins, so the capture
//! must come out as two parts, and each part must be exactly what `dereth-corpus scrub` makes of
//! the same raw input.

use std::io::{BufRead, BufReader};
use std::net::{SocketAddr, UdpSocket};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use dereth_client_net::client_session::testing::capture::{self, Datagram};

/// The two recordings played back to back: each is one login ending in a clean `Disconnect`.
const FIRST: &str = "ddd-interrogation-only";
const SECOND: &str = "short-second-connection";

const SLUG: &str = "replayed-two-logins";

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_dereth-corpus"))
}

fn loopback(port: u16) -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], port))
}

/// Two sockets on adjacent loopback ports, `P` and `P + 1`.
fn adjacent_pair() -> (UdpSocket, UdpSocket) {
    for _ in 0..200 {
        let a = UdpSocket::bind(loopback(0)).expect("an ephemeral port");
        let port = a.local_addr().expect("bound").port();
        if port == u16::MAX {
            continue;
        }
        if let Ok(b) = UdpSocket::bind(loopback(port + 1)) {
            return (a, b);
        }
    }
    panic!("no two adjacent free loopback ports");
}

/// A free `P` whose `P + 1` is free too, released for the proxy to bind.
fn free_adjacent_port() -> u16 {
    let (a, _b) = adjacent_pair();
    a.local_addr().expect("bound").port()
}

/// The proxy process, killed if the test ends before it does.
struct Proxy(Child);

impl Drop for Proxy {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn scratch() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("dereth-corpus-record-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch checkout");
    dir
}

/// The data bytes of every line of a recording file, in order.
fn datagrams(path: &Path) -> Vec<Vec<u8>> {
    capture::load(path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .into_iter()
        .map(|d| d.raw)
        .collect()
}

fn first_t(path: &Path) -> String {
    let text = std::fs::read_to_string(path).expect("written");
    let line = text.lines().next().expect("a line");
    line.split(',').next().expect("t first").to_owned()
}

/// Play `script` through the proxy listening on `listen`, as a client on one socket and a server
/// on `server`.
fn play(script: &[&Datagram], listen: u16, server: &[UdpSocket; 2]) {
    let client = UdpSocket::bind(loopback(0)).expect("the client's socket");
    let wait = Some(Duration::from_secs(10));
    client.set_read_timeout(wait).expect("timeout");
    for s in server {
        s.set_read_timeout(wait).expect("timeout");
    }
    let mut upstream: [Option<SocketAddr>; 2] = [None, None];
    let mut buf = vec![0u8; 65_536];
    for (n, d) in script.iter().enumerate() {
        let pair = usize::from(d.pair);
        if d.c2s {
            client
                .send_to(&d.raw, loopback(listen + d.pair))
                .expect("the client sends");
            let (len, from) = server[pair]
                .recv_from(&mut buf)
                .unwrap_or_else(|e| panic!("datagram {n}: the server heard nothing: {e}"));
            assert_eq!(
                &buf[..len],
                &d.raw[..],
                "datagram {n} reached the server intact"
            );
            upstream[pair] = Some(from);
        } else {
            let to = upstream[pair]
                .unwrap_or_else(|| panic!("datagram {n}: nothing has come from pair {pair} yet"));
            server[pair].send_to(&d.raw, to).expect("the server sends");
            let (len, from) = client
                .recv_from(&mut buf)
                .unwrap_or_else(|e| panic!("datagram {n}: the client heard nothing: {e}"));
            assert_eq!(
                &buf[..len],
                &d.raw[..],
                "datagram {n} reached the client intact"
            );
            assert_eq!(
                from,
                loopback(listen + d.pair),
                "datagram {n} came back from the listener of its own pair"
            );
        }
    }
}

#[test]
fn a_proxied_session_is_split_per_login_and_scrubbed_exactly_as_scrub_would() {
    let first = capture::shared_session(FIRST);
    let second = capture::shared_session(SECOND);
    let script: Vec<&Datagram> = first.iter().chain(second.iter()).collect();

    let repo = scratch();
    let (s0, s1) = adjacent_pair();
    let server_port = s0.local_addr().expect("bound").port();
    let listen = free_adjacent_port();
    let mut child = Command::new(bin())
        .args(["record", SLUG, "--repo"])
        .arg(&repo)
        .args(["--listen", &loopback(listen).to_string()])
        .args(["--server", &loopback(server_port).to_string()])
        .args(["--linger", "1"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("dereth-corpus starts");
    let stdout = child.stdout.take().expect("piped");
    let mut stderr = child.stderr.take().expect("piped");
    let proxy = Proxy(child);

    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let _ = tx.send(line);
        }
    });
    let mut said = Vec::new();
    loop {
        let line = rx
            .recv_timeout(Duration::from_secs(30))
            .unwrap_or_else(|_| panic!("the proxy never said it was listening: {said:?}"));
        let ready = line.contains("launch the client with");
        said.push(line);
        if ready {
            break;
        }
    }

    play(&script, listen, &[s0, s1]);

    // The second recording ends with a clean logout, so the proxy stops by itself.
    let mut proxy = proxy;
    let started = Instant::now();
    let status = loop {
        if let Some(s) = proxy.0.try_wait().expect("the process is there") {
            break s;
        }
        assert!(
            started.elapsed() < Duration::from_secs(60),
            "the proxy did not stop after the clean logout"
        );
        std::thread::sleep(Duration::from_millis(100));
    };
    let mut err = String::new();
    std::io::Read::read_to_string(&mut stderr, &mut err).expect("stderr");
    said.extend(rx.try_iter());
    assert!(
        status.success(),
        "record failed: {err}\n{}",
        said.join("\n")
    );

    let captures = repo.join("fixtures/packet-captures");
    let raw = captures.join("raw");
    let parts = [format!("{SLUG}-1"), format!("{SLUG}-2")];

    // The proxy forwarded and logged every datagram, both ways, in order and byte for byte.
    let whole = datagrams(&raw.join("unsplit").join(format!("{SLUG}.jsonl")));
    let want: Vec<Vec<u8>> = script.iter().map(|d| d.raw.clone()).collect();
    assert_eq!(whole, want, "the capture is the script");

    // Split at the second login, each part rebased to its own start.
    for (part, recording) in parts.iter().zip([first, second]) {
        let path = raw.join(format!("{part}.jsonl"));
        let want: Vec<Vec<u8>> = recording.iter().map(|d| d.raw.clone()).collect();
        assert_eq!(datagrams(&path), want, "{part} is one login");
        assert_eq!(first_t(&path), "{\"t\": 0.0", "{part} starts at t = 0");
    }

    // The tracked folder holds the two scrubbed parts and nothing else.
    let mut tracked: Vec<String> = std::fs::read_dir(&captures)
        .expect("the folder")
        .filter_map(Result::ok)
        .filter(|e| e.path().is_file())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    tracked.sort();
    assert_eq!(
        tracked,
        parts
            .iter()
            .map(|p| format!("{p}.jsonl"))
            .collect::<Vec<_>>()
    );
    assert!(
        captures.join("scrub-map").join("_corpus.json").is_file(),
        "the map went to the untracked folder"
    );

    // And each is exactly what `scrub` makes of the same raw part.
    let check = repo.join("check");
    let ran = Command::new(bin())
        .args(["scrub", "--repo"])
        .arg(&repo)
        .arg("--raw-dir")
        .arg(&raw)
        .arg("--out")
        .arg(&check)
        .arg("--map-dir")
        .arg(repo.join("check-map"))
        .arg("--fresh")
        .output()
        .expect("dereth-corpus scrub runs");
    assert!(
        ran.status.success(),
        "scrub failed: {}",
        String::from_utf8_lossy(&ran.stderr)
    );
    let mut substituted = 0usize;
    for part in &parts {
        let recorded = std::fs::read(captures.join(format!("{part}.jsonl"))).expect("recorded");
        let scrubbed = std::fs::read(check.join(format!("{part}.jsonl"))).expect("scrubbed");
        assert!(recorded == scrubbed, "{part}: record and scrub disagree");
        let raw_part = datagrams(&raw.join(format!("{part}.jsonl")));
        let out_part = datagrams(&captures.join(format!("{part}.jsonl")));
        assert_eq!(
            raw_part.len(),
            out_part.len(),
            "{part}: every datagram kept"
        );
        substituted += raw_part
            .iter()
            .zip(&out_part)
            .filter(|(a, b)| a != b)
            .count();
    }
    // With no dictionary in the scratch checkout the characters get generated stand-ins, which
    // differ from the names the recordings carry, so the scrub must have changed something.
    assert!(substituted > 0, "the scrub changed nothing");

    let _ = std::fs::remove_dir_all(&repo);
}
