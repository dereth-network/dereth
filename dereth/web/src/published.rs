//! What the world's server publishes (its portal table and its places), read for the client
//! through the page's fetch: the world's status, then each document it names on its own address,
//! as the desktop client reads them. The reading is the runtime's
//! ([`PublishedReading`]); this is its turn each frame, and the form an answer crosses from the
//! worker in.
//!
//! The worker starts each fetch it is given by its request's number and hands each answer back
//! as a line, the number and the HTTP status (or `error`), then the body (or why). Request
//! numbers never repeat in a worker, so an answer to an earlier play's request is nobody's.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

use dereth_client_runtime::travel::{Fetched, PublishedReading};

/// The next request's number, in this worker.
static NEXT_REQUEST: AtomicU32 = AtomicU32::new(1);

/// The reading of what one world publishes, for one play.
#[derive(Debug)]
pub struct Published {
    reading: PublishedReading,
    /// The request asked and not yet answered, by its number.
    waiting: Option<u32>,
}

impl Published {
    /// The reading for the world whose status is at `status_url` (`--world-status`); `None` for
    /// an address that is no status.
    #[must_use]
    pub fn new(status_url: &str) -> Option<Self> {
        Some(Self {
            reading: PublishedReading::new(status_url)?,
            waiting: None,
        })
    }

    /// This frame's turn, at `now` (the client's time, in seconds): the answers the worker has
    /// (`next` gives each, as it hands it back, and an empty one when it has no more) taken in,
    /// each document that answered put in `inbox` for the client; then the request to start now,
    /// when one is due: its number and its address.
    pub fn step(
        &mut self,
        now: f64,
        mut next: impl FnMut() -> String,
        inbox: &Mutex<Vec<String>>,
    ) -> Option<(u32, String)> {
        loop {
            let line = next();
            if line.is_empty() {
                break;
            }
            let Some((id, fetched)) = answer_of(&line) else {
                continue;
            };
            if self.waiting != Some(id) {
                continue;
            }
            self.waiting = None;
            match self.reading.answer(now, fetched) {
                Ok(Some(text)) => {
                    if let Ok(mut inbox) = inbox.lock() {
                        inbox.push(text);
                    }
                }
                Ok(None) => {}
                Err(e) => tracing::warn!("what the world publishes: {e}"),
            }
        }
        let url = self.reading.ask(now)?;
        let id = NEXT_REQUEST.fetch_add(1, Ordering::Relaxed);
        self.waiting = Some(id);
        tracing::info!("reading what the world publishes: {url}");
        Some((id, url))
    }

    /// Whether everything is read: the status, and each document it names.
    #[must_use]
    pub fn done(&self) -> bool {
        self.reading.done()
    }
}

/// An answer as the worker hands it back: the request's number and what its fetch brought.
/// `None` for a line that is no answer.
#[must_use]
pub fn answer_of(line: &str) -> Option<(u32, Fetched)> {
    let (head, body) = line.split_once('\n').unwrap_or((line, ""));
    let (id, status) = head.split_once(' ')?;
    let id = id.parse().ok()?;
    let fetched = match status.parse::<u16>() {
        Ok(200) => Fetched::Text(body.to_owned()),
        Ok(code) => Fetched::Http(code),
        Err(_) if status == "error" => Fetched::Failed(body.to_owned()),
        Err(_) => return None,
    };
    Some((id, fetched))
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the web client's own reading of what a world's server publishes, and its
    //! keeping of a world's places in the page's storage).
    use super::*;
    use dereth_client_runtime::travel::{PlaceSeen, StoreFiles, Travel};
    use dereth_navigation::{EdgeSource, PlaceKind};
    use dereth_primitives::{CellId, Frame, ObjectId, Position, Quat, Vec3};
    use std::cell::RefCell;
    use std::collections::{BTreeMap, VecDeque};

    const STATUS: &str = r#"{"world_name":"Server","portals":"/portals","places":"/places"}"#;
    const PORTALS: &str = "dereth-navigation-store\t1\n";
    const PLACES: &str = "dereth-places-store\t1\nworld\tServer\n\
place\tlifestone\tpublished\t509\tLife Stone\t0xA9B40011 10.000 10.000 0.000\t0\t0\t-\t-\n\
place\tportal\tpublished\t1000\tPortal\t0xA9B40011 30.000 10.000 0.000\t0\t0\t-\t0x01AC0118 60.000 -50.000 0.500\n";

    thread_local! {
        /// The page's storage: the settings store's bytes, as last saved.
        static SAVED: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    }

    fn save(bytes: &[u8]) {
        SAVED.with(|s| *s.borrow_mut() = bytes.to_vec());
    }

    #[test]
    fn an_answer_reads_as_the_worker_hands_it_back() {
        assert_eq!(
            answer_of("7 200\nthe body\nand more"),
            Some((7, Fetched::Text("the body\nand more".into())))
        );
        assert_eq!(answer_of("8 503\n"), Some((8, Fetched::Http(503))));
        assert_eq!(
            answer_of("9 error\nTypeError: Failed to fetch"),
            Some((9, Fetched::Failed("TypeError: Failed to fetch".into())))
        );
        assert_eq!(answer_of("nonsense"), None);
        assert_eq!(answer_of("x 200\n"), None);
        assert_eq!(answer_of("1 what\n"), None);
    }

    /// What a world publishes is fetched through the worker and taken in under the world's
    /// places, and what the character learns of them is kept in the page's storage, under the
    /// world's listed address, for the next visit.
    #[test]
    fn a_worlds_published_places_are_read_and_its_learned_ones_kept_in_the_page_across_visits() {
        crate::settings::install(&[], Some(save));
        // The world's words, as the front page gives them for a world it may read the status of.
        let words: Vec<String> = [
            "--world-status",
            "http://127.0.0.1:9100/status",
            "--world-address",
            "play.example.org:9050",
        ]
        .map(str::to_owned)
        .to_vec();
        let mut cfg = crate::play::world_config(&words, None).unwrap();
        cfg.preferences_file = crate::settings::preferences_file();

        // The worker: each request started, answered on a later frame, and one answered for
        // another play's request in between.
        let mut server = BTreeMap::from([
            ("http://127.0.0.1:9100/status", vec![STATUS]),
            ("http://127.0.0.1:9100/portals", vec![PORTALS]),
            ("http://127.0.0.1:9100/places", vec!["503", PLACES]),
        ]);
        let mut pending: VecDeque<String> = VecDeque::new();
        let inbox = Mutex::new(Vec::new());
        let mut published = Published::new(cfg.world_status.as_deref().unwrap()).unwrap();
        let mut asked = Vec::new();
        let mut now = 0.0;
        while !published.done() {
            if let Some((id, url)) =
                published.step(now, || pending.pop_front().unwrap_or_default(), &inbox)
            {
                if asked.is_empty() {
                    pending.push_back(format!("{} 200\n{PLACES}", id + 1000));
                }
                let answers = server.get_mut(url.as_str()).unwrap();
                let body = answers.remove(0);
                pending.push_back(if body == "503" {
                    format!("{id} 503\n")
                } else {
                    format!("{id} 200\n{body}")
                });
                asked.push(url);
            }
            now += 1.0;
            assert!(now < 100.0, "the reading ends");
        }
        assert_eq!(
            asked.iter().filter(|u| u.ends_with("/places")).count(),
            2,
            "{asked:?}"
        );
        let docs = std::mem::take(&mut *inbox.lock().unwrap());
        assert_eq!(docs, [PORTALS, PLACES]);

        // The client takes them in, under the world's name and listed address.
        let world = cfg.travel_world("Server");
        let mut travel = Travel::new(StoreFiles::of(&cfg));
        for d in &docs {
            travel.take_published(d).unwrap();
        }
        travel.note_world(&world);
        assert_eq!(travel.places().places().len(), 2);
        // The character finds a vendor; it is kept in the page's storage.
        let at = Position::new(
            CellId(0xA9B4_0011),
            Frame::new(Vec3::new(50.0, 10.0, 0.0), Quat::IDENTITY),
        );
        travel.see_object(&PlaceSeen {
            id: ObjectId(0x8000_0001),
            bitfield: 0x0000_0200,
            item_type: 0x10,
            wcid: 700,
            name: "Thomas the Vendor".into(),
            at: Some(at),
        });
        travel.save_places();
        let saved = SAVED.with(|s| s.borrow().clone());
        let files = crate::settings::decode(&saved);
        assert!(
            files.contains_key(std::path::Path::new(
                "/settings/navigation-places-Server_play_example_org_9050.tsv"
            )),
            "{:?}",
            files.keys().collect::<Vec<_>>()
        );

        // The next visit reads it back from the page's storage, the server's under it again.
        crate::settings::install(&saved, Some(save));
        let mut again = Travel::new(StoreFiles::of(&cfg));
        again.note_world(&world);
        let kinds: Vec<(PlaceKind, EdgeSource)> = again
            .places()
            .places()
            .iter()
            .map(|p| (p.kind, p.source))
            .collect();
        assert_eq!(kinds, [(PlaceKind::Vendor, EdgeSource::Learned)]);
        for d in &docs {
            again.take_published(d).unwrap();
        }
        assert_eq!(again.places().places().len(), 3);
        // Another world of the same name, at another address, has none of them.
        again.note_world("Server@other.example.org:9000");
        assert!(again
            .places()
            .places()
            .iter()
            .all(|p| p.source == EdgeSource::Published));
    }
}
