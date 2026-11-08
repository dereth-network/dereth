//! The server calls the world's controls make: selecting an object, using it, saying something.
//! Each is a call into the server's scripts with its arguments packed as the client packs them.

use auberean_client_sdk::script::args::Arg;
use auberean_client_sdk::script::graph::{field, Body, Entry, Graph, PackedObject};
use auberean_client_sdk::script::native_object::{NativeObject, TextInfo};
use auberean_client_sdk::Action;

/// The server's functions.
pub mod function {
    /// Select an object (its id; 0 for none).
    pub const SELECT: u32 = 0x8212_0016;
    /// Use an object: a use description naming it.
    pub const USE: u32 = 0x8216_0016;
    /// Say something aloud: flags, then the words.
    pub const SAY: u32 = 0x8205_0006;
}

/// The script package of a use description, and how many slots it has.
pub const USE_PACKAGE: u16 = 0x245;
pub const USE_SLOTS: usize = 20;
/// The slot of a use description holding the object used (two slots, low word first).
pub const USE_ITEM: usize = 2;
/// The native type of a string reference.
pub const TEXT_TYPE: u16 = 106;

/// Slot tags as the script machine sets them.
mod tag {
    pub use auberean_client_sdk::script::value::tag::{
        FRESH as HIGH, INT, LONG_INT as LONG, REFERENCE as REF,
    };
}

/// Every entry of a packed graph: an object, loaded, scannable.
const ENTRY: u32 = field::LOADED | field::SCAN | field::OBJECT;

fn call(function: u32, args: &[Arg]) -> Action {
    Action::ServerCall {
        function,
        args: auberean_client_sdk::script::args::encode(args).unwrap_or_default(),
    }
}

/// Select `id` (0 for nothing).
#[must_use]
pub fn select(id: u64) -> Action {
    call(function::SELECT, &[Arg::LongInt(id)])
}

/// Use object `id`: a use description with the object in its item slots and the rest empty, for
/// the server to fill in.
#[must_use]
pub fn use_object(id: u64) -> Action {
    let mut slots = Vec::with_capacity(USE_SLOTS);
    let mut tags = Vec::with_capacity(USE_SLOTS);
    for i in 0..USE_SLOTS {
        let (s, t) = match i {
            0 | 4 => (u32::MAX, tag::REF),
            #[allow(clippy::cast_possible_truncation)]
            USE_ITEM => (id as u32, tag::LONG),
            3 => ((id >> 32) as u32, tag::HIGH),
            12 => (0, tag::LONG),
            13 => (0, tag::HIGH),
            _ => (0, tag::INT),
        };
        slots.push(s);
        tags.push(t);
    }
    let g = Graph {
        ids: vec![1],
        entries: vec![Entry {
            field: ENTRY,
            body: Body::Object(PackedObject {
                state: 0,
                package: USE_PACKAGE,
                native_type: 0,
                native: None,
                slots,
                tags,
            }),
        }],
    };
    call(function::USE, &[Arg::Object(g)])
}

/// A string reference holding `text` as a literal.
#[must_use]
pub fn literal(text: &str) -> Graph {
    Graph {
        ids: vec![1],
        entries: vec![Entry {
            field: ENTRY,
            body: Body::Object(PackedObject {
                state: 0,
                package: 0xFFFF,
                native_type: TEXT_TYPE,
                native: Some(NativeObject::TextInfo(TextInfo {
                    has_literal: 1,
                    literal: Some(text.encode_utf16().collect()),
                    ..TextInfo::default()
                })),
                slots: Vec::new(),
                tags: Vec::new(),
            }),
        }],
    }
}

/// Say `text` aloud.
#[must_use]
pub fn say(text: &str) -> Action {
    call(function::SAY, &[Arg::Int(0), Arg::Object(literal(text))])
}

/// The literal text a call's packed arguments carry, if any: the words of a saying, a tell or a
/// text shown in the chat window.
#[must_use]
pub fn text_of(args: &[Arg]) -> Option<String> {
    args.iter().find_map(|a| match a {
        Arg::Object(g) => g.entries.iter().find_map(|e| match &e.body {
            Body::Object(PackedObject {
                native: Some(NativeObject::TextInfo(t)),
                ..
            }) => t.literal.as_ref().map(|l| String::from_utf16_lossy(l)),
            _ => None,
        }),
        _ => None,
    })
}
