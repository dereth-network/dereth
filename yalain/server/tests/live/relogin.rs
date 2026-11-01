//! Logging in again after a client was killed in the world, on the server's executable: the new
//! login logs the character left in the world off, entering it is refused until it has left, and
//! then it enters. Each step waits for the server's log or the client's model, with a time limit
//! only against a hang.

use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::path::Path;
use std::time::Instant;

use auberean_client_sdk::Action;
use yalain_testkit::session::{self as play, Drive};

use crate::session::{free_ports, must, scratch, start_server, Loopback, Running};

fn logged(dir: &Path, needle: &str) -> usize {
    std::fs::read_to_string(dir.join("server.log"))
        .unwrap_or_default()
        .matches(needle)
        .count()
}

fn client(login: SocketAddr, account: &str) -> Loopback {
    let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).expect("a client socket");
    socket.set_nonblocking(true).expect("nonblocking");
    Loopback {
        socket,
        client: play::client(login, account),
        start: Instant::now(),
        buf: vec![0; 65_536],
    }
}

/// A client killed in the world (its socket gone, no goodbye), then a new client of the account:
/// the server logs the character off at the new login, refuses the entry while it lingers, and
/// lets the new client enter it once it has left.
/// Behaviour: server.login.logging-in-again-logs-off-the-character-left-in-the-world
#[test]
fn a_client_killed_in_the_world_is_followed_by_a_new_one_that_enters_after_the_linger() {
    let dir = scratch("yalain-live-relogin");
    let port = free_ports();
    let _server = Running(start_server(&dir, port, 300));
    let login = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    let mut first = client(login, "relive");
    must(play::character_list(&mut first), &dir);
    let me = must(play::create(&mut first, "Lastwalker"), &dir);
    must(play::enter(&mut first, me), &dir);
    drop(first);

    let mut s = client(login, "relive");
    must(play::character_list(&mut s), &dir);
    must(
        play::until(
            &mut s,
            30.0,
            "the character left in the world logged off",
            |_| logged(&dir, "logged in again") > 0,
        ),
        &dir,
    );
    let refused = logged(&dir, "sent 0x10056");
    s.client().act(Action::PlayCharacter { character: me });
    must(
        play::until(&mut s, 30.0, "the entry refused", |_| {
            logged(&dir, "sent 0x10056") > refused
        }),
        &dir,
    );
    must(
        play::until(&mut s, 60.0, "the character out of the world", |_| {
            logged(&dir, "left the world") > 0
        }),
        &dir,
    );
    must(play::enter(&mut s, me), &dir);
    must(play::log_off(&mut s), &dir);
}
