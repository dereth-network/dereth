//! Received messages and world-send assertions for synthetic server fixtures.
use dereth_protocol::{events::split_ui_blob, Message};
use empyrean_entity::enums::PropertyInt;
use empyrean_net::SessionId;
use empyrean_testkit::{ClientId, TestServer};
use empyrean_world::network::game_messages::game_message::{start_capture, take_sent};
const PRIVATE_INT: u32 = 0x02CD;

/// A received message: its kind and the whole blob.
#[derive(Debug, Clone)]
pub(crate) struct Got {
    pub(crate) kind: u32,
    pub(crate) blob: Vec<u8>,
}

impl Got {
    pub(crate) fn decode<M: Message>(&self) -> M {
        let split = split_ui_blob(&self.blob).expect("a blob");
        let mut body = split.body;
        let m = M::read(&mut body).unwrap_or_else(|e| panic!("0x{:04X} decodes: {e:?}", self.kind));
        if split.order.is_none() {
            assert_eq!(split.sub_type.0, M::OPCODE.0);
        }
        m
    }
}

pub(crate) fn got(ts: &TestServer, id: ClientId, from: usize) -> Vec<Got> {
    ts.received_raw(id)[from..]
        .iter()
        .map(|m| {
            let mut blob = m.opcode.to_le_bytes().to_vec();
            blob.extend_from_slice(&m.body);
            let kind = if m.opcode == 0xF7B0 {
                u32::from_le_bytes(m.body[8..12].try_into().unwrap())
            } else {
                m.opcode
            };
            Got { kind, blob }
        })
        .collect()
}

pub(crate) fn kinds(g: &[Got]) -> Vec<u32> {
    g.iter().map(|g| g.kind).collect()
}

/// `PrivateUpdatePropertyInt(PropertyInt.Age)`, the player's own heartbeat, is left out.
pub(crate) fn is_age_update(blob: &[u8]) -> bool {
    blob.len() >= 9
        && u32::from_le_bytes(blob[0..4].try_into().unwrap()) == PRIVATE_INT
        && u32::from_le_bytes(blob[5..9].try_into().unwrap()) == u32::from(PropertyInt::Age.0)
}

/// What the world sent to `session` while `act` ran and the server advanced `secs` (ACE's send
/// order), and what the client received meanwhile. Everything sent must have arrived.
pub(crate) fn exchange(
    ts: &mut TestServer,
    id: ClientId,
    session: SessionId,
    secs: f64,
    act: impl FnOnce(&mut TestServer),
) -> (Vec<u32>, Vec<Got>) {
    let n = ts.received_raw(id).len();
    start_capture();
    act(ts);
    ts.advance(secs);
    let sent: Vec<u32> = take_sent()
        .into_iter()
        .filter(|(s, _, b)| *s == session && !is_age_update(b))
        .map(|(_, _, b)| {
            let word = |i: usize| u32::from_le_bytes(b[i..i + 4].try_into().unwrap());
            if word(0) == 0xF7B0 {
                word(12)
            } else {
                word(0)
            }
        })
        .collect();
    let received: Vec<Got> = got(ts, id, n)
        .into_iter()
        .filter(|m| !is_age_update(&m.blob))
        .collect();
    let (mut a, mut b) = (sent.clone(), kinds(&received));
    a.sort_unstable();
    b.sort_unstable();
    assert_eq!(a, b, "everything sent arrives: sent {sent:04X?}");
    (sent, received)
}

pub(crate) fn first(g: &[Got], kind: u32) -> &Got {
    g.iter()
        .find(|m| m.kind == kind)
        .unwrap_or_else(|| panic!("no 0x{kind:04X} in {:04X?}", kinds(g)))
}

/// Asserts the world-send stream, excluding the player's periodic age update.
pub(crate) fn assert_sent_kinds(session: SessionId, expected: &[u32]) {
    let sent: Vec<u32> = take_sent()
        .into_iter()
        .filter(|(recipient, _, blob)| *recipient == session && !is_age_update(blob))
        .map(|(_, _, blob)| {
            let word = |at: usize| u32::from_le_bytes(blob[at..at + 4].try_into().unwrap());
            if word(0) == 0xF7B0 {
                word(12)
            } else {
                word(0)
            }
        })
        .collect();
    assert_eq!(
        sent, expected,
        "the world sends exactly the expected replies"
    );
}
