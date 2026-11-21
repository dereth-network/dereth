//! Chat regions: a player outdoors has its region's channel in its channel list, sent again when it
//! walks into another region or lands from portal space in another region (or indoors); a line in a
//! region's room reaches the players there. Fixture: the test world, with a few landblocks laid
//! out in regions here.

use auberean_primitives::Vec3;
use auberean_protocol::login::ChatPacket;
use auberean_protocol::physics::ClientPosition;
use auberean_protocol::{queue, ClientMessage, ServerMessage};
use dereth_protocol::turbine::{decode_incoming, ExtraBlob, IncomingPayload, SendToRoomById};
use yalain_store::StoredPosition;
use yalain_testkit::Harness;
use yalain_world::calls::{self, Node, Value};
use yalain_world::chat::{self, ChatRegions};

/// The landblocks laid out in regions here: one in region 5, the two east of it in region 6, one
/// far off in region 7. The test world's start is indoors.
const HOME: u32 = 0x75B9;
const EAST: u32 = 0x76B9;
const FURTHER: u32 = 0x77B9;
const FAR: u32 = 0x95D9;

fn regions() -> ChatRegions {
    let mut cells = vec![0u8; 256 * 256];
    for (block, region) in [(HOME, 5), (EAST, 6), (FURTHER, 6), (FAR, 7)] {
        cells[block as usize] = region;
    }
    ChatRegions::new(cells).expect("256 by 256")
}

/// A harness with the regions above and one player in the world, walked out from the start to
/// the region 5 landblock. Returns the harness and the client and player.
fn in_regions() -> (Harness, usize, u64) {
    let mut h = Harness::new();
    h.server.chat_regions = Some(regions());
    let (b, me) = h.player_in_world("one", "Elara");
    h.run(10);
    walk(&mut h, b, me, HOME << 16 | 0x0001);
    (h, b, me)
}

/// The player reports standing in a cell, as its own keys take it there.
fn walk(h: &mut Harness, b: usize, me: u64, cell: u32) {
    let m = report_at(h, me, cell);
    h.bots[b].send(true, queue::WEENIE, &m);
    h.run(4);
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

/// The region channel's name a list holds, if any.
fn region_in(list: &[String]) -> Option<String> {
    list.iter()
        .find_map(|n| n.split_once(" Region ").map(|(_, r)| format!("Region {r}")))
}

fn world_of(h: &Harness, me: u64) -> yalain_net::ConnId {
    yalain_world::social::world_of(&h.server, me).expect("in the world")
}

/// The player's report of standing in a cell.
fn report_at(h: &Harness, me: u64, cell: u32) -> ClientMessage {
    let p = &h.server.players[&world_of(h, me)];
    ClientMessage::Position(ClientPosition {
        time: 0.0,
        cell,
        origin: Vec3::new(10.0, 10.0, p.position.origin[2]),
        heading_motion: yalain_world::world::IDLE_MOTION,
        flags: yalain_world::places::MOVED_BY_HAND,
        instance_stamp: 1,
        teleport_stamp: p.teleport_stamp,
        force_position_stamp: p.force_position_stamp,
        move_to_stamp: p.move_to_stamp,
        jump_velocity: Vec3::ZERO,
    })
}

/// A player indoors has no region's channel (its two lists at entry, at the test world's indoor
/// start, are the four alone); walked outdoors into a region, its list names the region's channel
/// too. The map gives no region indoors, nor where it names none.
/// Behaviour: server.chat.region-channel-outdoors
#[test]
fn a_player_outdoors_has_its_regions_channel_and_none_indoors() {
    let r = regions();
    assert_eq!(r.of(HOME << 16 | 0x0001), 5);
    assert_eq!(r.of(HOME << 16 | 0x0100), 0, "indoors");
    assert_eq!(r.of(0x1234_0001), 0, "no region there");
    let mut h = Harness::new();
    h.server.chat_regions = Some(r);
    let (b, me) = h.player_in_world("one", "Elara");
    h.run(10);
    let entry = lists(&h, b, 0);
    assert_eq!(entry.len(), 2, "twice at entry: {entry:?}");
    assert!(
        entry.iter().all(|l| l.len() == 4 && region_in(l).is_none()),
        "{entry:?}"
    );
    let from = h.bots[b].inbox.len();
    walk(&mut h, b, me, HOME << 16 | 0x0001);
    let out = lists(&h, b, from);
    assert_eq!(out.len(), 1, "{out:?}");
    assert_eq!(out[0].len(), 5, "the four, and the region's: {:?}", out[0]);
    assert_eq!(region_in(&out[0]).as_deref(), Some("Region 5"));
}

/// A player walking into another region's landblock is sent its channels again, once, naming the
/// new region; walking on within that region, or into a building, sends nothing.
/// Behaviour: server.chat.region-channel-on-walking-across
#[test]
fn a_player_walking_into_another_region_is_sent_its_channels_again_once() {
    let (mut h, b, me) = in_regions();
    let from = h.bots[b].inbox.len();
    walk(&mut h, b, me, EAST << 16 | 0x0001);
    let got = lists(&h, b, from);
    assert_eq!(got.len(), 1, "{got:?}");
    assert_eq!(region_in(&got[0]).as_deref(), Some("Region 6"));
    let on = h.bots[b].inbox.len();
    walk(&mut h, b, me, FURTHER << 16 | 0x0001);
    walk(&mut h, b, me, FURTHER << 16 | 0x0100);
    assert!(
        lists(&h, b, on).is_empty(),
        "the same region, then a building in it"
    );
}

/// A player landing from portal space in another region is sent its channels as it comes out,
/// naming the new region; landing indoors, a list with no region's; landing in the region its list
/// names, nothing.
/// Behaviour: server.chat.region-channel-at-a-landing
#[test]
fn a_player_landing_in_another_region_is_sent_its_channels_as_it_comes_out() {
    let (mut h, b, me) = in_regions();
    let w = world_of(&h, me);
    let go = |h: &mut Harness, cell: u32| {
        let from = h.bots[b].inbox.len();
        let to = StoredPosition {
            cell,
            origin: [20.0, 20.0, h.server.players[&w].position.origin[2]],
            rotation: [1.0, 0.0, 0.0, 0.0],
        };
        let clock = h.clock();
        yalain_world::places::sent_away(&mut h.server, &mut h.net, w, to, clock);
        h.advance(yalain_world::places::IN_PORTAL_SPACE + 1.0);
        h.run(10);
        lists(h, b, from)
    };
    let got = go(&mut h, FAR << 16 | 0x0001);
    assert_eq!(got.len(), 1, "{got:?}");
    assert_eq!(region_in(&got[0]).as_deref(), Some("Region 7"));
    assert!(go(&mut h, FAR << 16 | 0x0002).is_empty(), "the same region");
    let indoors = go(&mut h, FAR << 16 | 0x0100);
    assert_eq!(indoors.len(), 1, "{indoors:?}");
    assert_eq!(region_in(&indoors[0]), None, "indoors, no region's channel");
}

/// A client's service packets since `from`.
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
        text: "anyone here?".into(),
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

/// A line in a region's room reaches the players whose channels name that region and no others;
/// from a player of another region it is refused.
/// Behaviour: server.chat.region-room-reaches-its-region
#[test]
fn a_line_in_a_regions_room_reaches_the_players_in_that_region() {
    let (mut h, a, _) = in_regions();
    let (b, bob) = h.player_in_world("two", "Babyfare");
    let (c, carol) = h.player_in_world("three", "Carol");
    h.run(10);
    walk(&mut h, b, bob, HOME << 16 | 0x0001);
    walk(&mut h, c, carol, EAST << 16 | 0x0001);
    let room = chat::REGION_ROOMS_AFTER + 5;
    let (fb, fc) = (h.bots[b].inbox.len(), h.bots[c].inbox.len());
    say_in(&mut h, a, room, 7);
    assert!(
        matches!(&service(&h, b, fb)[..], [IncomingPayload::RoomEvent(e)] if e.room == room && e.name == "Elara"),
        "heard in region 5"
    );
    assert!(service(&h, c, fc).is_empty(), "not heard in region 6");
    let fc = h.bots[c].inbox.len();
    say_in(&mut h, c, room, 8);
    assert!(
        matches!(&service(&h, c, fc)[..], [IncomingPayload::RoomResponse(r)] if r.result == chat::REFUSED),
        "refused from region 6"
    );
}
