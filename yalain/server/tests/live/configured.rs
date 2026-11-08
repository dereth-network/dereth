//! The executable set up from a configuration file in its working folder, with an option given on
//! its command line too, answering its status endpoint. Each step waits for the endpoint's
//! answer, with a time limit only against a hang. Fixture: the built executable, the install, the
//! authored world and a configuration file in a scratch folder.
//!
//! A world whose release of the world database is not in the cache, as after a rebuild that moved
//! this build's pin, fetches it as it starts, or says it could not and does not start. Fixture: a
//! cache of its own in a scratch folder, and a local folder as the source, so nothing is downloaded.
//!
//! A real start describes what the server's rules and a world's rows make, asked through an
//! operator's own recipe. Fixture: the built executable, the install and the authored world.

use std::io::{Read as _, Write as _};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream, UdpSocket};
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use yalain_testkit::authored::{self, Portal};
use yalain_testkit::session::{self as play};
use yalain_world::makes::{by_the_rows, by_the_server, Rows};
use yalain_world_db::release::PINNED_TAG;

use crate::session::{free_ports, must, scratch, serving, Loopback, Running, Turn, LIFE};

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

/// The executable, with a cache of its own in `dir` on any system, reading releases from `source`.
fn yalain_with_its_own_cache(dir: &Path, source: &Path) -> Command {
    let cache = dir.join("cache");
    let mut command = Command::new(env!("CARGO_BIN_EXE_yalain"));
    command
        .env("XDG_CACHE_HOME", &cache)
        .env("LOCALAPPDATA", &cache)
        .env("HOME", &cache)
        .env("YALAIN_WORLD_URL", source)
        .current_dir(dir)
        .stdin(Stdio::null());
    command
}

fn said_by(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// A world lying over a release the cache lacks fetches it from its source, with the hash beside
/// the archive, and serves it.
/// Behaviour: server.world.a-world-fetches-its-release-as-it-starts
#[test]
fn a_world_over_a_release_not_in_the_cache_fetches_it_and_starts() {
    let dir = scratch("world-fetch");
    let source = dir.join("source");
    let out = yalain_with_its_own_cache(&dir, &source)
        .args(["world", "release"])
        .arg(authored::database_dir())
        .args(["--tag", "v0.0.9", "--out"])
        .arg(&source)
        .output()
        .expect("it runs");
    assert!(out.status.success(), "{}", said_by(&out));
    let world = dir.join("world");
    std::fs::create_dir_all(&world).expect("a folder");
    std::fs::write(
        world.join("world.toml"),
        "format_version = 1\nkey = \"over-the-hall\"\ngame = \"ac2\"\nera = \"eor\"\npacks = []\n\n\
         [base]\ndb = \"test-hall\"\nversion = \"v0.0.9\"\n",
    )
    .expect("written");

    let install = auberean_dat::testing::require_target_install();
    let (_, server, ()) = serving(&dir, |login| {
        let log = std::fs::File::create(dir.join("server.log")).expect("the server's log");
        let child = yalain_with_its_own_cache(&dir, &source)
            .arg("--dats")
            .arg(&install)
            .args(["--memory", "--login-port", &login.to_string()])
            .args(["--world-port", &(login + 2).to_string()])
            .arg("--world-db")
            .arg(&world)
            .args(["--seconds", &LIFE.to_string()])
            .stdout(log.try_clone().expect("the log twice"))
            .stderr(log)
            .spawn()
            .expect("the server starts");
        (child, ())
    });
    drop(server);
    let log = std::fs::read_to_string(dir.join("server.log")).expect("the log");
    assert!(
        log.contains("needs the world database v0.0.9") && log.contains("matches"),
        "{log}"
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// A world on this build's pin, with the release neither in the cache nor to be had, does not
/// start: it says so, and leaves no store behind it.
/// Behaviour: server.world.a-world-that-cannot-fetch-its-release-does-not-start
#[test]
fn a_world_on_a_pin_it_cannot_fetch_says_so_and_does_not_start() {
    let dir = scratch("world-fetch-none");
    let store = dir.join("store.sqlite");
    let out = yalain_with_its_own_cache(&dir, &dir.join("no-such-folder"))
        .arg("--dats")
        .arg(auberean_dat::testing::require_target_install())
        .arg("--data")
        .arg(&store)
        .args(["--seconds", &LIFE.to_string()])
        .output()
        .expect("it runs");
    let text = said_by(&out);
    assert_eq!(out.status.code(), Some(1), "{text}");
    assert!(
        text.contains(&format!("could not download {PINNED_TAG}"))
            && text.contains("Nothing was changed")
            && text.contains("yalain world fetch"),
        "{text}"
    );
    assert!(!store.exists(), "no store made");
    std::fs::remove_dir_all(&dir).ok();
}

/// How many items a recipe asks for: a pack holds them all.
const TO_A_RECIPE: usize = 40;

/// The world's log once it has answered a recipe for each share of the templates, whichever way: a
/// recipe is refused by the running world's catalog, so a template the start left out stops it.
fn answered(templates: &[u32], name: &str) -> String {
    let dir = scratch(name);
    let shares: Vec<&[u32]> = templates.chunks(TO_A_RECIPE).collect();
    let install = auberean_dat::testing::require_target_install();
    let log_path = dir.join("server.log");
    let log = std::fs::File::create(&log_path).expect("the server's log");
    let mut server = Command::new(env!("CARGO_BIN_EXE_yalain"));
    server
        .arg("--dats")
        .arg(&install)
        .args(["--memory", "--bind", "127.0.0.1"])
        .args(["--login-port", &free_ports().to_string()])
        .args(["--world-port", &free_ports().to_string()])
        .arg("--world-db")
        .arg(authored::database_dir())
        .args(["--seconds", &LIFE.to_string()]);
    for (n, share) in shares.iter().enumerate() {
        let items: Vec<String> = share
            .iter()
            .map(|t| format!("{{ template = {t:#x}, quantity = 1 }}"))
            .collect();
        let letter = char::from(b'a' + u8::try_from(n).expect("few shares"));
        let recipe = dir.join(format!("recipe-{n}.toml"));
        std::fs::write(
            &recipe,
            format!(
                "account = \"builder{letter}\"\nname = \"Asker{letter}\"\nrace = 1\nsex = 4096\n\
                 items = [{}]\n",
                items.join(", ")
            ),
        )
        .expect("the recipe");
        server.arg("--make-character").arg(&recipe);
    }
    let _turn = Turn::take();
    let child = server
        .current_dir(&dir)
        .stdin(Stdio::null())
        .stdout(log.try_clone().expect("the log twice"))
        .stderr(log)
        .spawn()
        .expect("the server starts");
    let _server = Running(child);
    let started = Instant::now();
    loop {
        let text = std::fs::read_to_string(&log_path).unwrap_or_default();
        if text.matches("make-character").count() >= shares.len() {
            return text;
        }
        assert!(
            started.elapsed().as_secs_f64() < play::HANG,
            "the recipes were never answered:\n{text}"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Were all the shares' recipes made, none refused?
fn all_made(log: &str, templates: &[u32]) -> bool {
    log.matches("[ok make-character]").count() == templates.chunks(TO_A_RECIPE).count()
}

/// A start describes every pet, plant and map node the server's rules make: a recipe holding all of
/// them is made. A template no rule makes is refused, so a recipe does ask.
/// Behaviour: server.makes.a-real-start-describes-what-the-server-makes
#[test]
fn a_real_start_describes_everything_the_servers_rules_make() {
    let named = by_the_server(&Portal::open().start_rules());
    let log = answered(&named, "yalain-live-catalog-server");
    assert!(all_made(&log, &named), "{log}");
    let log = answered(&[0x4700_0001], "yalain-live-catalog-control");
    assert!(log.contains("among the world's templates"), "{log}");
}

/// A start describes what the hall's rows make.
/// Behaviour: server.makes.a-real-start-describes-what-the-rows-make
#[test]
fn a_real_start_describes_everything_a_worlds_rows_make() {
    let t = authored::world(&Portal::open());
    let named = by_the_rows(Rows {
        objects: &t.content.objects,
        stores: &t.stores,
        quests: &t.quests,
        crafting: &t.crafting,
        death_spawns: &t.death_spawns,
        pets: &t.pet_kinds,
    });
    assert!(!named.is_empty());
    let log = answered(&named, "yalain-live-catalog-rows");
    assert!(all_made(&log, &named), "{log}");
}
