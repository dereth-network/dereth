//! Contracts for social recordings.
//! Fixture: shared recorded messages and synthetic state.

use dereth_client_net::client_session::testing::{Corpus, Direction};
use dereth_protocol::{Message, Opcode, OrderedEventHeader};

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

fn event_sub(payload: &[u8]) -> Option<u32> {
    if payload.len() >= 16 && u32_at(payload, 0) == OrderedEventHeader::MAGIC {
        Some(u32_at(payload, 12))
    } else {
        None
    }
}

/// Behaviour: social.recordings.fellowship-and-allegiance-rosters-reach-the-session
#[test]
fn every_recorded_social_roster_decodes_its_members_and_locks() {
    let mut total_full_updates = 0usize;
    let mut total_rosters = 0usize;
    let mut total_allegiance = 0usize;
    for corpus in Corpus::shared_all() {
        let name = &corpus.name;
        let mut full = 0usize;
        let mut allegiance = 0usize;
        let mut rosters = 0usize;
        for b in corpus
            .blobs
            .iter()
            .filter(|b| b.dir == Direction::ServerToClient)
        {
            match event_sub(&b.payload) {
                Some(sub) if sub == Opcode::FELLOWSHIP_FULL_UPDATE.0 => {
                    let mut r = dereth_protocol::archive::Reader::new(&b.payload[16..]);
                    let m = dereth_protocol::social::FellowshipFullUpdate::read(&mut r)
                        .unwrap_or_else(|e| panic!("{name} blob {}: 0x02BE decodes: {e}", b.idx));
                    assert!(
                        !m.0.members.entries.is_empty(),
                        "{name} blob {}: a fellowship with members",
                        b.idx
                    );
                    assert_eq!(
                        (m.0.locks, r.remaining()),
                        (dereth_protocol::social::FellowshipLocks::default(), 0),
                        "{name} blob {}: the empty lock table ends the message",
                        b.idx
                    );
                    full += 1;
                }
                Some(sub) if sub == Opcode::ALLEGIANCE_ALLEGIANCE_UPDATE.0 => {
                    let mut r = dereth_protocol::archive::Reader::new(&b.payload[16..]);
                    let m = dereth_protocol::social::AllegianceUpdate::read(&mut r)
                        .unwrap_or_else(|e| panic!("{name} blob {}: 0x0020 decodes: {e}", b.idx));
                    allegiance += 1;
                    if !m.profile.hierarchy.members.is_empty() {
                        rosters += 1;
                    }
                }
                _ => {}
            }
        }
        eprintln!(
            "{name}: {full} 0x02BE Fellowship_FullUpdate, {allegiance} 0x0020 \
             Allegiance_AllegianceUpdate of which {rosters} carry a roster"
        );
        let count = |op| {
            corpus
                .blobs
                .iter()
                .filter(|b| b.dir == Direction::ServerToClient && event_sub(&b.payload) == Some(op))
                .count()
        };
        assert_eq!(full, count(Opcode::FELLOWSHIP_FULL_UPDATE.0));
        assert_eq!(allegiance, count(Opcode::ALLEGIANCE_ALLEGIANCE_UPDATE.0));
        assert!(rosters <= allegiance);
        total_full_updates += full;
        total_allegiance += allegiance;
        total_rosters += rosters;
    }
    assert!(total_full_updates > 0 && total_allegiance > 0 && total_rosters > 0);
}
