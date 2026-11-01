//! A whole session of our client against the server's executable on loopback.

use std::io::ErrorKind;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use auberean_client_sdk::net::LocalTime;
use auberean_client_sdk::{Action, Client};
use yalain_testkit::authored;
use yalain_testkit::session::{self as play, sent, Drive};

/// A folder of its own under cargo's temporary folder for tests, emptied.
pub(crate) fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch folder");
    dir
}

/// An even port with the three above it free on loopback now, below the ports the system hands
/// out on its own (so a socket bound meanwhile cannot take one), starting from one of this
/// process's own; never one this process handed out before, so tests running beside each other
/// each have their own.
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
        if held.len() == 4 {
            given.push(p);
            return p;
        }
    }
    panic!("no four free ports on loopback");
}

/// How long a server a test starts runs before it stops by itself, in seconds: a stop for one a
/// test left behind, far past what any test takes on a loaded machine.
pub(crate) const LIFE: u32 = 1_800;

/// The server's executable, serving the authored world's database with its files in `dir`, for at
/// most `seconds`.
pub(crate) fn start_server(dir: &Path, login: u16, seconds: u32) -> Child {
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
    let dir = scratch("yalain-live-session");
    let port = free_ports();
    let _server = Running(start_server(&dir, port, LIFE));
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
    must(play::attack_until_dead(&mut s, sentinel.id), &dir);
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
}
