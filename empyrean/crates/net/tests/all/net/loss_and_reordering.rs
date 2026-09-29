//! Vectors: deterministic MemoryNet loss, duplication and reordering schedules in this module
//! At 20% loss/reorder/duplication every message arrives exactly once; server-to-client order
//! holds when nothing is lost.
//! Fixture: locally constructed packets and session state on a virtual clock.

use std::time::Duration;

use dereth_primitives::NetQueue;
use empyrean_net::driver::memory::LinkModel;
use empyrean_net::{Event, GameMessageGroup, OutboundMessage, SessionId};

use crate::common::{client_addr, connect, lossy_harness, session_of, TICK};
use crate::fragments::message;

/// Every message is delivered exactly once in both directions, and in order client to server,
/// although a fifth of the datagrams each way are lost, a fifth held back and a fifth duplicated.
/// Recovery is ACE's (the server asks for gaps and resends what the client asks for) against the
/// client transport's (the same, from the other side).
#[test]
fn every_message_arrives_exactly_once_at_20_percent_loss() {
    for seed in 1u64..=8 {
        run(
            LinkModel::lossy(0.2, seed),
            LinkModel::lossy(0.2, seed.wrapping_mul(0x9E37)),
            false,
        );
    }
}

/// With duplication (20%) but no loss or reordering, server-to-client order holds too.
#[test]
fn server_to_client_order_holds_when_nothing_is_lost() {
    let dup = |seed| LinkModel {
        duplicate: 0.2,
        ..LinkModel::lossy(0.0, seed)
    };
    run(dup(7), dup(8), true);
}

fn run(up_model: LinkModel, down_model: LinkModel, s2c_in_order: bool) {
    let seed = up_model.seed;
    let lossy = up_model.loss > 0.0;
    let mut h = lossy_harness(LinkModel::perfect(), LinkModel::perfect());
    let c = h.add_client(client_addr(30, 50_000), "lossy", "pw");
    assert!(
        connect(&mut h, c, Duration::from_secs(5)),
        "seed {seed}: handshake"
    );
    h.net.set_link_models(up_model, down_model);
    let id: SessionId = session_of(&h, c).expect("session");
    h.events.clear();

    let sizes = [12usize, 300, 1100, 40, 460, 2000, 8, 900];
    let mut c2s = Vec::new();
    let mut s2c: Vec<(GameMessageGroup, Vec<u8>)> = Vec::new();
    #[allow(clippy::cast_possible_truncation)]
    for i in 0..32u32 {
        let len = sizes[i as usize % sizes.len()];
        let up = message(0xF7B1 ^ (i << 16), len);
        h.clients[c].send(NetQueue::Weenie, &up);
        c2s.push(up);
        let group = if i % 3 == 0 {
            GameMessageGroup::SmartboxQueue
        } else {
            GameMessageGroup::UIQueue
        };
        let down = message(0xF000 + i, sizes[(i as usize + 3) % sizes.len()]);
        h.net.server.send(
            id,
            OutboundMessage {
                group,
                data: down.clone(),
            },
        );
        s2c.push((group, down));
        h.run_until(Duration::from_millis(100), TICK, |_| false);
    }

    let mut got_s2c: Vec<(NetQueue, Vec<u8>)> = Vec::new();
    let done = h.run_until(Duration::from_secs(120), TICK, |h| {
        while let Some(m) = h.clients[c].poll() {
            let mut data = m.opcode.to_le_bytes().to_vec();
            data.extend_from_slice(&m.body);
            got_s2c.push((m.queue, data));
        }
        let up = h
            .events
            .iter()
            .filter(|e| matches!(e, Event::Message { .. }))
            .count();
        up >= c2s.len() && got_s2c.len() >= s2c.len()
    });
    let got_c2s: Vec<&Vec<u8>> = h
        .events
        .iter()
        .filter_map(|e| match e {
            Event::Message { message, .. } => Some(&message.data),
            _ => None,
        })
        .collect();
    let (up, down) = h.net.link_stats();
    assert!(
        done,
        "seed {seed}: {} of {} up, {} of {} down; links up {up:?} down {down:?}",
        got_c2s.len(),
        c2s.len(),
        got_s2c.len(),
        s2c.len()
    );
    if lossy {
        assert!(
            up.dropped > 0 && up.reordered > 0 && up.duplicated > 0,
            "the link really was lossy: {up:?}"
        );
        assert!(
            down.dropped > 0 && down.reordered > 0 && down.duplicated > 0,
            "{down:?}"
        );
    } else {
        assert!(
            up.duplicated > 0 && down.duplicated > 0,
            "the links really duplicated"
        );
    }

    let summary = |v: &[&Vec<u8>]| {
        v.iter()
            .map(|m| (u32::from_le_bytes([m[0], m[1], m[2], m[3]]), m.len()))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        summary(&got_c2s),
        summary(&c2s.iter().collect::<Vec<_>>()),
        "seed {seed}: client to server"
    );
    assert_eq!(
        got_c2s,
        c2s.iter().collect::<Vec<_>>(),
        "seed {seed}: client to server"
    );
    for (group, queue) in [
        (GameMessageGroup::UIQueue, NetQueue::UiQueue),
        (GameMessageGroup::SmartboxQueue, NetQueue::WorldObjects),
    ] {
        let mut want: Vec<&Vec<u8>> = s2c
            .iter()
            .filter(|(g, _)| *g == group)
            .map(|(_, d)| d)
            .collect();
        let mut have: Vec<&Vec<u8>> = got_s2c
            .iter()
            .filter(|(q, _)| *q == queue)
            .map(|(_, d)| d)
            .collect();
        if !s2c_in_order {
            // Exactly once, in whatever order the resends produced.
            want.sort();
            have.sort();
        }
        assert_eq!(summary(&have), summary(&want), "seed {seed}: {group:?}");
        assert_eq!(have, want, "seed {seed}: {group:?}");
    }
    assert!(
        h.net.server.session(id).is_some(),
        "seed {seed}: the session survived"
    );
}
