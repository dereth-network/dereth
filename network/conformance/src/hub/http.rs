//! The hub's public HTTP routes, read as a page on another origin reads them: a `GET` without a
//! cookie, at the address the links use, and its answer.

use std::io::{Read as _, Write as _};
use std::net::TcpStream;
use std::time::{Duration, Instant};

use serde_json::Value;

/// How long one request may take, from connecting to the last byte of its answer.
const WAIT: Duration = Duration::from_secs(5);

/// One answer: its status, its headers and its body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answer {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Answer {
    /// The first header named `name`, without regard to case.
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// The body, read as JSON.
    ///
    /// # Errors
    /// When it is not JSON.
    pub fn json(&self) -> Result<Value, String> {
        serde_json::from_str(&self.body).map_err(|e| format!("{}: {e}: {}", self.status, self.body))
    }
}

/// `s` percent-encoded as one segment of a path, or one value of a query: every byte but the
/// unreserved ones.
#[must_use]
pub fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
                char::from(b).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

/// `GET` of `path` on the hub whose links are at `base` (`ws://host:port`, perhaps with a path the
/// hub is served under).
///
/// # Errors
/// When the hub cannot be reached, or its answer does not read as HTTP.
pub fn get(base: &str, path: &str) -> Result<Answer, String> {
    let rest = base
        .strip_prefix("ws://")
        .ok_or_else(|| format!("{base}: the suite speaks ws:// only"))?;
    let (authority, prefix) = match rest.find('/') {
        Some(i) => (&rest[..i], rest[i..].trim_end_matches('/')),
        None => (rest, ""),
    };
    let target = format!("{prefix}{path}");
    let mut tcp = TcpStream::connect(authority).map_err(|e| format!("{target}: {e}"))?;
    tcp.set_read_timeout(Some(Duration::from_millis(100)))
        .map_err(|e| e.to_string())?;
    tcp.write_all(
        format!(
            "GET {target} HTTP/1.1\r\nHost: {authority}\r\nAccept: application/json\r\n\
             Connection: close\r\n\r\n"
        )
        .as_bytes(),
    )
    .map_err(|e| format!("{target}: {e}"))?;
    let until = Instant::now() + WAIT;
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 16 * 1024];
    while !complete(&bytes) {
        match tcp.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => bytes.extend_from_slice(&chunk[..n]),
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(e) => return Err(format!("{target}: {e}")),
        }
        if Instant::now() >= until {
            return Err(format!("{target}: no whole answer within {WAIT:?}"));
        }
    }
    parse(&bytes).ok_or_else(|| format!("{target}: an answer that does not read as HTTP"))
}

/// Where the head ends, and the body starts.
fn head_end(bytes: &[u8]) -> Option<usize> {
    bytes
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|i| i + 4)
}

/// The head's fields: the status and the headers.
fn head(bytes: &[u8]) -> Option<(u16, Vec<(String, String)>)> {
    let text = std::str::from_utf8(bytes).ok()?;
    let mut lines = text.split("\r\n");
    let status = lines.next()?.split(' ').nth(1)?.parse().ok()?;
    let headers = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_owned(), v.trim().to_owned()))
        .collect();
    Some((status, headers))
}

fn length_of(headers: &[(String, String)]) -> Option<usize> {
    headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, v)| v.parse().ok())
}

fn chunked(headers: &[(String, String)]) -> bool {
    headers.iter().any(|(k, v)| {
        k.eq_ignore_ascii_case("transfer-encoding") && v.to_ascii_lowercase().contains("chunked")
    })
}

/// Whether `bytes` holds a whole answer whose end its head names; one that names none ends when
/// the hub closes the connection.
fn complete(bytes: &[u8]) -> bool {
    let Some(end) = head_end(bytes) else {
        return false;
    };
    let Some((_, headers)) = head(&bytes[..end]) else {
        return true;
    };
    if chunked(&headers) {
        return bytes[end..].windows(5).any(|w| w == b"0\r\n\r\n");
    }
    length_of(&headers).is_some_and(|n| bytes.len() - end >= n)
}

fn parse(bytes: &[u8]) -> Option<Answer> {
    let end = head_end(bytes)?;
    let (status, headers) = head(&bytes[..end])?;
    let rest = &bytes[end..];
    let body = if chunked(&headers) {
        unchunk(rest)?
    } else {
        match length_of(&headers) {
            Some(n) => rest.get(..n)?.to_vec(),
            None => rest.to_vec(),
        }
    };
    Some(Answer {
        status,
        headers,
        body: String::from_utf8(body).ok()?,
    })
}

/// A chunked body, joined.
fn unchunk(mut rest: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    loop {
        let line = rest.windows(2).position(|w| w == b"\r\n")?;
        let size = std::str::from_utf8(&rest[..line]).ok()?;
        let size = usize::from_str_radix(size.split(';').next()?.trim(), 16).ok()?;
        rest = &rest[line + 2..];
        if size == 0 {
            return Some(out);
        }
        out.extend_from_slice(rest.get(..size)?);
        rest = rest.get(size + 2..)?;
    }
}

#[cfg(test)]
mod tests {
    use super::{encode, parse};

    /// Spec: conformance: the suite reads a hub's answer with its length or sent in chunks alike.
    #[test]
    fn an_answer_reads_with_its_length_or_in_chunks() {
        let sized = parse(b"HTTP/1.1 404 Not Found\r\nContent-Length: 2\r\nX-A: b\r\n\r\n{}tail")
            .expect("reads");
        assert_eq!(sized.status, 404);
        assert_eq!(sized.body, "{}");
        assert_eq!(sized.header("x-a"), Some("b"));
        let chunked = parse(
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\n{\"a\r\n4\r\n\":1}\r\n0\r\n\r\n",
        )
        .expect("reads");
        assert_eq!(chunked.json().expect("JSON")["a"], 1);
    }

    /// Spec: conformance: the suite puts an address in a route's path with its space and its
    /// separator encoded.
    #[test]
    fn an_address_is_encoded_as_one_segment() {
        assert_eq!(encode("Sir Bob@aa"), "Sir%20Bob%40aa");
        assert_eq!(encode("bo<b"), "bo%3Cb");
    }
}
