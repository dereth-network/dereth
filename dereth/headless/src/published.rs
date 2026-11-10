//! What the world's server publishes, fetched from the address its status document is at
//! ([`Options::world_status`](crate::run::Options::world_status)): the status, then each document
//! it names (the server's portal table and its places), over plain HTTP. The reading is the
//! runtime's ([`PublishedReading`]); the fetch, an HTTP/1.0 `GET`, is this client's own.
//!
//! A server still gathering what it publishes answers `503` for a while after it starts; each
//! document is asked for again, a second apart, until it answers or [`TRIES`] have passed.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use dereth_client_sdk::runtime::travel::{Answered, Fetched, PublishedReading};

/// How many times a document answered `503` is asked for.
pub const TRIES: u32 = 30;

/// The documents the status at `status_url` names, each fetched: the texts of those that
/// answered, and a line for each that did not.
#[must_use]
pub fn fetch(status_url: &str) -> (Vec<String>, Vec<String>) {
    read(status_url, get, |seconds| {
        std::thread::sleep(Duration::from_secs_f64(seconds));
    })
}

/// [`fetch`], each address fetched by `fetch` and each `503` waited out by `wait` (given the
/// seconds).
fn read(
    status_url: &str,
    fetch: impl FnMut(&str) -> Fetched,
    wait: impl FnMut(f64),
) -> (Vec<String>, Vec<String>) {
    let mut texts = Vec::new();
    let mut failed = Vec::new();
    let Some(reading) = PublishedReading::new(status_url) else {
        return (texts, vec![format!("{status_url}: {ONLY_HTTP}")]);
    };
    reading
        .retrying(TRIES, 1.0)
        .read_all(fetch, wait, |answered| match answered {
            Answered::Document(text) => texts.push(text),
            Answered::StatusFailed(e) | Answered::DocumentFailed(e) => failed.push(e),
        });
    (texts, failed)
}

const ONLY_HTTP: &str = "only http:// addresses are fetched";

/// One `GET` of an `http://` URL, HTTP/1.0.
fn get(url: &str) -> Fetched {
    let failed = |e: &dyn std::fmt::Display| Fetched::Failed(e.to_string());
    let Some(rest) = url.strip_prefix("http://") else {
        return Fetched::Failed(ONLY_HTTP.to_owned());
    };
    let (host, path) = rest.find('/').map_or((rest, "/"), |i| rest.split_at(i));
    let address = if host.contains(':') {
        host.to_owned()
    } else {
        format!("{host}:80")
    };
    let to = match address.to_socket_addrs().map(|mut a| a.next()) {
        Ok(Some(to)) => to,
        Ok(None) => return Fetched::Failed(format!("{host} has no address")),
        Err(e) => return failed(&e),
    };
    let mut stream = match TcpStream::connect_timeout(&to, Duration::from_secs(10)) {
        Ok(stream) => stream,
        Err(e) => return failed(&e),
    };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
    let request = format!("GET {path} HTTP/1.0\r\nHost: {host}\r\nConnection: close\r\n\r\n");
    if let Err(e) = stream.write_all(request.as_bytes()) {
        return failed(&e);
    }
    let mut answer = Vec::new();
    if let Err(e) = stream.read_to_end(&mut answer) {
        return failed(&e);
    }
    answer_of(&answer)
}

/// What an HTTP answer brought: the body of a `200`, or the code of any other.
fn answer_of(answer: &[u8]) -> Fetched {
    let Some(at) = answer.windows(4).position(|w| w == b"\r\n\r\n") else {
        return Fetched::Failed("no headers".to_owned());
    };
    let head = String::from_utf8_lossy(&answer[..at]);
    let Some(code) = head.split_whitespace().nth(1) else {
        return Fetched::Failed("no status line".to_owned());
    };
    match code.parse::<u16>() {
        Ok(200) => String::from_utf8(answer[at + 4..].to_vec())
            .map_or_else(|_| Fetched::Failed("not text".to_owned()), Fetched::Text),
        Ok(code) => Fetched::Http(code),
        Err(_) => Fetched::Failed(format!("answered HTTP {code}")),
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the headless client's own fetch of what a status names).
    use super::*;
    use std::collections::BTreeMap;

    /// [`read`] through `server` (an address to what each fetch of it brings, in turn; `404` once
    /// it has nothing more): the texts, the failures, the addresses asked and the waits.
    fn read_from(
        status_url: &str,
        mut server: BTreeMap<&str, Vec<Fetched>>,
    ) -> (Vec<String>, Vec<String>, Vec<String>, Vec<f64>) {
        let (mut asked, mut waits) = (Vec::new(), Vec::new());
        let (texts, failed) = read(
            status_url,
            |url| {
                asked.push(url.to_owned());
                let answers = server.get_mut(url).filter(|a| !a.is_empty());
                answers.map_or(Fetched::Http(404), |a| a.remove(0))
            },
            |seconds| waits.push(seconds),
        );
        (texts, failed, asked, waits)
    }

    #[test]
    fn the_documents_a_status_names_are_fetched_from_its_address() {
        let (texts, failed, asked, waits) = read_from(
            "http://127.0.0.1:9100/status",
            BTreeMap::from([
                (
                    "http://127.0.0.1:9100/status",
                    vec![Fetched::Text(
                        r#"{"portals":"/portals","places":"/places"}"#.into(),
                    )],
                ),
                (
                    "http://127.0.0.1:9100/portals",
                    vec![Fetched::Text("dereth-navigation-store\t1\n".into())],
                ),
                (
                    "http://127.0.0.1:9100/places",
                    vec![
                        Fetched::Http(503),
                        Fetched::Text("dereth-places-store\t1\n".into()),
                    ],
                ),
            ]),
        );
        assert!(failed.is_empty(), "{failed:?}");
        assert_eq!(
            texts,
            ["dereth-navigation-store\t1\n", "dereth-places-store\t1\n"]
        );
        assert_eq!(asked.len(), 4, "{asked:?}");
        assert_eq!(waits, [1.0]);
    }

    #[test]
    fn a_document_answering_503_is_asked_thirty_times_a_second_apart() {
        let (texts, failed, asked, waits) = read_from(
            "http://127.0.0.1:9100/status",
            BTreeMap::from([
                (
                    "http://127.0.0.1:9100/status",
                    vec![Fetched::Text(r#"{"places":"/places"}"#.into())],
                ),
                ("http://127.0.0.1:9100/places", vec![Fetched::Http(503); 50]),
            ]),
        );
        assert!(texts.is_empty());
        assert_eq!(failed, ["http://127.0.0.1:9100/places answered HTTP 503"]);
        assert_eq!(asked.iter().filter(|u| u.ends_with("/places")).count(), 30);
        assert_eq!(waits, [1.0; 29]);
    }

    #[test]
    fn a_status_that_names_nothing_fetches_nothing_and_a_missing_document_is_told() {
        let (texts, failed, _, _) = read_from(
            "http://127.0.0.1:9100/status",
            BTreeMap::from([(
                "http://127.0.0.1:9100/status",
                vec![Fetched::Text(
                    r#"{"portals":null,"places":"/places"}"#.into(),
                )],
            )]),
        );
        assert!(texts.is_empty());
        assert_eq!(failed, ["http://127.0.0.1:9100/places answered HTTP 404"]);
        for status in ["https://example.org/status", "status.json"] {
            let (texts, failed) = read(status, get, |_| panic!("nothing to wait for"));
            assert!(texts.is_empty());
            assert_eq!(failed, [format!("{status}: {ONLY_HTTP}")]);
        }
    }

    #[test]
    fn an_answer_is_its_body_on_200_and_its_code_otherwise() {
        assert_eq!(
            answer_of(b"HTTP/1.0 200 OK\r\nContent-Length: 4\r\n\r\nbody"),
            Fetched::Text("body".into())
        );
        assert_eq!(
            answer_of(b"HTTP/1.1 503 Service Unavailable\r\n\r\n"),
            Fetched::Http(503)
        );
        assert_eq!(
            answer_of(b"HTTP/1.0 404 Not Found\r\n\r\n"),
            Fetched::Http(404)
        );
        assert_eq!(
            answer_of(b"HTTP/1.0 200 OK\r\n\r\n\xff"),
            Fetched::Failed("not text".into())
        );
        assert_eq!(
            answer_of(b"HTTP/1.0 200"),
            Fetched::Failed("no headers".into())
        );
        assert_eq!(
            answer_of(b"\r\n\r\n"),
            Fetched::Failed("no status line".into())
        );
    }
}
