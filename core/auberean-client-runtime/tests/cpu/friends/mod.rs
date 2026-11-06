//! The friends service's layer against a world's side of the friends stream, in memory: the
//! connection and the ticket, the names, the answers and events handed on, a stream lost and
//! dialed again, and the game login waiting for the service.

mod rooms;

use core::time::Duration;

use auberean_client_net::friends::stream::{
    Answer, Ended, Event, FriendStatus, Hr, Mode, Request, ServerConfig, ServerEvent, ServerOut,
    ServerStream, Ticket,
};
use auberean_client_net::friends::{FriendsLink, Io};
use auberean_client_runtime::data::{DataFiles, ENGLISH};
use auberean_client_runtime::friends::{
    heard_tell, status, ticket, weenie_result, ChatExtra, Friends, FriendsOptions, Heard, Refusal,
    UserStatus,
};
use auberean_client_runtime::{Client, ClientData, ClientSettings};

const SERVER: &str = "127.0.0.1";
const PORT: u16 = 9000;

fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

fn options(account: &str, secret: &str) -> FriendsOptions {
    FriendsOptions {
        server: SERVER.into(),
        port: PORT,
        use_ticket: true,
        ticket: Some(Ticket::new(account, secret).encode().expect("a ticket")),
        named: true,
    }
}

fn layer(account: &str, secret: &str) -> Friends {
    Friends::new(
        Box::new(FriendsLink::new("auberean-test/1.0", None)),
        &options(account, secret),
    )
}

/// The world's side: one stream at a time, the account it admits, and what it answers.
struct World {
    reachable: bool,
    stream: Option<ServerStream>,
    name: String,
    /// The secret it admits; any other ticket is refused with `refusal`.
    secret: String,
    refusal: Hr,
    /// The friends it lists, as it writes their names.
    friends: Vec<FriendStatus>,
    /// The code it answers a tell with.
    tell_answer: Hr,
    /// Every request it was asked, in order.
    asked: Vec<Request>,
    dials: u32,
    /// How its stream last ended.
    ended: Option<Ended>,
}

impl World {
    fn new() -> Self {
        Self {
            reachable: true,
            stream: None,
            name: "Dawnsong".into(),
            secret: "secret".into(),
            refusal: Hr::TICKET_REFUSED,
            friends: vec![
                FriendStatus::new("Cara@Dawnsong", Mode::ONLINE),
                FriendStatus::new("Bob@coldeve", Mode::OFFLINE),
            ],
            tell_answer: Hr::OK,
            asked: Vec::new(),
            dials: 0,
            ended: None,
        }
    }

    fn handle(&mut self, now: Duration, e: ServerEvent) {
        let Some(s) = self.stream.as_mut() else {
            return;
        };
        match e {
            ServerEvent::Ticket { cx, ticket } => {
                let ok = Ticket::decode(&ticket).is_ok_and(|t| t.secret == self.secret);
                if ok {
                    s.admit(now, cx);
                    let _ = s.enter(now, self.friends.clone());
                } else {
                    s.refuse(now, cx, self.refusal);
                }
            }
            ServerEvent::Request { cx, request } => {
                self.asked.push(request.clone());
                let (hr, answer) = match request {
                    Request::GetFriendsList => (
                        Hr::OK,
                        Answer::GetFriendsList {
                            friends: self.friends.clone(),
                        },
                    ),
                    Request::FriendAdd { user } => {
                        let _ = s.event(
                            now,
                            Event::FriendStat {
                                user: user.clone(),
                                mode: Mode::ONLINE,
                            },
                        );
                        (Hr::OK, Answer::FriendAdd)
                    }
                    Request::GetIgnoredList => (
                        Hr::OK,
                        Answer::GetIgnoredList {
                            ignored: vec!["Dan@Dawnsong".into(), "Eve@coldeve".into()],
                        },
                    ),
                    Request::SendToUserEx { .. } => (self.tell_answer, Answer::SendToUserEx),
                    other => (Hr::OK, Answer::empty(other.kind())),
                };
                let _ = s.answer(now, cx, hr, answer);
            }
            ServerEvent::Ended(ended) => self.ended = Some(ended),
            ServerEvent::Hello(_) => {}
        }
    }

    /// Close the stream from the world's side, saying why.
    fn close(&mut self, now: Duration) {
        if let Some(s) = self.stream.as_mut() {
            s.close(now, auberean_client_net::friends::ByeReason::Shutdown);
        }
    }
}

/// Carry bytes both ways until neither side has anything more.
fn carry(f: &mut Friends, w: &mut World, now: Duration) {
    loop {
        let mut moved = false;
        let link = f.connection().io().expect("the stream's link");
        while let Some(io) = link.poll_io(now) {
            moved = true;
            match io {
                Io::Dial { host, port } => {
                    assert_eq!((host.as_str(), port), (SERVER, PORT));
                    w.dials += 1;
                    if w.reachable {
                        w.stream = Some(ServerStream::new(
                            now,
                            ServerConfig::new("dawnsong", &w.name),
                        ));
                        link.opened(now);
                    } else {
                        link.failed(now);
                    }
                }
                Io::Write(bytes) => {
                    if let Some(s) = w.stream.as_mut() {
                        s.on_bytes(now, &bytes);
                    }
                }
                Io::Close => {
                    let outs = w.stream.as_mut().map_or_else(Vec::new, |s| {
                        s.closed(now);
                        s.poll(now)
                    });
                    for out in outs {
                        if let ServerOut::Event(e) = out {
                            w.handle(now, e);
                        }
                    }
                    w.stream = None;
                }
            }
        }
        let outs = w.stream.as_mut().map(|s| s.poll(now)).unwrap_or_default();
        for out in outs {
            moved = true;
            match out {
                ServerOut::Write(bytes) => link.on_bytes(now, &bytes),
                ServerOut::Close => {
                    link.closed(now);
                    w.stream = None;
                }
                ServerOut::Event(e) => w.handle(now, e),
            }
        }
        if !moved {
            return;
        }
    }
}

/// Carry, then take what the layer heard.
fn step(f: &mut Friends, w: &mut World, now: Duration) -> Vec<Heard> {
    let mut all = Vec::new();
    for _ in 0..4 {
        carry(f, w, now);
        all.extend(f.dispatch(now));
    }
    all
}

/// The client ending finishes with a ticket it presented while the service is up, once; the layer
/// ending says goodbye on the stream, dials no more and hears nothing more. A layer that presented
/// no ticket, or whose service is down, has none to finish with.
///
/// Behaviour: friends.layer.the-client-ending-finishes-with-its-ticket
#[test]
fn the_client_ending_finishes_with_its_ticket() {
    let mut w = World::new();
    let mut f = ready(&mut w);
    assert!(f.finish_with_ticket());
    assert!(!f.finish_with_ticket(), "once");
    f.end();
    let heard = step(&mut f, &mut w, Duration::from_secs(1));
    assert_eq!(heard, [], "nothing is listening");
    assert_eq!(
        w.ended,
        Some(Ended::PeerBye(
            auberean_client_net::friends::ByeReason::Shutdown
        ))
    );
    assert!(!f.connected());
    assert!(!f.finish_with_ticket());
    assert!(step(&mut f, &mut w, Duration::from_secs(60)).is_empty());
    assert_eq!(w.dials, 1, "not dialed again");

    let mut w = World::new();
    let mut f = layer("ann", "secret");
    assert!(!f.finish_with_ticket(), "not connected");
    f.connect(SERVER, PORT);
    step(&mut f, &mut w, Duration::ZERO);
    assert!(f.connected());
    assert!(!f.finish_with_ticket(), "no ticket presented");
}

/// A layer connected, its ticket taken and its events listened for, as the login leaves it.
fn ready(w: &mut World) -> Friends {
    let mut f = layer("ann", "secret");
    f.register_for_connect_events();
    f.connect(SERVER, PORT);
    let heard = step(&mut f, w, Duration::ZERO);
    assert_eq!(heard, [Heard::Connection(status::CONNECTED)]);
    assert!(f.present_ticket());
    let heard = step(&mut f, w, Duration::ZERO);
    assert_eq!(heard, [Heard::Authenticated(ticket::SUCCESS)]);
    f.register_for_chat_events();
    f.register_for_friends_events();
    f.set_world_name(&units("Dawnsong"));
    f
}

fn status_of(name: &str, mode: u32) -> UserStatus {
    UserStatus {
        name: units(name),
        location: Vec::new(),
        mode,
    }
}

/// The layer connects, presents the login's ticket and is told it was taken; events (the friends
/// list the world pushes as the character enters) reach the scripts only once the layer listens
/// for them, which the login does after the ticket.
///
/// Behaviour: friends.layer.events-reach-the-scripts-only-once-the-ticket-is-taken
#[test]
fn events_reach_the_scripts_only_once_the_ticket_is_taken() {
    let mut w = World::new();
    let mut f = layer("ann", "secret");
    f.register_for_connect_events();
    f.connect(SERVER, PORT);
    assert_eq!(
        step(&mut f, &mut w, Duration::ZERO),
        [Heard::Connection(status::CONNECTED)]
    );
    assert!(f.present_ticket());
    // The world pushes the list as it admits the stream; nothing listens yet.
    assert_eq!(
        step(&mut f, &mut w, Duration::ZERO),
        [Heard::Authenticated(ticket::SUCCESS)]
    );
    f.register_for_friends_events();
    f.set_world_name(&units("Dawnsong"));
    let now = Duration::from_secs(1);
    let s = w.stream.as_mut().expect("a stream");
    s.event(
        now,
        Event::FStart {
            friends: w.friends.clone(),
        },
    )
    .expect("sent");
    assert_eq!(
        step(&mut f, &mut w, now),
        [Heard::Entered(vec![
            status_of("Cara", 1),
            status_of("Bob@coldeve", 0)
        ])]
    );
}

/// A ticket the world refuses comes back as a refusal; a banned account as its own.
///
/// Behaviour: friends.layer.a-refused-ticket-comes-back-refused-and-a-banned-account-banned
#[test]
fn a_refused_ticket_comes_back_refused_and_a_banned_account_banned() {
    for (refusal, expect) in [
        (Hr::TICKET_REFUSED, ticket::FAILED),
        (Hr::BANNED, ticket::BANNED),
    ] {
        let mut w = World::new();
        w.refusal = refusal;
        let mut f = layer("ann", "wrong");
        f.register_for_connect_events();
        f.connect(SERVER, PORT);
        step(&mut f, &mut w, Duration::ZERO);
        assert!(f.present_ticket());
        assert_eq!(
            step(&mut f, &mut w, Duration::ZERO),
            [Heard::Authenticated(expect)]
        );
    }
}

/// The world's name, from the login, is added to a name with no `@` and taken off one whose part
/// from the `@` is exactly `@` and the world's name; any other name is left as it is. The first
/// name the login gives stands.
///
/// Behaviour: friends.layer.the-own-worlds-name-is-added-and-taken-off-exactly
#[test]
fn the_own_worlds_name_is_added_and_taken_off_exactly() {
    let mut f = layer("ann", "secret");
    assert_eq!(f.local_to_global(&units("Cara")), units("Cara"));
    assert!(f.set_world_name(&units("Dawnsong")));
    assert!(!f.set_world_name(&units("Elsewhere")));
    assert_eq!(f.world_suffix(), units("@Dawnsong"));
    assert_eq!(f.local_to_global(&units("Cara")), units("Cara@Dawnsong"));
    assert_eq!(
        f.local_to_global(&units("Bob@coldeve")),
        units("Bob@coldeve")
    );
    assert_eq!(f.global_to_local(&units("Cara@Dawnsong")), units("Cara"));
    assert_eq!(
        f.global_to_local(&units("Cara@dawnsong")),
        units("Cara@dawnsong")
    );
    assert_eq!(
        f.global_to_local(&units("Bob@coldeve")),
        units("Bob@coldeve")
    );
    assert_eq!(
        f.global_to_local(&units("Cara@Dawnsong@x")),
        units("Cara@Dawnsong@x")
    );
}

/// Requests go out with names as the service writes them, and their answers come back with what
/// the request kept: the friend added (its state told first), the friends list and the ignore list
/// with this world's names shortened and the asker's word, a tell's answer with the name and text
/// as the player typed them.
///
/// Behaviour: friends.layer.answers-come-back-with-what-the-request-kept
#[test]
fn answers_come_back_with_what_the_request_kept() {
    let mut w = World::new();
    let mut f = ready(&mut w);
    let now = Duration::from_secs(1);
    assert!(f.friend_add(&units("Cara")));
    assert!(f.get_friends_list(7));
    assert!(f.get_ignored_list(3));
    let extra = ChatExtra::new(&units("Ann [F]"), 0);
    assert!(f.send_to_user(&units("Bob@coldeve"), &units("hi"), &extra));
    let heard = step(&mut f, &mut w, now);
    assert_eq!(
        heard,
        [
            Heard::FriendState(status_of("Cara", 1)),
            Heard::FriendAdd {
                result: 0,
                user: units("Cara")
            },
            Heard::FriendsList {
                result: 0,
                context: 7,
                friends: vec![status_of("Cara", 1), status_of("Bob@coldeve", 0)],
            },
            Heard::IgnoredList {
                result: 0,
                context: 3,
                names: vec![units("Dan"), units("Eve@coldeve")],
            },
            Heard::SendToUser {
                user: units("Bob@coldeve"),
                text: units("hi"),
                result: 0
            },
        ]
    );
    assert_eq!(
        w.asked[0],
        Request::FriendAdd {
            user: "Cara@Dawnsong".into()
        }
    );
    let Request::SendToUserEx { user, text, extra } = &w.asked[3] else {
        panic!("a tell: {:?}", w.asked[3]);
    };
    assert_eq!((user.as_str(), text.as_str()), ("Bob@coldeve", "hi"));
    let carried = ChatExtra::unpack(&extra.expect("extra data").0).expect("it reads");
    assert_eq!(carried.meta_letters, units("F"));
}

/// A tell the world refuses comes back to the scripts as the client's own result for its code: not
/// online, no such player, and anything else as the unknown result.
///
/// Behaviour: friends.layer.a-tells-code-reaches-the-scripts-as-the-clients-own-result
#[test]
fn a_tells_code_reaches_the_scripts_as_the_clients_own_result() {
    for (hr, result) in [
        (Hr::NOT_ONLINE, 0xC73B),
        (Hr::NO_SUCH_USER, 0xC73D),
        (Hr::TIMED_OUT, 0xC74E),
        (Hr::DENIED, 0xC73F),
        (Hr(0xC112_4242), 0xC741),
    ] {
        assert_eq!(weenie_result(hr.0), result);
        let mut w = World::new();
        w.tell_answer = hr;
        let mut f = ready(&mut w);
        let extra = ChatExtra::new(&units("Ann [F]"), 0);
        assert!(f.send_to_user(&units("Bob@coldeve"), &units("hi"), &extra));
        assert_eq!(
            step(&mut f, &mut w, Duration::from_secs(1)),
            [Heard::SendToUser {
                user: units("Bob@coldeve"),
                text: units("hi"),
                result
            }]
        );
    }
}

/// While the service is down a request is refused at once and nothing comes back for it later;
/// a tell with no name or no text is refused too.
///
/// Behaviour: friends.layer.a-request-while-the-service-is-down-is-refused-at-once
#[test]
fn a_request_while_the_service_is_down_is_refused_at_once() {
    let mut w = World::new();
    w.reachable = false;
    let mut f = layer("ann", "secret");
    f.register_for_connect_events();
    f.connect(SERVER, PORT);
    assert_eq!(
        step(&mut f, &mut w, Duration::ZERO),
        [Heard::Connection(status::LOST)]
    );
    let extra = ChatExtra::new(&[], 0);
    assert!(!f.friend_add(&units("Cara")));
    assert!(!f.send_to_user(&units("Bob@coldeve"), &units("hi"), &extra));
    assert!(step(&mut f, &mut w, Duration::from_secs(5)).is_empty());
    let mut w = World::new();
    let mut f = ready(&mut w);
    assert!(!f.send_to_user(&units(""), &units("hi"), &extra));
    assert!(!f.send_to_user(&units("Bob@coldeve"), &units(""), &extra));
}

/// A first attempt that fails is not tried again; a stream that stood and is then lost is dialed
/// again on the stream's schedule, and coming back is told as such.
///
/// Behaviour: friends.layer.a-lost-stream-is-dialed-again-only-once-one-stood
#[test]
fn a_lost_stream_is_dialed_again_only_once_one_stood() {
    let mut w = World::new();
    w.reachable = false;
    let mut f = layer("ann", "secret");
    f.register_for_connect_events();
    f.connect(SERVER, PORT);
    step(&mut f, &mut w, Duration::ZERO);
    w.reachable = true;
    step(&mut f, &mut w, Duration::from_secs(60));
    assert_eq!(w.dials, 1, "never dialed again");
    assert!(!f.connected());

    let mut w = World::new();
    let mut f = ready(&mut w);
    w.close(Duration::from_secs(5));
    assert_eq!(
        step(&mut f, &mut w, Duration::from_secs(5)),
        [Heard::Connection(status::DISCONNECTED)]
    );
    assert_eq!(w.dials, 1);
    // The first wait is a second.
    assert!(step(&mut f, &mut w, Duration::from_millis(5_500)).is_empty());
    assert_eq!(
        step(&mut f, &mut w, Duration::from_secs(6)),
        [Heard::Connection(status::RECONNECTED)]
    );
    assert_eq!(w.dials, 2);
}

/// A tell's extra data carries the teller's meta letters and chat flags; a heard tell shows the
/// teller's name with the letters in brackets after it, its text escaped so nothing in it reads as
/// markup; with no extra data the flags are 0 and the name bare.
///
/// Behaviour: friends.layer.a-heard-tell-shows-the-tellers-meta-letters-and-escapes-its-text
#[test]
fn a_heard_tell_shows_the_tellers_meta_letters_and_escapes_its_text() {
    let extra = ChatExtra::new(&units("Ann [Fv]"), 0x4000_0001);
    assert_eq!(extra.meta_letters, units("Fv"));
    assert_eq!(ChatExtra::unpack(&extra.pack()), Some(extra.clone()));
    let (name, text, flags) = heard_tell(
        units("Ann@Dawnsong"),
        &units("a [b]"),
        Some(extra.to_stream()),
    );
    assert_eq!(name, units("Ann@Dawnsong[Fv]"));
    assert_eq!(text, units("a \\[b\\]"));
    assert_eq!(flags, 0x4000_0001);
    let (name, _, flags) = heard_tell(units("Bob@coldeve"), &units("hi"), None);
    assert_eq!((name, flags), (units("Bob@coldeve"), 0));
}

fn client() -> Client {
    client_with(b"secret")
}

/// A client logging in with `secret` given directly (none when empty).
fn client_with(secret: &[u8]) -> Client {
    Client::new(
        ClientSettings {
            login_server: "127.0.0.1:9000".parse().expect("an address"),
            account: b"ann".to_vec(),
            ticket: secret.to_vec(),
            timestamp: 0,
            session_key: [0; 64],
            script_sends: false,
        },
        ClientData {
            files: DataFiles {
                files: Vec::new(),
                language: ENGLISH,
            },
            scripts: None,
        },
    )
}

/// One client frame with the friends stream carried.
fn client_frame(c: &mut Client, w: &mut World, t: f64) {
    let now = Duration::from_secs_f64(t);
    for _ in 0..4 {
        if let Some(f) = c.friends_mut() {
            carry(f, w, now);
        }
        c.frame(auberean_client_net::LocalTime(t));
    }
}

/// The game login waits for the friends service: it goes out once the ticket is taken. When the
/// service cannot be reached, a login given its secret directly goes out at once (the player plays
/// without friends), and so does one whose service was not named; a login without its secret, its
/// service named, ends there.
///
/// Behaviour: friends.login.the-game-login-waits-for-the-friends-service
#[test]
fn the_game_login_waits_for_the_friends_service() {
    let mut w = World::new();
    let mut c = client();
    c.use_friends(layer("ann", "secret"));
    assert!(c.poll_transmit().is_none());
    client_frame(&mut c, &mut w, 1.0);
    assert!(c.friends_refusal().is_none());
    assert!(c.poll_transmit().is_some(), "the login went out");
    assert!(w.stream.as_ref().is_some_and(ServerStream::is_admitted));

    let mut w = World::new();
    w.reachable = false;
    let mut c = client();
    c.use_friends(layer("ann", "secret"));
    client_frame(&mut c, &mut w, 1.0);
    assert!(c.friends_refusal().is_none());
    assert!(
        c.poll_transmit().is_some(),
        "the login went out without the service"
    );

    let mut w = World::new();
    w.reachable = false;
    let mut c = client_with(b"");
    c.use_friends(layer("ann", ""));
    client_frame(&mut c, &mut w, 1.0);
    assert_eq!(c.friends_refusal(), Some(Refusal::Failed));
    assert!(c.poll_transmit().is_none(), "no login without its secret");

    let mut w = World::new();
    w.reachable = false;
    let mut c = client_with(b"");
    let mut unnamed = options("ann", "");
    unnamed.named = false;
    c.use_friends(Friends::new(
        Box::new(FriendsLink::new("auberean-test/1.0", None)),
        &unnamed,
    ));
    client_frame(&mut c, &mut w, 1.0);
    assert!(c.friends_refusal().is_none());
    assert!(
        c.poll_transmit().is_some(),
        "the login went out: no service was named"
    );
}

/// A ticket the service refuses ends the login: no game login goes out.
///
/// Behaviour: friends.login.a-refused-ticket-ends-the-login
#[test]
fn a_refused_ticket_ends_the_login() {
    for (refusal, why) in [
        (Hr::TICKET_REFUSED, Refusal::Failed),
        (Hr::BANNED, Refusal::Banned),
    ] {
        let mut w = World::new();
        w.refusal = refusal;
        let mut c = client();
        c.use_friends(layer("ann", "wrong"));
        client_frame(&mut c, &mut w, 1.0);
        assert_eq!(c.friends_refusal(), Some(why));
        assert!(c.poll_transmit().is_none());
    }
}
