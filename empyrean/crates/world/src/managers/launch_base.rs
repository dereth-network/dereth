//! Not ACE (V520): a world on the October 31, 1999 portal file patches a Dereth client holding the
//! retail CD's portal file of October 9, 1999 up to the later file, as ac1999's own server patches
//! a client on either: the records the later file adds or changes, in the revisions between the
//! two files' iterations, beneath the world's own data overlay when it has one. Both clients end
//! up reading the same records with the same iterations.
//!
//! The server holds only its own file. What it knows of the earlier one is a [`BaseStep`]: how a
//! client's copy is known (its fingerprint and its records' digest), its iteration, and the
//! records in which the two files differ. The earlier file holds no record the later one lacks,
//! so every one of them is the later file's to send, and nothing is deleted.

use dereth_primitives::{ContainerEra, EraId};
use dereth_protocol::admin as proto;
use empyrean_dat::{DatDatabase, DatDatabaseType, DatManager};

use crate::managers::ddd_manager::{self, DddManagerState, MissingIterations};

/// One portal file a step goes from or to, as the overlay extension names a base: its
/// fingerprint (the SHA-256 of its header and directory), the digest of its records, and the
/// iteration its header carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BaseFile {
    pub fingerprint: [u8; 32],
    pub records: [u8; 32],
    pub iteration: u32,
    /// What a player is told the file is.
    pub name: &'static str,
}

/// A client on `from` is patched up to a world on `to` with the world's own copies of `added`
/// and `changed`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BaseStep {
    pub from: BaseFile,
    pub to: BaseFile,
    /// The records `to` holds and `from` does not.
    pub added: &'static [u32],
    /// The records both hold, otherwise.
    pub changed: &'static [u32],
}

const fn hash(hex: &str) -> [u8; 32] {
    const fn digit(c: u8) -> u8 {
        match c {
            b'0'..=b'9' => c - b'0',
            b'a'..=b'f' => c - b'a' + 10,
            _ => panic!("not a lower-case hex digit"),
        }
    }
    let bytes = hex.as_bytes();
    assert!(bytes.len() == 64, "a SHA-256 is 64 hex digits");
    let mut out = [0u8; 32];
    let mut i = 0;
    while i < 32 {
        out[i] = digit(bytes[2 * i]) << 4 | digit(bytes[2 * i + 1]);
        i += 1;
    }
    out
}

/// The retail CD's portal file (October 9, 1999, iteration 102) up to the one of October 31, 1999
/// (iteration 105), which adds a world event's records and changes eleven others.
pub const CD_TO_OCTOBER_31: BaseStep = BaseStep {
    from: BaseFile {
        fingerprint: hash("2648dac8102e09f6a9b64aedd377651f0177964fc11a3cb5e0668a6d7e4b884e"),
        records: hash("4220e659d88d46cf603486c2c44f81e67b6693a91a777c01ad366a02d809315a"),
        iteration: 102,
        name: "the retail CD's portal.dat",
    },
    to: BaseFile {
        fingerprint: hash("fc920ff5a627875a5d4e59c86b106fe4468225100a7d07537e42489af51bd99b"),
        records: hash("9fc149b143a4dd221844a0e5e5e627b92634d1f72979596d0915f22d7ba24c8a"),
        iteration: 105,
        name: "the October 31, 1999 portal.dat",
    },
    added: &[
        0x0100_1932,
        0x0100_19CC,
        0x0100_19CD,
        0x0100_19CE,
        0x0100_19CF,
        0x0100_19D1,
        0x0100_19D2,
        0x0100_19D3,
        0x0200_0639,
        0x0200_06A3,
        0x0200_06F9,
        0x0200_06FA,
        0x0400_0EBE,
        0x0500_186F,
        0x0500_1870,
        0x0600_1B42,
        0x0800_14B4,
        0x0800_14B6,
        0x0900_008F,
        0x0D00_044F,
        0x2000_0056,
        0x3200_040B,
        0x3200_040C,
        0x3200_040D,
        0x3200_040E,
        0x3200_040F,
        0x3200_0410,
        0x3200_0412,
        0x3200_0413,
        0x3200_0414,
        0x3200_0415,
        0x3200_0416,
        0x3200_0439,
        0x3200_043B,
        0x3200_043C,
        0x3200_043D,
        0x3300_0782,
        0x3300_07A5,
        0x3300_07A6,
        0x3300_07A7,
        0x3300_07A8,
        0x3400_0070,
    ],
    changed: &[
        0x0200_04C7,
        0x1100_174E,
        0x1100_192B,
        0x1300_0000,
        0x130F_0000,
        0x2000_0005,
        0x2000_0007,
        0x2000_0032,
        0x2000_0050,
        0x3200_038F,
        0x3300_06DF,
    ],
};

const STEPS: [BaseStep; 1] = [CD_TO_OCTOBER_31];

/// The retail CD's cell file (October 9, 1999, iteration 67), which every 1999 portal file is
/// played with, known by its fingerprint.
const CD_CELL: ([u8; 32], &str) = (
    hash("26eed07eb2de543fab13c1d9278b316d0c5427954151cd9d825ab83b29aaa66a"),
    "the retail CD's cell.dat",
);

/// A file's own iterations, whatever overlay is over it.
fn own_iterations(db: &DatDatabase) -> u32 {
    db.overlay().map_or_else(
        || db.iteration().cast_unsigned(),
        |layer| layer.manifest().base_iterations,
    )
}

/// The files a world on 1999 files plays, as a player is told them, or `None` for a world on no
/// Launch-era files: a file of a step's and the retail CD's cell file are named by their
/// fingerprints, any other by its iteration.
#[must_use]
pub fn world_files(dats: &DatManager) -> Option<String> {
    let (portal, cell) = (dats.portal_dat(), dats.cell_dat());
    if portal.container_era() != ContainerEra::Classic {
        return None;
    }
    let (p, c) = (own_iterations(portal), own_iterations(cell));
    if !EraId::Launch
        .expected_dats()
        .admits(ContainerEra::Classic, Some(p), Some(c))
    {
        return None;
    }
    let portal_file = portal.base_fingerprint().and_then(|f| {
        STEPS
            .iter()
            .flat_map(|s| [s.from, s.to])
            .find(|b| b.fingerprint == f)
    });
    let cd_cell = cell.base_fingerprint() == Some(CD_CELL.0);
    Some(match portal_file {
        Some(b) if cd_cell && b == CD_TO_OCTOBER_31.from => "the retail CD's data files".to_owned(),
        _ => format!(
            "{} and {}",
            portal_file.map_or_else(
                || format!("portal.dat iteration {p}"),
                |b| b.name.to_owned()
            ),
            if cd_cell {
                CD_CELL.1.to_owned()
            } else {
                format!("cell.dat iteration {c}")
            }
        ),
    })
}

/// Whether a client holding `portal` and `cell` iterations holds the files the world's data
/// overlay was made against, when it has one.
#[must_use]
pub fn holds_overlay_base(dats: &DatManager, portal: i32, cell: i32) -> bool {
    let (p, c) = (dats.portal_dat(), dats.cell_dat());
    (p.overlay().is_some() || c.overlay().is_some())
        && (portal, cell)
            == (
                own_iterations(p).cast_signed(),
                own_iterations(c).cast_signed(),
            )
}

/// Not ACE (V520): fill the patch state with what patching a client on `step.from` up to the
/// world takes, when nothing filled it at start. A world that patches no other client builds it
/// when the first such client asks, never at start nor for a client on the world's own file: the
/// portal file's iterations, and the records of the step's revisions and those the step sends,
/// sized and compressed. Answers whether it built it.
pub fn prepare(step: &BaseStep, dats: &DatManager, state: &mut DddManagerState) -> bool {
    if state.iterations.contains_key(&DatDatabaseType::Portal) {
        return false;
    }
    let started = std::time::Instant::now();
    let sent: std::collections::HashSet<u32> = step.sent(dats).collect();
    let revisions = step.revisions();
    let n = ddd_manager::init_iterations_of(
        state,
        DatDatabaseType::Portal,
        dats.portal_dat(),
        |id, iteration| revisions.contains(&iteration) || sent.contains(&id),
    );
    log::info!(
        "[DDD] prepared the patch from {} up to this world's portal.dat: {n} records in {} ms",
        step.from.name,
        started.elapsed().as_millis()
    );
    true
}

impl BaseStep {
    /// The revisions between the two files, which a client on `from` lacks.
    pub fn revisions(&self) -> std::ops::RangeInclusive<u32> {
        self.from.iteration + 1..=self.to.iteration
    }

    /// The records the step sends over `dats`: every record the files differ in, but those the
    /// world's own overlay holds or deletes, which its own revisions carry.
    pub fn sent<'a>(&'a self, dats: &'a DatManager) -> impl Iterator<Item = u32> + 'a {
        let portal = dats.portal_dat();
        let layer = portal.overlay();
        self.added
            .iter()
            .chain(self.changed)
            .copied()
            .filter(move |id| {
                !layer.is_some_and(|l| l.touches(dereth_primitives::DataId(*id)))
                    && portal.contains_file(*id)
            })
    }

    /// The manifest's entries for the records the step sends: each as the world reads it.
    #[must_use]
    pub fn manifest_records(&self, dats: &DatManager) -> Vec<proto::OverlayRecord> {
        let portal = dats.portal_dat();
        self.sent(dats)
            .filter_map(|id| {
                let bytes = portal.get_reader_for_file(id)?;
                Some(proto::OverlayRecord {
                    id,
                    iteration: portal.file_iteration(id).unwrap_or_default(),
                    size: u32::try_from(bytes.len()).unwrap_or(u32::MAX),
                    sha256: empyrean_dat::data_overlay::record_hash(&bytes),
                })
            })
            .collect()
    }
}

/// Put the records `step` sends into the patch `missing`, in the step's last revision: answers the
/// bytes they add to it, counted as the patch counts its own. A record the patch state has no size
/// for is left out, as it could not be sent.
pub fn add_to_patch(
    step: &BaseStep,
    dats: &DatManager,
    state: &DddManagerState,
    missing: &mut MissingIterations,
) -> u32 {
    let sizes = state.dat_file_sizes.get(&DatDatabaseType::Portal);
    let mut bytes: u32 = 0;
    let mut ids = Vec::new();
    for id in step.sent(dats) {
        let Some(size) = sizes.and_then(|s| s.get(&id)) else {
            continue;
        };
        bytes = bytes.wrapping_add(if size.compressed_file_size > 0 {
            size.compressed_file_size.cast_unsigned().wrapping_add(4)
        } else {
            size.uncompressed_file_size.cast_unsigned()
        });
        ids.push(id);
    }
    if ids.is_empty() {
        return 0;
    }
    let files = missing
        .entry(DatDatabaseType::Portal)
        .or_default()
        .get_or_insert_with(step.to.iteration, Vec::new);
    for id in ids {
        if !files.contains(&id) {
            files.push(id);
        }
    }
    files.sort_unstable();
    bytes
}

/// The step this world patches a client up to its portal file by: the one whose later file the
/// world's portal file is, when the world's overlay, if it has one, counts its revisions past the
/// file's own iterations.
#[must_use]
pub fn of_world(dats: &DatManager) -> Option<&'static BaseStep> {
    let portal = dats.portal_dat();
    if portal.container_era() != ContainerEra::Classic {
        return None;
    }
    if portal
        .overlay()
        .is_some_and(|l| l.manifest().exact_iterations)
    {
        return None;
    }
    let held = portal.base_fingerprint()?;
    STEPS.iter().find(|s| s.to.fingerprint == held)
}

/// The iterations a list names: each run of `-n, first` is `first .. first + n - 1`, every other
/// entry one iteration.
fn listed(set: &proto::MostlyConsecutiveIntSet) -> Vec<u32> {
    let mut out = Vec::new();
    let mut ints = set.ints.iter().copied();
    while let Some(i) = ints.next() {
        if i < 0 {
            let Some(first) = ints.next() else {
                break;
            };
            let Ok(first) = u32::try_from(first) else {
                continue;
            };
            let n = i.unsigned_abs();
            out.extend(first..first.saturating_add(n));
        } else if let Ok(i) = u32::try_from(i) {
            out.push(i);
        }
    }
    out
}

/// The step a client that keeps overlays is patched up to this world's portal file by: when the
/// world is on a step's later file and the client holds the earlier one, known by its layout, or,
/// for a client that checks its base by its records itself, by its portal list holding the
/// earlier file's iterations and none of the step's revisions.
#[must_use]
pub fn for_client(
    dats: &DatManager,
    response: &proto::DddInterrogationResponse,
) -> Option<&'static BaseStep> {
    if response.flags & proto::DddInterrogationResponse::FLAG_OVERLAY == 0 {
        return None;
    }
    let step = of_world(dats)?;
    let held = response
        .overlay_bases
        .iter()
        .find(|b| (b.dat_file_type, b.dat_file_id) == (0, 1))?;
    if held.fingerprint == step.from.fingerprint {
        return Some(step);
    }
    if held.fingerprint == step.to.fingerprint
        || response.flags & proto::DddInterrogationResponse::FLAG_RECORDS == 0
    {
        return None;
    }
    let list = response
        .iters_with_keys
        .iter()
        .find(|l| (l.dat_file_type, l.dat_file_id) == (0, 1))?;
    let have = listed(&list.iterations);
    let earlier = (1..=step.from.iteration).all(|i| have.contains(&i));
    let later = step.revisions().any(|i| have.contains(&i));
    (earlier && !later).then_some(step)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Behaviour: none (tooling: the step's own table)
    #[test]
    fn the_step_names_each_record_once_and_its_revisions_lie_between_the_files() {
        let step = CD_TO_OCTOBER_31;
        let mut all: Vec<u32> = step.added.iter().chain(step.changed).copied().collect();
        let n = all.len();
        all.sort_unstable();
        all.dedup();
        assert_eq!((n, all.len()), (53, 53));
        assert_eq!(step.revisions().collect::<Vec<_>>(), [103, 104, 105]);
        assert_ne!(step.from.fingerprint, step.to.fingerprint);
    }

    /// Behaviour: none (tooling: reading a client's iteration list)
    #[test]
    fn a_list_names_its_runs_and_its_single_iterations() {
        let set = proto::MostlyConsecutiveIntSet {
            iterations: 6,
            ints: vec![-3, 1, 7, -2, 10],
        };
        assert_eq!(listed(&set), [1, 2, 3, 7, 10, 11]);
    }
}
