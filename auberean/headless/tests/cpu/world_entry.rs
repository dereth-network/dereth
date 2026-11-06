//! A server double takes the client through the login, the character list, the world entry and a
//! log-off, with no network.

use auberean_client_sdk::net::LocalTime;
use auberean_client_sdk::protocol::login::{CharacterList, DataQuery, Empty, PlayerObject};
use auberean_client_sdk::protocol::physics::CreateObject;
use auberean_client_sdk::protocol::quality::PrivateQuality;
use auberean_client_sdk::protocol::text::{NarrowString, WideString};
use auberean_client_sdk::protocol::types::{CharacterIdentity, PhysicsDesc, Placement};
use auberean_client_sdk::protocol::{ClientMessage, ServerMessage};
use auberean_client_sdk::{runtime, Action, Client, ClientData, ClientSettings, Motion};
use auberean_testkit::double::script::{ConnSpec, Event, Msg, Script};
use auberean_testkit::double::ServerDouble;

const PLAYER: u64 = 0x2130_0000_0000_0001;

fn conn(n: usize, login: bool, port: u16, t_request: f64) -> ConnSpec {
    ConnSpec {
        n,
        login,
        server_port: port,
        server_id: if login { 0xF1 } else { 0x0B },
        iteration: if login { 5 } else { 2 },
        net_id: u32::try_from(n + 1).expect("small"),
        server_time: 1.25e8,
        t_request,
        t_connect: t_request + 0.05,
        switched: !login,
    }
}

fn s2c(t: f64, conn: usize, queue: u16, blob_id: u64, m: &ServerMessage) -> Event {
    Event::Msg {
        t,
        msg: Msg {
            conn,
            c2s: false,
            queue,
            blob_id,
            mtype: m.message_type(),
            raw: m.encode(),
        },
    }
}

fn c2s(t: f64, conn: usize, queue: u16, mtype: u32) -> Event {
    Event::Msg {
        t,
        msg: Msg {
            conn,
            c2s: true,
            queue,
            blob_id: 0,
            mtype,
            raw: mtype.to_le_bytes().to_vec(),
        },
    }
}

/// Unordered, out of the world (the login connection's), private and unordered in the world.
const LOGIN: u64 = 0x2300_0000_0000_0000;
const WORLD: u64 = 0x0300_0000_0000_0000;
const PRIVATE: u64 = 0x0100_0000_0000_0000;

fn script() -> Script {
    let list = CharacterList {
        characters: vec![CharacterIdentity {
            id: PLAYER,
            name: WideString("Tester".encode_utf16().collect()),
            seconds_greyed_out: 0,
            visual: Default::default(),
        }],
        deleted: Vec::new(),
        status: 0,
        slots: 6,
        account: NarrowString::from("tester"),
        uses_chat_service: 1,
        has_expansion: 0,
        unknown: 0,
    };
    let mut player = CreateObject {
        id: PLAYER,
        ..CreateObject::default()
    };
    player.physics = PhysicsDesc {
        bits: PhysicsDesc::POSITION,
        instance_stamp: 1,
        visual_ordering_stamp: 8,
        position: Placement {
            cell: 0x0298_0109,
            ..Placement::default()
        },
        ..PhysicsDesc::default()
    };
    let events = vec![
        s2c(
            0.20,
            0,
            5,
            LOGIN,
            &ServerMessage::DataQuery(DataQuery {
                region: 1,
                name_language: 1,
                languages: vec![1],
            }),
        ),
        c2s(0.25, 0, 5, 0x10038),
        s2c(0.30, 0, 5, LOGIN | 1, &ServerMessage::DownloadEnds(Empty)),
        s2c(0.31, 0, 1, LOGIN | 2, &ServerMessage::CharacterList(list)),
        c2s(2.90, 0, 4, 0x1002B),
        Event::Referral {
            t: 2.95,
            from: 0,
            to: Some(1),
        },
        s2c(
            3.10,
            1,
            1,
            WORLD | 3,
            &ServerMessage::PlayerObject(PlayerObject {
                id: PLAYER,
                region: 1,
            }),
        ),
        s2c(3.11, 1, 1, WORLD | 4, &ServerMessage::CreateObject(player)),
        s2c(
            3.12,
            1,
            1,
            PRIVATE | 1 << 32,
            &ServerMessage::UpdateInt(PrivateQuality {
                quality: 280,
                value: 78,
            }),
        ),
        c2s(5.00, 1, 3, 0x100D1),
        s2c(
            5.10,
            1,
            1,
            PRIVATE | 2 << 32,
            &ServerMessage::UpdateInt(PrivateQuality {
                quality: 280,
                value: 79,
            }),
        ),
        c2s(20.0, 0, 4, 0x10027),
        s2c(20.5, 0, 1, WORLD | 5, &ServerMessage::CharExitGame(Empty)),
    ];
    let s = Script {
        meta: Vec::new(),
        conns: vec![conn(0, true, 9000, 0.0), conn(1, false, 9002, 3.0)],
        events,
    };
    s.check().expect("a well-formed script");
    s
}

/// The double plays the servers' side of a game login; the client sends its data versions when
/// asked, enters the world with the character the player picks from the list, follows the
/// referral, holds the player made, reports the scene drawn once the player's object is here,
/// reports its position while the player runs, and leaves the world when it is let go.
///
/// Behaviour: session.double.a-server-double-takes-the-client-into-the-world
#[test]
fn a_server_double_takes_the_client_through_login_character_list_and_world_entry() {
    let mut double = ServerDouble::new(script(), std::net::Ipv4Addr::LOCALHOST, 1.0, 7);
    let settings = ClientSettings {
        login_server: double.login_addr(),
        account: b"tester".to_vec(),
        ticket: b"t".to_vec(),
        timestamp: 0,
        session_key: [0; 64],
        script_sends: false,
    };
    let mut client = Client::new(settings, ClientData::default());
    let me = crate::replay::client_addr();
    let mut now = 0.0;
    let mut entered = false;
    let mut ran = false;
    let mut left = false;
    while now < 30.0 {
        now += 1.0 / 60.0;
        let t = LocalTime(1.0 + now);
        client.frame(t);
        while let Some((to, bytes)) = client.poll_transmit() {
            double.datagram(to, me, &bytes, t);
        }
        for d in double.tick(t) {
            client.handle_datagram(d.from, &d.bytes, t);
        }
        if !entered {
            if let Some(c) = client
                .model
                .characters
                .as_ref()
                .and_then(|l| l.characters.first())
            {
                client.act(Action::PlayCharacter { character: c.id });
                entered = true;
            }
        }
        if !ran && now > 8.0 {
            client.act(Action::Move(Motion {
                forward: 1.0,
                sideways: 0.0,
                heading: 90.0,
                active: true,
            }));
            ran = true;
        }
        if ran && now > 10.0 && client.moving() {
            client.act(Action::Move(Motion {
                forward: 0.0,
                sideways: 0.0,
                heading: 90.0,
                active: false,
            }));
        }
        if !left && now > 19.5 {
            client.act(Action::LogOff);
            left = true;
        }
        if (12.0..12.1).contains(&now) {
            assert_eq!(client.model.player, Some(PLAYER));
            let q = &client
                .model
                .player_object()
                .expect("the player's object")
                .qualities;
            assert_eq!(
                q.ints.get(&280),
                Some(&79),
                "private updates took effect in order"
            );
        }
    }
    let sent: Vec<u32> = client
        .sent()
        .iter()
        .map(|s| s.message.message_type())
        .collect();
    let count = |t: u32| sent.iter().filter(|x| **x == t).count();
    assert_eq!(count(0x10038), 1, "the data versions, when asked");
    assert_eq!(count(0x1002B), 1);
    assert_eq!(count(0x100D1), 1, "the scene drawn, once");
    assert!(
        count(0x1008E) >= 10,
        "position reports while running: {}",
        count(0x1008E)
    );
    let play = client.sent().iter().find_map(|s| match &s.message {
        ClientMessage::PlayCharacter(p) => Some(p.clone()),
        _ => None,
    });
    assert_eq!(
        play.map(|p| (p.character, p.account.0)),
        Some((PLAYER, b"tester".to_vec()))
    );
    let report = double.report();
    assert_eq!(report.connected, vec![0, 1]);
    assert!(report.gates_forced.is_empty() && report.unmatched_gates.is_empty());
    assert!(client.session.login_connected());
    assert_eq!(client.model.player, None, "the character left the world");
    assert_eq!(client.session.connections().count(), 1);
}

/// A client the double took into the world, 12 seconds in, and the time it stands at.
fn in_the_world() -> (ServerDouble, Client, f64) {
    let mut double = ServerDouble::new(script(), std::net::Ipv4Addr::LOCALHOST, 1.0, 7);
    let settings = ClientSettings {
        login_server: double.login_addr(),
        account: b"tester".to_vec(),
        ticket: b"t".to_vec(),
        timestamp: 0,
        session_key: [0; 64],
        script_sends: false,
    };
    let mut client = Client::new(settings, ClientData::default());
    let mut now = 0.0;
    let mut entered = false;
    while now < 12.0 {
        now += 1.0 / 60.0;
        frame(&mut client, &mut double, now);
        if !entered {
            if let Some(c) = client
                .model
                .characters
                .as_ref()
                .and_then(|l| l.characters.first())
            {
                client.act(Action::PlayCharacter { character: c.id });
                entered = true;
            }
        }
    }
    assert!(client.model.player_object().is_some(), "in the world");
    (double, client, now)
}

/// One frame of `client` against `double` at `now` seconds in.
fn frame(client: &mut Client, double: &mut ServerDouble, now: f64) {
    let me = crate::replay::client_addr();
    let t = LocalTime(1.0 + now);
    client.frame(t);
    while let Some((to, bytes)) = client.poll_transmit() {
        double.datagram(to, me, &bytes, t);
    }
    for d in double.tick(t) {
        client.handle_datagram(d.from, &d.bytes, t);
    }
}

/// What the client told the server the player looks at, in order.
fn looks(c: &Client) -> Vec<u64> {
    c.sent()
        .iter()
        .filter_map(|s| match &s.message {
            ClientMessage::LookAt(t) => Some(t.0),
            _ => None,
        })
        .collect()
}

/// A selection made from a panel (the radar, a name, the pack, a fellow) sends no look: the head
/// stays on what the selection keys and the world's clicks last chose.
///
/// Behaviour: session.interface.a-panels-selection-is-not-looked-at
#[test]
fn a_panels_selection_is_not_looked_at() {
    let (_double, mut client, _) = in_the_world();
    client.act(Action::Select { target: 0x40 });
    assert_eq!(looks(&client), [0x40]);
    client.act(Action::SetSelection { target: 0x41 });
    client.act(Action::SetSelection { target: 0 });
    assert_eq!(looks(&client), [0x40], "no look sent");
    assert_eq!(client.looking_at, 0x40, "the head stays");
    assert_eq!(client.selection_looks, [0x40]);
}

/// A selection turns the player to look at what it selected, and the client tells the server so
/// each time; with the option to look at the selection off, it tells the server once that the
/// player looks at nothing, and then nothing more. The world is told each look, to turn the
/// player's head; while no world takes them, the newest of them are kept. (No scripts run here:
/// the selection itself is the player's scripts' to send.)
///
/// Behaviour: session.interface.looking-at-the-selection
#[test]
fn a_selection_is_looked_at_and_with_the_option_off_nothing_is_once() {
    let (_double, mut client, _) = in_the_world();
    client.act(Action::Select { target: 0x40 });
    client.act(Action::Select { target: 0x41 });
    assert_eq!(looks(&client), [0x40, 0x41]);
    client.look_at_selection = false;
    client.act(Action::Select { target: 0x42 });
    client.act(Action::Select { target: 0x43 });
    assert_eq!(looks(&client), [0x40, 0x41, 0]);
    assert_eq!(client.looking_at, 0);
    assert_eq!(
        client.selection_looks,
        [0x40, 0x41, 0],
        "what the player's head is turned to, for the world"
    );
    client.look_at_selection = true;
    let many: Vec<u64> = (0x100..0x100 + 2 * runtime::LOOKS_KEPT as u64).collect();
    for &target in &many {
        client.act(Action::Select { target });
    }
    assert_eq!(
        client.selection_looks,
        many[many.len() - runtime::LOOKS_KEPT..],
        "the newest looks kept for a world that has not taken them"
    );
}

/// The player never looks at itself or at what it holds: selecting either tells the server the
/// player looks at nothing, and the server's saying it looks at one leaves it looking at nothing;
/// with the option to look at the selection off, a player the server set looking at something is
/// told to look at nothing at the next selection.
///
/// Behaviour: session.interface.the-player-never-looks-at-itself-or-what-it-holds
#[test]
fn selecting_itself_or_what_it_holds_looks_at_nothing() {
    use auberean_client_sdk::protocol::physics::{ObjectEvent, Target};
    use auberean_client_sdk::protocol::types::{ObjectStamp, ParentLink};
    const SWORD: u64 = 0x2130_0000_0000_0042;
    let (_double, mut client, _) = in_the_world();
    let mut sword = CreateObject {
        id: SWORD,
        ..CreateObject::default()
    };
    sword.physics = PhysicsDesc {
        bits: PhysicsDesc::PARENT,
        instance_stamp: 1,
        parent: ParentLink {
            parent: PLAYER,
            location: 1,
            stamp: 1,
        },
        ..PhysicsDesc::default()
    };
    client.take_message(WORLD | 100, ServerMessage::CreateObject(sword));
    assert_eq!(client.model.children(PLAYER), [SWORD], "held");
    client.act(Action::Select { target: 0x40 });
    client.act(Action::Select { target: PLAYER });
    client.act(Action::Select { target: SWORD });
    client.act(Action::Select { target: 0x41 });
    assert_eq!(looks(&client), [0x40, 0, 0, 0x41]);
    assert_eq!(client.selection_looks, [0x40, 0, 0, 0x41]);
    let look = |target: u64| {
        ServerMessage::LookAt(ObjectEvent {
            sender: ObjectStamp {
                id: PLAYER,
                instance_stamp: 1,
                other_stamp: 0,
            },
            action: Target(target),
        })
    };
    client.take_message(WORLD | 101, look(SWORD));
    assert_eq!(client.looking_at, 0, "what it holds is nothing to look at");
    client.take_message(WORLD | 102, look(0x44));
    assert_eq!(client.looking_at, 0x44);
    client.look_at_selection = false;
    client.act(Action::Select { target: 0x45 });
    client.act(Action::Select { target: 0x46 });
    assert_eq!(looks(&client), [0x40, 0, 0, 0x41, 0]);
}

/// Where the player's head points, once a world turns it, is sent to the server while the player
/// looks at nothing: at once, then only when it moved more than 0.0002 either way and more than
/// half a second after the last time; nothing while the player looks at an object.
///
/// Behaviour: session.interface.the-players-head-sent-at-most-every-half-second
#[test]
fn where_the_players_head_points_is_sent_at_most_every_half_second() {
    let (mut double, mut client, mut now) = in_the_world();
    let heads = |c: &Client| -> Vec<(f64, f32, f32)> {
        c.sent()
            .iter()
            .filter_map(|s| match &s.message {
                ClientMessage::TurnHead(h) => Some((s.at, h.x, h.y)),
                _ => None,
            })
            .collect()
    };
    assert!(heads(&client).is_empty(), "no world has turned the head");
    let mut run = |client: &mut Client, seconds: f64, head: (f32, f32)| {
        client.head = Some(head);
        let end = now + seconds;
        while now < end {
            now += 1.0 / 60.0;
            frame(client, &mut double, now);
        }
    };
    run(&mut client, 0.1, (-0.25, 0.0));
    run(&mut client, 0.2, (-0.25, 0.0001));
    run(&mut client, 0.2, (-0.25, 0.3));
    run(&mut client, 0.5, (-0.25, 0.3));
    let sent = heads(&client);
    assert_eq!(sent.len(), 2, "{sent:?}");
    assert_eq!((sent[0].1, sent[0].2), (-0.25, 0.0));
    assert_eq!((sent[1].1, sent[1].2), (-0.25, 0.3));
    assert!(sent[1].0 - sent[0].0 > 0.5, "{sent:?}");
    assert!(
        sent[1].0 - sent[0].0 < 0.55,
        "the first frame after: {sent:?}"
    );
    client.act(Action::Select { target: 0x40 });
    run(&mut client, 1.0, (0.1, 0.1));
    assert_eq!(heads(&client).len(), 2, "looking at an object");
    client.looking_at = 0;
    run(&mut client, 0.1, (0.1, 0.1));
    assert_eq!(heads(&client).len(), 3, "looking at nothing again");
}

/// The login of [`script`] with its first entry refused (the character still in the world) and
/// the second let in.
fn refused_script() -> Script {
    use auberean_client_sdk::protocol::login::CharacterRefused;
    let full = script();
    let list = full
        .events
        .iter()
        .find(|e| matches!(e, Event::Msg { msg, .. } if !msg.c2s && msg.mtype == 0x10055))
        .cloned()
        .expect("the list");
    let mut player = CreateObject {
        id: PLAYER,
        ..CreateObject::default()
    };
    player.physics = PhysicsDesc {
        bits: PhysicsDesc::POSITION,
        instance_stamp: 1,
        visual_ordering_stamp: 8,
        position: Placement {
            cell: 0x0298_0109,
            ..Placement::default()
        },
        ..PhysicsDesc::default()
    };
    let events = vec![
        s2c(
            0.20,
            0,
            5,
            LOGIN,
            &ServerMessage::DataQuery(DataQuery {
                region: 1,
                name_language: 1,
                languages: vec![1],
            }),
        ),
        c2s(0.25, 0, 5, 0x10038),
        s2c(0.30, 0, 5, LOGIN | 1, &ServerMessage::DownloadEnds(Empty)),
        list,
        c2s(2.90, 0, 4, 0x1002B),
        s2c(
            3.00,
            0,
            1,
            LOGIN | 3,
            &ServerMessage::CharacterRefused(CharacterRefused {
                reason: CharacterRefused::IN_WORLD,
            }),
        ),
        c2s(3.40, 0, 4, 0x10027),
        c2s(6.00, 0, 4, 0x1002B),
        Event::Referral {
            t: 6.05,
            from: 0,
            to: Some(1),
        },
        s2c(
            6.20,
            1,
            1,
            WORLD | 4,
            &ServerMessage::PlayerObject(PlayerObject {
                id: PLAYER,
                region: 1,
            }),
        ),
        s2c(6.21, 1, 1, WORLD | 5, &ServerMessage::CreateObject(player)),
        c2s(8.00, 1, 3, 0x100D1),
    ];
    let s = Script {
        meta: Vec::new(),
        conns: vec![conn(0, true, 9000, 0.0), conn(1, false, 9002, 6.1)],
        events,
    };
    s.check().expect("a well-formed script");
    s
}

fn tester(double: &ServerDouble) -> Client {
    let settings = ClientSettings {
        login_server: double.login_addr(),
        account: b"tester".to_vec(),
        ticket: b"t".to_vec(),
        timestamp: 0,
        session_key: [0; 64],
        script_sends: false,
    };
    Client::new(settings, ClientData::default())
}

/// One frame: the client's, what it sends to the double, the double's, and what that sends back.
fn exchange(client: &mut Client, double: &mut ServerDouble, t: LocalTime) {
    let me = crate::replay::client_addr();
    client.frame(t);
    while let Some((to, bytes)) = client.poll_transmit() {
        double.datagram(to, me, &bytes, t);
    }
    for d in double.tick(t) {
        client.handle_datagram(d.from, &d.bytes, t);
    }
}

/// An entry the server refuses for the character still being in the world is seen with its
/// reason; the client logs off the character it never had (no character) and stays at the list
/// with its login connection, and a second entry asked a little later goes into the world.
///
/// Behaviour: session.login.a-refused-entry-logs-off-no-character-and-may-ask-again
#[test]
fn a_refused_entry_logs_off_no_character_and_a_second_entry_goes_in() {
    let mut double = ServerDouble::new(refused_script(), std::net::Ipv4Addr::LOCALHOST, 1.0, 7);
    let mut client = tester(&double);
    let mut now = 0.0;
    let mut entries = 0;
    let mut retry_at = None;
    let mut at_refusal = None;
    while now < 12.0 {
        now += 1.0 / 60.0;
        exchange(&mut client, &mut double, LocalTime(1.0 + now));
        let listed = client
            .model
            .characters
            .as_ref()
            .and_then(|l| l.characters.first())
            .map(|c| c.id);
        if client.model.refused.is_some() && retry_at.is_none() {
            retry_at = Some(now + 2.0);
            at_refusal = Some((client.session.connections().count(), client.model.player));
        }
        let due = match (entries, retry_at) {
            (0, _) => true,
            (1, Some(t)) => now >= t,
            _ => false,
        };
        if let (true, Some(id)) = (due, listed) {
            client.act(Action::PlayCharacter { character: id });
            entries += 1;
        }
    }
    assert_eq!(client.model.refused, Some((11, 1)));
    assert_eq!(
        at_refusal,
        Some((1, None)),
        "at the list, on the login connection"
    );
    let sent: Vec<&ClientMessage> = client.sent().iter().map(|s| &s.message).collect();
    let kinds: Vec<u32> = sent
        .iter()
        .map(|m| m.message_type())
        .filter(|t| [0x1002B, 0x10027, 0x100D1].contains(t))
        .collect();
    assert_eq!(kinds, [0x1002B, 0x10027, 0x1002B, 0x100D1]);
    let off = sent.iter().find_map(|m| match m {
        ClientMessage::LogOff(c) => Some(c.character),
        _ => None,
    });
    assert_eq!(off, Some(0), "no character");
    assert_eq!(
        client.model.player,
        Some(PLAYER),
        "the second entry went in"
    );
    let report = double.report();
    assert_eq!(report.connected, vec![0, 1]);
    assert!(report.gates_forced.is_empty() && report.unmatched_gates.is_empty());
}

/// Closing in the world says goodbye on every connection at once, the login's and the world's,
/// with no log-off first; nothing more is sent after it, not even a new login.
///
/// Behaviour: session.login.closing-says-goodbye-on-every-connection
#[test]
fn closing_in_the_world_says_goodbye_on_every_connection() {
    let mut double = ServerDouble::new(script(), std::net::Ipv4Addr::LOCALHOST, 1.0, 7);
    let mut client = tester(&double);
    let me = crate::replay::client_addr();
    let mut now = 0.0;
    let mut entered = false;
    while now < 12.0 {
        now += 1.0 / 60.0;
        exchange(&mut client, &mut double, LocalTime(1.0 + now));
        if !entered {
            if let Some(c) = client
                .model
                .characters
                .as_ref()
                .and_then(|l| l.characters.first())
            {
                client.act(Action::PlayCharacter { character: c.id });
                entered = true;
            }
        }
    }
    assert_eq!(client.model.player, Some(PLAYER), "in the world");
    assert_eq!(client.session.connections().count(), 2);
    let t = LocalTime(1.0 + now);
    client.disconnect();
    let mut goodbyes = 0;
    while let Some((to, bytes)) = client.poll_transmit() {
        double.datagram(to, me, &bytes, t);
        goodbyes += 1;
    }
    assert_eq!(goodbyes, 2, "one datagram a connection");
    assert_eq!(double.report().client_goodbyes, vec![true, true]);
    assert!(!client
        .sent()
        .iter()
        .any(|s| s.message.message_type() == 0x10027));
    assert_eq!(client.session.connections().count(), 0);
    for i in 1..=600 {
        client.frame(LocalTime(1.0 + now + f64::from(i) / 60.0));
        assert!(client.poll_transmit().is_none(), "nothing more");
    }
}
