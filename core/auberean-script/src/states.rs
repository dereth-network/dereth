//! Saved script states read from a data file, decoded once and kept.
//!
//! A host hands the machine its saved states through [`crate::native::Host::saved_state`]; this
//! is the usual way to answer it: read the record from the portal file, read its packed graph
//! against the library, keep the result. A record that is not there is remembered as absent; one
//! that does not read is read again first, so that a read the machine got wrong once (under heavy
//! load) is not remembered for the rest of the run: only a failure that repeats is the data's own,
//! remembered with why.

use std::collections::HashMap;
use std::sync::Arc;

use auberean_assets::script::ScriptState;
use auberean_dat::DatFile;

use crate::graph::SavedState;
use crate::library::Library;

/// Saved states by id, read on first ask.
pub struct StateCache {
    dat: Arc<DatFile>,
    cache: HashMap<u32, Option<Arc<SavedState>>>,
    /// States that were in the data but would not read, with why.
    pub failures: Vec<(u32, String)>,
    /// Reads that failed once and read on the next try.
    pub reread: u32,
}

/// How many times a state that does not read is tried in all.
const TRIES: usize = 3;

impl core::fmt::Debug for StateCache {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("StateCache")
            .field("read", &self.cache.len())
            .field("failures", &self.failures.len())
            .finish()
    }
}

impl StateCache {
    /// A cache over a portal file.
    #[must_use]
    pub fn new(dat: Arc<DatFile>) -> Self {
        Self {
            dat,
            cache: HashMap::new(),
            failures: Vec::new(),
            reread: 0,
        }
    }

    /// A saved state by id, read against `lib` the first time it is asked for.
    pub fn get(&mut self, id: u32, lib: &Library) -> Option<Arc<SavedState>> {
        if let Some(s) = self.cache.get(&id) {
            return s.clone();
        }
        let mut s = self.read(id, lib);
        for _ in 1..TRIES {
            if s.is_ok() {
                break;
            }
            s = self.read(id, lib);
            if s.is_ok() {
                self.reread += 1;
            }
        }
        let s = s.unwrap_or_else(|why| {
            self.failures.push((id, why));
            None
        });
        self.cache.insert(id, s.clone());
        s
    }

    /// One read: the state, `None` when the data has no such record, or why it would not read.
    fn read(&self, id: u32, lib: &Library) -> Result<Option<Arc<SavedState>>, String> {
        if self.dat.entry(id).is_none() {
            return Ok(None);
        }
        let bytes = self.dat.read(id).map_err(|e| e.to_string())?;
        let rec: ScriptState =
            auberean_assets::decode(id, &bytes, &auberean_assets::Context::empty())
                .map_err(|e| e.to_string())?;
        SavedState::decode(&rec.graph, rec.package, lib)
            .map(|s| Some(Arc::new(s)))
            .map_err(|e| e.to_string())
    }

    /// How many states have been read (or found absent).
    #[must_use]
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }
}
