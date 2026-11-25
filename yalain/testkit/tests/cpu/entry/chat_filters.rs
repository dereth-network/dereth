//! Chat filters: a player's channel list holds only the kinds of channel one of its chat windows
//! shows, chat options that change that send the list again, and a channel's room hidden in every
//! window is neither heard nor spoken in.

use auberean_protocol::login::ChatPacket;
use auberean_protocol::{queue, ClientMessage, ServerMessage};
use auberean_script::args::Arg;
use auberean_script::library::Elem;
use auberean_script::native_object::Item;
use dereth_protocol::turbine::{decode_incoming, ExtraBlob, IncomingPayload, SendToRoomById};
use yalain_testkit::play::server_call;
use yalain_testkit::Harness;
use yalain_world::calls::{self, Node, Value};
use yalain_world::chat;
use yalain_world::graph::{hash, Builder};
use yalain_world::options::Pairs;
use yalain_world::session::request;

/// The kinds of the channels a new character has: PvP's, General's, Trade's, a kingdom's.
const PVP: u32 = 1 << 24;
const GENERAL: u32 = 1 << 19;
const TRADE: u32 = 1 << 20;
const FACTION: u32 = 1 << 22;

/// Chat filters of four windows that show every channel kind but those in `hidden`, as a player's
/// windows hold them: the first window most kinds, the others a few each.
fn filters(hidden: u32) -> Vec<(u32, u32)> {
    vec![
        (1, 0x0003_018D | (TRADE & !hidden)),
        (2, (PVP | GENERAL | 0x50) & !hidden),
        (3, (FACTION | 0x3_0020) & !hidden),
        (4, 2),
    ]
}

/// A word-to-word table as the client packs one into a call.
fn table(pairs: &[(u32, u32)]) -> Arg {
    let entries: Vec<(Item, Item)> = pairs
        .iter()
        .map(|(k, v)| (Item::Word(*k), Item::Word(*v)))
        .collect();
    let mut g = Builder::new();
    g.native(1, hash(Elem::Word, Elem::Word, 2, &entries));
    Arg::Object(g.graph())
}

/// The client sends its chat options with these filters (the other tables as a client leaves them).
fn upload(h: &mut Harness, b: usize, filters: &[(u32, u32)]) {
    let m = server_call(
        request::CHAT_OPTIONS,
        &[
            table(&[(1, 0)]),
            table(&[(1, 16)]),
            table(&[(1, 0), (2, 1)]),
            table(&[(1, 2), (2, 2)]),
            table(filters),
        ],
    );
    h.bots[b].send(true, queue::WEENIE, &m);
    h.run(10);
}

/// The channel names of each channel list a client was sent since `from`, in order.
fn lists(h: &Harness, b: usize, from: usize) -> Vec<Vec<String>> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c) if c.function.0 == chat::function::CHANNELS => {
                let v = c.args.as_ref().and_then(|a| calls::values(&a.0).ok())?;
                let mut names: Vec<String> = v
                    .iter()
                    .filter_map(|x| match x {
                        Value::Graph(g) => Some(g.iter().filter_map(|(_, n)| match n {
                            Node::Text(t) => Some(t.clone()),
                            _ => None,
                        })),
                        _ => None,
                    })
                    .flatten()
                    .collect();
                names.sort();
                Some(names)
            }
            _ => None,
        })
        .collect()
}

/// A list's channel names after the world's name.
fn names<'a>(h: &Harness, list: &'a [String]) -> Vec<&'a str> {
    let world = format!("{} ", h.server.cfg.world_name);
    list.iter()
        .map(|n| n.strip_prefix(world.as_str()).unwrap_or(n))
        .collect()
}

fn world_of(h: &Harness, me: u64) -> yalain_net::ConnId {
    yalain_world::social::world_of(&h.server, me).expect("in the world")
}

/// A client's chat-service packets since `from`.
fn service(h: &Harness, b: usize, from: usize) -> Vec<IncomingPayload> {
    h.bots[b].inbox[from..]
        .iter()
        .filter(|r| !r.world)
        .filter_map(|r| match r.message() {
            Some(ServerMessage::Chat(ChatPacket(p))) => decode_incoming(
                &[
                    &u32::try_from(p.len()).expect("small").to_le_bytes()[..],
                    &p,
                ]
                .concat(),
            )
            .ok()
            .map(|i| i.payload),
            _ => None,
        })
        .collect()
}

fn say_in(h: &mut Harness, b: usize, room: u32, context: u32) {
    let packet = SendToRoomById {
        context,
        room,
        text: "selling a shield".into(),
        extra: ExtraBlob::Speaker {
            sender: 1,
            chat_type: 100 << 16 | 1,
        },
    }
    .network_packet()
    .expect("packs");
    h.bots[b].send(false, 4, &ClientMessage::Chat(ChatPacket(packet)));
    h.run(30);
}

/// A player whose chat windows all hide Trade has no Trade channel in its list; hiding PvP leaves
/// PvP out the same way; with no filters kept, the list holds every channel.
/// Behaviour: server.chat.filter-leaves-out-hidden-channels
#[test]
fn a_channel_no_chat_window_shows_is_left_out_of_the_list() {
    let mut h = Harness::new();
    let (b, me) = h.player_in_world("one", "Elara");
    h.run(10);
    let w = world_of(&h, me);
    let sent = |h: &mut Harness, kept: Option<Vec<(u32, u32)>>| {
        h.server
            .players
            .get_mut(&w)
            .expect("in")
            .options
            .chat_filters = kept.map(|pairs| Pairs {
            size_index: 2,
            pairs,
        });
        let from = h.bots[b].inbox.len();
        chat::send_channels(&mut h.server, &mut h.net, w, 1);
        h.run(4);
        let got = lists(h, b, from);
        assert_eq!(got.len(), 1, "{got:?}");
        got[0].clone()
    };
    let all = sent(&mut h, None);
    let shown = sent(&mut h, Some(filters(0)));
    let no_trade = sent(&mut h, Some(filters(TRADE)));
    let no_pvp = sent(&mut h, Some(filters(PVP)));
    assert_eq!(
        names(&h, &all),
        ["Faction Neutral ", "General", "PvP", "Trade 1"],
        "no filters kept: every channel"
    );
    assert_eq!(names(&h, &shown), names(&h, &all));
    assert_eq!(
        names(&h, &no_trade),
        ["Faction Neutral ", "General", "PvP"],
        "Trade hidden in every window"
    );
    assert_eq!(
        names(&h, &no_pvp),
        ["Faction Neutral ", "General", "Trade 1"],
        "PvP hidden in every window"
    );
}

/// Chat options that hide Trade in every window send the list again at once, without Trade; options
/// that show it again send it with Trade; options that change no channel's showing (another kind
/// of text in a window) send nothing.
/// Behaviour: server.chat.filter-change-resends-channels
#[test]
fn chat_options_that_hide_a_listed_channel_send_the_list_again_and_others_send_nothing() {
    let mut h = Harness::new();
    let (b, _) = h.player_in_world("one", "Elara");
    h.run(10);
    let from = h.bots[b].inbox.len();
    upload(&mut h, b, &filters(0));
    assert!(
        lists(&h, b, from).is_empty(),
        "every channel shown, as before"
    );
    let from = h.bots[b].inbox.len();
    upload(&mut h, b, &filters(TRADE));
    let got = lists(&h, b, from);
    assert_eq!(got.len(), 1, "{got:?}");
    assert!(!names(&h, &got[0]).contains(&"Trade 1"), "{got:?}");
    let from = h.bots[b].inbox.len();
    let mut other = filters(TRADE);
    other[3].1 |= 0x8;
    upload(&mut h, b, &other);
    assert!(
        lists(&h, b, from).is_empty(),
        "no channel's showing changed"
    );
    let from = h.bots[b].inbox.len();
    upload(&mut h, b, &filters(0));
    let got = lists(&h, b, from);
    assert_eq!(got.len(), 1, "{got:?}");
    assert!(names(&h, &got[0]).contains(&"Trade 1"), "{got:?}");
}

/// A line in Trade's room reaches the players whose windows show Trade and not one whose windows
/// all hide it; from that player a line to Trade's room is refused and heard by nobody, while its
/// lines to General go as ever.
/// Behaviour: server.chat.filter-hidden-room-unheard
#[test]
fn a_channels_room_is_unheard_by_and_refused_from_a_player_whose_windows_hide_it() {
    let mut h = Harness::new();
    let (a, _) = h.player_in_world("one", "Elara");
    let (b, _) = h.player_in_world("two", "Babyfare");
    let (c, _) = h.player_in_world("three", "Carol");
    h.run(10);
    upload(&mut h, b, &filters(TRADE));
    let (fb, fc) = (h.bots[b].inbox.len(), h.bots[c].inbox.len());
    say_in(&mut h, a, chat::TRADE_ROOM, 7);
    assert!(service(&h, b, fb).is_empty(), "Trade hidden: not heard");
    assert!(
        matches!(&service(&h, c, fc)[..], [IncomingPayload::RoomEvent(e)] if e.room == chat::TRADE_ROOM && e.name == "Elara"),
        "Trade shown: heard"
    );
    let (fa, fb, fc) = (
        h.bots[a].inbox.len(),
        h.bots[b].inbox.len(),
        h.bots[c].inbox.len(),
    );
    say_in(&mut h, b, chat::TRADE_ROOM, 8);
    assert!(
        matches!(&service(&h, b, fb)[..], [IncomingPayload::RoomResponse(r)] if r.context == 8 && r.result == chat::REFUSED),
        "refused, and no echo: {:?}",
        service(&h, b, fb)
    );
    assert!(service(&h, a, fa).is_empty() && service(&h, c, fc).is_empty());
    let fa = h.bots[a].inbox.len();
    say_in(&mut h, b, chat::GENERAL_ROOM, 9);
    assert!(
        matches!(&service(&h, a, fa)[..], [IncomingPayload::RoomEvent(e)] if e.room == chat::GENERAL_ROOM && e.name == "Babyfare"),
        "General shown: its lines go"
    );
}
