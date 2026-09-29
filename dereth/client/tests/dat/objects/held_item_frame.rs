//! Where a held item is drawn: the holder's part frame composed with the setup's holding location,
//! checked against independent quaternion arithmetic, and every recorded parent event reaching the
//! object model.
//!
//! Fixture: the retail human setup `0x02000001` and its RightHand location, and the recorded
//! capture corpus replayed through `ClientNetwork` and `ObjectStream`.

use crate::common::workspace_root_buf as repo_root;

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use dereth_animation::data::LocationEntry;
use dereth_client::models::child_frame;
use dereth_client::net::ClientNetwork;
use dereth_client::objects::ObjectStream;
use dereth_client_net::client_session::SessionEvent;
use dereth_primitives::num::math;
use dereth_primitives::{Frame, LocalTime, Quat, Vec3};

use dereth_client_net::client_session::testing::capture::{self, Datagram as Record};

/// Every recording in the flat `fixtures/packet-captures/` corpus, sorted.
fn sessions() -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(repo_root().join("fixtures/packet-captures")) else {
        return Vec::new();
    };
    let mut paths: Vec<_> = entries
        .filter_map(Result::ok)
        .filter(|e| !crate::common::is_unclean_logout_recording(&e.path()))
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "jsonl"))
        .collect();
    paths.sort();
    paths
}

fn load(path: &Path) -> Vec<Record> {
    capture::load(path).unwrap_or_default()
}

fn connection_sequence_number(records: &[Record]) -> u32 {
    dereth_client_net::recording::connection_sequence_number(records).unwrap_or(0)
}

fn addr(pair: u16) -> SocketAddr {
    capture::peer(pair)
}

fn store() -> dereth_dat::RetailDatStore {
    dereth_dat::testing::open_store()
        .expect("the retail dats are this file's oracle: set DERETH_TEST_DAT_DIR")
}

fn setup_of(store: &dereth_dat::RetailDatStore, id: u32) -> Option<dereth_assets::Setup> {
    use dereth_assets::Decode;
    let id = dereth_primitives::DataId(id);
    let bytes = store.read_typed(dereth_dat::DbType::Setup, id).ok()?;
    dereth_assets::Setup::decode_payload(id, &bytes).ok()
}

const HUMAN_SETUP: u32 = 0x0200_0001;
const RIGHT_HAND: u32 = 1;

fn quat_about_z(degrees: f64) -> Quat {
    let half = degrees.to_radians() / 2.0;
    #[allow(clippy::cast_possible_truncation)]
    Quat::new(math::cos(half) as f32, 0.0, 0.0, math::sin(half) as f32)
}

fn rotate(q: Quat, v: Vec3) -> (f64, f64, f64) {
    let (w, x, y, z) = (
        f64::from(q.w),
        f64::from(q.x),
        f64::from(q.y),
        f64::from(q.z),
    );
    let (vx, vy, vz) = (f64::from(v.x), f64::from(v.y), f64::from(v.z));
    let (tx, ty, tz) = (
        2.0 * (y * vz - z * vy),
        2.0 * (z * vx - x * vz),
        2.0 * (x * vy - y * vx),
    );
    (
        vx + w * tx + (y * tz - z * ty),
        vy + w * ty + (z * tx - x * tz),
        vz + w * tz + (x * ty - y * tx),
    )
}

fn dist(a: Vec3, b: Vec3) -> f64 {
    let (dx, dy, dz) = (
        f64::from(a.x) - f64::from(b.x),
        f64::from(a.y) - f64::from(b.y),
        f64::from(a.z) - f64::from(b.z),
    );
    dx.mul_add(dx, dy.mul_add(dy, dz * dz)).sqrt()
}

/// Behaviour: objects.held-frame.is-the-holders-part-frame-composed-with-the-holding-location
/// The held frame is checked against independent quaternion arithmetic and retail RightHand data.
#[test]
fn the_held_frame_is_the_holders_part_frame_composed_with_the_holding_location() {
    let store = store();
    let setup = setup_of(&store, HUMAN_SETUP).expect("the human setup decodes");
    let dat = setup.holding_locations[&RIGHT_HAND];
    let holding = LocationEntry {
        part_id: dat.part_id,
        frame: dat.frame,
    };
    assert_eq!(
        holding.part_id, 15,
        "RightHand is part 15 on the human setup"
    );

    let root = Frame::new(Vec3::new(84.5, 17.25, 3.75), quat_about_z(37.0));
    let hand = Frame::new(Vec3::new(84.8, 17.05, 4.95), quat_about_z(-112.0));
    let mut parts = vec![Frame::new(Vec3::ZERO, Quat::IDENTITY); 34];
    parts[15] = hand;
    let got = child_frame(&root, &parts, &holding);

    let rotated = rotate(hand.rotation, holding.frame.origin);
    let want = (
        f64::from(hand.origin.x) + rotated.0,
        f64::from(hand.origin.y) + rotated.1,
        f64::from(hand.origin.z) + rotated.2,
    );
    for (mine, oracle, axis) in [
        (f64::from(got.origin.x), want.0, "x"),
        (f64::from(got.origin.y), want.1, "y"),
        (f64::from(got.origin.z), want.2, "z"),
    ] {
        assert!((mine - oracle).abs() < 1e-5, "{axis}: {mine} vs {oracle}");
    }

    let (parent, offset) = (hand.rotation, holding.frame.rotation);
    let expected = [
        parent.w * offset.w - parent.x * offset.x - parent.y * offset.y - parent.z * offset.z,
        parent.w * offset.x + parent.x * offset.w + parent.y * offset.z - parent.z * offset.y,
        parent.w * offset.y + parent.y * offset.w + parent.z * offset.x - parent.x * offset.z,
        parent.w * offset.z + parent.z * offset.w + parent.x * offset.y - parent.y * offset.x,
    ];
    let norm = expected
        .iter()
        .map(|value| value * value)
        .sum::<f32>()
        .sqrt();
    for (mine, oracle, axis) in [
        (got.rotation.w, expected[0] / norm, "w"),
        (got.rotation.x, expected[1] / norm, "x"),
        (got.rotation.y, expected[2] / norm, "y"),
        (got.rotation.z, expected[3] / norm, "z"),
    ] {
        assert!((mine - oracle).abs() < 1e-5, "q.{axis}: {mine} vs {oracle}");
    }

    assert!(dist(got.origin, holding.frame.origin) > 1.0);
    let at_root = child_frame(&root, &[], &holding);
    assert!(dist(got.origin, at_root.origin) > 1.0);
    let swapped = dereth_animation::frame::combine(&holding.frame, &hand);
    assert!(dist(got.origin, swapped.origin) > 0.05);
}

struct Replayed {
    objects: ObjectStream,
    last_populated: usize,
}

fn replay_objects_upto(path: &Path, limit: usize) -> Replayed {
    let records = load(path);
    assert!(
        !records.is_empty(),
        "{} decodes to no datagrams",
        path.display()
    );
    let mut net = ClientNetwork::new(
        "127.0.0.1:19000",
        7304,
        "replay",
        "replay",
        connection_sequence_number(&records),
    )
    .expect("the loopback client binds");
    let mut objects = ObjectStream::with_store(std::sync::Arc::new(store()));
    let mut entered = false;
    let mut last_populated = 0;
    for (index, record) in records.iter().enumerate() {
        if index >= limit {
            break;
        }
        let now = LocalTime(record.t);
        if !record.c2s {
            net.feed(&record.raw, addr(record.pair), now);
        }
        net.tick(now);
        let _ = net.take_outgoing();
        for event in objects.pump(&mut net, now) {
            if let SessionEvent::CharacterSet(set) = &event {
                if !entered {
                    if let Some(character) = set.characters.first() {
                        let account = set.account.clone();
                        net.enter_world(character.gid, &account);
                        entered = true;
                    }
                }
            }
        }
        if !objects.is_empty() {
            last_populated = index;
        }
    }
    Replayed {
        objects,
        last_populated,
    }
}

fn in_world(path: &Path) -> ObjectStream {
    let last = replay_objects_upto(path, usize::MAX).last_populated;
    replay_objects_upto(path, last + 1).objects
}

/// Behaviour: objects.parent.recorded-parent-events-attach-objects-to-their-holders
/// Every recorded parent event must reach the object model before session teardown clears it.
#[test]
fn the_corpus_parent_events_all_land() {
    let paths = sessions();
    assert!(
        !paths.is_empty(),
        "the capture corpus is this station's oracle and is empty"
    );
    let mut applied = 0u64;
    let mut held = 0usize;
    let mut by_location: BTreeMap<(u32, u32), usize> = BTreeMap::new();
    for path in &paths {
        let stream = in_world(path);
        applied += stream.stats.parent_events;
        for (_, presence) in stream.presences() {
            if let Some((_, location)) = presence.parent {
                held += 1;
                *by_location
                    .entry((location, presence.placement))
                    .or_default() += 1;
            }
        }
    }
    assert!(
        applied > 0,
        "no parent event in the corpus reached a presence"
    );
    assert!(
        held > 0,
        "no corpus object is attached while its object model is live"
    );
    assert!(!by_location.is_empty());
}
