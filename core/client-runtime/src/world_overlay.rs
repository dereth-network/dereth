//! The world's overlay folder: where a world's own records are kept, which worlds' overlays the
//! player refuses, and laying the overlay over the locked files.
//!
//! **A deliberate divergence from retail** (client divergence CD-031): retail patches its own
//! data files; this client keeps everything a world changes in an overlay folder of that world's
//! ([`dereth_dat::overlay`]) and never writes the files a player installed.
//!
//! - **Where.** `--overlay-dat-dir <dir>` names the folder for the run. Without it, a client that
//!   connects keeps one folder per server in the per-user cache, `overlays/<host>-<port>`. Naming
//!   a folder changes only where the overlay is; what is read and refused is the same.
//! - **Which world.** A folder holds one world's overlay. Its containers name the world, and a
//!   world announcing another name is refused (the patch writes nothing) and reported.
//! - **The blocklist.** `overlay-blocklist.txt` beside the preferences file, one world name a
//!   line: those worlds' overlays are never opened or written, wherever their folder is.
//! - **Refusal is not failure.** An overlay that is refused (another base, another world, a
//!   blocked world) is reported and left out, and the world is read from the locked files alone.
//! - **Where it is kept.** On disk, unless the host keeps its folders elsewhere
//!   ([`install_folders`]): a browser keeps them in its own storage, and the overlay is the same.

use std::cell::Cell;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use dereth_dat::overlay::{OverlayDir, OverlayError};
use dereth_dat::{DatFolder, RetailDatStore};

use crate::config::Config;
use crate::platform::files as host_files;

/// The blocklist's file name, beside the preferences file.
pub const BLOCKLIST_FILE: &str = "overlay-blocklist.txt";

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

/// The folder this run keeps the world's overlay in: `--overlay-dat-dir`, else the per-user
/// cache's folder for the server when the client connects; `None` for a run with neither.
#[must_use]
pub fn overlay_dir(cfg: &Config) -> Option<PathBuf> {
    if let Some(d) = &cfg.overlay_dat_dir {
        return Some(d.clone());
    }
    if !cfg.connect {
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

/// The name the client knows the world by before the world names itself: its server's address.
#[must_use]
pub fn address_key(cfg: &Config) -> String {
    format!("{}:{}", cfg.host, cfg.port)
}

/// Where the blocklist is.
#[must_use]
pub fn blocklist_file(cfg: &Config) -> PathBuf {
    cfg.preferences_file
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
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_owned)
        .collect()
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
    let Some(dir) = folder(cfg) else {
        return store;
    };
    if dir.containers().is_empty() {
        return store;
    }
    let key = dir.world_key();
    if let Some(k) = &key {
        if blocklist(cfg).contains(k) {
            tracing::warn!(
                "the overlay in {} belongs to the world {k:?}, which is on the overlay blocklist; \
                 it is not read",
                dir.path().display()
            );
            return store;
        }
    }
    match store.clone().with_overlay(&dir, key.as_deref()) {
        Ok(with) => {
            tracing::info!(
                "the world's overlay{} is read from {}",
                key.map(|k| format!(" for {k:?}")).unwrap_or_default(),
                dir.path().display()
            );
            with
        }
        Err(e) => {
            tracing::warn!("the world's overlay is refused and not read: {e}");
            store
        }
    }
}

/// Where this run's patches go: the overlay folder over the files `store` reads, with the
/// player's blocklist. `None` for a run with no overlay folder, whose patches are all refused.
#[must_use]
pub fn patcher(store: &RetailDatStore, cfg: &Config) -> crate::ddd::DddPatcher {
    let target =
        folder(cfg).map(|dir| crate::ddd::OverlayTarget::new(dir, store, &address_key(cfg)));
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
        let b = parse_blocklist("# blocked\n  bad world \n\nanother\n");
        assert_eq!(
            b.into_iter().collect::<Vec<_>>(),
            vec!["another".to_owned(), "bad world".to_owned()]
        );
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
        let r = crate::app::ddd_interrogation_response(&store, 0, none.keeps_overlay());
        assert_eq!(r.flags & DddInterrogationResponse::FLAG_OVERLAY, 0);
        assert!(r.overlay_bases.is_empty(), "no overlay, no bases");

        install_folders(kept);
        let cfg = Config {
            connect: false,
            overlay_dat_dir: Some(PathBuf::from("overlays/127.0.0.1-9000")),
            ..Config::default()
        };
        let p = patcher(&store, &cfg);
        assert!(p.keeps_overlay());
        let r = crate::app::ddd_interrogation_response(&store, 0, p.keeps_overlay());
        assert_ne!(r.flags & DddInterrogationResponse::FLAG_OVERLAY, 0);
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
    }
}
