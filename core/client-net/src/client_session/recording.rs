//! Recording values and byte parsing, independent of storage and packet transport.
//!
//! Hosts supply raw recording text; fixture loaders may additionally resolve corpus names.
//! Enter-world choices are read from whole client blobs in their recorded order.

use std::net::SocketAddr;

/// One recorded datagram, exactly as the proxy wrote it down.
#[derive(Debug, Clone, PartialEq)]
pub struct Datagram {
    /// Seconds from the start of the recording.
    ///
    /// **Not a clock to feed at.** A replay feeds at the client's own local time, because the login
    /// state machine's give-up timer is measured against the time a datagram arrives at, and a
    /// datagram fed at its recorded `t` arrives from the client's own future.
    pub t: f64,
    /// True when the client sent it, false when the shard did.
    pub c2s: bool,
    /// Which address pair it belongs to: the logon server is 0 and each world server that followed
    /// is the next number up.
    pub pair: u16,
    /// The datagram itself.
    pub raw: Vec<u8>,
}

impl Datagram {
    /// Where a datagram of this pair arrives from. See [`peer`].
    #[must_use]
    pub fn peer(&self) -> SocketAddr {
        peer(self.pair)
    }
}

/// The address a recorded pair's datagrams arrive from, and the address a replay endpoint sends to.
///
/// Pair 0 is the logon server and each world server the recording moved on to is the next port up.
/// Nothing binds this socket -- it is a name for a direction, so that every replaying harness gives
/// the same recording the same addresses and a session's per-peer state lines up.
#[must_use]
pub fn peer(pair: u16) -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], 19_000 + pair))
}

/// A capture file that is missing, unreadable, or not shaped like one.
#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    #[error("{path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{path} line {line}: {what}")]
    Malformed {
        path: String,
        line: usize,
        what: String,
    },
    #[error("{path} records no datagram at all")]
    Empty { path: String },
}

/// Parse every datagram from recording text, in recorded order.
///
/// `name` labels errors; the caller owns reading any backing file.
///
/// # Errors
/// Returns [`CaptureError`] for empty input or malformed fields, preserving source line numbers.
pub fn parse(name: &str, text: &str) -> Result<Vec<Datagram>, CaptureError> {
    let name = name.to_owned();
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let bad = |what: &str| CaptureError::Malformed {
            path: name.clone(),
            line: i + 1,
            what: what.to_owned(),
        };
        let t: f64 = field(line, "\"t\"")
            .ok_or_else(|| bad("no t"))?
            .parse()
            .map_err(|_| bad("t is not a number"))?;
        let dir = field(line, "\"dir\"").ok_or_else(|| bad("no dir"))?;
        let pair: u16 = field(line, "\"pair\"")
            .ok_or_else(|| bad("no pair"))?
            .parse()
            .map_err(|_| bad("pair is not a number"))?;
        let data = field(line, "\"data\"").ok_or_else(|| bad("no data"))?;
        let raw = hex(data).ok_or_else(|| bad("data is not hex"))?;
        out.push(Datagram {
            t,
            c2s: dir == "c2s",
            pair,
            raw,
        });
    }
    if out.is_empty() {
        return Err(CaptureError::Empty { path: name });
    }
    Ok(out)
}

/// The value of one `"key"` on a one-object JSON line, as text.
fn field<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let at = line.find(key)?;
    let rest = line[at + key.len()..].trim_start_matches([' ', ':', '"']);
    let end = rest.find(['"', ',', '}'])?;
    Some(&rest[..end])
}

fn hex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    s.as_bytes()
        .chunks(2)
        .map(|c| u8::from_str_radix(std::str::from_utf8(c).ok()?, 16).ok())
        .collect()
}

use dereth_primitives::ObjectId;
use dereth_protocol::login::LoginSendEnterWorld;
use dereth_protocol::{Message, Opcode, Reader};

/// One enter-world the recorded client performed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedEntry {
    /// The caller's index of the client-to-server datagram that carried the `0xF7C8`.
    pub record: usize,
    /// The character the following `0xF657` named.
    pub character: ObjectId,
    /// The account the following `0xF657` named.
    pub account: String,
}

/// Every enter-world in a sequence of client-to-server blobs, in order.
///
/// `blobs` yields `(index, blob)` in capture order, each blob whole (its opcode dword and its body)
/// and each once; the index is whatever the caller uses to find its place again.
///
/// # Panics
/// On an `0xF7C8` with no `0xF657` after it before the next `0xF7C8` or the end, and on an
/// `0xF657` with no `0xF7C8` before it: either means the recording is not what this reads it as,
/// and a replay that guessed would be measuring the guess.
pub fn entries<'a>(blobs: impl IntoIterator<Item = (usize, &'a [u8])>) -> Vec<RecordedEntry> {
    let mut out = Vec::new();
    let mut pending: Option<usize> = None;
    for (record, blob) in blobs {
        let Some(head) = blob.get(..4) else {
            continue;
        };
        let opcode = u32::from_le_bytes([head[0], head[1], head[2], head[3]]);
        if opcode == Opcode::LOGIN_SEND_ENTER_WORLD_REQUEST.0 {
            assert!(
                pending.is_none(),
                "datagram {record}: a second `0xF7C8` before the first one's `0xF657`"
            );
            pending = Some(record);
        } else if opcode == Opcode::LOGIN_SEND_ENTER_WORLD.0 {
            let mut rd = Reader::new(&blob[4..]);
            let m = LoginSendEnterWorld::read(&mut rd).unwrap_or_else(|e| {
                panic!("datagram {record}: a `0xF657` that does not decode: {e:?}")
            });
            let ask = pending.take().unwrap_or_else(|| {
                panic!("datagram {record}: a `0xF657` with no `0xF7C8` before it")
            });
            out.push(RecordedEntry {
                record: ask,
                character: m.character,
                account: m.account,
            });
        }
    }
    assert!(
        pending.is_none(),
        "a trailing `0xF7C8` that no `0xF657` answered"
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Behaviour: none (recording text parsing preserves field values and direction handling).
    #[test]
    fn recording_text_preserves_fields_empty_payloads_and_direction() {
        let records = parse(
            "memory",
            "\n{\"data\":\"00fF\",\"pair\":2,\"dir\":\"c2s\",\"t\":1.25}\n\n{\"t\":0.5,\"dir\":\"other\",\"pair\":0,\"data\":\"\"}\n",
        )
        .expect("recording text");
        assert_eq!(
            records,
            vec![
                Datagram {
                    t: 1.25,
                    c2s: true,
                    pair: 2,
                    raw: vec![0, 255]
                },
                Datagram {
                    t: 0.5,
                    c2s: false,
                    pair: 0,
                    raw: vec![]
                },
            ]
        );
    }

    /// Behaviour: none (recording parse errors retain their input label, line and field diagnosis).
    #[test]
    fn recording_text_errors_keep_source_lines_and_field_diagnostics() {
        assert_eq!(
            parse("memory", "\n \n").unwrap_err().to_string(),
            "memory records no datagram at all"
        );
        for (line, expected) in [
            ("{}", "no t"),
            (r#"{"t":"bad"}"#, "t is not a number"),
            (r#"{"t":1}"#, "no dir"),
            (r#"{"t":1,"dir":"c2s"}"#, "no pair"),
            (
                r#"{"t":1,"dir":"c2s","pair":65536}"#,
                "pair is not a number",
            ),
            (r#"{"t":1,"dir":"c2s","pair":0}"#, "no data"),
            (
                r#"{"t":1,"dir":"c2s","pair":0,"data":"a"}"#,
                "data is not hex",
            ),
            (
                r#"{"t":1,"dir":"c2s","pair":0,"data":"gg"}"#,
                "data is not hex",
            ),
        ] {
            let error = parse("memory", &format!("\n{line}")).unwrap_err();
            assert_eq!(error.to_string(), format!("memory line 2: {expected}"));
        }
    }
}
