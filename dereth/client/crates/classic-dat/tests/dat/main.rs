//! Behaviour: none (file-format readers and conversions, over the player's own portal)
//!
//! The pre-Throne-of-Destiny portal named by `DERETH_CLASSIC_PORTAL`, read whole. With
//! `DERETH_CLASSIC_REFERENCE` (a directory of independently exported art, creation tables and
//! face textures from the same portal) and `DERETH_CLASSIC_REFERENCE_PREVIEW` (a directory holding
//! an independently converted `client_portal.dat` and its `manifest.json`), each result is also
//! compared with those exports byte for byte.
//!
//! The tests are ignored without the `retail-dats` feature. With it, a test whose portal is not
//! named prints `NOT RUN` and passes, and a comparison whose reference is not named prints
//! `NOT RUN` for that comparison and still checks everything the portal alone can show.

use std::path::PathBuf;
use std::sync::OnceLock;

use dereth_classic_dat::ClassicPortal;

mod art;
mod creation_tables;

/// The portal, opened once per binary, or `None` (with a `NOT RUN` line) when it is not named.
fn portal(test: &str) -> Option<&'static ClassicPortal> {
    static PORTAL: OnceLock<Option<ClassicPortal>> = OnceLock::new();
    let portal = PORTAL.get_or_init(|| {
        let path = std::env::var_os("DERETH_CLASSIC_PORTAL")?;
        Some(ClassicPortal::open(&PathBuf::from(path)).expect("DERETH_CLASSIC_PORTAL opens"))
    });
    if portal.is_none() {
        eprintln!("NOT RUN: {test}: DERETH_CLASSIC_PORTAL names no portal.dat");
    }
    portal.as_ref()
}

/// A reference directory from the environment, or `None` (with a `NOT RUN` line).
fn reference(var: &str, what: &str) -> Option<PathBuf> {
    let dir = std::env::var_os(var).map(PathBuf::from);
    if dir.is_none() {
        eprintln!("NOT RUN: {what}: {var} is not set");
    }
    dir
}

fn read_json(path: &std::path::Path) -> serde_json::Value {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_slice(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}
