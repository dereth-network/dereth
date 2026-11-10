//! What the world's server publishes, read for the client off the frame's thread: the document
//! `--nav-published` names, and those the world's status (`--world-status`) names. The reading is
//! the runtime's ([`PublishedReading`](dereth_client_runtime::travel::PublishedReading)); the
//! fetch is the client's own ([`crate::plugins::fetched`]: a file, or an `http://` or `https://`
//! URL).

use std::sync::Mutex;

use dereth_client_runtime::travel::{Answered, Fetched, PublishedReading};

/// How many times an address answering `503` is asked for, and how many seconds apart: a server
/// still gathering what it publishes answers so for a while after it starts.
pub const TRIES: u32 = 11;
pub const AGAIN: f64 = 3.0;

/// Reads what the server publishes: the status at `world_status`, then `nav_published`, then each
/// document the status names, each fetched by `fetch`, a `503` waited out by `wait` (given the
/// seconds). Each document that answers goes into `inbox` as it comes; each that does not is
/// logged.
pub fn read(
    world_status: Option<&str>,
    nav_published: Option<String>,
    fetch: impl FnMut(&str) -> Fetched,
    wait: impl FnMut(f64),
    inbox: &Mutex<Vec<String>>,
) {
    let status = world_status.and_then(|status| {
        let reading = PublishedReading::new(status);
        if reading.is_none() {
            tracing::warn!("--world-status: {status} is not an http:// or https:// address");
        }
        reading
    });
    let reading = match (status, nav_published) {
        (Some(reading), Some(document)) => reading.and_document(document),
        (Some(reading), None) => reading,
        (None, document) => PublishedReading::documents(document),
    };
    reading
        .retrying(TRIES, AGAIN)
        .read_all(fetch, wait, |answered| match answered {
            Answered::Document(text) => {
                if let Ok(mut inbox) = inbox.lock() {
                    inbox.push(text);
                }
            }
            Answered::StatusFailed(e) => tracing::warn!("--world-status: {e}"),
            Answered::DocumentFailed(e) => tracing::warn!("a published document: {e}"),
        });
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the desktop client's own reading of what a world's server publishes).
    use super::*;
    use std::collections::BTreeMap;

    const STATUS: &str = r#"{"world_name":"Server","portals":"/portals","places":"/places"}"#;
    const PORTALS: &str = "dereth-navigation-store\t1\n";
    const PLACES: &str = "dereth-places-store\t1\nworld\tServer\n";
    const GIVEN: &str = "dereth-navigation-store\t1\n# given\n";

    /// Reads through `server` (an address to what each fetch of it brings, in turn; `404` once
    /// it has nothing more): the addresses asked, the waits, and the inbox.
    fn read_from(
        world_status: Option<&str>,
        nav_published: Option<&str>,
        mut server: BTreeMap<&str, Vec<Fetched>>,
    ) -> (Vec<String>, Vec<f64>, Vec<String>) {
        let (mut asked, mut waits) = (Vec::new(), Vec::new());
        let inbox = Mutex::new(Vec::new());
        read(
            world_status,
            nav_published.map(str::to_owned),
            |url| {
                asked.push(url.to_owned());
                let answers = server.get_mut(url).filter(|a| !a.is_empty());
                answers.map_or(Fetched::Http(404), |a| a.remove(0))
            },
            |seconds| waits.push(seconds),
            &inbox,
        );
        (asked, waits, inbox.into_inner().unwrap())
    }

    #[test]
    fn the_status_then_the_given_document_then_those_the_status_names_reach_the_inbox() {
        let (asked, waits, inbox) = read_from(
            Some("http://127.0.0.1:9100/status"),
            Some("given.tsv"),
            BTreeMap::from([
                (
                    "http://127.0.0.1:9100/status",
                    vec![Fetched::Text(STATUS.into())],
                ),
                ("given.tsv", vec![Fetched::Text(GIVEN.into())]),
                (
                    "http://127.0.0.1:9100/portals",
                    vec![Fetched::Text(PORTALS.into())],
                ),
                (
                    "http://127.0.0.1:9100/places",
                    vec![Fetched::Http(503), Fetched::Text(PLACES.into())],
                ),
            ]),
        );
        assert_eq!(
            asked,
            [
                "http://127.0.0.1:9100/status",
                "given.tsv",
                "http://127.0.0.1:9100/portals",
                "http://127.0.0.1:9100/places",
                "http://127.0.0.1:9100/places",
            ]
        );
        assert_eq!(waits, [AGAIN]);
        assert_eq!(inbox, [GIVEN, PORTALS, PLACES]);
    }

    #[test]
    fn a_document_answering_503_is_asked_eleven_times_three_seconds_apart() {
        let (asked, waits, inbox) = read_from(
            Some("http://127.0.0.1:9100/status"),
            None,
            BTreeMap::from([
                (
                    "http://127.0.0.1:9100/status",
                    vec![Fetched::Text(STATUS.into())],
                ),
                (
                    "http://127.0.0.1:9100/portals",
                    vec![Fetched::Http(503); 50],
                ),
                (
                    "http://127.0.0.1:9100/places",
                    vec![Fetched::Text(PLACES.into())],
                ),
            ]),
        );
        let portals = asked.iter().filter(|u| u.ends_with("/portals")).count();
        assert_eq!(portals, 11);
        assert_eq!(waits, [3.0; 10]);
        assert_eq!(inbox, [PLACES]);
    }

    #[test]
    fn a_status_that_is_no_address_or_does_not_answer_still_reads_the_given_document() {
        for status in ["status.json", "http://127.0.0.1:9100/status"] {
            let (asked, _, inbox) = read_from(
                Some(status),
                Some("given.tsv"),
                BTreeMap::from([
                    (
                        "http://127.0.0.1:9100/status",
                        vec![Fetched::Failed("refused".into())],
                    ),
                    ("given.tsv", vec![Fetched::Text(GIVEN.into())]),
                ]),
            );
            assert_eq!(
                asked.last().map(String::as_str),
                Some("given.tsv"),
                "{status}"
            );
            assert!(!asked.iter().any(|u| u == "status.json"), "{asked:?}");
            assert_eq!(inbox, [GIVEN], "{status}");
        }
        let (asked, waits, inbox) = read_from(None, None, BTreeMap::new());
        assert!(asked.is_empty() && waits.is_empty() && inbox.is_empty());
    }

    #[test]
    fn a_file_named_is_read_as_a_document_and_a_missing_one_is_told() {
        let here = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");
        assert!(
            matches!(crate::plugins::fetched(here), Fetched::Text(t) if t.contains("dereth-client"))
        );
        let missing = concat!(env!("CARGO_MANIFEST_DIR"), "/no-such-document.tsv");
        assert!(matches!(
            crate::plugins::fetched(missing),
            Fetched::Failed(_)
        ));
    }
}
