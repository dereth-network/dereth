//! The interface's text elements' string downloads: a string reference given to a text element
//! whose string tables the data files lack asks the data cache for them, and the element writes
//! its text once they have come or been refused.
//!
//! **Starting** ([`StringDownloads::start`]): the tables the reference reads (its own and every
//! nested reference's) are asked for when any of them is not in the files. When all of them are,
//! the private tables their entries send the reference's tokens on to are asked for instead, when
//! any of those is not. Otherwise nothing is asked and the text is written at once. Everything is
//! asked for in one batch, for the gathering [`GATHER_STRINGS`].
//!
//! **While it waits**, each record the batch asked for that finishes the first time names the
//! private tables of the reference's entry in it, and those not in the files are asked for too.
//! The download waits until everything it asked for has finished, whatever the outcome. A table
//! the server refused leaves the reference unwritable, and its text is the client's error message
//! naming the token and the table.
//!
//! **What is asked is kept** for the session once it has finished, as the client keeps it: a
//! table refused once is not asked for again. A download dropped while it waits lets its batches
//! go ([`StringDownloads::release`]).
//!
//! The text elements live in the interface and hold a [`StringDownloads`] (clones share it); the
//! data cache's own asking happens here, each time the cache is given time ([`ask`]) and as its
//! askers are told ([`told`]).

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use auberean_assets::requests::{Batch, Event, Listener};
use auberean_dat::{QualifiedId, QualifiedIds};
use auberean_text::{DatSource, StringInfo};

use crate::cache::DataCache;
use crate::download::DataDownload;

/// The gathering string tables are asked for by.
pub const GATHER_STRINGS: u32 = 0x4000_0007;

/// Who is told about a string download's batches (each download's number is the word it is told
/// with).
pub const STRINGS: u32 = crate::download::listeners::STRINGS;

/// The text elements' string downloads; clones share them.
#[derive(Clone, Default)]
pub struct StringDownloads(Arc<Mutex<Table>>);

#[derive(Default)]
struct Table {
    cache: Option<DataCache>,
    /// The text source over the cache, made at the first need.
    text: Option<DatSource>,
    next: u32,
    downloads: BTreeMap<u32, Download>,
}

struct Download {
    info: StringInfo,
    /// Records to ask for, not asked yet.
    to_ask: Vec<QualifiedId>,
    /// The batches asked for and not finished.
    waiting: Vec<Batch>,
    /// Dropped while it waited: its batches are to be let go.
    released: bool,
}

impl core::fmt::Debug for StringDownloads {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let n = self.0.lock().map_or(0, |t| t.downloads.len());
        f.debug_struct("StringDownloads")
            .field("downloads", &n)
            .finish()
    }
}

impl Table {
    /// The cache and its text source, when the cache has files.
    fn source(&mut self) -> Option<(DataCache, &mut DatSource)> {
        let cache = self.cache.clone().filter(|c| !c.is_empty())?;
        let text = self.text.get_or_insert_with(|| cache.text_source());
        Some((cache, text))
    }
}

/// The private tables `info`'s entries in each of `tables` send its tokens on to.
fn private_ids(info: &StringInfo, text: &mut DatSource, tables: &[QualifiedId]) -> QualifiedIds {
    let mut out = QualifiedIds::new();
    for q in tables {
        info.sub_private_ids(text, q.id, &mut out);
    }
    out
}

impl StringDownloads {
    /// Downloads over this data cache (a client's); the one given before is let go.
    pub fn set_cache(&self, cache: &DataCache) {
        if let Ok(mut t) = self.0.lock() {
            t.cache = Some(cache.clone());
            t.text = None;
        }
    }

    /// Start a download for a reference a text element was given: its number while it waits,
    /// `None` when every table it reads is at hand and it is written at once.
    // D2-11742, D2-11740
    pub fn start(&self, info: &StringInfo) -> Option<u32> {
        let mut t = self.0.lock().ok()?;
        let (cache, text) = t.source()?;
        let stores = cache.stores();
        let mut ids = QualifiedIds::new();
        info.sub_data_ids(&mut ids);
        let ids: Vec<QualifiedId> = ids.iter().collect();
        let to_ask = if stores.are_on_disk(&ids) {
            let private: Vec<QualifiedId> = private_ids(info, text, &ids).iter().collect();
            if private.is_empty() || stores.are_on_disk(&private) {
                return None;
            }
            private
        } else {
            ids
        };
        t.next = t.next.wrapping_add(1).max(1);
        let n = t.next;
        t.downloads.insert(
            n,
            Download {
                info: info.clone(),
                to_ask,
                waiting: Vec::new(),
                released: false,
            },
        );
        Some(n)
    }

    /// Whether a download still waits. One that does not is forgotten (the element writes it).
    #[must_use]
    pub fn waiting(&self, n: u32) -> bool {
        let Ok(mut t) = self.0.lock() else {
            return false;
        };
        let waits = t
            .downloads
            .get(&n)
            .is_some_and(|d| !d.to_ask.is_empty() || !d.waiting.is_empty());
        if !waits {
            t.downloads.remove(&n);
        }
        waits
    }

    /// A download dropped by its element: what it asked for is let go, and it is told nothing.
    // D2-11739
    pub fn release(&self, n: u32) {
        let Ok(mut t) = self.0.lock() else { return };
        match t.downloads.get_mut(&n) {
            Some(d) if !d.waiting.is_empty() => d.released = true,
            Some(_) => {
                t.downloads.remove(&n);
            }
            None => {}
        }
    }

    /// A reference's text as a text element shows it, read through the data cache: written, or
    /// the client's error message when it cannot be; `None` without a cache that has files.
    // D2-14126
    #[must_use]
    pub fn write(&self, info: &StringInfo) -> Option<Vec<u16>> {
        let mut t = self.0.lock().ok()?;
        let (_, text) = t.source()?;
        Some(info.clone().get_string(text, false))
    }

    /// A reference written only when it is valid (it and every nested reference name an entry
    /// their tables hold); `None` otherwise, or without a cache that has files.
    #[must_use]
    pub fn write_valid(&self, info: &StringInfo) -> Option<Vec<u16>> {
        let mut t = self.0.lock().ok()?;
        let (_, text) = t.source()?;
        info.is_valid(text, true)
            .then(|| info.clone().get_string(text, false))
    }

    /// A reference written when it renders; `None` when it does not, or without a cache that has
    /// files.
    #[must_use]
    pub fn write_rendered(&self, info: &StringInfo) -> Option<Vec<u16>> {
        let mut t = self.0.lock().ok()?;
        let (_, text) = t.source()?;
        let (ok, units) = info.clone().inq(text, false);
        ok.then_some(units)
    }

    /// Whether it reads through a data cache that has files (it writes nothing otherwise).
    #[must_use]
    pub fn reads(&self) -> bool {
        self.0
            .lock()
            .is_ok_and(|t| t.cache.as_ref().is_some_and(|c| !c.is_empty()))
    }

    /// How many downloads are kept (waiting, or finished and not yet written).
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.lock().map_or(0, |t| t.downloads.len())
    }

    /// Whether none is.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// The downloads' asking, as the cache is given time: what new downloads need is asked for, each
/// in a batch the download is told about; the batches of downloads dropped while they waited are
/// let go.
pub fn ask(download: &mut DataDownload) {
    let strings = download.strings().clone();
    let (asks, dropped) = {
        let Ok(mut t) = strings.0.lock() else { return };
        let mut asks = Vec::new();
        let mut dropped = Vec::new();
        t.downloads.retain(|n, d| {
            if d.released {
                dropped.append(&mut d.waiting);
                return false;
            }
            if !d.to_ask.is_empty() {
                asks.push((*n, std::mem::take(&mut d.to_ask)));
            }
            true
        });
        (asks, dropped)
    };
    for b in dropped {
        download.release(b);
    }
    for (n, ids) in asks {
        let listener = Listener {
            to: STRINGS,
            user: n,
        };
        let batch = download.get_many(GATHER_STRINGS, &ids, Some(listener));
        add_batch(&strings, n, batch);
    }
}

/// A batch asked for a download: it waits for it (one told about nobody is never waited for).
fn add_batch(strings: &StringDownloads, n: u32, batch: Batch) {
    if batch == Batch::NONE {
        return;
    }
    if let Ok(mut t) = strings.0.lock() {
        if let Some(d) = t.downloads.get_mut(&n) {
            d.waiting.push(batch);
        }
    }
}

/// What the downloads' batches were told: a record asked for that finished the first time has the
/// private tables of the reference's entry in it that the files lack asked for next (when the cache
/// is next given time); a batch done is no longer waited for.
// D2-11741, D2-11738
pub fn told(download: &DataDownload, events: Vec<Event>) {
    let Ok(mut t) = download.strings().0.lock() else {
        return;
    };
    for e in events {
        match e {
            Event::Finished {
                record,
                user,
                times: 1,
                ..
            } => {
                let Some(info) = t.downloads.get(&user).map(|d| d.info.clone()) else {
                    continue;
                };
                let Some((cache, text)) = t.source() else {
                    continue;
                };
                let private: Vec<QualifiedId> =
                    private_ids(&info, text, &[record]).iter().collect();
                if !private.is_empty() && !cache.stores().are_on_disk(&private) {
                    if let Some(d) = t.downloads.get_mut(&user) {
                        d.to_ask.extend(private);
                    }
                }
            }
            Event::BatchDone { batch, user, .. } => {
                if let Some(d) = t.downloads.get_mut(&user) {
                    d.waiting.retain(|b| *b != batch);
                }
            }
            _ => {}
        }
    }
}

/// Whether an event is told to the string downloads.
#[must_use]
pub const fn is_theirs(e: &Event) -> bool {
    matches!(
        e,
        Event::Finished { to: STRINGS, .. } | Event::BatchDone { to: STRINGS, .. }
    )
}
