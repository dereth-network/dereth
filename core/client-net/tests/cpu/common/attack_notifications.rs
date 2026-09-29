//! Shared fixtures for attack notifications.

#![allow(dead_code, unused_imports)]

pub(crate) use dereth_client_net::client_session::testing::{Corpus, Direction};
pub(crate) use dereth_protocol::combat::{AttackerNotification, DefenderNotification};
pub(crate) use dereth_protocol::Message;

pub(crate) const ATTACKER: u32 = 0x01B1;
pub(crate) const DEFENDER: u32 = 0x01B2;
pub(crate) const EVASION_ATTACKER: u32 = 0x01B3;
pub(crate) const EVASION_DEFENDER: u32 = 0x01B4;
pub(crate) const ALLEGIANCE_UPDATE_DONE: u32 = 0x01C8;

pub(crate) const GAME_EVENT: u32 = 0xF7B0;
pub(crate) const GAME_ACTION: u32 = 0xF7B1;

pub(crate) struct Body {
    pub(crate) scenario: String,
    pub(crate) idx: usize,
    pub(crate) bytes: Vec<u8>,
}

pub(crate) fn bodies_of(op: u32) -> Vec<Body> {
    let mut out = Vec::new();
    let (mut s2c, mut c2s) = (0usize, 0usize);
    for corpus in Corpus::shared_all() {
        let scenario = &corpus.name;
        for b in &corpus.blobs {
            match b.dir {
                Direction::ServerToClient => s2c += 1,
                Direction::ClientToServer => c2s += 1,
            }
            let p = &b.payload;
            let (sub, body): (u32, &[u8]) = if b.opcode == GAME_EVENT && p.len() >= 16 {
                (u32::from_le_bytes([p[12], p[13], p[14], p[15]]), &p[16..])
            } else if b.opcode == GAME_ACTION && p.len() >= 12 {
                (u32::from_le_bytes([p[8], p[9], p[10], p[11]]), &p[12..])
            } else {
                (b.opcode, if p.len() >= 4 { &p[4..] } else { &[] })
            };
            if sub == op {
                out.push(Body {
                    scenario: scenario.to_string(),
                    idx: b.idx,
                    bytes: body.to_vec(),
                });
            }
        }
    }
    assert!(s2c > 0 && c2s > 0, "recordings contain both directions");
    out
}

pub(crate) fn client_read<M: Message>(body: &[u8]) -> (M, usize, Vec<u8>) {
    let mut r = dereth_protocol::Reader::body(body);
    let m = M::read(&mut r).expect("the codec must decode a recorded body");
    let remaining = r.remaining();
    (m, remaining, r.rest().to_vec())
}

pub(crate) fn client_end(body: &[u8], fixed_len: usize) -> usize {
    let name_len = usize::from(u16::from_le_bytes([body[0], body[1]]));
    (2 + name_len).div_ceil(4) * 4 + fixed_len
}
