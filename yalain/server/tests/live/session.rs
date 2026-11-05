//! A whole session of our client against the server's executable on loopback, and a large record
//! a client asks it for, served whole.

use std::io::ErrorKind;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, UdpSocket};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

use auberean_client_sdk::dat::locate::role_file;
use auberean_client_sdk::dat::overlay::WORLD_FOLDER;
use auberean_client_sdk::net::LocalTime;
use auberean_client_sdk::protocol::login::{DataRecord, DataRequest};
use auberean_client_sdk::protocol::{ClientMessage, ServerMessage};
use auberean_client_sdk::{Action, Client};
use dereth_dat::overlay::OverlayDir;
use dereth_dat::ModernDat;
use dereth_primitives::DataId;
use yalain_testkit::authored;
use yalain_testkit::bot::Bot;
use yalain_testkit::session::{self as play, sent, Drive};
use yalain_world::download::{file_of, packed, DataFiles};

/// A folder of its own under cargo's temporary folder for tests, emptied.
pub(crate) fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch folder");
    dir
}

/// An even port with the three above it free on loopback now as UDP, and the port itself as TCP,
/// starting from one of this process's own; never one this process handed out before, so tests
/// running beside each other each have their own. Each is let go before it is returned, so another
/// program can take one before the server does: [`serving`] starts the server again on fresh ports.
pub(crate) fn free_ports() -> u16 {
    static GIVEN: Mutex<Vec<u16>> = Mutex::new(Vec::new());
    let mut given = GIVEN
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let start = 20_000 + u16::try_from(std::process::id() % 5_000).unwrap_or(0) * 2;
    for p in (start..40_000).step_by(4) {
        if given.contains(&p) {
            continue;
        }
        let held: Vec<UdpSocket> = (0..4)
            .filter_map(|i| UdpSocket::bind((Ipv4Addr::LOCALHOST, p + i)).ok())
            .collect();
        let stream = TcpListener::bind((Ipv4Addr::LOCALHOST, p)).is_ok();
        if held.len() == 4 && stream {
            given.push(p);
            return p;
        }
    }
    panic!("no four free ports on loopback");
}

/// How many times a server is started on fresh ports before a test gives up.
pub(crate) const STARTS: u32 = 5;

/// What the server's log says once it listens for clients, and once it has every port: its
/// friends service's TCP listener is the last it takes.
const LISTENING: &str = " [listening]";
const SERVING: &str = "friends service listening on TCP";

/// How many servers this test run starts at once. Each reads the install's tables and compiles its
/// world database before it listens, which on a loaded machine takes minutes; the others wait their
/// turn, and each one's wait against a hang begins only when it starts.
const STARTING_AT_ONCE: usize = 2;

/// A turn to start a server: held from its start until it serves or stops.
pub(crate) struct Turn;

static STARTING: (Mutex<usize>, Condvar) = (Mutex::new(0), Condvar::new());

impl Turn {
    /// Waits until fewer than [`STARTING_AT_ONCE`] servers are starting.
    pub(crate) fn take() -> Self {
        let (n, free) = &STARTING;
        let mut n = n.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        while *n >= STARTING_AT_ONCE {
            n = free
                .wait(n)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
        *n += 1;
        Self
    }
}

impl Drop for Turn {
    fn drop(&mut self) {
        let (n, free) = &STARTING;
        let mut n = n.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        *n -= 1;
        free.notify_one();
    }
}

/// The exit code of a server whose ports another program holds.
pub(crate) const PORTS_TAKEN: i32 = 3;

/// Whether the log holds the server's line saying another program holds a port of its.
pub(crate) fn ports_taken(log: &str) -> bool {
    log.lines().any(|l| {
        l.contains(" yalain: binding the ports: ") && l.trim_end().ends_with(" [ports-taken]")
    })
}

/// The server's executable started by `start` on a login port this test chose (the world's two
/// above), with its log in `dir`'s `server.log`, once its log says it listens and has every port:
/// the port, the server and what `start` gave beside it. It starts at its [`Turn`]. Each port was
/// let go before the server took it, so another program can take one first: the server then stops
/// with its ports-taken exit code, and is started again on fresh ports. Fails when the server
/// stops for anything else.
pub(crate) fn serving<T>(dir: &Path, start: impl FnMut(u16) -> (Child, T)) -> (u16, Running, T) {
    serving_on(dir, free_ports, start)
}

/// [`serving`] with the login ports chosen by `ports` for each start.
fn serving_on<T>(
    dir: &Path,
    mut ports: impl FnMut() -> u16,
    mut start: impl FnMut(u16) -> (Child, T),
) -> (u16, Running, T) {
    for _ in 0..STARTS {
        let turn = Turn::take();
        let port = ports();
        let (child, more) = start(port);
        let mut server = Running(child);
        let started = Instant::now();
        loop {
            let exit = server.0.try_wait().expect("the server's state");
            let log = std::fs::read_to_string(dir.join("server.log")).unwrap_or_default();
            if log.contains(LISTENING) && log.contains(SERVING) {
                drop(turn);
                return (port, server, more);
            }
            if let Some(exit) = exit {
                assert!(
                    exit.code() == Some(PORTS_TAKEN) && ports_taken(&log),
                    "the server stopped at its start: {exit}\n{log}"
                );
                eprintln!(
                    "a port of {port} was taken by another program; started again on fresh ports"
                );
                break;
            }
            assert!(
                started.elapsed().as_secs_f64() < play::HANG,
                "the server never had its ports\n{log}"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    panic!("the server lost a port to another program {STARTS} times");
}

/// How long a server a test starts runs before it stops by itself, in seconds: a stop for one a
/// test left behind, far past what any test takes on a loaded machine.
pub(crate) const LIFE: u32 = 1_800;

/// The server's executable, serving the authored world's database with its files in `dir`, for at
/// most `seconds`.
pub(crate) fn start_server(dir: &Path, login: u16, seconds: u32) -> Child {
    start_server_with(dir, login, seconds, &[])
}

/// [`start_server`] with more options on its command line.
pub(crate) fn start_server_with(dir: &Path, login: u16, seconds: u32, more: &[&str]) -> Child {
    let install = auberean_dat::testing::require_target_install();
    let log = std::fs::File::create(dir.join("server.log")).expect("the server's log");
    Command::new(env!("CARGO_BIN_EXE_yalain"))
        .arg("--dats")
        .arg(&install)
        .args(["--memory", "--login-port", &login.to_string()])
        .args(["--world-port", &(login + 2).to_string()])
        .arg("--world-db")
        .arg(authored::database_dir())
        .args(["--seconds", &seconds.to_string(), "--verbose"])
        .args(more)
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(log.try_clone().expect("the log twice"))
        .stderr(log)
        .spawn()
        .expect("the server starts")
}

/// Our client on a loopback socket, on the real clock.
pub(crate) struct Loopback {
    pub(crate) socket: UdpSocket,
    pub(crate) client: Client,
    pub(crate) start: Instant,
    pub(crate) buf: Vec<u8>,
}

impl Loopback {
    fn time(&self) -> LocalTime {
        LocalTime(1.0 + self.start.elapsed().as_secs_f64())
    }
}

impl Drive for Loopback {
    fn client(&mut self) -> &mut Client {
        &mut self.client
    }

    fn client_ref(&self) -> &Client {
        &self.client
    }

    fn step(&mut self) {
        loop {
            match self.socket.recv_from(&mut self.buf) {
                Ok((n, from)) => {
                    let now = self.time();
                    self.client.handle_datagram(from, &self.buf[..n], now);
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                // A reset from a port not answering yet: the client resends.
                Err(_) => break,
            }
        }
        let now = self.time();
        self.client.frame(now);
        while let Some((to, bytes)) = self.client.poll_transmit() {
            let _ = self.socket.send_to(&bytes, to);
        }
        std::thread::sleep(Duration::from_millis(5));
    }

    fn now(&self) -> f64 {
        self.time().0
    }
}

/// Stops the server however the test ends.
pub(crate) struct Running(pub(crate) Child);

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// A step's result, or the test's failure with the server's log.
pub(crate) fn must<T>(r: Result<T, String>, dir: &Path) -> T {
    r.unwrap_or_else(|e| {
        let log = std::fs::read_to_string(dir.join("server.log")).unwrap_or_default();
        panic!("{e}\nthe server's log:\n{log}")
    })
}

/// The server's executable, started on loopback with the install and an authored world, plays a
/// whole session with our client over real sockets: the character list, a character made and
/// entered, a walk, the people and creatures around, a creature selected, examined and killed,
/// its corpse opened and its item taken, a saying heard, the guard's line heard on a use, the
/// shard to the town, a log-off and an entry again where the character left, the sword it
/// wielded still in its hand.
/// Behaviour: session.server.our-client-plays-a-whole-session-on-the-server-executable
#[test]
fn our_client_plays_a_whole_session_on_the_server_executable() {
    // A new character can lose its fight with the sentinel, which strikes back (about one fight in
    // eighty): the session is played again from the start, on a server of its own, when it does.
    for _ in 0..FIGHTS {
        if plays_a_whole_session() {
            return;
        }
    }
    panic!("the player lost the fight {FIGHTS} times");
}

/// How many times the whole session is played before a lost fight fails it.
const FIGHTS: u32 = 3;

/// The whole session, played through; `false` when the player lost the fight.
fn plays_a_whole_session() -> bool {
    let dir = scratch("yalain-live-session");
    let (port, _server, ()) = serving(&dir, |port| (start_server(&dir, port, LIFE), ()));
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
    let me = must(play::create(&mut s, "Loopwalker"), &dir);
    must(play::enter(&mut s, me), &dir);
    assert_eq!(s.client_ref().body.cell, authored::HALL, "at the start");
    must(
        play::see(
            &mut s,
            &[authored::GUARD, authored::SHARD, authored::SENTINEL],
        ),
        &dir,
    );
    // A weapon in hand (a human has no attack with an empty hand), and nothing selected again.
    let sword = must(play::wield(&mut s, authored::SWORD), &dir);
    s.client().act(Action::Select { target: 0 });
    s.client().act(Action::Examination {
        tabs: Some(play::TABS),
    });
    let sentinel = play::find(s.client_ref(), authored::SENTINEL).expect("a sentinel");
    let at = sentinel.place.expect("placed").origin;
    let reports = must(play::walk_to(&mut s, [at.x, at.y, at.z], 1.5), &dir);
    assert!(reports >= 2, "the walk is reported ({reports} reports)");
    let order = must(play::select_and_examine(&mut s, sentinel.id), &dir);
    assert_eq!(
        order,
        [sent::SET_SELECTION, sent::QUERY_EXAMINATION, sent::LOOK_AT]
    );
    match play::attack_until_dead(&mut s, sentinel.id) {
        Err(e) if e.starts_with(play::LOST_THE_FIGHT) => {
            eprintln!("{e}; played again");
            return false;
        }
        r => must(r, &dir),
    }
    let (_, item) = must(play::open_corpse(&mut s), &dir);
    must(play::take(&mut s, item), &dir);
    must(play::say(&mut s, "Well met."), &dir);
    let guard = play::find(s.client_ref(), authored::GUARD).expect("the guard");
    let at = guard.place.expect("placed").origin;
    must(play::walk_to(&mut s, [at.x, at.y, at.z], 2.0), &dir);
    must(play::talk_to(&mut s, guard.id), &dir);
    let shard = play::find(s.client_ref(), authored::SHARD).expect("the shard");
    must(play::walk_to(&mut s, authored::SHARD_AT, 2.0), &dir);
    must(play::take_portal(&mut s, shard.id, authored::TOWN), &dir);
    play::wait(&mut s, 1.0);
    must(play::log_off(&mut s), &dir);
    must(play::enter(&mut s, me), &dir);
    assert_eq!(s.client_ref().body.cell, authored::TOWN, "where it left");
    let again = s
        .client_ref()
        .model
        .player_object()
        .cloned()
        .expect("the player");
    assert!(
        play::ground_distance(authored::TOWN_AT, &again).is_some_and(|d| d < 1.0),
        "the place it left: {:?}",
        again.place
    );
    must(
        play::until(&mut s, play::HANG, "the sword in hand again", |c| {
            c.model
                .object(sword)
                .is_some_and(|o| o.parent.is_some_and(|(p, _)| p == me) && o.wielder == Some(me))
        }),
        &dir,
    );
    must(play::log_off(&mut s), &dir);
    true
}

/// The script library: its kind and id.
const LIBRARY_KIND: u32 = 0x4000_004F;
const LIBRARY: u32 = 0x5600_0005;
/// The data download's queue.
const DATABASE: u16 = 5;

/// The script library, seven megabytes compressed and sixteen thousand fragments, goes from the
/// server's executable to a client on loopback whole, as the install holds it, at the server's
/// pace: sent all at once, most of it was lost in the client's socket and never came.
/// Behaviour: server.net.large-record-served-over-real-sockets
#[test]
fn a_client_on_loopback_takes_the_script_library_whole_from_the_server_executable() {
    let (file, name) = file_of(LIBRARY_KIND).expect("a portal kind");
    let record = authored::install_files()
        .read(file, LIBRARY)
        .expect("the install holds the library");
    let want = packed(name, LIBRARY_KIND, LIBRARY, &record);

    let dir = scratch("yalain-live-download");
    let (port, _server, ()) = serving(&dir, |port| (start_server(&dir, port, LIFE), ()));
    let got = ask_for(port, "downloader", LIBRARY_KIND, LIBRARY);
    assert_eq!(
        (got.dat_file, got.kind, got.size, got.bytes.len()),
        (want.dat_file, want.kind, want.size, want.bytes.len())
    );
    assert!(
        got.bytes == want.bytes,
        "the install's record, byte for byte"
    );
}

/// What a client on loopback logged in as `account` gets when it asks for a record.
fn ask_for(port: u16, account: &str, kind: u32, id: u32) -> DataRecord {
    let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).expect("a client socket");
    socket.set_nonblocking(true).expect("nonblocking");
    let login = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    let mut bot = Bot::new(socket.local_addr().expect("its address"), login);
    let start = Instant::now();
    let time = || LocalTime(1.0 + start.elapsed().as_secs_f64());
    bot.log_in(account, b"ticket", time());
    let mut buf = vec![0; 65_536];
    let mut asked = false;
    let mut checked = Instant::now();
    loop {
        assert!(
            start.elapsed().as_secs_f64() < play::HANG,
            "the record never came: {} messages",
            bot.messages(false).len()
        );
        loop {
            match socket.recv_from(&mut buf) {
                Ok((n, from)) => bot.on_datagram(from, &buf[..n], time()),
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(_) => break,
            }
        }
        if bot.logged_in() && !asked {
            bot.send(
                false,
                DATABASE,
                &ClientMessage::DataRequest(DataRequest { kind, id }),
            );
            asked = true;
        }
        for (to, bytes) in bot.tick(time()) {
            let _ = socket.send_to(&bytes, to);
        }
        if checked.elapsed() > Duration::from_millis(500) {
            checked = Instant::now();
            let record = bot.messages(false).into_iter().find_map(|m| match m {
                ServerMessage::DataRecord(r) if r.id == id => Some(r),
                _ => None,
            });
            if let Some(r) = record {
                return r;
            }
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// The authored world in a folder of its own, its overlay giving a landblock the world's own bytes.
fn authored_world_with_overlay(dir: &Path, landblock: u32, bytes: &[u8]) -> PathBuf {
    fn copied(from: &Path, to: &Path) {
        std::fs::create_dir_all(to).expect("a folder");
        for e in std::fs::read_dir(from).expect("the database").flatten() {
            let (f, t) = (e.path(), to.join(e.file_name()));
            if f.is_dir() {
                copied(&f, &t);
            } else {
                std::fs::copy(&f, &t).expect("a file");
            }
        }
    }
    let world = dir.join("world");
    copied(&authored::database_dir(), &world);
    let cell = authored::install_files().cell.expect("the cell file");
    let overlay = OverlayDir::new(&world.join(WORLD_FOLDER)).expect("the folder");
    let mut w = overlay
        .writer(
            ModernDat::Cell,
            cell.shared(),
            role_file(ModernDat::Cell),
            "test-hall",
            0,
        )
        .expect("a writer");
    w.put(DataId(landblock), bytes, 1, 0, 0)
        .expect("the record");
    w.flush(0).expect("written");
    world
}

/// **The server's executable, given a world with an overlay, answers a client's request for a
/// record with the overlay's.**
///
/// Behaviour: server.data.answers-from-a-worlds-overlay-over-real-sockets
#[test]
fn a_client_on_loopback_gets_a_landblock_from_the_worlds_overlay() {
    const LANDBLOCK: u32 = 0x4000_0050;
    let cell = authored::install_files().cell.expect("the cell file");
    let id = cell
        .entries()
        .map(|e| e.id)
        .find(|id| id & 0xFFFF == 0xFFFF)
        .expect("a landblock");
    let bytes = b"the world's own landblock";
    let (_, name) = file_of(LANDBLOCK).expect("a cell kind");

    let dir = scratch("yalain-live-overlay");
    let world = authored_world_with_overlay(&dir, id, bytes);
    let world = world.display().to_string();
    let (port, _server, ()) = serving(&dir, |port| {
        (
            start_server_with(&dir, port, LIFE, &["--world-db", &world]),
            (),
        )
    });
    let got = ask_for(port, "overlaid", LANDBLOCK, id);
    let want = packed(name, LANDBLOCK, id, bytes);
    assert_eq!((got.dat_file, got.size), (want.dat_file, want.size));
    assert!(
        got.bytes == want.bytes,
        "the overlay's record, byte for byte"
    );
    let log = std::fs::read_to_string(dir.join("server.log")).unwrap_or_default();
    assert!(log.contains("test-hall's overlay"), "{log}");
}

mod starts {
    //! The live tests' own start of the server: one whose port another program took before it
    //! bound it is started again on fresh ports.
    //!
    //! Behaviour: none (checks the live tests' own harness: how they start the server)

    use std::net::{Ipv4Addr, UdpSocket};

    use super::{free_ports, scratch, serving_on, start_server, LIFE};

    #[test]
    fn a_server_that_loses_a_port_before_it_binds_it_is_started_again_on_fresh_ports() {
        let dir = scratch("yalain-live-lost-port");
        // The first start's ports: chosen until this test holds the world's second port of them
        // (another program may take a port first), the socket holding it kept to the end.
        let mut held: Option<UdpSocket> = None;
        let mut chosen = Vec::new();
        let (port, _server, ()) = serving_on(
            &dir,
            || loop {
                let port = free_ports();
                if held.is_some() {
                    break port;
                }
                if let Ok(s) = UdpSocket::bind((Ipv4Addr::LOCALHOST, port + 3)) {
                    held = Some(s);
                    break port;
                }
            },
            |port| {
                chosen.push(port);
                (start_server(&dir, port, LIFE), ())
            },
        );
        assert!(held.is_some(), "a port held");
        assert!(chosen.len() >= 2, "started again: {chosen:?}");
        assert_eq!(chosen.last(), Some(&port));
        let log = std::fs::read_to_string(dir.join("server.log")).unwrap_or_default();
        let listening = format!("listening on 127.0.0.1:{port}/{} (login)", port + 1);
        assert!(log.contains(&listening), "{log}");
    }
}

/// The human starting sword, whose script sets its value at 1.
const STARTING_SWORD: u32 = 0x4700_051C;
/// How long the sale may take, against a hang only.
const HANG: f64 = 30.0;
/// The money quality.
const MONEY: u32 = 3100;

/// A new character's starting sword dragged to the money bag brings its 1 gold and goes.
/// Behaviour: server.inventory.a-new-characters-weapon-sells-for-its-scripts-value
#[test]
fn a_new_characters_starting_sword_sold_at_the_money_bag_brings_its_gold() {
    let dir = scratch("yalain-live-money-bag");
    let (port, _server, ()) = serving(&dir, |port| (start_server(&dir, port, LIFE), ()));
    let login = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).expect("a client socket");
    socket.set_nonblocking(true).expect("nonblocking");
    let mut s = Loopback {
        socket,
        client: play::client(login, "sellive"),
        start: Instant::now(),
        buf: vec![0; 65_536],
    };
    must(play::character_list(&mut s), &dir);
    let me = must(play::create(&mut s, "Firstsale"), &dir);
    must(play::enter(&mut s, me), &dir);
    let money = |c: &Client| {
        c.model
            .player_object()
            .and_then(|o| {
                auberean_client_sdk::runtime::description::int(&o.weenie, MONEY)
                    .or_else(|| o.qualities.ints.get(&MONEY).copied())
            })
            .unwrap_or(0)
    };
    let held = |c: &Client| {
        c.model
            .objects()
            .find(|o| o.weenie.entity_did == STARTING_SWORD && o.container == Some(me))
            .map(|o| o.id)
    };
    must(
        play::until(&mut s, HANG, "the starting sword in the pack", |c| {
            held(c).is_some()
        }),
        &dir,
    );
    let item = held(s.client_ref()).expect("held");
    let before = money(s.client_ref());
    s.client().act(auberean_client_sdk::Action::IntoMoney {
        item,
        quantity: 1,
        slot: 1,
        from: me,
    });
    must(
        play::until(&mut s, HANG, "the sword gone and the gold in", |c| {
            held(c).is_none() && money(c) > before
        }),
        &dir,
    );
    assert_eq!(money(s.client_ref()), before + 1, "its script's 1 gold");
}
