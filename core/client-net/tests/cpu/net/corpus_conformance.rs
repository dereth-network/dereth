//! Contracts for corpus conformance.
//! Fixture: shared recorded messages and synthetic state.

use crate::common::session_fixture::*;

/// Behaviour: link.session-replay.every-recorded-client-blob-is-re-originated-byte-for-byte
#[test]
fn replay_every_scenario() {
    let mut ran = 0usize;
    let mut delivered = 0usize;
    let mut compared = 0usize;
    let mut driven = 0usize;
    let mut want_delivered = 0usize;
    let mut want_clients = 0usize;
    let mut exact = 0usize;
    let mut unmodelled: std::collections::BTreeMap<Unmodelled, usize> =
        std::collections::BTreeMap::new();
    for corpus in Corpus::shared_all() {
        let name = &corpus.name;
        ran += 1;
        want_delivered += corpus
            .blobs
            .iter()
            .filter(|b| b.dir == Direction::ServerToClient)
            .count();
        want_clients += corpus
            .blobs
            .iter()
            .filter(|b| b.dir == Direction::ClientToServer)
            .count();
        let want_compared = corpus
            .blobs
            .iter()
            .filter(|b| b.dir == Direction::ClientToServer && drivable(b))
            .count();
        let mut s = session();
        let report = dereth_client_net::client_session::testing::replay(&mut s, corpus);
        let skipped: usize = report.unmodelled.values().sum();
        eprintln!(
            "  {name}: {} s2c blobs delivered | {} of {} client blobs compared ({} driven, \
             {} self-originated, {} byte-identical) | {} not modelled in {} opcode(s) {:?}",
            report.delivered,
            report.compared,
            report.compared + skipped,
            report.driven,
            report.compared - report.driven,
            report.exact,
            skipped,
            report.unmodelled.len(),
            report
                .unmodelled
                .iter()
                .map(|(k, v)| (k.to_string(), *v))
                .collect::<Vec<_>>()
        );
        assert!(
            report.mismatches.is_empty(),
            "scenario {name}: {} mismatches, first: {}",
            report.mismatches.len(),
            report.mismatches[0]
        );
        assert!(report.delivered > 0, "scenario {name} delivered nothing");
        assert_eq!(
            report.compared, want_compared,
            "scenario {name}: the corpus carries {want_compared} client blob(s) this harness \
             can originate and it compared {}",
            report.compared
        );
        delivered += report.delivered;
        compared += report.compared;
        driven += report.driven;
        exact += report.exact;
        for (op, n) in &report.unmodelled {
            *unmodelled.entry(*op).or_default() += n;
        }
    }
    let skipped: usize = unmodelled.values().sum();
    eprintln!(
        "corpus: {ran} scenarios replayed | {delivered} server blobs delivered | {compared} \
         client blobs compared, {driven} of them driven and {} originated by the session alone, \
         {exact} byte-identical including the action-order header stamp | {skipped} client blobs not \
         modelled, in {} opcode(s): {:?}",
        compared - driven,
        unmodelled.len(),
        unmodelled
            .iter()
            .map(|(k, v)| (k.to_string(), *v))
            .collect::<Vec<_>>()
    );
    assert!(ran > 0, "the replay harness iterated nothing");
    assert_eq!(
        ran,
        Corpus::shared_all().len(),
        "every named scenario must replay"
    );
    assert_eq!(
        delivered, want_delivered,
        "the corpus is not the one this gate was measured against"
    );
    assert_eq!(
        compared, want_clients,
        "the number of client blobs this gate is an oracle for"
    );
    assert!(driven > 0 && driven <= compared);
    assert_eq!(
        exact, compared,
        "compared *and* byte-identical, stamp included"
    );
    assert_eq!(
        skipped, 0,
        "the denominator: client blobs still not an oracle"
    );
    assert_eq!(
        unmodelled.keys().copied().collect::<Vec<_>>(),
        Vec::<Unmodelled>::new(),
        "every client blob in the corpus can now be re-originated; anything named here has \
         stopped being drivable and the opcode above says which"
    );
}

#[test]
fn the_requested_capture_changes_character_three_times_on_one_connection() {
    let corpus = Corpus::shared("requested-death-vitae-salvage");
    let mut s = session();
    let report = dereth_client_net::client_session::testing::replay(&mut s, corpus);
    assert!(
        report.mismatches.is_empty(),
        "{} mismatch(es), first: {}",
        report.mismatches.len(),
        report
            .mismatches
            .first()
            .map_or(String::new(), ToString::to_string)
    );

    let created: Vec<ObjectId> = s
        .drain_events()
        .filter_map(|e| match e {
            SessionEvent::PlayerCreated(id) => Some(id),
            _ => None,
        })
        .collect();
    assert_eq!(
        created,
        corpus
            .blobs
            .iter()
            .filter(|b| b.dir == Direction::ServerToClient && b.opcode == 0xF746)
            .map(|b| ObjectId(u32::from_le_bytes(b.payload[4..8].try_into().unwrap())))
            .collect::<Vec<_>>(),
        "six character sessions on three characters, over one connection"
    );

    assert_eq!(
        report.compared,
        corpus
            .blobs
            .iter()
            .filter(|b| b.dir == Direction::ClientToServer)
            .count(),
        "client blobs this gate is an oracle for in `requested-death-vitae-salvage`"
    );
    assert_eq!(
        report.exact, report.compared,
        "every one of them byte-identical, `action-order header` stamp included"
    );
    assert!(report.unmodelled.is_empty(), "{:?}", report.unmodelled);
}

#[test]
fn every_wire_queue_id_round_trips() {
    use dereth_client_net::client_session::testing::{queue_from_id, queue_id};
    for id in u8::MIN..=u8::MAX {
        assert_eq!(queue_id(queue_from_id(id)), id);
    }
}
