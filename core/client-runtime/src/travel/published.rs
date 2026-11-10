//! The documents a world's status names as published: the server's portal table and its places,
//! each a path on the address the status was read from; and the reading of them for a host that
//! fetches for the client a request at a time ([`PublishedReading`]).

use std::collections::VecDeque;

/// The addresses of the documents `status` (the world's status document, read from `status_url`)
/// names as published: its `portals` and its `places`, in that order, each a path on the address
/// the status was read from. A value that is no such path is passed over, so a status can never
/// send the client to another host.
#[must_use]
pub fn published_documents(status_url: &str, status: &str) -> Vec<String> {
    let Some(origin) = origin(status_url) else {
        return Vec::new();
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(status) else {
        return Vec::new();
    };
    ["portals", "places"]
        .into_iter()
        .filter_map(|k| v.get(k)?.as_str())
        .filter(|p| {
            p.starts_with('/')
                && !p.starts_with("//")
                && !p.contains(|c: char| c == '\\' || c.is_whitespace() || c.is_control())
        })
        .map(|p| format!("{origin}{p}"))
        .collect()
}

/// `http://host[:port]` or `https://host[:port]` of a URL of either scheme; `None` for any other,
/// or one naming a user.
fn origin(url: &str) -> Option<&str> {
    let rest = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))?;
    let scheme = url.len() - rest.len();
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    if end == 0 || rest[..end].contains('@') {
        return None;
    }
    Some(&url[..scheme + end])
}

/// How many times an address the server answers `503` for is asked for, and how many seconds
/// apart: a server still gathering what it publishes answers so for a while after it starts.
pub const PUBLISHED_TRIES: u32 = 10;
pub const PUBLISHED_AGAIN: f64 = 3.0;

/// What a host's fetch of an address brought.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fetched {
    /// The body of a `200`.
    Text(String),
    /// Any other HTTP answer, by its code.
    Http(u16),
    /// No answer, and why.
    Failed(String),
}

/// Reading what a world's server publishes, with the fetching left to the host: the world's
/// status, then each document it names ([`published_documents`]), one request at a time. The
/// host asks for the next address ([`Self::ask`]), fetches it, and hands back what came
/// ([`Self::answer`]); each document that answers is the host's to put in the client's inbox
/// (`App::published`).
#[derive(Debug, Clone)]
pub struct PublishedReading {
    status_url: String,
    /// The addresses still to read, the one asked first: the status, until it answers, and then
    /// the documents it names.
    queue: VecDeque<String>,
    status_read: bool,
    /// The first address is asked and not yet answered.
    asked: bool,
    /// How many times the first address has answered `503`, and when it is asked again.
    tries: u32,
    again_at: f64,
    /// How many times an address answering `503` is asked for, and how many seconds apart.
    most_tries: u32,
    again: f64,
}

impl PublishedReading {
    /// The reading of what the world whose status document is at `status_url` publishes; an
    /// address that names no path names the world's `/status`. `None` for an address that is not
    /// `http://` or `https://`.
    #[must_use]
    pub fn new(status_url: &str) -> Option<Self> {
        let status_url = status_url.trim();
        let origin = origin(status_url)?;
        let status_url = match &status_url[origin.len()..] {
            "" | "/" => format!("{origin}/status"),
            _ => status_url.to_owned(),
        };
        Some(Self {
            queue: VecDeque::from([status_url.clone()]),
            status_url,
            status_read: false,
            asked: false,
            tries: 0,
            again_at: f64::NEG_INFINITY,
            most_tries: PUBLISHED_TRIES,
            again: PUBLISHED_AGAIN,
        })
    }

    /// The reading of `documents` alone, with no status to read first: each is an address the
    /// host's fetch understands.
    #[must_use]
    pub fn documents(documents: impl IntoIterator<Item = String>) -> Self {
        Self {
            queue: documents.into_iter().collect(),
            status_url: String::new(),
            status_read: true,
            asked: false,
            tries: 0,
            again_at: f64::NEG_INFINITY,
            most_tries: PUBLISHED_TRIES,
            again: PUBLISHED_AGAIN,
        }
    }

    /// This reading, with `document` read too: after the status, and before the documents it
    /// names.
    #[must_use]
    pub fn and_document(mut self, document: String) -> Self {
        self.queue.push_back(document);
        self
    }

    /// This reading, with an address answering `503` asked for `tries` times in all (at least
    /// once), `again` seconds apart, in place of [`PUBLISHED_TRIES`] and [`PUBLISHED_AGAIN`].
    #[must_use]
    pub fn retrying(mut self, tries: u32, again: f64) -> Self {
        self.most_tries = tries.max(1);
        self.again = again;
        self
    }

    /// The address to fetch now, at `now` (seconds, on any clock that only goes forward): `None`
    /// while one is asked and not yet answered, while a `503` is waited out, and once it is all
    /// read.
    pub fn ask(&mut self, now: f64) -> Option<String> {
        if self.asked || now < self.again_at {
            return None;
        }
        let url = self.queue.front()?.clone();
        self.asked = true;
        Some(url)
    }

    /// What the fetch of the address last asked brought, at `now`: a document the server
    /// publishes, for the client to take in; `None` for the status, an address asked again
    /// later, or an answer nothing was asked for.
    ///
    /// # Errors
    /// The address did not answer, or answered anything but `200` (`503` too, once its tries
    /// have passed): the reading goes on with the next one, and ends with a
    /// status that does not answer.
    pub fn answer(&mut self, now: f64, fetched: Fetched) -> Result<Option<String>, String> {
        if !self.asked {
            return Ok(None);
        }
        self.asked = false;
        let Some(url) = self.queue.front().cloned() else {
            return Ok(None);
        };
        if fetched == Fetched::Http(503) && self.tries + 1 < self.most_tries {
            self.tries += 1;
            self.again_at = now + self.again;
            return Ok(None);
        }
        self.queue.pop_front();
        self.tries = 0;
        self.again_at = f64::NEG_INFINITY;
        let status = !self.status_read;
        self.status_read = true;
        match fetched {
            Fetched::Text(text) if status => {
                self.queue
                    .extend(published_documents(&self.status_url, &text));
                Ok(None)
            }
            Fetched::Text(text) => Ok(Some(text)),
            Fetched::Http(code) => Err(format!("{url} answered HTTP {code}")),
            Fetched::Failed(why) => Err(format!("{url}: {why}")),
        }
    }

    /// Whether everything is read: the status, and each document it names.
    #[must_use]
    pub fn done(&self) -> bool {
        self.queue.is_empty()
    }

    /// Reads it all now, for a host that can wait for each answer: each address fetched in turn
    /// by `fetch`, each `503` waited out by `wait` (given the seconds to wait), and what each
    /// address brought handed to `each` as it comes.
    pub fn read_all(
        mut self,
        mut fetch: impl FnMut(&str) -> Fetched,
        mut wait: impl FnMut(f64),
        mut each: impl FnMut(Answered),
    ) {
        // The reading's clock moves only by the waits: a fetch's own time only spaces the
        // addresses further apart.
        let mut now = 0.0;
        while !self.done() {
            let Some(url) = self.ask(now) else {
                if self.again_at <= now {
                    break;
                }
                wait(self.again_at - now);
                now = self.again_at;
                continue;
            };
            let status = !self.status_read;
            let fetched = fetch(&url);
            match self.answer(now, fetched) {
                Ok(Some(text)) => each(Answered::Document(text)),
                Ok(None) => {}
                Err(why) if status => each(Answered::StatusFailed(why)),
                Err(why) => each(Answered::DocumentFailed(why)),
            }
        }
    }
}

/// What an address brought a reading read all at once ([`PublishedReading::read_all`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answered {
    /// A document the server publishes, for the client's inbox.
    Document(String),
    /// The status did not answer, and why: nothing it names is read.
    StatusFailed(String),
    /// A document did not answer, and why.
    DocumentFailed(String),
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (reads the client's own switch: where the status says the documents are).
    use super::*;

    #[test]
    fn the_status_names_its_published_documents_on_its_own_address() {
        let status =
            r#"{"world_name":"W","portals":"/portals","places":"/places","not_ported":{}}"#;
        assert_eq!(
            published_documents("http://127.0.0.1:9100/status", status),
            [
                "http://127.0.0.1:9100/portals",
                "http://127.0.0.1:9100/places"
            ]
        );
        assert_eq!(
            published_documents("https://play.example.org/v1/world?x=1", status),
            [
                "https://play.example.org/portals",
                "https://play.example.org/places"
            ]
        );
    }

    #[test]
    fn a_status_naming_nothing_or_another_host_names_nothing() {
        let url = "http://127.0.0.1:9100/status";
        assert!(published_documents(url, r#"{"portals":null,"places":null}"#).is_empty());
        assert!(published_documents(url, r#"{"world_name":"ACE"}"#).is_empty());
        assert!(published_documents(url, "not json").is_empty());
        for elsewhere in [
            "http://evil.example/places",
            "//evil.example/places",
            "places",
            "/pla ces",
            "/places\\..",
        ] {
            let status = format!(r#"{{"places":{}}}"#, serde_json::Value::from(elsewhere));
            assert!(published_documents(url, &status).is_empty(), "{elsewhere}");
        }
        assert!(published_documents("file:///status", r#"{"places":"/places"}"#).is_empty());
        assert!(
            published_documents("http://user@host/status", r#"{"places":"/places"}"#).is_empty()
        );
    }

    /// Reads everything `r` asks for from `server` (an address to what each fetch of it brings,
    /// in turn; `404` once it has nothing more), on a clock that moves only by what `r` waits
    /// for: the documents handed over, the failures, and the addresses in the order asked.
    fn read_all(
        mut r: PublishedReading,
        server: &mut std::collections::BTreeMap<&str, Vec<Fetched>>,
    ) -> (Vec<String>, Vec<String>, Vec<String>) {
        let (mut docs, mut failed, mut asked) = (Vec::new(), Vec::new(), Vec::new());
        let mut now = 0.0;
        while !r.done() {
            let Some(url) = r.ask(now) else {
                assert_eq!(r.ask(now + PUBLISHED_AGAIN - 0.5), None, "asked early");
                now += PUBLISHED_AGAIN;
                continue;
            };
            assert_eq!(r.ask(now), None, "one request at a time");
            let answers = server.get_mut(url.as_str()).filter(|a| !a.is_empty());
            let fetched = answers.map_or(Fetched::Http(404), |a| a.remove(0));
            asked.push(url);
            match r.answer(now, fetched) {
                Ok(Some(text)) => docs.push(text),
                Ok(None) => {}
                Err(e) => failed.push(e),
            }
        }
        assert_eq!(r.ask(now + 100.0), None, "nothing once done");
        (docs, failed, asked)
    }

    const STATUS: &str = r#"{"world_name":"Server","portals":"/portals","places":"/places"}"#;
    const PLACES: &str = "dereth-places-store\t1\nworld\tServer\n\
place\tlifestone\tpublished\t509\tLife Stone\t0xA9B40011 10.000 10.000 0.000\t0\t0\t-\t-\n\
place\tportal\tpublished\t1000\tPortal\t0xA9B40011 30.000 10.000 0.000\t0\t0\t-\t0x01AC0118 60.000 -50.000 0.500\n";
    const PORTALS: &str = "dereth-navigation-store\t1\n";

    #[test]
    fn a_world_status_is_read_then_each_document_it_names_and_the_travel_takes_them_in() {
        let reading = PublishedReading::new("http://127.0.0.1:9100/status").unwrap();
        let mut server = std::collections::BTreeMap::from([
            (
                "http://127.0.0.1:9100/status",
                vec![Fetched::Text(STATUS.into())],
            ),
            (
                "http://127.0.0.1:9100/portals",
                vec![Fetched::Text(PORTALS.into())],
            ),
            // Still gathering its places for a while after it starts.
            (
                "http://127.0.0.1:9100/places",
                vec![
                    Fetched::Http(503),
                    Fetched::Http(503),
                    Fetched::Text(PLACES.into()),
                ],
            ),
        ]);
        let (docs, failed, asked) = read_all(reading, &mut server);
        assert!(failed.is_empty(), "{failed:?}");
        assert_eq!(docs, [PORTALS, PLACES]);
        assert_eq!(
            asked,
            [
                "http://127.0.0.1:9100/status",
                "http://127.0.0.1:9100/portals",
                "http://127.0.0.1:9100/places",
                "http://127.0.0.1:9100/places",
                "http://127.0.0.1:9100/places",
            ]
        );
        // The documents, in the client's inbox, are the travel's: the places under the world's.
        let mut travel = crate::travel::Travel::new(crate::travel::StoreFiles::default());
        for text in &docs {
            travel.take_published(text).unwrap();
        }
        travel.note_world("Server@127.0.0.1:9000");
        let names: Vec<&str> = travel
            .places()
            .places()
            .iter()
            .map(|p| p.name.as_str())
            .collect();
        assert_eq!(names.len(), 2, "{names:?}");
        assert!(names.contains(&"Life Stone") && names.contains(&"Portal"));
    }

    #[test]
    fn a_document_that_will_not_answer_is_passed_over_and_a_status_that_will_not_ends_it() {
        // A document `503` for good is asked for its tries and no more; one missing is told; the
        // rest are still read.
        let reading = PublishedReading::new("https://play.example.org/").unwrap();
        let mut server = std::collections::BTreeMap::from([
            (
                "https://play.example.org/status",
                vec![Fetched::Text(STATUS.into())],
            ),
            (
                "https://play.example.org/portals",
                vec![Fetched::Http(503); 50],
            ),
        ]);
        let (docs, failed, asked) = read_all(reading, &mut server);
        assert!(docs.is_empty());
        assert_eq!(
            failed,
            [
                "https://play.example.org/portals answered HTTP 503",
                "https://play.example.org/places answered HTTP 404",
            ]
        );
        let portals = asked.iter().filter(|u| u.ends_with("/portals")).count();
        assert_eq!(portals, PUBLISHED_TRIES as usize);
        // A status that does not answer, or names nothing, reads nothing more.
        for status in [
            Fetched::Failed("refused".into()),
            Fetched::Http(404),
            Fetched::Text(r#"{"world_name":"ACE"}"#.into()),
        ] {
            let reading = PublishedReading::new("http://127.0.0.1:9100/status").unwrap();
            let mut server =
                std::collections::BTreeMap::from([("http://127.0.0.1:9100/status", vec![status])]);
            let (docs, _, asked) = read_all(reading, &mut server);
            assert!(docs.is_empty());
            assert_eq!(asked, ["http://127.0.0.1:9100/status"]);
        }
        // An answer nothing asked for is nobody's.
        let mut reading = PublishedReading::new("http://127.0.0.1:9100").unwrap();
        assert_eq!(reading.answer(0.0, Fetched::Text(PLACES.into())), Ok(None));
        assert_eq!(
            reading.ask(0.0).as_deref(),
            Some("http://127.0.0.1:9100/status")
        );
        assert!(PublishedReading::new("file:///status").is_none());
        assert!(PublishedReading::new("wss://play.example.org/").is_none());
    }

    /// A host that waits reads it all in one go: the documents as they come, the waits a `503`
    /// asks for at its own spacing and count, a document it was given between the status and
    /// those the status names, and a failed status told apart from a failed document.
    #[test]
    fn a_host_that_waits_reads_it_all_at_its_own_spacing_with_its_own_documents() {
        let status_url = "http://127.0.0.1:9100/status";
        let mut server = std::collections::BTreeMap::from([
            (
                status_url,
                vec![Fetched::Http(503), Fetched::Text(STATUS.into())],
            ),
            ("given.tsv", vec![Fetched::Text(PORTALS.into())]),
            (
                "http://127.0.0.1:9100/portals",
                vec![Fetched::Http(503); 50],
            ),
            (
                "http://127.0.0.1:9100/places",
                vec![Fetched::Text(PLACES.into())],
            ),
        ]);
        let (mut asked, mut waits, mut told) = (Vec::new(), Vec::new(), Vec::new());
        PublishedReading::new(status_url)
            .unwrap()
            .and_document("given.tsv".into())
            .retrying(4, 1.5)
            .read_all(
                |url| {
                    asked.push(url.to_owned());
                    let answers = server.get_mut(url).filter(|a| !a.is_empty());
                    answers.map_or(Fetched::Http(404), |a| a.remove(0))
                },
                |seconds| waits.push(seconds),
                |a| told.push(a),
            );
        assert_eq!(
            asked,
            [
                status_url,
                status_url,
                "given.tsv",
                "http://127.0.0.1:9100/portals",
                "http://127.0.0.1:9100/portals",
                "http://127.0.0.1:9100/portals",
                "http://127.0.0.1:9100/portals",
                "http://127.0.0.1:9100/places",
            ]
        );
        assert_eq!(waits, [1.5; 4]);
        assert_eq!(
            told,
            [
                Answered::Document(PORTALS.into()),
                Answered::DocumentFailed("http://127.0.0.1:9100/portals answered HTTP 503".into()),
                Answered::Document(PLACES.into()),
            ]
        );

        // A status that fails is told as the status's; a document given is read all the same.
        let mut told = Vec::new();
        PublishedReading::new(status_url)
            .unwrap()
            .and_document("given.tsv".into())
            .read_all(
                |url| match url {
                    "given.tsv" => Fetched::Text(PORTALS.into()),
                    _ => Fetched::Failed("refused".into()),
                },
                |_| panic!("nothing to wait for"),
                |a| told.push(a),
            );
        assert_eq!(
            told,
            [
                Answered::StatusFailed(format!("{status_url}: refused")),
                Answered::Document(PORTALS.into()),
            ]
        );

        // Documents alone, with no status: asked once at the fewest tries.
        let mut told = Vec::new();
        PublishedReading::documents(["given.tsv".to_owned()])
            .retrying(0, 1.0)
            .read_all(
                |_| Fetched::Http(503),
                |_| panic!("asked once"),
                |a| told.push(a),
            );
        assert_eq!(
            told,
            [Answered::DocumentFailed(
                "given.tsv answered HTTP 503".into()
            )]
        );
    }
}
