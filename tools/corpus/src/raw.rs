//! Reading and rewriting one recording, without reformatting it.
//!
//! A recording is one JSON object per line, `{t, dir, pair, len, data}`, written by
//! `dereth-corpus record`. The scrubber substitutes **inside** `data` and changes nothing else,
//! so the rewriter keeps each original line verbatim and swaps only the hex run between the quotes
//! of the `data` value. Every substitution is the same length as what it replaces, so the hex run
//! is the same length too and the rewritten file differs from the original only in the bytes that
//! carried an identity.
//!
//! That is deliberate: a rewriter that re-serialised the JSON would make the diff between the
//! private recording and the public one unreadable, and the one property the scrubber has to be able
//! to demonstrate is *exactly which bytes changed*.

use std::path::Path;

/// One recorded datagram.
#[derive(Debug, Clone)]
pub struct Datagram {
    /// Index within the recording, from zero.
    pub idx: usize,
    /// `c2s` or `s2c`.
    pub dir: Dir,
    /// The datagram bytes: the 20-byte transport header followed by its payload.
    pub bytes: Vec<u8>,
    /// The original line, with `hex_at` naming the byte range of the `data` value's hex run.
    line: String,
    hex_at: std::ops::Range<usize>,
}

/// Which way a datagram went.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Dir {
    /// Client to server.
    C2s,
    /// Server to client.
    S2c,
}

impl Dir {
    /// The token the recording uses.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::C2s => "c2s",
            Self::S2c => "s2c",
        }
    }
}

/// A whole recording, in file order.
#[derive(Debug)]
pub struct Recording {
    /// The recording's name, i.e. the file stem.
    pub name: String,
    /// Its datagrams, in the order the proxy logged them.
    pub datagrams: Vec<Datagram>,
}

/// Why a recording could not be read or rewritten.
#[derive(Debug)]
pub enum RawError {
    /// The file could not be read or written.
    Io(String, std::io::Error),
    /// A line is not the shape this reader understands.
    Line(String, usize, String),
}

impl std::fmt::Display for RawError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(p, e) => write!(f, "{p}: {e}"),
            Self::Line(p, n, what) => write!(f, "{p} line {n}: {what}"),
        }
    }
}

impl std::error::Error for RawError {}

impl Recording {
    /// Read `<path>`, whose stem is the recording's name.
    ///
    /// # Errors
    /// [`RawError::Io`] when the file cannot be read, [`RawError::Line`] when a line is not a
    /// `{t, dir, pair, len, data}` object with an even-length hex `data` value at least as long as
    /// the 20-byte transport header.
    pub fn read(path: &Path) -> Result<Self, RawError> {
        let shown = path.display().to_string();
        let text = std::fs::read_to_string(path).map_err(|e| RawError::Io(shown.clone(), e))?;
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unnamed")
            .to_owned();
        let mut datagrams = Vec::new();
        for (n, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let hex_at = hex_span(line)
                .ok_or_else(|| RawError::Line(shown.clone(), n + 1, "no `data` value".into()))?;
            let hex = &line[hex_at.clone()];
            let bytes = decode_hex(hex).ok_or_else(|| {
                RawError::Line(shown.clone(), n + 1, "the `data` value is not hex".into())
            })?;
            if bytes.len() < dereth_transport::wire::HEADER_SIZE {
                return Err(RawError::Line(
                    shown.clone(),
                    n + 1,
                    format!("{} bytes, shorter than the 20-byte header", bytes.len()),
                ));
            }
            let dir = match dir_of(line) {
                Some("c2s") => Dir::C2s,
                Some("s2c") => Dir::S2c,
                other => {
                    return Err(RawError::Line(
                        shown.clone(),
                        n + 1,
                        format!("dir {other:?}"),
                    ))
                }
            };
            datagrams.push(Datagram {
                idx: datagrams.len(),
                dir,
                bytes,
                line: line.to_owned(),
                hex_at,
            });
        }
        if datagrams.is_empty() {
            return Err(RawError::Line(shown, 0, "the recording is empty".into()));
        }
        Ok(Self { name, datagrams })
    }

    /// Write the recording back out, each line verbatim except for its `data` hex.
    ///
    /// # Errors
    /// [`RawError::Io`] when the file cannot be written.
    pub fn write(&self, path: &Path) -> Result<(), RawError> {
        let mut out = String::new();
        for dg in &self.datagrams {
            let mut line = dg.line.clone();
            line.replace_range(dg.hex_at.clone(), &encode_hex(&dg.bytes));
            out.push_str(&line);
            // LF, always: these files are text fixtures and the repository's rule for a new file
            // is LF. The originals are LF too.
            out.push('\n');
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| RawError::Io(parent.display().to_string(), e))?;
        }
        std::fs::write(path, out).map_err(|e| RawError::Io(path.display().to_string(), e))
    }
}

/// The byte range of the string value of `"<key>"` in one line.
///
/// The key is only a key when a colon follows it, which is not pedantry: a line carrying
/// `"note": "data"` would otherwise have its *note* rewritten as if it were the payload.
fn string_value_span(line: &str, key: &str) -> Option<std::ops::Range<usize>> {
    let quoted = format!("\"{key}\"");
    let mut from = 0usize;
    while let Some(rel) = line[from..].find(&quoted) {
        let after = from + rel + quoted.len();
        let rest = line[after..].trim_start();
        if rest.starts_with(':') {
            let colon = line.len() - rest.len();
            let open = line[colon + 1..].find('"')? + colon + 2;
            let close = line[open..].find('"')? + open;
            return Some(open..close);
        }
        from = after;
    }
    None
}

/// The byte range of the hex run inside the `"data": "…"` value of one line.
fn hex_span(line: &str) -> Option<std::ops::Range<usize>> {
    string_value_span(line, "data")
}

/// The `dir` value of one line.
fn dir_of(line: &str) -> Option<&str> {
    Some(&line[string_value_span(line, "dir")?])
}

/// Decode an even-length lower- or upper-case hex string.
#[must_use]
pub fn decode_hex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    s.as_bytes()
        .chunks(2)
        .map(|c| u8::from_str_radix(std::str::from_utf8(c).ok()?, 16).ok())
        .collect()
}

/// Encode bytes as lower-case hex, the form the recordings use.
#[must_use]
pub fn encode_hex(b: &[u8]) -> String {
    let mut out = String::with_capacity(b.len() * 2);
    for x in b {
        out.push_str(&format!("{x:02x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rewriter is a byte-level substitution on one field, so a line with unusual spacing,
    /// an unexpected extra key or a different key order survives it unchanged.
    #[test]
    fn only_the_data_hex_is_rewritten() {
        let line = "{\"t\": 1.5, \"dir\":\"s2c\", \"pair\": 0, \"note\": \"data\", \
                    \"len\": 2, \"data\" : \"00ff\"}";
        let span = hex_span(line).expect("data span");
        assert_eq!(&line[span.clone()], "00ff");
        assert_eq!(dir_of(line), Some("s2c"));
        let mut rewritten = line.to_owned();
        rewritten.replace_range(span, "11ee");
        assert_eq!(
            rewritten,
            "{\"t\": 1.5, \"dir\":\"s2c\", \"pair\": 0, \"note\": \"data\", \
             \"len\": 2, \"data\" : \"11ee\"}"
        );
    }

    /// Hex round-trips in the case the recordings are written in.
    #[test]
    fn hex_round_trips_in_lower_case() {
        let bytes = vec![0x00, 0x0f, 0xf0, 0xff];
        assert_eq!(encode_hex(&bytes), "000ff0ff");
        assert_eq!(decode_hex("000FF0ff").as_deref(), Some(bytes.as_slice()));
        assert_eq!(decode_hex("abc"), None);
    }
}
