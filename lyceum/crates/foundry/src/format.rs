//! Source format versions and explicit upgrades that preserve content and assigned ids: the
//! shared pack format's (`dereth_pack::format`), over a Lyceum world and its packs.

use std::path::{Path, PathBuf};

use crate::dialect::Ac1;
use crate::Error;

pub use dereth_pack::format::CURRENT;

/// Checks the format before interpreting the rest of a source file.
///
/// # Errors
/// Invalid TOML, a malformed version, or a format newer than this program supports.
pub fn check(path: &Path, text: &str) -> Result<u32, Error> {
    dereth_pack::format::check::<Ac1>(path, text)
}

/// Upgrades a world's source headers, keeping backups and leaving world.lock untouched.
/// Format zero and one have identical content semantics; only the version declaration changes.
///
/// # Errors
/// Invalid sources, an existing conflicting backup, or a file that cannot be written.
pub fn upgrade(path: &Path, check_only: bool) -> Result<Vec<PathBuf>, Error> {
    let world = crate::world::World::read(path)?;
    let mut paths = vec![path.to_path_buf()];
    for pack in &world.packs {
        crate::source::read_pack(pack)?;
        paths.push(pack.join("pack.toml"));
    }
    dereth_pack::format::upgrade_files::<Ac1>(
        &dereth_pack::store::DiskStore,
        &paths,
        &world.lock_path(),
        check_only,
    )
}
