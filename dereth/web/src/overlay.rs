//! A world's overlay, kept in the browser's own storage: one folder per world, which the worker
//! opens before the client starts and reads and writes synchronously for it.
//!
//! The worker holds the folder's four containers open (the origin-private file system's
//! synchronous handles, which only a worker has) and answers the calls below; a container of no
//! length is no container. The client's overlay writer and reader go through
//! [`BrowserFolder`] exactly as they go through a folder on disk
//! (`dereth_dat::folder::DatFolder`), so a patch is written, laid and reread as on the desktop.

use std::cell::RefCell;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use dereth_dat::overlay::container_name;
use dereth_dat::{DatError, DatFolder, DatStorage, DatStorageMut, ModernDat};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    /// The length of the open folder's container `name`; 0 for none.
    #[wasm_bindgen(js_namespace = globalThis, js_name = derethOverlaySize)]
    fn overlay_size(name: &str) -> f64;

    /// Fill `buf` from `offset` of the container `name`; `false` when it is too short.
    #[wasm_bindgen(js_namespace = globalThis, js_name = derethOverlayRead)]
    fn overlay_read(name: &str, offset: f64, buf: &mut [u8]) -> bool;

    /// Write `bytes` at `offset` of the container `name`; `false` when the storage refuses (it is
    /// full).
    #[wasm_bindgen(js_namespace = globalThis, js_name = derethOverlayWrite)]
    fn overlay_write(name: &str, offset: f64, bytes: &[u8]) -> bool;

    /// Empty the container `name`; `false` when the storage refuses.
    #[wasm_bindgen(js_namespace = globalThis, js_name = derethOverlayTruncate)]
    fn overlay_truncate(name: &str) -> bool;
}

thread_local! {
    /// The folder the worker holds open, by its name; `None` while it holds none.
    static OPEN: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

/// The worker now holds the folder `folder` open (`overlays/<host>-<port>`), or none for an empty
/// name. The client keeps the world's overlay there.
pub fn hold(folder: &str) {
    OPEN.with(|o| *o.borrow_mut() = (!folder.is_empty()).then(|| PathBuf::from(folder)));
    dereth_client_runtime::world_overlay::install_folders(opener);
}

/// The folder the worker holds open, when it holds one.
#[must_use]
pub fn held() -> Option<PathBuf> {
    OPEN.with(|o| o.borrow().clone())
}

/// The overlay folder the client asks for: the one the worker holds, and no other.
fn opener(dir: &Path) -> Option<Arc<dyn DatFolder>> {
    let held = held()?;
    (held == dir).then(|| Arc::new(BrowserFolder { dir: held }) as Arc<dyn DatFolder>)
}

/// The folder the worker holds open.
#[derive(Debug, Clone)]
pub struct BrowserFolder {
    dir: PathBuf,
}

impl BrowserFolder {
    /// The folder the worker holds open, when it holds one.
    #[must_use]
    pub fn held() -> Option<Self> {
        held().map(|dir| Self { dir })
    }
}

/// The container a name names: one of the four, by their own names.
fn container(name: &str) -> io::Result<&'static str> {
    ModernDat::ALL
        .iter()
        .map(|t| container_name(*t))
        .find(|n| *n == name)
        .ok_or_else(|| io::ErrorKind::NotFound.into())
}

/// A JavaScript number holds every offset in a container exactly.
#[allow(clippy::cast_precision_loss)]
fn at(offset: u64) -> f64 {
    offset as f64
}

impl DatFolder for BrowserFolder {
    fn path(&self) -> &Path {
        &self.dir
    }

    fn is_file(&self, name: &str) -> bool {
        container(name).is_ok_and(|n| overlay_size(n) > 0.0)
    }

    fn open(&self, name: &str) -> io::Result<Box<dyn DatStorage>> {
        let name = container(name)?;
        if overlay_size(name) <= 0.0 {
            return Err(io::ErrorKind::NotFound.into());
        }
        Ok(Box::new(BrowserContainer(name)))
    }

    fn open_mut(&self, name: &str) -> Result<Box<dyn DatStorageMut>, DatError> {
        let name = container(name)?;
        if overlay_size(name) <= 0.0 {
            return Err(io::Error::from(io::ErrorKind::NotFound).into());
        }
        Ok(Box::new(BrowserContainer(name)))
    }

    fn create(&self, name: &str) -> Result<Box<dyn DatStorageMut>, DatError> {
        let name = container(name)?;
        if !overlay_truncate(name) {
            return Err(refused().into());
        }
        Ok(Box::new(BrowserContainer(name)))
    }

    fn remove(&self, name: &str) -> io::Result<()> {
        let name = container(name)?;
        if overlay_size(name) <= 0.0 {
            return Err(io::ErrorKind::NotFound.into());
        }
        if overlay_truncate(name) {
            Ok(())
        } else {
            Err(refused())
        }
    }
}

/// What a refused write is: the browser's storage for this site is full, or will not be written.
fn refused() -> io::Error {
    io::Error::new(
        io::ErrorKind::StorageFull,
        "the browser refused to store more for this site (its storage is full)",
    )
}

/// One container of the folder the worker holds, by its name. It holds nothing but the name, so
/// it is `Send` and `Sync` as the store requires; the reads and writes are calls into the worker.
#[derive(Debug, Clone, Copy)]
struct BrowserContainer(&'static str);

impl DatStorage for BrowserContainer {
    fn read_exact_at(&self, offset: u64, buf: &mut [u8]) -> io::Result<()> {
        if overlay_read(self.0, at(offset), buf) {
            Ok(())
        } else {
            Err(io::ErrorKind::UnexpectedEof.into())
        }
    }
}

impl DatStorageMut for BrowserContainer {
    fn read_exact_at(&mut self, offset: u64, buf: &mut [u8]) -> io::Result<()> {
        DatStorage::read_exact_at(self, offset, buf)
    }

    fn write_all_at(&mut self, offset: u64, buf: &[u8]) -> io::Result<()> {
        if overlay_write(self.0, at(offset), buf) {
            Ok(())
        } else {
            Err(refused())
        }
    }
}
