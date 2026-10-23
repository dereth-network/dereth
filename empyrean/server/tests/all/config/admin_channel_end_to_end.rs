//! Vectors: a booted server's admin channel, spoken to over loopback.
//! A server run as a service runs (no console), writes its admin channel's file, takes the secret,
//! sends its log, saves the world on `save-world` and says `Saved.`, stops on `exit`, and takes
//! its file away when it has.
//! Fixture: the built server, retail dat files and world.pack.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpStream, UdpSocket};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use empyrean_server::admin_channel;

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
            "(no retail dats at {} or no world.pack at {}: the admin channel check did not run)",
            client.display(),
            pack.display()
        );
        None
    }
}

/// A game port whose next port is free too, on loopback.
fn free_pair() -> u16 {
    loop {
        let a = UdpSocket::bind("127.0.0.1:0").unwrap();
        let port = a.local_addr().unwrap().port();
        if port < u16::MAX && UdpSocket::bind(("127.0.0.1", port + 1)).is_ok() {
            return port;
        }
    }
}

/// The next of the channel's lines whose console line contains `text`; the guard only stops a hang.
fn until(reader: &mut BufReader<TcpStream>, text: &str) -> String {
    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line).expect("the channel's line");
        assert!(n > 0, "the channel closed before {text:?}");
        let v: serde_json::Value = serde_json::from_str(&line).expect("JSON");
        if v["line"].as_str().is_some_and(|l| l.contains(text)) {
            return line;
        }
    }
}

fn wait_for(what: &str, mut done: impl FnMut() -> bool) {
    let guard = Instant::now();
    while !done() {
        assert!(
            guard.elapsed() < Duration::from_secs(300),
            "{what} never came"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn a_service_world_takes_commands_and_sends_its_log_over_its_admin_channel() {
    let Some((client, pack)) = content() else {
        return;
    };
    let dir = scratch("admin-channel-end-to-end");
    let port = free_pair();
    std::fs::write(
        dir.join("empyrean.toml"),
        format!(
            "[server]\ndat_files_directory = '{}'\nworld_pack_path = '{}'\nlandblock_preloading = false\ninteractive_console = false\n\
             [server.network]\nhost = \"127.0.0.1\"\nport = {port}\n",
            client.display(),
            pack.display()
        ),
    )
    .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_empyrean-server"))
        .current_dir(&dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("empyrean-server runs");
    let pid = child.id();
    let file = || admin_channel::read_file(&dir).filter(|f| f.pid == pid);
    wait_for("the admin channel's file", || {
        assert!(child.try_wait().unwrap().is_none(), "the server ended");
        file().is_some()
    });
    let channel = file().unwrap();
    let mut stream = TcpStream::connect(channel.address.as_str()).expect("the channel");
    stream
        .set_read_timeout(Some(Duration::from_secs(300)))
        .unwrap();
    writeln!(stream, "{}", channel.secret).unwrap();
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut hello = String::new();
    reader.read_line(&mut hello).unwrap();
    assert!(hello.contains("\"hello\":\"empyrean\""), "{hello}");
    // Its log came with it: the lines before the connection, then each new one.
    until(&mut reader, "Admin channel: on loopback");
    writeln!(stream, "save-world").unwrap();
    until(&mut reader, "Saving 0 players and");
    until(&mut reader, "Saved.");
    writeln!(stream, "exit").unwrap();
    wait_for("the server's end", || child.try_wait().unwrap().is_some());
    assert!(child.wait().unwrap().success());
    assert!(
        !Path::new(&dir.join(admin_channel::FILE_NAME)).exists(),
        "the file goes with the server"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
