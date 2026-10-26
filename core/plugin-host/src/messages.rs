//! The read-only message stream: each message the client received or sent this frame, decoded by
//! the protocol crate's codecs, as one event a plugin with the `messages` permission can hear.
//!
//! An event is named `messages.<name>`, the message's name in the master opcode table
//! (`messages.Item_SetAppraiseInfo`); `messages.any` hears every one. Its data is `{ name, opcode,
//! dir, recipient, codec, fields }`: `dir` is `received` or `sent`, `codec` the decoder that read it
//! (`module::Type`, or nil when no decoder reads that type that way), and `fields` the decoded
//! message, its field names the decoder's own.
//!
//! The decoders give every message a derived `Debug` text and nothing else, so `fields` is that
//! text read back: `Name { a: 1 }` is a table with `a` (and `_t`, the type's name, below the top
//! level), `Some(v)` and the identifier newtypes are their value, `None` is nil, a list or a tuple
//! is a list, a map is a table keyed by the key's text, a one-value tuple of any other name is
//! `{ Name = v }`, and a unit variant is its name. A type whose `Debug` is written by hand is kept
//! as its text.
//!
//! The stream is versioned on its own ([`dereth_plugin_api::event::MESSAGES_VERSION`]): its field
//! names follow the decoders, not the API. Nothing here sends: there is no raw send.

use std::collections::BTreeMap;

use dereth_client_runtime::frame_hook::WireMessage;
use dereth_plugin_api::{Event, Value};
use dereth_protocol::codecs::{read_blob, Dir};
use dereth_protocol::Opcode;

/// The prefix of a message event's name.
pub const PREFIX: &str = "messages.";

/// The event that hears every message.
pub const ANY: &str = "messages.any";

/// The name a message of type `mtype` is heard by: its name in the master opcode table, or
/// `0x` and its type in hexadecimal for a type the table does not have.
#[must_use]
pub fn name_of(mtype: u32) -> String {
    Opcode(mtype)
        .info()
        .map_or_else(|| format!("0x{mtype:04X}"), |i| i.name.to_owned())
}

/// Whether `event` (after `messages.`) names a message a plugin can hear: `any`, a name in the
/// master opcode table, or a type in hexadecimal.
#[must_use]
pub fn is_message_event(event: &str) -> bool {
    let Some(rest) = event.strip_prefix(PREFIX) else {
        return false;
    };
    rest == "any"
        || rest
            .strip_prefix("0x")
            .is_some_and(|h| !h.is_empty() && u32::from_str_radix(h, 16).is_ok())
        || dereth_protocol::OPCODES.iter().any(|o| o.name == rest)
}

/// The event for one message, or `None` for a blob too short to have a type.
#[must_use]
pub fn event_of(m: &WireMessage) -> Option<Event> {
    let read = read_blob(m.dir, &m.blob)?;
    let name = name_of(read.mtype);
    let (codec, fields) = match &read.fields {
        Some((codec, debug)) => (
            Value::from(*codec),
            fields_of(debug).unwrap_or_else(|| Value::from(debug.as_str())),
        ),
        None => (Value::Nil, Value::Nil),
    };
    Some(Event::new(
        &format!("{PREFIX}{name}"),
        Value::map()
            .with("name", name.as_str())
            .with("opcode", read.mtype)
            .with(
                "dir",
                match m.dir {
                    Dir::S2c => "received",
                    Dir::C2s => "sent",
                },
            )
            .with("recipient", read.recipient)
            .with("codec", codec)
            .with("fields", fields),
    ))
}

/// Newtypes whose single field is the value itself.
const UNWRAP: &[&str] = &[
    "Some",
    "ObjectId",
    "DataId",
    "CellId",
    "PropertyId",
    "LandblockId",
    "Opcode",
];

/// A derived `Debug` text read back as a value; `None` when it is not in the derived shape.
#[must_use]
pub fn fields_of(debug: &str) -> Option<Value> {
    let mut p = Parser {
        s: debug.as_bytes(),
        i: 0,
    };
    let v = p.value(true)?;
    p.ws();
    (p.i == p.s.len()).then_some(v)
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn ws(&mut self) {
        while self.s.get(self.i).is_some_and(u8::is_ascii_whitespace) {
            self.i += 1;
        }
    }

    fn peek(&mut self) -> Option<u8> {
        self.ws();
        self.s.get(self.i).copied()
    }

    fn eat(&mut self, c: u8) -> bool {
        if self.peek() == Some(c) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn value(&mut self, top: bool) -> Option<Value> {
        match self.peek()? {
            b'"' => self.string().map(Value::Str),
            b'\'' => self.char_lit().map(Value::Str),
            b'[' => {
                self.i += 1;
                self.seq(b']').map(Value::List)
            }
            b'(' => {
                self.i += 1;
                self.seq(b')').map(Value::List)
            }
            b'{' => {
                self.i += 1;
                self.map_or_set()
            }
            c if c == b'-' || c.is_ascii_digit() => self.number(),
            c if c.is_ascii_alphabetic() || c == b'_' => self.named(top),
            _ => None,
        }
    }

    fn seq(&mut self, close: u8) -> Option<Vec<Value>> {
        let mut v = Vec::new();
        if self.eat(close) {
            return Some(v);
        }
        loop {
            v.push(self.value(false)?);
            let comma = self.eat(b',');
            if self.eat(close) {
                return Some(v);
            }
            if !comma {
                return None;
            }
        }
    }

    fn map_or_set(&mut self) -> Option<Value> {
        if self.eat(b'}') {
            return Some(Value::Map(BTreeMap::new()));
        }
        let mut map = BTreeMap::new();
        let mut set = Vec::new();
        let mut is_map = None;
        loop {
            let k = self.value(false)?;
            let kind_map = self.eat(b':');
            if *is_map.get_or_insert(kind_map) != kind_map {
                return None;
            }
            if kind_map {
                let name = match &k {
                    Value::Str(s) => s.clone(),
                    other => other.to_json(),
                };
                map.insert(name, self.value(false)?);
            } else {
                set.push(k);
            }
            let comma = self.eat(b',');
            if self.eat(b'}') {
                break;
            }
            if !comma {
                return None;
            }
        }
        Some(if is_map == Some(true) {
            Value::Map(map)
        } else {
            Value::List(set)
        })
    }

    fn ident(&mut self) -> String {
        self.ws();
        let st = self.i;
        while let Some(&c) = self.s.get(self.i) {
            if c.is_ascii_alphanumeric() || c == b'_' {
                self.i += 1;
            } else if c == b':' && self.s.get(self.i + 1) == Some(&b':') {
                self.i += 2;
            } else {
                break;
            }
        }
        String::from_utf8_lossy(&self.s[st..self.i]).into_owned()
    }

    fn named(&mut self, top: bool) -> Option<Value> {
        let name = self.ident();
        match name.as_str() {
            "true" => return Some(Value::Bool(true)),
            "false" => return Some(Value::Bool(false)),
            "None" => return Some(Value::Nil),
            "NaN" | "inf" => return Some(Value::Str(name)),
            _ => {}
        }
        let short = name.rsplit("::").next().unwrap_or(&name).to_owned();
        match self.peek() {
            Some(b'{') => {
                self.i += 1;
                let mut obj = BTreeMap::new();
                if !top {
                    obj.insert("_t".to_owned(), Value::Str(short));
                }
                loop {
                    if self.eat(b'}') {
                        return Some(Value::Map(obj));
                    }
                    if self.s[self.i..].starts_with(b"..") {
                        self.i += 2;
                        continue;
                    }
                    let field = self.ident();
                    if field.is_empty() || !self.eat(b':') {
                        return None;
                    }
                    let v = self.value(false)?;
                    obj.insert(field, v);
                    let comma = self.eat(b',');
                    if !comma && self.peek() != Some(b'}') {
                        return None;
                    }
                }
            }
            Some(b'(') => {
                self.i += 1;
                let save = self.i;
                let items = if short == "Opcode" {
                    None
                } else {
                    self.seq(b')')
                };
                let Some(mut items) = items else {
                    // A hand-written Debug inside parentheses: its text, or its leading number for
                    // a newtype that is its value.
                    self.i = save;
                    let text = self.raw_until_close()?;
                    let lead = text.split_whitespace().next().and_then(parse_number);
                    return Some(match lead {
                        Some(n) if UNWRAP.contains(&short.as_str()) => n,
                        _ => Value::Str(format!("{short}({text})")),
                    });
                };
                let single = items.len() == 1;
                if single && (top || UNWRAP.contains(&short.as_str())) {
                    return items.pop();
                }
                let inner = if single {
                    items.pop()?
                } else {
                    Value::List(items)
                };
                Some(Value::map().with(&short, inner))
            }
            _ => Some(Value::Str(short)),
        }
    }

    /// The text up to the matching `)`, consuming it.
    fn raw_until_close(&mut self) -> Option<String> {
        let st = self.i;
        let mut depth = 1usize;
        let mut in_str = false;
        while let Some(&c) = self.s.get(self.i) {
            self.i += 1;
            if in_str {
                if c == b'\\' {
                    self.i += 1;
                } else if c == b'"' {
                    in_str = false;
                }
                continue;
            }
            match c {
                b'"' => in_str = true,
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(String::from_utf8_lossy(&self.s[st..self.i - 1]).into_owned());
                    }
                }
                _ => {}
            }
        }
        None
    }

    fn number(&mut self) -> Option<Value> {
        let st = self.i;
        while let Some(&c) = self.s.get(self.i) {
            let exp_sign = (c == b'-' || c == b'+')
                && self.i > st
                && matches!(self.s[self.i - 1], b'e' | b'E')
                && !self.s[st..self.i].starts_with(b"0x");
            if c.is_ascii_alphanumeric()
                || c == b'.'
                || c == b'_'
                || (c == b'-' && self.i == st)
                || exp_sign
            {
                self.i += 1;
            } else {
                break;
            }
        }
        let text = std::str::from_utf8(&self.s[st..self.i]).ok()?;
        if text == "-inf" {
            return Some(Value::Str(text.into()));
        }
        parse_number(text)
    }

    fn string(&mut self) -> Option<String> {
        self.i += 1;
        let mut out = String::new();
        let mut buf = Vec::new();
        while let Some(&c) = self.s.get(self.i) {
            self.i += 1;
            match c {
                b'"' => {
                    out.push_str(&String::from_utf8_lossy(&buf));
                    return Some(out);
                }
                b'\\' => {
                    out.push_str(&String::from_utf8_lossy(&buf));
                    buf.clear();
                    out.push(self.escape()?);
                }
                _ => buf.push(c),
            }
        }
        None
    }

    fn char_lit(&mut self) -> Option<String> {
        self.i += 1;
        let mut out = String::new();
        while let Some(&c) = self.s.get(self.i) {
            if c == b'\'' {
                self.i += 1;
                return Some(out);
            }
            if c == b'\\' {
                self.i += 1;
                out.push(self.escape()?);
            } else {
                let ch = std::str::from_utf8(&self.s[self.i..])
                    .ok()?
                    .chars()
                    .next()?;
                out.push(ch);
                self.i += ch.len_utf8();
            }
        }
        None
    }

    fn escape(&mut self) -> Option<char> {
        let c = *self.s.get(self.i)?;
        self.i += 1;
        Some(match c {
            b'n' => '\n',
            b'r' => '\r',
            b't' => '\t',
            b'0' => '\0',
            b'\\' => '\\',
            b'"' => '"',
            b'\'' => '\'',
            b'u' => {
                if self.s.get(self.i) != Some(&b'{') {
                    return None;
                }
                let st = self.i + 1;
                let end = st + self.s[st..].iter().position(|&b| b == b'}')?;
                let hex = std::str::from_utf8(&self.s[st..end]).ok()?;
                self.i = end + 1;
                char::from_u32(u32::from_str_radix(hex, 16).ok()?)?
            }
            _ => return None,
        })
    }
}

/// A number as `Debug` writes it: decimal or `0x` hexadecimal, an integer or a float; NaN and the
/// infinities as their text.
fn parse_number(text: &str) -> Option<Value> {
    let t = text.replace('_', "");
    if let Some(h) = t.strip_prefix("0x") {
        return u64::from_str_radix(h, 16)
            .ok()
            .and_then(|n| i64::try_from(n).ok())
            .map(Value::Int);
    }
    if let Ok(n) = t.parse::<i64>() {
        return Some(Value::Int(n));
    }
    let f: f64 = t.parse().ok()?;
    Some(if f.is_finite() {
        Value::Num(f)
    } else {
        Value::Str(t)
    })
}

#[cfg(test)]
mod tests {
    use super::{event_of, fields_of, is_message_event};
    use dereth_client_runtime::frame_hook::WireMessage;
    use dereth_plugin_api::Value;
    use dereth_primitives::ObjectId;

    #[test]
    fn a_derived_debug_text_reads_back_as_fields() {
        let v = fields_of(
            r#"Thing { id: ObjectId(0x50000001), name: "A \"b\"", list: [1, -2, 3.5], opt: Some(7), none: None, inner: Inner { x: 1 }, tag: Mode(3), unit: Peace, map: {1: "a"} }"#,
        )
        .expect("reads");
        assert_eq!(v.get("id").as_i64(), Some(0x5000_0001));
        assert_eq!(v.get("name").as_str(), Some("A \"b\""));
        assert_eq!(v.get("list").as_list().map(<[Value]>::len), Some(3));
        assert_eq!(v.get("opt").as_i64(), Some(7));
        assert!(v.get("none").is_nil());
        assert_eq!(v.get("inner").get("_t").as_str(), Some("Inner"));
        assert_eq!(v.get("tag").get("Mode").as_i64(), Some(3));
        assert_eq!(v.get("unit").as_str(), Some("Peace"));
        assert_eq!(v.get("map").get("1").as_str(), Some("a"));
        assert!(fields_of("Broken { a: ").is_none());
    }

    #[test]
    fn a_wrapped_event_is_named_by_its_own_type_and_decoded() {
        let m = dereth_protocol::objects::ItemServerSaysRemove {
            object: ObjectId(0x8000_1234),
        };
        let blob = dereth_protocol::events::pack_event(ObjectId(0x5000_0001), 7, &m).unwrap();
        let e = event_of(&WireMessage {
            dir: dereth_protocol::codecs::Dir::S2c,
            blob,
        })
        .expect("an event");
        assert_eq!(e.name, "messages.Item_ServerSaysRemove");
        assert_eq!(e.data.get("dir").as_str(), Some("received"));
        assert_eq!(e.data.get("opcode").as_i64(), Some(0x0024));
        assert_eq!(e.data.get("recipient").as_i64(), Some(0x5000_0001));
        assert_eq!(
            e.data.get("fields").get("object").as_i64(),
            Some(0x8000_1234)
        );
        let sent = dereth_protocol::actions::pack_action(
            3,
            &dereth_protocol::objects::ItemAppraise {
                target: ObjectId(0x8000_0042),
            },
        )
        .unwrap();
        let e = event_of(&WireMessage::sent(sent)).expect("an event");
        assert_eq!(e.name, "messages.Item_Appraise");
        assert_eq!(e.data.get("dir").as_str(), Some("sent"));
        assert_eq!(
            e.data.get("fields").get("target").as_i64(),
            Some(0x8000_0042)
        );
    }

    #[test]
    fn only_names_of_the_opcode_table_any_and_hex_types_are_message_events() {
        assert!(is_message_event("messages.any"));
        assert!(is_message_event("messages.Item_SetAppraiseInfo"));
        assert!(is_message_event("messages.0x02CD"));
        assert!(!is_message_event("messages.Item_Nothing"));
        assert!(!is_message_event("messages."));
        assert!(!is_message_event("world.object_created"));
    }
}
