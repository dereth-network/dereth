//! The dat exchange. The server pushes what the client lacks ahead of the character list and the 1999
//! begin names only counts, while the later begin names every record, so the relay holds the whole
//! push and then serves it through the later download messages: the overlay manifest, the begin, each
//! record and the end. What Dereth receives lands in its overlay for this world, never its files.
//!
//! The server's iterations stay the overlay's own: they may be numbers of the server's own (10007),
//! which say nothing about the files beneath, so the manifest marks them as the world's whole list.

use dereth_dat::overlay::record_hash;
use dereth_protocol::admin::{
    DddBeginDdd, DddData, DddInterrogation, DddInterrogationResponse, DddOverlayManifest,
    MostlyConsecutiveIntSet, OverlayFileManifest, OverlayRecord, PatchRevision,
};

use crate::codec::dat::{CellPair, CellRecord, DatBegin, DatFileKind, DatRecord, Packed};

/// The later wire pair of each file.
const PORTAL_PAIR: (u32, u32) = (0, 1);
const CELL_PAIR: (u32, u32) = (1, 2);
/// A record's version when the server names none.
const RECORD_VERSION: u32 = 1;

/// The newest iteration in a client's list, which is what the 1999 login reports. The list's count is
/// not it: a file at 102 with the server's 105 in its overlay holds 103 iterations.
///
/// A negative `x` opens a run of `|x|` iterations from the value that follows it.
#[must_use]
pub fn newest_iteration(set: &MostlyConsecutiveIntSet) -> u32 {
    let mut newest = 0;
    let mut run = 1;
    for &x in &set.ints {
        if x < 0 {
            run = x.unsigned_abs();
        } else {
            newest = newest.max(x.cast_unsigned().saturating_add(run - 1));
            run = 1;
        }
    }
    newest
}

/// What the client said it has, from its answer to the interrogation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ClientFiles {
    pub portal: u32,
    pub cell: u32,
    pub portal_base: [u8; 32],
    pub cell_base: [u8; 32],
}

impl ClientFiles {
    /// What an answer says.
    #[must_use]
    pub fn of(answer: &DddInterrogationResponse) -> Self {
        let count = |pair: (u32, u32)| {
            answer
                .iters_with_keys
                .iter()
                .find(|l| (l.dat_file_type, l.dat_file_id) == pair)
                .map_or(0, |l| newest_iteration(&l.iterations))
        };
        let base = |pair: (u32, u32)| {
            answer
                .overlay_bases
                .iter()
                .find(|b| (b.dat_file_type, b.dat_file_id) == pair)
                .map_or([0; 32], |b| b.fingerprint)
        };
        Self {
            portal: count(PORTAL_PAIR),
            cell: count(CELL_PAIR),
            portal_base: base(PORTAL_PAIR),
            cell_base: base(CELL_PAIR),
        }
    }
}

/// The interrogation the relay asks with: the same one ACE's server asks (region 1, language 1,
/// product 1, which offers no high-resolution file, and languages Invalid and English).
#[must_use]
pub fn interrogation() -> DddInterrogation {
    DddInterrogation {
        servers_region: 1,
        name_rule_language: 1,
        product_id: 1,
        supported_languages: vec![0, 1],
    }
}

/// One record inflated and ready to serve.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Held {
    kind: DatFileKind,
    id: u32,
    bytes: Vec<u8>,
}

/// The push as it arrives.
#[derive(Debug, Default)]
pub struct Push {
    begin: Option<DatBegin>,
    records: Vec<Held>,
    /// Records that did not inflate to their stated length.
    pub refused: u32,
    /// Records held back because Dereth's download refuses their ids.
    pub withheld: Vec<u32>,
}

/// Whether Dereth's download takes a record of this id into a 1999 file. A record it refuses stays
/// pending, and the download never ends: such a record is held back, so the player keeps the file's
/// own copy of it, and the session goes on.
#[must_use]
pub fn dereth_takes(kind: DatFileKind, id: u32) -> bool {
    let id = dereth_primitives::DataId(id);
    match kind {
        DatFileKind::Cell => dereth_dat::classify_cell_id(id).is_some(),
        DatFileKind::Portal => dereth_dat::divine_type_in(dereth_dat::ContainerEra::Classic, id)
            .is_some_and(|t| t.dat() != dereth_dat::DatKind::None),
    }
}

fn inflate(data: &Packed) -> Option<Vec<u8>> {
    let want = usize::try_from(data.inflated_len).ok()?;
    match dereth_dat::inflate_zlib(&data.bytes, want) {
        Ok(bytes) => Some(bytes),
        Err(e) => {
            log::warn!("a dat record did not inflate: {e}");
            None
        }
    }
}

impl Push {
    /// A begin. The server sends several; the first says what the rest of the push is.
    pub fn begin(&mut self, begin: DatBegin) {
        self.begin.get_or_insert(begin);
    }

    /// One pushed record.
    pub fn record(&mut self, record: &DatRecord) {
        self.keep(record.file(), record.id, &record.data);
    }

    fn keep(&mut self, kind: DatFileKind, id: u32, data: &Packed) {
        if !dereth_takes(kind, id) {
            log::warn!("dat record {id:#010x} held back: Dereth's download refuses this id");
            self.withheld.push(id);
            return;
        }
        match inflate(data) {
            Some(bytes) => self.records.push(Held { kind, id, bytes }),
            None => self.refused += 1,
        }
    }

    /// How many records are held.
    #[must_use]
    pub fn held(&self) -> usize {
        self.records.len()
    }

    /// The iterations the push brings the client to.
    #[must_use]
    pub fn reaches(&self, client: &ClientFiles) -> (u32, u32) {
        self.begin.map_or((client.portal, client.cell), |b| {
            (b.portal_iteration, b.cell_iteration)
        })
    }

    /// The push as the later download: nothing when the client is up to date.
    #[must_use]
    pub fn serve(self, world_key: &str, client: &ClientFiles) -> Serve {
        let (portal, cell) = self.reaches(client);
        let mut files = Vec::new();
        let mut revisions = Vec::new();
        let mut total: u64 = 0;
        for (kind, pair, name, iteration, base, held) in [
            (
                DatFileKind::Portal,
                PORTAL_PAIR,
                "portal.dat",
                portal,
                client.portal_base,
                client.portal,
            ),
            (
                DatFileKind::Cell,
                CELL_PAIR,
                "cell.dat",
                cell,
                client.cell_base,
                client.cell,
            ),
        ] {
            let ours: Vec<&Held> = self.records.iter().filter(|r| r.kind == kind).collect();
            if ours.is_empty() && iteration == held {
                continue;
            }
            let bytes = ours.iter().map(|r| r.bytes.len() as u64).sum::<u64>();
            total += bytes;
            files.push(OverlayFileManifest {
                dat_file_type: pair.0,
                dat_file_id: pair.1,
                base_name: name.to_owned(),
                base_fingerprint: base,
                base_records: None,
                base_iterations: held,
                exact_iterations: true,
                revisions: vec![iteration],
                records: ours
                    .iter()
                    .map(|r| OverlayRecord {
                        id: r.id,
                        iteration,
                        size: u32::try_from(r.bytes.len()).unwrap_or(u32::MAX),
                        sha256: record_hash(&r.bytes),
                    })
                    .collect(),
                tombstones: Vec::new(),
            });
            revisions.push(PatchRevision {
                dat_file_type: pair.0,
                dat_file_id: pair.1,
                iteration,
                ids_to_download: ours.iter().map(|r| r.id).collect(),
                ids_to_purge: Vec::new(),
            });
        }
        let data_expected = u32::try_from(total).unwrap_or(u32::MAX);
        let manifest = DddOverlayManifest {
            world_key: world_key.to_owned(),
            total_bytes: data_expected,
            files,
        };
        let begin = (!revisions.is_empty()).then_some(DddBeginDdd {
            data_expected,
            revisions,
        });
        let mut data: Vec<DddData> = Vec::with_capacity(self.records.len());
        for r in self.records {
            let (pair, iteration) = match r.kind {
                DatFileKind::Portal => (PORTAL_PAIR, portal),
                DatFileKind::Cell => (CELL_PAIR, cell),
            };
            data.push(later_record(pair, 0, r.id, iteration, r.bytes));
        }
        Serve {
            manifest,
            begin,
            data,
        }
    }
}

/// A record on the later wire, stored whole rather than compressed.
#[must_use]
pub fn later_record(
    pair: (u32, u32),
    resource_type: u32,
    id: u32,
    iteration: u32,
    bytes: Vec<u8>,
) -> DddData {
    DddData {
        dat_file_type: pair.0,
        dat_file_id: pair.1,
        resource_type,
        resource_id: id,
        iteration,
        compressed: 0,
        version: RECORD_VERSION,
        data_size: u32::try_from(bytes.len() + 4).unwrap_or(u32::MAX),
        data: bytes,
    }
}

/// The download to serve.
#[derive(Debug, Clone, PartialEq)]
pub struct Serve {
    pub manifest: DddOverlayManifest,
    /// The later begin, when there is anything to do.
    pub begin: Option<DddBeginDdd>,
    pub data: Vec<DddData>,
}

/// A record the client asked for during play, on the later wire. Without a request it answers, it is
/// still the cell file's: the answer keys by the id.
#[must_use]
pub fn answer_cell(resource_type: u32, record: &CellRecord, iteration: u32) -> Option<DddData> {
    let bytes = inflate(&record.data)?;
    Some(later_record(
        CELL_PAIR,
        resource_type,
        record.id,
        iteration,
        bytes,
    ))
}

/// Both records of a pair, on the later wire.
#[must_use]
pub fn answer_pair(resource_type: u32, pair: &CellPair, iteration: u32) -> Vec<DddData> {
    pair.first
        .iter()
        .chain([&pair.second])
        .filter_map(|r| answer_cell(resource_type, r, iteration))
        .collect()
}

/// The 1999 cache a cell id's kind of record is kept in: its landblock (`…FFFF`), its information
/// (`…FFFE`) or one of its cells.
#[must_use]
pub fn cell_cache(id: u32) -> u32 {
    match id & 0xFFFF {
        0xFFFF => 0,
        0xFFFE => 1,
        _ => 2,
    }
}

#[cfg(test)]
mod tests;
