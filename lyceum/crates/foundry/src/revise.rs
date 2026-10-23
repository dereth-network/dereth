//! A world published again: its new build's overlay numbered as the revision after the overlay it
//! was last published with ([`dereth_dat::revise`]), so a server patches a client holding the last
//! one with what changed alone, and a client with none with every revision.

use std::path::Path;

use dereth_dat::overlay::{OverlayDir, OverlayError};
use dereth_dat::revise::{self, ReviseError, Revised};
use dereth_dat::ModernDat;
use dereth_primitives::EraId;

use crate::dats::DatSet;
use crate::error::Error;

/// What [`after`] did.
#[derive(Clone, Debug, Default)]
pub struct Followed {
    /// Each file the world changes or changed, with what its overlay now holds.
    pub files: Vec<(ModernDat, Revised)>,
    /// Why the last overlay was not followed, when it was not: it is another world's, or was made
    /// over other files. The world's revisions then start again.
    pub restarted: Option<String>,
}

fn dat(e: ReviseError) -> Error {
    Error::Dat(e.to_string())
}

/// Make the overlay folder `overlay`, a build of the world `key` over the files of `era` in
/// `dats`, the revision after `last`, the overlay folder the world was last published with. The
/// folder is replaced only once the whole revision is written.
///
/// # Errors
/// Files or overlays that will not open, an overlay carrying its world's own iterations, and the
/// writer's errors.
pub fn after(
    dats: &Path,
    era: EraId,
    last: &Path,
    overlay: &Path,
    key: &str,
) -> Result<Followed, Error> {
    let set = DatSet::for_world(dats, era)?;
    let bases: Vec<(ModernDat, dereth_dat::DatFile)> =
        set.files().map(|(t, f)| (t, f.clone())).collect();
    let fresh = OverlayDir::new(overlay)?;
    let previous = OverlayDir::new(last)?;
    let staging = overlay.with_file_name(".revising");
    let write = |last: Option<&OverlayDir>| -> Result<Vec<(ModernDat, Revised)>, ReviseError> {
        if staging.exists() {
            std::fs::remove_dir_all(&staging).map_err(dereth_dat::DatError::from)?;
        }
        std::fs::create_dir_all(&staging).map_err(dereth_dat::DatError::from)?;
        let out = OverlayDir::new(&staging)?;
        revise::revise_folder(&bases, last, &fresh, &out, key)
    };
    let mut done = Followed::default();
    let result = match write(Some(&previous)) {
        Err(ReviseError::Overlay(
            e @ (OverlayError::OtherWorld { .. } | OverlayError::BaseMismatch { .. }),
        )) => {
            done.restarted = Some(e.to_string());
            write(None)
        }
        other => other,
    };
    match result {
        Ok(files) => done.files = files,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(dat(e));
        }
    }
    std::fs::remove_dir_all(overlay).map_err(|e| Error::io(overlay, e))?;
    std::fs::rename(&staging, overlay).map_err(|e| Error::io(overlay, e))?;
    Ok(done)
}
