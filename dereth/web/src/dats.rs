//! The player's data files, opened from whatever the platform reads them through: the four files
//! of the later set, and the two of the set from before Throne of Destiny when the player has them.
//!
//! The container walks its whole directory at open and then reads one block chain per record, all
//! through [`DatStorage`]. In a browser that storage is a synchronous read the worker answers from
//! its own storage, which is why the client runs in a worker: the page's main thread may not block.
//!
//! Which files make the world follows the desktop's rule: a world of an era from before Throne of
//! Destiny is drawn from the older set, with the later files beside it answering the later
//! interface; any other world is drawn from the later set, with the older `portal.dat` beside it
//! for the classic interface and the older looks when the player has it. With no era named, the
//! older set alone opens as the world when the later files are not there.

use std::fmt::Write as _;
use std::path::PathBuf;

use dereth_client_sdk::dat::{ContainerEra, DatError, DatFile, DatStorage, RetailDatStore};

/// The files, in the order a platform numbers them: the later set's four, then the older set's two.
pub const FILE_NAMES: [&str; 6] = [
    "client_portal.dat",
    "client_cell_1.dat",
    "client_local_English.dat",
    "client_highres.dat",
    "portal.dat",
    "cell.dat",
];

const PORTAL: usize = 0;
const CELL: usize = 1;
const LOCAL: usize = 2;
const HIGHRES: usize = 3;
const OLDER_PORTAL: usize = 4;
const OLDER_CELL: usize = 5;

/// What one opened file says about itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileReport {
    pub name: &'static str,
    pub entries: usize,
    pub block_size: u32,
    pub file_size: u32,
    /// How far the file has been patched: the highest iteration in a later file's iteration list,
    /// or the iteration an older file's header carries.
    pub iteration: Option<u32>,
}

impl FileReport {
    /// # Errors
    /// The iteration list's read or decode error.
    pub fn of(name: &'static str, file: &DatFile) -> Result<Self, DatError> {
        let iteration = match file.era() {
            ContainerEra::Modern => file.iteration_list()?.iter().copied().max(),
            ContainerEra::Classic => file.header_iteration(),
        };
        Ok(Self {
            name,
            entries: file.len(),
            block_size: file.header().block_size,
            file_size: file.header().file_size,
            iteration,
        })
    }
}

/// One line per file, for the console.
#[must_use]
pub fn describe(reports: &[FileReport]) -> String {
    let mut out = String::new();
    for r in reports {
        let _ = writeln!(
            out,
            "{:<26} iteration {:>5}  {:>7} entries  block {:#06x}  {} bytes",
            r.name,
            r.iteration
                .map_or_else(|| "-".to_string(), |i| i.to_string()),
            r.entries,
            r.block_size,
            r.file_size,
        );
    }
    out
}

/// Which files make the world, from which are present (by index in [`FILE_NAMES`]) and the dat set
/// the world is drawn from (`world_set`, the era's; `None` when no era is named).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Plan {
    /// The later set is the world, with the older portal (and its cell file) beside it when there.
    Later { older_beside: bool },
    /// The older set is the world, with the later interface files beside it when there.
    Older { later_beside: bool },
}

/// The [`Plan`] for the files `present`, or the index of the first file the world cannot do
/// without.
///
/// # Errors
/// The index of a missing required file.
pub fn plan(present: &[bool; 6], world_set: Option<ContainerEra>) -> Result<Plan, usize> {
    let later = present[PORTAL];
    let older = present[OLDER_PORTAL];
    let older_world = match world_set {
        Some(set) => set == ContainerEra::Classic,
        None => !later && older,
    };
    let missing = |files: &[usize]| files.iter().copied().find(|&i| !present[i]);
    if older_world {
        if let Some(i) = missing(&[OLDER_PORTAL, OLDER_CELL]) {
            return Err(i);
        }
        return Ok(Plan::Older {
            later_beside: present[PORTAL] && present[LOCAL],
        });
    }
    if let Some(i) = missing(&[PORTAL, CELL, LOCAL]) {
        return Err(i);
    }
    Ok(Plan::Later {
        older_beside: older,
    })
}

/// Open the files through `source` (called with each file's index in [`FILE_NAMES`]) for a world
/// drawn from `world_set` ([`plan`]), and report on each. The high-res file is optional, as it is
/// to the client; an older portal beside a later world that will not open is reported and left
/// out, as on the desktop. The client's own records lie over what opens.
///
/// # Errors
/// The first required file that is missing or will not open.
pub fn open_store<S, F>(
    mut source: F,
    world_set: Option<ContainerEra>,
) -> Result<(RetailDatStore, Vec<FileReport>), DatError>
where
    S: DatStorage + 'static,
    F: FnMut(usize) -> Option<S>,
{
    let mut storages: Vec<Option<S>> = (0..FILE_NAMES.len()).map(&mut source).collect();
    let present: [bool; 6] = std::array::from_fn(|i| storages[i].is_some());
    let plan = plan(&present, world_set).map_err(|index| {
        DatError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            FILE_NAMES[index],
        ))
    })?;
    let mut reports = Vec::new();
    let mut open = |index: usize| -> Option<Result<DatFile, DatError>> {
        let s = storages[index].take()?;
        let file = DatFile::from_storage(PathBuf::from(FILE_NAMES[index]), Box::new(s));
        Some(file.and_then(|f| {
            reports.push(FileReport::of(FILE_NAMES[index], &f)?);
            Ok(f)
        }))
    };
    let store = match plan {
        Plan::Later { older_beside } => {
            let portal = open(PORTAL).transpose()?;
            let cell = open(CELL).transpose()?;
            let local = open(LOCAL).transpose()?;
            let (Some(portal), Some(cell), Some(local)) = (portal, cell, local) else {
                unreachable!("the plan found the later set");
            };
            let highres = open(HIGHRES).transpose()?;
            let store = RetailDatStore::open_with(portal, cell, local, highres);
            if older_beside {
                match beside(&mut open, store.clone()) {
                    Ok(with) => with,
                    Err(e) => {
                        tracing::warn!(
                            "the older portal.dat will not open ({e}); the classic interface and \
                             the older grounds, skies and object looks are unavailable"
                        );
                        store
                    }
                }
            } else {
                store
            }
        }
        Plan::Older { later_beside } => {
            let portal = open(OLDER_PORTAL).transpose()?;
            let cell = open(OLDER_CELL).transpose()?;
            let (Some(portal), Some(cell)) = (portal, cell) else {
                unreachable!("the plan found the older set");
            };
            let store = RetailDatStore::open_classic_with(portal, cell)?;
            if later_beside {
                let portal = open(PORTAL).transpose()?;
                let local = open(LOCAL).transpose()?;
                // The later interiors, for drawing only; without them the world still opens.
                let cell = open(CELL).and_then(Result::ok);
                let (Some(portal), Some(local)) = (portal, local) else {
                    unreachable!("the plan found the later interface files");
                };
                store.with_modern_interface(portal, local, cell)?
            } else {
                store
            }
        }
    };
    Ok((
        dereth_client_sdk::runtime::assets::with_client_layers(store),
        reports,
    ))
}

/// `store` with the older portal, and its cell file when there, beside it.
fn beside(
    open: &mut impl FnMut(usize) -> Option<Result<DatFile, DatError>>,
    store: RetailDatStore,
) -> Result<RetailDatStore, DatError> {
    let portal = open(OLDER_PORTAL).transpose()?;
    let cell = open(OLDER_CELL).and_then(Result::ok);
    match portal {
        Some(portal) => store.with_classic_files(portal, cell),
        None => Ok(store),
    }
}

/// A data file the worker reads for the client, named by its index in [`FILE_NAMES`].
///
/// It holds nothing but the index, so it is `Send` and `Sync` as the store requires; the read
/// itself is a call into the worker's script.
#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone, Copy)]
pub struct BrowserFile(pub u32);

#[cfg(target_arch = "wasm32")]
impl DatStorage for BrowserFile {
    fn read_exact_at(&self, offset: u64, buf: &mut [u8]) -> std::io::Result<()> {
        // A dat file is under 4 GiB, so the offset is exact as a JavaScript number.
        #[allow(clippy::cast_precision_loss)]
        let at = offset as f64;
        if crate::browser::dat_read(self.0, at, buf) {
            Ok(())
        } else {
            Err(std::io::ErrorKind::UnexpectedEof.into())
        }
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (which of the player's files make the world; the rule is the desktop's).
    use super::*;

    fn present(names: &[&str]) -> [bool; 6] {
        std::array::from_fn(|i| names.contains(&FILE_NAMES[i]))
    }

    const LATER: [&str; 3] = [
        "client_portal.dat",
        "client_cell_1.dat",
        "client_local_English.dat",
    ];

    #[test]
    fn a_later_world_takes_the_older_portal_beside_it_when_the_player_has_one() {
        assert_eq!(
            plan(&present(&LATER), None),
            Ok(Plan::Later {
                older_beside: false
            })
        );
        let all = [&LATER[..], &["portal.dat", "cell.dat"]].concat();
        assert_eq!(
            plan(&present(&all), Some(ContainerEra::Modern)),
            Ok(Plan::Later { older_beside: true })
        );
        assert_eq!(
            plan(&present(&all), None),
            Ok(Plan::Later { older_beside: true }),
            "no era named: the later files are the world"
        );
        // The older portal alone is enough beside a later world.
        let portal_only = [&LATER[..], &["portal.dat"]].concat();
        assert_eq!(
            plan(&present(&portal_only), None),
            Ok(Plan::Later { older_beside: true })
        );
        assert_eq!(plan(&present(&LATER[..2]), None), Err(LOCAL));
    }

    #[test]
    fn an_older_world_needs_both_older_files_and_takes_the_later_interface_beside_it() {
        let all = [&LATER[..], &["portal.dat", "cell.dat"]].concat();
        assert_eq!(
            plan(&present(&all), Some(ContainerEra::Classic)),
            Ok(Plan::Older { later_beside: true })
        );
        assert_eq!(
            plan(&present(&["portal.dat", "cell.dat"]), None),
            Ok(Plan::Older {
                later_beside: false
            }),
            "the older set alone is the world"
        );
        assert_eq!(
            plan(&present(&LATER), Some(ContainerEra::Classic)),
            Err(OLDER_PORTAL)
        );
        assert_eq!(
            plan(&present(&["portal.dat"]), Some(ContainerEra::Classic)),
            Err(OLDER_CELL)
        );
    }
}
