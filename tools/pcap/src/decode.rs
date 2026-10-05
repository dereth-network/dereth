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

use dereth_protocol::{Message, MessageError, Opcode, Reader};

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

/// Why a codec refused a body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    /// The error's variant, and the field for the variants that name one: `UnexpectedEof`,
    /// `TrailingBytes`, `InvalidValue:<field>`, ...
    pub kind: String,
    /// The offset in the blob where the reader stood when it stopped.
    pub offset: usize,
    /// The error's full text.
    pub text: String,
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

type DecodeFn = fn(&[u8], usize) -> Result<(String, u8), Failure>;

/// One codec.
#[derive(Debug, Clone, Copy)]
pub struct Codec {
    /// `module::Type` in `dereth_protocol`.
    pub name: &'static str,
    pub opcode: u32,
    /// The direction, when the opcode's two directions carry different bodies.
    pub only: Option<Dir>,
    decode: DecodeFn,
}

impl Codec {
    /// Whether this codec reads messages travelling `dir`.
    #[must_use]
    pub fn reads(&self, dir: Dir) -> bool {
        if let Some(only) = self.only {
            return only == dir;
        }
        match Opcode(self.opcode).info().map(|i| i.direction) {
            Some(dereth_protocol::Direction::S2C) => dir == Dir::S2c,
            Some(dereth_protocol::Direction::C2S) => dir == Dir::C2s,
            _ => true,
        }
    }
}

fn failure(e: &MessageError, offset: usize) -> Failure {
    let kind = match e {
        MessageError::UnexpectedEof { .. } => "UnexpectedEof".to_string(),
        MessageError::TrailingBytes { .. } => "TrailingBytes".to_string(),
        MessageError::BadMagic { .. } => "BadMagic".to_string(),
        MessageError::OpcodeMismatch { .. } => "OpcodeMismatch".to_string(),
        MessageError::InvalidValue { field, .. } => format!("InvalidValue:{field}"),
        MessageError::LengthOverrun { field, .. } => format!("LengthOverrun:{field}"),
        MessageError::Unsupported { field, .. } => format!("Unsupported:{field}"),
        MessageError::Unencodable { field, .. } => format!("Unencodable:{field}"),
    };
    Failure {
        kind,
        offset,
        text: e.to_string(),
    }
}

/// Read one body at `origin` with message type `M`.
fn dec<M: Message + std::fmt::Debug>(body: &[u8], origin: usize) -> Result<(String, u8), Failure> {
    let mut r = Reader::with_origin(body, origin);
    match M::read(&mut r) {
        Ok(m) => {
            let left = r.remaining();
            let at = r.blob_offset();
            if left == 0 {
                return Ok((format!("{m:?}"), 0));
            }
            if left < 4 && r.rest().iter().all(|b| *b == 0) {
                return Ok((format!("{m:?}"), u8::try_from(left).unwrap_or(0)));
            }
            Err(failure(&MessageError::TrailingBytes { left }, at))
        }
        Err(e) => Err(failure(&e, r.blob_offset())),
    }
}

/// The server's `0xF7DE` chat-server packet: the chat library's own framing, read by the typed
/// incoming decoder rather than the pass-through envelope.
fn turbine_incoming(body: &[u8], origin: usize) -> Result<(String, u8), Failure> {
    dereth_protocol::turbine::decode_incoming(body)
        .map(|p| (format!("{p:?}"), 0))
        .map_err(|e| failure(&e, origin))
}

macro_rules! c {
    ($m:ident :: $t:ident, $ty:ty, auto) => {
        Codec {
            name: concat!(stringify!($m), "::", stringify!($t)),
            opcode: <$ty as Message>::OPCODE.0,
            only: None,
            decode: dec::<$ty>,
        }
    };
    ($m:ident :: $t:ident, $ty:ty, $d:ident) => {
        Codec {
            name: concat!(stringify!($m), "::", stringify!($t)),
            opcode: <$ty as Message>::OPCODE.0,
            only: Some(Dir::$d),
            decode: dec::<$ty>,
        }
    };
}

/// Types of `dereth_protocol` deliberately not in [`REGISTRY`], with the reason. The registry test
/// requires every `Message` in the crate's source to be in one list or the other.
pub const NOT_REGISTERED: &[(&str, &str)] = &[(
    "comms::CommunicationTurbineChat",
    "the pass-through envelope accepts any body; 0xF7DE is read by turbine::SendToRoomById \
     (client to server) and turbine::decode_incoming (server to client)",
)];

/// The name of the one codec that is not a `Message` type.
const TURBINE_INCOMING: &str = "turbine::decode_incoming";

macro_rules! codecs {
    (messages { $( $m:ident :: $t:ident, $ty:ty, $capture:ident, $rank:tt, $fields:ident; )* }
     opaque { $( $om:ident :: $ot:ident, $oty:ty, $orank:tt, $ofields:ident; )* }) => {
        /// Every codec, in protocol declaration order, after the structured incoming envelope.
        pub static REGISTRY: &[Codec] = &[
            Codec {
                name: TURBINE_INCOMING,
                opcode: 0xF7DE,
                only: Some(Dir::S2c),
                decode: turbine_incoming,
            },
            $( c!($m::$t, $ty, $capture), )*
        ];
    };
}

dereth_protocol::for_each_message!(codecs);

/// The codecs for one type in one direction.
pub fn codecs(dir: Dir, mtype: u32) -> impl Iterator<Item = &'static Codec> {
    REGISTRY
        .iter()
        .filter(move |c| c.opcode == mtype && c.reads(dir))
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
        match (c.decode)(body, origin) {
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
                if c.reads(dir) {
                    let n = codecs(dir, c.opcode).count();
                    assert_eq!(n, 1, "{:#06X} {} has {n} codecs", c.opcode, dir.as_str());
                }
            }
        }
    }
}
