//! Contracts for corpus matches raw recordings.
//! Fixture: shared recorded messages and synthetic state.
//! Behaviour: none (codec, fixture conformance or host-state contracts)

use std::collections::BTreeMap;

use dereth_client_net::client_session::testing::{Corpus, Direction};

const HEADER_SIZE: usize = 20;

const FRAG_HEADER_SIZE: usize = 16;

const FLAG_FRAGMENTS: u32 = 0x0000_0004;

const FLAG_LOGIN_REQUEST: u32 = 0x0001_0000;

const FIXED: &[(u32, usize)] = &[
    (0x0000_0100, 8),  // ServerSwitch
    (0x0000_0200, 16), // LogonServerAddr
    (0x0000_0400, 0),  // EmptyHeader1
    (0x0000_0800, 32), // Referral
    (0x0000_4000, 4),  // AckSequence
    (0x0000_8000, 0),  // Disconnect
    (0x0002_0000, 8),  // WorldLoginRequest
    (0x0004_0000, 32), // ConnectRequest
    (0x0008_0000, 8),  // ConnectResponse
    (0x0010_0000, 8),  // NetError
    (0x0020_0000, 8),  // NetErrorDisconnect
    (0x0040_0000, 8),  // CICMDCommand
    (0x0100_0000, 8),  // TimeSync
    (0x0200_0000, 4),  // EchoRequest
    (0x0400_0000, 8),  // EchoResponse
    (0x0800_0000, 6),  // Flow -- the only 6-byte section, and always last
];

const SEQ_ID_LISTS: &[u32] = &[0x0000_1000, 0x0000_2000];

fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

fn skip_optional(b: &[u8], flags: u32) -> usize {
    let mut masks: Vec<u32> = FIXED.iter().map(|(m, _)| *m).collect();
    masks.extend_from_slice(SEQ_ID_LISTS);
    masks.push(FLAG_LOGIN_REQUEST);
    masks.sort_unstable();
    let mut at = HEADER_SIZE;
    for mask in masks {
        if flags & mask == 0 {
            continue;
        }
        if let Some((_, n)) = FIXED.iter().find(|(m, _)| *m == mask) {
            at += n;
        } else if SEQ_ID_LISTS.contains(&mask) {
            let count = u32_at(b, at) as usize;
            at += 4 + 4 * count;
        } else {
            let slen = u16_at(b, at) as usize;
            at += 2 + slen;
            at += (4 - at % 4) % 4;
            let cb = u32_at(b, at) as usize;
            at += 4 + cb;
        }
    }
    at
}

fn raw_blob_counts(name: &str) -> (usize, usize, usize, usize) {
    let mut pending: BTreeMap<(bool, u64), (u16, Vec<u16>)> = BTreeMap::new();
    let (mut s2c, mut c2s, mut logins) = (0usize, 0usize, 0usize);
    let mut seen: std::collections::BTreeSet<(bool, u32)> = std::collections::BTreeSet::new();
    let mut duplicates = 0usize;
    for datagram in dereth_client_net::client_session::testing::capture::shared_session(name) {
        let c2s_dir = datagram.c2s;
        let b = &datagram.raw;
        assert!(
            b.len() >= HEADER_SIZE,
            "{name}: a datagram shorter than its header"
        );
        let flags = u32_at(b, 4);
        assert_eq!(
            HEADER_SIZE + u16_at(b, 16) as usize,
            b.len(),
            "{name}: header size"
        );
        if c2s_dir && flags & FLAG_LOGIN_REQUEST != 0 {
            logins += 1;
        }
        if flags & FLAG_FRAGMENTS == 0 {
            continue;
        }
        if !seen.insert((c2s_dir, u32_at(b, 0))) {
            duplicates += 1;
            continue;
        }
        let mut at = skip_optional(b, flags);
        while at + FRAG_HEADER_SIZE <= b.len() {
            let lo = u32_at(b, at);
            let hi = u32_at(b, at + 4);
            let count = u16_at(b, at + 8);
            let size = u16_at(b, at + 10) as usize;
            let num = u16_at(b, at + 12);
            assert!(
                size >= FRAG_HEADER_SIZE && at + size <= b.len(),
                "{name}: fragment overruns"
            );
            at += size;
            let key = (c2s_dir, (u64::from(hi) << 32) | u64::from(lo));
            let slot = pending.entry(key).or_insert((count, Vec::new()));
            if !slot.1.contains(&num) {
                slot.1.push(num);
            }
            if slot.1.len() == usize::from(slot.0) {
                pending.remove(&key);
                if c2s_dir {
                    c2s += 1;
                } else {
                    s2c += 1;
                }
            }
        }
    }
    assert!(
        pending.is_empty(),
        "{name}: {} blob(s) never completed",
        pending.len()
    );
    (s2c, c2s, logins, duplicates)
}

#[test]
fn every_recording_matches_raw_fragment_reassembly() {
    for corpus in Corpus::shared_all() {
        let name = &corpus.name;
        let (raw_s2c, raw_c2s, raw_logins, _) = raw_blob_counts(name);
        let got_s2c = corpus
            .blobs
            .iter()
            .filter(|b| b.dir == Direction::ServerToClient)
            .count();
        let got_c2s = corpus.blobs.len() - got_s2c;
        assert_eq!(
            (got_s2c, got_c2s),
            (raw_s2c, raw_c2s),
            "{name}: promoted versus raw messages"
        );
        assert!(raw_logins > 0, "{name}: the recording starts a connection");
        assert!(!corpus.blobs.is_empty(), "{name}: no messages");
        for (i, b) in corpus.blobs.iter().enumerate() {
            assert_eq!(b.idx, i, "{name}: dense message indices");
            assert!(b.payload.len() >= 4, "{name}: a message has an opcode");
        }
    }
}
