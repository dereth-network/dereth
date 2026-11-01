//! The decode check: every decoder over every record of a data file, counting what decoded with
//! every byte consumed and what failed, and why.

use std::collections::BTreeMap;

use auberean_dat::container::CELL_SET;
use auberean_dat::{cell_type, type_of_id, DatFile, DecodeError, RecordType};

use crate::{Context, Record};

/// The type of a record of a data file: by its id's low word in a cell file, else by the id chain.
#[must_use]
pub fn type_in(dat: &DatFile, id: u32) -> Option<RecordType> {
    if dat.header().data_set == CELL_SET {
        cell_type(id)
    } else {
        type_of_id(id)
    }
}

/// What one type's records did.
#[derive(Debug, Default, Clone)]
pub struct Tally {
    pub total: usize,
    pub exact: usize,
    /// Failures grouped by message with its digits blanked: how many, and the first id.
    pub failures: BTreeMap<String, (usize, u32)>,
}

impl Tally {
    pub fn add<T>(&mut self, id: u32, r: &Result<T, DecodeError>) {
        self.total += 1;
        match r {
            Ok(_) => self.exact += 1,
            Err(e) => {
                let key: String = e
                    .what
                    .chars()
                    .map(|c| if c.is_ascii_digit() { '#' } else { c })
                    .collect();
                self.failures.entry(key).or_insert((0, id)).0 += 1;
            }
        }
    }

    pub fn merge(&mut self, o: &Self) {
        self.total += o.total;
        self.exact += o.exact;
        for (k, (n, id)) in &o.failures {
            self.failures.entry(k.clone()).or_insert((0, *id)).0 += n;
        }
    }

    /// Whether every record decoded.
    #[must_use]
    pub fn all_exact(&self) -> bool {
        self.exact == self.total
    }
}

/// Decode every record of a data file (or only those of one type) and tally the results by type.
/// A record that cannot be read from the container counts as a failure of its type.
#[must_use]
pub fn verify_dat(
    dat: &DatFile,
    ctx: &Context,
    only: Option<RecordType>,
) -> BTreeMap<RecordType, Tally> {
    let mut out: BTreeMap<RecordType, Tally> = BTreeMap::new();
    for e in dat.entries() {
        let Some(ty) = type_in(dat, e.id) else {
            continue;
        };
        if only.is_some_and(|o| o != ty) || !Record::has_decoder(ty) {
            continue;
        }
        let r = match dat.read_entry(e) {
            Ok(bytes) => Record::decode(ty, e.id, &bytes, ctx).map(|r| r.map(drop)),
            Err(err) => Some(Err(DecodeError {
                at: 0,
                what: format!("read: {err}"),
            })),
        };
        if let Some(r) = r {
            out.entry(ty).or_default().add(e.id, &r);
        }
    }
    out
}

/// Decode every record of one type in a data file, reporting each failure with its id.
pub fn failures_of(dat: &DatFile, ctx: &Context, ty: RecordType) -> Vec<(u32, DecodeError)> {
    let mut v = Vec::new();
    for e in dat.entries() {
        if type_in(dat, e.id) != Some(ty) {
            continue;
        }
        let r = match dat.read_entry(e) {
            Ok(bytes) => Record::decode(ty, e.id, &bytes, ctx).map(|r| r.map(drop)),
            Err(err) => Some(Err(DecodeError {
                at: 0,
                what: format!("read: {err}"),
            })),
        };
        if let Some(Err(err)) = r {
            v.push((e.id, err));
        }
    }
    v
}
