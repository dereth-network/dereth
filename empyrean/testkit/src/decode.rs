//! Decoding every message a [`TestClient`](crate::TestClient) receives: each server-to-client
//! message type dereth-protocol knows, registered by its opcode, so a scenario can prove that every
//! message it was sent decodes as the retail client would read it.
//!
//! A message counts as decoded only when dereth-protocol also writes it back to the same bytes (at most
//! three zero bytes of the sender's alignment padding aside): decoding alone is easy to satisfy with
//! a lenient reader, re-encoding is not. [`TestServer`](crate::TestServer) checks this for every
//! message any of its clients receives ([`assert_reencodes`]), which is what proves the server's
//! builders byte for byte against dereth-protocol end to end, including the ones ACE's vectors cannot
//! reach.
//!
//! Nothing here is an ACE port; there are no ACE anchors.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use dereth_primitives::IncomingMessage;
use dereth_protocol::events::split_ui_blob;
use dereth_protocol::{self as proto, Message, MessageError, Opcode, Reader, Writer};

#[cfg(test)]
mod tests;

/// One decoder: the type's name and a function that reads a body of it and writes the decoded
/// value back (at the reader's blob offset, so the alignment rule is the same).
type Decoder = (
    &'static str,
    fn(&mut Reader<'_>) -> Result<Vec<u8>, MessageError>,
);

/// Reads a body of `M` and writes it back with dereth-protocol's default writer, whose known-type DataID
/// short form (2 bytes only below 0x4000) is the retail client's; the server writes the same
/// (V236; ACE's writer allows the short form up to 0x7FFF).
fn read_as<M: Message>(r: &mut Reader<'_>) -> Result<Vec<u8>, MessageError> {
    let origin = r.blob_offset();
    let m = M::read(r)?;
    let mut w = Writer::with_origin(origin);
    m.write(&mut w)?;
    Ok(w.into_inner())
}

/// Kinds after whose last field ACE writes more than the retail client reads, and what. The
/// message does not decode (it has bytes left over), but what dereth-protocol did read must write back
/// to the bytes sent, which [`assert_reencodes`] checks.
pub const KNOWN_TRAILING: &[(u32, &str)] = &[
    (0x02BE, "ACE appends a FellowshipLocks table after the departed fellows; the retail client stops before it"),
];

macro_rules! add_decoder {
    ($table:ident, $t:ident, $ty:ty, skip) => {};
    ($table:ident, $t:ident, $ty:ty, $rank:literal) => {
        $table
            .entry(<$ty as Message>::OPCODE.0)
            .or_default()
            .push(($rank, (stringify!($t), read_as::<$ty>)));
    };
}

macro_rules! decoders {
    (messages { $( $m:ident :: $t:ident, $ty:ty, $capture:ident, $rank:tt, $fields:ident; )* }
     opaque { $( $om:ident :: $ot:ident, $oty:ty, $orank:tt, $ofields:ident; )* }) => {
        fn table() -> &'static BTreeMap<u32, Vec<Decoder>> {
            static TABLE: OnceLock<BTreeMap<u32, Vec<Decoder>>> = OnceLock::new();
            TABLE.get_or_init(|| {
                let mut candidates: BTreeMap<u32, Vec<(usize, Decoder)>> = BTreeMap::new();
                $( add_decoder!(candidates, $t, $ty, $rank); )*
                $( add_decoder!(candidates, $ot, $oty, $orank); )*
                candidates.into_iter().map(|(op, mut rows)| {
                    rows.sort_by_key(|(rank, _)| *rank);
                    (op, rows.into_iter().map(|(_, decoder)| decoder).collect())
                }).collect()
            })
        }
    };
}

dereth_protocol::for_each_message!(decoders);

/// What one received message is: its kind (the game-event type inside a `0xF7B0`, else the
/// opcode), the dereth-protocol type it decoded as, and why it did not decode when it did not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decoded {
    /// The game-event type inside a `0xF7B0`, else the opcode.
    pub kind: u32,
    /// Whether the message came in a `0xF7B0` game-event wrapper.
    pub event: bool,
    /// The dereth-protocol type that read it, or `Err` with every candidate's error (or "no decoder").
    pub result: Result<&'static str, String>,
}

impl Decoded {
    /// The kind's name in the opcode table, when it has one.
    #[must_use]
    pub fn name(&self) -> &'static str {
        Opcode(self.kind).info().map_or("?", |i| i.name)
    }
}

/// Reads `body` with each candidate for `kind` until one consumes it (at most three zero bytes of
/// the sender's alignment padding may be left, as [`proto::read_body_padded`] allows) and writes it
/// back to the same bytes. A candidate that reads the body but writes it back differently is an
/// error ("re-encodes differently").
fn try_decoders<'a>(
    kind: u32,
    mut make: impl FnMut() -> Reader<'a>,
) -> Result<&'static str, String> {
    let Some(candidates) = table().get(&kind) else {
        return Err("no dereth-protocol decoder".to_owned());
    };
    let mut errors = Vec::new();
    for (name, read) in candidates {
        let mut r = make();
        let whole = r.clone().rest();
        match read(&mut r) {
            Ok(again) => {
                let left = r.remaining();
                let read_bytes = &whole[..whole.len() - left];
                if left >= 4 || r.rest().iter().any(|b| *b != 0) {
                    // What was read must still write back, even when ACE sent more after it.
                    if again != read_bytes && KNOWN_TRAILING.iter().any(|(k, _)| *k == kind) {
                        errors.push(format!(
                            "{name}: re-encodes differently before the {left} bytes left over"
                        ));
                    } else {
                        errors.push(format!("{name}: {left} bytes left over"));
                    }
                    continue;
                }
                if again == read_bytes {
                    return Ok(name);
                }
                let at = again
                    .iter()
                    .zip(read_bytes)
                    .position(|(a, b)| a != b)
                    .unwrap_or(again.len().min(read_bytes.len()));
                let hex = |b: &[u8]| b.iter().map(|x| format!("{x:02X}")).collect::<String>();
                errors.push(format!(
                    "{name}: re-encodes differently from byte {at} ({} bytes read, {} written): sent {}, dereth-protocol {}",
                    read_bytes.len(),
                    again.len(),
                    hex(read_bytes),
                    hex(&again)
                ));
            }
            Err(e) => errors.push(format!("{name}: {e:?}")),
        }
    }
    Err(errors.join("; "))
}

/// Decodes one received message with dereth-protocol.
#[must_use]
pub fn decode(m: &IncomingMessage) -> Decoded {
    if m.opcode == proto::OrderedEventHeader::MAGIC {
        let mut blob = m.opcode.to_le_bytes().to_vec();
        blob.extend_from_slice(&m.body);
        let kind = match split_ui_blob(&blob) {
            Ok(split) => split.sub_type.0,
            Err(e) => {
                return Decoded {
                    kind: proto::OrderedEventHeader::MAGIC,
                    event: true,
                    result: Err(format!("wrapper: {e:?}")),
                }
            }
        };
        let result = try_decoders(kind, || split_ui_blob(&blob).expect("split above").body);
        Decoded {
            kind,
            event: true,
            result,
        }
    } else {
        let result = try_decoders(m.opcode, || Reader::body(&m.body));
        Decoded {
            kind: m.opcode,
            event: false,
            result,
        }
    }
}

/// Panics when `m` decodes as a dereth-protocol type but does not re-encode to its own bytes. A message
/// dereth-protocol cannot read at all is left to [`undecoded`] and the scenario that expects it.
///
/// A TurbineChat (`0xF7DE`, passed through opaque) must also read as the retail chat client
/// reads it, with exact "bytes to follow" counts (not the ACE-extent allowance).
///
/// # Panics
/// On a message that reads but re-encodes differently, or a TurbineChat the chat client would
/// not read exactly.
pub fn assert_reencodes(m: &IncomingMessage) {
    let d = decode(m);
    if let Err(e) = &d.result {
        assert!(
            !e.contains("re-encodes differently"),
            "0x{:04X} {}: {e}",
            d.kind,
            d.name()
        );
    }
    if m.opcode == proto::Opcode::COMMUNICATION_TURBINE_CHAT.0 {
        match proto::turbine::decode_incoming(&m.body) {
            Ok(p) => assert!(
                !p.ace_overstated_extent,
                "0xF7DE: the extents are 8 too large"
            ),
            Err(e) => panic!("0xF7DE: the chat client does not read it: {e:?}"),
        }
    }
}

/// Decodes every message in `messages`; returns the ones that did not decode.
#[must_use]
pub fn undecoded(messages: &[IncomingMessage]) -> Vec<Decoded> {
    messages
        .iter()
        .map(decode)
        .filter(|d| d.result.is_err())
        .collect()
}

/// Every message of type `M` in `messages`, decoded, oldest first: a plain message with `M`'s
/// opcode, or a `0xF7B0` game event whose type is `M`'s.
///
/// # Panics
/// When a message of `M`'s kind does not decode as `M`.
#[must_use]
pub fn all_of<M: Message>(messages: &[IncomingMessage]) -> Vec<M> {
    let op = M::OPCODE.0;
    messages
        .iter()
        .filter_map(|m| {
            if m.opcode == op && op != proto::OrderedEventHeader::MAGIC {
                return Some(
                    proto::read_body_padded::<M>(&m.body)
                        .unwrap_or_else(|e| panic!("0x{op:04X} decodes: {e:?}")),
                );
            }
            if m.opcode != proto::OrderedEventHeader::MAGIC {
                return None;
            }
            let mut blob = m.opcode.to_le_bytes().to_vec();
            blob.extend_from_slice(&m.body);
            let split = split_ui_blob(&blob).ok()?;
            if split.order.is_none() || split.sub_type.0 != op {
                return None;
            }
            let mut body = split.body;
            Some(M::read(&mut body).unwrap_or_else(|e| panic!("event 0x{op:04X} decodes: {e:?}")))
        })
        .collect()
}
