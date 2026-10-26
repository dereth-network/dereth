//! Every reassembled message through `dereth-protocol`.
//!
//! A message is `[opcode][body]`. Server-to-client game events may be wrapped in the ordered-event
//! header (`0xF7B0`, the recipient and a stamp, then the event type) and client-to-server game
//! actions in the ordered-action header (`0xF7B1`, a stamp, then the action type); the wrapped
//! type is the *sub-opcode*, and the type a message is filed under ([`Decoded::mtype`]) is the
//! sub-opcode when there is one and the opcode otherwise.
//!
//! The codec for a type is looked up in [`REGISTRY`]: every `dereth_protocol::Message`, keyed by its
//! opcode and by the direction the master opcode table gives it. A handful of opcodes carry a
//! different body each way and name their direction here. The body is read at the alignment
//! origin it has in the blob, and it must be consumed exactly, allowing only the sender's
//! alignment padding (at most three trailing zero bytes), which is the rule the client's own
//! dispatch uses (`dereth_protocol::read_body_padded`).

use dereth_protocol::Opcode;

use crate::flows::Dir;

/// How a message decoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// A codec read the body exactly.
    Ok,
    /// A codec exists and refused the body.
    Error,
    /// No codec for this type in this direction.
    Unknown,
    /// Shorter than its own type dword.
    Short,
    /// A message of an Asheron's Call 2 session, stored whole and not decoded ([`crate::ac2`]).
    Ac2,
}

impl Status {
    /// The spelling the index stores.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Error => "error",
            Self::Unknown => "unknown",
            Self::Short => "short",
            Self::Ac2 => "ac2",
        }
    }

    /// The slot in a session's per-status counts: ok, error, unknown, short, ac2.
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::Ok => 0,
            Self::Error => 1,
            Self::Unknown => 2,
            Self::Short => 3,
            Self::Ac2 => 4,
        }
    }
}

/// Decode one reassembled blob of a session of `game`: the per-game decode hook. AC1 (and a
/// session of undetermined game) goes through `dereth-protocol` ([`decode`]); AC2 through
/// [`crate::ac2::decode`], which is where an AC2 codec plugs in.
#[must_use]
pub fn decode_as(game: crate::game::Game, dir: Dir, blob: &[u8]) -> Decoded {
    match game.decoder() {
        crate::game::Game::Ac2 => crate::ac2::decode(dir, blob),
        crate::game::Game::Ac1 | crate::game::Game::Unknown => decode(dir, blob),
    }
}

#[cfg(test)]
use dereth_protocol::codecs::TURBINE_INCOMING;
pub use dereth_protocol::codecs::{Codec, Failure, NOT_REGISTERED, REGISTRY};

/// The protocol registry's direction for one of this tool's.
#[must_use]
pub const fn way(dir: Dir) -> dereth_protocol::codecs::Dir {
    match dir {
        Dir::C2s => dereth_protocol::codecs::Dir::C2s,
        Dir::S2c => dereth_protocol::codecs::Dir::S2c,
    }
}

/// A decoded message.
#[derive(Debug, Clone, PartialEq)]
pub struct Decoded {
    /// The blob's first dword.
    pub opcode: u32,
    /// The wrapped type, for an ordered event or action.
    pub sub_opcode: Option<u32>,
    /// The type the message is filed under.
    pub mtype: u32,
    /// The ordered-event header's object (the event's recipient).
    pub order_iid: Option<u32>,
    /// The ordered header's stamp.
    pub order_stamp: Option<u32>,
    pub status: Status,
    /// The codec that read it (`module::Type`), or the first that refused it.
    pub codec: Option<&'static str>,
    pub failure: Option<Failure>,
    /// Trailing zero bytes accepted as the sender's alignment padding.
    pub padding: u8,
    /// The decoded fields as JSON ([`crate::debug_json`]).
    pub fields: Option<String>,
    /// Every object id in the decoded fields, with its path.
    pub guids: Vec<(u32, String)>,
}

/// The codecs for one type in one direction.
pub fn codecs(dir: Dir, mtype: u32) -> impl Iterator<Item = &'static Codec> {
    dereth_protocol::codecs::codecs(way(dir), mtype)
}

fn u32_at(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?))
}

/// The most object-id entries kept per message.
const MAX_GUIDS: usize = 4096;

/// Decode one reassembled blob travelling `dir`.
#[must_use]
pub fn decode(dir: Dir, blob: &[u8]) -> Decoded {
    let mut d = Decoded {
        opcode: 0,
        sub_opcode: None,
        mtype: 0,
        order_iid: None,
        order_stamp: None,
        status: Status::Short,
        codec: None,
        failure: None,
        padding: 0,
        fields: None,
        guids: Vec::new(),
    };
    let Some(opcode) = u32_at(blob, 0) else {
        return d;
    };
    let (sub, iid, stamp, origin) = match (dir, opcode) {
        (Dir::S2c, 0xF7B0) if blob.len() >= 16 => {
            (u32_at(blob, 12), u32_at(blob, 4), u32_at(blob, 8), 16)
        }
        (Dir::C2s, 0xF7B1) if blob.len() >= 12 => (u32_at(blob, 8), None, u32_at(blob, 4), 12),
        _ => (None, None, None, 4),
    };
    d.opcode = opcode;
    d.sub_opcode = sub;
    d.mtype = sub.unwrap_or(opcode);
    d.order_iid = iid;
    d.order_stamp = stamp;
    d.status = Status::Unknown;
    let body = &blob[origin..];
    let mut first_failure: Option<(&'static str, Failure)> = None;
    for c in codecs(dir, d.mtype) {
        match c.decode(body, origin) {
            Ok((debug, padding)) => {
                d.status = Status::Ok;
                d.codec = Some(c.name);
                d.padding = padding;
                if let Some(f) = crate::debug_json::convert(&debug) {
                    d.fields = Some(f.json.to_string());
                    d.guids = f.guids;
                    d.guids.truncate(MAX_GUIDS);
                } else {
                    d.fields = Some(serde_json::Value::String(debug).to_string());
                }
                first_failure = None;
                break;
            }
            Err(f) => {
                if first_failure.is_none() {
                    first_failure = Some((c.name, f));
                }
            }
        }
    }
    if let Some((name, f)) = first_failure {
        d.status = Status::Error;
        d.codec = Some(name);
        d.failure = Some(f);
    }
    if let Some(i) = iid {
        d.guids.push((i, "$order.iid".into()));
    }
    d
}

/// The master table's name for a type, if it has one.
#[must_use]
pub fn type_name(mtype: u32) -> Option<&'static str> {
    Opcode(mtype).info().map(|i| i.name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_primitives::ObjectId;
    use dereth_protocol::login::LoginWorldInfo;

    #[test]
    fn a_bare_message_decodes_and_names_its_codec() {
        let m = LoginWorldInfo {
            connections: 12,
            max_connections: 800,
            world_name: "Frostfell".into(),
        };
        let blob = dereth_protocol::write_blob(&m).unwrap();
        let d = decode(Dir::S2c, &blob);
        assert_eq!(d.status, Status::Ok, "{d:?}");
        assert_eq!(d.codec, Some("login::LoginWorldInfo"));
        assert_eq!((d.opcode, d.sub_opcode, d.mtype), (0xF7E1, None, 0xF7E1));
        let v: serde_json::Value = serde_json::from_str(d.fields.as_deref().unwrap()).unwrap();
        assert_eq!(v["world_name"], "Frostfell");
    }

    #[test]
    fn a_wrapped_event_files_under_its_sub_opcode_and_indexes_the_recipient() {
        let m = dereth_protocol::objects::ItemServerSaysRemove {
            object: ObjectId(0x8000_1234),
        };
        let blob = dereth_protocol::events::pack_event(ObjectId(0x5000_0001), 7, &m).unwrap();
        let d = decode(Dir::S2c, &blob);
        assert_eq!(d.status, Status::Ok, "{d:?}");
        assert_eq!(
            (d.opcode, d.sub_opcode, d.mtype),
            (0xF7B0, Some(0x0024), 0x0024)
        );
        assert_eq!((d.order_iid, d.order_stamp), (Some(0x5000_0001), Some(7)));
        assert!(d.guids.contains(&(0x8000_1234, "$.object".into())));
        assert!(d.guids.contains(&(0x5000_0001, "$order.iid".into())));
    }

    #[test]
    fn a_short_body_is_an_error_with_its_offset_and_trailing_zeros_are_padding() {
        let m = dereth_protocol::objects::ItemServerSaysRemove {
            object: ObjectId(1),
        };
        let mut blob = dereth_protocol::events::pack_event(ObjectId(2), 1, &m).unwrap();
        blob.truncate(blob.len() - 1);
        let d = decode(Dir::S2c, &blob);
        assert_eq!(d.status, Status::Error);
        let f = d.failure.unwrap();
        assert_eq!((f.kind.as_str(), f.offset), ("UnexpectedEof", 16));

        let mut padded = dereth_protocol::events::pack_event(ObjectId(2), 1, &m).unwrap();
        padded.extend_from_slice(&[0, 0]);
        let d = decode(Dir::S2c, &padded);
        assert_eq!((d.status, d.padding), (Status::Ok, 2));
        padded.push(1);
        let d = decode(Dir::S2c, &padded);
        assert_eq!(d.failure.unwrap().kind, "TrailingBytes");
    }

    #[test]
    fn direction_picks_the_codec_and_unknowns_are_unknown() {
        // 0xF653 Login_ExecuteLogOff: an empty body from the server, the character from the client.
        let s2c = 0xF653u32.to_le_bytes();
        assert_eq!(
            decode(Dir::S2c, &s2c).codec,
            Some("login::LoginExecuteLogOff")
        );
        let mut c2s = s2c.to_vec();
        c2s.extend_from_slice(&0x5000_0001u32.to_le_bytes());
        assert_eq!(
            decode(Dir::C2s, &c2s).codec,
            Some("login::LoginExecuteLogOffRequest")
        );
        let d = decode(Dir::S2c, &0x1234_5678u32.to_le_bytes());
        assert_eq!(d.status, Status::Unknown);
        assert_eq!(decode(Dir::S2c, &[1, 2]).status, Status::Short);
    }

    #[test]
    fn a_game_action_reads_its_body_at_origin_twelve() {
        let m = dereth_protocol::objects::ItemAppraise {
            target: ObjectId(0x8000_0042),
        };
        let blob = dereth_protocol::actions::pack_action(3, &m).unwrap();
        let d = decode(Dir::C2s, &blob);
        assert_eq!(d.status, Status::Ok, "{d:?}");
        assert_eq!(
            (d.opcode, d.sub_opcode, d.order_stamp),
            (0xF7B1, Some(0x00C8), Some(3))
        );
        assert_eq!(d.guids, vec![(0x8000_0042, "$.target".into())]);
    }

    /// Every `Message` in `dereth-protocol`'s source is registered here or listed in
    /// [`NOT_REGISTERED`], so a new codec cannot be missed by the ingest.
    #[test]
    fn the_registry_names_every_message_in_dereth_protocol() {
        macro_rules! declared_types {
            (messages { $( $m:ident :: $t:ident, $ty:ty, $capture:ident, $rank:tt, $fields:ident; )* }
             opaque { $( $om:ident :: $ot:ident, $oty:ty, $orank:tt, $ofields:ident; )* }) => {
                [
                    $( (concat!(stringify!($m), "::", stringify!($t)), std::any::type_name::<$ty>()), )*
                    $( (concat!(stringify!($om), "::", stringify!($ot)), std::any::type_name::<$oty>()), )*
                ]
            };
        }
        let declarations = dereth_protocol::for_each_message!(declared_types);
        let mut names = std::collections::BTreeSet::new();
        for (name, actual_type) in declarations {
            assert!(names.insert(name), "duplicate declaration: {name}");
            assert_eq!(actual_type.strip_prefix("dereth_protocol::"), Some(name));
        }
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../core/protocol/src");
        let found = crate::registry_scan::message_types(&src);
        let source_names: std::collections::BTreeSet<_> =
            found.iter().map(String::as_str).collect();
        assert_eq!(
            names, source_names,
            "the declarations must cover every Message type"
        );
        assert!(
            found.len() > 300,
            "the scan found only {} types",
            found.len()
        );
        let missing: Vec<&String> = found
            .iter()
            .filter(|n| {
                !REGISTRY.iter().any(|c| c.name == n.as_str())
                    && !NOT_REGISTERED.iter().any(|(x, _)| x == n)
            })
            .collect();
        assert!(
            missing.is_empty(),
            "dereth-protocol messages not in the registry: {missing:?}"
        );
        for c in REGISTRY.iter().filter(|c| c.name != TURBINE_INCOMING) {
            assert!(
                found.iter().any(|n| n == c.name),
                "{} is registered but not found",
                c.name
            );
        }
    }

    /// Two codecs for one type and direction would make the choice depend on order.
    #[test]
    fn no_type_has_two_codecs_in_one_direction() {
        for c in REGISTRY {
            for dir in [Dir::C2s, Dir::S2c] {
                if c.reads(way(dir)) {
                    let n = codecs(dir, c.opcode).count();
                    assert_eq!(n, 1, "{:#06X} {} has {n} codecs", c.opcode, dir.as_str());
                }
            }
        }
    }
}
