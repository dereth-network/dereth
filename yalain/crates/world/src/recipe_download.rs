//! The world's own recipes ([`crate::recipe_rows`]) sent to a retail client in the login's data
//! download.
//!
//! The September 2013 client reads a recipe from its record whenever its crafting page lists one,
//! and never asks the server for a recipe's record its files lack (it asks only for text, pictures,
//! the landscape and sounds). A recipe it was taught and holds no record of is left out of the page,
//! its category with it. So a retail client is sent the world's recipes unasked, as the login's
//! download sends a world's records: their ids under an iteration of the portal file, then each
//! record, and its characters once it has stored them.
//!
//! - **Only when the operator patches retail clients** (`[ddd] enable_dat_patching`): a client that
//!   opens its files only to read them can't store a record, and stops. Off, a retail client is sent
//!   none and doesn't see the world's recipes.
//! - **Not to a client that keeps overlays**: it asks for a record it lacks, and is answered
//!   ([`crate::download`]).
//! - **Once for each set of recipes**: the iteration is the server's own (as the admin's script
//!   library's), one of [`ITERATIONS`] the records choose. A client listing it is sent nothing; one
//!   holding an earlier set's is sent the whole set again.

use std::collections::HashMap;

use auberean_protocol::login::{DataHeld, DataRecord, DownloadBegins, MissingIterations};
use auberean_protocol::types::{IterationRun, IterationSet};
use auberean_protocol::ServerMessage;
use sha2::{Digest, Sha256};

use crate::download::{packed, PORTAL_FILE};

/// The kind of a saved script state, a recipe's record.
pub const KIND: u32 = 0x4000_002F;

/// The iterations a set of recipes may go out as: the server's own, past the admin's script
/// library's.
pub const FIRST_ITERATION: u32 = 9_100;
pub const ITERATIONS: u32 = 900;

/// The world's recipes as a client is sent them: their iteration, what the download begins with,
/// and the records.
#[derive(Debug, Clone, PartialEq)]
pub struct RecipeDownload {
    pub iteration: u32,
    pub begins: ServerMessage,
    pub records: Vec<ServerMessage>,
}

/// The iteration a set of records goes out as: the same records, the same iteration.
#[must_use]
pub fn iteration(records: &HashMap<u32, Vec<u8>>) -> u32 {
    let mut ids: Vec<&u32> = records.keys().collect();
    ids.sort_unstable();
    let mut h = Sha256::new();
    for id in ids {
        h.update(id.to_le_bytes());
        h.update((records[id].len() as u64).to_le_bytes());
        h.update(&records[id]);
    }
    let d = h.finalize();
    FIRST_ITERATION + u32::from_le_bytes([d[0], d[1], d[2], d[3]]) % ITERATIONS
}

/// The download of these records; `None` when there are none.
#[must_use]
pub fn of(records: &HashMap<u32, Vec<u8>>) -> Option<RecipeDownload> {
    if records.is_empty() {
        return None;
    }
    let iteration = iteration(records);
    let mut ids: Vec<u32> = records.keys().copied().collect();
    ids.sort_unstable();
    let packed: Vec<DataRecord> = ids
        .iter()
        .map(|id| packed(PORTAL_FILE, KIND, *id, &records[id]))
        .collect();
    let bytes: usize = packed.iter().map(|r| r.bytes.len()).sum();
    Some(RecipeDownload {
        iteration,
        begins: ServerMessage::DownloadBegins(DownloadBegins {
            bytes: u32::try_from(bytes).unwrap_or(u32::MAX),
            files: vec![MissingIterations {
                dat_file: PORTAL_FILE,
                iteration,
                download: ids,
                purge: vec![],
            }],
        }),
        records: packed.into_iter().map(ServerMessage::DataRecord).collect(),
    })
}

/// What a client told of its data, with an iteration of the portal file it has stored since.
#[must_use]
pub fn with_iteration(held: &DataHeld, iteration: u32) -> DataHeld {
    let mut held = held.clone();
    let one = IterationRun::One(iteration.cast_signed());
    match held.files.iter_mut().find(|f| f.dat_file == PORTAL_FILE) {
        Some(f) => {
            f.iterations.total += 1;
            f.iterations.runs.push(one);
        }
        None => held
            .files
            .push(auberean_protocol::login::DataFileIterations {
                dat_file: PORTAL_FILE,
                iterations: IterationSet {
                    total: 1,
                    runs: vec![one],
                },
            }),
    }
    held
}
