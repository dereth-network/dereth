//! The data sets the Library can download for the player from the community's archive
//! (`dereth-datfetch`), as every front end offers them: each set on offer, by the game and kind
//! of set it is, and the last check of one got whole, that its files report the iterations the
//! launcher matches the set by.
//!
//! The fetching itself is `dereth-datfetch`'s, with the front end doing the requests, reads and
//! writes: the desktop launcher in a thread of its own, the web page in its worker.

use serde::Serialize;

use dereth_datfetch::sources::{SetKind as SourceKind, SetSource};
use dereth_datfetch::Catalogue;

use crate::datset::{DatFileState, DatRole, SetKind};
use crate::world::Game;

/// The file that marks a set's folder as not yet whole: a folder holding it is never read as a set.
pub const INCOMPLETE: &str = ".incomplete";

/// One set on offer, as a Library lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Offer {
    pub id: String,
    /// What the player is shown: "End of Retail", "Infiltration", "Launch".
    pub label: String,
    pub game: Game,
    /// The kind of set it ends as.
    pub kind: SetKind,
    /// The bytes downloaded.
    pub download: u64,
    /// The bytes its files take.
    pub size: u64,
    /// The most storage getting it takes at once.
    pub peak: u64,
    /// The names its archives are kept under while they are got.
    pub archives: Vec<String>,
    /// The names of the files it ends with.
    pub files: Vec<String>,
}

/// The game and kind of set a set on offer ends as.
#[must_use]
pub fn kind_of(source: &SetSource) -> (Game, SetKind) {
    match source.kind {
        SourceKind::Modern => (Game::Dereth, SetKind::Modern),
        SourceKind::Classic => (Game::Dereth, SetKind::Classic),
        SourceKind::Auberean => (Game::Auberean, SetKind::Auberean),
    }
}

/// Every set `catalogue` offers, in its order.
#[must_use]
pub fn offers(catalogue: &Catalogue) -> Vec<Offer> {
    catalogue
        .sets
        .iter()
        .map(|s| {
            let (game, kind) = kind_of(s);
            Offer {
                id: s.id.clone(),
                label: s.label.clone(),
                game,
                kind,
                download: s.download_bytes(),
                size: s.set_bytes(),
                peak: s.peak_bytes(),
                archives: s.archive_names(),
                files: s.file_names(),
            }
        })
        .collect()
}

/// The last check of a set got whole, its files already checked by their hashes: read as the
/// launcher reads a set (`files`), every file it lists is there, readable, and reports the
/// iteration the set is listed with.
///
/// # Errors
/// What is wrong, in words for the log and the player.
pub fn check(source: &SetSource, files: &[DatFileState]) -> Result<(), String> {
    let want = |role: DatRole| match role {
        DatRole::Portal => source.iterations.portal,
        DatRole::Cell => source.iterations.cell,
        DatRole::Local => source.iterations.local,
        DatRole::Highres => source.iterations.highres,
    };
    for role in DatRole::ALL {
        let Some(w) = want(role) else {
            continue;
        };
        let Some(f) = files.iter().find(|f| f.role == role) else {
            return Err(format!("the set has no {} file", role.label()));
        };
        if let Some(e) = &f.error {
            return Err(format!("{}: {e}", f.file_name));
        }
        if f.iterations != Some(w) {
            return Err(format!(
                "{} reports iteration {}, not {w}",
                f.file_name,
                f.iterations
                    .map_or_else(|| "none".to_owned(), |n| n.to_string())
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the launcher's offer of the community archive's data sets: tooling around
    //! the client).
    use super::*;
    use crate::datset::{scan_classic_dir, scan_dir};

    #[test]
    fn the_offers_are_both_games_sets_by_the_kind_each_ends_as() {
        let o = offers(&Catalogue::built_in());
        let by = |id: &str| o.iter().find(|x| x.id == id).unwrap();
        assert_eq!(
            (by("end-of-retail").game, by("end-of-retail").kind),
            (Game::Dereth, SetKind::Modern)
        );
        assert_eq!(by("infiltration").kind, SetKind::Classic);
        assert_eq!(by("launch").kind, SetKind::Classic);
        assert_eq!(by("launch").label, "Launch");
        let ac2 = by("auberean-end-of-retail");
        assert_eq!((ac2.game, ac2.kind), (Game::Auberean, SetKind::Auberean));
        assert_eq!(ac2.label, "End of Retail");
        assert!(o.iter().all(|x| x.peak >= x.size && x.download > 0));
    }

    /// A Launch pair got whole passes when its files report the Launch iterations, and fails when
    /// a file is missing or reports another.
    #[test]
    fn a_set_got_whole_must_report_the_iterations_it_is_listed_with() {
        let c = Catalogue::built_in();
        let launch = c.set("launch").unwrap();
        let d = crate::datset::tests::tmp("downloads-check");
        crate::testing::write_classic_set(&d, 102, 67);
        assert_eq!(check(launch, &scan_classic_dir(&d)), Ok(()));
        crate::testing::write_classic_set(&d, 2112, 67);
        let why = check(launch, &scan_classic_dir(&d)).unwrap_err();
        assert!(why.contains("not 102"), "{why}");
        std::fs::remove_file(d.join("cell.dat")).unwrap();
        crate::testing::write_classic_set(&d, 102, 67);
        std::fs::remove_file(d.join("cell.dat")).unwrap();
        let why = check(launch, &scan_classic_dir(&d)).unwrap_err();
        assert!(why.contains("no cell file"), "{why}");
        let eor = c.set("end-of-retail").unwrap();
        crate::datset::tests::fake_set(&d, crate::Iterations::END_OF_RETAIL);
        assert_eq!(check(eor, &scan_dir(&d)), Ok(()));
        let _ = std::fs::remove_dir_all(&d);
    }
}
