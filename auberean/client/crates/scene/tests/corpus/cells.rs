//! The world's cells about the player, asked of the data cache as the player moves, in recorded
//! 2017 sessions: what the client asks the server for, and a record the server sends made into
//! the land and drawn.
//! Fixture: the end-of-retail install and the capture index.

use std::collections::BTreeMap;

use auberean_client_sdk::dat::QualifiedId;
use auberean_client_sdk::protocol::ClientMessage;
use auberean_draw::quality::{key, Graphics};
use auberean_testkit::captures;

use crate::combat::{own_world, played_in_frames, played_with, FRAME};

/// The client's data requests: the record each asked for, by type and id.
const DATA_REQUEST: u32 = 0x10034;
/// The player's object, the first message of a world entry.
const PLAYER_OBJECT: u32 = 0x10060;
/// A landblock's type.
const LANDBLOCK: u32 = 0x4000_0050;

/// The records the recorded client asked its server for in the first `seconds` after it entered
/// the world in the capture of `session`, counted by type and id.
fn retail_asks(session: &str, seconds: f64) -> BTreeMap<(u32, u32), usize> {
    let index = auberean_testkit::data::require_capture_index();
    let mut entered = None;
    let mut asked = BTreeMap::new();
    captures::capture_messages_in_time_order(
        &index,
        &[DATA_REQUEST, PLAYER_OBJECT],
        session,
        |m| {
            if m.message_type == PLAYER_OBJECT {
                entered.get_or_insert(m.time);
                return;
            }
            let Some(e) = entered else { return };
            if m.time > e + seconds || m.to_client {
                return;
            }
            if let Ok(ClientMessage::DataRequest(r)) = ClientMessage::decode(&m.raw) {
                *asked.entry((r.kind, r.id)).or_default() += 1;
            }
        },
    )
    .expect("the index reads");
    asked
}

/// What this client asked the server for, counted by type and id.
fn our_asks(c: &auberean_client_sdk::Client) -> BTreeMap<(u32, u32), usize> {
    let mut asked = BTreeMap::new();
    for s in c.sent() {
        if let ClientMessage::DataRequest(r) = &s.message {
            *asked.entry((r.kind, r.id)).or_default() += 1;
        }
    }
    asked
}

/// Every one of the end-of-retail files' light records the 2017 clients asked for is missing
/// from them, and every ask was refused: their asks for them came from the cells' asks as the
/// players moved. Played through this client over the same recorded minutes, the cells' asks
/// ask the server for the same light records, as often: in a session that went back and forth
/// past two blocks whose infos name missing light records, 7 times each in 20 minutes, as the
/// 2017 client did; in one moving among five such blocks, the same count for each in half an
/// hour but one ask of one of them, the last, made as the half hour ended.
///
/// Behaviour: desktop.world.the-cells-asks-are-the-2017-clients
#[test]
fn the_cells_asks_for_records_the_files_lack_are_the_2017_clients() {
    for (session, seconds, slack) in [
        ("c86520bced727bf4:1", 1200.0, 0),
        ("9e1bfd4a21feab58:1", 1800.0, 1),
    ] {
        let (c, _scene, _) = played_in_frames(session, seconds, FRAME, &[], &mut |_, _| {});
        let ours = our_asks(&c);
        let theirs = retail_asks(session, seconds);
        eprintln!("{session}: this client {ours:08X?}, the 2017 client {theirs:08X?}");
        assert_eq!(
            ours.keys().collect::<Vec<_>>(),
            theirs.keys().collect::<Vec<_>>(),
            "{session}: the same records"
        );
        let off: usize = ours.iter().map(|(k, n)| n.abs_diff(theirs[k])).sum();
        assert!(off <= slack, "{session}: {off} asks off");
        assert!(theirs.values().sum::<usize>() >= 14);
    }
}

/// A landblock the client's files lack is asked of the server as the cells' rings reach it, and
/// the one it sends is stored, read by the world as the files' own, and made into the land: in a
/// session whose player went out to blocks its files lacked (the 2017 servers sent twelve
/// landblocks in its first 34 minutes, the player's draw distance at its farthest), with those
/// landblocks taken out of the files, this client asks for the same twelve; the world could not
/// read them before, reads them all once they are sent, and the two under the player at the end
/// stand in its land, with the frame drawn over them.
///
/// Behaviour: desktop.world.a-landblock-the-server-sends-is-made-into-the-land
#[test]
fn a_landblock_the_server_sends_is_made_into_the_land_the_player_stands_on() {
    let session = "ea8286451208d699:1";
    let seconds = 2030.0;
    let theirs = retail_asks(session, seconds);
    let landblocks: Vec<QualifiedId> = theirs
        .keys()
        .filter(|(k, _)| *k == LANDBLOCK)
        .map(|&(kind, id)| QualifiedId { id, kind })
        .collect();
    assert_eq!(landblocks.len(), 12, "{theirs:08X?}");
    let mut farthest = Graphics::first_run();
    assert!(farthest.set(key::LANDSCAPE_DRAW_DISTANCE, 4.0));
    // A world of its own: the one the other replays share reads through their caches.
    let world = own_world();
    let records = &world.records;
    let read = |q: &QualifiedId| {
        let (x, y) = (q.id >> 24, (q.id >> 16) & 0xFF);
        records
            .landblock(i32::try_from(x).unwrap(), i32::try_from(y).unwrap())
            .is_some()
    };
    let mut unread_before = None;
    let (c, mut scene, _) = played_with(
        world,
        session,
        seconds,
        FRAME,
        &[],
        &farthest,
        &mut |c| {
            for q in &landblocks {
                assert!(
                    c.data_download().cache().stores().purge_data(*q),
                    "{q:08X?}"
                );
            }
        },
        &mut |c, _| {
            if unread_before.is_none() && c.model.player_object().is_some() {
                unread_before = Some(landblocks.iter().filter(|q| !read(q)).count());
            }
        },
    );
    assert_eq!(unread_before, Some(12), "the world could not read them");
    let ours = our_asks(&c);
    let asked: Vec<QualifiedId> = ours
        .keys()
        .filter(|(k, _)| *k == LANDBLOCK)
        .map(|&(kind, id)| QualifiedId { id, kind })
        .collect();
    assert_eq!(asked, landblocks, "the same landblocks asked for");
    assert!(landblocks.iter().all(read), "all read once sent");
    assert_eq!(
        scene.status.cell >> 16,
        0xA394,
        "where the player is at the end"
    );
    for id in [0xA294_FFFF_u32, 0xA394_FFFF] {
        let (x, y) = (
            u8::try_from(id >> 24).unwrap(),
            u8::try_from((id >> 16) & 0xFF).unwrap(),
        );
        assert!(landblocks.iter().any(|q| q.id == id));
        assert!(scene.land().block(x, y).is_some(), "{id:08X} in the land");
    }
    let frame = scene.frame(2.0).expect("the world's frame");
    assert!(!frame.terrain.is_empty(), "the land drawn");
}
