//! The host installed as the client's frame hook, against a server double on a fixed step and a
//! network in memory: a plugin enters the world with the first character listed, hears it enter,
//! and reads the player from the view.

use std::collections::VecDeque;
use std::net::{Ipv4Addr, SocketAddr};

use auberean_client_net::LocalTime;
use auberean_client_runtime::{Client, ClientData, ClientSettings};
use auberean_plugin_host::PluginHook;
use auberean_protocol::login::{CharacterList, DataQuery, Empty, PlayerObject};
use auberean_protocol::physics::CreateObject;
use auberean_protocol::text::{NarrowString, WideString};
use auberean_protocol::types::{CharacterIdentity, PhysicsDesc, Placement, VisualDesc};
use auberean_protocol::ServerMessage;
use auberean_testkit::double::script::{ConnSpec, Event, Msg, Script};
use auberean_testkit::double::ServerDouble;

use crate::common::{host, package};

const PLAYER: u64 = 0x2130_0000_0000_0001;
/// The client's entry into the world, by its message type.
const ENTER_WORLD: u32 = 0x1002B;
/// A minute of frames, a sixtieth of a second each.
const FRAMES: u32 = 3600;

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

const LOGIN: u64 = 0x2300_0000_0000_0000;
const WORLD: u64 = 0x0300_0000_0000_0000;

/// A game login: the data versions asked, the character list, the entry, the referral to the
/// world server and the player made there.
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
        c2s(2.90, 0, 4, ENTER_WORLD),
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
    ];
    let s = Script {
        meta: Vec::new(),
        conns: vec![conn(0, true, 9000, 0.0), conn(1, false, 9002, 3.0)],
        events,
    };
    s.check().expect("a well-formed script");
    s
}

/// A plugin asks to enter the world with the first character the server lists; the client sends
/// the entry once, the plugin's promise completes once the character is in the world, and it hears
/// the entry with the character's id; a moment later it reads the player, by the same id, from
/// the view the host refreshes each frame.
///
/// Behaviour: plugins.host.a-plugin-enters-the-world-with-a-listed-character
#[test]
fn a_plugin_enters_the_world_with_the_first_character_listed_and_hears_it_enter() {
    let mut double = ServerDouble::new(script(), Ipv4Addr::LOCALHOST, 1.0, 7);
    let settings = ClientSettings {
        login_server: double.login_addr(),
        account: b"tester".to_vec(),
        ticket: b"t".to_vec(),
        timestamp: 0,
        session_key: [0; 64],
        script_sends: false,
    };
    let me = SocketAddr::from(([127, 0, 0, 1], 50_000));
    let mut client = Client::new(settings, ClientData::default());
    let mut plugins = host();
    plugins.request_load(package(
        "test.autologin",
        "auberean",
        "contexts = [\"pregame\", \"world\"]\n",
        r#"
        on("session.pregame", function(e)
            session.select_character(e.characters[1].id):next(function() print("in the world") end)
        end)
        on("session.enter_world", function(e)
            print("entered as " .. e.id)
            task.after(200, function() print("the player is " .. game.player().id) end)
        end)
        "#,
    ));
    client.install_frame_hook(Box::new(PluginHook(plugins)));
    let mut lines = Vec::new();
    let mut inbox = VecDeque::new();
    for n in 0..FRAMES {
        let now = LocalTime(1.0 + f64::from(n) / 60.0);
        inbox.extend(double.tick(now).into_iter().map(|d| (d.from, d.bytes)));
        while let Some((from, bytes)) = inbox.pop_front() {
            client.handle_datagram(from, &bytes, now);
        }
        client.frame(now);
        client.refresh_view(&|_: &VisualDesc| Vec::new());
        while let Some((to, bytes)) = client.poll_transmit() {
            double.datagram(to, me, &bytes, now);
        }
        let hook = client
            .frame_hook_mut()
            .and_then(|h| h.as_any_mut().downcast_mut::<PluginHook>())
            .expect("the host installed");
        lines.extend(
            hook.host_mut()
                .take_printed()
                .into_iter()
                .filter(|l| l.from.is_some())
                .map(|l| l.text),
        );
        if lines.len() >= 3 {
            break;
        }
    }
    assert_eq!(
        lines,
        [
            "entered as 0x2130000000000001",
            "in the world",
            "the player is 0x2130000000000001"
        ]
    );
    let entries = client
        .sent()
        .iter()
        .filter(|s| s.message.message_type() == ENTER_WORLD)
        .count();
    assert_eq!(entries, 1, "the entry, once");
}
