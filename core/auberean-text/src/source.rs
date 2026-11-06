//! A [`Source`] over the client's data files: string tables and the language's rules decoded once
//! and kept, and the enumeration mappings that name records. A record a file holds that does not
//! read or decode is read again first: only a failure that repeats counts it absent, so that a read
//! the machine got wrong once is not remembered for the rest of the run.

use std::collections::HashMap;
use std::sync::Arc;

use auberean_assets::strings::{StringState, StringTable};
use auberean_assets::tables::IdMap;
use auberean_assets::{decode, Context, Decode};
use auberean_dat::{type_of_id, DatFile, RecordType};

use crate::info::{Source, Table};
use crate::language::Language;

/// The enumeration mapping string tables (and the language's rules) are named in.
pub const TABLE_MAPPING: u32 = 0x4000_0004;
/// The value the language's string state has in [`TABLE_MAPPING`].
pub const LANGUAGE_VALUE: u32 = 0x4000_0001;

/// How many times a held record that does not read is tried in all.
const TRIES: usize = 3;

/// A record's bytes by id, as a host reads them.
pub type Reader = Arc<dyn Fn(u32) -> Option<Vec<u8>> + Send + Sync>;

/// Tables, the language and the enumeration mappings, read from data files in order (the first
/// file that has a record is the one read).
pub struct DatSource {
    files: Vec<Arc<DatFile>>,
    /// Where records are read when not straight from the files (the client's data cache): a
    /// record absent now may be there later, so absences are not remembered.
    reader: Option<Reader>,
    tables: HashMap<u32, Option<Arc<Table>>>,
    language: Option<Option<Arc<Language>>>,
    maps: HashMap<u32, Option<Arc<IdMap>>>,
}

impl core::fmt::Debug for DatSource {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("DatSource")
            .field("files", &self.files.len())
            .field("tables", &self.tables.len())
            .finish_non_exhaustive()
    }
}

impl DatSource {
    /// A source over these files, looked in in this order.
    #[must_use]
    pub fn new(files: Vec<Arc<DatFile>>) -> Self {
        Self {
            files,
            reader: None,
            tables: HashMap::new(),
            language: None,
            maps: HashMap::new(),
        }
    }

    /// A source whose records are read through `reader` (the files still name the master id
    /// map): a record it lacks is looked for again the next time it is asked for, so one that
    /// arrives later (a record the server sent) is found then.
    #[must_use]
    pub fn with_reader(files: Vec<Arc<DatFile>>, reader: Reader) -> Self {
        Self {
            reader: Some(reader),
            ..Self::new(files)
        }
    }

    fn read(&self, id: u32) -> Option<Vec<u8>> {
        match &self.reader {
            Some(r) => r(id),
            None => self.files.iter().find_map(|f| f.read(id).ok()),
        }
    }

    /// Whether an absence is kept: only when reading the files themselves, which never change.
    fn keeps_absences(&self) -> bool {
        self.reader.is_none()
    }

    /// A record decoded as `T`; one a file holds is tried again when it does not read.
    fn decoded<T: Decode>(&self, id: u32) -> Option<T> {
        let held = self.files.iter().any(|f| f.entry(id).is_some());
        let tries = if held { TRIES } else { 1 };
        (0..tries).find_map(|_| {
            self.read(id)
                .and_then(|b| decode::<T>(id, &b, &Context::empty()).ok())
        })
    }

    fn map(&mut self, id: u32) -> Option<Arc<IdMap>> {
        if let Some(m) = self.maps.get(&id) {
            return m.clone();
        }
        let m = (type_of_id(id) == Some(RecordType::ID_MAP))
            .then(|| self.decoded::<IdMap>(id))
            .flatten()
            .map(Arc::new);
        if m.is_some() || self.keeps_absences() {
            self.maps.insert(id, m.clone());
        }
        m
    }

    /// The record id an enumeration value maps to (0 for none): the first file's id map names the
    /// mapping's own map, and that map the record (its game half first, then its tools' half).
    // D2-12602, D2-12601, D2-12696, D2-12713
    pub fn data_id_by_enum(&mut self, value: u32, mapping: u32) -> u32 {
        let Some(master) = self.files.first().map(|f| f.header().master_map_id) else {
            return 0;
        };
        let lookup = |m: &IdMap, v: u32| {
            m.ids
                .iter()
                .chain(m.tool_ids.iter())
                .find(|(k, _)| *k == v)
                .map_or(0, |(_, d)| d.0)
        };
        let Some(master) = self.map(master) else {
            return 0;
        };
        let sub = lookup(&master, mapping);
        self.map(sub).map_or(0, |m| lookup(&m, value))
    }
}

impl Source for DatSource {
    fn table(&mut self, id: u32) -> Option<Arc<Table>> {
        if let Some(t) = self.tables.get(&id) {
            return t.clone();
        }
        let t = (type_of_id(id) == Some(RecordType::STRING_TABLE))
            .then(|| self.decoded::<StringTable>(id))
            .flatten()
            .map(|t| Arc::new(Table::from(&t)));
        if t.is_some() || self.keeps_absences() {
            self.tables.insert(id, t.clone());
        }
        t
    }

    // D2-14184
    fn language(&mut self) -> Option<Arc<Language>> {
        if let Some(l) = &self.language {
            return l.clone();
        }
        let id = self.data_id_by_enum(LANGUAGE_VALUE, TABLE_MAPPING);
        let l = (type_of_id(id) == Some(RecordType::STRING_STATE))
            .then(|| self.decoded::<StringState>(id))
            .flatten()
            .map(|s| Arc::new(Language::from_state(&s)));
        if l.is_some() || self.keeps_absences() {
            self.language = Some(l.clone());
        }
        l
    }
}
