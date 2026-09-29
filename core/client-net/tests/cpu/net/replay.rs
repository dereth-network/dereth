//! Contracts for replay.
//! Fixture: shared recorded messages and synthetic state.

#![cfg(feature = "replay")]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use dereth_client_net::replay::{
    compare, coverage, interval_offset, masks_of, normalise_against, Capture, Direction,
    ReplayError,
};
use dereth_transport::{CryptoSystem, OutPacket, PacketFlags, ParsedPacket, ProtoHeader};

fn captures_dir() -> PathBuf {
    const REL: &str = "fixtures/packet-captures";
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    loop {
        if dir.join(REL).join("index.json").is_file() {
            return dir.join(REL);
        }
        if !dir.pop() {
            return PathBuf::from(REL);
        }
    }
}

use dereth_client_net::client_session::testing::session_names;

/// Behaviour: link.replay.every-recorded-client-datagram-is-reproduced-byte-for-byte
#[test]
fn replay_every_recorded_capture() {
    let root = captures_dir();
    let mut total = 0usize;
    let mut total_parked = 0usize;
    let mut total_reused = 0usize;
    for name in session_names() {
        let capture = Capture::load(&root, name).unwrap_or_else(|e| {
            panic!(
                "scenario `{name}`: {e}\n\
                 This gate's oracle is the recording and a missing recording is a failure, not \
                 a skip: `fixtures/packet-captures/{name}.jsonl` is part of the checkout."
            )
        });
        let (checked, parked, reused) = run_replay(&capture);
        total_parked += parked;
        total_reused += reused;
        assert_eq!(
            checked,
            capture.client_datagrams().len(),
            "scenario {name}: the capture holds {} client datagrams and the replay checked \
             {checked}",
            capture.client_datagrams().len()
        );
        total += checked;
    }
    eprintln!(
        "corpus: {} scenarios replayed | {total} client->server datagrams reproduced byte for \
         byte, checksum included",
        session_names().len()
    );
    assert!(total > 0);
    assert!(
        total_parked > 0 && total_reused > 0,
        "recordings exercise key reuse"
    );
}

#[test]
#[ignore = "requires a capture with induced loss reaching a rejected retransmission"]
fn replay_lossy() {
    let root = captures_dir();
    let capture = Capture::load(&root, "lossy").expect("the lossy capture must exist to run this");
    run_replay(&capture);
}

#[derive(Default)]
struct KeyLedger {
    drawn: BTreeMap<u32, u32>,
    next_seq: Option<u32>,
    parked: usize,
    reused: usize,
}

impl KeyLedger {
    fn key_for(&mut self, stream: &mut CryptoSystem, header: &ProtoHeader, idx: usize) -> u32 {
        let seq = header.seq_id;
        if header.header.0 & PacketFlags::RETRANSMISSION != 0 {
            self.reused += 1;
            return *self.drawn.get(&seq).unwrap_or_else(|| {
                panic!("packet {idx}: a Retransmission of seq {seq}, whose key was never drawn")
            });
        }
        if let Some(next) = self.next_seq {
            for missing in next..seq {
                self.drawn.insert(missing, stream.next());
                self.parked += 1;
            }
        }
        let key = stream.next();
        self.drawn.insert(seq, key);
        self.next_seq = Some(seq + 1);
        key
    }
}

fn run_replay(capture: &Capture) -> (usize, usize, usize) {
    eprintln!(
        "replaying {} ({} datagrams, {} client->server); header_ coverage: {:?}",
        capture.scenario,
        capture.packets.len(),
        capture.client_datagrams().len(),
        coverage(capture)
    );

    let mut incoming = CryptoSystem::new(capture.meta.outgoing_seed);
    let mut outgoing = CryptoSystem::new(capture.meta.incoming_seed);
    let mut interval_delta: Option<u16> = None;
    let mut checked = 0usize;
    let mut in_keys = KeyLedger::default();
    let mut out_keys = KeyLedger::default();

    for dg in &capture.packets {
        match dg.dir {
            Direction::S2c => {
                let parsed = ParsedPacket::parse(&dg.raw).unwrap_or_else(|e| {
                    panic!("packet {}: the capture does not parse: {e}", dg.idx)
                });
                if parsed.header.header.is_encrypted() {
                    let key = in_keys.key_for(&mut incoming, &parsed.header, dg.idx);
                    assert!(
                        parsed.checksum_ok(Some(key)),
                        "packet {}: the captured server packet does not verify under our \
                         incoming stream -- the key stream has lost its position",
                        dg.idx
                    );
                }
            }
            Direction::C2s => {
                let expected = ParsedPacket::parse(&dg.raw).unwrap_or_else(|e| {
                    panic!("packet {}: the capture does not parse: {e}", dg.idx)
                });

                let mut ours = OutPacket::new(ProtoHeader {
                    seq_id: expected.header.seq_id,
                    rec_id: expected.header.rec_id,
                    interval: expected.header.interval,
                    iteration: expected.header.iteration,
                    header: PacketFlags(expected.header.header.0 & PacketFlags::RETRANSMISSION),
                    ..Default::default()
                });
                for (mask, bytes) in &expected.optional {
                    ours.add_optional_header(*mask, bytes.clone())
                        .unwrap_or_else(|e| panic!("packet {}: {e}", dg.idx));
                }
                for frag in &expected.fragments {
                    ours.add_fragment(frag.clone())
                        .unwrap_or_else(|e| panic!("packet {}: {e}", dg.idx));
                }

                let key = ours
                    .needs_encryption()
                    .then(|| out_keys.key_for(&mut outgoing, &expected.header, dg.idx));
                if interval_delta.is_none() {
                    interval_delta = Some(interval_offset(
                        expected.header.interval,
                        ours.header.interval,
                    ));
                }
                let emitted = normalise_against(&mut ours, &expected, key)
                    .unwrap_or_else(|e| panic!("packet {}: {e}", dg.idx));
                compare(dg.idx, &dg.raw, &emitted).expect("byte-identical replay");
                checked += 1;
            }
        }
    }
    assert!(
        checked > 0,
        "the capture has no client->server datagrams to check"
    );
    let parked = in_keys.parked + out_keys.parked;
    let reused = in_keys.reused + out_keys.reused;
    eprintln!(
        "  {}: {checked} client->server datagrams matched byte for byte ({parked} key(s) parked \
         for datagrams the capture never saw, {reused} retransmission(s) re-using one)",
        capture.scenario
    );
    (checked, parked, reused)
}

#[test]
fn the_harness_accepts_an_exact_match_and_rejects_each_kind_of_difference() {
    let hex = concat!(
        "01000000060000002e4bd6490b0000011c000100",
        "010000000000008001001c0000000900b0f700000100005002000000",
    );
    let captured: Vec<u8> = hex
        .as_bytes()
        .chunks(2)
        .map(|c| u8::from_str_radix(std::str::from_utf8(c).expect("ascii"), 16).expect("hex"))
        .collect();

    let parsed = ParsedPacket::parse(&captured).expect("the worked packet parses");
    let mut ours = OutPacket::new(ProtoHeader {
        seq_id: parsed.header.seq_id,
        rec_id: parsed.header.rec_id,
        interval: parsed.header.interval,
        iteration: parsed.header.iteration,
        ..Default::default()
    });
    ours.add_fragment(parsed.fragments[0].clone())
        .expect("frag");
    let mut outgoing = CryptoSystem::new(0xDEAD_BEEF);
    let emitted = normalise_against(&mut ours, &parsed, Some(outgoing.next())).expect("normalise");
    assert_eq!(emitted, captured);
    compare(0, &captured, &emitted).expect("an exact match is accepted");

    for (what, offset) in [
        ("seq_id", 0usize),
        ("header", 4),
        ("checksum", 8),
        ("fragment payload", 44),
    ] {
        let mut broken = captured.clone();
        broken[offset] ^= 0x01;
        assert!(
            compare(0, &captured, &broken).is_err(),
            "the harness must notice a change to {what}"
        );
    }

    let mut wrong_position = CryptoSystem::new(0xDEAD_BEEF);
    wrong_position.next();
    let second_draw = wrong_position.next();
    let mut ours = OutPacket::new(ProtoHeader {
        seq_id: parsed.header.seq_id,
        rec_id: parsed.header.rec_id,
        interval: parsed.header.interval,
        iteration: parsed.header.iteration,
        ..Default::default()
    });
    ours.add_fragment(parsed.fragments[0].clone())
        .expect("frag");
    let desynced = ours.serialize(Some(second_draw)).expect("serialize");
    let err = compare(0, &captured, &desynced).expect_err("a desynchronised key stream must fail");
    assert!(
        matches!(
            err,
            ReplayError::Mismatch {
                field: "checksum",
                ..
            }
        ),
        "{err}"
    );
}

#[test]
fn a_missing_or_malformed_corpus_never_passes_quietly() {
    let err =
        Capture::load(Path::new("definitely/not/here"), "handshake").expect_err("no corpus there");
    assert!(matches!(err, ReplayError::NoCapture(_)), "{err}");

    let empty = Capture {
        scenario: "empty".into(),
        meta: Default::default(),
        packets: Vec::new(),
    };
    assert!(empty.client_datagrams().is_empty());
}

#[test]
fn masks_are_reported_in_wire_order() {
    let mut p = OutPacket::new(ProtoHeader {
        seq_id: 1,
        ..Default::default()
    });
    p.add_optional_header(PacketFlags::FLOW, vec![0; 6])
        .expect("flow");
    p.add_optional_header(PacketFlags::ACK_SEQUENCE, vec![0; 4])
        .expect("ack");
    p.add_optional_header(PacketFlags::TIME_SYNC, vec![0; 8])
        .expect("timesync");
    let bytes = p.serialize(Some(1)).expect("serialize");
    let parsed = ParsedPacket::parse(&bytes).expect("parse");
    assert_eq!(
        masks_of(&parsed),
        vec![
            PacketFlags::ACK_SEQUENCE,
            PacketFlags::TIME_SYNC,
            PacketFlags::FLOW
        ]
    );
}
