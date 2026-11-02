//! Room chat through the executable's chat service over real sockets: two players of our client
//! in the world, one writing to the general channel through its scripts' command, the other
//! hearing it. Each step waits for what it should bring, with a time limit only against a hang.
//! Fixture: the built executable, the install and the authored world in a scratch folder.

use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::path::Path;
use std::time::Instant;

use auberean_client_sdk::protocol::ClientMessage;
use auberean_client_sdk::Client;
use yalain_testkit::session::{self as play, Drive};

use crate::session::{must, scratch, serving, start_server, Loopback, LIFE};

/// The world's call that gives the player its channels as it enters.
const CHANNELS: u32 = 0x8205_001E;
/// The general channel's command, as the player's scripts make it known.
const GENERAL_CHANNEL_CHAT: u32 = 0x1AA4_0194;

/// A player of our client, logged in as `account` and in the world as a new character `name`,
/// its channels given.
fn player(login: SocketAddr, account: &str, name: &str, dir: &Path) -> Loopback {
    let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).expect("a client socket");
    socket.set_nonblocking(true).expect("nonblocking");
    let mut client = play::client(login, account);
    client.observe.push(CHANNELS);
    let mut s = Loopback {
        socket,
        client,
        start: Instant::now(),
        buf: vec![0; 65_536],
    };
    must(play::character_list(&mut s), dir);
    let me = must(play::create(&mut s, name), dir);
    must(play::enter(&mut s, me), dir);
    must(
        play::until(&mut s, play::HANG, "the channels", |c| {
            c.observed.iter().any(|(_, f, _, _)| *f == CHANNELS)
        }),
        dir,
    );
    s
}

/// The lines the player's scripts wrote so far.
fn written(c: &mut Client) -> Vec<String> {
    let shown = c.shown.clone();
    shown.iter().filter_map(|s| c.write_text(&s.text)).collect()
}

/// Two players in the world: a line the first writes to the general channel's command goes to the
/// server's chat service and comes back to it as the channel's line, its answer taken; the second
/// player's scripts write it too, under the channel's name with the speaker's name.
/// Behaviour: server.chat.two-players-talk-in-a-channel-over-real-sockets
#[test]
fn two_players_talk_in_the_general_channel_over_real_sockets() {
    let dir = scratch("yalain-live-rooms");
    let (port, _server, ()) = serving(&dir, |port| (start_server(&dir, port, LIFE), ()));
    let login = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    let mut a = player(login, "roomone", "Roomspeaker", &dir);
    let mut b = player(login, "roomtwo", "Roomlistener", &dir);
    a.client().know_commands();
    let command = a
        .client()
        .with_host(|h| {
            h.commands
                .names
                .iter()
                .find(|(_, f)| **f == GENERAL_CHANNEL_CHAT)
                .map(|(n, _)| n.clone())
        })
        .flatten()
        .expect("the general channel's command");
    let before = a.client_ref().sent().len();
    a.client()
        .process_text(&format!("/{command} Well met, all."));
    assert!(
        a.client_ref().sent()[before..]
            .iter()
            .any(|s| matches!(s.message, ClientMessage::Chat(_))),
        "the line goes to the chat service"
    );
    let want = "[General] Roomspeaker: Well met, all.\n".to_owned();
    let hang = Instant::now() + std::time::Duration::from_secs_f64(play::HANG);
    loop {
        let heard_a = written(a.client()).contains(&want);
        let heard_b = written(b.client()).contains(&want);
        let answered = a
            .client_ref()
            .rooms()
            .is_some_and(|r| r.waiting().is_empty());
        if heard_a && heard_b && answered {
            break;
        }
        assert!(
            Instant::now() < hang,
            "the line heard by both ({heard_a}, {heard_b}) and answered ({answered}): {:?}",
            written(b.client())
        );
        a.step();
        b.step();
    }
}
