//! The world's overlay folder: where a world's own records are kept, which worlds' overlays the
//! player refuses, and laying the overlay over the locked files.
//!
//! **A deliberate divergence from retail** (client divergence CD-031): retail patches its own
//! data files; this client keeps everything a world changes in an overlay folder of that world's
//! ([`dereth_dat::overlay`]) and never writes the files a player installed.
//!
//! - **Where.** `--overlay-dat-dir <dir>` names the folder for the run: it holds one world's
//!   overlay, and a world announcing another name is refused. Without it, a client that connects
//!   keeps a folder per server in the per-user cache, `overlays/<host>-<port>`, and in it one
//!   folder for each world the server has served, named by the world
//!   ([`world_folder_name`](crate::world_overlay::world_folder_name)), with `world.txt` naming
//!   the one it served last ([`LAST_WORLD_FILE`](crate::world_overlay::LAST_WORLD_FILE)). The
//!   run starts on that world's folder; when the server names another world during the data
//!   patch, that world's folder is used from then on, made when it is new. A server folder of an
//!   earlier client, which held one world's overlay itself, is moved into that world's folder the
//!   first time.
//! - **Which world.** The containers name the world they belong to, and a folder whose
//!   containers name another world than the one it is for is refused.
//! - **The blocklist.** `overlay-blocklist.txt` beside the preferences file, one world name a
//!   line: those worlds' overlays are never opened or written, wherever their folder is.
//! - **A refusal ends the session.** A world whose overlay the client cannot take is not played
//!   with the installed files instead. A refusal the client can tell at the start (the folder's
//!   world is blocked, holds another world's data, was made for other files, or cannot be read)
//!   is made before it connects; one only the server can tell (the world is blocked, its overlay
//!   was made for other files, a record is not the one it names) is made during the data patch,
//!   before character select. Either way the player is told why
//!   ([`Refusal::message`](crate::world_overlay::Refusal::message)) and the session ends there.
//! - **Where it is kept.** On disk, unless the host keeps its folders elsewhere
//!   ([`install_folders`](crate::world_overlay::install_folders)): a browser keeps them in its
//!   own storage, and the overlay is the same.

use std::cell::Cell;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use dereth_dat::overlay::{OverlayDir, OverlayError};
use dereth_dat::{DatFolder, RetailDatStore};

use crate::config::Config;
use crate::platform::files as host_files;

/// The blocklist's file name, beside the preferences file.
pub const BLOCKLIST_FILE: &str = dereth_client_contract::persist::overlay_blocklist::FILE;

/// In a server's folder, the file naming the world the server served last: the world's name, on
/// one line.
pub const LAST_WORLD_FILE: &str = "world.txt";

/// Why the client did not take a world's overlay, as the player is told it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// The world is on the player's overlay blocklist.
    Blocked { world: String },
    /// The folder holds the overlay of another world, `held`.
    OtherWorld { folder: PathBuf, held: String },
    /// The overlay was made against other data files than the player's.
    OtherFiles,
    /// The run keeps no folder for the world's overlay.
    NoFolder,
    /// A record the server sent is not the one the world's overlay names.
    BadRecord,
    /// The folder's files could not be read as an overlay.
    Unreadable,
    /// The server named another world than the one this client answered for, and patched it as
    /// that one: what it sent is not this world's whole overlay.
    WorldChanged,
}

impl Refusal {
    /// The refusal an error laying or writing the overlay stands for.
    #[must_use]
    pub fn from_error(e: &OverlayError) -> Self {
        match e {
            OverlayError::BaseMismatch { .. } => Self::OtherFiles,
            OverlayError::OtherWorld { path, found, .. } => Self::OtherWorld {
                folder: path
                    .parent()
                    .map_or_else(|| path.clone(), Path::to_path_buf),
                held: found.clone(),
            },
            _ => Self::Unreadable,
        }
    }

    /// What the player is told: that the world's data was refused, and why.
    #[must_use]
    pub fn message(&self) -> String {
        let why = match self {
            Self::Blocked { world } => format!("\"{world}\" is on your overlay blocklist"),
            Self::OtherWorld { held, .. } => {
                format!("its folder holds another world's data (\"{held}\")")
            }
            Self::OtherFiles => "it was made for other game files than yours".to_owned(),
            Self::NoFolder => "there is no folder to keep it in".to_owned(),
            Self::BadRecord => "the server sent data that does not match it".to_owned(),
            Self::Unreadable => "its folder could not be read".to_owned(),
            Self::WorldChanged => {
                return "This server now hosts another world. Connect again to update its data."
                    .to_owned()
            }
        };
        format!("This world's data was refused: {why}.")
    }
}

/// Where a host keeps the overlay folder named by a path: its storage for that folder, or `None`
/// when it keeps none there.
pub type FolderOpener = fn(&Path) -> Option<Arc<dyn DatFolder>>;

thread_local! {
    static FOLDERS: Cell<Option<FolderOpener>> = const { Cell::new(None) };
}

/// Keep this thread's overlay folders where `open` says, rather than on disk. A host with no disk
/// the client can write by path (a browser) installs one before the client starts.
pub fn install_folders(open: FolderOpener) {
    FOLDERS.with(|f| f.set(Some(open)));
}

/// The overlay folder at `dir`: on disk, or in the host's storage when it keeps its folders there.
///
/// # Errors
/// [`OverlayError::BaseFolder`] for a folder holding base files, and an I/O error when the host
/// keeps no folder at `dir`.
pub fn open_folder(dir: &Path) -> Result<OverlayDir, OverlayError> {
    match FOLDERS.with(Cell::get) {
        None => OverlayDir::new(dir),
        Some(open) => match open(dir) {
            Some(folder) => OverlayDir::in_folder(folder),
            None => Err(OverlayError::Dat(dereth_dat::DatError::Io(
                std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("{} is not kept here", dir.display()),
                ),
            ))),
        },
    }
}

/// The folder this run lays the world's overlay from at the start: `--overlay-dat-dir`, else, when
/// the client connects, the folder in its server's folder ([`address_folder`]) of the world the
/// server served last (the server's own name for a server that has named none); `None` for a run
/// with neither.
#[must_use]
pub fn overlay_dir(cfg: &Config) -> Option<PathBuf> {
    if let Some(d) = &cfg.overlay_dat_dir {
        return Some(d.clone());
    }
    let root = address_folder(cfg)?;
    adopt_single_world(&root);
    let world = last_world(&root).unwrap_or_else(|| address_key(cfg));
    Some(root.join(world_folder_name(&world)))
}

/// The server's folder in the per-user cache, `overlays/<host>-<port>`, which holds a folder for
/// each world the server has served: `None` for a run that names its folder
/// (`--overlay-dat-dir`), and for one that does not connect.
#[must_use]
pub fn address_folder(cfg: &Config) -> Option<PathBuf> {
    if cfg.overlay_dat_dir.is_some() || !cfg.connect {
        return None;
    }
    Some(
        host_files::cache_dir()?
            .join("overlays")
            .join(folder_name(&cfg.host, cfg.port)),
    )
}

/// The per-server folder's name: the host and port, with anything a file name cannot hold made
/// an underscore.
#[must_use]
pub fn folder_name(host: &str, port: u32) -> String {
    let host: String = host
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("{host}-{port}")
}

/// The name of a world's folder in its server's folder: the world's name when a file name can hold
/// it as it is, else the name with anything a file name cannot hold made an underscore and the
/// first eight hex digits of the name's SHA-256 after it, so two names never share a folder.
#[must_use]
pub fn world_folder_name(world_key: &str) -> String {
    let kept: String = world_key
        .chars()
        .take(48)
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    // The names Windows keeps for its devices are never a folder's.
    let device = {
        let upper = kept.to_ascii_uppercase();
        ["CON", "PRN", "AUX", "NUL"].contains(&upper.as_str())
            || ((upper.starts_with("COM") || upper.starts_with("LPT"))
                && upper.len() == 4
                && upper.as_bytes()[3].is_ascii_digit())
    };
    if kept == world_key && !kept.is_empty() && !device {
        return kept;
    }
    let hash = dereth_dat::overlay::world_hash(world_key);
    format!("{kept}-{}", dereth_dat::overlay::hex(&hash[..4]))
}

/// The world's folder in the server's folder `root`.
#[must_use]
pub fn world_folder(root: &Path, world_key: &str) -> PathBuf {
    root.join(world_folder_name(world_key))
}

/// The world the server whose folder is `root` served last, as [`LAST_WORLD_FILE`] names it.
#[must_use]
pub fn last_world(root: &Path) -> Option<String> {
    let text = std::fs::read_to_string(root.join(LAST_WORLD_FILE)).ok()?;
    let name = text.lines().next()?.trim_end_matches('\r');
    (!name.is_empty()).then(|| name.to_owned())
}

/// Note `world_key` as the world the server whose folder is `root` served last.
///
/// # Errors
/// The folder cannot be made or the file written.
pub fn remember_world(root: &Path, world_key: &str) -> std::io::Result<()> {
    if last_world(root).as_deref() == Some(world_key) {
        return Ok(());
    }
    std::fs::create_dir_all(root)?;
    std::fs::write(root.join(LAST_WORLD_FILE), format!("{world_key}\n"))
}

/// Move the overlay a server's folder `root` holds itself (as an earlier client kept one world's
/// overlay a server) into the folder of the world it names, and note that world as the one the
/// server served last when nothing is noted yet. A container the world's folder already holds is
/// left where it is. Answers the world moved, if any.
pub fn adopt_single_world(root: &Path) -> Option<String> {
    let dir = OverlayDir::new(root).ok()?;
    if dir.containers().is_empty() {
        return None;
    }
    let world = dir.world_key()?;
    let to = world_folder(root, &world);
    if let Err(e) = std::fs::create_dir_all(&to) {
        tracing::warn!("{} could not be made: {e}", to.display());
        return None;
    }
    for from in dir.containers() {
        let Some(name) = from.file_name() else {
            continue;
        };
        let dest = to.join(name);
        if dest.exists() {
            tracing::warn!(
                "{} is left where it is: {} already holds one",
                from.display(),
                to.display()
            );
            continue;
        }
        match std::fs::rename(&from, &dest) {
            Ok(()) => tracing::info!(
                "the world's overlay {} is now {}",
                from.display(),
                dest.display()
            ),
            Err(e) => tracing::warn!("{} could not be moved: {e}", from.display()),
        }
    }
    if last_world(root).is_none() {
        if let Err(e) = remember_world(root, &world) {
            tracing::warn!("{} could not be noted: {e}", root.display());
        }
    }
    Some(world)
}

/// The name the client knows the world by before the world names itself: its server's address.
#[must_use]
pub fn address_key(cfg: &Config) -> String {
    format!("{}:{}", cfg.host, cfg.port)
}

/// Where the blocklist is.
#[must_use]
pub fn blocklist_file(cfg: &Config) -> PathBuf {
    blocklist_beside(&cfg.preferences_file)
}

/// The blocklist beside the preferences file `preferences`.
#[must_use]
pub fn blocklist_beside(preferences: &Path) -> PathBuf {
    preferences
        .parent()
        .map_or_else(|| PathBuf::from(BLOCKLIST_FILE), |p| p.join(BLOCKLIST_FILE))
}

/// The worlds whose overlays the player refuses: the blocklist's lines, trimmed, blank lines and
/// `#` comments left out. An absent file blocks nothing.
#[must_use]
pub fn blocklist(cfg: &Config) -> BTreeSet<String> {
    parse_blocklist(&host_files::read_to_string(&blocklist_file(cfg)).unwrap_or_default())
}

/// The blocklist's text, read.
#[must_use]
pub fn parse_blocklist(text: &str) -> BTreeSet<String> {
    dereth_client_contract::persist::overlay_blocklist::parse(text)
}

/// The overlay folder, checked to be one: `None`, with the reason logged, when the path names a
/// folder holding base data files.
#[must_use]
pub fn folder(cfg: &Config) -> Option<OverlayDir> {
    let dir = overlay_dir(cfg)?;
    match open_folder(&dir) {
        Ok(d) => Some(d),
        Err(e) => {
            tracing::warn!("no world overlay is kept: {e}");
            None
        }
    }
}

/// `store` with the world's overlay over it when the folder holds one the player does not
/// refuse. A refused overlay is logged and left out.
#[must_use]
pub fn lay_over(store: RetailDatStore, cfg: &Config) -> RetailDatStore {
    lay_over_noting(store, cfg).0
}

/// [`lay_over`], with the reason when the folder's overlay is refused.
#[must_use]
pub fn lay_over_noting(store: RetailDatStore, cfg: &Config) -> (RetailDatStore, Option<Refusal>) {
    lay_folder(store, folder(cfg).as_ref(), &blocklist(cfg))
}

/// `store` with the overlay in `dir` over it, when there is one and the world it belongs to is
/// not in `blocked`; with the reason when it is refused. `store` carries no overlay of its own.
#[must_use]
pub fn lay_folder(
    store: RetailDatStore,
    dir: Option<&OverlayDir>,
    blocked: &BTreeSet<String>,
) -> (RetailDatStore, Option<Refusal>) {
    let Some(dir) = dir else {
        return (store, None);
    };
    if dir.containers().is_empty() {
        return (store, None);
    }
    let key = dir.world_key();
    let Some(k) = key else {
        tracing::warn!(
            "the overlay in {} names no world it belongs to; it is not read",
            dir.path().display()
        );
        return (store, Some(Refusal::Unreadable));
    };
    if blocked.contains(&k) {
        tracing::warn!(
            "the overlay in {} belongs to the world {k:?}, which is on the overlay blocklist; \
             it is not read",
            dir.path().display()
        );
        return (store, Some(Refusal::Blocked { world: k }));
    }
    match store.clone().with_overlay(dir, Some(&k)) {
        Ok(with) => {
            tracing::info!(
                "the world's overlay for {k:?} is read from {}",
                dir.path().display()
            );
            (with, None)
        }
        Err(e) => {
            tracing::warn!("the world's overlay is refused and not read: {e}");
            (store, Some(Refusal::from_error(&e)))
        }
    }
}

/// The world whose overlay `store`'s files read through, which the data patch's answer names.
#[must_use]
pub fn laid_world(store: &RetailDatStore) -> Option<String> {
    store.overlay_world()
}

/// The world whose overlay a run lays at the start, when its folder holds one: for a launcher to
/// tell, before it starts the client, that the client would refuse the world.
#[must_use]
pub fn start_world(cfg: &Config) -> Option<String> {
    folder(cfg)?.world_key()
}

/// Where this run's patches go: the overlay folder over the files `store` reads, with the
/// player's blocklist; for a run keeping a folder for each world at its server, that server's
/// folder too. `None` for a run with no overlay folder, whose patches are all refused.
#[must_use]
pub fn patcher(store: &RetailDatStore, cfg: &Config) -> crate::ddd::DddPatcher {
    let root = address_folder(cfg);
    let address = address_key(cfg);
    let target = folder(cfg).map(|dir| {
        let t = crate::ddd::OverlayTarget::new(dir, store, &address);
        match &root {
            Some(root) => {
                let world = last_world(root).unwrap_or_else(|| address.clone());
                t.for_each_world_in(root, &address, &world)
            }
            None => t,
        }
    });
    let mut p = crate::ddd::DddPatcher::new(target);
    p.set_blocklist(blocklist(cfg));
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Behaviour: none (tooling: where the overlay folder is and what the blocklist says)
    #[test]
    fn the_overlay_folder_is_the_switch_else_one_per_server_and_the_blocklist_reads_one_world_a_line(
    ) {
        assert_eq!(folder_name("127.0.0.1", 19661), "127.0.0.1-19661");
        assert_eq!(folder_name("a:b/c", 9000), "a_b_c-9000");
        let mut cfg = Config {
            connect: false,
            ..Config::default()
        };
        assert_eq!(overlay_dir(&cfg), None, "an offline run keeps none");
        cfg.overlay_dat_dir = Some(PathBuf::from("mine"));
        assert_eq!(overlay_dir(&cfg), Some(PathBuf::from("mine")));
        cfg.connect = true;
        assert_eq!(
            address_folder(&cfg),
            None,
            "a folder named for the run is the run's alone"
        );
        let b = parse_blocklist("# blocked\n  bad world \n\nanother\n");
        assert_eq!(
            b.into_iter().collect::<Vec<_>>(),
            vec!["another".to_owned(), "bad world".to_owned()]
        );
    }

    /// Behaviour: none (tooling: the name of a world's folder)
    #[test]
    fn a_worlds_folder_is_its_name_or_a_name_a_folder_can_hold_with_the_names_hash() {
        assert_eq!(world_folder_name("my-world_2"), "my-world_2");
        let spaced = world_folder_name("my world");
        assert!(spaced.starts_with("my_world-") && spaced.len() == "my_world-".len() + 8);
        assert_ne!(
            world_folder_name("my world"),
            world_folder_name("my_world"),
            "two names, two folders"
        );
        for awkward in ["..", ".", "", "a/../b", "con", "LPT1", "127.0.0.1:9000"] {
            let name = world_folder_name(awkward);
            assert!(
                name.contains('-') && !name.contains(['.', '/', '\\', ':']),
                "{awkward:?} -> {name:?}"
            );
            assert_eq!(
                Path::new("root").join(&name).parent(),
                Some(Path::new("root"))
            );
        }
        let long = "x".repeat(200);
        assert!(world_folder_name(&long).len() < 64);
    }

    /// A store of three small files kept in memory, each with one record and the iteration list.
    fn memory_store() -> RetailDatStore {
        use dereth_dat::{write::DatWriter, DatFile, MemoryFile, ITERATION_LIST};
        let file = |name: &str, set: u32, subset: u32| {
            let bytes = MemoryFile::default();
            let mut w = DatWriter::create_in(
                PathBuf::from(name),
                Box::new(bytes.clone()),
                0x400,
                set,
                subset,
                0x400 * 17,
            )
            .unwrap();
            w.save(
                ITERATION_LIST,
                &dereth_dat::iteration::encode(&[1, 2]),
                1,
                0,
                1,
            )
            .unwrap();
            w.save(dereth_primitives::DataId(0x0600_0001), b"base", 1, 2, 1)
                .unwrap();
            drop(w);
            DatFile::from_storage(PathBuf::from(name), Box::new(bytes)).unwrap()
        };
        RetailDatStore::open_with(
            file("client_portal.dat", 1, 0),
            file("client_cell_1.dat", 2, 1),
            file("client_local_English.dat", 3, 1),
            None,
        )
    }

    /// Write one record of the world `world` into the portal overlay in `dir`.
    fn write_world(store: &RetailDatStore, dir: &Path, world: &str, bytes: &[u8]) {
        let dir = OverlayDir::new(dir).unwrap();
        let mut w = dir
            .writer(
                dereth_dat::ModernDat::Portal,
                store.portal(),
                "client_portal.dat",
                world,
                1,
            )
            .unwrap();
        w.save(
            store.portal(),
            dereth_primitives::DataId(0x0600_0001),
            bytes,
            1,
            3,
            1,
        )
        .unwrap();
        w.add_iteration(3, 1).unwrap();
        w.flush(1).unwrap();
    }

    /// Behaviour: none (tooling: an earlier client's folder for a server is kept as its world's)
    /// A server's folder that holds one world's overlay itself is moved into that world's folder
    /// the first time, the world noted as the one the server served last, and read from there:
    /// nothing of it is lost and nothing is fetched again.
    #[test]
    fn a_servers_folder_holding_one_worlds_overlay_becomes_that_worlds_folder() {
        let scratch = dereth_dat::testing::ScratchDir::new("overlay-adopt").expect("scratch");
        let root = scratch.path().join("127.0.0.1-9000");
        let store = memory_store();
        write_world(&store, &root, "Old World", b"old");
        assert_eq!(adopt_single_world(&root).as_deref(), Some("Old World"));
        assert!(
            OverlayDir::new(&root).unwrap().containers().is_empty(),
            "nothing is left in the server's folder itself"
        );
        assert_eq!(last_world(&root).as_deref(), Some("Old World"));
        let moved = OverlayDir::new(&world_folder(&root, "Old World")).unwrap();
        assert_eq!(moved.world_key().as_deref(), Some("Old World"));
        let (laid, refused) = lay_folder(store.clone(), Some(&moved), &BTreeSet::new());
        assert_eq!(refused, None);
        assert_eq!(
            laid.read_portal(dereth_primitives::DataId(0x0600_0001))
                .unwrap(),
            b"old"
        );
        assert_eq!(laid_world(&laid).as_deref(), Some("Old World"));
        assert_eq!(adopt_single_world(&root), None, "once");
        // Another world noted since is not displaced by the move.
        write_world(&store, &root, "Old World", b"again");
        remember_world(&root, "Newer").unwrap();
        adopt_single_world(&root);
        assert_eq!(last_world(&root).as_deref(), Some("Newer"));
    }

    thread_local! {
        static KEPT: dereth_dat::MemoryFolder =
            dereth_dat::MemoryFolder::new(Path::new("overlays/127.0.0.1-9000"));
    }

    fn kept(dir: &Path) -> Option<Arc<dyn DatFolder>> {
        KEPT.with(|f| (f.path() == dir).then(|| Arc::new(f.clone()) as Arc<dyn DatFolder>))
    }

    /// Behaviour: net.dat-patch.the-overlay-extension-is-offered-only-with-an-overlay-kept
    #[test]
    fn a_host_kept_overlay_folder_is_written_and_laid_and_only_a_run_keeping_one_offers_the_extension(
    ) {
        use dereth_protocol::admin::DddInterrogationResponse;
        let store = memory_store();
        let offline = Config {
            connect: false,
            ..Config::default()
        };
        let none = patcher(&store, &offline);
        assert!(!none.keeps_overlay());
        let r = crate::app::ddd_interrogation_response(&store, 0, none.keeps_overlay(), None);
        assert_eq!(r.flags, 0);
        assert!(r.overlay_bases.is_empty(), "no overlay, no bases");

        install_folders(kept);
        let cfg = Config {
            connect: false,
            overlay_dat_dir: Some(PathBuf::from("overlays/127.0.0.1-9000")),
            ..Config::default()
        };
        let p = patcher(&store, &cfg);
        assert!(p.keeps_overlay());
        let r = crate::app::ddd_interrogation_response(&store, 0, p.keeps_overlay(), None);
        assert_ne!(r.flags & DddInterrogationResponse::FLAG_OVERLAY, 0);
        assert_ne!(r.flags & DddInterrogationResponse::FLAG_RECORDS, 0);
        assert_eq!(r.flags & DddInterrogationResponse::FLAG_WORLD, 0);
        assert_eq!(r.overlay_bases.len(), 3);
        // A folder the host does not keep is no folder.
        let elsewhere = Config {
            overlay_dat_dir: Some(PathBuf::from("overlays/other-9000")),
            ..cfg.clone()
        };
        assert!(!patcher(&store, &elsewhere).keeps_overlay());

        // What is written into the host's folder is laid over the store at the next open.
        let dir = folder(&cfg).expect("the host keeps it");
        let mut w = dir
            .writer(
                dereth_dat::ModernDat::Portal,
                store.portal(),
                "client_portal.dat",
                "a world",
                1,
            )
            .unwrap();
        w.save(
            store.portal(),
            dereth_primitives::DataId(0x0600_0001),
            b"patched",
            1,
            3,
            1,
        )
        .unwrap();
        w.add_iteration(3, 1).unwrap();
        w.flush(1).unwrap();
        drop(w);
        assert_eq!(
            KEPT.with(dereth_dat::MemoryFolder::names),
            ["overlay_portal.dat"]
        );
        let laid = lay_over(store.clone(), &cfg);
        assert!(laid.has_overlay());
        assert_eq!(
            laid.read_portal(dereth_primitives::DataId(0x0600_0001))
                .unwrap(),
            b"patched"
        );
        assert_eq!(
            store
                .read_portal(dereth_primitives::DataId(0x0600_0001))
                .unwrap(),
            b"base",
            "the base is never written"
        );
        // The answer names the world whose lists it carries, and the store without the overlay is
        // the files as installed.
        let world = laid_world(&laid);
        assert_eq!(world.as_deref(), Some("a world"));
        let r = crate::app::ddd_interrogation_response(&laid, 0, true, world.as_deref());
        assert_eq!(r.world(), Some(dereth_dat::overlay::world_hash("a world")));
        let bare = laid.without_overlay();
        assert!(!bare.has_overlay());
        assert_eq!(
            bare.read_portal(dereth_primitives::DataId(0x0600_0001))
                .unwrap(),
            b"base"
        );
    }
}
