//! The decoded-fields form: a message's `Debug` text turned into JSON that SQLite's `json_*`
//! functions can query.
//!
//! `dereth-protocol` gives every message a derived `Debug` and no serialisation, so this reads the
//! derived format back:
//!
//! | `Debug` | JSON |
//! |---|---|
//! | `Name { a: 1, b: "x" }` | `{"_t": "Name", "a": 1, "b": "x"}` (`_t` left out at the top level) |
//! | `Some(v)`, `ObjectId(0x50000001)` and the other id newtypes | the inner value, unwrapped |
//! | `Name(v)` (any other one-field tuple) | the inner value at the top level, `{"Name": v}` below it |
//! | `Name(a, b)` | `{"Name": [a, b]}` |
//! | `Name` (a unit variant or struct) | `"Name"` |
//! | `None` | `null` |
//! | `[a, b]`, `(a, b)` | arrays |
//! | `{k: v}` (a map) | an object with the key's text as its name; `{a, b}` (a set) is an array |
//! | `0x1F`, `-3`, `1.5`, `NaN`, `inf` | numbers; NaN and the infinities become strings |
//!
//! Every `ObjectId` met on the way is also returned with its path (`$.object`,
//! `$.contents[3].object`), which is what the index's `message_guid` table is built from. A type
//! whose `Debug` is hand-written and does not follow the derived shape is kept as a string of its
//! text rather than failing the whole message.

use serde_json::{Map, Number, Value};

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

/// The result: the JSON value and every object id with its path.
#[derive(Debug, Clone, PartialEq)]
pub struct Fields {
    pub json: Value,
    pub guids: Vec<(u32, String)>,
}

/// Convert a derived `Debug` string. `None` when the text is not in the derived shape at all.
#[must_use]
pub fn convert(debug: &str) -> Option<Fields> {
    let mut p = Parser {
        s: debug.as_bytes(),
        i: 0,
        guids: Vec::new(),
    };
    let v = p.value("$", true)?;
    p.ws();
    if p.i != p.s.len() {
        return None;
    }
    Some(Fields {
        json: v,
        guids: p.guids,
    })
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
    guids: Vec<(u32, String)>,
}

impl Parser<'_> {
    fn ws(&mut self) {
        while self.i < self.s.len() && self.s[self.i].is_ascii_whitespace() {
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

    fn value(&mut self, path: &str, top: bool) -> Option<Value> {
        match self.peek()? {
            b'"' => self.string().map(Value::String),
            b'\'' => self.char_lit().map(Value::String),
            b'[' => {
                self.i += 1;
                self.seq(b']', path).map(Value::Array)
            }
            b'(' => {
                self.i += 1;
                self.seq(b')', path).map(Value::Array)
            }
            b'{' => {
                self.i += 1;
                self.map_or_set(path)
            }
            c if c == b'-' || c.is_ascii_digit() => self.number(),
            c if c.is_ascii_alphabetic() || c == b'_' => self.named(path, top),
            _ => None,
        }
    }

    fn seq(&mut self, close: u8, path: &str) -> Option<Vec<Value>> {
        let mut v = Vec::new();
        if self.eat(close) {
            return Some(v);
        }
        loop {
            let p = format!("{path}[{}]", v.len());
            v.push(self.value(&p, false)?);
            if self.eat(b',') {
                if self.eat(close) {
                    return Some(v);
                }
                continue;
            }
            if self.eat(close) {
                return Some(v);
            }
            return None;
        }
    }

    fn map_or_set(&mut self, path: &str) -> Option<Value> {
        if self.eat(b'}') {
            return Some(Value::Object(Map::new()));
        }
        let mut map = Map::new();
        let mut set = Vec::new();
        let mut is_map = None;
        loop {
            let key_start = self.guids.len();
            let k = self.value(&format!("{path}{{key}}"), false)?;
            let kind_map = self.eat(b':');
            match is_map {
                None => is_map = Some(kind_map),
                Some(m) if m != kind_map => return None,
                _ => {}
            }
            if kind_map {
                let name = match &k {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                // An object id used as a key is indexed under the entry's path.
                for g in &mut self.guids[key_start..] {
                    g.1 = format!("{path}.{name}#key");
                }
                let v = self.value(&format!("{path}.{name}"), false)?;
                map.insert(name, v);
            } else {
                set.push(k);
            }
            if self.eat(b',') {
                if self.eat(b'}') {
                    break;
                }
                continue;
            }
            if self.eat(b'}') {
                break;
            }
            return None;
        }
        Some(if is_map == Some(true) {
            Value::Object(map)
        } else {
            Value::Array(set)
        })
    }

    fn ident(&mut self) -> String {
        self.ws();
        let st = self.i;
        while self.i < self.s.len() {
            let c = self.s[self.i];
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

    fn named(&mut self, path: &str, top: bool) -> Option<Value> {
        let name = self.ident();
        match name.as_str() {
            "true" => return Some(Value::Bool(true)),
            "false" => return Some(Value::Bool(false)),
            "None" => return Some(Value::Null),
            "NaN" | "inf" => return Some(Value::String(name)),
            _ => {}
        }
        let short = name.rsplit("::").next().unwrap_or(&name).to_string();
        match self.peek() {
            Some(b'{') => {
                self.i += 1;
                let mut obj = Map::new();
                if !top {
                    obj.insert("_t".into(), Value::String(short));
                }
                if self.eat(b'}') {
                    return Some(Value::Object(obj));
                }
                loop {
                    if self.s[self.i..].starts_with(b"..") {
                        self.i += 2;
                        if self.eat(b'}') {
                            return Some(Value::Object(obj));
                        }
                        return None;
                    }
                    let field = self.ident();
                    if field.is_empty() || !self.eat(b':') {
                        return None;
                    }
                    let v = self.value(&format!("{path}.{field}"), false)?;
                    obj.insert(field, v);
                    if self.eat(b',') {
                        if self.eat(b'}') {
                            return Some(Value::Object(obj));
                        }
                        continue;
                    }
                    if self.eat(b'}') {
                        return Some(Value::Object(obj));
                    }
                    return None;
                }
            }
            Some(b'(') => {
                self.i += 1;
                let save = (self.i, self.guids.len());
                let items = if short == "Opcode" {
                    None
                } else {
                    self.seq(b')', path)
                };
                let Some(mut items) = items else {
                    // A hand-written Debug inside parentheses: keep its text, and its leading
                    // number when it has one.
                    self.i = save.0;
                    self.guids.truncate(save.1);
                    let text = self.raw_until_close()?;
                    let lead = text.split_whitespace().next().and_then(parse_number);
                    return Some(match lead {
                        Some(n) if UNWRAP.contains(&short.as_str()) => n,
                        _ => Value::String(format!("{short}({text})")),
                    });
                };
                if short == "ObjectId" {
                    if let Some(g) = items
                        .first()
                        .and_then(Value::as_u64)
                        .and_then(|g| u32::try_from(g).ok())
                    {
                        self.guids.push((g, path.to_string()));
                    }
                }
                // The items were parsed under `path[i]`; give the ids inside them the path they
                // have in the JSON this produces.
                let single = items.len() == 1;
                let unwrap = single && (top || UNWRAP.contains(&short.as_str()));
                let (from, to) = if unwrap {
                    (format!("{path}[0]"), path.to_string())
                } else if single {
                    (format!("{path}[0]"), format!("{path}.{short}"))
                } else {
                    (path.to_string(), format!("{path}.{short}"))
                };
                for g in &mut self.guids[save.1..] {
                    if let Some(rest) = g.1.strip_prefix(&from) {
                        g.1 = format!("{to}{rest}");
                    }
                }
                if unwrap {
                    return items.pop();
                }
                let inner = if single {
                    items.pop()?
                } else {
                    Value::Array(items)
                };
                let mut obj = Map::new();
                obj.insert(short, inner);
                Some(Value::Object(obj))
            }
            _ => Some(Value::String(short)),
        }
    }

    /// The text up to the matching `)`, consuming it.
    fn raw_until_close(&mut self) -> Option<String> {
        let st = self.i;
        let mut depth = 1usize;
        let mut in_str = false;
        while self.i < self.s.len() {
            let c = self.s[self.i];
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
        while self.i < self.s.len() {
            let c = self.s[self.i];
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
            return Some(Value::String(text.into()));
        }
        parse_number(text)
    }

    fn string(&mut self) -> Option<String> {
        self.i += 1;
        let mut out = String::new();
        let mut buf = Vec::new();
        while self.i < self.s.len() {
            let c = self.s[self.i];
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
        while self.i < self.s.len() {
            let c = self.s[self.i];
            if c == b'\'' {
                self.i += 1;
                return Some(out);
            }
            if c == b'\\' {
                self.i += 1;
                out.push(self.escape()?);
            } else {
                let rest = std::str::from_utf8(&self.s[self.i..]).ok()?;
                let ch = rest.chars().next()?;
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

fn parse_number(t: &str) -> Option<Value> {
    let t = t.replace('_', "");
    if let Some(h) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        return u64::from_str_radix(h, 16)
            .ok()
            .map(|v| Value::Number(v.into()));
    }
    if let Ok(v) = t.parse::<i64>() {
        return Some(Value::Number(v.into()));
    }
    if let Ok(v) = t.parse::<u64>() {
        return Some(Value::Number(v.into()));
    }
    let f: f64 = t.parse().ok()?;
    Some(Number::from_f64(f).map_or_else(|| Value::String(t.clone()), Value::Number))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[derive(Debug)]
    #[allow(dead_code)]
    struct Inner {
        object: dereth_primitives::ObjectId,
        name: String,
    }

    #[derive(Debug)]
    #[allow(dead_code)]
    enum E {
        Unit,
        Tup(u8, i16),
        Stru { x: f32 },
    }

    #[derive(Debug)]
    #[allow(dead_code)]
    struct Top {
        a: u32,
        list: Vec<Inner>,
        opt: Option<u8>,
        none: Option<u8>,
        e: Vec<E>,
        map: std::collections::BTreeMap<u32, bool>,
        t: (u8, char),
        f: f64,
        nan: f32,
        s: &'static str,
    }

    #[derive(Debug)]
    #[allow(dead_code)]
    struct Newtype(Inner);

    #[test]
    fn a_derived_debug_becomes_json_and_its_object_ids_are_found() {
        let v = Top {
            a: 7,
            list: vec![
                Inner {
                    object: dereth_primitives::ObjectId(0x5000_0001),
                    name: "Tusker \"Bob\"\n".into(),
                },
                Inner {
                    object: dereth_primitives::ObjectId(0x8000_0002),
                    name: "é".into(),
                },
            ],
            opt: Some(3),
            none: None,
            e: vec![E::Unit, E::Tup(1, -2), E::Stru { x: 1.5 }],
            map: [(1, true), (2, false)].into_iter().collect(),
            t: (9, 'q'),
            f: -1.25e-7,
            nan: f32::NAN,
            s: "a, b",
        };
        let f = convert(&format!("{v:?}")).expect("converts");
        assert_eq!(
            f.json,
            json!({
                "a": 7,
                "list": [
                    {"_t": "Inner", "object": 0x5000_0001u32, "name": "Tusker \"Bob\"\n"},
                    {"_t": "Inner", "object": 0x8000_0002u32, "name": "é"},
                ],
                "opt": 3,
                "none": null,
                "e": ["Unit", {"Tup": [1, -2]}, {"_t": "Stru", "x": 1.5}],
                "map": {"1": true, "2": false},
                "t": [9, "q"],
                "f": -1.25e-7,
                "nan": "NaN",
                "s": "a, b",
            })
        );
        assert_eq!(
            f.guids,
            vec![
                (0x5000_0001, "$.list[0].object".into()),
                (0x8000_0002, "$.list[1].object".into())
            ]
        );
    }

    #[test]
    fn a_top_level_newtype_is_unwrapped_and_hand_written_debug_is_kept_as_text() {
        let n = Newtype(Inner {
            object: dereth_primitives::ObjectId(1),
            name: String::new(),
        });
        let f = convert(&format!("{n:?}")).unwrap();
        assert_eq!(f.json, json!({"_t": "Inner", "object": 1, "name": ""}));
        let op = dereth_protocol::Opcode(0xF7B0);
        let f = convert(&format!("X {{ op: {op:?}, other: Weird(a b c) }}")).unwrap();
        assert_eq!(f.json["op"], json!(0xF7B0));
        assert_eq!(f.json["other"], json!("Weird(a b c)"));
    }

    #[derive(Debug)]
    #[allow(dead_code)]
    struct Holder {
        pair: Pair,
        one: One,
    }

    #[derive(Debug)]
    #[allow(dead_code)]
    struct Pair(dereth_primitives::ObjectId, dereth_primitives::ObjectId);

    #[derive(Debug)]
    #[allow(dead_code)]
    struct One(Inner);

    #[test]
    fn object_id_paths_follow_the_json_through_tuples() {
        let top = Newtype(Inner {
            object: dereth_primitives::ObjectId(5),
            name: String::new(),
        });
        assert_eq!(
            convert(&format!("{top:?}")).unwrap().guids,
            vec![(5, "$.object".into())]
        );
        let h = Holder {
            pair: Pair(
                dereth_primitives::ObjectId(1),
                dereth_primitives::ObjectId(2),
            ),
            one: One(Inner {
                object: dereth_primitives::ObjectId(3),
                name: String::new(),
            }),
        };
        let f = convert(&format!("{h:?}")).unwrap();
        assert_eq!(f.json["pair"]["Pair"], json!([1, 2]));
        assert_eq!(f.json["one"]["One"]["object"], json!(3));
        assert_eq!(
            f.guids,
            vec![
                (1, "$.pair.Pair[0]".into()),
                (2, "$.pair.Pair[1]".into()),
                (3, "$.one.One.object".into())
            ]
        );
    }

    #[test]
    fn not_debug_text_is_refused() {
        assert!(convert("{ unbalanced").is_none());
        assert!(convert("A { b: 1 } trailing").is_none());
    }
}
