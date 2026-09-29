//! Divergence: V385
//! ACE: ServerManager.BeginShutdown, ServerManager.CancelShutdown (the countdown before the
//! restart, and cancelling it)
//! Behaviour: none (a server tooling claim: the running server updating itself, not game
//! behaviour).
//! The real server with `server.update = "patch"`, pointed at a release served on loopback: it
//! stages the release, counts down in its log as to its players, installs it after a clean stop,
//! health-checks it and restarts into it (on Windows as its child in the same console, the
//! console's input forwarded), and the new server comes up; a cancelled countdown installs
//! nothing; a release that fails its health check is rolled back and the old server starts again.
//! Fixture: the built server and the unhealthy-server fixture, retail dat files and world.pack
//! (real content only), temporary installations, a synthetic release served on 127.0.0.1.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use empyrean_server::update::release::{target_triple, Facts};
use empyrean_server::update::verify;

use super::Loopback;

const EXE: &str = std::env::consts::EXE_SUFFIX;

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
            "(no retail dats at {} or no world.pack at {}: the running server's update was not tried)",
            client.display(),
            pack.display()
        );
        None
    }
}

fn free_port() -> u16 {
    TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .expect("a free port")
}

/// The server binary built with these tests.
fn real_server() -> Vec<u8> {
    std::fs::read(env!("CARGO_BIN_EXE_empyrean-server")).expect("the built server")
}

/// A server build that reports its facts and never comes up.
#[cfg(feature = "real-content")]
fn unhealthy_server() -> Vec<u8> {
    std::fs::read(env!("CARGO_BIN_EXE_empyrean-server-unhealthy-fixture")).expect("the fixture")
}

#[cfg(not(feature = "real-content"))]
fn unhealthy_server() -> Vec<u8> {
    Vec::new()
}

/// Publishes this build's version as a patch release on `web`, its server binary `server`.
fn publish(web: &Loopback, server: Vec<u8>) {
    let facts = Facts::this_build();
    let version = facts.version.clone();
    let target = target_triple();
    let root = format!("empyrean-{version}-{target}");
    let file = format!("{root}.zip");
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default().unix_permissions(0o755);
    zip.add_directory(format!("{root}/"), opts).unwrap();
    for (name, bytes) in [
        (format!("empyrean-server{EXE}"), server),
        (format!("empyrean-import{EXE}"), b"importer".to_vec()),
        ("README.md".to_owned(), b"new guide".to_vec()),
    ] {
        zip.start_file(format!("{root}/{name}"), opts).unwrap();
        zip.write_all(&bytes).unwrap();
    }
    let archive = zip.finish().unwrap().into_inner();
    let index = serde_json::json!({
        "schema": 2,
        "product": "empyrean",
        "version": version,
        "prerelease": false,
        "assets": [{"target": target, "file": file, "size": archive.len(), "sha256": verify::sha256_hex(&archive)}],
        "upgrade": {
            "kind": "patch",
            "previous": "0.0.1",
            "upgrades_from": "0.0.1",
            "databases": facts.databases,
            "world_pack": facts.world_pack,
            "migrations": [],
            "world_pack_rebuild": false,
            "config": []
        }
    })
    .to_string();
    let sums = format!(
        "{}  {file}\n{}  release.json\n",
        verify::sha256_hex(&archive),
        verify::sha256_hex(index.as_bytes())
    );
    let tag = format!("empyrean-v{version}");
    let dl = |name: &str| format!("{}/download/{tag}/{name}", web.base);
    web.put(&format!("/download/{tag}/{file}"), archive);
    web.put(&format!("/download/{tag}/release.json"), index);
    web.put(&format!("/download/{tag}/SHA256SUMS"), sums);
    let list = serde_json::json!([
        {"tag_name": tag, "draft": false, "prerelease": false, "assets": [
            {"name": file, "browser_download_url": dl(&file)},
            {"name": "release.json", "browser_download_url": dl("release.json")},
            {"name": "SHA256SUMS", "browser_download_url": dl("SHA256SUMS")}
        ]}
    ]);
    web.put(
        "/repos/example/empyrean/releases?per_page=100",
        list.to_string(),
    );
}

/// The version the running server is taken for: a pre-release of this build's version, so this
/// build's version is a newer patch release of the same minor line.
fn older_version() -> String {
    format!("{}-rc.1", env!("CARGO_PKG_VERSION"))
}

/// An installation of the real server in its own folder, running.
struct Running {
    dir: PathBuf,
    status: u16,
    child: Child,
    stdin: Option<ChildStdin>,
    log: Arc<Mutex<Vec<String>>>,
}

impl Running {
    fn start(name: &str, client: &Path, pack: &Path, web: &Loopback, udp: u16) -> Self {
        let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
            .join("update-running")
            .join(format!("{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("empyrean-server{EXE}")), real_server()).unwrap();
        std::fs::write(dir.join("README.md"), b"old guide").unwrap();
        let status = free_port();
        std::fs::write(
            dir.join("empyrean.toml"),
            format!(
                "[server]\ndat_files_directory = '{}'\nworld_pack_path = '{}'\nlandblock_preloading = false\n\
                 status_address = \"127.0.0.1:{status}\"\nupdate = \"patch\"\nupdate_warning_seconds = 20\n\
                 update_source = \"{}/repos/example/empyrean\"\n\
                 [server.network]\nhost = \"127.0.0.1\"\nport = {udp}\n",
                client.display(),
                pack.display(),
                web.base
            ),
        )
        .unwrap();
        let mut child = Command::new(dir.join(format!("empyrean-server{EXE}")))
            .args(["--config", "empyrean.toml", "--update-test-version"])
            .arg(older_version())
            .current_dir(&dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("the server starts");
        let log: Arc<Mutex<Vec<String>>> = Arc::default();
        for stream in [
            Box::new(child.stdout.take().unwrap()) as Box<dyn Read + Send>,
            Box::new(child.stderr.take().unwrap()),
        ] {
            let log = Arc::clone(&log);
            std::thread::spawn(move || {
                for line in BufReader::new(stream).lines().map_while(Result::ok) {
                    log.lock().unwrap().push(line);
                }
            });
        }
        let stdin = child.stdin.take();
        Self {
            dir,
            status,
            child,
            stdin,
            log,
        }
    }

    fn lines(&self) -> Vec<String> {
        self.log.lock().unwrap().clone()
    }

    /// The index of the first log line after line `after` that contains `text`.
    fn find(&self, text: &str, after: usize) -> Option<usize> {
        self.lines()
            .iter()
            .enumerate()
            .skip(after)
            .find(|(_, l)| l.contains(text))
            .map(|(i, _)| i)
    }

    /// Waits up to `secs` for a log line after line `after` that contains `text`; its index.
    fn wait_for(&self, text: &str, after: usize, secs: u64) -> usize {
        let end = Instant::now() + Duration::from_secs(secs);
        loop {
            if let Some(i) = self.find(text, after) {
                return i;
            }
            assert!(
                Instant::now() < end,
                "no log line with `{text}` within {secs} s; the log:\n{}",
                self.lines().join("\n")
            );
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    fn console(&mut self, line: &str) {
        let stdin = self.stdin.as_mut().expect("the console");
        stdin.write_all(line.as_bytes()).unwrap();
        stdin.write_all(b"\n").unwrap();
        stdin.flush().unwrap();
    }

    /// `GET /status` answers 200.
    fn status_answers(&self) -> bool {
        let end = Instant::now() + Duration::from_secs(30);
        while Instant::now() < end {
            if let Ok(mut s) = TcpStream::connect((Ipv4Addr::LOCALHOST, self.status)) {
                let _ = s.write_all(b"GET /status HTTP/1.0\r\n\r\n");
                let mut body = String::new();
                let _ = s.read_to_string(&mut body);
                if body.starts_with("HTTP/1.0 200") {
                    return true;
                }
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        false
    }

    /// Stops the server from its console (`exit`) and waits for the process to end.
    fn exit(mut self) -> Vec<String> {
        self.console("exit");
        let end = Instant::now() + Duration::from_secs(60);
        loop {
            match self.child.try_wait().unwrap() {
                Some(status) => {
                    assert!(status.success(), "exited with {status}");
                    break;
                }
                None if Instant::now() < end => std::thread::sleep(Duration::from_millis(100)),
                None => {
                    self.kill_tree();
                    panic!(
                        "the server did not stop on `exit`; the log:\n{}",
                        self.lines().join("\n")
                    );
                }
            }
        }
        std::thread::sleep(Duration::from_millis(200));
        self.lines()
    }

    /// Ends the server and any server it started in its place (on Windows the new server runs as
    /// its child), so a test that fails leaves no process behind.
    fn kill_tree(&mut self) {
        if matches!(self.child.try_wait(), Ok(None)) {
            if cfg!(windows) {
                let _ = Command::new("taskkill")
                    .args(["/T", "/F", "/PID", &self.child.id().to_string()])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
            }
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }

    fn read(&self, name: &str) -> Vec<u8> {
        std::fs::read(self.dir.join(name)).unwrap_or_default()
    }

    fn names(&self) -> Vec<String> {
        std::fs::read_dir(&self.dir)
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect()
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        self.kill_tree();
    }
}

#[test]
fn the_running_server_counts_down_installs_and_restarts_into_a_patch_release() {
    let Some((client, pack)) = content() else {
        return;
    };
    let web = Loopback::start();
    publish(&web, real_server());
    let server = Running::start("installs", &client, &pack, &web, 19_480);
    let version = Facts::this_build().version;

    let staged = server.wait_for(&format!("Update: {version} is staged"), 0, 120);
    assert!(server.find("checked by SHA-256 only", 0).is_some());
    // The countdown the players are sent.
    server.wait_for("will be shutting down in 15 seconds", staged, 60);
    let healthy = server.wait_for(
        &format!("Update: {version} passed its health check"),
        staged,
        180,
    );
    let ready = server
        .find("Health check: ready", staged)
        .expect("the trial reported ready");
    assert!(ready < healthy);
    let restarted = server.wait_for(&format!("Update: starting {version}"), healthy, 10);
    // The new server comes up: its start-up, then its status endpoint.
    server.wait_for("Status endpoint: http://", restarted, 120);
    assert!(server.status_answers(), "the new server answers /status");

    assert_eq!(server.read("README.md"), b"new guide");
    assert_eq!(server.read(&format!("empyrean-server{EXE}")), real_server());
    let names = server.names();
    assert!(
        names.contains(&format!(
            "empyrean-server{EXE}.previous-v{}",
            older_version()
        )),
        "{names:?}"
    );
    assert!(
        names
            .iter()
            .any(|n| n.starts_with("shard.db.backup-update-")),
        "{names:?}"
    );

    // The new server reads the console (on Windows forwarded by the process that started it).
    let log = server.exit();
    let bye = log
        .iter()
        .rposition(|l| l.contains("Server shutdown interval reset: 0"))
        .expect("the new server ran the console's exit");
    assert!(bye > restarted);
}

#[test]
fn a_cancelled_countdown_installs_nothing() {
    let Some((client, pack)) = content() else {
        return;
    };
    let web = Loopback::start();
    publish(&web, real_server());
    let mut server = Running::start("cancelled", &client, &pack, &web, 19_490);
    let version = Facts::this_build().version;

    let staged = server.wait_for(&format!("Update: {version} is staged"), 0, 120);
    server.console("cancel-shutdown");
    server.wait_for("the shutdown was cancelled", staged, 60);
    let dir = server.dir.clone();
    let log = server.exit();
    assert!(
        !log.iter()
            .any(|l| l.contains("health check") || l.contains("Update: starting")),
        "{}",
        log.join("\n")
    );
    assert_eq!(std::fs::read(dir.join("README.md")).unwrap(), b"old guide");
    assert!(std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(Result::ok)
        .all(|e| !e.file_name().to_string_lossy().contains(".previous-v")));
}

#[test]
fn a_release_that_fails_its_health_check_leaves_the_old_server_running() {
    let Some((client, pack)) = content() else {
        return;
    };
    let web = Loopback::start();
    publish(&web, unhealthy_server());
    let server = Running::start("rollback", &client, &pack, &web, 19_500);
    let version = Facts::this_build().version;

    let staged = server.wait_for(&format!("Update: {version} is staged"), 0, 120);
    let failed = server.wait_for("starting the previous release again", staged, 180);
    assert!(server.find("rolled back to", staged).is_some());
    server.wait_for("Status endpoint: http://", failed, 120);
    assert!(server.status_answers(), "the old server answers /status");

    assert_eq!(server.read("README.md"), b"old guide");
    assert_eq!(server.read(&format!("empyrean-server{EXE}")), real_server());
    let state = String::from_utf8(server.read(".empyrean-update/state.json")).unwrap();
    assert!(
        state.contains(&format!("\"version\": \"{version}\"")),
        "{state}"
    );
    server.exit();
}
