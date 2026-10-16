//! The vectors: the address, text-rule, message, challenge, directory, reach, say, rooms and web
//! address examples every implementation must agree on.
//!
//! The JSON files in `vectors/` are the data, readable from any language; this module loads them
//! and runs them against this repository's own protocol crate, which is how a Rust implementation
//! proves it agrees. An implementation in another language reads the same files.

use serde_json::Value;

use dereth_network_protocol::challenge::{nonce_hex, parse_nonce_hex, Challenge};
use dereth_network_protocol::say::without_mark;
use dereth_network_protocol::text::{sanitize_line, sanitize_name, MAX_LINE_CHARS};
use dereth_network_protocol::wire::WireError;
use dereth_network_protocol::DirectoryMessage;
use dereth_network_protocol::{Address, Message, ReachMessage, RoomsMessage, SayMessage};

use crate::Check;

/// `vectors/addresses.json`.
pub const ADDRESSES: &str = include_str!("../vectors/addresses.json");
/// `vectors/text.json`.
pub const TEXT: &str = include_str!("../vectors/text.json");
/// `vectors/messages.json`.
pub const MESSAGES: &str = include_str!("../vectors/messages.json");
/// `vectors/challenge.json`.
pub const CHALLENGE: &str = include_str!("../vectors/challenge.json");
/// `vectors/directory.json`.
pub const DIRECTORY: &str = include_str!("../vectors/directory.json");
/// `vectors/reach.json`.
pub const REACH: &str = include_str!("../vectors/reach.json");
/// `vectors/say.json`.
pub const SAY: &str = include_str!("../vectors/say.json");
/// `vectors/rooms.json`.
pub const ROOMS: &str = include_str!("../vectors/rooms.json");
/// `vectors/web.json`.
pub const WEB: &str = include_str!("../vectors/web.json");

fn parse(json: &str) -> Value {
    serde_json::from_str(json).expect("the vector files are JSON")
}

fn s(v: &Value) -> &str {
    v.as_str().unwrap_or("")
}

/// A text vector's string: `field` as it is, or `<field>_parts` joined, each piece repeated its
/// number of times.
fn text_of(case: &Value, field: &str) -> String {
    if let Some(parts) = case[format!("{field}_parts")].as_array() {
        return parts
            .iter()
            .map(|p| {
                let n = p[1]
                    .as_u64()
                    .and_then(|n| usize::try_from(n).ok())
                    .unwrap_or(0);
                s(&p[0]).repeat(n)
            })
            .collect();
    }
    s(&case[field]).to_owned()
}

/// A frame with every `${c*N}` written out as the character c, N times.
#[must_use]
pub fn expand(frame: &str) -> String {
    let mut out = String::with_capacity(frame.len());
    let mut rest = frame;
    while let Some(at) = rest.find("${") {
        out.push_str(&rest[..at]);
        let after = &rest[at + 2..];
        let Some(end) = after.find('}') else {
            out.push_str(&rest[at..]);
            return out;
        };
        let spec = &after[..end];
        match spec.split_once('*') {
            Some((c, n)) if c.chars().count() == 1 => {
                out.push_str(&c.repeat(n.parse().unwrap_or(0)));
            }
            _ => out.push_str(&rest[at..at + 3 + end]),
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

/// Whether `written` holds every field of `vector`, with the same value (objects field by field,
/// so `written` may hold more).
fn covers(written: &Value, vector: &Value) -> bool {
    match (written, vector) {
        (Value::Object(w), Value::Object(v)) => v
            .iter()
            .all(|(k, x)| w.get(k).is_some_and(|y| covers(y, x))),
        (Value::Number(a), Value::Number(b)) => a.as_f64() == b.as_f64(),
        (a, b) => a == b,
    }
}

/// The value of one field of a message, by what the field means: read from the message itself,
/// never from a writing of it.
fn field(m: &Message, name: &str) -> Option<Value> {
    use serde_json::json;
    Some(match (m, name) {
        (Message::Welcome { world, .. }, "world") => json!(world),
        (Message::Welcome { worlds, .. }, "worlds") => json!(worlds.len()),
        (Message::Welcome { limits, .. }, "hold_secs") => json!(limits.hold_secs),
        (Message::Welcome { limits, .. }, "result_timeout_secs") => {
            json!(limits.result_timeout_secs)
        }
        (Message::Welcome { limits, .. }, "tells_per_character.burst") => {
            json!(limits.tells_per_character.burst)
        }
        (Message::Watch { owner, .. }, "owner") => json!(owner.to_string()),
        (Message::Watch { target, .. }, "target") => json!(target.to_string()),
        (Message::Watch { on, .. }, "on") => json!(on),
        (Message::Presence { to, .. }, "to") => json!(to.to_string()),
        (Message::Presence { of, .. }, "of") => json!(of.to_string()),
        (Message::Presence { online, .. }, "online") => json!(online),
        (Message::Tell { id, .. }, "id") => json!(id),
        (Message::Tell { from, .. }, "from") => json!(from.to_string()),
        (Message::Tell { to, .. }, "to") => json!(to.to_string()),
        (Message::Tell { text, .. }, "text") => json!(text),
        (Message::Tell { friend, .. }, "friend") => json!(friend),
        (Message::TellResult { id, .. }, "id") => json!(id),
        (Message::TellResult { outcome, .. }, "outcome") => serde_json::to_value(outcome).ok()?,
        (Message::TellResult { message, .. }, "message") => json!(message),
        (Message::RoomSay { id, .. }, "id") => json!(id),
        (Message::RoomSay { room, .. } | Message::RoomLine { room, .. }, "room") => json!(room),
        (Message::RoomSay { from, .. } | Message::RoomLine { from, .. }, "from") => {
            json!(from.to_string())
        }
        (Message::RoomSay { text, .. } | Message::RoomLine { text, .. }, "text") => json!(text),
        (Message::RoomSay { via, .. } | Message::RoomLine { via, .. }, "via") => json!(via),
        (Message::ChannelLine { channel, .. }, "channel") => json!(channel),
        (Message::ChannelLine { from, .. }, "from") => json!(from.to_string()),
        (Message::ChannelLine { to, .. }, "to") => json!(to.to_string()),
        (Message::ChannelLine { text, .. }, "text") => json!(text),
        (Message::Person { id, .. }, "id") => json!(id),
        (Message::Person { op, .. }, "op") => serde_json::to_value(op).ok()?,
        (Message::Person { who, .. }, "who") => json!(who.to_string()),
        (Message::Person { target, .. }, "target") => {
            json!(target.as_ref().map(ToString::to_string))
        }
        (Message::Person { targets, .. }, "targets") => {
            json!(targets.iter().map(ToString::to_string).collect::<Vec<_>>())
        }
        (Message::Person { shown, .. }, "shown") => json!(shown),
        (Message::PersonResult { id, .. }, "id") => json!(id),
        (Message::PersonResult { outcome, .. }, "outcome") => serde_json::to_value(outcome).ok()?,
        (Message::PersonResult { code, .. }, "code") => json!(code),
        (Message::PersonResult { expires_secs, .. }, "expires_secs") => json!(expires_secs),
        (Message::PersonResult { url, .. }, "url") => json!(url),
        (Message::PersonResult { characters, .. }, "characters") => {
            json!(characters
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>())
        }
        (Message::PersonWatch { who, .. }, "who") => json!(who.to_string()),
        (Message::PersonWatch { on, .. }, "on") => json!(on),
        (Message::PersonFriends { to, .. }, "to") => json!(to.to_string()),
        (Message::PersonFriends { linked, .. }, "linked") => json!(linked),
        (Message::PersonFriends { alts, .. }, "alts") => serde_json::to_value(alts).ok()?,
        (Message::PersonFriends { persons, .. }, "persons") => json!(persons
            .iter()
            .map(|p| json!({
                "handle": p.handle.to_string(),
                "characters": p.characters.iter().map(ToString::to_string).collect::<Vec<_>>(),
            }))
            .collect::<Vec<_>>()),
        _ => return None,
    })
}

/// Every address vector, against `dereth-network-protocol`.
#[must_use]
pub fn addresses() -> Vec<Check> {
    let v = parse(ADDRESSES);
    let mut out = Vec::new();
    for case in v["ok"].as_array().into_iter().flatten() {
        let input = s(&case["input"]);
        let result = match Address::parse(input) {
            Ok(a) => {
                let got = (a.name(), a.world(), a.key(), a.is_channel());
                let want = (
                    s(&case["name"]),
                    s(&case["world"]),
                    s(&case["key"]).to_owned(),
                    case["channel"].as_bool().unwrap_or(false),
                );
                if (got.0, got.1, got.2.as_str(), got.3)
                    == (want.0, want.1, want.2.as_str(), want.3)
                {
                    Ok(())
                } else {
                    Err(format!("{input:?}: got {got:?}, want {want:?}"))
                }
            }
            Err(e) => Err(format!("{input:?} refused: {e}")),
        };
        out.push(Check::new("addresses", "addresses.parse", result));
    }
    for case in v["refused"].as_array().into_iter().flatten() {
        let input = s(case);
        let result = match Address::parse(input) {
            Ok(a) => Err(format!("{input:?} accepted as {a}")),
            Err(_) => Ok(()),
        };
        out.push(Check::new("addresses", "addresses.refuse", result));
    }
    for slug in v["reserved_slugs"].as_array().into_iter().flatten() {
        let slug = s(slug);
        let result = if dereth_network_protocol::address::is_network_slug(slug) {
            Err(format!("{slug} is not reserved"))
        } else {
            Ok(())
        };
        out.push(Check::new("addresses", "addresses.reserved", result));
    }
    out
}

/// Every text-rule vector, against `dereth-network-protocol`.
#[must_use]
pub fn text() -> Vec<Check> {
    let v = parse(TEXT);
    let mut out = Vec::new();
    for case in v["lines"].as_array().into_iter().flatten() {
        let (input, want) = match s(&case["input"]) {
            "LONG" => ("x".repeat(300), "x".repeat(MAX_LINE_CHARS)),
            _ => (text_of(case, "input"), text_of(case, "output")),
        };
        let got = sanitize_line(&input);
        let once = sanitize_line(&got);
        let result = if got != want {
            Err(format!("{input:?}: got {got:?}, want {want:?}"))
        } else if once != got {
            Err(format!("{input:?}: not idempotent ({once:?})"))
        } else {
            Ok(())
        };
        out.push(Check::new("text rules", "text.line", result));
    }
    for case in v["names"].as_array().into_iter().flatten() {
        let input = s(&case["input"]);
        let got = sanitize_name(input);
        let result = match (case["refused"].as_bool().unwrap_or(false), got) {
            (true, Err(_)) => Ok(()),
            (true, Ok(n)) => Err(format!("{input:?} should be refused, got {n:?}")),
            (false, Ok(n)) if n == s(&case["output"]) => match sanitize_name(&n) {
                Ok(again) if again == n => Ok(()),
                other => Err(format!("{input:?}: not idempotent ({other:?})")),
            },
            (false, other) => Err(format!("{input:?}: got {other:?}")),
        };
        out.push(Check::new("text rules", "text.name", result));
    }
    let max = v["max_line_chars"].as_u64();
    out.push(Check::new(
        "text rules",
        "text.max_line_chars",
        if max == Some(MAX_LINE_CHARS as u64) {
            Ok(())
        } else {
            Err(format!("the vectors say {max:?}"))
        },
    ));
    out
}

/// Every message vector, against `dereth-network-protocol`.
#[must_use]
pub fn messages() -> Vec<Check> {
    let v = parse(MESSAGES);
    let mut out = Vec::new();
    for case in v["ok"].as_array().into_iter().flatten() {
        let frame = expand(s(&case["frame"]));
        let frame = frame.as_str();
        let result = match Message::from_json(frame) {
            Ok(m) if m.kind() == s(&case["type"]) => ok_message(case, frame, &m),
            Ok(m) => Err(format!("{frame}: read as {}", m.kind())),
            Err(e) => Err(format!("{frame}: {e}")),
        };
        out.push(Check::new("link messages", "messages.ok", result));
    }
    for case in v["unknown"].as_array().into_iter().flatten() {
        let frame = expand(s(case));
        let frame = frame.as_str();
        let result = match Message::from_json(frame) {
            Err(WireError::Unknown(_)) => Ok(()),
            other => Err(format!("{frame}: {other:?}, not skipped as unknown")),
        };
        out.push(Check::new("link messages", "messages.unknown", result));
    }
    for case in v["malformed"].as_array().into_iter().flatten() {
        let frame = expand(s(case));
        let frame = frame.as_str();
        let result = match Message::from_json(frame) {
            Err(WireError::Malformed(_)) => Ok(()),
            other => Err(format!("{frame}: {other:?}, not refused as malformed")),
        };
        out.push(Check::new("link messages", "messages.malformed", result));
    }
    out
}

/// An `ok` frame, read as `m`: written again it covers the frame, it reads back the same, and it
/// has the values the case expects.
fn ok_message(case: &Value, frame: &str, m: &Message) -> Result<(), String> {
    let written = m.to_json();
    let (Ok(w), Ok(f)) = (
        serde_json::from_str::<Value>(&written),
        serde_json::from_str::<Value>(frame),
    ) else {
        return Err(format!("{frame}: written as {written}, which is not JSON"));
    };
    if !covers(&w, &f) {
        return Err(format!(
            "{frame}: written as {written}, which leaves out or changes a field"
        ));
    }
    match Message::from_json(&written) {
        Ok(again) if again == *m => {}
        other => {
            return Err(format!(
                "{frame}: does not survive a write and a read: {other:?}"
            ))
        }
    }
    for (name, want) in case["expect"].as_object().into_iter().flatten() {
        let got = field(m, name);
        let same = match (&got, want) {
            (Some(Value::Number(a)), Value::Number(b)) => a.as_f64() == b.as_f64(),
            (Some(g), w) => g == w,
            (None, _) => false,
        };
        if !same {
            return Err(format!("{frame}: {name} is {got:?}, not {want}"));
        }
    }
    Ok(())
}

/// A datagram's bytes from its hex; none for text that is not hex.
fn unhex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(text.get(i..i + 2)?, 16).ok())
        .collect()
}

/// Every challenge vector, against `dereth-network-protocol`.
#[must_use]
pub fn challenge() -> Vec<Check> {
    let v = parse(CHALLENGE);
    let mut out = Vec::new();
    let reads = |case: &Value| -> Result<Vec<u8>, String> {
        let hex = s(&case["datagram"]);
        let bytes = unhex(hex).ok_or_else(|| format!("{hex}: not hex"))?;
        let want = (s(&case["slug"]), parse_nonce_hex(s(&case["nonce"])));
        match Challenge::parse(&bytes) {
            Some(c) if (c.slug.as_str(), Some(c.nonce)) == want => Ok(bytes),
            Some(c) => Err(format!(
                "{hex}: read as {} {}, not {} {}",
                c.slug,
                nonce_hex(&c.nonce),
                want.0,
                s(&case["nonce"])
            )),
            None => Err(format!("{hex}: refused")),
        }
    };
    for case in v["ok"].as_array().into_iter().flatten() {
        let result = reads(case).and_then(|bytes| {
            let written = parse_nonce_hex(s(&case["nonce"])).and_then(|nonce| {
                Challenge {
                    slug: s(&case["slug"]).to_owned(),
                    nonce,
                }
                .encode()
            });
            if written.as_deref() == Some(bytes.as_slice()) {
                Ok(())
            } else {
                Err(format!(
                    "{}: written again as {:?}",
                    s(&case["datagram"]),
                    written.map(|w| w.iter().map(|b| format!("{b:02x}")).collect::<String>())
                ))
            }
        });
        out.push(Check::new("challenge", "challenge.ok", result));
    }
    for case in v["later"].as_array().into_iter().flatten() {
        out.push(Check::new(
            "challenge",
            "challenge.later",
            reads(case).map(|_| ()),
        ));
    }
    for case in v["refused"].as_array().into_iter().flatten() {
        let hex = s(&case["datagram"]);
        let result = match unhex(hex).as_deref().map(Challenge::parse) {
            Some(None) => Ok(()),
            Some(Some(c)) => Err(format!("{hex} ({}): read as {}", s(&case["why"]), c.slug)),
            None => Err(format!("{hex}: not hex")),
        };
        out.push(Check::new("challenge", "challenge.refused", result));
    }
    out
}

/// The value of one field of a directory message, by what the field means; a profile's fields
/// and its allegiance's by their path (`allegiance.patron`), and a list's by its length.
fn directory_field(m: &DirectoryMessage, name: &str) -> Option<Value> {
    use serde_json::json;
    Some(match (m, name) {
        (DirectoryMessage::Listing { id, .. }, "id") => json!(id),
        (DirectoryMessage::Listing { who, .. }, "who") => json!(who.to_string()),
        (DirectoryMessage::Listing { on, .. }, "on") => json!(on),
        (DirectoryMessage::Listing { profile: None, .. }, "profile") => Value::Null,
        (
            DirectoryMessage::Listing {
                profile: Some(p), ..
            },
            field,
        ) => match field {
            "level" => json!(p.level),
            "heritage" => json!(p.heritage),
            "gender" => json!(p.gender),
            "title" => json!(p.title),
            "skills" => json!(p.skills.len()),
            "augmentations" => json!(p.augmentations.len()),
            "deaths" => json!(p.deaths),
            "birth" => json!(p.birth),
            "attributes.self.base" => json!(p.attributes.map(|a| a.self_.base)),
            "allegiance.name" => json!(p.allegiance.as_ref().and_then(|a| a.name.clone())),
            "allegiance.patron" => json!(p
                .allegiance
                .as_ref()
                .and_then(|a| a.patron.as_ref())
                .map(ToString::to_string)),
            _ => return None,
        },
        (DirectoryMessage::ListingResult { id, .. }, "id") => json!(id),
        (DirectoryMessage::ListingResult { outcome, .. }, "outcome") => {
            serde_json::to_value(outcome).ok()?
        }
        (DirectoryMessage::ListingResult { reason, .. }, "reason") => {
            serde_json::to_value(reason).ok()?
        }
        (DirectoryMessage::ListingResult { url, .. }, "url") => json!(url),
        (DirectoryMessage::ListingOff { who, .. }, "who") => json!(who.to_string()),
        (DirectoryMessage::ListingOff { by, .. }, "by") => serde_json::to_value(by).ok()?,
        _ => return None,
    })
}

/// Every directory vector, against `dereth-network-protocol`: the frames that must read as their
/// type, write back covering every field and read back unchanged, with the values `expect` gives;
/// a later version's frames that must read, ignoring what they add; the link messages a reader of
/// the directory alone skips; and the frames that must be refused.
#[must_use]
pub fn directory() -> Vec<Check> {
    let v = parse(DIRECTORY);
    let mut out = Vec::new();
    for case in v["ok"].as_array().into_iter().flatten() {
        let frame = expand(s(&case["frame"]));
        let result = match DirectoryMessage::from_json(&frame) {
            Ok(m) if m.kind() == s(&case["type"]) => ok_directory(case, &frame, &m),
            Ok(m) => Err(format!("{frame}: read as {}", m.kind())),
            Err(e) => Err(format!("{frame}: {e}")),
        };
        out.push(Check::new("directory", "directory.ok", result));
    }
    for case in v["later"].as_array().into_iter().flatten() {
        let frame = expand(s(case));
        let result = match DirectoryMessage::from_json(&frame) {
            Ok(m) => {
                let written = m.to_json();
                if written.len() < frame.len() {
                    Ok(())
                } else {
                    Err(format!("{frame}: kept what it does not know: {written}"))
                }
            }
            Err(e) => Err(format!("{frame}: {e}")),
        };
        out.push(Check::new("directory", "directory.later", result));
    }
    for case in v["unknown"].as_array().into_iter().flatten() {
        let frame = expand(s(case));
        let result = match DirectoryMessage::from_json(&frame) {
            Err(WireError::Unknown(_)) => Ok(()),
            other => Err(format!("{frame}: {other:?}, not skipped as unknown")),
        };
        out.push(Check::new("directory", "directory.unknown", result));
    }
    for case in v["malformed"].as_array().into_iter().flatten() {
        let frame = expand(s(case));
        let result = match DirectoryMessage::from_json(&frame) {
            Err(WireError::Malformed(_)) => Ok(()),
            other => Err(format!("{frame}: {other:?}, not refused as malformed")),
        };
        out.push(Check::new("directory", "directory.malformed", result));
    }
    out
}

/// An `ok` directory frame, read as `m`: as [`ok_message`] holds a link message to.
fn ok_directory(case: &Value, frame: &str, m: &DirectoryMessage) -> Result<(), String> {
    let written = m.to_json();
    let (Ok(w), Ok(f)) = (
        serde_json::from_str::<Value>(&written),
        serde_json::from_str::<Value>(frame),
    ) else {
        return Err(format!("{frame}: written as {written}, which is not JSON"));
    };
    if !covers(&w, &f) {
        return Err(format!(
            "{frame}: written as {written}, which leaves out or changes a field"
        ));
    }
    match DirectoryMessage::from_json(&written) {
        Ok(again) if again == *m => {}
        other => {
            return Err(format!(
                "{frame}: does not survive a write and a read: {other:?}"
            ))
        }
    }
    for (name, want) in case["expect"].as_object().into_iter().flatten() {
        let got = directory_field(m, name);
        let same = match (&got, want) {
            (Some(Value::Number(a)), Value::Number(b)) => a.as_f64() == b.as_f64(),
            (Some(g), w) => g == w,
            (None, _) => false,
        };
        if !same {
            return Err(format!("{frame}: {name} is {got:?}, not {want}"));
        }
    }
    Ok(())
}

/// The value of one field of a reach message, by what the field means.
fn reach_field(m: &ReachMessage, name: &str) -> Option<Value> {
    use serde_json::json;
    Some(match (m, name) {
        (ReachMessage::Ask { id, .. } | ReachMessage::Result { id, .. }, "id") => json!(id),
        (ReachMessage::Ask { from, .. }, "from") => json!(from.to_string()),
        (ReachMessage::Ask { to, .. }, "to") => json!(to.to_string()),
        (ReachMessage::Result { reachable, .. }, "reachable") => json!(reachable),
        _ => return None,
    })
}

/// Every reach vector, against `dereth-network-protocol`: the frames that must read as their type,
/// write back covering every field and read back unchanged, with the values `expect` gives; a
/// later version's frames that must read, ignoring what they add; the other link messages a reader
/// of the reach messages alone skips; and the frames that must be refused.
#[must_use]
pub fn reach() -> Vec<Check> {
    let v = parse(REACH);
    let mut out = Vec::new();
    for case in v["ok"].as_array().into_iter().flatten() {
        let frame = expand(s(&case["frame"]));
        let result = match ReachMessage::from_json(&frame) {
            Ok(m) if m.kind() == s(&case["type"]) => ok_reach(case, &frame, &m),
            Ok(m) => Err(format!("{frame}: read as {}", m.kind())),
            Err(e) => Err(format!("{frame}: {e}")),
        };
        out.push(Check::new("reach", "reach.ok", result));
    }
    for case in v["later"].as_array().into_iter().flatten() {
        let frame = expand(s(case));
        let result = match ReachMessage::from_json(&frame) {
            Ok(m) => {
                let written = m.to_json();
                if written.len() < frame.len() {
                    Ok(())
                } else {
                    Err(format!("{frame}: kept what it does not know: {written}"))
                }
            }
            Err(e) => Err(format!("{frame}: {e}")),
        };
        out.push(Check::new("reach", "reach.later", result));
    }
    for case in v["unknown"].as_array().into_iter().flatten() {
        let frame = expand(s(case));
        let result = match ReachMessage::from_json(&frame) {
            Err(WireError::Unknown(_)) => Ok(()),
            other => Err(format!("{frame}: {other:?}, not skipped as unknown")),
        };
        out.push(Check::new("reach", "reach.unknown", result));
    }
    for case in v["malformed"].as_array().into_iter().flatten() {
        let frame = expand(s(case));
        let result = match ReachMessage::from_json(&frame) {
            Err(WireError::Malformed(_)) => Ok(()),
            other => Err(format!("{frame}: {other:?}, not refused as malformed")),
        };
        out.push(Check::new("reach", "reach.malformed", result));
    }
    out
}

/// An `ok` reach frame, read as `m`: as [`ok_message`] holds a link message to.
fn ok_reach(case: &Value, frame: &str, m: &ReachMessage) -> Result<(), String> {
    let written = m.to_json();
    let (Ok(w), Ok(f)) = (
        serde_json::from_str::<Value>(&written),
        serde_json::from_str::<Value>(frame),
    ) else {
        return Err(format!("{frame}: written as {written}, which is not JSON"));
    };
    if !covers(&w, &f) {
        return Err(format!(
            "{frame}: written as {written}, which leaves out or changes a field"
        ));
    }
    match ReachMessage::from_json(&written) {
        Ok(again) if again == *m => {}
        other => {
            return Err(format!(
                "{frame}: does not survive a write and a read: {other:?}"
            ))
        }
    }
    for (name, want) in case["expect"].as_object().into_iter().flatten() {
        let got = reach_field(m, name);
        if got.as_ref() != Some(want) {
            return Err(format!("{frame}: {name} is {got:?}, not {want}"));
        }
    }
    Ok(())
}

/// The value of one field of a say message, by what the field means.
fn say_field(m: &SayMessage, name: &str) -> Option<Value> {
    use serde_json::json;
    Some(match (m, name) {
        (SayMessage::Ask { id, .. } | SayMessage::Result { id, .. }, "id") => json!(id),
        (SayMessage::Ask { room, .. }, "room") => json!(room),
        (SayMessage::Ask { who, .. }, "who") => json!(who.to_string()),
        (SayMessage::Ask { text, .. }, "text") => json!(text),
        (SayMessage::Ask { via, .. }, "via") => json!(via),
        (SayMessage::Result { outcome, .. }, "outcome") => serde_json::to_value(outcome).ok()?,
        (SayMessage::Result { reason, .. }, "reason") => json!(reason),
        _ => return None,
    })
}

/// Every say vector, against `dereth-network-protocol`, as [`reach`] runs the reach messages':
/// the frames that must read as their type, write back covering every field and read back
/// unchanged, with the values `expect` gives; a later version's frames that must read, ignoring
/// what they add; the other link messages a reader of the say messages alone skips; the frames
/// that must be refused; and the names a player gives with the mark a line said from Discord
/// shows, each naming the character without it.
#[must_use]
pub fn say() -> Vec<Check> {
    let v = parse(SAY);
    let mut out = Vec::new();
    for case in v["ok"].as_array().into_iter().flatten() {
        let frame = expand(s(&case["frame"]));
        let result = match SayMessage::from_json(&frame) {
            Ok(m) if m.kind() == s(&case["type"]) => ok_say(case, &frame, &m),
            Ok(m) => Err(format!("{frame}: read as {}", m.kind())),
            Err(e) => Err(format!("{frame}: {e}")),
        };
        out.push(Check::new("say", "say.ok", result));
    }
    for case in v["later"].as_array().into_iter().flatten() {
        let frame = expand(s(case));
        let result = match SayMessage::from_json(&frame) {
            Ok(m) => {
                let written = m.to_json();
                if written.len() < frame.len() {
                    Ok(())
                } else {
                    Err(format!("{frame}: kept what it does not know: {written}"))
                }
            }
            Err(e) => Err(format!("{frame}: {e}")),
        };
        out.push(Check::new("say", "say.later", result));
    }
    for case in v["unknown"].as_array().into_iter().flatten() {
        let frame = expand(s(case));
        let result = match SayMessage::from_json(&frame) {
            Err(WireError::Unknown(_)) => Ok(()),
            other => Err(format!("{frame}: {other:?}, not skipped as unknown")),
        };
        out.push(Check::new("say", "say.unknown", result));
    }
    for case in v["malformed"].as_array().into_iter().flatten() {
        let frame = expand(s(case));
        let result = match SayMessage::from_json(&frame) {
            Err(WireError::Malformed(_)) => Ok(()),
            other => Err(format!("{frame}: {other:?}, not refused as malformed")),
        };
        out.push(Check::new("say", "say.malformed", result));
    }
    for case in v["names"].as_array().into_iter().flatten() {
        let (given, want) = (s(&case["given"]), s(&case["names"]));
        let got = without_mark(given);
        let result = if got == want {
            Ok(())
        } else {
            Err(format!("{given:?} names {got:?}, not {want:?}"))
        };
        out.push(Check::new("say", "say.names", result));
    }
    out
}

/// The value of one field of a rooms message, by what the field means.
fn rooms_field(m: &RoomsMessage, name: &str) -> Option<Value> {
    use serde_json::json;
    Some(match (m, name) {
        (RoomsMessage::Setup { id, .. }, "id") => json!(id),
        (RoomsMessage::Setup { global, .. }, "global") => json!(global),
        (RoomsMessage::Setup { mirrored, .. }, "mirrored") => json!(mirrored),
        (RoomsMessage::Global { id, .. }, "id") => json!(id),
        (RoomsMessage::Global { room, .. }, "room") => json!(room),
        (RoomsMessage::Global { seed, .. }, "seed") => json!(seed),
        _ => return None,
    })
}

/// Every rooms vector, against `dereth-network-protocol`, as [`say`] runs the say messages': the
/// frames that must read as their type, write back covering every field and read back unchanged,
/// with the values `expect` gives; a later version's frames that must read, ignoring what they
/// add; the other link messages a reader of the rooms messages alone skips; and the frames that
/// must be refused.
#[must_use]
pub fn rooms() -> Vec<Check> {
    let v = parse(ROOMS);
    let mut out = Vec::new();
    for case in v["ok"].as_array().into_iter().flatten() {
        let frame = expand(s(&case["frame"]));
        let result = match RoomsMessage::from_json(&frame) {
            Ok(m) if m.kind() == s(&case["type"]) => ok_rooms(case, &frame, &m),
            Ok(m) => Err(format!("{frame}: read as {}", m.kind())),
            Err(e) => Err(format!("{frame}: {e}")),
        };
        out.push(Check::new("rooms", "rooms.ok", result));
    }
    for case in v["later"].as_array().into_iter().flatten() {
        let frame = expand(s(case));
        let result = match RoomsMessage::from_json(&frame) {
            Ok(m) => {
                let written = m.to_json();
                if written.len() < frame.len() {
                    Ok(())
                } else {
                    Err(format!("{frame}: kept what it does not know: {written}"))
                }
            }
            Err(e) => Err(format!("{frame}: {e}")),
        };
        out.push(Check::new("rooms", "rooms.later", result));
    }
    for case in v["unknown"].as_array().into_iter().flatten() {
        let frame = expand(s(case));
        let result = match RoomsMessage::from_json(&frame) {
            Err(WireError::Unknown(_)) => Ok(()),
            other => Err(format!("{frame}: {other:?}, not skipped as unknown")),
        };
        out.push(Check::new("rooms", "rooms.unknown", result));
    }
    for case in v["malformed"].as_array().into_iter().flatten() {
        let frame = expand(s(case));
        let result = match RoomsMessage::from_json(&frame) {
            Err(WireError::Malformed(_)) => Ok(()),
            other => Err(format!("{frame}: {other:?}, not refused as malformed")),
        };
        out.push(Check::new("rooms", "rooms.malformed", result));
    }
    out
}

/// An `ok` rooms frame, read as `m`: as [`ok_message`] holds a link message to.
fn ok_rooms(case: &Value, frame: &str, m: &RoomsMessage) -> Result<(), String> {
    let written = m.to_json();
    let (Ok(w), Ok(f)) = (
        serde_json::from_str::<Value>(&written),
        serde_json::from_str::<Value>(frame),
    ) else {
        return Err(format!("{frame}: written as {written}, which is not JSON"));
    };
    if !covers(&w, &f) {
        return Err(format!(
            "{frame}: written as {written}, which leaves out or changes a field"
        ));
    }
    match RoomsMessage::from_json(&written) {
        Ok(again) if again == *m => {}
        other => {
            return Err(format!(
                "{frame}: does not survive a write and a read: {other:?}"
            ))
        }
    }
    for (name, want) in case["expect"].as_object().into_iter().flatten() {
        let got = rooms_field(m, name);
        if got.as_ref() != Some(want) {
            return Err(format!("{frame}: {name} is {got:?}, not {want}"));
        }
    }
    Ok(())
}

/// An `ok` say frame, read as `m`: as [`ok_message`] holds a link message to.
fn ok_say(case: &Value, frame: &str, m: &SayMessage) -> Result<(), String> {
    let written = m.to_json();
    let (Ok(w), Ok(f)) = (
        serde_json::from_str::<Value>(&written),
        serde_json::from_str::<Value>(frame),
    ) else {
        return Err(format!("{frame}: written as {written}, which is not JSON"));
    };
    if !covers(&w, &f) {
        return Err(format!(
            "{frame}: written as {written}, which leaves out or changes a field"
        ));
    }
    match SayMessage::from_json(&written) {
        Ok(again) if again == *m => {}
        other => {
            return Err(format!(
                "{frame}: does not survive a write and a read: {other:?}"
            ))
        }
    }
    for (name, want) in case["expect"].as_object().into_iter().flatten() {
        let got = say_field(m, name);
        if got.as_ref() != Some(want) {
            return Err(format!("{frame}: {name} is {got:?}, not {want}"));
        }
    }
    Ok(())
}

/// All eight sets.
#[must_use]
pub fn all() -> Vec<Check> {
    let mut v = addresses();
    v.extend(text());
    v.extend(messages());
    v.extend(challenge());
    v.extend(directory());
    v.extend(reach());
    v.extend(say());
    v.extend(rooms());
    v.extend(web());
    v
}

/// Every web address vector, against `dereth-network-protocol`: each `hello` reads with the
/// addresses it names, and each URL is kept or refused by its rule.
#[must_use]
pub fn web() -> Vec<Check> {
    use dereth_network_protocol::web::{is_status_url, is_websocket_url};
    let v = parse(WEB);
    let mut out = Vec::new();
    for case in v["hello"].as_array().into_iter().flatten() {
        let frame = expand(s(&case["frame"]));
        let want = |field: &str| case["expect"][field].as_str().map(str::to_owned);
        let result = match Message::from_json(&frame) {
            Ok(Message::Hello {
                websocket_url,
                status_url,
                ..
            }) => {
                let got = (websocket_url, status_url);
                let want = (want("websocket_url"), want("status_url"));
                (got == want)
                    .then_some(())
                    .ok_or_else(|| format!("{frame}: got {got:?}, want {want:?}"))
            }
            other => Err(format!("{frame}: read as {other:?}")),
        };
        out.push(Check::new("web addresses", "web.hello", result));
    }
    type Rule = fn(&str) -> bool;
    let rules: [(&str, Rule); 2] = [("websocket", is_websocket_url), ("status", is_status_url)];
    for (kind, rule) in rules {
        for (verdict, keep) in [("kept", true), ("refused", false)] {
            for url in v[kind][verdict].as_array().into_iter().flatten() {
                let url = expand(s(url));
                let result = (rule(&url) == keep)
                    .then_some(())
                    .ok_or_else(|| format!("{kind} {url:?} should be {verdict}"));
                let name = if keep { "web.keep" } else { "web.refuse" };
                out.push(Check::new("web addresses", name, result));
            }
        }
    }
    out
}
