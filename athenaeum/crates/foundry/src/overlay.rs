//! A world's overlay: the records of the client's data files a world changes, built from its rows
//! into a folder beside its tables, as Yalain's compile builds it ([`yalain_world_db::overlay`]).
//! The install is never written.

use std::path::{Path, PathBuf};

pub use yalain_world_db::overlay::{
    cleared_blocks, cleared_rows, outline, shop_copies, write_records, write_rows, Built, Records,
    FOLDER, SPREAD,
};
use yalain_world_db::rows;

use crate::build;
use crate::world::Opened;

/// Write the world's overlay into `out`'s [`FOLDER`]: the revision after what the folder holds,
/// when it reads otherwise. Answers the folder and what it holds; a world that changes no record
/// and never did makes no container.
///
/// # Errors
/// As [`yalain_world_db::overlay::write`], and the world not checking.
pub fn write(world: &Opened, dats: &Path, out: &Path) -> Result<(PathBuf, Built), String> {
    let merged = build::merged(world)?;
    let key = rows::read(&world.files)
        .map_err(|p| yalain_world_db::report(&p))?
        .world
        .key;
    let folder = out.join(FOLDER);
    let built = yalain_world_db::overlay::write(&merged, &key, dats, &folder)?;
    Ok((folder, built))
}
